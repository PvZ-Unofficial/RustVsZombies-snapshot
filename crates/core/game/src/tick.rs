//! tick operations for the selected backend.

use crate::frame::profile_if;
use crate::runtime::RuntimeError;
use crate::runtime::RuntimeResult;
use crate::session::{DispatchOutcome, record_dispatch_outcome, script_stop_requested};
use rsvz_profiling::{
    DetailedProfileCounter, DetailedProfileStage, detailed_profile_enabled, increment_detailed_profile_counter,
};
use rsvz_schedule::tick::{TickControl, TickOptions};
use rsvz_schedule::tick::{TickDispatchResult, TickMeta, TickScheduler, with_scheduler};
use std::ops::ControlFlow;

pub fn dispatch_scheduler_tick(meta: TickMeta) -> TickDispatchResult
where
    rsvz_current::CurrentBackend:
        rsvz_backend_api::BoardReadinessBackend + rsvz_backend_api::GameUiBackend + rsvz_backend_api::WaveTimingBackend,
{
    dispatch_scheduler_tick_with_error_handler(meta, false, &mut ControlFlow::Break)
}

fn dispatch_scheduler_tick_with_error_handler(
    mut meta: TickMeta, finalizers_only: bool, handle_error: &mut impl FnMut(RuntimeError) -> ControlFlow<RuntimeError>,
) -> TickDispatchResult
where
    rsvz_current::CurrentBackend:
        rsvz_backend_api::BoardReadinessBackend + rsvz_backend_api::GameUiBackend + rsvz_backend_api::WaveTimingBackend,
{
    let mut epoch = rsvz_current::backend_access_epoch();
    let profile_detail = detailed_profile_enabled();
    let mut order = with_scheduler(|scheduler| {
        let order = profile_if(profile_detail, SCHEDULER_COLLECT_ORDER, || {
            if finalizers_only {
                scheduler.begin_runtime_finalizer_dispatch_tick(meta)
            } else {
                scheduler.begin_runtime_dispatch_tick(meta)
            }
        });
        if profile_detail {
            increment_detailed_profile_counter(SCHEDULER_BUCKETS_SCANNED, order.buckets_scanned());
            increment_detailed_profile_counter(
                SCHEDULER_CALLBACKS_QUEUED,
                order.queued_len().try_into().unwrap_or(u64::MAX),
            );
        }
        order
    });
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        loop {
            if epoch != rsvz_current::backend_access_epoch() {
                epoch = rsvz_current::backend_access_epoch();
                match crate::runtime::runtime_frame_facts() {
                    Ok(facts) => {
                        meta.is_new_frame = facts.phase == rsvz_schedule::tick::TickPhase::Playing
                            && (meta.is_new_frame || facts.clock != meta.clock);
                        meta.phase = facts.phase;
                        meta.game_ui = facts.game_ui;
                        meta.clock = facts.clock;
                    }
                    Err(error) => break TickDispatchResult::TaskError(error),
                }
            }
            let callback = with_scheduler(|scheduler| scheduler.take_next_runtime_callback(&mut order, meta));
            let Some(callback) = callback else {
                break TickDispatchResult::Continue;
            };
            if profile_detail {
                increment_detailed_profile_counter(SCHEDULER_CALLBACKS_RUN, 1);
            }
            let outcome = profile_if(profile_detail, SCHEDULER_CALLBACK, || {
                TickScheduler::run_runtime_callback_catching(callback, meta)
            });
            match with_scheduler(|scheduler| scheduler.finish_runtime_callback(outcome)) {
                None => {}
                Some(TickDispatchResult::TaskError(error)) => match handle_error(error) {
                    ControlFlow::Continue(()) => {}
                    ControlFlow::Break(error) => break TickDispatchResult::TaskError(error),
                },
                Some(result) => break result,
            }
            if script_stop_requested() {
                break TickDispatchResult::Continue;
            }
        }
    }));
    with_scheduler(|scheduler| scheduler.end_runtime_dispatch_tick(order));
    result.unwrap_or_else(|payload| {
        rsvz_schedule::timeline::resume_timeline_termination(payload);
        TickDispatchResult::TaskPanic
    })
}

