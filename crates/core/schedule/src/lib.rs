#![feature(trivial_bounds)]
#![allow(incomplete_features, trivial_bounds)]

//! Backend-neutral event, timeline, and tick scheduling.

#[doc(hidden)]
pub mod callback;

pub mod event;
pub mod state_hook;

pub mod backend {
    pub use rsvz_backend_api::*;
}

pub mod model {
    pub use rsvz_model::*;
}

pub mod tick {
    mod current;
    pub use current::{with_scheduler, with_scheduler_ref};
    mod handle;
    mod options;
    mod scheduler;
    mod types;

    pub use handle::TickHandle;
    pub use options::TickOptions;
    pub use scheduler::{TickDispatch, TickDispatchResult, TickRegistrationCheckpoint, TickScheduler};
    pub use types::{
        TickAvailability, TickCommandOutcome, TickControl, TickFrameGate, TickLane, TickLifetime, TickMeta, TickPhase,
        TickPriority, TickPriorityError, TickTaskState, TickTrigger, TickWhen,
    };

    pub(crate) use handle::TickTaskKey;
}

pub mod timeline {
    mod current;
    pub use current::{with_timeline, with_timeline_ref};
    mod conversion;
    mod dispatch;
    mod error;
    mod op;
    mod queue;
    mod state;
    mod termination;
    #[cfg(test)]
    #[expect(
        clippy::expect_used,
        reason = "timeline tests use expect to make fixture failures explicit"
    )]
    mod tests;
    mod timing;
    #[doc(hidden)]
    pub use termination::{is_timeline_termination, resume_timeline_termination, terminate_timeline_callback};
    mod types;

    pub use conversion::{IntoAssumedWavelength, IntoRelativeTime};
    pub use error::{
        RuntimeTimelineDispatchStart, TimelineAssumptionError, TimelineAssumptionMismatch, TimelineAssumptionWarning,
        TimelineControlTimingError, TimelineDispatchResult, TimelineRefreshDelay, TimelineRegistrationError,
        TimelineTimingViolation,
    };
    #[doc(hidden)]
    pub use op::RuntimeReadyTimedOp;
    pub use state::Timeline;
    pub use timing::{current_wave_refresh_clock, next_wave_countdown, recommended_wavelength_bounds};
    pub use types::{
        TimeCommandOutcome, TimeHandle, TimeOpId, TimelineDiagnostics, TimelineRegistration, TimingViolationPolicy,
    };
}

pub use tick::*;
pub use timeline::*;
