use std::panic::{self, AssertUnwindSafe};

use rsvz_model::{GameUi, ReloadBoundary};
use rsvz_schedule::tick::TickPhase;

use crate::runtime::{RuntimeError, RuntimeResult};

type Input = rsvz_current::DispatchInput;
type Output = rsvz_current::DispatchResult;

#[doc(hidden)]
pub fn run(
    backend: &mut rsvz_current::CurrentBackend, input: Input, script: fn() -> RuntimeResult<()>,
    install_user_hooks: fn() -> RuntimeResult<()>,
) -> Output
where
    rsvz_current::CurrentBackend: rsvz_backend_api::BoardReadinessBackend
        + rsvz_backend_api::GameUiBackend
        + rsvz_backend_api::WaveTimingBackend
        + crate::lineup::LineupApplyBackend
        + rsvz_backend_api::BattleEntryBackend
        + rsvz_backend_api::CardAppendSelectionBackend
        + rsvz_backend_api::CardSelectionReadBackend
        + rsvz_backend_api::GameSpeedHintBackend
        + rsvz_backend_api::NativeEventBackend
        + rsvz_backend_api::SeedChooserFastForwardBackend
        + rsvz_backend_api::SpawnScheduleBackend,
{
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        rsvz_current::scope_backend(backend, || {
            match panic::catch_unwind(AssertUnwindSafe(|| dispatch(input, script, install_user_hooks))) {
                Ok(output) => output,
                Err(payload) if rsvz_schedule::timeline::is_timeline_termination(&*payload) => {
                    let _abort = crate::lifecycle::abort_logic_tick();
                    stop(None, false)
                }
                Err(payload) => match rsvz_schedule::callback::into_error(payload) {
                    Ok(error) => {
                        // An operation outside user callbacks failed during physical
                        // preparation/sampling. Keep its error and normal teardown.
                        let _abort = crate::lifecycle::abort_logic_tick();
                        stop(Some(error), false)
                    }
                    Err(payload) => panic::resume_unwind(payload),
                },
            }
        })
    }));
    result.unwrap_or_else(|_panic| Output::Stop {
        artifact: None,
        error: Some(RuntimeError::new("runtime dispatch panicked")),
    })
}

