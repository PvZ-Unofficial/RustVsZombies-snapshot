use std::cell::RefCell;
use std::rc::Rc;

use crate::model::{
    AssumedWavelength, GameUi, RelativeTime, Wave, WaveTimingError, WaveTimingSnapshot, WavelengthCheck,
    WavelengthMode, WavelengthValidation,
};
use crate::tick::{TickMeta, TickPhase};

use super::*;

use rsvz_backend_api::error::RuntimeError;

#[derive(Default)]
struct TimingInput {
    clock: i32,
    wave: Wave,
    refresh_countdown: Option<i32>,
    total_waves: Option<i32>,
}

impl TimingInput {
    fn snapshot(&self) -> WaveTimingSnapshot {
        let mut snapshot = WaveTimingSnapshot::minimal(self.clock, self.wave);
        snapshot.refresh_countdown = self.refresh_countdown;
        snapshot.total_waves = self.total_waves;
        snapshot
    }
}

fn force<I: IntoIterator<Item = (i32, i32)>>(
    timeline: &mut Timeline, items: I,
) -> Result<Vec<AssumedWavelength>, TimelineAssumptionError> {
    timeline.apply_forced_wavelength_declarations(
        items
            .into_iter()
            .map(|(wave, length)| crate::model::WavelengthDeclaration::forced(Wave(wave), length)),
        |_, _| || Ok(()),
    )
}

type Controls = Rc<RefCell<Vec<(Wave, i32)>>>;
fn force_recording<I: IntoIterator<Item = (i32, i32)>>(
    timeline: &mut Timeline, items: I, controls: &Controls,
) -> Result<Vec<AssumedWavelength>, TimelineAssumptionError> {
    timeline.apply_forced_wavelength_declarations(
        items
            .into_iter()
            .map(|(wave, length)| crate::model::WavelengthDeclaration::forced(Wave(wave), length)),
        |wave, countdown| {
            let controls = Rc::clone(controls);
            move || {
                controls.borrow_mut().push((wave, countdown));
                Ok(())
            }
        },
    )
}

fn playing_meta(clock: i32) -> TickMeta {
    TickMeta {
        phase: TickPhase::Playing,
        game_ui: Some(GameUi::Playing),
        clock: Some(clock),
        is_new_frame: true,
    }
}

type Log = Rc<RefCell<Vec<&'static str>>>;

fn push_op(log: &Log, label: &'static str) -> impl FnMut() -> Result<(), RuntimeError> + 'static {
    let log = Rc::clone(log);
    move || {
        log.borrow_mut().push(label);
        Ok(())
    }
}

fn logged(log: &Log) -> Vec<&'static str> {
    log.borrow().clone()
}

fn handle(registration: Result<TimelineRegistration, TimelineRegistrationError>) -> TimeHandle {
    registration.expect("timeline registration should succeed").handle()
}

fn register(registration: Result<TimelineRegistration, TimelineRegistrationError>) {
    let _registration = registration.expect("timeline registration should succeed");
}

fn dispatch(timeline: &mut Timeline, input: &mut TimingInput, clock: i32) -> TimelineDispatchResult {
    input.clock = clock;
    timeline.dispatch_tick_catching(input.snapshot(), playing_meta(clock))
}

#[test]
fn runs_same_wave_operations_by_due_time() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 100,
        wave: Wave(1),
        ..TimingInput::default()
    };
    let log = Log::default();
    register(timeline.at(Wave(1), 5, push_op(&log, "late")));
    register(timeline.at(Wave(1), 2, push_op(&log, "early")));

    assert_eq!(
        dispatch(&mut timeline, &mut input, 100),
        TimelineDispatchResult::Continue
    );
    assert_eq!(
        dispatch(&mut timeline, &mut input, 102),
        TimelineDispatchResult::Continue
    );
    assert_eq!(logged(&log), ["early"]);
    assert_eq!(
        dispatch(&mut timeline, &mut input, 105),
        TimelineDispatchResult::Continue
    );
    assert_eq!(logged(&log), ["early", "late"]);
}

#[test]
fn preserves_registration_order_at_same_time() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 10,
        wave: Wave(1),
        ..TimingInput::default()
    };
    let log = Log::default();
    register(timeline.at(Wave(1), 0, push_op(&log, "a")));
    register(timeline.at(Wave(1), 0, push_op(&log, "b")));

    assert_eq!(
        dispatch(&mut timeline, &mut input, 10),
        TimelineDispatchResult::Continue
    );
    assert_eq!(logged(&log), ["a", "b"]);
}

#[test]
fn batch_registration_keeps_global_same_time_order() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 10,
        wave: Wave(1),
        ..TimingInput::default()
    };
    let log = Log::default();
    let time = RelativeTime::new(Wave(1), 0);

    register(timeline.at_time_runtime(time, push_op(&log, "raw-before")));
    let entries: Vec<(RelativeTime, Box<dyn FnMut() -> Result<(), RuntimeError>>)> = vec![
        (time, Box::new(push_op(&log, "batch-1"))),
        (time, Box::new(push_op(&log, "batch-2"))),
    ];
    let _ = timeline
        .at_times_runtime(entries)
        .expect("batch registration should succeed");
    register(timeline.at_time_runtime(time, push_op(&log, "raw-after")));

    assert_eq!(
        dispatch(&mut timeline, &mut input, 10),
        TimelineDispatchResult::Continue
    );
    assert_eq!(logged(&log), ["raw-before", "batch-1", "batch-2", "raw-after"]);
}

#[test]
fn stopped_operation_does_not_run() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 0,
        wave: Wave(1),
        ..TimingInput::default()
    };
    let log = Log::default();
    let handle = handle(timeline.after(5, push_op(&log, "stopped")));

    assert_eq!(timeline.stop(handle), TimeCommandOutcome::Applied);
    assert_eq!(dispatch(&mut timeline, &mut input, 5), TimelineDispatchResult::Continue);
    assert!(log.borrow().is_empty(), "stopped operation should not run");
}

