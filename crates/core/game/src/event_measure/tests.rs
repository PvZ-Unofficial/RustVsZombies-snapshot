use std::time::Duration;

use rsvz_model::model::{PlantId, ProjectileId, ZombieId, ZombiePhase};

use super::*;

fn config(mode: MeasureMode) -> EventMeasureConfig {
    let mut protection = ProtectionPolicy::default();
    protection.add([ProtectTarget::plant(Grid::new(0, 0).expect("grid"), PlantKind::WallNut)]);
    EventMeasureConfig::new(mode, protection, None).expect("config")
}

fn attempt(source: PlantEffectSource, effect: PlantEffect, max_hp: i32) -> PlantEffectAttemptFact {
    PlantEffectAttemptFact {
        source,
        plant_id: PlantId::from_raw(1),
        raw_kind: PlantKind::WallNut,
        effective_kind: PlantKind::WallNut,
        grid: Grid::new(0, 0).expect("grid"),
        hp_before: 100,
        max_hp,
        effect,
        main_counter: 11,
    }
}

fn ready_state(mode: MeasureMode) -> EventMeasurementState {
    let mut state = EventMeasurementState::new(config(mode));
    state.initialized = true;
    state.start_trial(10);
    state.begin_frame(1, 11);
    state
}

fn grid(row: i32, col: i32) -> Grid {
    Grid::new(row, col).expect("test grid")
}

#[test]
fn plant_loss_requires_a_destructive_outcome_and_only_narrow_mode() {
    let sources = [
        PlantEffectSource::Jack(ZombieId::from_raw(1)),
        PlantEffectSource::Bite(ZombieId::from_raw(1)),
        PlantEffectSource::Gargantuar(ZombieId::from_raw(1)),
        PlantEffectSource::Basketball(ProjectileId::from_raw(1)),
        PlantEffectSource::ZomboniCrush(ZombieId::from_raw(1)),
        PlantEffectSource::CatapultCrush(ZombieId::from_raw(1)),
        PlantEffectSource::Bungee(ZombieId::from_raw(1)),
    ];
    for source in sources {
        for outcome in [
            PlantEffectOutcome::Killed,
            PlantEffectOutcome::Squished,
            PlantEffectOutcome::Stolen,
            PlantEffectOutcome::HpDelta { applied: 4 },
            PlantEffectOutcome::Activated,
            PlantEffectOutcome::PreventedByRule,
            PlantEffectOutcome::NoEffect,
        ] {
            for mode in [
                MeasureMode::DamageNarrow,
                MeasureMode::BroadPass,
                MeasureMode::Pogo,
                MeasureMode::Smash,
            ] {
                let mut state = ready_state(mode);
                let mut fact = attempt(source, PlantEffect::Squish, 300);
                fact.raw_kind = PlantKind::Imitator;
                fact.effective_kind = PlantKind::IceShroom;
                assert_eq!(state.begin_effect(fact), EventDecision::Apply);
                state.finish_effect(EffectOutcomeFact {
                    token: rsvz_model::EventToken::from_raw(1),
                    key: fact.key(),
                    decision_origin: rsvz_model::EventDecisionOrigin::Native,
                    outcome,
                });
                let expected = (mode == MeasureMode::DamageNarrow
                    && matches!(
                        outcome,
                        PlantEffectOutcome::Killed | PlantEffectOutcome::Squished | PlantEffectOutcome::Stolen
                    ))
                .then_some(EventTrialOutcome::ImitatorIceLoss);
                assert_eq!(state.frame.unwrap().terminal, expected);
            }
        }
    }
}

#[test]
fn vehicle_crush_respects_protection_and_survives_objective_reached() {
    for source in [
        PlantEffectSource::ZomboniCrush(ZombieId::from_raw(1)),
        PlantEffectSource::CatapultCrush(ZombieId::from_raw(1)),
    ] {
        for protected in [false, true] {
            let mut state = ready_state(MeasureMode::DamageNarrow);
            let mut fact = attempt(source, PlantEffect::Squish, 4_000);
            if !protected {
                fact.grid = grid(0, 1);
            }
            assert_eq!(state.begin_effect(fact), EventDecision::Apply);
            state.finish_effect(EffectOutcomeFact {
                token: rsvz_model::EventToken::from_raw(1),
                key: fact.key(),
                decision_origin: rsvz_model::EventDecisionOrigin::Native,
                outcome: PlantEffectOutcome::Squished,
            });
            state.end_frame(BattleStatus::ObjectiveReached);
            state.finish_pending(&WaveClockState::new());
            assert_eq!(
                state.completed(),
                Some(if protected {
                    EventTrialOutcome::VehicleCrush
                } else {
                    EventTrialOutcome::ObjectiveReached
                })
            );
        }
    }
}

