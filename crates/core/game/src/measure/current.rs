//! Current measurement declarations and ordinary session tasks.

use super::{MeasurementState, RefreshTask, RefreshTaskControl, local_trial_quota};
use crate::event_measure::{
    EventMeasureConfig, EventMeasureInterruption, EventMeasureTask, EventMeasureTaskControl, SharedEventMeasure,
};
use crate::runtime::{RuntimeError, RuntimeResult};
use rsvz_backend_api::{
    BattleStatusBackend, CobImpactDelayBackend, CommonZombieDanceBackend, PlantReadBackend, WaveHealthBackend,
    WaveTimingBackend, WorldResetBackend, ZombieRawFactsBackend, ZombieStateBackend,
};
use rsvz_current::CurrentBackend;
use rsvz_model::{
    MeasureLimit, MeasureMode, MeasureTrialOutcome, MeasurementSetup, ProtectTarget, RefreshDance, RefreshMeasureConfig,
};
use rsvz_schedule::state_hook::StateEvent;
use rsvz_schedule::tick::{TickControl, TickLane, TickLifetime, TickMeta, TickOptions};
use rsvz_schedule::timeline::IntoRelativeTime;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

fn with_measurement<T>(f: impl FnOnce(&mut MeasurementSetup) -> T) -> T {
    crate::setup::with_script_setup(|setup| f(&mut setup.measurement))
}

#[doc(hidden)]
pub fn __current_setup() -> MeasurementSetup {
    with_measurement(|measurement| measurement.clone())
}

/// Marks plants missing from the current selected card set as protected.
pub fn protect_unrepairable_from_cards() {
    with_measurement(|measurement| {
        measurement.protection_mut().enable_protect_unrepairable_from_cards();
    });
}

/// Adds targets to the current measurement protection set.
pub fn protect_add<I>(targets: I)
where
    I: IntoIterator<Item = ProtectTarget>,
{
    with_measurement(|measurement| {
        measurement.protection_mut().add(targets);
    });
}

/// Removes targets from the current measurement protection set.
pub fn protect_remove<I>(targets: I)
where
    I: IntoIterator<Item = ProtectTarget>,
{
    with_measurement(|measurement| {
        measurement.protection_mut().remove(targets);
    });
}

/// Replaces the current measurement protection set with these targets.
pub fn protect_only<I>(targets: I)
where
    I: IntoIterator<Item = ProtectTarget>,
{
    with_measurement(|measurement| {
        measurement.protection_mut().only(targets);
    });
}

/// Selects seml's activate-side accident-rate interpretation.
pub fn refresh_activate(activate: bool) {
    with_measurement(|measurement| {
        measurement.refresh_mut().set_assume_activate(activate);
    });
}

/// Selects seml/AvZ maid-cheat DanceCheat handling for refresh measurement.
///
/// This does not select `ZombieKind::Dancing` and does not model the Tree of
/// Wisdom normal-zombie dancing gait.
pub fn refresh_dance(dance: RefreshDance) {
    with_measurement(|measurement| {
        measurement.refresh_mut().set_dance(dance);
    });
}

/// Records seml's cobDelay flag for refresh measurement.
///
/// The requested rule is applied during Opening when Refresh measurement is enabled.
pub fn refresh_cob_delay(cob_delay: bool) {
    with_measurement(|measurement| {
        measurement.refresh_mut().set_cob_delay(cob_delay);
    });
}

/// Sets the number of completed endless rounds reconstructed for each trial.
pub fn completed_rounds(rounds: u32) {
    with_measurement(|measurement| measurement.set_completed_rounds(rounds));
}

/// Enables or disables DamageNarrow's cumulative Imp-chewing failure rule.
pub fn imp_leak_detection(enabled: bool) {
    with_measurement(|measurement| measurement.imp_leak_mut().set_enabled(enabled));
}

/// Sets the cumulative effective chewing threshold for one Imp.
pub fn imp_leak_threshold(threshold_cs: u32) {
    with_measurement(|measurement| measurement.imp_leak_mut().set_threshold_cs(threshold_cs));
}

