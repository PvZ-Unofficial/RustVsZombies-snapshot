//! frame operations for the selected backend.

use crate::diagnostics::with_runtime_report_time;
use crate::runtime::{CurrentFrameSample, RuntimeError, RuntimeFrameDispatch, RuntimeFrameState, runtime_frame_facts};
use crate::session::{DispatchOutcome, RuntimeDispatchState, record_dispatch_outcome, script_stop_requested};
use crate::setup::reset_script_setup;
use crate::tick::{
    dispatch_scheduler_finalizers_reporting, dispatch_scheduler_tick, dispatch_scheduler_tick_reporting,
};
use crate::timeline::{dispatch_timeline_tick, dispatch_timeline_tick_reporting};
use rsvz_backend_api::backend::{BoardReadinessBackend, GameUiBackend, WaveTimingBackend};
use rsvz_model::{RelativeTime, WaveClockState, WaveTimingSnapshot};
use rsvz_profiling::{DetailedProfileStage, measure_detailed_profile};
use rsvz_schedule::event::{EventDispatcher, with_events};
use rsvz_schedule::tick::{TickDispatchResult, TickMeta, with_scheduler, with_scheduler_ref};
use rsvz_schedule::timeline::{TimelineDispatchResult, current_wave_refresh_clock, with_timeline, with_timeline_ref};
use std::cell::{Cell, RefCell};

thread_local! {
    pub(crate) static FRAME: RefCell<RuntimeFrameState> = RefCell::new(RuntimeFrameState::default());
    pub(crate) static DISPATCH_STATE: Cell<RuntimeDispatchState> = const { Cell::new(RuntimeDispatchState::new()) };
}

pub(crate) fn profile_if<R>(enabled: bool, stage: DetailedProfileStage, f: impl FnOnce() -> R) -> R {
    if enabled {
        measure_detailed_profile(stage, f)
    } else {
        f()
    }
}

#[must_use]
pub fn runtime_frame_meta() -> TickMeta {
    FRAME.with(|frame| frame.borrow().last_meta())
}

pub(crate) fn sample_round_completion(game_ui: Option<rsvz_model::GameUi>) -> CurrentFrameSample {
    let (meta, snapshot) = FRAME.with_borrow_mut(|frame| {
        frame.input(crate::runtime::RuntimeFrameFacts {
            phase: rsvz_schedule::tick::TickPhase::RoundComplete,
            game_ui,
            clock: None,
            snapshot: None,
        })
    });
    CurrentFrameSample {
        // This is a logical notification, not a physical frame to resample after hooks.
        access_epoch: None,
        meta,
        snapshot,
        report_time: None,
    }
}

#[must_use]
pub fn runtime_timed_operations_idle() -> bool {
    with_timeline_ref(|timeline| timeline.is_idle()) && with_scheduler_ref(|scheduler| scheduler.is_idle())
}

#[doc(hidden)]
pub fn sample_current_frame() -> Result<CurrentFrameSample, RuntimeError>
where
    rsvz_current::CurrentBackend: GameUiBackend + BoardReadinessBackend + WaveTimingBackend,
{
    let facts = runtime_frame_facts()?;
    let relative_time = facts
        .snapshot
        .and_then(|snapshot| with_timeline_ref(|timeline| report_time(snapshot, timeline.wave_clocks())));
    let (meta, snapshot) = FRAME.with(|frame| frame.borrow_mut().input(facts));
    Ok(CurrentFrameSample {
        access_epoch: rsvz_current::backend_access_epoch(),
        meta,
        snapshot,
        report_time: relative_time,
    })
}

impl CurrentFrameSample {
    /// BeforeTick and unbound callbacks may replace the world after sampling.
    /// Resampling must not advance the per-dispatch new-frame bookkeeping.
    fn revalidated(&self) -> Result<Self, RuntimeError>
    where
        rsvz_current::CurrentBackend: GameUiBackend + BoardReadinessBackend + WaveTimingBackend,
    {
        let epoch = rsvz_current::backend_access_epoch();
        if self.access_epoch.is_none() || self.access_epoch == epoch {
            return Ok(*self);
        }
        let facts = runtime_frame_facts()?;
        Ok(Self {
            access_epoch: epoch,
            meta: TickMeta {
                phase: facts.phase,
                game_ui: facts.game_ui,
                clock: facts.clock,
                is_new_frame: facts.phase == rsvz_schedule::tick::TickPhase::Playing
                    && (self.meta.is_new_frame || facts.clock != self.meta.clock),
            },
            snapshot: facts.snapshot,
            report_time: facts
                .snapshot
                .and_then(|snapshot| with_timeline_ref(|timeline| report_time(snapshot, timeline.wave_clocks()))),
        })
    }
}

fn report_time(snapshot: WaveTimingSnapshot, wave_clocks: &WaveClockState) -> Option<RelativeTime> {
    let refresh_clock = wave_clocks
        .refresh_clock(snapshot.current_wave)
        .or_else(|| current_wave_refresh_clock(snapshot))?;
    Some(RelativeTime::new(
        snapshot.current_wave,
        snapshot.clock.saturating_sub(refresh_clock),
    ))
}

