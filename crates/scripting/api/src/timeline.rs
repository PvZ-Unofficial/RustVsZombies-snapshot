//! Current timeline registration and handle types.

pub use rsvz_game::timeline::{at, at_frame, try_at, try_at_frame};
pub use rsvz_schedule::timeline::{
    TimeCommandOutcome, TimeHandle, TimeOpId, TimelineDiagnostics, TimelineDispatchResult, TimelineRegistration,
    TimingViolationPolicy,
};
