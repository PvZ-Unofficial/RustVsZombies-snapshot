use std::num::NonZeroU64;

use crate::model::RelativeTime;

use super::RuntimeReadyTimedOp;
use super::{TimelineAssumptionWarning, TimelineTimingViolation};

/// Stable identifier for a Timeline operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TimeOpId(NonZeroU64);

impl TimeOpId {
    pub(super) const fn new(raw: NonZeroU64) -> Self {
        Self(raw)
    }

    /// Returns the raw non-zero identifier.
    #[must_use]
    pub const fn raw(self) -> NonZeroU64 {
        self.0
    }
}

/// Detached handle for cancelling a Timeline operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TimeHandle {
    id: TimeOpId,
}

impl TimeHandle {
    pub(super) const fn new(id: TimeOpId) -> Self {
        Self { id }
    }

    /// Returns the operation identifier.
    #[must_use]
    pub const fn id(self) -> TimeOpId {
        self.id
    }
}

/// Result of registering Timeline work.
#[must_use = "queued registrations should keep or explicitly ignore the handle; immediate registrations must be executed"]
pub enum TimelineRegistration {
    /// Work was queued for future Timeline dispatch.
    Queued(TimeHandle),
    /// Work is due at the current known clock and must run after Timeline borrows are released.
    Immediate {
        /// Completed handle for the immediate operation.
        handle: TimeHandle,
        /// Callback payload to execute outside Timeline storage borrows.
        op: RuntimeReadyTimedOp,
    },
}

impl TimelineRegistration {
    /// Returns the operation handle regardless of registration outcome.
    #[must_use]
    pub fn handle(self) -> TimeHandle {
        match self {
            Self::Queued(handle) | Self::Immediate { handle, .. } => handle,
        }
    }
}

/// Outcome of a Timeline control command.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TimeCommandOutcome {
    /// The command changed operation state.
    Applied,
    /// No operation exists for the handle.
    NotFound,
    /// The operation has already run or been cancelled.
    AlreadyStopped,
}

/// Host/runtime policy for dispatch-time wavelength validation violations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TimingViolationPolicy {
    /// Record mismatches for diagnostics while allowing catching dispatch to continue.
    #[default]
    RecordOnly,
    /// Return a dispatch failure when a host-default mismatch is observed.
    ReportFailure,
}

impl TimingViolationPolicy {
    #[must_use]
    pub(super) const fn reports_host_default(self) -> bool {
        matches!(self, Self::ReportFailure)
    }
}

/// Snapshot of Timeline queue state for host diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimelineDiagnostics {
    pub pending_count: usize,
    pub current_clock: Option<i32>,
    pub current_wave: Option<crate::model::Wave>,
    pub total_waves: Option<i32>,
    pub start_clock_floor: Option<i32>,
    pub start_relative_floor: Option<RelativeTime>,
    pub timing_violation: Option<TimelineTimingViolation>,
    pub assumption_warning: Option<TimelineAssumptionWarning>,
    pub timing_violation_policy: TimingViolationPolicy,
    pub total_wave_pruned_queues: u64,
    pub total_wave_pruned_ops: u64,
    pub late_attach_pruned_ops: u64,
    pub dispatching: bool,
}