#[test]
fn dynamic_immediate_operation_runs_in_same_drain() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 7,
        wave: Wave(1),
        ..TimingInput::default()
    };
    let log = Log::default();
    register(timeline.after(0, push_op(&log, "outer")));
    register(timeline.after(0, push_op(&log, "inner")));

    assert_eq!(dispatch(&mut timeline, &mut input, 7), TimelineDispatchResult::Continue);
    assert_eq!(logged(&log), ["outer", "inner"]);
}

#[test]
fn operation_error_stops_drain_without_dropping_remaining_due_ops() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 100,
        wave: Wave(1),
        ..TimingInput::default()
    };
    let log = Log::default();
    let error_log = Rc::clone(&log);
    register(timeline.at(Wave(1), 0, move || {
        error_log.borrow_mut().push("error");
        Err(RuntimeError::new("test error"))
    }));
    register(timeline.at(Wave(1), 0, push_op(&log, "after")));

    assert_eq!(
        dispatch(&mut timeline, &mut input, 100),
        TimelineDispatchResult::OperationError(RuntimeError::new("test error"))
    );
    assert_eq!(logged(&log), ["error"]);
    assert_eq!(timeline.diagnostics().pending_count, 1);

    assert_eq!(
        dispatch(&mut timeline, &mut input, 100),
        TimelineDispatchResult::Continue
    );
    assert_eq!(logged(&log), ["error", "after"]);
    assert!(timeline.is_idle());
}

#[test]
fn split_runtime_dispatch_end_clears_dispatching_after_callback_error() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 100,
        wave: Wave(1),
        ..TimingInput::default()
    };
    let log = Log::default();
    register(timeline.at(Wave(1), 0, || Err(RuntimeError::new("test error"))));
    register(timeline.at(Wave(1), 0, push_op(&log, "after")));

    let snapshot = WaveTimingSnapshot::minimal(100, Wave(1));
    let RuntimeTimelineDispatchStart::Drain { clock } =
        timeline.begin_runtime_dispatch_tick_catching(snapshot, playing_meta(100))
    else {
        panic!("dispatch should drain due operations");
    };
    assert!(timeline.is_dispatching());

    let op = timeline
        .take_due_runtime_op(clock)
        .expect("first due operation should be available");
    assert_eq!(
        Timeline::run_runtime_op_catching(op),
        TimelineDispatchResult::OperationError(RuntimeError::new("test error"))
    );
    assert!(log.borrow().is_empty());
    assert_eq!(timeline.diagnostics().pending_count, 1);

    timeline.end_runtime_dispatch();
    assert!(!timeline.is_dispatching());

    assert_eq!(
        dispatch(&mut timeline, &mut input, 100),
        TimelineDispatchResult::Continue
    );
    assert_eq!(logged(&log), ["after"]);
}

#[test]
fn ordinary_operation_runs_without_timeline_context() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 7,
        wave: Wave(1),
        ..TimingInput::default()
    };

    register(timeline.after(0, || Ok(())));

    assert_eq!(dispatch(&mut timeline, &mut input, 7), TimelineDispatchResult::Continue);
    assert!(timeline.is_idle());
}

#[test]
fn runtime_dispatch_uses_precollected_timing_snapshot() {
    let mut timeline = Timeline::new();
    let input = TimingInput {
        clock: 100,
        wave: Wave(1),
        ..TimingInput::default()
    };
    let snapshot = input.snapshot();
    register(timeline.at_time_runtime(RelativeTime::new(Wave(1), 0), || Ok(())));

    assert!(matches!(
        timeline.dispatch_tick_catching(snapshot, playing_meta(100)),
        TimelineDispatchResult::Continue
    ));
    assert!(timeline.is_idle());
}

#[test]
fn dynamic_future_operation_waits_for_due_clock() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 7,
        wave: Wave(1),
        ..TimingInput::default()
    };
    let log = Log::default();
    register(timeline.after(0, push_op(&log, "outer")));
    register(timeline.after(3, push_op(&log, "inner")));

    assert_eq!(dispatch(&mut timeline, &mut input, 7), TimelineDispatchResult::Continue);
    assert_eq!(logged(&log), ["outer"]);
    assert_eq!(dispatch(&mut timeline, &mut input, 9), TimelineDispatchResult::Continue);
    assert_eq!(logged(&log), ["outer"]);
    assert_eq!(
        dispatch(&mut timeline, &mut input, 10),
        TimelineDispatchResult::Continue
    );
    assert_eq!(logged(&log), ["outer", "inner"]);
}

#[test]
fn pending_negative_time_uses_next_wave_countdown() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 90,
        wave: Wave(1),
        refresh_countdown: Some(10),
        ..TimingInput::default()
    };
    let log = Log::default();
    register(timeline.at(Wave(2), -3, push_op(&log, "pre-wave")));

    assert_eq!(
        dispatch(&mut timeline, &mut input, 90),
        TimelineDispatchResult::Continue
    );
    assert!(log.borrow().is_empty(), "operation should not run before due");
    assert_eq!(
        dispatch(&mut timeline, &mut input, 96),
        TimelineDispatchResult::Continue
    );
    assert!(log.borrow().is_empty(), "operation should not run before due");
    assert_eq!(
        dispatch(&mut timeline, &mut input, 97),
        TimelineDispatchResult::Continue
    );
    assert_eq!(logged(&log), ["pre-wave"]);
}

#[test]
fn negative_time_waits_when_countdown_exists_but_is_not_trusted_yet() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 90,
        wave: Wave(1),
        refresh_countdown: Some(500),
        ..TimingInput::default()
    };
    let log = Log::default();
    register(timeline.at(Wave(2), -3, push_op(&log, "pre-wave")));

    assert_eq!(
        dispatch(&mut timeline, &mut input, 90),
        TimelineDispatchResult::Continue
    );
    assert!(
        log.borrow().is_empty(),
        "operation should wait until countdown enters the trusted range"
    );

    input.refresh_countdown = Some(10);
    assert_eq!(
        dispatch(&mut timeline, &mut input, 97),
        TimelineDispatchResult::Continue
    );
    assert!(log.borrow().is_empty(), "operation should not run before due");
    assert_eq!(
        dispatch(&mut timeline, &mut input, 104),
        TimelineDispatchResult::Continue
    );
    assert_eq!(logged(&log), ["pre-wave"]);
}

