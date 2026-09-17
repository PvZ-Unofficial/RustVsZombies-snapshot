//! Script-facing diagnostics.

#[cfg(all(test, feature = "pvz-emulator"))]
use rsvz_game::diagnostics::LogLevel;
#[cfg(test)]
pub use rsvz_game::diagnostics::error;
pub use rsvz_game::diagnostics::{log, replace_error_reporter};

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::panic;

    use crate::runtime::RuntimeError;

    use super::*;

    thread_local! {
        static CAPTURED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }

    fn capture(message: &str) {
        CAPTURED.with(|captured| captured.borrow_mut().push(message.to_owned()));
    }

    fn panic_reporter(_message: &str) {
        panic!("reporter panic probe");
    }

    #[test]
    fn installed_reporter_receives_a_localized_error_header() {
        CAPTURED.with(|captured| captured.borrow_mut().clear());
        let previous = replace_error_reporter(capture);
        error(RuntimeError::new("expected report"));
        rsvz_game::diagnostics::restore_logger(previous);

        CAPTURED.with(|captured| assert_eq!(captured.borrow().as_slice(), ["[错误] expected report"]));
    }

    #[test]
    fn registration_reports_are_labeled_without_runtime_time() {
        CAPTURED.with(|captured| captured.borrow_mut().clear());
        let previous = replace_error_reporter(capture);
        crate::registration::run_script(|| {
            error(RuntimeError::new("registration failure"));
            Ok(())
        })
        .expect("the report itself does not fail registration");
        rsvz_game::diagnostics::restore_logger(previous);

        CAPTURED.with(|captured| {
            assert_eq!(captured.borrow().as_slice(), ["[注册期][错误] registration failure"]);
        });
    }

    #[test]
    fn reporter_panic_is_contained() {
        let previous = replace_error_reporter(panic_reporter);
        let result = panic::catch_unwind(|| error(RuntimeError::new("fallback report")));
        rsvz_game::diagnostics::restore_logger(previous);

        assert!(result.is_ok());
    }

    #[cfg(feature = "pvz-emulator")]
    #[test]
    fn operation_errors_mark_the_current_dispatch_recoverable() {
        rsvz_game::session::reset_session_control();
        let previous = replace_error_reporter(capture);

        rsvz_game::diagnostics::report_operation_error(RuntimeError::new("recoverable failure"));

        rsvz_game::diagnostics::restore_logger(previous);
        assert_eq!(
            rsvz_game::session::take_dispatch_outcome(),
            Some(rsvz_game::session::DispatchOutcome::RecoverableError)
        );
    }

    #[cfg(feature = "pvz-emulator")]
    #[test]
    fn explicit_error_log_does_not_mark_the_dispatch_failed() {
        rsvz_game::session::reset_session_control();
        let previous = replace_error_reporter(capture);

        log(LogLevel::Error, "diagnostic only");
        error("diagnostic only via error");

        rsvz_game::diagnostics::restore_logger(previous);
        assert_eq!(rsvz_game::session::take_dispatch_outcome(), None);
    }

    #[cfg(feature = "pvz-emulator")]
    #[test]
    fn operation_error_invalidates_an_event_measure_trial() {
        use rsvz_game::event_measure::{
            EventMeasureConfig, EventMeasureInterruption, EventMeasureTask, EventMeasureTaskControl,
        };
        use rsvz_model::model::{MeasureLimit, MeasureMode, ProtectionPolicy, SessionShard};

        rsvz_game::session::reset_session_control();
        let previous = replace_error_reporter(capture);
        rsvz_game::diagnostics::report_operation_error(RuntimeError::new("card failed"));
        let interrupted = match rsvz_game::session::take_dispatch_outcome() {
            Some(rsvz_game::session::DispatchOutcome::RecoverableError) => EventMeasureInterruption::RecoverableError,
            Some(rsvz_game::session::DispatchOutcome::TimingViolation) => EventMeasureInterruption::TimingViolation,
            None => panic!("direct error did not record a dispatch outcome"),
        };
        let mut task = EventMeasureTask::new(
            MeasureLimit::trials(1).expect("limit"),
            EventMeasureConfig::new(MeasureMode::Pogo, ProtectionPolicy::default(), None).expect("config"),
            0,
            SessionShard {
                index: 0,
                count: 1,
                seed_base: 0,
            },
            0,
        );
        let EventMeasureTaskControl::Complete(artifact) =
            task.tick(1, 0, 0, Some(interrupted)).expect("recoverable direct error")
        else {
            panic!("one invalid trial should complete the requested quota")
        };
        let mut json = Vec::new();
        artifact.write_json(&mut json).expect("event measure JSON");
        let json = String::from_utf8(json).expect("UTF-8 JSON");
        assert!(json.contains(r#""attempted_trials":1"#));
        assert!(json.contains(r#""invalid_trials":1"#));
        rsvz_game::diagnostics::restore_logger(previous);
    }
}