#[test]
fn protected_imitator_loss_wins_dual_classification_and_samples_merge_by_trial() {
    let mut merged = EventMeasurementPartial::default();
    for sequence in [9, 2, 7] {
        let mut state = ready_state(MeasureMode::DamageNarrow);
        state.start_trial_with_identity(10, sequence, 100 + sequence as u32, 42);
        state.begin_frame(42, 11);
        state
            .config
            .protection
            .add([ProtectTarget::plant(grid(0, 0), PlantKind::Imitator)]);
        let mut fact = attempt(
            PlantEffectSource::ZomboniCrush(ZombieId::from_raw(4)),
            PlantEffect::Squish,
            300,
        );
        fact.raw_kind = PlantKind::Imitator;
        fact.effective_kind = PlantKind::IceShroom;
        assert_eq!(state.begin_effect(fact), EventDecision::Apply);
        state.finish_effect(EffectOutcomeFact {
            token: rsvz_model::EventToken::from_raw(1),
            key: fact.key(),
            decision_origin: rsvz_model::EventDecisionOrigin::Native,
            outcome: PlantEffectOutcome::Squished,
        });
        // A later protected vehicle target cannot replace the first completed failure.
        fact.raw_kind = PlantKind::WallNut;
        fact.effective_kind = PlantKind::WallNut;
        state.finish_effect(EffectOutcomeFact {
            token: rsvz_model::EventToken::from_raw(2),
            key: fact.key(),
            decision_origin: rsvz_model::EventDecisionOrigin::Native,
            outcome: PlantEffectOutcome::Squished,
        });
        state.end_frame(BattleStatus::ObjectiveReached);
        state.finish_pending(&WaveClockState::new());
        assert_eq!(state.completed(), Some(EventTrialOutcome::ImitatorIceLoss));
        assert!(state.partial.vehicle_crush_sample.is_none());
        merged.merge_from(&state.partial);
        state.start_trial(20);
        assert!(state.trial.imitator_ice_loss_sample.is_none());
    }
    let sample = merged.imitator_ice_loss_sample.unwrap().report();
    assert_eq!((sample.trial_sequence, sample.seed, sample.world_epoch), (2, 102, 42));
    assert_eq!((sample.row, sample.col, sample.main_counter), (1, 1, 11));
    assert_eq!(merged.outcome(EventTrialOutcome::ImitatorIceLoss), 3);
}

