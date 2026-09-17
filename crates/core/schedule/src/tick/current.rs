//! Current tick instance, independent of game lifecycle policy.

use super::TickScheduler;
use std::cell::RefCell;

thread_local! {
    static SCHEDULER: RefCell<TickScheduler> = RefCell::new(TickScheduler::new());
}

pub fn with_scheduler<R>(f: impl FnOnce(&mut TickScheduler) -> R) -> R {
    SCHEDULER.with_borrow_mut(f)
}

#[doc(hidden)]
pub fn with_scheduler_ref<R>(f: impl FnOnce(&TickScheduler) -> R) -> R {
    SCHEDULER.with_borrow(f)
}
