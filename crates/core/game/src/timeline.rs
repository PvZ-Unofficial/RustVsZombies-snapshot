//! Ordinary registration into the current Timeline.

use crate::runtime::{RuntimeError, RuntimeResult};
use rsvz_model::{RelativeTime, Wave};
pub use rsvz_schedule::timeline::{TimeHandle, with_timeline};
use rsvz_schedule::timeline::{Timeline, TimelineDispatchResult, TimelineRegistration, terminate_timeline_callback};

/// Successful callback values are discarded; callback errors use RuntimeError.
pub trait TimelineOutput {
    fn into_timeline_result(self) -> RuntimeResult<()>;
}

impl TimelineOutput for () {
    fn into_timeline_result(self) -> RuntimeResult<()> {
        Ok(())
    }
}

impl<T> TimelineOutput for Option<T> {
    fn into_timeline_result(self) -> RuntimeResult<()> {
        Ok(())
    }
}

impl<T> TimelineOutput for Vec<T> {
    fn into_timeline_result(self) -> RuntimeResult<()> {
        Ok(())
    }
}

impl<T> TimelineOutput for RuntimeResult<T> {
    fn into_timeline_result(self) -> RuntimeResult<()> {
        self.map(|_| ())
    }
}

/// Registers a callback and reports a binding error, returning None on failure.
pub fn at<O: TimelineOutput>(wave: i32, time: i32, callback: impl FnMut() -> O + 'static) -> Option<TimeHandle>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::BoardReadinessBackend + rsvz_backend_api::GameUiBackend,
{
    match try_at(wave, time, callback) {
        Ok(handle) => Some(handle),
        Err(error) => {
            crate::diagnostics::report_operation_error(error);
            None
        }
    }
}

/// Returns only binding failures. Immediate callback failures follow dispatch policy.
pub fn try_at<O: TimelineOutput>(
    wave: i32, time: i32, mut callback: impl FnMut() -> O + 'static,
) -> RuntimeResult<TimeHandle>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::BoardReadinessBackend + rsvz_backend_api::GameUiBackend,
{
    let callback = move || callback().into_timeline_result();
    let registration = with_timeline(|timeline| {
        let time = RelativeTime::new(Wave(wave), time);
        if crate::registration::is_active() || timeline.is_dispatching() {
            timeline.at_time_runtime_deferred(time, callback)
        } else {
            timeline.at_time_runtime(time, callback)
        }
    })
    .map_err(|error| RuntimeError::new(error.to_string()))?;
    Ok(consume_registration(registration))
}

pub(crate) fn consume_registration(registration: TimelineRegistration) -> TimeHandle
where
    rsvz_current::CurrentBackend: rsvz_backend_api::BoardReadinessBackend + rsvz_backend_api::GameUiBackend,
{
    match registration {
        TimelineRegistration::Queued(handle) => handle,
        TimelineRegistration::Immediate { handle, op } => {
            // Registration and the queue borrow end before the user's callback runs.
            let result = Timeline::run_runtime_op_catching(op);
            let error = match result {
                TimelineDispatchResult::Continue => return handle,
                TimelineDispatchResult::OperationError(error) => {
                    crate::session::record_dispatch_outcome(crate::session::DispatchOutcome::RecoverableError);
                    crate::diagnostics::report_runtime_error(error);
                    return handle;
                }
                TimelineDispatchResult::OperationPanic => RuntimeError::new("timeline immediate callback panicked"),
                TimelineDispatchResult::BackendError(error) | TimelineDispatchResult::BackendControlError(error) => {
                    error
                }
                TimelineDispatchResult::ControlTimingError(error) => RuntimeError::new(error.to_string()),
                TimelineDispatchResult::TimingViolation(error) => RuntimeError::new(error.to_string()),
            };
            crate::session::fail_script(error);
            terminate_timeline_callback();
        }
    }
}

mod dispatch;
pub use dispatch::{
    discard_runtime_before_relative_time, dispatch_timeline_tick, dispatch_timeline_tick_reporting,
    prime_runtime_total_waves, runtime_timeline_diagnostics, runtime_wave_clocks, with_runtime_wave_clocks,
};

/// Executes already accepted runtime registrations after the caller's commit actions.
#[doc(hidden)]
pub fn consume_registrations(registrations: Vec<TimelineRegistration>)
where
    rsvz_current::CurrentBackend: rsvz_backend_api::BoardReadinessBackend + rsvz_backend_api::GameUiBackend,
{
    for registration in registrations {
        consume_registration(registration);
    }
}

/// Registers an operation with a shared backend borrow acquired at execution time.
pub fn at_frame<O: TimelineOutput>(
    wave: i32, time: i32, mut callback: impl for<'frame> FnMut(crate::frame::Frame<'frame>) -> O + 'static,
) -> Option<TimeHandle>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::BoardReadinessBackend + rsvz_backend_api::GameUiBackend,
{
    at(wave, time, move || {
        crate::frame::with_frame(|frame| callback(frame).into_timeline_result()).and_then(|value| value)
    })
}

/// Returns binding failures; Frame acquisition errors follow callback execution policy.
pub fn try_at_frame<O: TimelineOutput>(
    wave: i32, time: i32, mut callback: impl for<'frame> FnMut(crate::frame::Frame<'frame>) -> O + 'static,
) -> RuntimeResult<TimeHandle>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::BoardReadinessBackend + rsvz_backend_api::GameUiBackend,
{
    try_at(wave, time, move || {
        crate::frame::with_frame(|frame| callback(frame).into_timeline_result()).and_then(|value| value)
    })
}
