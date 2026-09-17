use rsvz_backend_api::error::RuntimeError;
use std::num::NonZeroU64;

use crate::model::{RelativeTime, Wave, WaveClockState, WaveTimingSnapshot};
use rsvz_profiling::{DetailedProfileCounter, increment_detailed_profile_counter};

use super::op::{
    PendingAfter, QueuedTimeOp, ReadyTimedOp, TimeCallback, TimeOpRecord, max_relative_time,
    relative_time_is_before_floor,
};
use super::{
    TimeCommandOutcome, TimeHandle, TimeOpId, Timeline, TimelineDiagnostics, TimelineRegistration,
    TimelineRegistrationError, TimingViolationPolicy,
};

impl Timeline {
    /// Creates an empty Timeline.
    #[must_use]
    pub fn new() -> Self {
        Self {
            wave_queues: std::collections::BTreeMap::new(),
            pending_after: Vec::new(),
            op_store: Vec::new(),
            internal_controls: std::collections::VecDeque::new(),
            wave_clocks: WaveClockState::new(),
            next_id: NonZeroU64::MIN,
            next_order: 0,
            current_clock: None,
            current_wave: None,
            total_waves: None,
            activated: false,
            start_clock_floor: None,
            start_relative_floor: None,
            timing_violation: None,
            assumption_warning: None,
            timing_violation_policy: TimingViolationPolicy::RecordOnly,
            dispatching: false,
            total_wave_pruned_queues: 0,
            total_wave_pruned_ops: 0,
            late_attach_pruned_ops: 0,
        }
    }

    /// Creates an empty Timeline with an explicit timing-violation policy.
    #[must_use]
    pub fn with_timing_violation_policy(policy: TimingViolationPolicy) -> Self {
        let mut timeline = Self::new();
        timeline.timing_violation_policy = policy;
        timeline
    }

    /// Registers a ordinary operation at a wave-relative time.
    pub fn at<F>(&mut self, wave: Wave, time: i32, f: F) -> Result<TimelineRegistration, TimelineRegistrationError>
    where
        F: FnMut() -> Result<(), RuntimeError> + 'static,
    {
        self.at_time_runtime(RelativeTime::new(wave, time), f)
    }

    /// Registers a ordinary operation at a wave-relative time.
    pub fn at_time_runtime<F>(
        &mut self, time: RelativeTime, f: F,
    ) -> Result<TimelineRegistration, TimelineRegistrationError>
    where
        F: FnMut() -> Result<(), RuntimeError> + 'static,
    {
        self.register_at_time(time, TimeCallback::new(Box::new(f)))
    }

