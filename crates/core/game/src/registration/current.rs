//! registration operations for the selected backend.

use crate::frame::FRAME;
use crate::lifecycle::{RuntimeLifecycle, close_attempt, lifecycle_error};
use crate::runtime::{RuntimeError, RuntimeFrameState};
use crate::session::{
    ARTIFACT, ArtifactSlot, DispatchOutcome, DispatchOutcomeState, RESET_STATE, SESSION_JOB, SessionControl,
    SessionJobCheckpoint, SessionJobState, WorldResetState,
};
use crate::setup::{AUTO_ENTER, reset_script_setup};
use crate::state_hook::{dispatch_state_event, hook_error, rollback_state_hooks_to, state_hook_checkpoint};
use rsvz_schedule::event::{EventCheckpoint, EventDispatcher, with_events};
use rsvz_schedule::tick::{
    TickLifetime, TickRegistrationCheckpoint, TickScheduler, with_scheduler, with_scheduler_ref,
};
use rsvz_schedule::timeline::{Timeline, with_timeline};

/// Opaque transaction for one script generation.
#[derive(Clone, Copy, Debug)]
pub struct ScriptGeneration {
    epoch: u64,
    hook_checkpoint: u64,
    event_checkpoint: EventCheckpoint,
    runtime_checkpoint: ScriptRuntimeCheckpoint,
}

#[derive(Clone, Copy, Debug)]
struct ScriptRuntimeCheckpoint {
    tick: TickRegistrationCheckpoint,
    session_job: SessionJobCheckpoint,
    stop_requested: bool,
    reset_was_pending: bool,
    artifact_was_present: bool,
    dispatch_outcome: Option<DispatchOutcome>,
    auto_enter: bool,
}

fn clear_script_runtime() {
    with_timeline(Timeline::clear_all);
    with_scheduler(|scheduler| {
        scheduler.clear_lifetime(TickLifetime::Fight);
        scheduler.clear_lifetime(TickLifetime::Script);
    });
    FRAME.with_borrow_mut(|frame| *frame = RuntimeFrameState::default());
    crate::diagnostics::clear_report_time();
}

fn script_runtime_checkpoint() -> ScriptRuntimeCheckpoint {
    ScriptRuntimeCheckpoint {
        tick: with_scheduler_ref(TickScheduler::checkpoint),
        session_job: SESSION_JOB.with_borrow(SessionJobState::checkpoint),
        stop_requested: crate::session::with_session_control_ref(SessionControl::stop_requested),
        reset_was_pending: RESET_STATE.with_borrow(WorldResetState::pending),
        artifact_was_present: ARTIFACT.with_borrow(ArtifactSlot::is_present),
        dispatch_outcome: crate::session::with_dispatch_outcome_ref(DispatchOutcomeState::get),
        auto_enter: AUTO_ENTER.get(),
    }
}

fn rollback_script_runtime(checkpoint: ScriptRuntimeCheckpoint) {
    with_scheduler(|scheduler| scheduler.rollback_to(checkpoint.tick));
    SESSION_JOB.with_borrow_mut(|state| state.restore(checkpoint.session_job));
    crate::session::with_session_control(|state| state.set_stop_requested(checkpoint.stop_requested));
    if !checkpoint.reset_was_pending {
        RESET_STATE.with_borrow_mut(WorldResetState::clear_pending);
    }
    if !checkpoint.artifact_was_present {
        ARTIFACT.with_borrow_mut(ArtifactSlot::clear);
    }
    crate::session::with_dispatch_outcome(|state| state.set(checkpoint.dispatch_outcome));
    AUTO_ENTER.set(checkpoint.auto_enter);
}

fn rollback_script_registration(generation: ScriptGeneration) {
    rollback_state_hooks_to(generation.hook_checkpoint);
    with_events(|events| events.rollback_to(generation.event_checkpoint));
    rollback_script_runtime(generation.runtime_checkpoint);
}

pub fn begin_script_generation() -> Result<ScriptGeneration, RuntimeError>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::NativeEventBackend,
{
    close_attempt()?;
    clear_script_runtime();
    reset_script_setup();
    let hook_checkpoint = state_hook_checkpoint();
    let event_checkpoint = with_events(EventDispatcher::begin_script_generation);
    let runtime_checkpoint = script_runtime_checkpoint();
    let (epoch, event) =
        crate::lifecycle::with_lifecycle(RuntimeLifecycle::begin_generation).map_err(lifecycle_error)?;
    if let Err(error) = hook_error(dispatch_state_event(event)) {
        abort_script_generation(ScriptGeneration {
            epoch,
            hook_checkpoint,
            event_checkpoint,
            runtime_checkpoint,
        })?;
        return Err(error);
    }
    Ok(ScriptGeneration {
        epoch,
        hook_checkpoint,
        event_checkpoint,
        runtime_checkpoint,
    })
}

pub fn finish_script_generation(generation: ScriptGeneration) -> Result<(), RuntimeError> {
    let event = crate::lifecycle::with_lifecycle(|lifecycle| lifecycle.after_script(generation.epoch))
        .map_err(lifecycle_error)?;
    if let Err(error) = hook_error(dispatch_state_event(event)) {
        abort_script_generation(generation)?;
        return Err(error);
    }
    if let Err(error) = with_events(EventDispatcher::freeze) {
        abort_script_generation(generation)?;
        return Err(RuntimeError::new(error.to_string()));
    }
    crate::lifecycle::with_lifecycle(|lifecycle| lifecycle.activate_generation(generation.epoch))
        .map_err(lifecycle_error)
}

pub fn abort_script_generation(generation: ScriptGeneration) -> Result<(), RuntimeError> {
    rollback_script_registration(generation);
    crate::lifecycle::with_lifecycle(|lifecycle| lifecycle.abort_generation(generation.epoch))
        .map_err(lifecycle_error)?;
    clear_script_runtime();
    Ok(())
}