/// Dispatches current runtime tasks while reporting ordinary task errors and
/// continuing with the remaining tasks from the same frame.
///
/// A repeating task retains its registration after an error. One-shot tasks
/// consume their execution opportunity. Task panics remain terminal.
pub fn dispatch_scheduler_tick_reporting(
    meta: TickMeta, report_error: &mut impl FnMut(RuntimeError),
) -> TickDispatchResult
where
    rsvz_current::CurrentBackend:
        rsvz_backend_api::BoardReadinessBackend + rsvz_backend_api::GameUiBackend + rsvz_backend_api::WaveTimingBackend,
{
    dispatch_scheduler_tick_with_error_handler(meta, false, &mut |error| {
        record_dispatch_outcome(DispatchOutcome::RecoverableError);
        report_error(error);
        ControlFlow::Continue(())
    })
}

pub(crate) fn dispatch_scheduler_finalizers_reporting(
    meta: TickMeta, report_error: &mut impl FnMut(RuntimeError),
) -> TickDispatchResult
where
    rsvz_current::CurrentBackend:
        rsvz_backend_api::BoardReadinessBackend + rsvz_backend_api::GameUiBackend + rsvz_backend_api::WaveTimingBackend,
{
    dispatch_scheduler_tick_with_error_handler(meta, true, &mut |error| {
        record_dispatch_outcome(DispatchOutcome::RecoverableError);
        report_error(error);
        ControlFlow::Continue(())
    })
}

pub fn spawn<F>(options: TickOptions, callback: F) -> rsvz_schedule::TickHandle
where
    F: FnMut(TickMeta) -> RuntimeResult<TickControl> + 'static,
{
    with_scheduler(|scheduler| scheduler.spawn(options, callback))
}

pub(crate) fn spawn_finalizer<F>(options: TickOptions, callback: F) -> rsvz_schedule::TickHandle
where
    F: FnMut(TickMeta) -> RuntimeResult<TickControl> + 'static,
{
    with_scheduler(|scheduler| scheduler.spawn_runtime_finalizer(options, callback))
}

const SCHEDULER_COLLECT_ORDER: DetailedProfileStage = DetailedProfileStage::new("scheduler_collect_order");
const SCHEDULER_CALLBACK: DetailedProfileStage = DetailedProfileStage::new("scheduler_callback");
const SCHEDULER_BUCKETS_SCANNED: DetailedProfileCounter = DetailedProfileCounter::new("scheduler_buckets_scanned");
const SCHEDULER_CALLBACKS_QUEUED: DetailedProfileCounter = DetailedProfileCounter::new("scheduler_callbacks_queued");
const SCHEDULER_CALLBACKS_RUN: DetailedProfileCounter = DetailedProfileCounter::new("scheduler_callbacks_run");

/// Output accepted by callbacks with a scoped Frame.
pub trait FrameTickOutput {
    fn into_tick_result(self) -> RuntimeResult<TickControl>;
}
impl FrameTickOutput for () {
    fn into_tick_result(self) -> RuntimeResult<TickControl> {
        Ok(TickControl::Continue)
    }
}
impl FrameTickOutput for TickControl {
    fn into_tick_result(self) -> RuntimeResult<TickControl> {
        Ok(self)
    }
}
impl FrameTickOutput for RuntimeResult<()> {
    fn into_tick_result(self) -> RuntimeResult<TickControl> {
        self.map(|_| TickControl::Continue)
    }
}
impl FrameTickOutput for RuntimeResult<TickControl> {
    fn into_tick_result(self) -> RuntimeResult<TickControl> {
        self
    }
}

pub fn on_frame<O: FrameTickOutput>(
    mut callback: impl for<'frame> FnMut(crate::frame::Frame<'frame>) -> O + 'static,
) -> rsvz_schedule::TickHandle {
    spawn(TickOptions::playing_frame(), move |_| {
        crate::frame::with_frame(|frame| callback(frame).into_tick_result()).and_then(|value| value)
    })
}

pub fn spawn_frame<O: FrameTickOutput>(
    options: TickOptions, mut callback: impl for<'frame> FnMut(crate::frame::Frame<'frame>) -> O + 'static,
) -> RuntimeResult<rsvz_schedule::TickHandle> {
    if options.availability == rsvz_schedule::TickAvailability::AnyDispatch {
        return Err(RuntimeError::new(
            "Frame tick tasks require Playing or Active availability",
        ));
    }
    Ok(spawn(options, move |_| {
        crate::frame::with_frame(|frame| callback(frame).into_tick_result()).and_then(|value| value)
    }))
}