    /// Atomically registers ordinary operations at wave-relative times.
    #[doc(hidden)]
    pub fn at_times_runtime(
        &mut self, entries: Vec<(RelativeTime, Box<dyn FnMut() -> Result<(), RuntimeError> + 'static>)>,
    ) -> Result<Vec<TimelineRegistration>, TimelineRegistrationError> {
        for (time, _) in &entries {
            self.validate_registration(*time)?;
        }

        Ok(entries
            .into_iter()
            .map(|(time, callback)| self.commit_at_time_deferred(time, TimeCallback::new(callback)))
            .collect())
    }

    /// Registers a same-clock operation without executing it during script
    /// registration. It remains ordered with other deferred registrations.
    #[doc(hidden)]
    pub fn at_time_runtime_deferred<F>(
        &mut self, time: RelativeTime, f: F,
    ) -> Result<TimelineRegistration, TimelineRegistrationError>
    where
        F: FnMut() -> Result<(), RuntimeError> + 'static,
    {
        self.validate_registration(time)?;
        Ok(self.commit_at_time_deferred(time, TimeCallback::new(Box::new(f))))
    }

    /// Registers a ordinary operation after a number of frames from the current Timeline clock.
    pub fn after<F>(&mut self, frames: i32, f: F) -> Result<TimelineRegistration, TimelineRegistrationError>
    where
        F: FnMut() -> Result<(), RuntimeError> + 'static,
    {
        if frames < 0 {
            return Err(TimelineRegistrationError::InvalidDelay { frames });
        }

        if let Some(basis) = self.current_relative_basis() {
            let time = RelativeTime::new(basis.wave, basis.time.saturating_add(frames));
            return self.register_at_time(time, TimeCallback::new(Box::new(f)));
        }

        let id = self.allocate_id();
        let handle = TimeHandle::new(id);
        self.op_store.push(TimeOpRecord {
            id,
            callback: Some(TimeCallback::new(Box::new(f))),
        });
        self.pending_after.push(PendingAfter { id, frames });
        Ok(TimelineRegistration::Queued(handle))
    }

    /// Cancels an operation if it has not run yet.
    pub fn stop(&mut self, handle: TimeHandle) -> TimeCommandOutcome {
        let Some(record) = self.op_record_mut(handle.id()) else {
            return TimeCommandOutcome::NotFound;
        };
        if record.callback.take().is_some() {
            return TimeCommandOutcome::Applied;
        }
        TimeCommandOutcome::AlreadyStopped
    }

    /// Clears pending operations, ready operations, and observed timing state.
    /// Configured wavelength assumptions are kept for the next run.
    pub fn clear(&mut self) {
        self.wave_queues.clear();
        self.pending_after.clear();
        self.op_store.clear();
        self.wave_clocks.clear();
        self.current_clock = None;
        self.current_wave = None;
        self.total_waves = None;
        self.activated = false;
        self.start_clock_floor = None;
        self.start_relative_floor = None;
        self.timing_violation = None;
        self.assumption_warning = None;
        self.total_wave_pruned_queues = 0;
        self.total_wave_pruned_ops = 0;
        self.late_attach_pruned_ops = 0;
    }

    /// Clears pending operations, observed timing state, and configured wavelength assumptions.
    pub fn clear_all(&mut self) {
        self.clear();
        self.internal_controls.clear();
        self.wave_clocks.clear_all();
    }

    /// Whether Timeline dispatch is active.
    #[must_use]
    pub const fn is_dispatching(&self) -> bool {
        self.dispatching
    }

    /// Returns a lightweight diagnostic snapshot of queue state.
    #[must_use]
    pub fn diagnostics(&self) -> TimelineDiagnostics {
        TimelineDiagnostics {
            pending_count: self.live_op_count().saturating_add(self.internal_controls.len()),
            current_clock: self.current_clock,
            current_wave: self.current_wave,
            total_waves: self.total_waves,
            start_clock_floor: self.start_clock_floor,
            start_relative_floor: self.start_relative_floor,
            timing_violation: self.timing_violation,
            assumption_warning: self.assumption_warning,
            timing_violation_policy: self.timing_violation_policy,
            total_wave_pruned_queues: self.total_wave_pruned_queues,
            total_wave_pruned_ops: self.total_wave_pruned_ops,
            late_attach_pruned_ops: self.late_attach_pruned_ops,
            dispatching: self.dispatching,
        }
    }

    /// Returns the dispatch-time timing-violation policy.
    #[must_use]
    pub const fn timing_violation_policy(&self) -> TimingViolationPolicy {
        self.timing_violation_policy
    }

    /// Sets the dispatch-time timing-violation policy.
    pub const fn set_timing_violation_policy(&mut self, policy: TimingViolationPolicy) {
        self.timing_violation_policy = policy;
    }

    /// Current Timeline clock, if known.
    #[must_use]
    pub const fn current_clock(&self) -> Option<i32> {
        self.current_clock
    }

    /// Drops timed operations whose absolute due clock is earlier than `clock`.
    ///
    /// This mirrors AvZ's fight-interface attach behavior: connections before the attach time are
    /// not replayed in a burst when the script takes over an existing battle.
    pub fn discard_before_clock(&mut self, clock: i32) {
        self.activated = true;
        self.start_clock_floor = Some(self.start_clock_floor.map_or(clock, |floor| floor.max(clock)));
        self.discard_before_floor();
    }

    /// Drops timed operations before the given wave-relative attach point.
    pub fn discard_before_relative_time(&mut self, time: RelativeTime, clock: i32) {
        self.activated = true;
        self.start_relative_floor = Some(
            self.start_relative_floor
                .map_or(time, |floor| max_relative_time(floor, time)),
        );
        self.start_clock_floor = Some(self.start_clock_floor.map_or(clock, |floor| floor.max(clock)));
        self.discard_before_floor();
    }

    pub(super) fn activate_from_current_snapshot(&mut self) {
        if self.activated {
            return;
        }
        self.activated = true;
        if let Some(clock) = self.current_clock {
            self.start_clock_floor = Some(self.start_clock_floor.map_or(clock, |floor| floor.max(clock)));
        }
        if let Some(time) = self.current_relative_basis() {
            self.start_relative_floor = Some(
                self.start_relative_floor
                    .map_or(time, |floor| max_relative_time(floor, time)),
            );
        }
        self.discard_before_floor();
    }

    fn discard_before_floor(&mut self) {
        if let Some(floor) = self.start_relative_floor {
            self.prune_queued_ops(|time, _due_clock| relative_time_is_before_floor(time, floor));
        }

        if let Some(floor) = self.start_clock_floor {
            self.prune_queued_ops(|_time, due_clock| due_clock.is_some_and(|due_clock| due_clock < floor));
        }
    }

    /// Known wave clock state.
    #[must_use]
    pub const fn wave_clocks(&self) -> &WaveClockState {
        &self.wave_clocks
    }

    /// Whether there are no live timed operations.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.live_op_count() == 0 && self.internal_controls.is_empty()
    }