/// Ends each measurement trial at the first dispatch reaching this wave-relative time.
pub fn end_at(time: impl IntoRelativeTime) {
    let result = crate::setup::with_script_setup(|setup| setup.measurement.set_window_end(time.into_relative_time()));
    if let Err(error) = result {
        crate::registration::record_error(RuntimeError::new(error.to_string()));
    }
}

static REFRESH_JOB_ID: u8 = 1;
static DAMAGE_NARROW_JOB_ID: u8 = 1;
static BROAD_PASS_JOB_ID: u8 = 1;
static SMASH_JOB_ID: u8 = 1;
static POGO_JOB_ID: u8 = 1;

fn event_measure_job_key(mode: MeasureMode) -> crate::session::SessionJobKey {
    let id = match mode {
        MeasureMode::Refresh => &REFRESH_JOB_ID,
        MeasureMode::DamageNarrow => &DAMAGE_NARROW_JOB_ID,
        MeasureMode::BroadPass => &BROAD_PASS_JOB_ID,
        MeasureMode::Smash => &SMASH_JOB_ID,
        MeasureMode::Pogo => &POGO_JOB_ID,
    };
    crate::session::SessionJobKey::new(id)
}

struct SilentReporter(Option<crate::diagnostics::LoggerHandle>);

impl SilentReporter {
    fn install() -> Self {
        Self(Some(crate::diagnostics::replace_logger(
            |_record: &crate::diagnostics::LogRecord<'_>| {},
        )))
    }

    fn restore(&mut self) {
        if let Some(reporter) = self.0.take() {
            crate::diagnostics::restore_logger(reporter);
        }
    }
}

impl Drop for SilentReporter {
    fn drop(&mut self) {
        self.restore();
    }
}

// These are the actual measurement callback boundaries. A required read may
// abort locally, but measurement sampling already promises to fail the Session.
fn run_measure_callback<T>(body: impl FnOnce() -> RuntimeResult<T>) -> RuntimeResult<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)).unwrap_or_else(|payload| {
        match rsvz_schedule::callback::into_error(payload) {
            Ok(error) => Err(error),
            Err(payload) => std::panic::resume_unwind(payload),
        }
    })
}

fn fail_measure_session(error: impl std::fmt::Display) -> TickControl {
    crate::session::fail_script(RuntimeError::new(error.to_string()));
    TickControl::Stop
}

fn install_event_measure<S>(mode: MeasureMode, limit: MeasureLimit, sample: S)
where
    CurrentBackend: PlantReadBackend + WorldResetBackend,
    S: Fn(&Rc<RefCell<EventMeasureTask>>, TickMeta) -> RuntimeResult<()> + 'static,
{
    match crate::session::claim_session_job(event_measure_job_key(mode)) {
        Ok(true) => {}
        Ok(false) => return,
        Err(error) => {
            crate::registration::record_error(error);
            return;
        }
    }
    let mut silent_reporter = SilentReporter::install();
    let _handle = crate::state_hook::register_fallible::<_>(StateEvent::BeforeExit, i32::MAX, move || {
        silent_reporter.restore();
        Ok(())
    });
    let mut pending = Some((mode, limit, sample));
    let _handle = crate::state_hook::register_fallible::<_>(StateEvent::AfterScript, i32::MAX, move || {
        let Some((mode, limit, sample)) = pending.take() else {
            return Ok(());
        };
        initialize_event_measure::<_>(mode, limit, sample)
    });
}

