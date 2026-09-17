use std::time::Duration;

use super::*;
use crate::model::RefreshDance;

#[test]
fn measurement_dispatch_classifier_covers_active_inactive_and_reported_paths() {
    let cases = [
        (
            RuntimeFrameDispatch::Continue,
            MeasurementDispatchDecision::ObserveAfterUpdate,
        ),
        (
            RuntimeFrameDispatch::OperationError(RuntimeError::new("test failure")),
            MeasurementDispatchDecision::Finish(MeasureTrialOutcome::Invalid),
        ),
        (
            RuntimeFrameDispatch::OperationPanic,
            MeasurementDispatchDecision::Finish(MeasureTrialOutcome::Invalid),
        ),
        (
            RuntimeFrameDispatch::TimingBackendError(RuntimeError::new("test failure")),
            MeasurementDispatchDecision::Finish(MeasureTrialOutcome::Invalid),
        ),
        (
            RuntimeFrameDispatch::TimingViolation("late".to_owned()),
            MeasurementDispatchDecision::Finish(MeasureTrialOutcome::RefreshFailure),
        ),
        (
            RuntimeFrameDispatch::TimingControlViolation("past due".to_owned()),
            MeasurementDispatchDecision::Finish(MeasureTrialOutcome::RefreshFailure),
        ),
        (
            RuntimeFrameDispatch::FrameCallbackError(RuntimeError::new("test failure")),
            MeasurementDispatchDecision::Finish(MeasureTrialOutcome::Invalid),
        ),
        (
            RuntimeFrameDispatch::FrameCallbackPanic,
            MeasurementDispatchDecision::Finish(MeasureTrialOutcome::Invalid),
        ),
    ];
    for (dispatch, expected) in cases {
        assert_eq!(classify_measurement_dispatch(&dispatch, true, false), expected);
    }
    assert_eq!(
        classify_measurement_dispatch(&RuntimeFrameDispatch::Continue, true, true),
        MeasurementDispatchDecision::Finish(MeasureTrialOutcome::Invalid)
    );
    assert_eq!(
        classify_measurement_dispatch(&RuntimeFrameDispatch::FrameCallbackPanic, false, true),
        MeasurementDispatchDecision::Inactive
    );
}

#[test]
fn measurement_window_uses_absolute_clocks_and_waits_for_unknown_waves() {
    let mut clocks = WaveClockState::new();
    clocks.record_refresh_clock(Wave(1), 100);
    clocks.record_refresh_clock(Wave(2), 800);

    let end = Some(RelativeTime::new(Wave(2), -200));
    assert_eq!(measurement_window_reached(end, Some(599), &clocks), Some(false));
    assert_eq!(measurement_window_reached(end, Some(600), &clocks), Some(true));
    assert_eq!(measurement_window_reached(end, Some(900), &clocks), Some(true));
    assert_eq!(
        measurement_window_reached(Some(RelativeTime::new(Wave(3), 0)), Some(i32::MAX), &clocks),
        Some(false)
    );
    assert_eq!(measurement_window_reached(None, Some(600), &clocks), None);
}

#[test]
fn refresh_window_seals_at_endpoint_and_rejects_an_unreached_level_end() {
    let end = RelativeTime::new(Wave(2), 0);
    let mut clocks = WaveClockState::new();
    clocks.record_refresh_clock(Wave(1), 0);
    clocks.record_refresh_clock(Wave(2), 600);
    let shard = SessionShard {
        index: 0,
        count: 1,
        seed_base: 10,
    };
    let mut task = RefreshTask::new_with_window(
        MeasureLimit::trials(1).expect("limit"),
        RefreshMeasureConfig::default(),
        63,
        shard,
        0,
        Some(end),
    );
    task.tick_at(1, Some(0), &clocks, 0, None).expect("start trial");
    assert!(matches!(
        task.tick_at(1, Some(599), &clocks, 0, None).expect("before endpoint"),
        RefreshTaskControl::Continue
    ));
    let RefreshTaskControl::Complete(artifact) = task.tick_at(1, Some(600), &clocks, 0, None).expect("exact endpoint")
    else {
        panic!("one endpoint trial should complete")
    };
    let mut json = Vec::new();
    artifact.write_json(&mut json).expect("report");
    let report: RefreshReport = serde_json::from_slice(&json).expect("typed report");
    assert_eq!(report.valid_trials, 1);
    assert_eq!(report.window_end, Some(MeasurementWindowEndReport { wave: 2, time: 0 }));

    let mut ended = RefreshTask::new_with_window(
        MeasureLimit::trials(1).expect("limit"),
        RefreshMeasureConfig::default(),
        63,
        shard,
        0,
        Some(RelativeTime::new(Wave(3), 0)),
    );
    ended.tick_at(1, Some(0), &clocks, 0, None).expect("start trial");
    ended.observed = Some(ObservedRefreshFrame {
        sample: None,
        battle_status: BattleStatus::Ended,
    });
    assert!(matches!(
        ended.tick_at(1, Some(600), &clocks, 0, None),
        Err(RefreshTaskError::WindowEndUnreached)
    ));

    let mut lost = RefreshTask::new_with_window(
        MeasureLimit::trials(1).expect("limit"),
        RefreshMeasureConfig::default(),
        63,
        shard,
        0,
        Some(RelativeTime::new(Wave(3), 0)),
    );
    lost.tick_at(1, Some(0), &clocks, 0, None).expect("start lost trial");
    lost.observed = Some(ObservedRefreshFrame {
        sample: None,
        battle_status: BattleStatus::Lost,
    });
    assert!(matches!(
        lost.tick_at(1, Some(600), &clocks, 0, None),
        Ok(RefreshTaskControl::Complete(_))
    ));
}

