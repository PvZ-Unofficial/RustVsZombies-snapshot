//! Backend-neutral logging state for the current runtime thread.

use std::cell::{Cell, RefCell};
use std::fmt;
use std::panic::{self, AssertUnwindSafe};
use std::rc::Rc;

use rsvz_model::RelativeTime;

/// Presentation severity for one runtime log record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Debug => "调试",
            Self::Info => "信息",
            Self::Warning => "警告",
            Self::Error => "错误",
        })
    }
}

/// Where a runtime log record originated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LogContext {
    Registration,
    Runtime(RelativeTime),
    Unscoped,
}

/// A structured message passed to the installed [`Logger`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LogRecord<'a> {
    pub level: LogLevel,
    pub message: &'a str,
    pub context: LogContext,
}

impl<'a> LogRecord<'a> {
    #[must_use]
    pub const fn new(level: LogLevel, message: &'a str, context: LogContext) -> Self {
        Self {
            level,
            message,
            context,
        }
    }
}

impl fmt::Display for LogRecord<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.context {
            LogContext::Registration => write!(formatter, "[注册期][{}] {}", self.level, self.message),
            LogContext::Runtime(time) => {
                write!(
                    formatter,
                    "[第 {} 波, {}cs][{}] {}",
                    time.wave.0, time.time, self.level, self.message
                )
            }
            LogContext::Unscoped => write!(formatter, "[{}] {}", self.level, self.message),
        }
    }
}

/// Destination for structured runtime log records.
pub trait Logger: 'static {
    fn log(&self, record: &LogRecord<'_>);
}

impl<F> Logger for F
where
    F: for<'a> Fn(&LogRecord<'a>) + 'static,
{
    fn log(&self, record: &LogRecord<'_>) {
        self(record);
    }
}

/// Opaque ownership of a previously installed logger.
pub struct LoggerHandle(Rc<dyn Logger>);

impl LoggerHandle {
    fn new(logger: impl Logger) -> Self {
        Self(Rc::new(logger))
    }
}

struct DefaultLogger;

impl Logger for DefaultLogger {
    fn log(&self, record: &LogRecord<'_>) {
        if matches!(record.level, LogLevel::Warning | LogLevel::Error) {
            emergency_output(&record.to_string());
        }
    }
}

thread_local! {
    static LOGGER: RefCell<LoggerHandle> = RefCell::new(LoggerHandle::new(DefaultLogger));
    static REPORTING: Cell<bool> = const { Cell::new(false) };
}

/// Installs one logger for the current runtime thread and discards the previous logger.
pub fn set_logger(logger: impl Logger) {
    let _previous = replace_logger(logger);
}

/// Restores the backend's default logger for the current runtime thread.
pub fn reset_logger() {
    let _previous = replace_logger(DefaultLogger);
}

/// Installs one logger and returns ownership of the previous logger.
#[must_use]
pub fn replace_logger(logger: impl Logger) -> LoggerHandle {
    LOGGER.with_borrow_mut(|slot| std::mem::replace(slot, LoggerHandle::new(logger)))
}

/// Restores a logger returned by [`replace_logger`].
/// If thread teardown has already destroyed the logger slot, releases the saved logger instead.
pub fn restore_logger(logger: LoggerHandle) {
    let _ = LOGGER.try_with(|slot| *slot.borrow_mut() = logger);
}

/// Delivers one record synchronously without changing runtime failure state.
pub fn emit_log(record: &LogRecord<'_>) {
    emit_log_inner(record, false);
}

fn emit_log_inner(record: &LogRecord<'_>, reporting_failure: bool) {
    let nested = REPORTING.with(|reporting| reporting.replace(true));
    if nested {
        emergency_output(&record.to_string());
        return;
    }
    let _guard = ReportingGuard;
    let logger = LOGGER.with_borrow(|slot| Rc::clone(&slot.0));
    if let Err(payload) = panic::catch_unwind(AssertUnwindSafe(|| logger.log(record))) {
        if rsvz_schedule::callback::is_abort(&*payload) {
            if !reporting_failure {
                panic::resume_unwind(payload);
            }
            // Preserve the original failure when its logger encounters another failure.
            emergency_output(&record.to_string());
            return;
        }
        rsvz_schedule::timeline::resume_timeline_termination(payload);
        emergency_output(&record.to_string());
    }
}

/// Emits one unscoped error message through the installed logger.
#[doc(hidden)]
pub fn report_message(message: &str) {
    emit_log(&LogRecord::new(LogLevel::Error, message, LogContext::Unscoped));
}

struct ReportingGuard;

impl Drop for ReportingGuard {
    fn drop(&mut self) {
        REPORTING.set(false);
    }
}

