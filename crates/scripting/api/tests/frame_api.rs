#![cfg(feature = "pvz-emulator")]
use rsvz_backend_api::{
    BattleEntryBackend, CardAppendSelectionBackend, PlantCreateBackend, PlantStateWriteBackend, SeedRuleEditBackend,
    ZombieCreateBackend, ZombieReadBackend,
};
use rsvz_model::{CardSelection, GameUi, Grid, PlantKind, Wave, WaveTimingSnapshot, ZombieKind};
use rsvz_pvz_emulator_backend::{PeWorldConfig, runner_internal::PeWorldOwner};
use rsvz_schedule::{TickControl, TickMeta, TickOptions, TickPhase, TickTaskState, TimelineDispatchResult};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

fn reset() {
    rsvz_game::frame::reset_runtime_state_preserving_backend();
    rsvz_game::state_hook::clear_state_hooks();
    rsvz_game::lifecycle::reset_session_resources();
    rsvz_game::session::reset_session_control();
    rsvz::cob::__reset_current();
}
fn meta(clock: i32) -> TickMeta {
    TickMeta {
        phase: TickPhase::Playing,
        game_ui: Some(GameUi::Playing),
        clock: Some(clock),
        is_new_frame: true,
    }
}

#[test]
fn lineup_matures_imitators_before_ladders_without_accelerating_played_cards() {
    use rsvz_backend_api::{GridItemReadBackend, PlantReadBackend, PlantStateBackend};
    use rsvz_model::GridItemKind;
    reset();
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let lineup =
                rsvz::Lineup::from_code("LI5HDH/tAib/1HpXSFw4wch97UF84sI1eRCQNXFEOiEMneRlNHlQFhE4RHbM7lBkGFJU8MFKAFY=")
                    .unwrap();
            rsvz_game::lineup::apply_lineup(&lineup, Default::default()).unwrap();
        });
    });
    let assert_lineup = |backend: &mut rsvz_pvz_emulator_backend::PeBackend| {
        let mut imitated_pumpkins = 0;
        for plant in backend.plants().unwrap() {
            assert_ne!(
                backend.plant_raw_kind(plant).unwrap(),
                PlantKind::Imitator,
                "lineup has no pending morphs"
            );
            if backend.plant_imitater_kind(plant) == 48 {
                assert_eq!(backend.plant_raw_kind(plant).unwrap(), PlantKind::Pumpkin);
                assert_eq!(backend.plant_hp(plant), 4000);
                imitated_pumpkins += 1;
            }
        }
        assert_eq!(imitated_pumpkins, 10);
        assert_eq!(
            backend
                .grid_items()
                .unwrap()
                .filter(|&g| backend.grid_item_kind(g).unwrap() == GridItemKind::Ladder)
                .count(),
            10
        );
    };
    world.with_backend(assert_lineup);
    world.with_backend(|backend| backend.start_battle().unwrap());
    for _ in 0..400 {
        world.update_world().unwrap();
    }
    world.with_backend(assert_lineup);
    let id = world.with_backend(|backend| {
        let placeholder = backend
            .new_plant(
                CardSelection::Imitator(PlantKind::IceShroom).checked().unwrap(),
                Grid { row: 0, col: 8 },
            )
            .unwrap();
        assert_eq!(backend.plant_raw_kind(placeholder).unwrap(), PlantKind::Imitator);
        assert_eq!(backend.plant_state_countdown(placeholder), 200);
        backend.plant_id(placeholder)
    });
    world.update_world().unwrap();
    world.with_backend(|backend| {
        let placeholder = backend.plant(id).unwrap().unwrap();
        assert_eq!(backend.plant_raw_kind(placeholder).unwrap(), PlantKind::Imitator);
        assert_eq!(backend.plant_state_countdown(placeholder), 199);
    });
    reset();
}