#[test]
fn refresh_window_can_end_on_the_first_trial_dispatch() {
    let mut clocks = WaveClockState::new();
    clocks.record_refresh_clock(Wave(1), 100);
    for time in [-100, -101] {
        let mut task = RefreshTask::new_with_window(
            MeasureLimit::trials(1).expect("limit"),
            RefreshMeasureConfig::default(),
            63,
            SessionShard::default(),
            0,
            Some(RelativeTime::new(Wave(1), time)),
        );
        assert!(matches!(
            task.tick_at(1, Some(0), &clocks, 0, None),
            Ok(RefreshTaskControl::Complete(_))
        ));
    }
}

#[test]
fn refresh_failure_precedes_the_window_and_different_windows_do_not_merge() {
    let end = RelativeTime::new(Wave(2), 0);
    let clocks = WaveClockState::new();
    let shard = SessionShard::default();
    let mut task = RefreshTask::new_with_window(
        MeasureLimit::trials(1).expect("limit"),
        RefreshMeasureConfig::default(),
        63,
        shard,
        0,
        Some(end),
    );
    task.tick_at(1, Some(0), &clocks, 0, None).expect("start trial");
    let RefreshTaskControl::Complete(artifact) = task
        .tick_at(1, Some(0), &clocks, 0, Some(MeasureTrialOutcome::RefreshFailure))
        .expect("existing failure wins")
    else {
        panic!("failure should seal the trial")
    };
    let mut json = Vec::new();
    artifact.write_json(&mut json).expect("report");
    let report: RefreshReport = serde_json::from_slice(&json).expect("typed report");
    assert_eq!(report.refresh.failed, 1);

    let mut left = MeasurementState::empty_artifact_with_window(Some(0), RefreshMeasureConfig::default(), Some(end));
    let right = MeasurementState::empty_artifact_with_window(
        Some(0),
        RefreshMeasureConfig::default(),
        Some(RelativeTime::new(Wave(2), 1)),
    );
    assert!(left.merge_from(right).is_err());
}

#[test]
fn refresh_artifacts_reject_different_global_run_configuration() {
    let refresh = RefreshMeasureConfig::default();
    let trials = MeasureLimit::trials(2).expect("trials");
    let mut left =
        MeasurementState::empty_artifact_with_run(Some(0), refresh.clone(), trials, 63, SessionShard::default(), None);
    assert!(
        left.merge_from(MeasurementState::empty_artifact_with_run(
            Some(0),
            refresh.clone(),
            MeasureLimit::trials(4).expect("trials"),
            63,
            SessionShard::default(),
            None,
        ))
        .is_err()
    );
    let mut duration = MeasurementState::empty_artifact_with_run(
        None,
        refresh.clone(),
        MeasureLimit::duration(Duration::from_secs(1)).expect("duration"),
        63,
        SessionShard::default(),
        None,
    );
    assert!(
        duration
            .merge_from(MeasurementState::empty_artifact_with_run(
                None,
                refresh.clone(),
                MeasureLimit::duration(Duration::from_secs(60)).expect("duration"),
                63,
                SessionShard::default(),
                None,
            ))
            .is_err()
    );
    assert!(
        left.merge_from(MeasurementState::empty_artifact_with_run(
            Some(0),
            refresh,
            trials,
            64,
            SessionShard::default(),
            None,
        ))
        .is_err()
    );
    assert!(
        left.merge_from(MeasurementState::empty_artifact_with_run(
            Some(0),
            RefreshMeasureConfig::default(),
            trials,
            63,
            SessionShard {
                index: 0,
                count: 2,
                seed_base: 1,
            },
            None,
        ))
        .is_err()
    );
}

