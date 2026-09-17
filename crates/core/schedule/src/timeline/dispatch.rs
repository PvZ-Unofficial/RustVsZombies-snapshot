use std::panic::{self, AssertUnwindSafe};

use crate::model::WaveTimingSnapshot;
use crate::tick::TickMeta;

use super::op::{ReadyTimedOp, RuntimeReadyTimedOp};
use super::{RuntimeTimelineDispatchStart, Timeline, TimelineControlTimingError, TimelineDispatchResult};

impl Timeline {
    /// Dispatches ordinary Timeline operations from a pre-collected timing snapshot.
    pub fn dispatch_tick_catching(&mut self, snapshot: WaveTimingSnapshot, meta: TickMeta) -> TimelineDispatchResult {
        self.dispatching = true;
        let result = panic::catch_unwind(AssertUnwindSafe(|| self.dispatch_tick_catching_inner(snapshot, meta)));
        self.dispatching = false;
        result.unwrap_or_else(|payload| {
            super::resume_timeline_termination(payload);
            TimelineDispatchResult::OperationPanic
        })
    }

    /// Prepares ordinary Timeline dispatch while allowing the caller to run callbacks after
    /// dropping any outer runtime storage borrow.
    #[doc(hidden)]
    pub fn begin_runtime_dispatch_tick_catching(
        &mut self, snapshot: WaveTimingSnapshot, meta: TickMeta,
    ) -> RuntimeTimelineDispatchStart {
        self.dispatching = true;
        let Some(clock) = meta.clock else {
            return RuntimeTimelineDispatchStart::Finished(TimelineDispatchResult::Continue);
        };
        if let Some(violation) = self.timing_violation
            && self.should_report_timing_violation()
        {
            return RuntimeTimelineDispatchStart::Finished(TimelineDispatchResult::TimingViolation(violation));
        }
        if let Err(result) = self.prepare_timing_from_snapshot(snapshot) {
            return RuntimeTimelineDispatchStart::Finished(result);
        }

        RuntimeTimelineDispatchStart::Drain { clock }
    }

    /// Ends a split ordinary Timeline dispatch.
    #[doc(hidden)]
    pub const fn end_runtime_dispatch(&mut self) {
        self.dispatching = false;
    }

    /// Takes one due ordinary ready operation for split runtime dispatch.
    #[doc(hidden)]
    pub fn take_due_runtime_op(&mut self, clock: i32) -> Option<RuntimeReadyTimedOp> {
        self.pop_due_wave_op(clock).map(RuntimeReadyTimedOp::new)
    }

    /// Runs one ordinary ready operation that was taken out of Timeline storage.
    #[doc(hidden)]
    pub fn run_runtime_op_catching(op: RuntimeReadyTimedOp) -> TimelineDispatchResult {
        Self::run_runtime_ready_op_catching(op.into_inner())
    }

    /// Shared timing-violation checks and pending-operation conversion after a snapshot is obtained.
    fn prepare_timing_from_snapshot(&mut self, snapshot: WaveTimingSnapshot) -> Result<(), TimelineDispatchResult> {
        let new_observed_waves = self.update_time(snapshot);
        if let Some(error) = self.past_due_internal_control(snapshot) {
            return Err(TimelineDispatchResult::ControlTimingError(error));
        }
        if let Some(violation) = self.timing_violation_for_new_clocks(&new_observed_waves) {
            self.timing_violation = Some(violation);
            if self.should_report_timing_violation() {
                return Err(TimelineDispatchResult::TimingViolation(violation));
            }
        }
        if let Some(violation) = self.refresh_delay_for_snapshot(snapshot) {
            self.timing_violation = Some(violation);
            if self.should_report_timing_violation() {
                return Err(TimelineDispatchResult::TimingViolation(violation));
            }
        }
        self.cache_total_waves_from_snapshot(snapshot);
        self.activate_from_current_snapshot();
        self.convert_pending_after_from_known_basis();
        Ok(())
    }

    fn dispatch_tick_catching_inner(&mut self, snapshot: WaveTimingSnapshot, meta: TickMeta) -> TimelineDispatchResult {
        let Some(clock) = meta.clock else {
            return TimelineDispatchResult::Continue;
        };
        if let Some(violation) = self.timing_violation
            && self.should_report_timing_violation()
        {
            return TimelineDispatchResult::TimingViolation(violation);
        }
        self.dispatch_runtime_snapshot_tick_catching(snapshot, clock)
    }

    fn dispatch_runtime_snapshot_tick_catching(
        &mut self, snapshot: WaveTimingSnapshot, clock: i32,
    ) -> TimelineDispatchResult {
        if let Err(result) = self.prepare_timing_from_snapshot(snapshot) {
            return result;
        }

        self.drain_due_runtime_ops_catching(clock)
    }

    fn drain_due_runtime_ops_catching(&mut self, clock: i32) -> TimelineDispatchResult {
        while let Some(op) = self.pop_due_wave_op(clock) {
            match Self::run_runtime_ready_op_catching(op) {
                TimelineDispatchResult::Continue => {}
                result => return result,
            }
        }

        TimelineDispatchResult::Continue
    }

    fn run_runtime_ready_op_catching(op: ReadyTimedOp) -> TimelineDispatchResult {
        let super::op::TimeCallback { mut callback, control } = op.callback;
        if let Some(target) = control
            && op.due_clock != op.dispatch_clock
        {
            return TimelineDispatchResult::ControlTimingError(TimelineControlTimingError::PastDue {
                target,
                due_clock: Some(op.due_clock),
                current_wave: target.wave,
                current_clock: op.dispatch_clock,
            });
        }
        let result = panic::catch_unwind(AssertUnwindSafe(&mut callback));

        match result {
            Ok(Ok(())) => TimelineDispatchResult::Continue,
            Ok(Err(error)) if control.is_some() => TimelineDispatchResult::BackendControlError(error),
            Ok(Err(error)) => TimelineDispatchResult::OperationError(error),
            Err(payload) => match crate::callback::into_error(payload) {
                Ok(error) if control.is_some() => TimelineDispatchResult::BackendControlError(error),
                Ok(error) => TimelineDispatchResult::OperationError(error),
                Err(payload) => {
                    super::resume_timeline_termination(payload);
                    TimelineDispatchResult::OperationPanic
                }
            },
        }
    }
}