#[test]
fn missing_countdown_waits_for_negative_time() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 90,
        wave: Wave(1),
        ..TimingInput::default()
    };
    register(timeline.at(Wave(2), -3, || Ok(())));

    assert_eq!(
        dispatch(&mut timeline, &mut input, 90),
        TimelineDispatchResult::Continue
    );
    assert!(timeline.diagnostics().timing_violation.is_none());
    assert!(!timeline.is_idle());
}

#[test]
fn pending_after_preserves_handle_and_can_be_stopped_after_conversion() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 7,
        wave: Wave(1),
        ..TimingInput::default()
    };
    let log = Log::default();
    let handle = handle(timeline.after(3, push_op(&log, "delayed")));

    assert_eq!(dispatch(&mut timeline, &mut input, 7), TimelineDispatchResult::Continue);
    assert_eq!(timeline.stop(handle), TimeCommandOutcome::Applied);
    assert_eq!(
        dispatch(&mut timeline, &mut input, 10),
        TimelineDispatchResult::Continue
    );
    assert!(log.borrow().is_empty());
    assert!(timeline.is_idle());
}

#[test]
fn stopped_pending_after_does_not_convert() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 7,
        wave: Wave(1),
        ..TimingInput::default()
    };
    let log = Log::default();
    let handle = handle(timeline.after(0, push_op(&log, "stopped")));

    assert_eq!(timeline.stop(handle), TimeCommandOutcome::Applied);
    assert_eq!(dispatch(&mut timeline, &mut input, 7), TimelineDispatchResult::Continue);
    assert!(log.borrow().is_empty());
    assert!(timeline.is_idle());
}

#[test]
fn cross_wave_dispatch_follows_wave_key_order() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 100,
        wave: Wave(1),
        ..TimingInput::default()
    };
    timeline.wave_clocks.record_refresh_clock(Wave(1), 100);
    timeline.wave_clocks.record_refresh_clock(Wave(2), 0);
    let log = Log::default();
    register(timeline.at(Wave(2), 100, push_op(&log, "wave-2")));
    register(timeline.at(Wave(1), 0, push_op(&log, "wave-1")));

    assert_eq!(
        dispatch(&mut timeline, &mut input, 100),
        TimelineDispatchResult::Continue
    );
    assert_eq!(logged(&log), ["wave-1", "wave-2"]);
}

#[test]
fn registration_validation_rejects_negative_wave_and_delay() {
    let mut timeline = Timeline::new();

    assert_eq!(
        timeline.at(Wave(-1), 0, || Ok(())).map(TimelineRegistration::handle),
        Err(TimelineRegistrationError::InvalidWave { wave: -1 })
    );
    assert_eq!(
        timeline.after(-1, || Ok(())).map(TimelineRegistration::handle),
        Err(TimelineRegistrationError::InvalidDelay { frames: -1 })
    );
    assert!(timeline.is_idle());
}

#[test]
fn invalid_batch_registration_does_not_change_queue_or_sequence_counters() {
    let mut timeline = Timeline::new();
    let before = timeline.diagnostics();
    let next_id = timeline.next_id;
    let next_order = timeline.next_order;
    let entries: Vec<(RelativeTime, Box<dyn FnMut() -> Result<(), RuntimeError>>)> = vec![
        (RelativeTime::new(Wave(1), 0), Box::new(|| Ok(()))),
        (RelativeTime::new(Wave(-1), 0), Box::new(|| Ok(()))),
    ];

    let Err(error) = timeline.at_times_runtime(entries) else {
        panic!("batch with an invalid later time must fail")
    };

    assert_eq!(error, TimelineRegistrationError::InvalidWave { wave: -1 });
    assert_eq!(timeline.diagnostics(), before);
    assert_eq!(timeline.next_id, next_id);
    assert_eq!(timeline.next_order, next_order);
}

#[test]
fn core_accepts_wave_zero_registration() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 10,
        wave: Wave(0),
        ..TimingInput::default()
    };
    let log = Log::default();

    register(timeline.at(Wave(0), 0, push_op(&log, "wave-zero")));

    assert_eq!(
        dispatch(&mut timeline, &mut input, 10),
        TimelineDispatchResult::Continue
    );
    assert_eq!(logged(&log), ["wave-zero"]);
}

#[test]
fn same_clock_registration_returns_immediate_work() {
    let mut timeline = Timeline::new();
    let snapshot = WaveTimingSnapshot::minimal(10, Wave(1));
    let log = Log::default();

    assert!(matches!(
        timeline.dispatch_tick_catching(snapshot, playing_meta(10)),
        TimelineDispatchResult::Continue
    ));
    let log_for_callback = Rc::clone(&log);
    let registration = timeline
        .at(Wave(1), 0, move || {
            log_for_callback.borrow_mut().push("immediate");
            Ok(())
        })
        .expect("same-clock registration should succeed");
    let TimelineRegistration::Immediate { handle, op } = registration else {
        panic!("same-clock registration should be immediate");
    };
    assert!(matches!(
        Timeline::run_runtime_op_catching(op),
        TimelineDispatchResult::Continue
    ));
    assert_eq!(timeline.stop(handle), TimeCommandOutcome::AlreadyStopped);
    assert_eq!(logged(&log), ["immediate"]);
}

#[test]
fn same_clock_batch_registration_defers_work_in_input_order() {
    let mut timeline = Timeline::new();
    let snapshot = WaveTimingSnapshot::minimal(10, Wave(1));
    let log = Log::default();
    assert!(matches!(
        timeline.dispatch_tick_catching(snapshot, playing_meta(10)),
        TimelineDispatchResult::Continue
    ));
    let time = RelativeTime::new(Wave(1), 0);
    let first_log = Rc::clone(&log);
    let second_log = Rc::clone(&log);
    let entries: Vec<(RelativeTime, Box<dyn FnMut() -> Result<(), RuntimeError>>)> = vec![
        (
            time,
            Box::new(move || {
                first_log.borrow_mut().push("first");
                Ok(())
            }),
        ),
        (
            time,
            Box::new(move || {
                second_log.borrow_mut().push("second");
                Ok(())
            }),
        ),
    ];

    for registration in timeline
        .at_times_runtime(entries)
        .expect("same-clock batch registration should succeed")
    {
        let TimelineRegistration::Queued(_handle) = registration else {
            panic!("same-clock batch registration should defer until dispatch finishes registration");
        };
    }
    assert!(matches!(
        timeline.dispatch_tick_catching(snapshot, playing_meta(10)),
        TimelineDispatchResult::Continue
    ));
    assert_eq!(logged(&log), ["first", "second"]);
}