#[test]
fn refresh_setup_validation_and_puc_rule_mapping_are_independent_of_spawn() {
    let capabilities = RefreshMeasureCapabilities { cob_delay: false };
    let mut refresh = RefreshMeasureConfig::default();
    refresh.set_dance(RefreshDance::Fast);
    assert_eq!(validate_refresh_setup(&refresh, capabilities), Ok(()));
    refresh.set_cob_delay(true);
    assert_eq!(
        validate_refresh_setup(&refresh, capabilities),
        Err(RefreshSetupError::CobDelayUnsupported)
    );

    refresh.enable();
    refresh.set_cob_delay(false);
    assert_eq!(refresh.dance(), RefreshDance::Slow);
    refresh.set_assume_activate(true);
    assert_eq!(refresh.dance(), RefreshDance::Fast);
}

fn due(wave: i32) -> RefreshTimingFact {
    RefreshTimingFact {
        wave,
        clock: 800,
        expected_next_refresh: Some(1_000),
        observed_next_refresh: None,
        wavelength_declared: true,
    }
}

#[test]
fn refresh_trial_is_immediate_and_samples_each_wave_once() {
    let mut state = MeasurementState::new_refresh();
    state.start_trial();
    assert!(state.refresh_sample_due(due(1)));
    state
        .record_refresh_sample(
            RefreshSample {
                wave: 1,
                initial_hp: 1_000,
                current_hp: 500,
            },
            false,
        )
        .expect("sample");
    assert!(!state.refresh_sample_due(due(1)));
    assert_eq!(
        state.finish_trial(MeasureTrialOutcome::ObjectiveReached),
        MeasureTrialOutcome::ObjectiveReached
    );

    let counts = state
        .trial_counts(MeasureLimit::trials(1).expect("limit"), MeasurementEnd::Completed)
        .expect("counts");
    let report = state.partial().report(counts, &RefreshMeasureConfig::default());
    assert_eq!(report.valid_trials, 1);
    assert_eq!(report.samples.count, 1);
    assert_eq!(report.samples.by_wave["w1"].count, 1);
}

#[test]
fn replacing_or_finishing_an_incomplete_trial_records_invalid() {
    let mut state = MeasurementState::new_refresh();
    state.start_trial();
    state.start_trial();
    assert_eq!(state.partial().trial_count(MeasureTrialOutcome::Invalid), 1);
    assert!(state.refresh_sample_due(due(2)));
    assert_eq!(
        state.finish_trial(MeasureTrialOutcome::ObjectiveReached),
        MeasureTrialOutcome::Invalid
    );
    assert_eq!(state.partial().trial_count(MeasureTrialOutcome::Invalid), 2);
}

#[test]
fn finishing_without_an_active_trial_does_not_fabricate_an_attempt() {
    let mut state = MeasurementState::new_refresh();
    assert_eq!(
        state.finish_trial(MeasureTrialOutcome::ObjectiveReached),
        MeasureTrialOutcome::Invalid
    );
    assert_eq!(state.partial().attempted_trials(), 0);
    assert_eq!(state.completed_outcome(), None);
}

#[test]
fn partials_merge_without_a_report_protocol() {
    let mut left = MeasurementState::new_refresh();
    left.start_trial();
    left.finish_trial(MeasureTrialOutcome::RefreshFailure);
    let mut right = MeasurementState::new_refresh();
    right.start_trial();
    right.finish_trial(MeasureTrialOutcome::ObjectiveReached);

    let mut total = left.partial().clone();
    total.merge_from(right.partial());
    let report = total.report(
        MeasurementTrialCounts {
            requested_trials: Some(2),
            attempted_trials: 2,
            invalid_trials: 0,
            aborted_unrun_trials: 0,
        },
        &RefreshMeasureConfig::default(),
    );
    assert_eq!(report.refresh.satisfied, 1);
    assert_eq!(report.refresh.failed, 1);
}

#[test]
fn refresh_report_validation_rejects_inconsistent_terminal_counts() {
    let state = MeasurementState::new_refresh();
    let counts = state
        .trial_counts(
            MeasureLimit::duration(std::time::Duration::from_secs(1)).expect("limit"),
            MeasurementEnd::Aborted,
        )
        .expect("counts");
    let mut report = state.partial().report(counts, &RefreshMeasureConfig::default());
    report.validate().expect("valid report");

    report.aborted_unrun_trials = 1;
    assert!(matches!(
        report.validate(),
        Err(RefreshReportValidationError::DurationHasUnrun { .. })
    ));

    report.aborted_unrun_trials = 0;
    report.attempted_trials = 1;
    assert!(matches!(
        report.validate(),
        Err(RefreshReportValidationError::AttemptedBreakdown { .. })
    ));
}