fn timeline_outcome(result: TimelineDispatchResult) -> RuntimeFrameDispatch {
    match result {
        TimelineDispatchResult::Continue => RuntimeFrameDispatch::Continue,
        TimelineDispatchResult::OperationError(error) => RuntimeFrameDispatch::OperationError(error),
        TimelineDispatchResult::OperationPanic => RuntimeFrameDispatch::OperationPanic,
        TimelineDispatchResult::BackendError(error) => RuntimeFrameDispatch::TimingBackendError(error),
        TimelineDispatchResult::BackendControlError(error) => RuntimeFrameDispatch::TimingControlError(error),
        TimelineDispatchResult::ControlTimingError(error) => {
            RuntimeFrameDispatch::TimingControlViolation(error.to_string())
        }
        TimelineDispatchResult::TimingViolation(error) => RuntimeFrameDispatch::TimingViolation(error.to_string()),
    }
}

fn tick_outcome(result: TickDispatchResult) -> RuntimeFrameDispatch {
    match result {
        TickDispatchResult::Continue => RuntimeFrameDispatch::Continue,
        TickDispatchResult::TaskError(error) => RuntimeFrameDispatch::FrameCallbackError(error),
        TickDispatchResult::TaskPanic => RuntimeFrameDispatch::FrameCallbackPanic,
    }
}

pub fn dispatch_runtime_tick(snapshot: WaveTimingSnapshot, meta: TickMeta) -> RuntimeFrameDispatch
where
    rsvz_current::CurrentBackend:
        rsvz_backend_api::BoardReadinessBackend + rsvz_backend_api::GameUiBackend + rsvz_backend_api::WaveTimingBackend,
{
    match timeline_outcome(dispatch_timeline_tick(snapshot, meta)) {
        RuntimeFrameDispatch::Continue => tick_outcome(dispatch_scheduler_tick(meta)),
        result => result,
    }
}

/// Dispatches one runtime tick while reporting ordinary callback errors and
/// preserving fatal dispatch outcomes for the host.
pub fn dispatch_runtime_tick_reporting(
    snapshot: WaveTimingSnapshot, meta: TickMeta, mut report_error: impl FnMut(RuntimeError),
) -> RuntimeFrameDispatch
where
    rsvz_current::CurrentBackend:
        rsvz_backend_api::BoardReadinessBackend + rsvz_backend_api::GameUiBackend + rsvz_backend_api::WaveTimingBackend,
{
    match timeline_outcome(dispatch_timeline_tick_reporting(snapshot, meta, &mut report_error)) {
        RuntimeFrameDispatch::Continue => tick_outcome(dispatch_scheduler_tick_reporting(meta, &mut report_error)),
        result => result,
    }
}

#[doc(hidden)]
pub fn dispatch_sample_timeline(sample: &CurrentFrameSample) -> RuntimeFrameDispatch
where
    rsvz_current::CurrentBackend:
        rsvz_backend_api::BoardReadinessBackend + rsvz_backend_api::GameUiBackend + rsvz_backend_api::WaveTimingBackend,
{
    let sample = match sample.revalidated() {
        Ok(sample) => sample,
        Err(error) => return RuntimeFrameDispatch::TimingBackendError(error),
    };

    with_runtime_report_time(sample.report_time, || {
        sample.snapshot.map_or(RuntimeFrameDispatch::Continue, |snapshot| {
            timeline_outcome(dispatch_timeline_tick(snapshot, sample.meta))
        })
    })
}

#[doc(hidden)]
pub fn dispatch_sample_timeline_reporting(
    sample: &CurrentFrameSample, report_error: &mut impl FnMut(RuntimeError),
) -> RuntimeFrameDispatch
where
    rsvz_current::CurrentBackend:
        rsvz_backend_api::BoardReadinessBackend + rsvz_backend_api::GameUiBackend + rsvz_backend_api::WaveTimingBackend,
{
    let sample = match sample.revalidated() {
        Ok(sample) => sample,
        Err(error) => return RuntimeFrameDispatch::TimingBackendError(error),
    };

    with_runtime_report_time(sample.report_time, || {
        sample.snapshot.map_or(RuntimeFrameDispatch::Continue, |snapshot| {
            timeline_outcome(dispatch_timeline_tick_reporting(snapshot, sample.meta, report_error))
        })
    })
}

#[doc(hidden)]
pub fn dispatch_sample_scheduler(sample: &CurrentFrameSample) -> RuntimeFrameDispatch
where
    rsvz_current::CurrentBackend:
        rsvz_backend_api::BoardReadinessBackend + rsvz_backend_api::GameUiBackend + rsvz_backend_api::WaveTimingBackend,
{
    let sample = match sample.revalidated() {
        Ok(sample) => sample,
        Err(error) => return RuntimeFrameDispatch::FrameCallbackError(error),
    };

    with_runtime_report_time(sample.report_time, || {
        tick_outcome(dispatch_scheduler_tick(sample.meta))
    })
}