fn emergency_output(message: &str) {
    rsvz_current::default_log_output(message);
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use rsvz_model::{RelativeTime, Wave};

    use super::*;

    #[test]
    fn logger_restore_survives_tls_destruction() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };

        struct RestoreOnDrop(LoggerHandle, Arc<AtomicBool>, Arc<AtomicBool>);
        impl Drop for RestoreOnDrop {
            fn drop(&mut self) {
                self.1.store(LOGGER.try_with(|_| ()).is_err(), Ordering::SeqCst);
                let saved = LoggerHandle(Rc::clone(&self.0.0));
                let restored = panic::catch_unwind(AssertUnwindSafe(|| restore_logger(saved))).is_ok();
                self.2.store(restored, Ordering::SeqCst);
            }
        }
        thread_local! {
            static SAVED: RefCell<Option<RestoreOnDrop>> = const { RefCell::new(None) };
        }
        let destroyed = Arc::new(AtomicBool::new(false));
        let restored = Arc::new(AtomicBool::new(false));
        let flags = (Arc::clone(&destroyed), Arc::clone(&restored));
        std::thread::spawn(move || {
            // Like the state-hook registry, this owner is initialized before LOGGER.
            SAVED.with(|_| ());
            let saved = replace_logger(DefaultLogger);
            SAVED.with_borrow_mut(|slot| *slot = Some(RestoreOnDrop(saved, flags.0, flags.1)));
        })
        .join()
        .unwrap();
        assert!(
            destroyed.load(Ordering::SeqCst),
            "the test must restore after LOGGER destruction"
        );
        assert!(
            restored.load(Ordering::SeqCst),
            "restoring a dropped logger must not panic"
        );
    }

    thread_local! {
        static RECORDS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }

    fn capture(record: &LogRecord<'_>) {
        RECORDS.with_borrow_mut(|records| records.push(record.to_string()));
    }

    #[test]
    fn logger_receives_structured_context_and_level() {
        RECORDS.with_borrow_mut(Vec::clear);
        let previous = replace_logger(capture);
        emit_log(&LogRecord::new(
            LogLevel::Warning,
            "probe",
            LogContext::Runtime(RelativeTime::new(Wave(3), 125)),
        ));
        restore_logger(previous);

        RECORDS.with_borrow(|records| assert_eq!(records.as_slice(), ["[第 3 波, 125cs][警告] probe"]));
    }

    #[test]
    fn closures_can_capture_logger_state() {
        let captured = Rc::new(RefCell::new(Vec::new()));
        let target = Rc::clone(&captured);
        let previous = replace_logger(move |record: &LogRecord<'_>| {
            target.borrow_mut().push(record.message.to_owned());
        });
        emit_log(&LogRecord::new(LogLevel::Info, "captured", LogContext::Unscoped));
        restore_logger(previous);

        assert_eq!(captured.borrow().as_slice(), ["captured"]);
    }

    #[test]
    fn boxed_callbacks_are_loggers_too() {
        let captured = Rc::new(RefCell::new(Vec::new()));
        let target = Rc::clone(&captured);
        let logger: Box<dyn for<'a> Fn(&LogRecord<'a>)> = Box::new(move |record| {
            target.borrow_mut().push(record.level);
        });
        let previous = replace_logger(logger);
        emit_log(&LogRecord::new(LogLevel::Debug, "boxed", LogContext::Unscoped));
        restore_logger(previous);

        assert_eq!(captured.borrow().as_slice(), [LogLevel::Debug]);
    }
}

thread_local! {
    static REPORT_TIME: Cell<Option<RelativeTime>> = const { Cell::new(None) };
}

/// Returns the current battle time used to annotate recoverable script errors.
///
/// The value is cached before callbacks run, so reporting never re-borrows the
/// backend or Timeline from inside a callback.
#[doc(hidden)]
#[must_use]
pub fn runtime_report_time() -> Option<RelativeTime> {
    REPORT_TIME.get()
}

pub fn report_runtime_error(error: impl std::fmt::Display) {
    let error = error.to_string();
    let context = runtime_report_time().map_or(LogContext::Unscoped, LogContext::Runtime);
    emit_log_inner(&LogRecord::new(LogLevel::Error, &error, context), true);
}

struct RuntimeReportTimeGuard(Option<RelativeTime>);

impl Drop for RuntimeReportTimeGuard {
    fn drop(&mut self) {
        REPORT_TIME.set(self.0);
    }
}

pub fn with_runtime_report_time<R>(time: Option<RelativeTime>, f: impl FnOnce() -> R) -> R {
    let _guard = RuntimeReportTimeGuard(REPORT_TIME.replace(time));
    f()
}

#[doc(hidden)]
pub fn clear_report_time() {
    REPORT_TIME.set(None);
}

/// Reports an operation failure and records it for dispatch/measurement policy.
#[doc(hidden)]
pub fn report_operation_error(error: impl fmt::Display) {
    crate::session::record_dispatch_outcome(crate::session::DispatchOutcome::RecoverableError);

    let message = error.to_string();
    emit_log_inner(&LogRecord::new(LogLevel::Error, &message, current_context()), true);
}

/// Stops the nearest callback on an operation failure that cannot return a value.
/// Its execution boundary reports the error after access and component guards unwind.
#[doc(hidden)]
pub fn abort_operation(error: crate::runtime::RuntimeError) -> ! {
    rsvz_schedule::callback::abort(error)
}

/// Writes an Error-level diagnostic without changing callback or measurement state.
pub fn error(message: impl fmt::Display) {
    log(LogLevel::Error, message);
}

pub fn log(level: LogLevel, message: impl fmt::Display) {
    let message = message.to_string();
    emit_log(&LogRecord::new(level, &message, current_context()));
}

pub fn replace_error_reporter(reporter: fn(&str)) -> LoggerHandle {
    replace_logger(move |record: &LogRecord<'_>| reporter(&record.to_string()))
}

fn current_context() -> LogContext {
    if let Some(time) = runtime_report_time() {
        return LogContext::Runtime(time);
    }

    if crate::registration::is_active() {
        LogContext::Registration
    } else {
        LogContext::Unscoped
    }
}
