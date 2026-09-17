//! Current-world capture through public rsvz APIs.

use std::cell::RefCell;
use std::rc::Rc;

use super::core::read_error;
use super::*;
use rsvz::__private::CurrentBackend;
use rsvz::core::backend::{
    BattleStatusBackend, DancerClockWriteBackend, DropRuleEditBackend, RandomControlBackend, SunProductionModeBackend,
    WaveTimingBackend, WorldResetBackend,
};
use rsvz::core::model::{BattleStatus, GameEvent, RandomMode, ReloadMode};
use rsvz::event::{EventLifetime, EventOptions};
use rsvz::runtime::{RuntimeError, RuntimeResult};
use rsvz::state_hook::{StateEvent, register_fallible};

const EVENT_CAPACITY: usize = 4096;
static WITNESS_JOB_ID: u8 = 1;

struct EventBuffer {
    events: Vec<GameEvent>,
    overflowed: bool,
}

impl EventBuffer {
    fn new() -> Self {
        Self {
            events: Vec::with_capacity(EVENT_CAPACITY),
            overflowed: false,
        }
    }

    fn push(&mut self, event: GameEvent) {
        if self.events.len() == EVENT_CAPACITY {
            self.overflowed = true;
        } else {
            self.events.push(event);
        }
    }

    fn clear(&mut self) {
        self.events.clear();
        self.overflowed = false;
    }
}

struct WitnessRun {
    options: WitnessOptions,
    visitor: CanonicalVisitor,
    events: EventBuffer,
    manifest: ReproManifest,
    writer: Option<WitnessSpool>,
    sealed: bool,
    pending: Option<Result<CapturedFrame, WitnessError>>,
    frame: u64,
    frames_written: u64,
    initial_rounds: u64,
    observed_rounds: u64,
    round_boundary: bool,
    dancer_clock_reset_pending: bool,
    active: bool,
    terminal: bool,
}

impl WitnessRun {
    fn new(options: WitnessOptions, manifest: ReproManifest) -> Self {
        Self {
            options,
            visitor: CanonicalVisitor::new(),
            events: EventBuffer::new(),
            manifest,
            writer: None,
            sealed: false,
            pending: None,
            frame: 0,
            frames_written: 0,
            initial_rounds: 0,
            observed_rounds: 0,
            round_boundary: false,
            dancer_clock_reset_pending: false,
            active: false,
            terminal: false,
        }
    }

    fn capture(&mut self, world_epoch: u64, dancer_clock: u32)
    where
        CurrentBackend: WitnessCapabilities + DancerClockWriteBackend,
    {
        if !self.active || self.pending.is_some() {
            return;
        }
        // This short condition read ends before either physical preparation.
        let level_end_countdown = match rsvz::__private::with_backend_shared(|access| access.level_end_countdown())
            .map_err(|error| read_error("board.wave_timing", error))
            .and_then(|result| result.map_err(|error| read_error("board.wave_timing", error)))
        {
            Ok(countdown) => Some(countdown),
            Err(error) => {
                self.pending = Some(Err(error));
                return;
            }
        };
        if is_level_end_transport(level_end_countdown) {
            self.events.clear();
            return;
        }
        if self.dancer_clock_reset_pending() {
            // Keep this latest pre-capture compensation as well as the earliest
            // BeforeTick normalization. Only this successful pass clears pending.
            let reset = rsvz::with_backend(|backend| -> Result<(), WitnessError> {
                backend
                    .set_dancer_clock(dancer_clock)
                    .map_err(|error| read_error("witness.dancer_clock", error))?;
                Ok(())
            });
            if let Err(error) = reset {
                self.pending = Some(Err(error));
                return;
            }
            self.take_dancer_clock_reset();
        }
        if self.events.overflowed {
            self.pending = Some(Err(WitnessError::Overflow("public event buffer")));
            self.events.clear();
            return;
        }
        let result = rsvz::__private::with_backend_shared(|access| -> Result<_, WitnessError> {
            let backend = access;
            self.terminal = matches!(
                backend
                    .battle_status()
                    .map_err(|error| read_error("board.battle_status", error))?,
                BattleStatus::Lost | BattleStatus::Ended
            );
            let full = self.options.capture.captures(self.frame);
            self.visitor
                .capture(backend, world_epoch, self.frame, &self.events.events, full)
        })
        .map_err(|error| read_error("board", error))
        .and_then(|result| result);
        self.events.clear();
        self.pending = Some(result);
    }

