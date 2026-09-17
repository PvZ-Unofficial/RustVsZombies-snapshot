//! Keeps an Immediate Timeline failure terminal across nested callback adapters.

use std::any::Any;

struct TimelineTermination;

#[doc(hidden)]
pub fn is_timeline_termination(payload: &(dyn Any + Send)) -> bool {
    payload.is::<TimelineTermination>()
}

/// The game layer has already saved the first failure before unwinding.
#[doc(hidden)]
pub fn terminate_timeline_callback() -> ! {
    std::panic::resume_unwind(Box::new(TimelineTermination))
}

#[doc(hidden)]
pub fn resume_timeline_termination(payload: Box<dyn Any + Send>) {
    if is_timeline_termination(&*payload) {
        std::panic::resume_unwind(payload);
    }
}