fn initialize_event_measure<S>(mode: MeasureMode, limit: MeasureLimit, sample: S) -> RuntimeResult<()>
where
    CurrentBackend: PlantReadBackend + WorldResetBackend,
    S: Fn(&Rc<RefCell<EventMeasureTask>>, TickMeta) -> RuntimeResult<()> + 'static,
{
    let (protection, imp_leak, declared_cards, reconstructed_rounds, window_end) =
        crate::setup::with_script_setup(|setup| {
            (
                setup.measurement.protection().clone(),
                setup.measurement.imp_leak(),
                setup.desired_cards.clone(),
                setup.measurement.completed_rounds(),
                setup.measurement.window_end(),
            )
        });
    let mut config = EventMeasureConfig::new(mode, protection, declared_cards)
        .map_err(|error| RuntimeError::new(error.to_string()))?;
    config.set_window_end(window_end);
    config.set_imp_leak(imp_leak);
    let shard = crate::session::session_shard();
    let artifact_limit = limit;
    let limit = if let Some(total) = limit.trial_target() {
        let local = local_trial_quota(total, shard);
        if local == 0 {
            let artifact =
                EventMeasureTask::empty_artifact_with_run(Some(0), config, artifact_limit, reconstructed_rounds, shard);
            crate::session::set_session_artifact(artifact)?;
            crate::session::stop_script();
            return Ok(());
        }
        MeasureLimit::trials(local).map_err(|error| RuntimeError::new(error.to_string()))?
    } else {
        limit
    };
    let mut task = EventMeasureTask::new(
        limit,
        config,
        reconstructed_rounds,
        shard,
        crate::session::completed_rounds(),
    );
    task.set_artifact_limit(artifact_limit);
    let task = Rc::new(RefCell::new(task));
    crate::event::install_internal_event_interceptor(Box::new(SharedEventMeasure::new(Rc::clone(&task))))?;

    let abort_task = Rc::clone(&task);
    let _handle = crate::state_hook::register_fallible::<_>(StateEvent::BeforeExit, i32::MAX - 1, move || {
        let artifact = {
            let mut task = abort_task.borrow_mut();
            if task.is_finished() {
                return Ok(());
            }
            task.seal_active_invalid();
            task.aborted_artifact()
                .map_err(|error| RuntimeError::new(error.to_string()))?
        };
        crate::session::set_session_artifact(artifact)
    });

    crate::session::request_world_reset(task.borrow().next_reset_config())?;
    crate::tick::spawn_finalizer::<_>(
        TickOptions::any_dispatch()
            .lifetime(TickLifetime::Session)
            .lane(TickLane::After)
            .idle_neutral()
            .name("session_event_measure"),
        move |meta| {
            run_measure_callback(|| {
                if !task.borrow().initialized() {
                    let initialized = task.borrow_mut().initialize();
                    if let Err(error) = initialized {
                        return Ok(fail_measure_session(error));
                    }
                }
                let interrupted = crate::session::take_dispatch_outcome().map(|outcome| match outcome {
                    crate::session::DispatchOutcome::RecoverableError => EventMeasureInterruption::RecoverableError,
                    crate::session::DispatchOutcome::TimingViolation => EventMeasureInterruption::TimingViolation,
                });
                if interrupted.is_none()
                    && let Err(error) = sample(&task, meta)
                {
                    return Ok(fail_measure_session(error));
                }
                let control = match rsvz_schedule::timeline::with_timeline(|timeline| {
                    task.borrow_mut().tick_at(
                        crate::session::world_epoch(),
                        meta.clock,
                        timeline.wave_clocks(),
                        crate::session::completed_rounds(),
                        interrupted,
                    )
                }) {
                    Ok(control) => control,
                    Err(error) => return Ok(fail_measure_session(error)),
                };
                match control {
                    EventMeasureTaskControl::Continue => Ok(TickControl::Continue),
                    EventMeasureTaskControl::Reset(config) => match crate::session::request_world_reset(config) {
                        Ok(()) => Ok(TickControl::Continue),
                        Err(error) => Ok(fail_measure_session(error)),
                    },
                    EventMeasureTaskControl::Complete(artifact) => {
                        if let Err(error) = crate::session::set_session_artifact(artifact) {
                            Ok(fail_measure_session(error))
                        } else {
                            crate::session::stop_script();
                            Ok(TickControl::Stop)
                        }
                    }
                }
            })
            .or_else(|error| Ok(fail_measure_session(error)))
        },
    );
    Ok(())
}