fn dispatch(input: Input, script: fn() -> RuntimeResult<()>, install_user_hooks: fn() -> RuntimeResult<()>) -> Output
where
    rsvz_current::CurrentBackend: rsvz_backend_api::BoardReadinessBackend
        + rsvz_backend_api::GameUiBackend
        + rsvz_backend_api::WaveTimingBackend
        + crate::lineup::LineupApplyBackend
        + rsvz_backend_api::BattleEntryBackend
        + rsvz_backend_api::CardAppendSelectionBackend
        + rsvz_backend_api::CardSelectionReadBackend
        + rsvz_backend_api::GameSpeedHintBackend
        + rsvz_backend_api::NativeEventBackend
        + rsvz_backend_api::SeedChooserFastForwardBackend
        + rsvz_backend_api::SpawnScheduleBackend,
{
    let mut state = crate::frame::runtime_dispatch_state();
    if !state.started {
        if let Err(error) = initialize(input.session_shard(), install_user_hooks) {
            return stop(Some(error), false);
        }
        state = crate::frame::runtime_dispatch_state();
        state.started = true;
        crate::frame::set_runtime_dispatch_state(state);
    }
    if input.stop_requested {
        // A host limit can arrive together with the final completed round. Settle that
        // round before teardown, without starting another opening or native update.
        if input.completed_rounds() != 0 && !input.world_replaced() && state.registered && state.opening_applied {
            crate::session::record_completed_rounds(input.completed_rounds());
            let ui = match crate::frame::sample_current_frame() {
                Ok(frame) => frame.game_ui(),
                Err(error) => return stop(Some(error), false),
            };
            if let Err(error) = dispatch_tick(&crate::frame::sample_round_completion(ui)) {
                return stop(Some(error), false);
            }
        }
        return stop(None, false);
    }
    if crate::session::script_stop_requested() {
        return stop(None, true);
    }
    crate::session::record_completed_rounds(input.completed_rounds());
    let completed_round_boundary = input.completed_rounds() != 0 && !input.world_replaced();
    if input.world_replaced() {
        if let Err(error) = crate::lifecycle::close_attempt() {
            return stop(Some(error), false);
        }
        crate::session::mark_world_replaced();
        state.registered = false;
        state.opening_prepared = false;
        state.opening_applied = false;
        state.post_update_dispatched = false;
        crate::frame::set_runtime_dispatch_state(state);
    }

    let frame = match crate::frame::sample_current_frame() {
        Ok(frame) => frame,
        Err(error) => return stop(Some(error), false),
    };
    let ui = frame.game_ui();
    let previous_ui = state.last_ui;
    let boundary = crate::setup::classify_reload_boundary(previous_ui, ui.unwrap_or(GameUi::Loading))
        .or_else(|| completed_round_boundary.then_some(ReloadBoundary::FightUi));
    state.retain_current_ui_dispatch(ui);
    let deferred_boundary = if completed_round_boundary && state.registered && state.opening_applied {
        boundary
    } else {
        None
    };
    if state.registered
        && let Some(boundary) = boundary
        && deferred_boundary.is_none()
        && !state.prepare_reload(crate::setup::with_script_setup(|setup| setup.reload_mode), boundary)
        && state.ever_played
    {
        crate::frame::set_runtime_dispatch_state(state);
        return stop(None, true);
    }
    crate::frame::set_runtime_dispatch_state(state);

    if deferred_boundary.is_some() {
        let completion = crate::frame::sample_round_completion(ui);
        if let Err(error) = dispatch_tick(&completion) {
            return stop(Some(error), false);
        }
        return after_dispatch_tick(&mut state, deferred_boundary).unwrap_or(Output::Continue);
    }

    if frame.phase() == TickPhase::Finished {
        if !state.registered {
            return stop(None, true);
        }
        let wait_for_main_ui = crate::session::waits_for_main_ui_reload(
            crate::setup::with_script_setup(|setup| setup.reload_mode),
            input.main_ui_reload_supported(),
        );
        if state.post_update_dispatched {
            return if wait_for_main_ui {
                Output::Continue
            } else {
                stop(crate::session::take_fatal_session_error(), true)
            };
        }
        if let Err(error) = dispatch_tick(&frame) {
            return stop(Some(error), false);
        }
        if let Some(output) = after_dispatch_tick(&mut state, None) {
            return output;
        }
        if wait_for_main_ui {
            state.post_update_dispatched = true;
            crate::frame::set_runtime_dispatch_state(state);
            return Output::Continue;
        }
        return stop(crate::session::take_fatal_session_error(), true);
    }
    if !matches!(frame.phase(), TickPhase::LevelIntro | TickPhase::Playing) {
        return inactive_frame_result(frame.game_ui());
    }

    let mut opening_basis_changed = false;
    if !state.registered {
        if let Some(total_waves) = frame.snapshot().and_then(|snapshot| snapshot.total_waves) {
            crate::timeline::prime_runtime_total_waves(total_waves);
        }
        if let Err(error) = register(script) {
            return stop(Some(error), false);
        }
        state.registered = true;
        state.opening_prepared = false;
        state.opening_applied = false;
        crate::frame::set_runtime_dispatch_state(state);
        opening_basis_changed = true;
    }

    if crate::session::script_stop_requested() {
        return stop(None, true);
    }
    if crate::session::world_reset_pending() {
        if let Err(error) = crate::lifecycle::close_attempt() {
            return stop(Some(error), false);
        }
        let reset = rsvz_current::with_backend(crate::session::execute_pending_reset);
        if let Err(error) = reset {
            return stop(Some(error), false);
        }
        state.registered = false;
        state.opening_prepared = false;
        state.opening_applied = false;
        state.post_update_dispatched = false;
        crate::frame::set_runtime_dispatch_state(state);
        return Output::SkipUpdate;
    }
    if opening_basis_changed && input.registration_safe_point_required() {
        return Output::SkipUpdate;
    }

    if !state.opening_prepared {
        if let Err(error) = prepare_opening() {
            return stop(Some(error), false);
        }
        state.opening_prepared = true;
        crate::frame::set_runtime_dispatch_state(state);
        opening_basis_changed = true;
    }

    if !input.opening_ready() {
        return opening_wait_output(opening_basis_changed);
    }

    if !state.opening_applied {
        if let Err(error) = finish_opening(frame.phase()) {
            return stop(Some(error), false);
        }
        state.opening_applied = true;
        crate::frame::set_runtime_dispatch_state(state);
        return if crate::session::script_stop_requested() {
            stop(None, true)
        } else if input.opening_transition_requires_native_update() && frame.phase() == TickPhase::LevelIntro {
            Output::Continue
        } else {
            Output::SkipUpdate
        };
    }

    if frame.phase() == TickPhase::LevelIntro {
        return Output::SkipUpdate;
    }
    if let Err(error) = crate::lifecycle::enter_fight() {
        return stop(Some(error), false);
    }
    if crate::session::script_stop_requested() {
        return stop(None, true);
    }
    if state.post_update_dispatched {
        state.post_update_dispatched = false;
        crate::frame::set_runtime_dispatch_state(state);
        return Output::Continue;
    }
    if let Err(error) = dispatch_tick(&frame) {
        return stop(Some(error), false);
    }
    if let Some(output) = after_dispatch_tick(&mut state, None) {
        return output;
    }
    Output::Continue
}