#[doc(hidden)]
pub fn dispatch_sample_scheduler_reporting(
    sample: &CurrentFrameSample, report_error: &mut impl FnMut(RuntimeError),
) -> RuntimeFrameDispatch
where
    rsvz_current::CurrentBackend:
        rsvz_backend_api::BoardReadinessBackend + rsvz_backend_api::GameUiBackend + rsvz_backend_api::WaveTimingBackend,
{
    let sample = match sample.revalidated() {
        Ok(sample) => sample,
        Err(error) => return RuntimeFrameDispatch::FrameCallbackError(error),
    };

    with_runtime_report_time(sample.report_time, || {
        tick_outcome(dispatch_scheduler_tick_reporting(sample.meta, report_error))
    })
}

#[doc(hidden)]
pub fn dispatch_current_frame_sample(
    sample: &CurrentFrameSample, mut report_error: impl FnMut(RuntimeError),
) -> RuntimeFrameDispatch
where
    rsvz_current::CurrentBackend:
        rsvz_backend_api::BoardReadinessBackend + rsvz_backend_api::GameUiBackend + rsvz_backend_api::WaveTimingBackend,
{
    let timeline_result = dispatch_sample_timeline_reporting(sample, &mut report_error);
    if matches!(
        timeline_result,
        RuntimeFrameDispatch::TimingViolation(_) | RuntimeFrameDispatch::TimingControlViolation(_)
    ) {
        record_dispatch_outcome(DispatchOutcome::TimingViolation);
    }
    match timeline_result {
        RuntimeFrameDispatch::Continue if !script_stop_requested() => {
            dispatch_sample_scheduler_reporting(sample, &mut report_error)
        }
        RuntimeFrameDispatch::Continue => RuntimeFrameDispatch::Continue,
        result @ (RuntimeFrameDispatch::TimingViolation(_) | RuntimeFrameDispatch::TimingControlViolation(_)) => {
            match with_runtime_report_time(sample.report_time, || {
                dispatch_scheduler_finalizers_reporting(sample.meta, &mut report_error)
            }) {
                TickDispatchResult::Continue => result,
                error => tick_outcome(error),
            }
        }
        result => result,
    }
}

pub fn dispatch_current_runtime_frame() -> RuntimeFrameDispatch
where
    rsvz_current::CurrentBackend: GameUiBackend + BoardReadinessBackend + WaveTimingBackend,
{
    let sample = match sample_current_frame() {
        Ok(sample) => sample,
        Err(error) => return RuntimeFrameDispatch::TimingBackendError(error),
    };
    match dispatch_sample_timeline(&sample) {
        RuntimeFrameDispatch::Continue => dispatch_sample_scheduler(&sample),
        result => result,
    }
}

/// Dispatches the current runtime frame while reporting ordinary operation and
/// task errors without promoting them to host-fatal outcomes.
pub fn dispatch_current_runtime_frame_reporting(mut report_error: impl FnMut(RuntimeError)) -> RuntimeFrameDispatch
where
    rsvz_current::CurrentBackend: GameUiBackend + BoardReadinessBackend + WaveTimingBackend,
{
    let sample = match sample_current_frame() {
        Ok(sample) => sample,
        Err(error) => return RuntimeFrameDispatch::TimingBackendError(error),
    };
    match dispatch_sample_timeline_reporting(&sample, &mut report_error) {
        RuntimeFrameDispatch::Continue => dispatch_sample_scheduler_reporting(&sample, &mut report_error),
        result => result,
    }
}

pub fn dispatch_runtime_frame() -> RuntimeFrameDispatch
where
    rsvz_current::CurrentBackend: GameUiBackend + BoardReadinessBackend + WaveTimingBackend,
{
    dispatch_current_runtime_frame_reporting(crate::diagnostics::report_runtime_error)
}

pub fn reset_runtime_state_preserving_backend() {
    with_timeline(|timeline| timeline.clear_all());
    with_scheduler(|scheduler| scheduler.clear_all());
    with_events(EventDispatcher::clear_session);
    FRAME.with(|frame| *frame.borrow_mut() = RuntimeFrameState::default());
    crate::diagnostics::clear_report_time();
    reset_script_setup();
}

#[doc(hidden)]
#[must_use]
pub fn runtime_dispatch_state() -> RuntimeDispatchState {
    DISPATCH_STATE.get()
}

#[doc(hidden)]
pub fn set_runtime_dispatch_state(state: RuntimeDispatchState) {
    DISPATCH_STATE.set(state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsvz_model::Wave;
    #[test]
    fn report_time_uses_the_current_wave_refresh_clock() {
        let mut clocks = WaveClockState::new();
        clocks.record_refresh_clock(Wave(2), 1_000);

        assert_eq!(
            report_time(WaveTimingSnapshot::minimal(1_723, Wave(2)), &clocks),
            Some(RelativeTime::new(Wave(2), 723))
        );
    }
}

mod entities;
pub(crate) use entities::with_frame;
pub use entities::{Frame, PlantRef, ZombieRef};