#[test]
fn old_failure_reports_default_new_diagnostics_to_zero() {
    let report: NarrowFailures =
        serde_json::from_str(r#"{"garg_smash":1,"home_total":0,"pogo_home":0,"refresh":0}"#).unwrap();
    assert_eq!((report.vehicle_crush, report.imitator_ice_loss), (0, 0));
    assert!(std::mem::size_of::<Option<PlantLossSample>>() <= 104);
}

#[test]
fn legacy_damage_projection_uses_suppressed_requested_semantics() {
    let mut state = ready_state(MeasureMode::DamageNarrow);
    for fact in [
        attempt(
            PlantEffectSource::Jack(ZombieId::from_raw(1)),
            PlantEffect::InstantKill,
            4_000,
        ),
        attempt(
            PlantEffectSource::Bite(ZombieId::from_raw(2)),
            PlantEffect::HpDamage { native_requested: 4 },
            4_000,
        ),
        attempt(
            PlantEffectSource::Basketball(ProjectileId::from_raw(3)),
            PlantEffect::HpDamage { native_requested: 75 },
            4_000,
        ),
    ] {
        assert_eq!(state.begin_effect(fact), EventDecision::SuppressByMeasurement);
    }
    state.end_frame(BattleStatus::ObjectiveReached);
    state.finish_pending(&WaveClockState::new());
    let report = state.partial.report(
        MeasureMode::DamageNarrow,
        MeasurementTrialCounts {
            requested_trials: Some(1),
            attempted_trials: 1,
            invalid_trials: 0,
            aborted_unrun_trials: 0,
        },
    );
    let EventMeasureReport::DamageNarrow(report) = report else {
        panic!("damage report")
    };
    assert_eq!(report.damage.total, 8_004);
    assert_eq!(report.damage.by_source["jack_explosion"], 4_000);
    assert_eq!(report.damage.by_source["zombie_chew"], 4);
    assert_eq!(report.damage.by_source["catapult_basket"], 4_000);
}

#[test]
fn disabling_imp_diagnostics_keeps_the_original_damage_interest() {
    let mut config = config(MeasureMode::DamageNarrow);
    let mut imp = ImpLeakDiagnosticConfig::default();
    imp.set_enabled(false);
    config.set_imp_leak(imp);
    let state = EventMeasurementState::new(config);
    assert!(state.imp_tracker.is_none());
    assert_eq!(
        state.interest(),
        EventInterest::PLANT_EFFECT.union(EventInterest::HOME_ENTRY)
    );
}

#[test]
fn protection_resolves_key_layers_and_keeps_raw_and_effective_kinds_distinct() {
    let mut unrepairable = ProtectionPolicy::default();
    unrepairable.enable_protect_unrepairable_from_cards();
    assert_eq!(
        EventMeasureConfig::new(MeasureMode::DamageNarrow, unrepairable.clone(), None),
        Err(EventMeasureConfigError::SelectedCardsRequired)
    );
    assert_eq!(
        EventMeasureConfig::new(MeasureMode::DamageNarrow, unrepairable.clone(), Some(Vec::new())),
        Err(EventMeasureConfigError::EmptySelectedCards)
    );
    let cards = vec![
        CardSelection::Plant(PlantKind::WallNut),
        CardSelection::Imitator(PlantKind::TallNut),
    ];
    let config = EventMeasureConfig::new(MeasureMode::DamageNarrow, unrepairable, Some(cards)).expect("config");
    assert!(!protects(&config, grid(0, 0), PlantKind::WallNut, PlantKind::WallNut));
    assert!(!protects(&config, grid(0, 1), PlantKind::Imitator, PlantKind::TallNut));
    assert!(protects(&config, grid(0, 2), PlantKind::Pumpkin, PlantKind::Pumpkin));
}

#[test]
fn four_modes_keep_distinct_terminal_semantics() {
    let mut damage = ready_state(MeasureMode::DamageNarrow);
    damage.home_entry(HomeEntryFact {
        zombie_id: ZombieId::from_raw(1),
        zombie_kind: ZombieKind::Pogo,
        row: 0,
        main_counter: 11,
    });
    damage.end_frame(BattleStatus::Running);
    damage.finish_pending(&WaveClockState::new());
    assert_eq!(damage.completed(), Some(EventTrialOutcome::PogoHome));

    let mut broad = ready_state(MeasureMode::BroadPass);
    broad.home_entry(HomeEntryFact {
        zombie_id: ZombieId::from_raw(1),
        zombie_kind: ZombieKind::Normal,
        row: 1,
        main_counter: 11,
    });
    broad.end_frame(BattleStatus::Running);
    broad.finish_pending(&WaveClockState::new());
    assert_eq!(broad.completed(), Some(EventTrialOutcome::Home));

    let mut smash = ready_state(MeasureMode::Smash);
    assert_eq!(
        smash.begin_effect(attempt(
            PlantEffectSource::Gargantuar(ZombieId::from_raw(2)),
            PlantEffect::Squish,
            300,
        )),
        EventDecision::SuppressByMeasurement
    );
    smash.end_frame(BattleStatus::Running);
    smash.finish_pending(&WaveClockState::new());
    assert_eq!(smash.completed(), Some(EventTrialOutcome::GargSmash));

    let mut pogo = ready_state(MeasureMode::Pogo);
    pogo.home_entry(HomeEntryFact {
        zombie_id: ZombieId::from_raw(3),
        zombie_kind: ZombieKind::Normal,
        row: 2,
        main_counter: 11,
    });
    pogo.end_frame(BattleStatus::Running);
    pogo.finish_pending(&WaveClockState::new());
    assert!(pogo.trial_active());
}

#[test]
fn invalid_trial_discards_its_accumulated_data_and_zero_denominators_are_zero() {
    let mut state = ready_state(MeasureMode::BroadPass);
    state.home_entry(HomeEntryFact {
        zombie_id: ZombieId::from_raw(1),
        zombie_kind: ZombieKind::Normal,
        row: 0,
        main_counter: 11,
    });
    state.commit(EventTrialOutcome::Invalid);
    assert_eq!(state.partial.home_entries, 0);
    let EventMeasureReport::BroadPass(report) = state.partial.report(
        MeasureMode::BroadPass,
        MeasurementTrialCounts {
            requested_trials: Some(1),
            attempted_trials: 1,
            invalid_trials: 1,
            aborted_unrun_trials: 0,
        },
    ) else {
        panic!("broad report")
    };
    assert_eq!(report.broad_pass.rate, 0.0);
}

#[test]
fn artifact_json_uses_common_counts_and_merges_commutatively() {
    let limit = MeasureLimit::trials(1).expect("limit");
    let mut left = EventMeasurementState::new(config(MeasureMode::Pogo));
    left.initialized = true;
    left.start_trial(0);
    left.finish(EventTrialOutcome::ObjectiveReached);
    let mut right = EventMeasurementState::new(config(MeasureMode::Pogo));
    right.initialized = true;
    right.start_trial(0);
    right.finish(EventTrialOutcome::Invalid);
    let mut artifact = left.artifact(limit, MeasurementEnd::Completed).expect("artifact");
    artifact
        .merge_from(right.artifact(limit, MeasurementEnd::Completed).expect("artifact"))
        .expect("merge");
    let mut json = Vec::new();
    artifact.write_json(&mut json).expect("json");
    let report: PogoReport = serde_json::from_slice(&json).expect("report");
    report.common.validate().expect("counts");
    assert_eq!(report.common.requested_trials, Some(2));
    assert_eq!(report.common.attempted_trials, 2);
    assert_eq!(report.common.invalid_trials, 1);
}

#[test]
fn trial_and_duration_limits_stop_only_after_a_sealed_trial() {
    let shard = SessionShard {
        index: 0,
        count: 1,
        seed_base: 10,
    };
    let mut trials = EventMeasureTask::new(
        MeasureLimit::trials(2).expect("limit"),
        config(MeasureMode::BroadPass),
        63,
        shard,
        0,
    );
    trials.state.initialized = true;
    trials.tick(1, 0, 0, None).expect("start trial");
    trials.state.finish(EventTrialOutcome::ObjectiveReached);
    let EventMeasureTaskControl::Reset(reset) = trials.tick(1, 1, 0, None).expect("first boundary") else {
        panic!("the first of two trials must request reset")
    };
    assert_eq!((reset.completed_rounds, reset.seed), (63, 11));
    trials.tick(2, 0, 0, None).expect("start second trial");
    trials.state.finish(EventTrialOutcome::ObjectiveReached);
    assert!(matches!(
        trials.tick(2, 1, 0, None).expect("second boundary"),
        EventMeasureTaskControl::Complete(_)
    ));

    let mut duration = EventMeasureTask::new(
        MeasureLimit::duration(Duration::from_millis(1)).expect("limit"),
        config(MeasureMode::Pogo),
        0,
        shard,
        0,
    );
    duration.state.initialized = true;
    duration.run.started = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .expect("the monotonic clock supports a one-second test offset");
    duration.tick(1, 0, 0, None).expect("start duration trial");
    assert!(!duration.is_finished());
    duration.state.finish(EventTrialOutcome::GameOver);
    let EventMeasureTaskControl::Complete(artifact) = duration.tick(1, 1, 0, None).expect("sealed boundary") else {
        panic!("elapsed duration must complete at the trial boundary")
    };
    let mut json = Vec::new();
    artifact.write_json(&mut json).expect("json");
    let report: PogoReport = serde_json::from_slice(&json).expect("duration report");
    report.common.validate().expect("counts");
    assert_eq!(report.common.requested_trials, None);
}

#[test]
fn all_event_reports_round_trip_and_fact_reduction_is_deterministic() {
    fn trace(mode: MeasureMode) -> Vec<u8> {
        let mut state = ready_state(mode);
        match mode {
            MeasureMode::DamageNarrow => {
                assert_eq!(
                    state.begin_effect(attempt(
                        PlantEffectSource::Bite(ZombieId::from_raw(1)),
                        PlantEffect::HpDamage { native_requested: 4 },
                        4_000,
                    )),
                    EventDecision::SuppressByMeasurement
                );
                state.end_frame(BattleStatus::ObjectiveReached);
                state.finish_pending(&WaveClockState::new());
            }
            MeasureMode::BroadPass => {
                state.home_entry(HomeEntryFact {
                    zombie_id: ZombieId::from_raw(2),
                    zombie_kind: ZombieKind::Normal,
                    row: 1,
                    main_counter: 11,
                });
                state.end_frame(BattleStatus::Running);
                state.finish_pending(&WaveClockState::new());
            }
            MeasureMode::Smash => {
                state.begin_effect(attempt(
                    PlantEffectSource::Gargantuar(ZombieId::from_raw(3)),
                    PlantEffect::Squish,
                    300,
                ));
                state.end_frame(BattleStatus::Running);
                state.finish_pending(&WaveClockState::new());
            }
            MeasureMode::Pogo => {
                state.home_entry(HomeEntryFact {
                    zombie_id: ZombieId::from_raw(4),
                    zombie_kind: ZombieKind::Pogo,
                    row: 2,
                    main_counter: 11,
                });
                state.end_frame(BattleStatus::Running);
                state.finish_pending(&WaveClockState::new());
            }
            MeasureMode::Refresh => unreachable!(),
        }
        let artifact = state
            .artifact(MeasureLimit::trials(1).expect("limit"), MeasurementEnd::Completed)
            .expect("artifact");
        let mut json = Vec::new();
        artifact.write_json(&mut json).expect("json");
        json
    }

    for (mode, payload) in [
        (MeasureMode::DamageNarrow, "narrow_pass"),
        (MeasureMode::BroadPass, "broad_pass"),
        (MeasureMode::Smash, "smash"),
        (MeasureMode::Pogo, "pogo_home"),
    ] {
        let first = trace(mode);
        assert_eq!(
            first,
            trace(mode),
            "the same backend-neutral fact trace must reduce deterministically"
        );
        let report: EventMeasureReport = serde_json::from_slice(&first).expect("typed event report");
        let common = match &report {
            EventMeasureReport::DamageNarrow(report) => &report.common,
            EventMeasureReport::BroadPass(report) => &report.common,
            EventMeasureReport::Smash(report) => &report.common,
            EventMeasureReport::Pogo(report) => &report.common,
        };
        common.validate().expect("common count invariants");
        assert_eq!(common.mode, mode.as_str());
        let value: serde_json::Value = serde_json::from_slice(&first).expect("JSON object");
        assert!(value.get(payload).is_some());
        assert!(value.get("dropped_events").is_none());
    }
}

#[test]
fn task_accepts_event_terminal_before_completed_round_signal() {
    let mut task = EventMeasureTask::new(
        MeasureLimit::trials(1).expect("limit"),
        config(MeasureMode::BroadPass),
        0,
        SessionShard {
            index: 0,
            count: 1,
            seed_base: 10,
        },
        0,
    );
    task.state.initialized = true;
    assert!(matches!(
        task.tick(1, 10, 0, None).expect("start trial"),
        EventMeasureTaskControl::Continue
    ));
    task.state.begin_frame(1, 11);
    task.state.end_frame(BattleStatus::ObjectiveReached);
    let EventMeasureTaskControl::Complete(artifact) = task.tick(1, 12, 1, None).expect("acknowledge completed round")
    else {
        panic!("one requested trial should complete")
    };
    let mut json = Vec::new();
    artifact.write_json(&mut json).expect("json");
    let report: BroadPassReport = serde_json::from_slice(&json).expect("report");
    assert_eq!((report.broad_pass.success, report.common.attempted_trials), (1, 1));
}

#[test]
fn terminal_frame_interruption_invalidates_before_commit() {
    let mut task = EventMeasureTask::new(
        MeasureLimit::trials(1).expect("limit"),
        config(MeasureMode::BroadPass),
        0,
        SessionShard::default(),
        0,
    );
    task.state.initialized = true;
    task.tick(1, 10, 0, None).expect("start trial");
    task.state.begin_frame(1, 11);
    task.state.home_entry(HomeEntryFact {
        zombie_id: ZombieId::from_raw(1),
        zombie_kind: ZombieKind::Normal,
        row: 0,
        main_counter: 11,
    });
    task.state.end_frame(BattleStatus::Running);
    assert!(task.state.trial_active(), "native terminal must wait for the finalizer");

    let EventMeasureTaskControl::Complete(artifact) = task
        .tick(1, 11, 0, Some(EventMeasureInterruption::RecoverableError))
        .expect("observer failure seals the trial")
    else {
        panic!("one requested trial should complete")
    };
    let mut json = Vec::new();
    artifact.write_json(&mut json).expect("report");
    let report: BroadPassReport = serde_json::from_slice(&json).expect("report");
    assert_eq!((report.common.invalid_trials, report.home.entries), (1, 0));
}

#[test]
fn critical_imp_capacity_overflow_precedes_a_same_frame_terminal() {
    let mut state = ready_state(MeasureMode::DamageNarrow);
    for raw in 1..=129 {
        state.gargantuar_spawned(GargantuarSpawnedFact {
            parent_id: ZombieId::from_raw(raw),
            parent_kind: ZombieKind::Gargantuar,
            from_wave: 0,
            row: 0,
            main_counter: 11,
        });
    }
    state.end_frame(BattleStatus::ObjectiveReached);
    state.finish_pending(&WaveClockState::new());
    assert_eq!(state.completed(), Some(EventTrialOutcome::Invalid));
    assert_eq!(state.partial.outcomes[EventTrialOutcome::Invalid.index()], 1);
    assert_eq!(
        state
            .partial
            .imp_leak
            .report(true, 200, 0)
            .trace
            .active_family_overflows,
        1
    );
}

#[test]
fn terminal_frame_interruption_precedes_unreached_window_error() {
    let end = RelativeTime::new(rsvz_model::Wave(2), 0);
    let clocks = WaveClockState::new();
    for (mode, interruption) in [
        (MeasureMode::BroadPass, EventMeasureInterruption::RecoverableError),
        (MeasureMode::DamageNarrow, EventMeasureInterruption::TimingViolation),
    ] {
        let mut measure_config = config(mode);
        measure_config.set_window_end(Some(end));
        let mut task = EventMeasureTask::new(
            MeasureLimit::trials(1).expect("limit"),
            measure_config,
            0,
            SessionShard::default(),
            0,
        );
        task.state.initialized = true;
        task.tick_at(1, Some(0), &clocks, 0, None).expect("start trial");
        task.state.begin_frame(1, 1);
        task.state.end_frame(BattleStatus::ObjectiveReached);

        assert!(matches!(
            task.tick_at(1, Some(1), &clocks, 0, Some(interruption)),
            Ok(EventMeasureTaskControl::Complete(_))
        ));
    }
}

#[test]
fn damage_timing_failure_keeps_the_just_thrown_imp_exposure() {
    let clocks = WaveClockState::new();
    let mut task = EventMeasureTask::new(
        MeasureLimit::trials(1).expect("limit"),
        config(MeasureMode::DamageNarrow),
        0,
        SessionShard::default(),
        0,
    );
    task.state.initialized = true;
    task.tick_at(1, Some(0), &clocks, 0, None).expect("start trial");
    task.state
        .imp_tracker
        .as_mut()
        .expect("default imp tracker")
        .note_imp_thrown(ImpThrownFact {
            parent_id: ZombieId::from_raw(1),
            imp_id: ZombieId::from_raw(2),
            parent_kind: ZombieKind::Gargantuar,
            from_wave: 0,
            row: 0,
            main_counter: 0,
            parent_x: 700.0,
            parent_hp: 1_800,
            parent_max_hp: 3_000,
            parent_phase: ZombiePhase::GargantuarThrowing,
            parent_speed_x: 0.0,
            parent_frozen: 0,
            parent_chilled: 0,
            parent_buttered: 0,
        });

    let EventMeasureTaskControl::Complete(artifact) = task
        .tick_at(1, Some(0), &clocks, 0, Some(EventMeasureInterruption::TimingViolation))
        .expect("timing failure seals the trial")
    else {
        panic!("one requested trial should complete")
    };
    let mut json = Vec::new();
    artifact.write_json(&mut json).expect("report");
    let report: DamageNarrowReport = serde_json::from_slice(&json).expect("report");
    assert_eq!(report.failures.refresh, 1);
    let imp = report.imp_leak.expect("imp diagnostics");
    assert_eq!(imp.cohorts.len(), 1);
    assert_eq!(imp.cohorts[0].thrown_imps, 1);
    assert_eq!(imp.cohorts[0].censored_at_stop, 1);
}

#[test]
fn first_dispatch_interruption_invalidates_the_new_trial() {
    let mut task = EventMeasureTask::new(
        MeasureLimit::trials(1).expect("limit"),
        config(MeasureMode::Pogo),
        0,
        SessionShard {
            index: 0,
            count: 1,
            seed_base: 10,
        },
        0,
    );
    let EventMeasureTaskControl::Complete(artifact) = task
        .tick(1, 10, 0, Some(EventMeasureInterruption::RecoverableError))
        .expect("interrupted first trial")
    else {
        panic!("one requested trial should complete")
    };
    let mut json = Vec::new();
    artifact.write_json(&mut json).expect("json");
    let report: PogoReport = serde_json::from_slice(&json).expect("report");
    assert_eq!((report.common.attempted_trials, report.common.invalid_trials), (1, 1));

    let mut damage = EventMeasureTask::new(
        MeasureLimit::trials(1).expect("limit"),
        config(MeasureMode::DamageNarrow),
        0,
        SessionShard {
            index: 0,
            count: 1,
            seed_base: 10,
        },
        0,
    );
    let EventMeasureTaskControl::Complete(artifact) = damage
        .tick(1, 10, 0, Some(EventMeasureInterruption::TimingViolation))
        .expect("DamageNarrow timing failure")
    else {
        panic!("one requested trial should complete")
    };
    let mut json = Vec::new();
    artifact.write_json(&mut json).expect("json");
    let report: DamageNarrowReport = serde_json::from_slice(&json).expect("report");
    assert_eq!((report.failures.refresh, report.common.invalid_trials), (1, 0));
}

#[test]
fn every_event_mode_seals_at_the_window_and_serializes_it() {
    let end = RelativeTime::new(rsvz_model::Wave(2), 0);
    let mut clocks = WaveClockState::new();
    clocks.record_refresh_clock(rsvz_model::Wave(1), 0);
    clocks.record_refresh_clock(rsvz_model::Wave(2), 600);

    for mode in [
        MeasureMode::DamageNarrow,
        MeasureMode::BroadPass,
        MeasureMode::Smash,
        MeasureMode::Pogo,
    ] {
        let mut config = config(mode);
        config.set_window_end(Some(end));
        let mut task = EventMeasureTask::new(
            MeasureLimit::trials(1).expect("limit"),
            config,
            0,
            SessionShard::default(),
            0,
        );
        assert!(matches!(
            task.tick_at(1, Some(0), &clocks, 0, None).expect("start trial"),
            EventMeasureTaskControl::Continue
        ));
        assert!(matches!(
            task.tick_at(1, Some(599), &clocks, 0, None).expect("before endpoint"),
            EventMeasureTaskControl::Continue
        ));
        let EventMeasureTaskControl::Complete(artifact) =
            task.tick_at(1, Some(600), &clocks, 0, None).expect("exact endpoint")
        else {
            panic!("one endpoint trial should complete")
        };
        let mut json = Vec::new();
        artifact.write_json(&mut json).expect("report");
        let report: serde_json::Value = serde_json::from_slice(&json).expect("JSON");
        assert_eq!(report["valid_trials"], 1);
        assert_eq!(report["window_end"], serde_json::json!({"wave": 2, "time": 0}));
    }

    let mut json = Vec::new();
    EventMeasureTask::empty_artifact(Some(0), config(MeasureMode::Pogo))
        .write_json(&mut json)
        .expect("whole-level report");
    assert!(!String::from_utf8(json).expect("UTF-8").contains("window_end"));
}

#[test]
fn event_terminals_precede_the_window_but_level_end_before_it_is_an_error() {
    let end = RelativeTime::new(rsvz_model::Wave(2), 0);
    let mut damage_config = config(MeasureMode::DamageNarrow);
    damage_config.set_window_end(Some(end));
    let clocks = WaveClockState::new();
    let mut failure = EventMeasureTask::new(
        MeasureLimit::trials(1).expect("limit"),
        damage_config.clone(),
        0,
        SessionShard::default(),
        0,
    );
    failure
        .tick_at(1, Some(0), &clocks, 0, None)
        .expect("start failure trial");
    failure.state.finish(EventTrialOutcome::Home);
    assert!(matches!(
        failure.tick_at(1, Some(1), &clocks, 0, None),
        Ok(EventMeasureTaskControl::Complete(_))
    ));

    let mut ended = EventMeasureTask::new(
        MeasureLimit::trials(1).expect("limit"),
        damage_config,
        0,
        SessionShard::default(),
        0,
    );
    ended.tick_at(1, Some(0), &clocks, 0, None).expect("start ended trial");
    ended.state.begin_frame(1, 1);
    ended.state.end_frame(BattleStatus::Ended);
    assert!(matches!(
        ended.tick_at(1, Some(1), &clocks, 0, None),
        Err(EventMeasureTaskError::WindowEndUnreached)
    ));

    let mut smash_config = config(MeasureMode::Smash);
    smash_config.set_window_end(Some(end));
    let mut lost = EventMeasureTask::new(
        MeasureLimit::trials(1).expect("limit"),
        smash_config,
        0,
        SessionShard::default(),
        0,
    );
    lost.tick_at(1, Some(0), &clocks, 0, None).expect("start lost trial");
    lost.state.begin_frame(1, 1);
    lost.state.end_frame(BattleStatus::Lost);
    assert!(matches!(
        lost.tick_at(1, Some(1), &clocks, 0, None),
        Ok(EventMeasureTaskControl::Complete(_))
    ));
}

#[test]
fn event_artifacts_with_different_windows_do_not_merge() {
    let mut left_config = config(MeasureMode::Pogo);
    left_config.set_window_end(Some(RelativeTime::new(rsvz_model::Wave(2), 0)));
    let mut right_config = config(MeasureMode::Pogo);
    right_config.set_window_end(Some(RelativeTime::new(rsvz_model::Wave(2), 1)));
    let mut left = EventMeasureTask::empty_artifact(Some(0), left_config);
    assert!(
        left.merge_from(EventMeasureTask::empty_artifact(Some(0), right_config))
            .is_err()
    );
}

#[test]
fn event_artifacts_reject_different_global_run_configuration() {
    let measure_config = config(MeasureMode::Pogo);
    let trials = MeasureLimit::trials(2).expect("trials");
    let mut left =
        EventMeasureTask::empty_artifact_with_run(Some(0), measure_config.clone(), trials, 63, SessionShard::default());
    assert!(
        left.merge_from(EventMeasureTask::empty_artifact_with_run(
            Some(0),
            measure_config.clone(),
            MeasureLimit::trials(4).expect("trials"),
            63,
            SessionShard::default(),
        ))
        .is_err()
    );
    assert!(
        left.merge_from(EventMeasureTask::empty_artifact_with_run(
            Some(0),
            measure_config.clone(),
            trials,
            64,
            SessionShard::default(),
        ))
        .is_err()
    );
    let mut duration = EventMeasureTask::empty_artifact_with_run(
        None,
        measure_config.clone(),
        MeasureLimit::duration(Duration::from_secs(1)).expect("duration"),
        63,
        SessionShard::default(),
    );
    assert!(
        duration
            .merge_from(EventMeasureTask::empty_artifact_with_run(
                None,
                measure_config,
                MeasureLimit::duration(Duration::from_secs(60)).expect("duration"),
                63,
                SessionShard::default(),
            ))
            .is_err()
    );
}

#[test]
fn artifact_merge_uses_the_declared_not_worker_resolved_protection() {
    let mut policy = ProtectionPolicy::default();
    policy.add([ProtectTarget::grid(Grid::new(0, 0).expect("grid"))]);
    let declared = EventMeasureConfig::new(MeasureMode::Pogo, policy, None).expect("config");
    let mut state = EventMeasurementState::new(declared.clone());
    state.config = config(MeasureMode::Pogo);
    state.start_trial(0);
    state.finish(EventTrialOutcome::ObjectiveReached);
    let mut artifact = state
        .artifact(MeasureLimit::trials(1).expect("limit"), MeasurementEnd::Completed)
        .expect("artifact");
    artifact
        .merge_from(EventMeasureTask::empty_artifact(Some(0), declared))
        .expect("zero-quota worker uses the same declared config");

    let mut incompatible = EventMeasureTask::empty_artifact(Some(0), config(MeasureMode::Pogo));
    assert!(
        incompatible
            .merge_from(EventMeasureTask::empty_artifact(Some(0), config(MeasureMode::Smash)))
            .is_err()
    );
}