fn sample_imp_leak_dispatch(task: &Rc<RefCell<EventMeasureTask>>, meta: TickMeta) -> RuntimeResult<()>
where
    CurrentBackend: PlantReadBackend + ZombieRawFactsBackend + ZombieStateBackend,
{
    rsvz_schedule::timeline::with_timeline(|timeline| {
        task.borrow_mut()
            .sample_imp_leak(crate::session::world_epoch(), meta.clock, timeline.wave_clocks())
    })
}

fn install_refresh(limit: MeasureLimit)
where
    CurrentBackend: BattleStatusBackend
        + WaveHealthBackend
        + WaveTimingBackend
        + WorldResetBackend
        + CommonZombieDanceBackend
        + CobImpactDelayBackend,
{
    crate::setup::with_script_setup(|setup| setup.measurement.refresh_mut().enable());
    REFRESH_RULE_APPLIER.set(Some(apply_selected_refresh_rules));
    match crate::session::claim_session_job(crate::session::SessionJobKey::new(&REFRESH_JOB_ID)) {
        Ok(true) => {}
        Ok(false) => return,
        Err(error) => {
            crate::registration::record_error(error);
            return;
        }
    }
    let mut silent_reporter = SilentReporter::install();
    let _handle = crate::state_hook::register_fallible::<_>(StateEvent::BeforeExit, i32::MAX, move || {
        silent_reporter.restore();
        Ok(())
    });
    let mut pending = Some(limit);
    let _handle = crate::state_hook::register_fallible::<_>(StateEvent::AfterScript, i32::MAX, move || {
        let Some(limit) = pending.take() else {
            return Ok(());
        };
        initialize_refresh(limit)
    });
}

fn initialize_refresh(limit: MeasureLimit) -> RuntimeResult<()>
where
    CurrentBackend: BattleStatusBackend + WaveHealthBackend + WaveTimingBackend + WorldResetBackend,
{
    let (refresh, reconstructed_rounds, window_end) = crate::setup::with_script_setup(|setup| {
        (
            setup.measurement.refresh().clone(),
            setup.measurement.completed_rounds(),
            setup.measurement.window_end(),
        )
    });

    let shard = crate::session::session_shard();
    let artifact_limit = limit;
    let limit = if let Some(total) = limit.trial_target() {
        let local = local_trial_quota(total, shard);
        if local == 0 {
            crate::session::set_session_artifact(MeasurementState::empty_artifact_with_run(
                Some(0),
                refresh,
                artifact_limit,
                reconstructed_rounds,
                shard,
                window_end,
            ))?;
            crate::session::stop_script();
            return Ok(());
        }
        MeasureLimit::trials(local).map_err(|error| RuntimeError::new(error.to_string()))?
    } else {
        limit
    };
    let mut task = RefreshTask::new_with_window(
        limit,
        refresh,
        reconstructed_rounds,
        shard,
        crate::session::completed_rounds(),
        window_end,
    );
    task.set_artifact_limit(artifact_limit);
    let task = Rc::new(RefCell::new(task));

    let observer_task = Rc::clone(&task);
    let _handle = crate::state_hook::register_fallible::<_>(StateEvent::BeforeTick, 0, move || {
        run_measure_callback(|| {
            let observed = rsvz_schedule::timeline::with_timeline(|timeline| {
                observer_task.borrow_mut().observe_after_update(timeline.wave_clocks())
            });
            if let Err(error) = observed {
                crate::session::fail_script(RuntimeError::new(error.to_string()));
            }
            Ok(())
        })
        .or_else(|error| {
            crate::session::fail_script(error);
            Ok(())
        })
    });

    crate::session::request_world_reset(task.borrow().next_reset_config())?;
    crate::tick::spawn_finalizer::<_>(
        TickOptions::any_dispatch()
            .lifetime(TickLifetime::Session)
            .lane(TickLane::After)
            .idle_neutral()
            .name("session_refresh_measure"),
        move |meta| {
            run_measure_callback(|| {
                let interrupted = crate::session::take_dispatch_outcome().map(|outcome| match outcome {
                    crate::session::DispatchOutcome::RecoverableError => MeasureTrialOutcome::Invalid,
                    crate::session::DispatchOutcome::TimingViolation => MeasureTrialOutcome::RefreshFailure,
                });
                let control = match rsvz_schedule::timeline::with_timeline(|timeline| {
                    task.borrow_mut().tick_at(
                        crate::session::world_epoch(),
                        meta.clock,
                        timeline.wave_clocks(),
                        crate::session::completed_rounds(),
                        interrupted,
                    )
                }) {
                    Ok(control) => control,
                    Err(error) => return Ok(fail_measure_session(error)),
                };
                match control {
                    RefreshTaskControl::Continue => Ok(TickControl::Continue),
                    RefreshTaskControl::Reset(config) => match crate::session::request_world_reset(config) {
                        Ok(()) => Ok(TickControl::Continue),
                        Err(error) => Ok(fail_measure_session(error)),
                    },
                    RefreshTaskControl::Complete(artifact) => {
                        if let Err(error) = crate::session::set_session_artifact(artifact) {
                            Ok(fail_measure_session(error))
                        } else {
                            crate::session::stop_script();
                            Ok(TickControl::Stop)
                        }
                    }
                }
            })
            .or_else(|error| Ok(fail_measure_session(error)))
        },
    );
    Ok(())
}

