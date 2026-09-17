use rsvz_backend_api::error::RuntimeError;
use std::collections::{BTreeMap, VecDeque};

use crate::model::RelativeTime;

use super::TimeOpId;

pub(super) type TimeRuntimeCallback = Box<dyn FnMut() -> Result<(), RuntimeError> + 'static>;

pub(super) struct TimeCallback {
    pub(super) callback: TimeRuntimeCallback,
    pub(super) control: Option<RelativeTime>,
}

impl TimeCallback {
    pub(super) fn new(callback: TimeRuntimeCallback) -> Self {
        Self {
            callback,
            control: None,
        }
    }
}

pub(super) fn max_relative_time(left: RelativeTime, right: RelativeTime) -> RelativeTime {
    if right.wave.0 > left.wave.0 || (right.wave == left.wave && right.time > left.time) {
        right
    } else {
        left
    }
}

pub(super) fn relative_time_is_before_floor(time: RelativeTime, floor: RelativeTime) -> bool {
    time.wave.0 < floor.wave.0 || (time.wave == floor.wave && time.time < floor.time)
}

#[derive(Clone, Copy)]
pub(super) struct QueuedTimeOp {
    pub(super) id: TimeOpId,
}

#[derive(Default)]
pub(super) struct WaveQueue {
    pub(super) entries: BTreeMap<i32, VecDeque<QueuedTimeOp>>,
}

impl WaveQueue {
    pub(super) fn push(&mut self, time: i32, op: QueuedTimeOp) {
        self.entries.entry(time).or_default().push_back(op);
    }

    pub(super) fn pop_due(&mut self, now_time: i32) -> Option<(i32, QueuedTimeOp)> {
        let (&time, _) = self.entries.first_key_value()?;
        if time > now_time {
            return None;
        }
        let queue = self.entries.get_mut(&time)?;
        let op = queue.pop_front()?;
        if queue.is_empty() {
            self.entries.remove(&time);
        }
        Some((time, op))
    }
}

#[derive(Clone, Copy)]
pub(super) struct PendingAfter {
    pub(super) id: TimeOpId,
    pub(super) frames: i32,
}

pub(super) struct TimeOpRecord {
    pub(super) id: TimeOpId,
    pub(super) callback: Option<TimeCallback>,
}

pub(super) struct ReadyTimedOp {
    pub(super) due_clock: i32,
    pub(super) dispatch_clock: i32,
    pub(super) callback: TimeCallback,
}

pub(super) struct InternalControlOp {
    pub(super) order: u64,
    pub(super) target: RelativeTime,
    pub(super) callback: TimeRuntimeCallback,
}

/// Opaque ready Timeline operation used by split dispatch.
#[doc(hidden)]
pub struct RuntimeReadyTimedOp {
    inner: ReadyTimedOp,
}

impl RuntimeReadyTimedOp {
    pub(super) const fn new(inner: ReadyTimedOp) -> Self {
        Self { inner }
    }

    pub(super) fn into_inner(self) -> ReadyTimedOp {
        self.inner
    }
}
