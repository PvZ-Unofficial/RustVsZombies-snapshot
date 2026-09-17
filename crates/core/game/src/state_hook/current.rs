//! state hook operations for the selected backend.

use crate::runtime::RuntimeError;
use crate::session::script_stop_requested;
use rsvz_schedule::state_hook::{
    StateEvent, StateHookDispatchError, StateHookDispatchResult, StateHookRegistry, with_state_hooks,
    with_state_hooks_ref,
};

#[doc(hidden)]
pub fn dispatch_state_event(event: StateEvent) -> StateHookDispatchResult<RuntimeError> {
    let drain_after_stop = matches!(event, StateEvent::AfterTick | StateEvent::BeforeExit);
    let mut dispatch = match with_state_hooks(|hooks| hooks.begin_dispatch(event)) {
        Ok(dispatch) => dispatch,
        Err(StateHookDispatchError::NestedDispatch) => return StateHookDispatchResult::NestedDispatch,
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        loop {
            let callback = with_state_hooks(|hooks| hooks.take_next(&mut dispatch));
            let Some(callback) = callback else {
                break StateHookDispatchResult::Continue;
            };
            let outcome = StateHookRegistry::<RuntimeError>::run_callback_catching(callback);
            match with_state_hooks(|hooks| hooks.finish_callback(outcome)) {
                StateHookDispatchResult::Continue if drain_after_stop || !script_stop_requested() => {}
                StateHookDispatchResult::HookAborted(error) => {
                    if crate::registration::is_active()
                        || super::INSTALLER_ACTIVE.get()
                        || matches!(event, StateEvent::BeforeScript | StateEvent::AfterScript)
                    {
                        break StateHookDispatchResult::HookError(error);
                    }
                    crate::diagnostics::report_operation_error(error);
                    if !drain_after_stop && script_stop_requested() {
                        break StateHookDispatchResult::Continue;
                    }
                }
                result => break result,
            }
        }
    }));
    with_state_hooks(|hooks| hooks.end_dispatch());
    result.unwrap_or_else(|payload| {
        rsvz_schedule::timeline::resume_timeline_termination(payload);
        StateHookDispatchResult::HookPanic
    })
}

#[doc(hidden)]
pub fn clear_state_hooks() {
    with_state_hooks(|hooks| hooks.clear());
}

pub fn state_hook_checkpoint() -> u64 {
    with_state_hooks_ref(|hooks| hooks.checkpoint())
}

pub fn rollback_state_hooks_to(checkpoint: u64) {
    with_state_hooks(|hooks| hooks.rollback_to(checkpoint));
}

pub(crate) fn hook_error(result: StateHookDispatchResult<RuntimeError>) -> Result<(), RuntimeError> {
    match result {
        StateHookDispatchResult::Continue => Ok(()),
        StateHookDispatchResult::HookError(error) | StateHookDispatchResult::HookAborted(error) => Err(error),
        StateHookDispatchResult::HookPanic => Err(RuntimeError::new("state-hook callback panicked")),
        StateHookDispatchResult::NestedDispatch => Err(RuntimeError::new("nested state-event dispatch is not allowed")),
    }
}

/// Registers a fallible callback on the existing lifecycle registry.
pub fn register_fallible<F>(
    event: StateEvent, order: i32, mut callback: F,
) -> rsvz_schedule::state_hook::StateHookHandle
where
    F: FnMut() -> crate::runtime::RuntimeResult<()> + 'static,
{
    with_state_hooks(|registry| registry.register(event, order, move || super::with_callback_context(&mut callback)))
}

use rsvz_schedule::state_hook::{StateHookCommandOutcome, StateHookHandle};
pub fn register_user_hook<F>(event: StateEvent, order: i32, mut callback: F) -> StateHookHandle
where
    F: FnMut() + 'static,
{
    let allowed = super::registration_allowed();
    if !allowed {
        let message = "session state hooks may only be registered by #[rsvz::state_hooks] or a state-hook callback";
        if crate::registration::is_active() {
            crate::registration::record_error(message);
        } else {
            panic!("{message}");
        }
    }
    let handle = with_state_hooks(|registry| {
        registry.register(event, order, move || {
            super::with_callback_context(|| {
                callback();
                Ok(())
            })
        })
    });
    if !allowed {
        let _outcome = with_state_hooks(|registry| registry.remove(handle));
    }
    handle
}

pub fn remove_state_hook(handle: StateHookHandle) -> StateHookCommandOutcome {
    with_state_hooks(|registry| registry.remove(handle))
}
