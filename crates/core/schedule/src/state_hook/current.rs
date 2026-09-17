//! Current state_hook instance, independent of game lifecycle policy.

use super::StateHookRegistry;
use rsvz_backend_api::error::RuntimeError;
use std::cell::RefCell;

thread_local! {
    static STATE_HOOKS: RefCell<StateHookRegistry<RuntimeError>> = RefCell::new(StateHookRegistry::new());
}

pub fn with_state_hooks<R>(f: impl FnOnce(&mut StateHookRegistry<RuntimeError>) -> R) -> R {
    STATE_HOOKS.with_borrow_mut(f)
}

#[doc(hidden)]
pub fn with_state_hooks_ref<R>(f: impl FnOnce(&StateHookRegistry<RuntimeError>) -> R) -> R {
    STATE_HOOKS.with_borrow(f)
}
