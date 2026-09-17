//! lifecycle operations for the selected backend.

use crate::event::{EVENT_SINK_INSTALLED, dispatch_public_events, native_event_sink};
use crate::lifecycle::{LifecycleError, RuntimeLifecycle};
use crate::runtime::RuntimeError;
use crate::session::DispatchOutcomeState;
use crate::state_hook::{clear_state_hooks, dispatch_state_event, hook_error};
use rsvz_backend_api::backend::NativeEventBackend;
use rsvz_current::with_backend;
use rsvz_schedule::event::{EventDispatcher, with_events};
use rsvz_schedule::tick::{TickLifetime, TickScheduler, with_scheduler};
use rsvz_schedule::timeline::{Timeline, with_timeline};

// A terminal callback still ends its event immediately, but ExitFight/BeforeExit
// must finish their owned cleanup before returning the failure to dispatch.
fn dispatch_cleanup_event(event: rsvz_schedule::state_hook::StateEvent) -> Result<(), RuntimeError> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| hook_error(dispatch_state_event(event)))) {
        Ok(result) => result,
        Err(payload) if rsvz_schedule::timeline::is_timeline_termination(&*payload) => {
            Err(crate::session::fatal_session_error().expect("Timeline termination has a latched fatal error"))
        }
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

pub(crate) fn lifecycle_error(error: LifecycleError) -> RuntimeError {
    RuntimeError::new(error.to_string())
}

pub fn finish_hook_installation() -> Result<(), RuntimeError> {
    let event = crate::lifecycle::with_lifecycle(RuntimeLifecycle::finish_installation).map_err(lifecycle_error)?;
    hook_error(dispatch_state_event(event))
}

pub fn enter_chooser() -> Result<(), RuntimeError> {
    crate::lifecycle::with_lifecycle(RuntimeLifecycle::enter_chooser).map_err(lifecycle_error)
}

pub fn enter_fight() -> Result<(), RuntimeError>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::NativeEventBackend,
{
    let event = crate::lifecycle::with_lifecycle(RuntimeLifecycle::enter_fight).map_err(lifecycle_error)?;
    let Some(event) = event else {
        return Ok(());
    };
    hook_error(dispatch_state_event(event))?;
    let interest = with_events(|events| events.interest());
    if interest.is_empty() {
        return Ok(());
    }
    let sink = native_event_sink(interest);
    with_backend(|backend| backend.install_native_event_sink(sink))
        .map_err(|error| RuntimeError::new(error.to_string()))?;
    EVENT_SINK_INSTALLED.set(true);
    Ok(())
}

pub fn close_attempt() -> Result<(), RuntimeError>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::NativeEventBackend,
{
    close_attempt_with(remove_native_event_sink)
}

fn remove_native_event_sink() -> Result<(), RuntimeError>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::NativeEventBackend,
{
    with_backend(NativeEventBackend::remove_native_event_sink).map_err(|error| RuntimeError::new(error.to_string()))
}

fn close_attempt_with(remove_sink: impl FnOnce() -> Result<(), RuntimeError>) -> Result<(), RuntimeError> {
    let event = crate::lifecycle::with_lifecycle(RuntimeLifecycle::begin_attempt_close).map_err(lifecycle_error)?;
    let mut result = Ok(());
    if event.is_some() {
        with_events(EventDispatcher::close_fight);
        if let Some(fault) = with_events(EventDispatcher::take_fault) {
            result = Err(RuntimeError::new(fault.to_string()));
        }
    }
    if EVENT_SINK_INSTALLED.get() {
        match remove_sink() {
            Ok(()) => EVENT_SINK_INSTALLED.set(false),
            Err(error) if result.is_ok() => result = Err(error),
            Err(_) => {}
        }
    }
    if let Some(event) = event {
        if let Err(error) = dispatch_cleanup_event(event)
            && result.is_ok()
        {
            result = Err(error);
        }
        with_scheduler(|scheduler| scheduler.clear_lifetime(TickLifetime::Fight));
        crate::lifecycle::with_lifecycle(RuntimeLifecycle::finish_attempt_close).map_err(lifecycle_error)?;
    }
    result
}

pub fn begin_logic_tick() -> Result<(), RuntimeError> {
    let event = crate::lifecycle::with_lifecycle(RuntimeLifecycle::begin_tick).map_err(lifecycle_error)?;
    let (fault, overflowed, public_dispatch) = with_events(|events| {
        (
            events.take_fault(),
            events.take_public_overflow(),
            events.begin_public_dispatch(),
        )
    });
    if let Some(fault) = fault {
        crate::lifecycle::with_lifecycle(RuntimeLifecycle::abort_tick).map_err(lifecycle_error)?;
        return Err(RuntimeError::new(fault.to_string()));
    }
    dispatch_public_events(overflowed, public_dispatch);
    if let Err(error) = hook_error(dispatch_state_event(event)) {
        crate::lifecycle::with_lifecycle(RuntimeLifecycle::abort_tick).map_err(lifecycle_error)?;
        return Err(error);
    }
    Ok(())
}

pub fn finish_logic_tick() -> Result<(), RuntimeError> {
    let event = crate::lifecycle::with_lifecycle(RuntimeLifecycle::finish_tick).map_err(lifecycle_error)?;
    let result = hook_error(dispatch_state_event(event));
    crate::session::with_dispatch_outcome(DispatchOutcomeState::take);
    result
}

pub fn abort_logic_tick() -> Result<(), RuntimeError> {
    crate::lifecycle::with_lifecycle(RuntimeLifecycle::abort_tick).map_err(lifecycle_error)
}

pub fn finalize_session() -> Result<(), RuntimeError>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::NativeEventBackend,
{
    finalize_session_with(remove_native_event_sink)
}

fn finalize_session_with(remove_sink: impl FnOnce() -> Result<(), RuntimeError>) -> Result<(), RuntimeError> {
    let mut first_error = close_attempt_with(remove_sink).err();
    if EVENT_SINK_INSTALLED.get() {
        return Err(first_error.unwrap_or_else(|| RuntimeError::new("native event sink remains installed")));
    }
    let event = crate::lifecycle::with_lifecycle(RuntimeLifecycle::begin_finalize).map_err(lifecycle_error)?;
    if let Some(event) = event
        && let Err(error) = dispatch_cleanup_event(event)
        && first_error.is_none()
    {
        first_error = Some(error);
    }
    with_scheduler(TickScheduler::clear_all);
    with_timeline(Timeline::clear_all);
    with_events(EventDispatcher::clear_session);
    clear_state_hooks();
    crate::lifecycle::with_lifecycle(RuntimeLifecycle::finish_finalize).map_err(lifecycle_error)?;
    first_error.map_or(Ok(()), Err)
}

#[cfg(all(test, feature = "backend-tests"))]
mod tests {
    use super::*;
    use crate::lifecycle::{AttemptState, SessionState};
    use rsvz_schedule::state_hook::StateEvent;
    use rsvz_schedule::tick::{TickControl, TickOptions, TickTaskState};
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn failed_sink_removal_defers_session_cleanup_until_a_successful_retry() {
        crate::frame::reset_runtime_state_preserving_backend();
        crate::session::reset_session_control();
        crate::lifecycle::reset_session_resources();
        clear_state_hooks();
        let before_exit = Rc::new(Cell::new(false));
        let observed = Rc::clone(&before_exit);
        crate::state_hook::register_fallible(StateEvent::BeforeExit, 0, move || {
            observed.set(true);
            Ok(())
        });
        finish_hook_installation().unwrap();
        let generation = crate::registration::begin_script_generation().unwrap();
        crate::registration::finish_script_generation(generation).unwrap();
        enter_fight().unwrap();
        EVENT_SINK_INSTALLED.set(true);
        let session_task = with_scheduler(|scheduler| {
            scheduler.spawn(TickOptions::any_dispatch().lifetime(TickLifetime::Session), |_| {
                Ok(TickControl::Continue)
            })
        });
        let fight_task = with_scheduler(|scheduler| {
            scheduler.spawn(TickOptions::playing_frame().lifetime(TickLifetime::Fight), |_| {
                Ok(TickControl::Continue)
            })
        });
        let removals = Cell::new(0);
        let error = finalize_session_with(|| {
            removals.set(removals.get() + 1);
            Err(RuntimeError::new("native sink removal failed"))
        })
        .unwrap_err();
        assert_eq!(error.to_string(), "native sink removal failed");
        assert!(EVENT_SINK_INSTALLED.get());
        assert!(!before_exit.get());
        crate::lifecycle::with_lifecycle(|state| {
            assert_eq!(state.session(), SessionState::Running);
            assert_eq!(state.attempt(), AttemptState::Idle);
        });
        with_scheduler(|scheduler| {
            assert_eq!(scheduler.state(fight_task), TickTaskState::Stopped);
            assert_ne!(scheduler.state(session_task), TickTaskState::Stopped);
        });

        // An idle logical attempt still has a physical sink to remove.
        finalize_session_with(|| {
            removals.set(removals.get() + 1);
            Ok(())
        })
        .unwrap();
        assert_eq!(removals.get(), 2);
        assert!(!EVENT_SINK_INSTALLED.get());
        assert!(before_exit.get());
        crate::lifecycle::with_lifecycle(|state| assert_eq!(state.session(), SessionState::Done));
        with_scheduler(|scheduler| assert_eq!(scheduler.state(session_task), TickTaskState::Stopped));
    }
}