    fn activate(&mut self, completed_rounds: u64) -> bool {
        if !self.active {
            self.initial_rounds = completed_rounds;
            self.observed_rounds = completed_rounds;
            self.active = true;
            true
        } else {
            false
        }
    }

    fn note_completed_rounds(&mut self, completed_rounds: u64) -> bool {
        if !self.active || completed_rounds <= self.observed_rounds {
            return false;
        }
        self.observed_rounds = completed_rounds;
        self.round_boundary = true;
        // The first physical tick after EnterFight is the completed-round/UI boundary that
        // Witness deliberately excludes. Rearm normalization for the first captured gameplay
        // tick, after backend-specific transport has finished.
        self.dancer_clock_reset_pending = true;
        self.events.clear();
        true
    }

    fn take_dancer_clock_reset(&mut self) -> bool {
        std::mem::take(&mut self.dancer_clock_reset_pending)
    }

    const fn dancer_clock_reset_pending(&self) -> bool {
        self.dancer_clock_reset_pending
    }

    fn take_writer(&mut self) -> Result<WitnessSpool, WitnessError> {
        self.sealed = true;
        self.writer
            .take()
            .ok_or(WitnessError::Encoding("witness already finalized"))
    }

    fn ensure_writer<B: WitnessCapabilities>(&mut self, backend: &B, resolved_setup: bool) -> Result<(), WitnessError> {
        if self.writer.is_some() {
            return Ok(());
        }
        if self.sealed {
            return Err(WitnessError::Encoding("witness already finalized"));
        }
        if resolved_setup {
            self.manifest.record_resolved_spawn(backend)?;
        }
        let header = WitnessHeader::new(backend, &self.options, self.manifest.clone());
        self.writer = Some(WitnessSpool::new(&header)?);
        Ok(())
    }
}

fn is_level_end_transport(countdown: Option<i32>) -> bool {
    countdown.is_some_and(|countdown| countdown > 0)
}

fn spawn_setup_is_explicit(setup: &rsvz::core::setup::ScriptSetup) -> bool {
    setup.spawn_list.is_some() && setup.zombie_spawn_request.is_none()
}

fn card_setup_is_explicit(setup: &rsvz::core::setup::ScriptSetup) -> bool {
    setup
        .desired_cards
        .as_ref()
        .is_some_and(|cards| cards.len() == rsvz::core::model::MAX_SEED_SLOTS)
}

fn install(options: WitnessOptions) -> RuntimeResult<()>
where
    CurrentBackend: WitnessCapabilities
        + DancerClockWriteBackend
        + DropRuleEditBackend
        + SunProductionModeBackend
        + WorldResetBackend
        + 'static,
{
    options
        .validate()
        .map_err(|error| RuntimeError::new(error.to_string()))?;
    let shard = rsvz::session_shard();
    if shard.count != 1 {
        return Err(RuntimeError::new(
            "Witness requires exactly one worker (--threads 1 for PE)",
        ));
    }
    rsvz::setup::with_script_setup(|setup| setup.reload_mode = ReloadMode::MainUiOrFightUi);
    match rsvz::claim_session_job(rsvz::SessionJobKey::new(&WITNESS_JOB_ID))? {
        true => {}
        false => return Ok(()),
    }

    let mut pending = Some(options);
    let _handle = register_fallible::<_>(StateEvent::AfterScript, i32::MAX, move || {
        let Some(options) = pending.take() else {
            return Ok(());
        };
        initialize(&options)
    });
    Ok(())
}