pub fn damage_narrow_trials(trials: u64)
where
    CurrentBackend: PlantReadBackend + ZombieRawFactsBackend + ZombieStateBackend + WorldResetBackend,
{
    match MeasureLimit::trials(trials) {
        Ok(limit) => install_event_measure(MeasureMode::DamageNarrow, limit, sample_imp_leak_dispatch),
        Err(error) => crate::registration::record_error(RuntimeError::new(error.to_string())),
    }
}

pub fn damage_narrow_for(duration: Duration)
where
    CurrentBackend: PlantReadBackend + ZombieRawFactsBackend + ZombieStateBackend + WorldResetBackend,
{
    match MeasureLimit::duration(duration) {
        Ok(limit) => install_event_measure(MeasureMode::DamageNarrow, limit, sample_imp_leak_dispatch),
        Err(error) => crate::registration::record_error(RuntimeError::new(error.to_string())),
    }
}

pub fn broad_pass_trials(trials: u64)
where
    CurrentBackend: PlantReadBackend + WorldResetBackend,
{
    match MeasureLimit::trials(trials) {
        Ok(limit) => install_event_measure(MeasureMode::BroadPass, limit, |_task, _meta| Ok(())),
        Err(error) => crate::registration::record_error(RuntimeError::new(error.to_string())),
    }
}

pub fn broad_pass_for(duration: Duration)
where
    CurrentBackend: PlantReadBackend + WorldResetBackend,
{
    match MeasureLimit::duration(duration) {
        Ok(limit) => install_event_measure(MeasureMode::BroadPass, limit, |_task, _meta| Ok(())),
        Err(error) => crate::registration::record_error(RuntimeError::new(error.to_string())),
    }
}

pub fn smash_trials(trials: u64)
where
    CurrentBackend: PlantReadBackend + WorldResetBackend,
{
    match MeasureLimit::trials(trials) {
        Ok(limit) => install_event_measure(MeasureMode::Smash, limit, |_task, _meta| Ok(())),
        Err(error) => crate::registration::record_error(RuntimeError::new(error.to_string())),
    }
}

pub fn smash_for(duration: Duration)
where
    CurrentBackend: PlantReadBackend + WorldResetBackend,
{
    match MeasureLimit::duration(duration) {
        Ok(limit) => install_event_measure(MeasureMode::Smash, limit, |_task, _meta| Ok(())),
        Err(error) => crate::registration::record_error(RuntimeError::new(error.to_string())),
    }
}

pub fn pogo_trials(trials: u64)
where
    CurrentBackend: PlantReadBackend + WorldResetBackend,
{
    match MeasureLimit::trials(trials) {
        Ok(limit) => install_event_measure(MeasureMode::Pogo, limit, |_task, _meta| Ok(())),
        Err(error) => crate::registration::record_error(RuntimeError::new(error.to_string())),
    }
}