#[test]
fn active_past_registration_returns_error() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 10,
        wave: Wave(1),
        ..TimingInput::default()
    };

    assert_eq!(
        dispatch(&mut timeline, &mut input, 10),
        TimelineDispatchResult::Continue
    );

    assert_eq!(
        timeline.at(Wave(1), -1, || Ok(())).map(TimelineRegistration::handle),
        Err(TimelineRegistrationError::PastDue {
            target: RelativeTime::new(Wave(1), -1),
            due_clock: 9,
            current_clock: 10,
        })
    );
}

#[test]
fn activated_older_wave_future_clock_registration_is_allowed() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 100,
        wave: Wave(2),
        ..TimingInput::default()
    };
    let log = Log::default();
    timeline.wave_clocks.record_refresh_clock(Wave(1), 0);

    assert_eq!(
        dispatch(&mut timeline, &mut input, 100),
        TimelineDispatchResult::Continue
    );

    register(timeline.at(Wave(1), 150, push_op(&log, "future-old-wave")));

    assert_eq!(
        dispatch(&mut timeline, &mut input, 149),
        TimelineDispatchResult::Continue
    );
    assert_eq!(logged(&log), Vec::<&str>::new());

    assert_eq!(
        dispatch(&mut timeline, &mut input, 150),
        TimelineDispatchResult::Continue
    );
    assert_eq!(logged(&log), vec!["future-old-wave"]);
}

#[test]
fn known_total_waves_accepts_post_final_and_rejects_beyond() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 0,
        wave: Wave(1),
        total_waves: Some(20),
        ..TimingInput::default()
    };

    assert_eq!(dispatch(&mut timeline, &mut input, 0), TimelineDispatchResult::Continue);

    register(timeline.at(Wave(21), 0, || Ok(())));
    assert_eq!(
        timeline.at(Wave(22), 0, || Ok(())).map(TimelineRegistration::handle),
        Err(TimelineRegistrationError::WaveOutOfRange {
            wave: 22,
            total_waves: 20,
        })
    );
}

#[test]
fn first_total_waves_snapshot_prunes_out_of_range_queue() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 0,
        wave: Wave(1),
        ..TimingInput::default()
    };
    register(timeline.at(Wave(200), 0, || Ok(())));

    input.total_waves = Some(20);
    assert_eq!(dispatch(&mut timeline, &mut input, 0), TimelineDispatchResult::Continue);

    let diagnostics = timeline.diagnostics();
    assert_eq!(diagnostics.total_wave_pruned_queues, 1);
    assert_eq!(diagnostics.total_wave_pruned_ops, 1);
    assert!(timeline.is_idle());
}

#[test]
fn prime_total_waves_prunes_without_activating_or_converting_pending_after() {
    let mut timeline = Timeline::new();
    register(timeline.at(Wave(200), 0, || Ok(())));
    register(timeline.after(10, || Ok(())));

    timeline.prime_total_waves(20);

    let diagnostics = timeline.diagnostics();
    assert_eq!(diagnostics.total_waves, Some(20));
    assert_eq!(diagnostics.current_clock, None);
    assert_eq!(diagnostics.current_wave, None);
    assert_eq!(diagnostics.start_clock_floor, None);
    assert_eq!(diagnostics.start_relative_floor, None);
    assert_eq!(diagnostics.total_wave_pruned_queues, 1);
    assert_eq!(diagnostics.total_wave_pruned_ops, 1);
    assert_eq!(diagnostics.pending_count, 1);
}

#[test]
fn assumed_wavelength_resolves_future_negative_time() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 0,
        wave: Wave(1),
        ..TimingInput::default()
    };
    assert_eq!(timeline.assume_wavelengths([(1, 601), (2, 1800)]), Ok(()));
    let log = Log::default();
    register(timeline.at(Wave(2), -10, push_op(&log, "assumed")));

    assert_eq!(dispatch(&mut timeline, &mut input, 0), TimelineDispatchResult::Continue);
    assert_eq!(
        dispatch(&mut timeline, &mut input, 590),
        TimelineDispatchResult::Continue
    );
    assert!(log.borrow().is_empty(), "operation should not run early");
    assert_eq!(
        dispatch(&mut timeline, &mut input, 591),
        TimelineDispatchResult::Continue
    );
    assert_eq!(logged(&log), ["assumed"]);
}

#[test]
fn tuple_assumed_wavelengths_are_host_default_non_forcing_declarations() {
    let mut timeline = Timeline::new();

    assert_eq!(timeline.assume_wavelengths([(1, 601)]), Ok(()));

    let declaration = timeline
        .wave_clocks
        .wavelength_declaration(Wave(1))
        .expect("declaration should be recorded");
    assert_eq!(declaration.mode, WavelengthMode::Assumed);
    assert_eq!(declaration.validation, WavelengthValidation::HostDefault);
    assert_eq!(timeline.wave_clocks.assumed_wavelength(Wave(1)), Some(601));
}

#[test]
fn unchecked_wavelength_basis_resolves_time_without_strict_mismatch() {
    let mut timeline = Timeline::with_timing_violation_policy(TimingViolationPolicy::ReportFailure);
    let mut input = TimingInput {
        clock: 100,
        wave: Wave(1),
        ..TimingInput::default()
    };
    assert_eq!(timeline.use_wavelength_basis([(1, 601)]), Ok(()));
    let log = Log::default();
    register(timeline.at(Wave(2), -1, push_op(&log, "basis")));

    assert_eq!(
        dispatch(&mut timeline, &mut input, 100),
        TimelineDispatchResult::Continue
    );
    input.wave = Wave(2);
    assert_eq!(
        dispatch(&mut timeline, &mut input, 800),
        TimelineDispatchResult::Continue
    );
    assert_eq!(logged(&log), ["basis"]);
    assert_eq!(timeline.diagnostics().timing_violation, None);
}