fn initialize(options: &WitnessOptions) -> RuntimeResult<()>
where
    CurrentBackend: WitnessCapabilities
        + DancerClockWriteBackend
        + DropRuleEditBackend
        + SunProductionModeBackend
        + WorldResetBackend
        + 'static,
{
    let (manifest, spawn_valid, cards_valid) = rsvz::setup::with_script_setup(|setup| {
        (
            ReproManifest::from_setup(setup, &options.repro),
            spawn_setup_is_explicit(setup),
            card_setup_is_explicit(setup),
        )
    });
    if !spawn_valid {
        return Err(RuntimeError::new(
            "Witness requires an explicit spawn list and rejects deferred spawn-list generation",
        ));
    }
    if !cards_valid {
        return Err(RuntimeError::new(
            "Witness requires exactly ten explicitly selected cards before Locked setup",
        ));
    }

    let run = Rc::new(RefCell::new(WitnessRun::new(options.clone(), manifest)));

    let origin_run = Rc::clone(&run);
    let locked_random = options.locked_random;
    let wave_spawn_seed = options.wave_spawn_random.then_some(options.reset.seed);
    let dancer_clock = rsvz::core::setup::mix_dancer_clock(options.reset.seed);
    let _handle = register_fallible::<_>(StateEvent::EnterFight, i32::MIN, move || {
        origin_run.borrow_mut().dancer_clock_reset_pending = true;
        let setup_error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> RuntimeResult<()> {
            rsvz::with_backend(|backend| {
                backend.set_random_mode(RandomMode::Locked(locked_random))?;
                backend.set_wave_spawn_random_seed(wave_spawn_seed)?;
                backend.set_item_drop_disabled(true)
            })
            .map_err(RuntimeError::from)?;
            rsvz::core::modifier::stabilize_natural_sun_drop()?;
            rsvz::with_backend(|backend| backend.set_sun_production_mode(origin_run.borrow().options.sun))
                .map_err(RuntimeError::from)
        }))
        .unwrap_or_else(|payload| match rsvz::__private::into_callback_error(payload) {
            Ok(error) => Err(error),
            Err(payload) => std::panic::resume_unwind(payload),
        })
        .err()
        .map(|error| error.to_string());
        if let Some(message) = setup_error {
            origin_run.borrow_mut().pending = Some(Err(WitnessError::Read {
                path: "witness.setup",
                message,
            }));
        }
        Ok(())
    });

    let dancer_clock_run = Rc::clone(&run);
    let _handle = register_fallible::<_>(StateEvent::BeforeTick, i32::MIN, move || {
        if !dancer_clock_run.borrow().dancer_clock_reset_pending() {
            return Ok(());
        }
        rsvz::with_backend(|backend| {
            backend.set_dancer_clock(dancer_clock)?;
            Ok::<_, <CurrentBackend as rsvz::core::backend::Backend>::Error>(())
        })
        .map_err(|error| RuntimeError::new(error.to_string()))?;
        Ok(())
    });

    rsvz::event::reserve(EVENT_CAPACITY).map_err(|error| RuntimeError::new(error.to_string()))?;
    let event_run = Rc::clone(&run);
    rsvz::event::on(EventOptions::new().lifetime(EventLifetime::Session), move |event| {
        event_run.borrow_mut().events.push(*event)
    })
    .map_err(|error| RuntimeError::new(error.to_string()))?;

    let enter_run = Rc::clone(&run);
    let _handle = register_fallible::<_>(StateEvent::EnterFight, i32::MAX, move || {
        enter_run.borrow_mut().activate(rsvz::session::completed_rounds());
        rsvz::__private::with_backend_shared(|access| {
            enter_run
                .borrow_mut()
                .ensure_writer(access, true)
                .map_err(|error| RuntimeError::new(error.to_string()))
        })
        .map_err(|error| RuntimeError::new(error.to_string()))??;
        Ok(())
    });

    let capture_run = Rc::clone(&run);
    let _handle = register_fallible::<_>(StateEvent::BeforeTick, i32::MAX, move || {
        if capture_run
            .borrow_mut()
            .note_completed_rounds(rsvz::session::completed_rounds())
        {
            return Ok(());
        }
        let world_epoch = rsvz::session::world_epoch();
        capture_run.borrow_mut().capture(world_epoch, dancer_clock);
        Ok(())
    });

    let exit_run = Rc::clone(&run);
    let _handle = register_fallible::<_>(StateEvent::BeforeExit, i32::MAX - 1, move || {
        if exit_run.borrow().sealed {
            return Ok(());
        }
        rsvz::__private::with_backend_shared(|backend| {
            exit_run
                .borrow_mut()
                .ensure_writer(backend, false)
                .map_err(|error| RuntimeError::new(error.to_string()))
        })
        .map_err(|error| RuntimeError::new(error.to_string()))??;
        let writer = {
            let mut run = exit_run.borrow_mut();
            let Some(writer) = run.writer.take() else {
                return Ok(());
            };
            (writer, run.frames_written)
        };
        let artifact = writer
            .0
            .finish(WitnessCompletion::Incomplete { frames: writer.1 })
            .map_err(|error| RuntimeError::new(error.to_string()))?;
        rsvz::publish_artifact(artifact)
    });

    let tick_run = Rc::clone(&run);
    let _handle = register_fallible::<_>(StateEvent::AfterTick, 0, move || tick(&tick_run));

    rsvz::with_backend(|backend| backend.set_random_mode(RandomMode::Locked(options.locked_random)))
        .map_err(|error| RuntimeError::new(error.to_string()))?;
    if let Err(error) = rsvz::request_world_reset(options.reset) {
        rsvz::with_backend(|backend| {
            run.borrow_mut()
                .ensure_writer(&*backend, false)
                .map_err(|error| RuntimeError::new(error.to_string()))
        })?;
        return finish_invalid(&run, 0, format!("request reset: {error}"));
    }
    Ok(())
}