    pub(super) fn cache_total_waves_from_snapshot(&mut self, snapshot: WaveTimingSnapshot) {
        let Some(total_waves) = snapshot.total_waves else {
            return;
        };
        self.prime_total_waves(total_waves);
    }

    /// Primes total-wave context without activating dispatch timing or converting pending `after`.
    pub fn prime_total_waves(&mut self, total_waves: i32) {
        if self.total_waves.is_none() {
            self.total_waves = Some(total_waves);
            self.prune_waves_above_total(total_waves);
        }
    }

    pub(super) fn convert_pending_after_from_known_basis(&mut self) {
        if self.pending_after.is_empty() {
            return;
        }
        let Some(basis) = self.current_relative_basis() else {
            return;
        };
        let pending = std::mem::take(&mut self.pending_after);
        for pending_after in pending {
            if !self.is_live_op(pending_after.id) {
                continue;
            }
            let time = RelativeTime::new(basis.wave, basis.time.saturating_add(pending_after.frames));
            if self
                .start_relative_floor
                .is_some_and(|floor| relative_time_is_before_floor(time, floor))
            {
                if self.mark_op_pruned(pending_after.id) {
                    self.late_attach_pruned_ops = self.late_attach_pruned_ops.saturating_add(1);
                    increment_detailed_profile_counter(TIMELINE_PRUNED_OPS, 1);
                }
                continue;
            }
            self.enqueue_existing(time, pending_after.id);
            increment_detailed_profile_counter(TIMELINE_PENDING_AFTER_CONVERSIONS, 1);
        }
    }