#[test]
fn aborted_trial_counts_are_constant_time_and_keep_unrun_separate() {
    let mut state = MeasurementState::new_refresh();
    state.start_trial();
    assert!(state.seal_active_invalid());
    assert!(!state.seal_active_invalid());

    let counts = state
        .trial_counts(
            MeasureLimit::trials(1_000_000_000).expect("limit"),
            MeasurementEnd::Aborted,
        )
        .expect("valid aborted counts");
    assert_eq!(
        counts,
        MeasurementTrialCounts {
            requested_trials: Some(1_000_000_000),
            attempted_trials: 1,
            invalid_trials: 1,
            aborted_unrun_trials: 999_999_999,
        }
    );
}

#[test]
fn strided_shards_cover_exactly_the_requested_global_trials() {
    for total in 0..32 {
        for count in 1..8 {
            let mut sequences = Vec::new();
            let mut quotas = 0;
            for index in 0..count {
                let shard = SessionShard {
                    index,
                    count,
                    seed_base: 100,
                };
                let quota = local_trial_quota(total, shard);
                quotas += quota;
                for local in 0..quota {
                    let sequence = global_trial_sequence(shard, local);
                    assert!(sequence < total);
                    assert_eq!(sequence % u64::from(count), u64::from(index));
                    sequences.push(sequence);
                }
            }
            sequences.sort_unstable();
            assert_eq!(quotas, total);
            assert_eq!(sequences, (0..total).collect::<Vec<_>>());
            let seeds = sequences
                .into_iter()
                .map(|sequence| 100u32.wrapping_add(sequence as u32))
                .collect::<Vec<_>>();
            assert_eq!(
                seeds,
                (0..total)
                    .map(|sequence| 100u32.wrapping_add(sequence as u32))
                    .collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn refresh_task_owns_trial_reset_and_terminal_artifact_flow() {
    let mut task = RefreshTask::new(
        MeasureLimit::trials(2).expect("limit"),
        RefreshMeasureConfig::default(),
        63,
        SessionShard {
            index: 0,
            count: 1,
            seed_base: 100,
        },
        0,
    );
    assert_eq!(
        task.next_reset_config(),
        WorldResetConfig {
            completed_rounds: 63,
            seed: 100,
            initial_sun: 8_000,
            card_cooldowns: rsvz_model::ResetCardCooldowns::Ready,
        }
    );
    assert!(matches!(
        task.tick(1, 0, None).expect("first epoch starts a trial"),
        RefreshTaskControl::Continue
    ));
    let reset = task
        .tick(1, 0, Some(MeasureTrialOutcome::Invalid))
        .expect("first trial finishes");
    assert!(matches!(
        reset,
        RefreshTaskControl::Reset(WorldResetConfig {
            completed_rounds: 63,
            seed: 101,
            initial_sun: 8_000,
            card_cooldowns: rsvz_model::ResetCardCooldowns::Ready,
        })
    ));

    assert!(matches!(
        task.tick(2, 0, None).expect("second epoch starts a trial"),
        RefreshTaskControl::Continue
    ));
    let RefreshTaskControl::Complete(artifact) = task
        .tick(2, 0, Some(MeasureTrialOutcome::RefreshFailure))
        .expect("second trial completes the task")
    else {
        panic!("second trial should complete the task");
    };
    let mut json = Vec::new();
    artifact.write_json(&mut json).expect("artifact JSON");
    let report: RefreshReport = serde_json::from_slice(&json).expect("Refresh report");
    assert_eq!(report.attempted_trials, 2);
    assert_eq!(report.invalid_trials, 1);
    assert_eq!(report.refresh.failed, 1);
}

#[test]
fn terminal_count_invariants_reject_active_early_and_overrun_reports() {
    let limit = MeasureLimit::trials(2).expect("limit");
    let mut state = MeasurementState::new_refresh();
    state.start_trial();
    assert_eq!(
        state.trial_counts(limit, MeasurementEnd::Aborted),
        Err(MeasurementTrialCountError::ActiveTrial)
    );
    state.finish_trial(MeasureTrialOutcome::ObjectiveReached);
    assert!(matches!(
        state.trial_counts(limit, MeasurementEnd::Completed),
        Err(MeasurementTrialCountError::CompletedTrialCountMismatch { .. })
    ));
    state.start_trial();
    state.finish_trial(MeasureTrialOutcome::ObjectiveReached);
    state.start_trial();
    state.finish_trial(MeasureTrialOutcome::ObjectiveReached);
    assert!(matches!(
        state.trial_counts(limit, MeasurementEnd::Aborted),
        Err(MeasurementTrialCountError::AttemptedExceedsRequested { .. })
    ));
}