fn tick(run: &Rc<RefCell<WitnessRun>>) -> RuntimeResult<()> {
    let round_boundary = {
        let mut run = run.borrow_mut();
        std::mem::take(&mut run.round_boundary)
    };
    if round_boundary {
        let stop = {
            let run = run.borrow();
            limit_reached(
                run.options.limit,
                run.frame.saturating_sub(1),
                run.initial_rounds,
                rsvz::session::completed_rounds(),
            )
        };
        if stop {
            finish_complete(run)?;
        }
        return Ok(());
    }
    let pending = run.borrow_mut().pending.take();
    let Some(pending) = pending else {
        return Ok(());
    };
    let captured = match pending {
        Ok(captured) => captured,
        Err(error) => {
            let frame = run.borrow().frame;
            finish_invalid(run, frame, error.to_string())?;
            return Ok(());
        }
    };
    let frame_number = captured.frame;
    let outcome = match rsvz::session::take_dispatch_outcome() {
        None => WitnessTransitionOutcome::Clean,
        Some(rsvz::DispatchOutcome::RecoverableError) => WitnessTransitionOutcome::RecoverableError,
        Some(rsvz::DispatchOutcome::TimingViolation) => WitnessTransitionOutcome::TimingViolation,
    };
    let finished = match captured.finish(outcome) {
        Ok(frame) => frame,
        Err(error) => {
            finish_invalid(run, frame_number, error.to_string())?;
            return Ok(());
        }
    };
    let records = {
        let mut run = run.borrow_mut();
        let Some(writer) = &mut run.writer else {
            return Err(RuntimeError::new("Witness writer is unavailable before completion"));
        };
        match writer.push(finished) {
            Ok(records) => records,
            Err(error) => {
                let doomed = run.writer.take();
                run.sealed = true;
                drop(run);
                drop(doomed);
                rsvz::fail_script(RuntimeError::new(error.to_string()));
                return Ok(());
            }
        }
    };
    let (stop, interrupted) = {
        let mut run = run.borrow_mut();
        run.visitor.recycle(records);
        run.frames_written += 1;
        run.frame += 1;
        let stop = limit_reached(
            run.options.limit,
            frame_number,
            run.initial_rounds,
            rsvz::session::completed_rounds(),
        ) || run.terminal;
        (stop, outcome != WitnessTransitionOutcome::Clean)
    };
    if interrupted {
        finish_invalid(run, frame_number, format!("transition outcome: {outcome:?}"))?;
        return Ok(());
    }
    if stop {
        finish_complete(run)?;
        return Ok(());
    }
    Ok(())
}

