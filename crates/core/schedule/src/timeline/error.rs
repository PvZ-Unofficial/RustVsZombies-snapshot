use crate::model::{RelativeTime, Wave, WaveTimingError};
use rsvz_backend_api::error::RuntimeError;

/// A configured wavelength disagreed with observed refresh clocks.
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq, Hash)]
#[error("assumed wavelength mismatch for {wave:?}: assumed {assumed}, actual {actual}")]
pub struct TimelineAssumptionMismatch {
    pub wave: Wave,
    pub assumed: i32,
    pub actual: i32,
}

/// A configured wavelength reached its refresh confirmation deadline without the next wave being
/// observed or exposed by a trusted countdown.
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq, Hash)]
#[error(
    "assumed wavelength refresh delay for {wave:?}: assumed {assumed}, expected next refresh at {expected_next_refresh}, deadline {deadline_clock}, current clock {current_clock}"
)]
pub struct TimelineRefreshDelay {
    pub wave: Wave,
    pub assumed: i32,
    pub expected_next_refresh: i32,
    pub deadline_clock: i32,
    pub current_clock: i32,
}

/// Dispatch-time wavelength timing violation.
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq, Hash)]
pub enum TimelineTimingViolation {
    /// The next wave was observed, but the actual wavelength differed from the declaration.
    #[error("{0}")]
    WavelengthMismatch(#[from] TimelineAssumptionMismatch),
    /// The AvZ-style `next wave -200` confirmation deadline was reached without a confirmed
    /// next-wave refresh.
    #[error("{0}")]
    RefreshDelay(TimelineRefreshDelay),
}

/// Non-fatal timing assumption warning recorded for AvZ-compatible declarations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TimelineAssumptionWarning {
    /// A wavelength was above the PvZ/AvZ recommended upper bound. AvZ warns and accepts it.
    WavelengthAboveRecommended {
        wave: Wave,
        length: i32,
        min: i32,
        max: i32,
    },
}

/// Error while registering assumed wave lengths.
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum TimelineAssumptionError {
    #[error(transparent)]
    Timing(#[from] WaveTimingError),
    #[error("{0}")]
    AssumptionMismatch(TimelineAssumptionMismatch),
    #[error(transparent)]
    ControlRegistration(#[from] TimelineRegistrationError),
}

/// Fatal dispatch-time timing error for an internal wave-refresh control.
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq, Hash)]
pub enum TimelineControlTimingError {
    /// The exact `(wave, 1)` control point was missed.
    #[error(
        "forced wave-refresh control for {target:?} is past due: due clock {due_clock:?}, current wave {current_wave:?}, current clock {current_clock}"
    )]
    PastDue {
        target: RelativeTime,
        due_clock: Option<i32>,
        current_wave: Wave,
        current_clock: i32,
    },
}

/// Error while registering Timeline work.
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq, Hash)]
pub enum TimelineRegistrationError {
    /// Core Timeline accepts wave 0, but negative waves are never valid.
    #[error("invalid timeline wave {wave}: wave must be non-negative")]
    InvalidWave { wave: i32 },
    /// Backend total waves are known and the target is beyond the post-final pseudo-wave.
    #[error("timeline wave {wave} exceeds backend total waves {total_waves} plus post-final pseudo-wave")]
    WaveOutOfRange { wave: i32, total_waves: i32 },
    /// The target is before the established late-attach floor.
    #[error("timeline target {target:?} is before late-attach floor {floor:?}")]
    BeforeStartFloor { target: RelativeTime, floor: RelativeTime },
    /// The target absolute clock is known and already past.
    #[error("timeline target {target:?} is past due: due clock {due_clock}, current clock {current_clock}")]
    PastDue {
        target: RelativeTime,
        due_clock: i32,
        current_clock: i32,
    },
    /// Negative delay registration is invalid.
    #[error("invalid timeline delay {frames}: delay must be non-negative")]
    InvalidDelay { frames: i32 },
}

/// Result of a panic-catching Timeline dispatch.
#[derive(Debug, PartialEq, Eq)]
pub enum TimelineDispatchResult {
    /// Dispatch completed without callback errors or panics.
    Continue,
    /// A callback returned an error.
    OperationError(RuntimeError),
    /// A callback panicked.
    OperationPanic,
    /// Backend timing snapshot acquisition failed.
    BackendError(RuntimeError),
    /// A forced wave-refresh control was rejected by the backend.
    BackendControlError(RuntimeError),
    /// A forced wave-refresh control missed its exact execution point.
    ControlTimingError(TimelineControlTimingError),
    /// A configured wavelength timing contract was violated.
    TimingViolation(TimelineTimingViolation),
}

/// Internal state returned after preparing a backend-free Timeline dispatch.
#[doc(hidden)]
#[derive(Debug, PartialEq, Eq)]
pub enum RuntimeTimelineDispatchStart {
    /// Dispatch completed before any callback needed to run.
    Finished(TimelineDispatchResult),
    /// Due callbacks should be drained for the supplied clock.
    Drain { clock: i32 },
}