fn opening_wait_output(opening_basis_changed: bool) -> Output {
    if opening_basis_changed {
        Output::SkipUpdate
    } else {
        Output::Continue
    }
}

fn after_dispatch_tick(
    state: &mut crate::session::RuntimeDispatchState, deferred_boundary: Option<ReloadBoundary>,
) -> Option<Output>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::NativeEventBackend,
{
    if crate::session::script_stop_requested() {
        return Some(stop(None, true));
    }
    if crate::session::world_reset_pending() {
        if let Err(error) = crate::lifecycle::close_attempt() {
            return Some(stop(Some(error), false));
        }
        let reset = rsvz_current::with_backend(crate::session::execute_pending_reset);
        if let Err(error) = reset {
            return Some(stop(Some(error), false));
        }
        state.registered = false;
        state.opening_prepared = false;
        state.opening_applied = false;
        state.post_update_dispatched = false;
        crate::frame::set_runtime_dispatch_state(*state);
        return Some(Output::SkipUpdate);
    }
    let boundary = deferred_boundary?;
    let reload = state.prepare_reload(crate::setup::with_script_setup(|setup| setup.reload_mode), boundary);
    state.post_update_dispatched = reload;
    crate::frame::set_runtime_dispatch_state(*state);
    if reload {
        Some(Output::SkipUpdate)
    } else if state.ever_played {
        Some(stop(None, true))
    } else {
        None
    }
}

fn inactive_frame_result(game_ui: Option<GameUi>) -> Output {
    if !crate::setup::auto_enter_enabled()
        && matches!(game_ui, Some(GameUi::Loading | GameUi::Menu | GameUi::Challenge))
    {
        Output::Continue
    } else {
        Output::SkipUpdate
    }
}

fn dispatch_tick(frame: &crate::runtime::CurrentFrameSample) -> RuntimeResult<()>
where
    rsvz_current::CurrentBackend:
        rsvz_backend_api::BoardReadinessBackend + rsvz_backend_api::GameUiBackend + rsvz_backend_api::WaveTimingBackend,
{
    crate::lifecycle::begin_logic_tick()?;
    if crate::session::script_stop_requested() {
        return crate::lifecycle::abort_logic_tick();
    }
    let reset_pending_before_frame = crate::session::world_reset_pending();
    let result = crate::frame::dispatch_current_frame_sample(frame, crate::diagnostics::report_runtime_error);
    if matches!(
        result,
        crate::runtime::RuntimeFrameDispatch::Continue | crate::runtime::RuntimeFrameDispatch::OperationError(_)
    ) || (matches!(
        result,
        crate::runtime::RuntimeFrameDispatch::TimingViolation(_)
            | crate::runtime::RuntimeFrameDispatch::TimingControlViolation(_)
    ) && (crate::session::script_stop_requested()
        || (!reset_pending_before_frame && crate::session::world_reset_pending())))
    {
        crate::lifecycle::finish_logic_tick()
    } else {
        let _abort_result = crate::lifecycle::abort_logic_tick();
        Err(dispatch_error(result))
    }
}