fn limit_reached(limit: WitnessLimit, frame: u64, initial_rounds: u64, completed_rounds: u64) -> bool {
    match limit {
        WitnessLimit::Frames(limit) => frame >= limit,
        WitnessLimit::CompletedRounds(rounds) => completed_rounds >= initial_rounds.saturating_add(rounds),
    }
}

fn finish_complete(run: &Rc<RefCell<WitnessRun>>) -> RuntimeResult<()> {
    let (writer, frames) = {
        let mut run = run.borrow_mut();
        (
            run.take_writer()
                .map_err(|error| RuntimeError::new(error.to_string()))?,
            run.frames_written,
        )
    };
    let artifact = writer
        .finish(WitnessCompletion::Complete { frames })
        .map_err(|error| RuntimeError::new(error.to_string()))?;
    rsvz::publish_artifact(artifact)?;
    rsvz::stop_script();
    Ok(())
}

fn finish_invalid(run: &Rc<RefCell<WitnessRun>>, frame: u64, message: String) -> RuntimeResult<()> {
    let (writer, frames) = {
        let mut run = run.borrow_mut();
        (
            run.take_writer()
                .map_err(|error| RuntimeError::new(error.to_string()))?,
            run.frames_written,
        )
    };
    let artifact = writer
        .finish(WitnessCompletion::Invalid {
            frames,
            first_error_frame: frame,
            message,
        })
        .map_err(|error| RuntimeError::new(error.to_string()))?;
    rsvz::publish_artifact(artifact)?;
    rsvz::stop_script();
    Ok(())
}

/// Backend-selected Witness installer for compile-time script extensions.
#[derive(Clone, Copy, Debug)]
pub struct StartWitness;

