use std::collections::{BTreeMap, VecDeque};
use std::num::NonZeroU64;

use crate::model::{RelativeTime, Wave, WaveClockState};

use super::op::{InternalControlOp, PendingAfter, TimeOpRecord, WaveQueue};
use super::{TimelineAssumptionWarning, TimelineTimingViolation, TimingViolationPolicy};

/// Reusable Timeline queue with ordinary callbacks.
pub struct Timeline {
    pub(super) wave_queues: BTreeMap<i32, WaveQueue>,
    pub(super) pending_after: Vec<PendingAfter>,
    pub(super) op_store: Vec<TimeOpRecord>,
    pub(super) internal_controls: VecDeque<InternalControlOp>,
    pub(super) wave_clocks: WaveClockState,
    pub(super) next_id: NonZeroU64,
    pub(super) next_order: u64,
    pub(super) current_clock: Option<i32>,
    pub(super) current_wave: Option<Wave>,
    pub(super) total_waves: Option<i32>,
    pub(super) activated: bool,
    pub(super) start_clock_floor: Option<i32>,
    pub(super) start_relative_floor: Option<RelativeTime>,
    pub(super) timing_violation: Option<TimelineTimingViolation>,
    pub(super) assumption_warning: Option<TimelineAssumptionWarning>,
    pub(super) timing_violation_policy: TimingViolationPolicy,
    pub(super) dispatching: bool,
    pub(super) total_wave_pruned_queues: u64,
    pub(super) total_wave_pruned_ops: u64,
    pub(super) late_attach_pruned_ops: u64,
}

impl Default for Timeline {
    fn default() -> Self {
        Self::new()
    }
}