    pub(super) fn pop_due_wave_op(&mut self, clock: i32) -> Option<ReadyTimedOp> {
        if let Some(control) = self.internal_controls.front()
            && let Some(refresh_clock) = self.wave_clocks.refresh_clock(control.target.wave)
            && refresh_clock.saturating_add(control.target.time) <= clock
        {
            let control = self.internal_controls.pop_front()?;
            return Some(ReadyTimedOp {
                due_clock: refresh_clock.saturating_add(control.target.time),
                dispatch_clock: clock,
                callback: TimeCallback {
                    control: Some(control.target),
                    callback: control.callback,
                },
            });
        }

        loop {
            let mut skipped_all = true;
            let mut previous_wave_key = None;
            while let Some(wave_key) = match previous_wave_key {
                Some(previous) => self
                    .wave_queues
                    .range((std::ops::Bound::Excluded(previous), std::ops::Bound::Unbounded))
                    .next()
                    .map(|(&wave_key, _)| wave_key),
                None => self.wave_queues.first_key_value().map(|(&wave_key, _)| wave_key),
            } {
                previous_wave_key = Some(wave_key);
                increment_detailed_profile_counter(TIMELINE_WAVE_QUEUES_VISITED, 1);
                let wave = Wave(wave_key);
                let Some(refresh_clock) = self.wave_clocks.refresh_clock(wave) else {
                    increment_detailed_profile_counter(TIMELINE_WAVE_QUEUES_SKIPPED_UNKNOWN_CLOCK, 1);
                    continue;
                };
                let now_time = clock.saturating_sub(refresh_clock);
                let popped = {
                    let Some(queue) = self.wave_queues.get_mut(&wave_key) else {
                        continue;
                    };
                    increment_detailed_profile_counter(TIMELINE_WAVE_HEAD_CHECKS, 1);
                    queue.pop_due(now_time)
                };
                let remove_wave = self
                    .wave_queues
                    .get(&wave_key)
                    .is_some_and(|queue| queue.entries.is_empty());
                if remove_wave {
                    self.wave_queues.remove(&wave_key);
                }
                let Some((time, queued)) = popped else {
                    continue;
                };
                skipped_all = false;
                increment_detailed_profile_counter(TIMELINE_WAVE_OPS_POPPED, 1);
                if let Some(callback) = self.take_live_callback(queued.id) {
                    return Some(ReadyTimedOp {
                        due_clock: refresh_clock.saturating_add(time),
                        dispatch_clock: clock,
                        callback,
                    });
                }
                increment_detailed_profile_counter(TIMELINE_WAVE_STALE_SKIPS, 1);
            }
            if skipped_all {
                return None;
            }
        }
    }

    fn register_at_time(
        &mut self, time: RelativeTime, callback: TimeCallback,
    ) -> Result<TimelineRegistration, TimelineRegistrationError> {
        self.validate_registration(time)?;
        Ok(if self.dispatching {
            self.commit_at_time_deferred(time, callback)
        } else {
            self.commit_at_time(time, callback)
        })
    }

    fn commit_at_time(&mut self, time: RelativeTime, callback: TimeCallback) -> TimelineRegistration {
        let due_clock = self.known_due_clock_for_time(time);
        if let (Some(due_clock), Some(current_clock)) = (due_clock, self.current_clock)
            && due_clock == current_clock
        {
            let id = self.allocate_id();
            let handle = TimeHandle::new(id);
            self.op_store.push(TimeOpRecord { id, callback: None });
            return TimelineRegistration::Immediate {
                handle,
                op: super::RuntimeReadyTimedOp::new(ReadyTimedOp {
                    due_clock,
                    dispatch_clock: current_clock,
                    callback,
                }),
            };
        }

        let id = self.allocate_id();
        let handle = TimeHandle::new(id);
        self.op_store.push(TimeOpRecord {
            id,
            callback: Some(callback),
        });
        self.enqueue_existing(time, id);
        TimelineRegistration::Queued(handle)
    }

    fn commit_at_time_deferred(&mut self, time: RelativeTime, callback: TimeCallback) -> TimelineRegistration {
        let id = self.allocate_id();
        let handle = TimeHandle::new(id);
        self.op_store.push(TimeOpRecord {
            id,
            callback: Some(callback),
        });
        self.enqueue_existing(time, id);
        TimelineRegistration::Queued(handle)
    }

    pub(super) fn validate_registration(&self, time: RelativeTime) -> Result<(), TimelineRegistrationError> {
        self.validate_registration_with_wave_clocks(time, &self.wave_clocks)
    }

    pub(super) fn validate_registration_with_wave_clocks(
        &self, time: RelativeTime, wave_clocks: &WaveClockState,
    ) -> Result<(), TimelineRegistrationError> {
        self.validate_time_for_registration(time)?;
        if let (Some(due_clock), Some(current_clock)) = (
            wave_clocks
                .refresh_clock(time.wave)
                .map(|refresh_clock| refresh_clock.saturating_add(time.time)),
            self.current_clock,
        ) && self.activated
            && due_clock < current_clock
        {
            return Err(TimelineRegistrationError::PastDue {
                target: time,
                due_clock,
                current_clock,
            });
        }
        Ok(())
    }