fn initialize(shard: rsvz_model::SessionShard, install_user_hooks: fn() -> RuntimeResult<()>) -> RuntimeResult<()> {
    crate::frame::reset_runtime_state_preserving_backend();
    crate::state_hook::clear_state_hooks();
    crate::lifecycle::reset_session_resources();
    crate::session::reset_session_control();
    crate::session::install_session_shard(shard)?;

    let checkpoint = crate::state_hook::state_hook_checkpoint();
    let installed = panic::catch_unwind(AssertUnwindSafe(install_user_hooks))
        .map_err(|payload| {
            rsvz_schedule::callback::into_error(payload)
                .unwrap_or_else(|_| RuntimeError::new("state-hook installer panicked"))
        })
        .and_then(|result| result);
    if let Err(error) = installed {
        crate::state_hook::rollback_state_hooks_to(checkpoint);
        return Err(error);
    }
    crate::lifecycle::finish_hook_installation()
}

fn register(script: fn() -> RuntimeResult<()>) -> RuntimeResult<()>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::NativeEventBackend,
{
    let generation = crate::registration::begin_script_generation()?;
    let registration = panic::catch_unwind(AssertUnwindSafe(script))
        .map_err(|payload| {
            rsvz_schedule::callback::into_error(payload)
                .unwrap_or_else(|_| RuntimeError::new("user script panicked during registration"))
        })
        .and_then(|result| result);
    if let Err(error) = registration {
        crate::registration::abort_script_generation(generation)?;
        return Err(error);
    }
    crate::registration::finish_script_generation(generation)
}

fn prepare_opening() -> RuntimeResult<()>
where
    rsvz_current::CurrentBackend: crate::lineup::LineupApplyBackend
        + rsvz_backend_api::GameSpeedHintBackend
        + rsvz_backend_api::SeedChooserFastForwardBackend
        + rsvz_backend_api::SpawnScheduleBackend,
{
    let shard = crate::session::session_shard();
    let fallback = u64::from(shard.seed_base).wrapping_add(u64::from(shard.index));
    let seed = crate::session::take_next_opening_seed(fallback);
    crate::setup::prepare_current_opening(seed)
}

fn finish_opening(phase: TickPhase) -> RuntimeResult<()>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::BattleEntryBackend
        + rsvz_backend_api::CardAppendSelectionBackend
        + rsvz_backend_api::CardSelectionReadBackend
        + rsvz_backend_api::NativeEventBackend,
{
    crate::setup::finish_current_opening()?;
    if phase == TickPhase::LevelIntro {
        use rsvz_backend_api::backend::BattleEntryBackend;
        rsvz_current::with_backend(BattleEntryBackend::start_battle)
            .map_err(|error| RuntimeError::new(error.to_string()))?;
    }
    if phase == TickPhase::LevelIntro {
        crate::lifecycle::enter_chooser()?;
    } else {
        crate::lifecycle::enter_fight()?;
    }
    Ok(())
}

fn stop(error: Option<RuntimeError>, publish_artifact: bool) -> Output
where
    rsvz_current::CurrentBackend: rsvz_backend_api::NativeEventBackend,
{
    let initial_error = crate::session::take_fatal_session_error().or(error);
    let finalize_error = crate::lifecycle::finalize_session().err();
    let late_fatal = crate::session::take_fatal_session_error();
    let error = initial_error.or(late_fatal).or(finalize_error);
    let artifact = if publish_artifact && error.is_none() {
        crate::session::take_session_artifact()
    } else {
        None
    };
    Output::Stop { artifact, error }
}

