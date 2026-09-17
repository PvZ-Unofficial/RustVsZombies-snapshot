//! Lifecycle hook context and current dispatch.

use crate::runtime::RuntimeResult;
use std::cell::Cell;

thread_local! {
    static INSTALLER_ACTIVE: Cell<bool> = const { Cell::new(false) };
    static CALLBACK_ACTIVE: Cell<bool> = const { Cell::new(false) };
}

struct ContextGuard {
    cell: &'static std::thread::LocalKey<Cell<bool>>,
    previous: bool,
}

impl ContextGuard {
    fn enter(cell: &'static std::thread::LocalKey<Cell<bool>>) -> Self {
        let previous = cell.replace(true);
        Self { cell, previous }
    }
}

impl Drop for ContextGuard {
    fn drop(&mut self) {
        self.cell.set(self.previous);
    }
}

#[doc(hidden)]
pub fn registration_allowed() -> bool {
    INSTALLER_ACTIVE.get() || CALLBACK_ACTIVE.get()
}

#[doc(hidden)]
pub fn run_installer(body: impl FnOnce() -> RuntimeResult<()>) -> RuntimeResult<()> {
    let _guard = ContextGuard::enter(&INSTALLER_ACTIVE);
    body()
}

#[doc(hidden)]
pub fn with_callback_context<R>(body: impl FnOnce() -> R) -> R {
    let _guard = ContextGuard::enter(&CALLBACK_ACTIVE);
    body()
}

mod current;
pub(crate) use current::hook_error;
pub use current::{
    clear_state_hooks, dispatch_state_event, register_fallible, register_user_hook, remove_state_hook,
    rollback_state_hooks_to, state_hook_checkpoint,
};
