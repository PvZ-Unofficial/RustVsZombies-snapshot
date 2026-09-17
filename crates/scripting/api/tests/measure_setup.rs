#![cfg(feature = "pvz-emulator")]
use rsvz::core::model::{MeasurementSetup, PlantKind, RefreshDance};
use rsvz::measure::*;
fn reset_runtime_state() {
    rsvz_game::setup::reset_script_setup();
}
fn current_measurement() -> MeasurementSetup {
    __current_setup()
}

#[test]
fn protection_setup_records_edits() {
    reset_runtime_state();

    protect_unrepairable_from_cards();
    protect_add([protect::grid(1, 1)]);
    protect_remove([protect::plant(2, 3, PlantKind::Pumpkin)]);

    let measurement = current_measurement();
    let policy = measurement.protection();
    assert!(policy.protect_unrepairable_from_cards_enabled());
    assert_eq!(policy.edits().len(), 2);
}

#[test]
fn refresh_setup_records_seml_options() {
    reset_runtime_state();

    refresh_activate(true);
    refresh_dance(RefreshDance::Fast);
    refresh_cob_delay(true);

    let measurement = current_measurement();
    let refresh = measurement.refresh();
    assert!(refresh.assume_activate());
    assert_eq!(refresh.dance(), RefreshDance::Fast);
    assert!(refresh.cob_delay());
}

#[test]
fn imp_leak_setup_keeps_enable_and_threshold_independent() {
    reset_runtime_state();

    imp_leak_detection(false);
    imp_leak_threshold(0);

    let config = current_measurement().imp_leak();
    assert!(!config.enabled());
    assert_eq!(config.threshold_cs(), 0);
}

#[test]
fn end_at_accepts_all_relative_time_inputs_and_last_call_wins() {
    reset_runtime_state();
    rsvz::__run_script(|| {
        end_at((2, 10));
        end_at((rsvz_model::Wave(3), -200));
        end_at(rsvz_model::RelativeTime::new(rsvz_model::Wave(4), 0));
        Ok(())
    })
    .expect("valid endpoints");
    assert_eq!(
        current_measurement().window_end(),
        Some(rsvz_model::RelativeTime::new(rsvz_model::Wave(4), 0))
    );

    let error = rsvz::__run_script(|| {
        end_at((-1, 0));
        Ok(())
    })
    .expect_err("negative wave must be reported during registration");
    assert!(error.to_string().contains("must be non-negative"));
}

#[test]
fn end_at_is_independent_of_other_measurement_setup_order() {
    reset_runtime_state();
    end_at((4, 0));
    completed_rounds(63);
    let end_first = current_measurement();

    reset_runtime_state();
    completed_rounds(63);
    end_at((4, 0));
    assert_eq!(end_first, current_measurement());
}

#[test]
fn event_protection_resolves_native_layers_cannon_anchor_and_imitator() {
    use rsvz_backend_api::PlantCreateBackend;
    use rsvz_game::event_measure::{EventMeasureConfig, ProtectionGridResolveError, resolve_event_measure_config};
    use rsvz_model::{CardSelection, Grid, MeasureMode, ProtectTarget, ProtectionEdit, ProtectionPolicy};
    use rsvz_pvz_emulator_backend::{PeWorldConfig, runner_internal::PeWorldOwner};
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            rsvz_current::with_backend_shared(|access| {
                let native = access;
                for (selection, grid) in [
                    (CardSelection::Plant(PlantKind::Pumpkin), Grid { row: 0, col: 0 }),
                    (CardSelection::Plant(PlantKind::WallNut), Grid { row: 0, col: 0 }),
                    (CardSelection::Plant(PlantKind::CobCannon), Grid { row: 1, col: 1 }),
                    (CardSelection::Imitator(PlantKind::TallNut), Grid { row: 4, col: 0 }),
                ] {
                    native.new_plant(selection.checked().unwrap(), grid).unwrap();
                }
                let mut policy = ProtectionPolicy::default();
                policy.add([
                    ProtectTarget::grid(Grid { row: 0, col: 0 }),
                    ProtectTarget::grid(Grid { row: 1, col: 2 }),
                    ProtectTarget::grid(Grid { row: 4, col: 0 }),
                ]);
                let mut config = EventMeasureConfig::new(MeasureMode::Smash, policy, None).unwrap();
                resolve_event_measure_config(&mut config).unwrap();
                assert_eq!(
                    config.protection().edits(),
                    &[ProtectionEdit::Add(vec![
                        ProtectTarget::plant(Grid { row: 0, col: 0 }, PlantKind::WallNut),
                        ProtectTarget::plant(Grid { row: 1, col: 1 }, PlantKind::CobCannon),
                        ProtectTarget::plant(Grid { row: 4, col: 0 }, PlantKind::Imitator),
                    ])]
                );
                let mut missing = ProtectionPolicy::default();
                missing.add([ProtectTarget::grid(Grid { row: 5, col: 8 })]);
                let mut config = EventMeasureConfig::new(MeasureMode::Pogo, missing, None).unwrap();
                assert!(matches!(
                    resolve_event_measure_config(&mut config),
                    Err(ProtectionGridResolveError::EmptyGrid { row: 6, col: 9 })
                ));
            })
            .unwrap();
        })
    });
}