#[test]
fn strict_policy_reports_assumed_wavelength_mismatch_in_catching_dispatch() {
    let mut timeline = Timeline::with_timing_violation_policy(TimingViolationPolicy::ReportFailure);
    let mut input = TimingInput {
        clock: 100,
        wave: Wave(1),
        ..TimingInput::default()
    };
    assert_eq!(timeline.assume_wavelengths([(1, 601)]), Ok(()));

    assert_eq!(
        dispatch(&mut timeline, &mut input, 100),
        TimelineDispatchResult::Continue
    );
    input.wave = Wave(2);
    assert_eq!(
        dispatch(&mut timeline, &mut input, 800),
        TimelineDispatchResult::TimingViolation(TimelineTimingViolation::WavelengthMismatch(
            TimelineAssumptionMismatch {
                wave: Wave(1),
                assumed: 601,
                actual: 700,
            }
        ))
    );
}

#[test]
fn assumed_wavelength_refresh_delay_is_not_reported_before_deadline() {
    let mut timeline = Timeline::with_timing_violation_policy(TimingViolationPolicy::ReportFailure);
    let mut input = TimingInput {
        clock: 400,
        wave: Wave(1),
        refresh_countdown: Some(500),
        ..TimingInput::default()
    };
    assert_eq!(timeline.assume_wavelengths([(1, 601)]), Ok(()));
    timeline.wave_clocks.record_refresh_clock(Wave(1), 0);

    assert_eq!(
        dispatch(&mut timeline, &mut input, 400),
        TimelineDispatchResult::Continue
    );
    assert_eq!(timeline.diagnostics().timing_violation, None);
}

#[test]
fn assumed_wavelength_refresh_delay_reports_at_deadline() {
    let mut timeline = Timeline::with_timing_violation_policy(TimingViolationPolicy::ReportFailure);
    let mut input = TimingInput {
        clock: 401,
        wave: Wave(1),
        refresh_countdown: Some(500),
        ..TimingInput::default()
    };
    assert_eq!(timeline.assume_wavelengths([(1, 601)]), Ok(()));
    timeline.wave_clocks.record_refresh_clock(Wave(1), 0);

    assert_eq!(
        dispatch(&mut timeline, &mut input, 401),
        TimelineDispatchResult::TimingViolation(TimelineTimingViolation::RefreshDelay(TimelineRefreshDelay {
            wave: Wave(1),
            assumed: 601,
            expected_next_refresh: 601,
            deadline_clock: 401,
            current_clock: 401,
        }))
    );
}

#[test]
fn assumed_wavelength_refresh_delay_records_under_record_only_policy() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 401,
        wave: Wave(1),
        refresh_countdown: Some(500),
        ..TimingInput::default()
    };
    assert_eq!(timeline.assume_wavelengths([(1, 601)]), Ok(()));
    timeline.wave_clocks.record_refresh_clock(Wave(1), 0);

    assert_eq!(
        dispatch(&mut timeline, &mut input, 401),
        TimelineDispatchResult::Continue
    );
    assert_eq!(
        timeline.diagnostics().timing_violation,
        Some(TimelineTimingViolation::RefreshDelay(TimelineRefreshDelay {
            wave: Wave(1),
            assumed: 601,
            expected_next_refresh: 601,
            deadline_clock: 401,
            current_clock: 401,
        }))
    );
}

#[test]
fn confirmed_next_wave_countdown_suppresses_refresh_delay() {
    let mut timeline = Timeline::with_timing_violation_policy(TimingViolationPolicy::ReportFailure);
    let mut input = TimingInput {
        clock: 401,
        wave: Wave(1),
        refresh_countdown: Some(200),
        ..TimingInput::default()
    };
    assert_eq!(timeline.assume_wavelengths([(1, 601)]), Ok(()));
    timeline.wave_clocks.record_refresh_clock(Wave(1), 0);

    assert_eq!(
        dispatch(&mut timeline, &mut input, 401),
        TimelineDispatchResult::Continue
    );
    assert_eq!(timeline.diagnostics().timing_violation, None);
    assert_eq!(timeline.wave_clocks.observed_refresh_clock(Wave(2)), Some(601));
}

#[test]
fn forced_wavelength_resolves_future_negative_time() {
    let controls = Controls::default();
    let mut timeline = Timeline::new();
    timeline.prime_total_waves(20);
    let mut input = TimingInput {
        clock: 0,
        wave: Wave(1),
        ..TimingInput::default()
    };
    assert_eq!(
        force_recording(&mut timeline, [(1, 601), (2, 1800)], &controls),
        Ok(vec![
            AssumedWavelength {
                wave: Wave(1),
                length: 601,
            },
            AssumedWavelength {
                wave: Wave(2),
                length: 1800,
            },
        ])
    );
    let log = Log::default();
    register(timeline.at(Wave(2), -10, push_op(&log, "forced")));

    assert_eq!(dispatch(&mut timeline, &mut input, 0), TimelineDispatchResult::Continue);
    assert_eq!(dispatch(&mut timeline, &mut input, 1), TimelineDispatchResult::Continue);
    assert_eq!(controls.borrow().as_slice(), [(Wave(1), 601)]);
    assert_eq!(
        dispatch(&mut timeline, &mut input, 591),
        TimelineDispatchResult::Continue
    );
    assert_eq!(logged(&log), ["forced"]);
}

#[test]
fn forced_wavelength_requires_known_total_without_partial_registration() {
    let mut timeline = Timeline::new();

    assert_eq!(
        force(&mut timeline, [(0, 601)]),
        Err(TimelineAssumptionError::Timing(WaveTimingError::TotalWavesUnknown {
            wave: Wave(0),
        }))
    );
    assert_eq!(timeline.wave_clocks.wavelength_declaration(Wave(0)), None);
    assert_eq!(timeline.diagnostics().pending_count, 0);
}