#[test]
fn active_time_skips_native_delays_without_interrupting_the_script() {
    use rsvz_backend_api::{PlantEffectCountdownWriteBackend, PlantReadBackend};
    use rsvz_model::NonNegativeI32;
    reset();
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let id = rsvz::with_backend(|backend| {
                backend.start_battle().unwrap();
                let plant = backend
                    .add_plant(
                        CardSelection::Plant(PlantKind::IceShroom).checked().unwrap(),
                        Grid { row: 0, col: 0 },
                    )
                    .unwrap();
                backend.set_plant_state(plant, 2).unwrap();
                backend.plant_id(plant)
            });
            let messages = Rc::new(RefCell::new(Vec::new()));
            let captured = Rc::clone(&messages);
            let logger = rsvz::replace_logger(move |record: &rsvz::LogRecord<'_>| {
                captured.borrow_mut().push((record.level, record.message.to_owned()));
            });
            let snapshot = rsvz_game::timing::wave_timing().unwrap();
            rsvz_game::timeline::dispatch_timeline_tick(snapshot, meta(snapshot.clock));
            for (before, after) in [(7, 10), (13, 10), (44, 44)] {
                rsvz::with_backend(|backend| {
                    backend
                        .set_plant_effect_countdown(
                            backend.plant(id).unwrap().unwrap(),
                            NonNegativeI32::new(before).unwrap(),
                        )
                        .unwrap();
                });
                rsvz::cards::set_plant_active_time(PlantKind::IceShroom, 10).unwrap();
                assert_eq!(
                    rsvz_game::timeline::dispatch_timeline_tick(snapshot, meta(snapshot.clock)),
                    TimelineDispatchResult::Continue
                );
                rsvz::with_backend(|backend| {
                    assert_eq!(
                        backend.plant_effect_countdown(backend.plant(id).unwrap().unwrap()),
                        after
                    );
                });
                assert!(rsvz::session::take_dispatch_outcome().is_none());
            }
            assert_eq!(messages.borrow().len(), 1);
            assert_eq!(messages.borrow()[0].0, rsvz::LogLevel::Debug);
            assert!(messages.borrow()[0].1.contains("countdown=44"));
            rsvz::restore_logger(logger);
        });
    });
    reset();
}

#[test]
fn frame_actions_reenter_current_core_without_invalidating_borrowed_entities() {
    reset();
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let zombie_id = rsvz::with_backend(|backend| {
                backend
                    .select_card(CardSelection::Plant(PlantKind::Sunflower).checked().unwrap())
                    .unwrap();
                backend.start_battle().unwrap();
                backend.set_seed_recharge_ignored(true).unwrap();
                let cob = backend
                    .add_plant(
                        CardSelection::Plant(PlantKind::CobCannon).checked().unwrap(),
                        Grid { row: 0, col: 0 },
                    )
                    .unwrap();
                backend.set_plant_state(cob, 37).unwrap();
                let zombie = backend.add_zombie_in_row(ZombieKind::Normal, 1, 0).unwrap().unwrap();
                backend.zombie_id(zombie)
            });
            rsvz_game::cob::try_set_cobs([(1, 1)]).unwrap();
            let expected_motion =
                rsvz_game::logic::zombie_stable_x_trace(zombie_id, 8).map_err(|error| error.to_string());
            let ran = Rc::new(Cell::new(0));
            let called = Rc::clone(&ran);
            rsvz::__run_script(|| {
                rsvz::at_frame(1, 0, move |frame| {
                    let zombie = frame.zombie(zombie_id).unwrap();
                    assert_eq!(zombie.id(), zombie_id);
                    assert_eq!(zombie.hp(), 270);
                    assert!(zombie.x().is_finite());
                    assert_eq!(zombie.phase(), rsvz_model::ZombiePhase::ZombieNormal);
                    assert!(zombie.animation_progress().is_some());
                    assert_eq!(
                        zombie.stable_x_trace(8).map_err(|error| error.to_string()),
                        expected_motion
                    );
                    if let Ok(trace) = &expected_motion {
                        assert_eq!(zombie.stable_x_at(8).unwrap(), trace[8]);
                    }
                    zombie.remove();
                    assert!(zombie.motion_state().is_none());
                    assert!(rsvz_game::logic::zombie_motion_state(zombie_id).is_none());
                    assert!(matches!(
                        zombie.stable_x_trace(8),
                        Err(rsvz_model::ZombieMotionCallError::ObjectUnavailable)
                    ));
                    assert!(!zombie.is_alive());
                    assert_eq!(zombie.hp(), 270);
                    assert!(frame.zombie(zombie_id).is_none());
                    let mut original_plants = frame.plants();
                    let cob = original_plants.next().unwrap();
                    assert_eq!(cob.kind(), PlantKind::CobCannon);
                    rsvz::cob::fire(2, 9);
                    let sunflower = rsvz::cards::card(PlantKind::Sunflower, 2, 5).unwrap();
                    assert_eq!(cob.kind(), PlantKind::CobCannon);
                    assert_eq!(zombie.hp(), 270);
                    assert!(!zombie.is_alive());
                    assert_eq!(
                        original_plants.count(),
                        0,
                        "the old iterator retains its creation bound"
                    );
                    assert_eq!(frame.plants().count(), 2);
                    assert_eq!(frame.plant(sunflower).unwrap().grid(), Grid { row: 1, col: 4 });
                    let at = frame
                        .plant_at(Grid { row: 1, col: 4 }, |p| p.raw_kind() == PlantKind::Sunflower)
                        .unwrap();
                    assert_eq!(at.id(), sunflower);
                    assert!(!at.is_sleeping());
                    at.remove();
                    assert!(
                        frame.plant_at(Grid { row: 1, col: 4 }, |_| true).is_none(),
                        "grid reads observe same-frame removal"
                    );
                    called.set(called.get() + 1);
                    Some(sunflower)
                });
                Ok(())
            })
            .unwrap();
            assert_eq!(
                rsvz_game::timeline::dispatch_timeline_tick(WaveTimingSnapshot::minimal(100, Wave(1)), meta(100)),
                TimelineDispatchResult::Continue
            );
            assert_eq!(ran.get(), 1);
            // Same-clock Immediate uses a fresh execution borrow and the same adapter.
            rsvz::timeline::try_at_frame(1, 0, |frame| frame.plants().map(|plant| plant.id()).collect::<Vec<_>>())
                .unwrap();
        })
    });
    reset();
}