    pub(super) fn commit_internal_control<F>(&mut self, target: RelativeTime, callback: F)
    where
        F: FnMut() -> Result<(), RuntimeError> + 'static,
    {
        self.allocate_id();
        let control = super::op::InternalControlOp {
            order: self.allocate_order(),
            target,
            callback: Box::new(callback),
        };
        self.internal_controls.push_back(control);
        self.internal_controls
            .make_contiguous()
            .sort_by_key(|control| (control.target.wave.0, control.target.time, control.order));
    }

    fn validate_time_for_registration(&self, time: RelativeTime) -> Result<(), TimelineRegistrationError> {
        if time.wave.0 < 0 {
            return Err(TimelineRegistrationError::InvalidWave { wave: time.wave.0 });
        }
        if let Some(total_waves) = self.total_waves
            && time.wave.0 > total_waves.saturating_add(1)
        {
            return Err(TimelineRegistrationError::WaveOutOfRange {
                wave: time.wave.0,
                total_waves,
            });
        }
        if self.activated {
            if let (Some(due_clock), Some(clock_floor)) = (self.known_due_clock_for_time(time), self.start_clock_floor)
            {
                if due_clock < clock_floor {
                    return Err(TimelineRegistrationError::PastDue {
                        target: time,
                        due_clock,
                        current_clock: clock_floor,
                    });
                }
                return Ok(());
            }
            if let Some(floor) = self.start_relative_floor
                && relative_time_is_before_floor(time, floor)
            {
                return Err(TimelineRegistrationError::BeforeStartFloor { target: time, floor });
            }
        }
        Ok(())
    }

    fn enqueue_existing(&mut self, time: RelativeTime, id: TimeOpId) {
        self.wave_queues
            .entry(time.wave.0)
            .or_default()
            .push(time.time, QueuedTimeOp { id });
    }

    fn current_relative_basis(&self) -> Option<RelativeTime> {
        let wave = self.current_wave?;
        let clock = self.current_clock?;
        let refresh_clock = self.wave_clocks.refresh_clock(wave)?;
        Some(RelativeTime::new(wave, clock.saturating_sub(refresh_clock)))
    }

    fn known_due_clock_for_time(&self, time: RelativeTime) -> Option<i32> {
        self.wave_clocks
            .refresh_clock(time.wave)
            .map(|refresh_clock| refresh_clock.saturating_add(time.time))
    }

    fn prune_waves_above_total(&mut self, total_waves: i32) {
        let max_wave = total_waves.saturating_add(1);
        let waves: Vec<i32> = self
            .wave_queues
            .keys()
            .copied()
            .filter(|wave| *wave > max_wave)
            .collect();
        for wave in waves {
            if let Some(queue) = self.wave_queues.remove(&wave) {
                self.total_wave_pruned_queues = self.total_wave_pruned_queues.saturating_add(1);
                for (_, ops) in queue.entries {
                    for op in ops {
                        if self.mark_op_pruned(op.id) {
                            self.total_wave_pruned_ops = self.total_wave_pruned_ops.saturating_add(1);
                            increment_detailed_profile_counter(TIMELINE_PRUNED_OPS, 1);
                        }
                    }
                }
            }
        }
    }

    fn prune_queued_ops(&mut self, mut should_prune: impl FnMut(RelativeTime, Option<i32>) -> bool) {
        let wave_keys: Vec<i32> = self.wave_queues.keys().copied().collect();
        for wave_key in wave_keys {
            let time_keys: Vec<i32> = self
                .wave_queues
                .get(&wave_key)
                .map(|queue| queue.entries.keys().copied().collect())
                .unwrap_or_default();
            for time_key in time_keys {
                let time = RelativeTime::new(Wave(wave_key), time_key);
                let due_clock = self.known_due_clock_for_time(time);
                if !should_prune(time, due_clock) {
                    continue;
                }
                let Some(queue) = self.wave_queues.get_mut(&wave_key) else {
                    continue;
                };
                let Some(ops) = queue.entries.remove(&time_key) else {
                    continue;
                };
                for op in ops {
                    if self.mark_op_pruned(op.id) {
                        self.late_attach_pruned_ops = self.late_attach_pruned_ops.saturating_add(1);
                        increment_detailed_profile_counter(TIMELINE_PRUNED_OPS, 1);
                    }
                }
            }
            let remove_wave = self
                .wave_queues
                .get(&wave_key)
                .is_some_and(|queue| queue.entries.is_empty());
            if remove_wave {
                self.wave_queues.remove(&wave_key);
            }
        }
    }