#[test]
fn event_sampling_tracks_a_live_child_after_parent_death_and_rejects_a_replacement_id() {
    use rsvz_backend_api::{ZombieReadBackend, ZombieRemoveBackend};
    use rsvz_game::event_measure::{EventMeasureConfig, EventMeasureTask, EventMeasureTaskControl, SharedEventMeasure};
    use rsvz_model::{
        BattleStatus, Grid, ImpThrownFact, MeasureLimit, MeasureMode, ProtectionPolicy, SessionShard, WaveClockState,
        ZombieKind, ZombiePhase,
    };
    use rsvz_pvz_emulator_backend::{PeWorldConfig, runner_internal::PeWorldOwner};
    use rsvz_schedule::event::InternalEventInterceptor;
    use std::{cell::RefCell, rc::Rc};
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let parent = rsvz_game::modifier::spawn_zombie(ZombieKind::Gargantuar, Grid { row: 0, col: 7 }).unwrap();
            let child = rsvz_game::modifier::spawn_zombie(ZombieKind::Imp, Grid { row: 0, col: 4 }).unwrap();
            let config = EventMeasureConfig::new(MeasureMode::DamageNarrow, ProtectionPolicy::default(), None).unwrap();
            let task = Rc::new(RefCell::new(EventMeasureTask::new(
                MeasureLimit::trials(1).unwrap(),
                config,
                63,
                SessionShard {
                    index: 0,
                    count: 1,
                    seed_base: 100,
                },
                0,
            )));
            task.borrow_mut().initialize().unwrap();
            task.borrow_mut().tick(1, 0, 0, None).unwrap();
            let mut interceptor = SharedEventMeasure::new(Rc::clone(&task));
            interceptor.begin_logic_frame(1, 0);
            interceptor.emit_imp_thrown(ImpThrownFact {
                parent_id: parent,
                imp_id: child,
                parent_kind: ZombieKind::Gargantuar,
                from_wave: 0,
                row: 0,
                main_counter: 0,
                parent_x: 700.0,
                parent_hp: 1000,
                parent_max_hp: 3000,
                parent_phase: ZombiePhase::GargantuarThrowing,
                parent_speed_x: 0.0,
                parent_frozen: 0,
                parent_chilled: 0,
                parent_buttered: 0,
            });
            interceptor.end_logic_frame(BattleStatus::Running);
            let clocks = WaveClockState::new();
            task.borrow_mut().sample_imp_leak(1, Some(0), &clocks).unwrap();
            rsvz_current::with_backend_shared(|a| {
                let b = a;
                b.remove_zombie(b.zombie(parent).unwrap().unwrap()).unwrap();
            })
            .unwrap();
            task.borrow_mut().sample_imp_leak(1, Some(1), &clocks).unwrap();
            rsvz_current::with_backend_shared(|a| {
                let b = a;
                b.remove_zombie(b.zombie(child).unwrap().unwrap()).unwrap();
            })
            .unwrap();
            let replacement = rsvz_game::modifier::spawn_zombie(ZombieKind::Imp, Grid { row: 0, col: 4 }).unwrap();
            assert_ne!(replacement, child);
            task.borrow_mut().sample_imp_leak(1, Some(2), &clocks).unwrap();
            interceptor.begin_logic_frame(1, 3);
            interceptor.end_logic_frame(BattleStatus::ObjectiveReached);
            let EventMeasureTaskControl::Complete(artifact) = task.borrow_mut().tick(1, 3, 0, None).unwrap() else {
                panic!("trial should finish")
            };
            let mut json = Vec::new();
            artifact.write_json(&mut json).unwrap();
            let report = String::from_utf8(json).unwrap();
            assert!(report.contains("\"valid_trials\":1,"), "{report}");
            assert_eq!(report.matches("\"safe_deaths\":").count(), 1);
            assert!(report.contains("\"thrown_imps\":1,"), "{report}");
            assert!(report.contains("\"safe_deaths\":1,"), "{report}");
            assert!(report.contains("\"imp_leak\":0"), "{report}");
        })
    });
}

#[test]
fn event_measure_access_failures_reach_the_existing_session_handler() {
    use rsvz_game::event_measure::{EventMeasureConfig, EventMeasureTask, ProtectionGridResolveError};
    use rsvz_model::{Grid, MeasureLimit, MeasureMode, ProtectTarget, ProtectionPolicy, SessionShard, WaveClockState};
    let mut policy = ProtectionPolicy::default();
    policy.add([ProtectTarget::grid(Grid { row: 0, col: 0 })]);
    let config = EventMeasureConfig::new(MeasureMode::DamageNarrow, policy, None).unwrap();
    let mut task = EventMeasureTask::new(
        MeasureLimit::trials(1).unwrap(),
        config,
        63,
        SessionShard {
            index: 0,
            count: 1,
            seed_base: 0,
        },
        0,
    );
    assert!(matches!(task.initialize(), Err(ProtectionGridResolveError::Backend(_))));
    assert!(!task.initialized());
    assert!(task.sample_imp_leak(1, Some(0), &WaveClockState::new()).is_err());
    let config = EventMeasureConfig::new(MeasureMode::Smash, ProtectionPolicy::default(), None).unwrap();
    let mut task = EventMeasureTask::new(
        MeasureLimit::trials(1).unwrap(),
        config,
        63,
        SessionShard {
            index: 0,
            count: 1,
            seed_base: 0,
        },
        0,
    );
    task.initialize().unwrap();
    task.sample_imp_leak(1, Some(0), &WaveClockState::new()).unwrap();
}