#[test]
fn frame_ticks_preserve_options_output_control_and_retry_after_error() {
    reset();
    assert!(rsvz::tick::spawn_frame(TickOptions::any_dispatch(), |_| ()).is_err());
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let calls = Rc::new(Cell::new(0));
            let count = Rc::clone(&calls);
            let task = rsvz::tick::on_frame(move |frame| -> rsvz::RuntimeResult<TickControl> {
                assert_eq!(frame.zombies().count(), 0);
                count.set(count.get() + 1);
                if count.get() == 1 {
                    Err(rsvz::RuntimeError::new("retry"))
                } else {
                    Ok(TickControl::Stop)
                }
            });
            let intro_calls = Rc::new(Cell::new(0));
            let intro = Rc::clone(&intro_calls);
            let intro_task = rsvz::tick::spawn_frame(TickOptions::active_phase(), move |frame| {
                assert_eq!(frame.plants().count(), 0);
                intro.set(intro.get() + 1);
                TickControl::Pause
            })
            .unwrap();
            let intro_meta = TickMeta {
                phase: TickPhase::LevelIntro,
                game_ui: Some(GameUi::LevelIntro),
                clock: None,
                is_new_frame: false,
            };
            let mut errors = Vec::new();
            rsvz_game::tick::dispatch_scheduler_tick_reporting(intro_meta, &mut |error| errors.push(error.to_string()));
            assert_eq!(intro_calls.get(), 1);
            assert_eq!(calls.get(), 0);
            for clock in [1, 2, 3] {
                rsvz_game::tick::dispatch_scheduler_tick_reporting(meta(clock), &mut |error| {
                    errors.push(error.to_string())
                });
            }
            assert_eq!(calls.get(), 2);
            assert_eq!(errors, ["retry"]);
            rsvz_schedule::tick::with_scheduler(|scheduler| {
                assert_eq!(scheduler.state(task), TickTaskState::Stopped);
                assert_eq!(scheduler.state(intro_task), TickTaskState::Paused);
            });
        })
    });
    reset();
}