#[test]
fn wavelength_bounds_cover_wave_zero_all_flag_waves_and_final_wave() {
    assert_eq!(recommended_wavelength_bounds(Wave(0), Some(20)), Ok((601, 3100)));
    assert_eq!(recommended_wavelength_bounds(Wave(9), Some(40)), Ok((1346, 5245)));
    assert_eq!(recommended_wavelength_bounds(Wave(19), Some(40)), Ok((1346, 5245)));
    assert_eq!(recommended_wavelength_bounds(Wave(29), Some(40)), Ok((1346, 5245)));
    assert_eq!(recommended_wavelength_bounds(Wave(20), Some(20)), Ok((500, 5999)));
    assert!(matches!(
        recommended_wavelength_bounds(Wave(-1), Some(20)),
        Err(WaveTimingError::InvalidWave(Wave(-1)))
    ));
}

#[test]
fn forced_wavelength_above_recommended_is_warned_and_registered() {
    let mut timeline = Timeline::new();
    timeline.prime_total_waves(20);

    assert_eq!(
        force(&mut timeline, [(1, 4000)]),
        Ok(vec![AssumedWavelength {
            wave: Wave(1),
            length: 4000,
        }])
    );
    assert_eq!(timeline.diagnostics().pending_count, 1);
    assert_eq!(
        timeline.diagnostics().assumption_warning,
        Some(TimelineAssumptionWarning::WavelengthAboveRecommended {
            wave: Wave(1),
            length: 4000,
            min: 601,
            max: 3100,
        })
    );
}

#[test]
fn forced_wave_zero_and_flag_wave_commit_canonical_countdowns() {
    let controls = Controls::default();
    let mut wave_zero = Timeline::new();
    wave_zero.prime_total_waves(20);
    assert!(force_recording(&mut wave_zero, [(0, 601)], &controls).is_ok());
    wave_zero.wave_clocks.record_refresh_clock(Wave(0), 0);
    let mut input = TimingInput {
        wave: Wave(0),
        ..TimingInput::default()
    };
    assert_eq!(
        dispatch(&mut wave_zero, &mut input, 1),
        TimelineDispatchResult::Continue
    );
    assert_eq!(controls.borrow().as_slice(), [(Wave(0), 601)]);

    let mut flag_wave = Timeline::new();
    flag_wave.prime_total_waves(20);
    assert!(force_recording(&mut flag_wave, [(9, 1346)], &controls).is_ok());
    flag_wave.wave_clocks.record_refresh_clock(Wave(9), 100);
    input.wave = Wave(9);
    assert_eq!(
        dispatch(&mut flag_wave, &mut input, 101),
        TimelineDispatchResult::Continue
    );
    assert_eq!(controls.borrow().as_slice(), [(Wave(0), 601), (Wave(9), 601)]);
}

#[test]
fn forced_wavelength_batch_failure_rolls_back_declaration_and_control() {
    let mut timeline = Timeline::new();
    timeline.prime_total_waves(20);

    assert!(matches!(
        force(&mut timeline, [(1, 601), (20, 601)]),
        Err(TimelineAssumptionError::Timing(WaveTimingError::FinalWaveForced { .. }))
    ));
    assert_eq!(timeline.wave_clocks.wavelength_declaration(Wave(1)), None);
    assert_eq!(timeline.diagnostics().pending_count, 0);
}

#[test]
fn forced_wavelength_control_is_exact_due_and_control_failure_is_fatal() {
    let mut past_due = Timeline::new();
    past_due.prime_total_waves(20);
    assert!(force(&mut past_due, [(1, 601)]).is_ok());
    past_due.wave_clocks.record_refresh_clock(Wave(1), 0);
    let mut input = TimingInput {
        wave: Wave(1),
        ..TimingInput::default()
    };
    assert!(matches!(
        dispatch(&mut past_due, &mut input, 2),
        TimelineDispatchResult::ControlTimingError(TimelineControlTimingError::PastDue { .. })
    ));

    let mut rejected = Timeline::new();
    rejected.prime_total_waves(20);
    assert!(
        rejected
            .apply_forced_wavelength_declarations(
                [crate::model::WavelengthDeclaration::forced(Wave(1), 601)],
                |_, _| || Err(RuntimeError::new("test error")),
            )
            .is_ok()
    );
    rejected.wave_clocks.record_refresh_clock(Wave(1), 0);
    assert_eq!(
        dispatch(&mut rejected, &mut input, 1),
        TimelineDispatchResult::BackendControlError(RuntimeError::new("test error"))
    );
}

#[test]
fn clear_preserves_forced_control_while_clear_all_removes_it() {
    let controls = Controls::default();
    let mut timeline = Timeline::new();
    timeline.prime_total_waves(20);
    assert!(force_recording(&mut timeline, [(1, 601)], &controls).is_ok());
    assert_eq!(timeline.diagnostics().pending_count, 1);

    timeline.clear();
    assert_eq!(timeline.diagnostics().pending_count, 1);
    assert!(matches!(
        timeline.wave_clocks.wavelength_declaration(Wave(1)),
        Some(declaration) if declaration.mode == WavelengthMode::Forced
    ));
    timeline.wave_clocks.record_refresh_clock(Wave(1), 0);
    let mut input = TimingInput {
        wave: Wave(1),
        ..TimingInput::default()
    };
    assert_eq!(dispatch(&mut timeline, &mut input, 1), TimelineDispatchResult::Continue);
    assert_eq!(controls.borrow().as_slice(), [(Wave(1), 601)]);

    timeline.prime_total_waves(20);
    assert!(force_recording(&mut timeline, [(2, 601)], &controls).is_ok());
    timeline.clear_all();
    assert_eq!(timeline.diagnostics().pending_count, 0);
    assert_eq!(timeline.wave_clocks.wavelength_declaration(Wave(2)), None);
}

