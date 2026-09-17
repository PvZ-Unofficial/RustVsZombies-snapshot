//! fast forward current state.

use rsvz_schedule::tick::TickHandle;
use std::cell::RefCell;

thread_local! {
    static FAST_FORWARD_TASK: RefCell<Option<TickHandle>> = const { RefCell::new(None) };
}

pub fn with_fast_forward_task<R>(f: impl FnOnce(&mut Option<TickHandle>) -> R) -> R {
    FAST_FORWARD_TASK.with_borrow_mut(f)
}

mod current;
pub use current::register_fast_forward_window_task;
