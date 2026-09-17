//! timeline operations for the selected backend.

use crate::frame::profile_if;
use crate::runtime::RuntimeError;
use crate::session::{DispatchOutcome, record_dispatch_outcome, script_stop_requested};
use rsvz_model::{RelativeTime, WaveClockState, WaveTimingSnapshot};
use rsvz_profiling::{DetailedProfileStage, detailed_profile_enabled};
use rsvz_schedule::tick::TickMeta;
use rsvz_schedule::timeline::{
    RuntimeTimelineDispatchStart, Timeline, TimelineDiagnostics, TimelineDispatchResult, with_timeline,
    with_timeline_ref,
};
use std::ops::ControlFlow;

#[must_use]
pub fn runtime_timeline_diagnostics() -> TimelineDiagnostics {
    with_timeline_ref(|timeline| timeline.diagnostics())
}

#[must_use]
pub fn runtime_wave_clocks() -> WaveClockState {
    with_timeline_ref(|timeline| timeline.wave_clocks().clone())
}

pub fn with_runtime_wave_clocks<R>(f: impl FnOnce(&WaveClockState) -> R) -> R {
    with_timeline_ref(|timeline| f(timeline.wave_clocks()))
}

pub fn discard_runtime_before_relative_time(time: RelativeTime, current_clock: i32) {
    with_timeline(|timeline| timeline.discard_before_relative_time(time, current_clock));
}

pub fn prime_runtime_total_waves(total_waves: i32) {
    with_timeline(|timeline| timeline.prime_total_waves(total_waves));
}

pub fn dispatch_timeline_tick(snapshot: WaveTimingSnapshot, meta: TickMeta) -> TimelineDispatchResult
where
    rsvz_current::CurrentBackend:
        rsvz_backend_api::BoardReadinessBackend + rsvz_backend_api::GameUiBackend + rsvz_backend_api::WaveTimingBackend,
{
    dispatch_timeline_tick_with_error_handler(snapshot, meta, &mut ControlFlow::Break)
}

fn dispatch_timeline_tick_with_error_handler(
    snapshot: WaveTimingSnapshot, meta: TickMeta,
    handle_error: &mut impl FnMut(RuntimeError) -> ControlFlow<RuntimeError>,
) -> TimelineDispatchResult
where
    rsvz_current::CurrentBackend:
        rsvz_backend_api::BoardReadinessBackend + rsvz_backend_api::GameUiBackend + rsvz_backend_api::WaveTimingBackend,
{
    let mut epoch = rsvz_current::backend_access_epoch();
    let profile_detail = detailed_profile_enabled();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let start = with_timeline(|timeline| {
            profile_if(profile_detail, TIMELINE_PREPARE, || {
                timeline.begin_runtime_dispatch_tick_catching(snapshot, meta)
            })
        });
        match start {
            RuntimeTimelineDispatchStart::Finished(TimelineDispatchResult::OperationError(error)) => {
                match handle_error(error) {
                    ControlFlow::Continue(()) => TimelineDispatchResult::Continue,
                    ControlFlow::Break(error) => TimelineDispatchResult::OperationError(error),
                }
            }
            RuntimeTimelineDispatchStart::Finished(result) => result,
            RuntimeTimelineDispatchStart::Drain { clock } => loop {
                if epoch != rsvz_current::backend_access_epoch() {
                    epoch = rsvz_current::backend_access_epoch();
                    match crate::runtime::runtime_frame_facts() {
                        Ok(facts)
                            if facts.phase == rsvz_schedule::tick::TickPhase::Playing && facts.clock == Some(clock) => {
                        }
                        Ok(_) => break TimelineDispatchResult::Continue,
                        Err(error) => break TimelineDispatchResult::BackendError(error),
                    }
                }
                let op = with_timeline(|timeline| timeline.take_due_runtime_op(clock));
                let Some(op) = op else {
                    break TimelineDispatchResult::Continue;
                };
                let result = profile_if(profile_detail, TIMELINE_CALLBACK, || {
                    Timeline::run_runtime_op_catching(op)
                });
                match result {
                    TimelineDispatchResult::Continue => {}
                    TimelineDispatchResult::OperationError(error) => match handle_error(error) {
                        ControlFlow::Continue(()) => {}
                        ControlFlow::Break(error) => break TimelineDispatchResult::OperationError(error),
                    },
                    result => break result,
                }
                if script_stop_requested() {
                    break TimelineDispatchResult::Continue;
                }
            },
        }
    }));
    with_timeline(|timeline| timeline.end_runtime_dispatch());
    result.unwrap_or_else(|payload| {
        rsvz_schedule::timeline::resume_timeline_termination(payload);
        TimelineDispatchResult::OperationPanic
    })
}

/// Dispatches the current Timeline while reporting ordinary operation errors
/// and continuing with the remaining due operations from the same frame.
///
/// Panics, timing failures, and backend failures remain terminal dispatch
/// outcomes. The reporter runs after the failing one-shot operation has been
/// consumed and outside the Timeline storage borrow.
pub fn dispatch_timeline_tick_reporting(
    snapshot: WaveTimingSnapshot, meta: TickMeta, report_error: &mut impl FnMut(RuntimeError),
) -> TimelineDispatchResult
where
    rsvz_current::CurrentBackend:
        rsvz_backend_api::BoardReadinessBackend + rsvz_backend_api::GameUiBackend + rsvz_backend_api::WaveTimingBackend,
{
    dispatch_timeline_tick_with_error_handler(snapshot, meta, &mut |error| {
        record_dispatch_outcome(DispatchOutcome::RecoverableError);
        report_error(error);
        ControlFlow::Continue(())
    })
}

const TIMELINE_PREPARE: DetailedProfileStage = DetailedProfileStage::new("timeline_prepare");
const TIMELINE_CALLBACK: DetailedProfileStage = DetailedProfileStage::new("timeline_callback");
