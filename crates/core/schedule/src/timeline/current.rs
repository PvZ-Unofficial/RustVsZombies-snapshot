//! Current timeline instance, independent of game lifecycle policy.

use super::Timeline;
use std::cell::RefCell;

thread_local! {
    static TIMELINE: RefCell<Timeline> = RefCell::new(Timeline::new());
}

pub fn with_timeline<R>(f: impl FnOnce(&mut Timeline) -> R) -> R {
    TIMELINE.with_borrow_mut(f)
}

#[doc(hidden)]
pub fn with_timeline_ref<R>(f: impl FnOnce(&Timeline) -> R) -> R {
    TIMELINE.with_borrow(f)
}