#[test]
fn frame_acquisition_failure_is_local_and_does_not_use_tick_meta_as_proof() {
    reset();
    let reached = Rc::new(Cell::new(false));
    let callback = Rc::clone(&reached);
    let after_read = Rc::new(Cell::new(false));
    let after_read_callback = Rc::clone(&after_read);
    let logs = Rc::new(RefCell::new(Vec::new()));
    let captured = Rc::clone(&logs);
    let logger =
        rsvz::replace_logger(move |record: &rsvz::LogRecord<'_>| captured.borrow_mut().push(record.message.to_owned()));
    rsvz::__run_script(|| {
        rsvz::try_at_frame(1, 0, |_| -> () { panic!("missing Board must not invoke the callback") })?;
        rsvz::at(1, 0, move || {
            let _ = rsvz_game::logic::zombie_motion_state(rsvz_model::ZombieId::from_raw(1));
            after_read_callback.set(true);
        });
        rsvz::at(1, 0, || -> () {
            let _ = rsvz::refresh_countdown();
            panic!("missing Board must end the scalar read callback");
        });
        rsvz::at(1, 0, move || callback.set(true));
        Ok(())
    })
    .unwrap();
    assert_eq!(
        rsvz_game::timeline::dispatch_timeline_tick_reporting(
            WaveTimingSnapshot::minimal(100, Wave(1)),
            meta(100),
            &mut rsvz_game::diagnostics::report_runtime_error
        ),
        TimelineDispatchResult::Continue
    );
    assert!(reached.get());
    assert!(!after_read.get());
    assert_eq!(logs.borrow().len(), 3);
    assert!(!rsvz_game::session::script_stop_requested());
    rsvz::restore_logger(logger);
    reset();
}

#[test]
fn ordinary_callbacks_allow_exclusive_access_but_frames_reject_it_locally() {
    reset();
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    // Exclusive access advances the sampling epoch. Keep the real world at
    // the supplied wave boundary so later callbacks remain eligible on resample.
    world.with_backend(|backend| rsvz_backend_api::BattleEntryBackend::start_battle(backend).unwrap());
    for _ in 0..601 {
        world.update_world().unwrap();
    }
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let current_clock = rsvz_game::runtime::runtime_frame_facts().unwrap().clock.unwrap();
            rsvz::with_backend(|backend| backend.start_battle().unwrap());
            let ran = Rc::new(Cell::new(0));
            let later = Rc::clone(&ran);
            rsvz::__run_script(|| {
                rsvz::at(1, 0, || rsvz::with_backend(|_| ()));
                rsvz::at_frame(1, 0, |_frame| -> () {
                    rsvz::with_backend(|_| panic!("exclusive callback must not execute"));
                    panic!("access failure must end the Frame callback");
                });
                rsvz::at(1, 0, move || {
                    later.set(later.get() + 1);
                });
                Ok(())
            })
            .unwrap();
            let mut errors = Vec::new();
            assert_eq!(
                rsvz_game::timeline::dispatch_timeline_tick_reporting(
                    WaveTimingSnapshot::minimal(current_clock, Wave(1)),
                    meta(current_clock),
                    &mut |error| errors.push(error)
                ),
                TimelineDispatchResult::Continue
            );
            assert_eq!(ran.get(), 1);
            assert_eq!(errors.len(), 1);
            assert!(errors[0].to_string().contains("active borrow"));
            rsvz::with_backend(|_| ());
        });
    });
    reset();
}

#[test]
fn unbound_reset_rechecks_remaining_task_availability_and_pre_hook_samples() {
    use rsvz_backend_api::WorldResetBackend;
    use rsvz_model::WorldResetConfig;
    use rsvz_schedule::TickDispatchResult;
    reset();
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            rsvz::with_backend(|backend| backend.start_battle().unwrap());
            let sample = rsvz_game::frame::sample_current_frame().unwrap();
            assert_eq!(sample.phase(), TickPhase::Playing);
            rsvz::tick::spawn(TickOptions::any_dispatch(), |_| {
                rsvz::with_backend(|backend| {
                    backend.reset_world(WorldResetConfig {
                        completed_rounds: 63,
                        ..WorldResetConfig::default()
                    })
                })?;
                Ok(TickControl::Stop)
            });
            let active = Rc::new(Cell::new(0));
            let called = Rc::clone(&active);
            rsvz::tick::spawn(TickOptions::active_phase(), move |meta| {
                assert_eq!(meta.phase, TickPhase::LevelIntro);
                called.set(called.get() + 1);
                Ok(TickControl::Continue)
            });
            let playing = rsvz::tick::spawn(TickOptions::playing_frame(), |_| panic!("stale Playing eligibility"));
            assert_eq!(
                rsvz_game::tick::dispatch_scheduler_tick(sample.meta),
                TickDispatchResult::Continue
            );
            assert_eq!(active.get(), 1);
            assert_eq!(
                rsvz_game::frame::dispatch_sample_scheduler(&sample),
                rsvz_game::runtime::RuntimeFrameDispatch::Continue
            );
            assert_eq!(active.get(), 2);
            rsvz_schedule::tick::with_scheduler(|scheduler| {
                assert_eq!(scheduler.state(playing), TickTaskState::Running)
            });
        });
    });
    reset();
}