fn dispatch_error(result: crate::runtime::RuntimeFrameDispatch) -> RuntimeError {
    match result {
        crate::runtime::RuntimeFrameDispatch::OperationError(error)
        | crate::runtime::RuntimeFrameDispatch::TimingBackendError(error)
        | crate::runtime::RuntimeFrameDispatch::TimingControlError(error)
        | crate::runtime::RuntimeFrameDispatch::FrameCallbackError(error) => error,
        other => RuntimeError::new(format!("runtime frame dispatch failed: {other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsvz_current::DispatchResult;
    #[cfg(feature = "backend-tests")]
    use rsvz_schedule::state_hook::StateEvent;
    #[test]
    fn opening_wait_skips_only_the_dispatch_that_changed_its_basis() {
        assert!(matches!(opening_wait_output(true), DispatchResult::SkipUpdate));
        assert!(matches!(opening_wait_output(false), DispatchResult::Continue));
    }
    #[test]
    fn manual_entry_keeps_native_menu_updates_running() {
        crate::session::reset_session_control();
        assert!(matches!(
            inactive_frame_result(Some(GameUi::Menu)),
            DispatchResult::SkipUpdate
        ));

        crate::setup::set_auto_enter(false);
        assert!(matches!(
            inactive_frame_result(Some(GameUi::Menu)),
            DispatchResult::Continue
        ));
        assert!(matches!(
            inactive_frame_result(Some(GameUi::LevelIntro)),
            DispatchResult::SkipUpdate
        ));

        crate::session::reset_session_control();
        assert!(matches!(
            inactive_frame_result(Some(GameUi::Menu)),
            DispatchResult::SkipUpdate
        ));
    }
    #[cfg(feature = "backend-tests")]
    #[test]
    fn before_exit_fatal_prevents_artifact_publication() {
        fn merge_probe(target: &mut u8, source: u8) -> RuntimeResult<()> {
            *target = target.saturating_add(source);
            Ok(())
        }

        fn write_probe(value: &u8, writer: &mut dyn std::io::Write) -> std::io::Result<()> {
            write!(writer, "{value}")
        }

        crate::frame::reset_runtime_state_preserving_backend();
        crate::state_hook::clear_state_hooks();
        crate::lifecycle::reset_session_resources();
        crate::session::reset_session_control();
        rsvz_schedule::state_hook::with_state_hooks(|hooks| {
            hooks.register(StateEvent::BeforeExit, 0, || {
                crate::session::fail_script(RuntimeError::new("late fatal"));
                Ok(())
            });
            hooks.register(StateEvent::BeforeExit, 1, || {
                crate::session::set_session_artifact(crate::SessionArtifact::new(1_u8, merge_probe, write_probe))
            });
        });
        crate::lifecycle::finish_hook_installation().expect("install hooks");

        match stop(None, true) {
            DispatchResult::Stop {
                artifact: None,
                error: Some(error),
            } => assert_eq!(error.to_string(), "late fatal"),
            _ => panic!("a BeforeExit fatal must suppress the artifact"),
        }
        assert!(crate::session::take_fatal_session_error().is_none());
    }
    #[cfg(feature = "backend-tests")]
    #[test]
    fn before_exit_fatal_is_drained_behind_an_earlier_error() {
        crate::frame::reset_runtime_state_preserving_backend();
        crate::state_hook::clear_state_hooks();
        crate::lifecycle::reset_session_resources();
        crate::session::reset_session_control();
        rsvz_schedule::state_hook::with_state_hooks(|hooks| {
            hooks.register(StateEvent::BeforeExit, 0, || {
                crate::session::fail_script(RuntimeError::new("late fatal"));
                Ok(())
            });
        });
        crate::lifecycle::finish_hook_installation().expect("install hooks");

        match stop(Some(RuntimeError::new("primary")), false) {
            DispatchResult::Stop {
                artifact: None,
                error: Some(error),
            } => assert_eq!(error.to_string(), "primary"),
            _ => panic!("the earlier error must keep priority"),
        }
        assert!(crate::session::take_fatal_session_error().is_none());
    }
}