impl StartWitness {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for StartWitness {
    fn default() -> Self {
        Self::new()
    }
}

impl FnOnce<()> for StartWitness
where
    CurrentBackend: WitnessCapabilities
        + DancerClockWriteBackend
        + DropRuleEditBackend
        + SunProductionModeBackend
        + WorldResetBackend
        + 'static,
{
    type Output = RuntimeResult<()>;

    extern "rust-call" fn call_once(self, (): ()) -> Self::Output {
        install(WitnessOptions::default())
    }
}

impl FnMut<()> for StartWitness
where
    CurrentBackend: WitnessCapabilities
        + DancerClockWriteBackend
        + DropRuleEditBackend
        + SunProductionModeBackend
        + WorldResetBackend
        + 'static,
{
    extern "rust-call" fn call_mut(&mut self, (): ()) -> Self::Output {
        install(WitnessOptions::default())
    }
}

impl Fn<()> for StartWitness
where
    CurrentBackend: WitnessCapabilities
        + DancerClockWriteBackend
        + DropRuleEditBackend
        + SunProductionModeBackend
        + WorldResetBackend
        + 'static,
{
    extern "rust-call" fn call(&self, (): ()) -> Self::Output {
        install(WitnessOptions::default())
    }
}

impl FnOnce<(WitnessOptions,)> for StartWitness
where
    CurrentBackend: WitnessCapabilities
        + DancerClockWriteBackend
        + DropRuleEditBackend
        + SunProductionModeBackend
        + WorldResetBackend
        + 'static,
{
    type Output = RuntimeResult<()>;

    extern "rust-call" fn call_once(self, (options,): (WitnessOptions,)) -> Self::Output {
        install(options)
    }
}

impl FnMut<(WitnessOptions,)> for StartWitness
where
    CurrentBackend: WitnessCapabilities
        + DancerClockWriteBackend
        + DropRuleEditBackend
        + SunProductionModeBackend
        + WorldResetBackend
        + 'static,
{
    extern "rust-call" fn call_mut(&mut self, (options,): (WitnessOptions,)) -> Self::Output {
        install(options)
    }
}

impl Fn<(WitnessOptions,)> for StartWitness
where
    CurrentBackend: WitnessCapabilities
        + DancerClockWriteBackend
        + DropRuleEditBackend
        + SunProductionModeBackend
        + WorldResetBackend
        + 'static,
{
    extern "rust-call" fn call(&self, (options,): (WitnessOptions,)) -> Self::Output {
        install(options)
    }
}

/// Starts a single-session deterministic Witness.
#[expect(
    non_upper_case_globals,
    reason = "callable user API aliases use function-style names"
)]
pub const start: StartWitness = StartWitness::new();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_start_arities_use_the_current_backend() {
        fn default_form<F: Fn() -> RuntimeResult<()>>(_: &F) {}
        fn options_form<F: Fn(WitnessOptions) -> RuntimeResult<()>>(_: &F) {}
        default_form(&start);
        options_form(&start);
    }

    #[test]
    fn locked_setup_requires_a_complete_explicit_card_bank() {
        let mut setup = rsvz::core::setup::ScriptSetup::default();
        assert!(!card_setup_is_explicit(&setup));

        setup.desired_cards = Some(vec![
            rsvz::core::model::CardSelection::Plant(
                rsvz::core::model::PlantKind::Peashooter
            );
            rsvz::core::model::MAX_SEED_SLOTS - 1
        ]);
        assert!(!card_setup_is_explicit(&setup));

        setup
            .desired_cards
            .as_mut()
            .expect("fixture card bank")
            .push(rsvz::core::model::CardSelection::Plant(
                rsvz::core::model::PlantKind::Sunflower,
            ));
        assert!(card_setup_is_explicit(&setup));
    }

    #[test]
    fn defaults_are_locked_and_completed_round_limits_are_supported() {
        let mut options = WitnessOptions::default();
        assert_eq!(options.reset.initial_sun, 8_000);
        assert_eq!(options.reset.completed_rounds, 63);
        assert_eq!(options.locked_random, 0x5eed_1051);
        assert!(!options.wave_spawn_random);
        assert_eq!(options.limit, WitnessLimit::Frames(100));

        options.limit = WitnessLimit::CompletedRounds(1);
        options
            .validate()
            .expect("Locked Witness supports reconstructed fights");
    }

    #[test]
    fn frame_limits_include_the_baseline() {
        assert!(limit_reached(WitnessLimit::Frames(0), 0, 0, 0));
        assert!(!limit_reached(WitnessLimit::Frames(1), 0, 0, 0));
        assert!(limit_reached(WitnessLimit::Frames(1), 1, 0, 0));
        assert!(!limit_reached(WitnessLimit::CompletedRounds(2), 99, 5, 6));
        assert!(limit_reached(WitnessLimit::CompletedRounds(2), 100, 5, 7));
    }

    #[test]
    fn reconstructed_fights_do_not_move_the_completed_round_target() {
        let mut run = WitnessRun::new(
            WitnessOptions::default(),
            ReproManifest {
                case_id: String::new(),
                scenario_seed: None,
                script_id: String::new(),
                build_id: String::new(),
                expanded_setup: Default::default(),
            },
        );
        assert!(run.activate(5));
        assert!(!run.activate(6));
        assert_eq!(run.initial_rounds, 5);
    }

    #[test]
    fn completed_round_boundary_skips_transport_state_once() {
        let mut run = WitnessRun::new(
            WitnessOptions::default(),
            ReproManifest {
                case_id: String::new(),
                scenario_seed: None,
                script_id: String::new(),
                build_id: String::new(),
                expanded_setup: Default::default(),
            },
        );
        assert!(run.activate(5));
        assert!(!run.note_completed_rounds(5));
        assert!(run.note_completed_rounds(6));
        assert!(run.round_boundary);
        assert!(run.take_dancer_clock_reset());
        assert!(!run.take_dancer_clock_reset());
        assert!(!run.note_completed_rounds(6));
    }

    #[test]
    fn positive_level_end_countdown_is_transport_only() {
        let mut timing = rsvz::core::model::WaveTimingSnapshot::minimal(100, rsvz::core::model::Wave(20));
        assert!(!is_level_end_transport(timing.level_end_countdown));
        timing.level_end_countdown = Some(500);
        assert!(is_level_end_transport(timing.level_end_countdown));
        timing.level_end_countdown = Some(0);
        assert!(!is_level_end_transport(timing.level_end_countdown));
    }
}