    fn live_op_count(&self) -> usize {
        self.op_store.iter().filter(|record| record.callback.is_some()).count()
    }

    fn is_live_op(&self, id: TimeOpId) -> bool {
        self.op_store
            .iter()
            .any(|record| record.id == id && record.callback.is_some())
    }

    fn take_live_callback(&mut self, id: TimeOpId) -> Option<TimeCallback> {
        let record = self.op_record_mut(id)?;
        record.callback.take()
    }

    fn mark_op_pruned(&mut self, id: TimeOpId) -> bool {
        let Some(record) = self.op_record_mut(id) else {
            return false;
        };
        if record.callback.take().is_some() {
            return true;
        }
        false
    }

    fn op_record_mut(&mut self, id: TimeOpId) -> Option<&mut TimeOpRecord> {
        self.op_store.iter_mut().find(|record| record.id == id)
    }

    fn allocate_id(&mut self) -> TimeOpId {
        let id = self.next_id;
        let next = id.get().wrapping_add(1);
        self.next_id = NonZeroU64::new(next).unwrap_or(NonZeroU64::MIN);
        TimeOpId::new(id)
    }

    fn allocate_order(&mut self) -> u64 {
        let order = self.next_order;
        self.next_order = self.next_order.wrapping_add(1);
        order
    }
}

const TIMELINE_WAVE_QUEUES_VISITED: DetailedProfileCounter =
    DetailedProfileCounter::new("timeline_wave_queues_visited");
const TIMELINE_WAVE_QUEUES_SKIPPED_UNKNOWN_CLOCK: DetailedProfileCounter =
    DetailedProfileCounter::new("timeline_wave_queues_skipped_unknown_clock");
const TIMELINE_WAVE_HEAD_CHECKS: DetailedProfileCounter = DetailedProfileCounter::new("timeline_wave_head_checks");
const TIMELINE_WAVE_OPS_POPPED: DetailedProfileCounter = DetailedProfileCounter::new("timeline_wave_ops_popped");
const TIMELINE_WAVE_STALE_SKIPS: DetailedProfileCounter = DetailedProfileCounter::new("timeline_wave_stale_skips");
const TIMELINE_PENDING_AFTER_CONVERSIONS: DetailedProfileCounter =
    DetailedProfileCounter::new("timeline_pending_after_conversions");
const TIMELINE_PRUNED_OPS: DetailedProfileCounter = DetailedProfileCounter::new("timeline_pruned_ops");

#[cfg(test)]
mod profile_names {
    use super::*;
    #[test]
    fn timeline_detailed_profile_counter_names_use_per_wave_terms() {
        assert_eq!(TIMELINE_WAVE_QUEUES_VISITED.name(), "timeline_wave_queues_visited");
        assert_eq!(
            TIMELINE_WAVE_QUEUES_SKIPPED_UNKNOWN_CLOCK.name(),
            "timeline_wave_queues_skipped_unknown_clock"
        );
        assert_eq!(TIMELINE_WAVE_HEAD_CHECKS.name(), "timeline_wave_head_checks");
        assert_eq!(TIMELINE_WAVE_OPS_POPPED.name(), "timeline_wave_ops_popped");
        assert_eq!(TIMELINE_WAVE_STALE_SKIPS.name(), "timeline_wave_stale_skips");
        assert_eq!(
            TIMELINE_PENDING_AFTER_CONVERSIONS.name(),
            "timeline_pending_after_conversions"
        );
        assert_eq!(TIMELINE_PRUNED_OPS.name(), "timeline_pruned_ops");
    }
}