#[test]
fn forced_wavelength_upgrades_assumption_and_is_idempotent() {
    let mut timeline = Timeline::new();
    timeline.prime_total_waves(20);
    assert_eq!(timeline.assume_wavelength(Wave(1), 601), Ok(()));

    assert_eq!(
        force(&mut timeline, [(1, 601)]),
        Ok(vec![AssumedWavelength {
            wave: Wave(1),
            length: 601,
        }])
    );
    assert_eq!(force(&mut timeline, [(1, 601)]), Ok(Vec::new()));
    assert_eq!(timeline.assume_wavelength(Wave(1), 601), Ok(()));

    let declaration = timeline
        .wave_clocks
        .wavelength_declaration(Wave(1))
        .expect("declaration should be recorded");
    assert_eq!(declaration.mode, WavelengthMode::Forced);
    assert_eq!(declaration.validation, WavelengthValidation::HostDefault);
}

#[test]
fn forced_wavelength_conflicts_with_different_assumption_length() {
    let mut timeline = Timeline::new();
    timeline.prime_total_waves(20);
    assert_eq!(timeline.assume_wavelength(Wave(1), 601), Ok(()));

    let result = force(&mut timeline, [(1, 700)]);

    assert_eq!(
        result,
        Err(TimelineAssumptionError::Timing(
            WaveTimingError::ConflictingWavelength {
                wave: Wave(1),
                existing: 601,
                requested: 700,
            }
        ))
    );
    assert_eq!(timeline.wave_clocks.assumed_wavelength(Wave(1)), Some(601));
}

#[test]
fn assumed_wavelength_keeps_registration_order() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 0,
        wave: Wave(1),
        ..TimingInput::default()
    };
    assert_eq!(timeline.assume_wavelengths([(1, 601)]), Ok(()));
    let log = Log::default();
    register(timeline.at(Wave(2), -1, push_op(&log, "a")));
    register(timeline.at(Wave(2), -1, push_op(&log, "b")));

    assert_eq!(dispatch(&mut timeline, &mut input, 0), TimelineDispatchResult::Continue);
    assert_eq!(
        dispatch(&mut timeline, &mut input, 600),
        TimelineDispatchResult::Continue
    );
    assert_eq!(logged(&log), ["a", "b"]);
}

#[test]
fn assumed_wavelengths_failure_is_atomic() {
    let mut timeline = Timeline::new();
    let result = timeline.assume_wavelengths([(1, 601), (2, 1)]);

    assert!(result.is_err());
    assert_eq!(timeline.wave_clocks.assumed_wavelength(Wave(1)), None);
    assert_eq!(timeline.wave_clocks.assumed_wavelength(Wave(2)), None);
}

#[test]
fn conflicting_assumed_wavelength_does_not_rewrite_ready_clock_basis() {
    let mut timeline = Timeline::new();
    assert_eq!(timeline.assume_wavelength(Wave(1), 601), Ok(()));

    let result = timeline.assume_wavelength(Wave(1), 700);

    assert_eq!(
        result,
        Err(TimelineAssumptionError::Timing(
            WaveTimingError::ConflictingWavelength {
                wave: Wave(1),
                existing: 601,
                requested: 700,
            }
        ))
    );
    assert_eq!(timeline.wave_clocks.assumed_wavelength(Wave(1)), Some(601));
}

#[test]
fn assumed_wavelength_allows_high_wave_when_total_waves_unknown() {
    let mut timeline = Timeline::new();

    let result = timeline.assume_wavelength(Wave(20), 601);

    assert_eq!(result, Ok(()));
    assert_eq!(timeline.wave_clocks.assumed_wavelength(Wave(20)), Some(601));
}

#[test]
fn known_total_waves_allows_final_assumption_but_rejects_final_forced_wavelength() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 0,
        wave: Wave(1),
        total_waves: Some(20),
        ..TimingInput::default()
    };

    assert_eq!(dispatch(&mut timeline, &mut input, 0), TimelineDispatchResult::Continue);

    assert_eq!(timeline.assume_wavelength(Wave(19), 1346), Ok(()));
    assert_eq!(timeline.assume_wavelength(Wave(20), 500), Ok(()));
    assert_eq!(timeline.wave_clocks.assumed_wavelength(Wave(20)), Some(500));
    assert_eq!(
        force(&mut timeline, [(20, 601)]),
        Err(TimelineAssumptionError::Timing(WaveTimingError::FinalWaveForced {
            wave: Wave(20),
            total_waves: 20,
        }))
    );
}

#[test]
fn high_wavelength_records_warning_without_rejecting_registration() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 0,
        wave: Wave(1),
        total_waves: Some(20),
        ..TimingInput::default()
    };

    assert_eq!(dispatch(&mut timeline, &mut input, 0), TimelineDispatchResult::Continue);

    assert_eq!(timeline.assume_wavelength(Wave(20), 6000), Ok(()));
    assert_eq!(
        timeline.diagnostics().assumption_warning,
        Some(TimelineAssumptionWarning::WavelengthAboveRecommended {
            wave: Wave(20),
            length: 6000,
            min: 500,
            max: 5999,
        })
    );
}

#[test]
fn assumed_wavelength_reports_mismatch() {
    let mut timeline = Timeline::new();
    assert_eq!(timeline.assume_wavelengths([(1, 601)]), Ok(()));
    timeline.wave_clocks.record_refresh_clock(Wave(1), 100);
    timeline.wave_clocks.record_refresh_clock(Wave(2), 800);

    assert_eq!(
        timeline.check_assumed_wavelength(Wave(1)),
        WavelengthCheck::Mismatched {
            assumed: 601,
            actual: 700,
        }
    );
}

#[test]
fn late_assumed_wavelength_mismatch_is_rejected() {
    let mut timeline = Timeline::new();
    timeline.wave_clocks.record_refresh_clock(Wave(1), 100);
    timeline.wave_clocks.record_refresh_clock(Wave(2), 800);

    let result = timeline.assume_wavelength(Wave(1), 601);

    assert_eq!(
        result,
        Err(TimelineAssumptionError::AssumptionMismatch(
            TimelineAssumptionMismatch {
                wave: Wave(1),
                assumed: 601,
                actual: 700,
            }
        ))
    );
    assert_eq!(timeline.wave_clocks.assumed_wavelength(Wave(1)), None);
}