pub fn pogo_for(duration: Duration)
where
    CurrentBackend: PlantReadBackend + WorldResetBackend,
{
    match MeasureLimit::duration(duration) {
        Ok(limit) => install_event_measure(MeasureMode::Pogo, limit, |_task, _meta| Ok(())),
        Err(error) => crate::registration::record_error(RuntimeError::new(error.to_string())),
    }
}

pub fn refresh_trials(trials: u64)
where
    CurrentBackend: BattleStatusBackend
        + WaveHealthBackend
        + WaveTimingBackend
        + WorldResetBackend
        + CommonZombieDanceBackend
        + CobImpactDelayBackend,
{
    match MeasureLimit::trials(trials) {
        Ok(limit) => install_refresh(limit),
        Err(error) => crate::registration::record_error(RuntimeError::new(error.to_string())),
    }
}

pub fn refresh_for(duration: Duration)
where
    CurrentBackend: BattleStatusBackend
        + WaveHealthBackend
        + WaveTimingBackend
        + WorldResetBackend
        + CommonZombieDanceBackend
        + CobImpactDelayBackend,
{
    match MeasureLimit::duration(duration) {
        Ok(limit) => install_refresh(limit),
        Err(error) => crate::registration::record_error(RuntimeError::new(error.to_string())),
    }
}

type RefreshRuleApplier = fn(&RefreshMeasureConfig) -> RuntimeResult<()>;
thread_local! { static REFRESH_RULE_APPLIER: Cell<Option<RefreshRuleApplier>> = const { Cell::new(None) }; }

fn apply_selected_refresh_rules(config: &RefreshMeasureConfig) -> RuntimeResult<()>
where
    CurrentBackend: CommonZombieDanceBackend + CobImpactDelayBackend,
{
    super::apply_refresh_rules(config).map_err(|error| {
        RuntimeError::new(crate::setup::ApplyScriptOpeningError::MeasurementModifier(error.into()).to_string())
    })
}

pub(crate) fn clear_refresh_rule_applier() {
    REFRESH_RULE_APPLIER.set(None);
}

pub(crate) fn apply_current_refresh_rules(config: &RefreshMeasureConfig) -> RuntimeResult<()> {
    match REFRESH_RULE_APPLIER.get() {
        Some(apply) => apply(config),
        None => Ok(()),
    }
}

#[cfg(test)]
mod callback_failure_tests {
    use super::*;
    use rsvz_schedule::tick::{TickDispatchResult, TickScheduler, TickTaskState};

    #[test]
    fn sample_abort_keeps_explicit_session_failure_and_stops_the_measure_task() {
        crate::session::reset_session_control();
        crate::session::fail_script(RuntimeError::new("first session failure"));
        let mut scheduler = TickScheduler::new();
        let handle = scheduler.spawn(TickOptions::any_dispatch(), |_| {
            run_measure_callback(|| -> RuntimeResult<TickControl> {
                crate::diagnostics::abort_operation(RuntimeError::new("sample read failed"));
            })
            .or_else(|error| Ok(fail_measure_session(error)))
        });
        assert_eq!(
            scheduler.dispatch_tick_catching(TickMeta::unavailable()),
            TickDispatchResult::Continue
        );
        assert_eq!(scheduler.state(handle), TickTaskState::Stopped);
        assert!(crate::session::script_stop_requested());
        assert_eq!(
            crate::session::take_fatal_session_error().unwrap().to_string(),
            "first session failure"
        );
        crate::session::reset_session_control();
    }

    #[test]
    fn measurement_boundary_does_not_convert_true_panic_to_read_failure() {
        let payload = std::panic::catch_unwind(|| {
            run_measure_callback(|| -> RuntimeResult<()> {
                panic!("actual panic");
            })
        })
        .unwrap_err();
        assert!(!rsvz_schedule::callback::is_abort(payload.as_ref()));
        assert_eq!(payload.downcast_ref::<&str>(), Some(&"actual panic"));
    }
}