#[test]
fn required_cooldown_read_has_no_fallback_but_native_action_rejection_is_recoverable() {
    reset();
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    let reached = Rc::new(Cell::new(false));
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let after = Rc::clone(&reached);
            rsvz::__run_script(|| {
                rsvz::at(1, 0, || -> () {
                    let _ = rsvz::cards::card_cd(PlantKind::Imitator);
                    panic!("invalid required cooldown must not return zero");
                });
                rsvz::at(1, 0, move || {
                    assert!(rsvz_game::modifier::set_sun(u32::MAX).is_err());
                    assert!(matches!(
                        rsvz_game::modifier::spawn_zombie(ZombieKind::Normal, Grid { row: 99, col: 0 }),
                        Err(rsvz_game::modifier::SpawnZombieError::Rejected(_))
                    ));
                    after.set(true);
                });
                Ok(())
            })
            .unwrap();
            let mut errors = Vec::new();
            assert_eq!(
                rsvz_game::timeline::dispatch_timeline_tick_reporting(
                    WaveTimingSnapshot::minimal(100, Wave(1)),
                    meta(100),
                    &mut |error| errors.push(error)
                ),
                TimelineDispatchResult::Continue
            );
            assert_eq!(errors.len(), 1);
            assert!(errors[0].to_string().contains("invalid cooldown card selection"));
        });
    });
    assert!(reached.get());
    reset();
}

#[test]
fn dancing_and_runtime_cob_scan_binding_use_current_core_without_exclusive_reentry() {
    use rsvz_backend_api::MaidCheatsBackend;
    use rsvz_model::MaidCheat;
    reset();
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            rsvz::with_backend(|backend| {
                for grid in [Grid { row: 0, col: 0 }, Grid { row: 1, col: 2 }] {
                    backend
                        .add_plant(CardSelection::Plant(PlantKind::CobCannon).checked().unwrap(), grid)
                        .unwrap();
                }
            });
            let selected = rsvz::cob::CobManager::new();
            let scan = selected.clone();
            rsvz::__run_script(|| {
                (1, 0) << rsvz::dsl::dancing();
                rsvz::at(1, 0, move || {
                    (1, 1) << rsvz::dsl::auto_cobs();
                    (1, 1) << rsvz::dsl::auto_cobs_col(&scan, 1);
                });
                Ok(())
            })
            .unwrap();
            assert_eq!(
                rsvz_game::timeline::dispatch_timeline_tick(WaveTimingSnapshot::minimal(100, Wave(1)), meta(100)),
                TimelineDispatchResult::Continue
            );
            assert_eq!(
                rsvz_current::with_backend_shared(|backend| backend.maid_cheat())
                    .unwrap()
                    .unwrap(),
                MaidCheat::Dancing
            );
            let mut before = Vec::new();
            rsvz::cob::for_each_cob_grid(|grid| before.push(grid));
            assert!(
                before.is_empty(),
                "preparation reserves capacity; the scan stays scheduled"
            );
            assert_eq!(
                rsvz_game::timeline::dispatch_timeline_tick(WaveTimingSnapshot::minimal(101, Wave(1)), meta(101)),
                TimelineDispatchResult::Continue
            );
            let mut all = Vec::new();
            let mut column = Vec::new();
            rsvz::cob::for_each_cob_grid(|grid| all.push(grid));
            rsvz::cob::for_each_cob_grid(&selected, |grid| column.push(grid));
            assert_eq!(all, [Grid { row: 0, col: 0 }, Grid { row: 1, col: 2 }]);
            assert_eq!(column, [Grid { row: 0, col: 0 }]);
        });
    });
    reset();
}

#[test]
fn pure_immediate_can_run_during_exclusive_access_but_getters_and_frames_cannot() {
    reset();
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            assert_eq!(
                rsvz_game::timeline::dispatch_timeline_tick(WaveTimingSnapshot::minimal(100, Wave(1)), meta(100)),
                TimelineDispatchResult::Continue
            );
            let pure = Rc::new(Cell::new(false));
            let called = Rc::clone(&pure);
            let errors = Rc::new(RefCell::new(Vec::new()));
            let reported = Rc::clone(&errors);
            let logger = rsvz::replace_logger(move |record: &rsvz::LogRecord<'_>| {
                reported.borrow_mut().push(record.message.to_owned())
            });
            rsvz::with_backend(|_| {
                rsvz::try_at(1, 0, move || called.set(true)).unwrap();
                assert!(pure.get());
                rsvz::try_at(1, 0, || -> () {
                    let _ = rsvz::refresh_countdown();
                    panic!("required getter must not pass an exclusive borrow");
                })
                .unwrap();
                rsvz::try_at_frame(1, 0, |_| -> () {
                    panic!("Frame callback must not acquire a conflicting borrow");
                })
                .unwrap();
            });
            rsvz::restore_logger(logger);
            assert_eq!(errors.borrow().len(), 2);
            assert!(errors.borrow().iter().all(|message| message.contains("active borrow")));
            assert!(!rsvz_game::session::script_stop_requested());
            // Both guards were released after the failing callbacks.
            rsvz::try_at_frame(1, 0, |frame| assert_eq!(frame.zombies().count(), 0)).unwrap();
        });
    });
    reset();
}