#[test]
fn timeline_clear_preserves_assumed_wavelengths() {
    let mut timeline = Timeline::new();
    assert_eq!(timeline.assume_wavelength(Wave(1), 700), Ok(()));
    timeline.wave_clocks.record_refresh_clock(Wave(1), 100);
    assert_eq!(timeline.wave_clocks.refresh_clock(Wave(2)), Some(800));

    timeline.clear();

    assert_eq!(timeline.wave_clocks.assumed_wavelength(Wave(1)), Some(700));
    assert_eq!(timeline.wave_clocks.observed_refresh_clock(Wave(1)), None);
    assert_eq!(timeline.wave_clocks.assumed_refresh_clock(Wave(2)), None);
    timeline.wave_clocks.record_refresh_clock(Wave(1), 200);
    assert_eq!(timeline.wave_clocks.refresh_clock(Wave(2)), Some(900));
}

#[test]
fn timeline_clear_all_drops_assumed_wavelengths() {
    let mut timeline = Timeline::new();
    assert_eq!(timeline.assume_wavelength(Wave(1), 700), Ok(()));
    timeline.wave_clocks.record_refresh_clock(Wave(1), 100);

    timeline.clear_all();

    assert_eq!(timeline.wave_clocks.assumed_wavelength(Wave(1)), None);
    assert_eq!(timeline.wave_clocks.observed_refresh_clock(Wave(1)), None);
    assert_eq!(timeline.wave_clocks.assumed_refresh_clock(Wave(2)), None);
}

#[test]
fn discard_before_clock_skips_operations_before_late_attach_time() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 100,
        wave: Wave(1),
        ..TimingInput::default()
    };
    timeline.wave_clocks.record_refresh_clock(Wave(1), 100);
    let log = Log::default();
    register(timeline.at(Wave(1), 0, push_op(&log, "past")));
    register(timeline.at(Wave(1), 50, push_op(&log, "future")));

    timeline.discard_before_clock(130);

    assert_eq!(
        dispatch(&mut timeline, &mut input, 160),
        TimelineDispatchResult::Continue
    );
    assert_eq!(logged(&log), ["future"]);
    assert!(timeline.is_idle());
}

#[test]
fn discard_before_relative_time_drops_unresolved_current_wave_operations() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 800,
        wave: Wave(2),
        ..TimingInput::default()
    };
    let log = Log::default();
    register(timeline.at(Wave(1), 50, push_op(&log, "missed")));

    timeline.discard_before_relative_time(RelativeTime::new(Wave(1), i32::MAX), 300);
    timeline.wave_clocks.record_refresh_clock(Wave(1), 100);

    assert_eq!(
        dispatch(&mut timeline, &mut input, 800),
        TimelineDispatchResult::Continue
    );
    assert!(log.borrow().is_empty());
    assert!(timeline.is_idle());
}

#[test]
fn assumed_wavelength_mismatch_does_not_interrupt_catching_dispatch() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 100,
        wave: Wave(1),
        ..TimingInput::default()
    };
    assert_eq!(timeline.assume_wavelengths([(1, 601)]), Ok(()));
    let log = Log::default();
    register(timeline.at(Wave(2), 0, push_op(&log, "should-not-run")));

    assert_eq!(
        dispatch(&mut timeline, &mut input, 100),
        TimelineDispatchResult::Continue
    );
    input.wave = Wave(2);
    assert_eq!(
        dispatch(&mut timeline, &mut input, 800),
        TimelineDispatchResult::Continue
    );
    assert_eq!(
        dispatch(&mut timeline, &mut input, 801),
        TimelineDispatchResult::Continue
    );
    assert_eq!(logged(&log), ["should-not-run"]);
}

#[test]
fn assumption_mismatch_warning_keeps_other_timing_errors_visible() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 100,
        wave: Wave(1),
        ..TimingInput::default()
    };
    assert_eq!(timeline.assume_wavelengths([(1, 601)]), Ok(()));
    let log = Log::default();
    register(timeline.at(Wave(3), -1, push_op(&log, "should-not-run")));

    assert_eq!(
        dispatch(&mut timeline, &mut input, 100),
        TimelineDispatchResult::Continue
    );
    input.wave = Wave(2);

    assert_eq!(
        dispatch(&mut timeline, &mut input, 800),
        TimelineDispatchResult::Continue
    );
    assert!(log.borrow().is_empty());
    assert!(timeline.diagnostics().timing_violation.is_some());
}

#[test]
fn matching_assumed_wavelength_does_not_interrupt_dispatch() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 100,
        wave: Wave(1),
        ..TimingInput::default()
    };
    assert_eq!(timeline.assume_wavelengths([(1, 700)]), Ok(()));
    let log = Log::default();
    register(timeline.at(Wave(2), 0, push_op(&log, "ran")));

    assert_eq!(
        dispatch(&mut timeline, &mut input, 100),
        TimelineDispatchResult::Continue
    );
    input.wave = Wave(2);
    assert_eq!(
        dispatch(&mut timeline, &mut input, 800),
        TimelineDispatchResult::Continue
    );
    assert_eq!(logged(&log), ["ran"]);
}

#[test]
fn one_sided_assumed_wavelength_observation_does_not_report_mismatch() {
    let mut timeline = Timeline::new();
    let mut input = TimingInput {
        clock: 100,
        wave: Wave(1),
        ..TimingInput::default()
    };
    assert_eq!(timeline.assume_wavelengths([(1, 601)]), Ok(()));

    assert_eq!(
        dispatch(&mut timeline, &mut input, 100),
        TimelineDispatchResult::Continue
    );
}

#[test]
fn assumed_wavelength_does_not_override_real_clock() {
    let mut timeline = Timeline::new();
    assert_eq!(timeline.assume_wavelengths([(1, 601)]), Ok(()));
    timeline.wave_clocks.record_refresh_clock(Wave(1), 100);
    timeline.wave_clocks.record_refresh_clock(Wave(2), 900);

    assert_eq!(timeline.wave_clocks.refresh_clock(Wave(2)), Some(900));
    assert_eq!(
        timeline.check_assumed_wavelength(Wave(1)),
        WavelengthCheck::Mismatched {
            assumed: 601,
            actual: 800,
        }
    );
}
