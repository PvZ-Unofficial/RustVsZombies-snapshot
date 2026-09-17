//! Current event instance, independent of game lifecycle policy.

use super::EventDispatcher;
use std::cell::RefCell;

thread_local! {
    static EVENTS: RefCell<EventDispatcher> = RefCell::new(EventDispatcher::new());
}

pub fn with_events<R>(f: impl FnOnce(&mut EventDispatcher) -> R) -> R {
    EVENTS.with_borrow_mut(f)
}

#[doc(hidden)]
pub fn with_events_ref<R>(f: impl FnOnce(&EventDispatcher) -> R) -> R {
    EVENTS.with_borrow(f)
}
