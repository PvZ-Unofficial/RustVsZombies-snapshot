#![cfg(feature = "pvz-emulator")]

use rsvz::cob;
use rsvz::cob::{CobManager, fire, try_fire};
use rsvz_backend_api::PlantStateBackend;
use rsvz_backend_api::backend::{ClockBackend, PlantCreateBackend, PlantReadBackend, PlantStateWriteBackend};
use rsvz_backend_api::error::RuntimeResult;
use rsvz_game::logic::cob::{CobListOrder, CobSequentialMode, cob_target_to_pixel};
use rsvz_model::{CardSelection, CobTarget, Grid, PlantId, PlantKind, Wave, WaveTimingSnapshot};
use rsvz_pvz_emulator_backend::{PeBackend, PeWorldConfig, pe_rs, runner_internal::PeWorldOwner};
use rsvz_schedule::tick::{TickMeta, TickPhase};
use rsvz_schedule::timeline::TimelineDispatchResult;

fn reset() {
    rsvz_game::frame::reset_runtime_state_preserving_backend();
    rsvz_game::state_hook::clear_state_hooks();
    rsvz_game::lifecycle::reset_session_resources();
    rsvz_game::session::reset_session_control();
    rsvz::cob::__reset_current();
}

fn ready(backend: &PeBackend, id: PlantId) {
    let plant = backend.plant(id).unwrap().unwrap();
    backend.set_plant_state(plant, 37).unwrap();
}

fn add_cob(backend: &PeBackend, grid: Grid) -> PlantId {
    let plant = backend
        .add_plant(CardSelection::Plant(PlantKind::CobCannon).checked().unwrap(), grid)
        .unwrap();
    let id = backend.plant_id(plant);
    ready(backend, id);
    id
}

#[test]
fn recovery_uses_present_animation_in_both_nonready_animation_states() {
    reset();
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    owner.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            rsvz_current::with_backend_shared(|access| {
                let backend = access;
                let id = add_cob(backend, Grid { row: 0, col: 0 });
                let plant = backend.plant(id).unwrap().unwrap();
                assert_eq!(backend.plant_reanim_circulation(plant).unwrap(), Some(0.0));
                backend.set_plant_state(plant, 36).unwrap();
                assert_eq!(rsvz_game::logic::cob::cob_recover_time(id), 126);
                backend.set_plant_state(plant, 38).unwrap();
                assert_eq!(rsvz_game::logic::cob::cob_recover_time(id), 3475);
            })
            .unwrap();
        });
    });
}

fn assert_fired(selected: PlantId, untouched: PlantId) {
    rsvz_current::with_backend_shared(|access| {
        let backend = access;

        assert_eq!(backend.plant_state(backend.plant(selected).unwrap().unwrap()), 38);
        assert_eq!(backend.plant_state(backend.plant(untouched).unwrap().unwrap()), 37);
        ready(backend, selected);
    })
    .unwrap();
}

#[test]
fn ordinary_core_functions_and_script_overloads_share_the_manager() {
    reset();
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let grid = Grid { row: 0, col: 0 };
            let id = rsvz_current::with_backend_shared(|access| {
                let backend = access;
                add_cob(backend, grid)
            })
            .unwrap();
            rsvz::cob::try_set_cobs([grid]).unwrap();
            let explicit_grid = Grid { row: 1, col: 0 };
            let explicit_id = rsvz_current::with_backend_shared(|access| {
                let backend = access;
                add_cob(backend, explicit_grid)
            })
            .unwrap();
            let explicit = CobManager::new();
            rsvz::cob::try_set_cobs(&explicit, [explicit_grid]).unwrap();
            let copied = try_fire;
            let core_fn: fn(i32, f32) -> RuntimeResult<Option<i32>> = rsvz_game::cob::try_fire::<f32>;
            assert_eq!(core_fn(1, 9.0).unwrap(), Some(0));
            assert_fired(id, explicit_id);
            assert_eq!(copied(1, 9).unwrap(), Some(0));
            assert_fired(id, explicit_id);
            assert_eq!(try_fire([(1, 9)]).unwrap(), [Some(0)]);
            assert_fired(id, explicit_id);
            assert_eq!(try_fire(&explicit, 1, 9.0).unwrap(), Some(0));
            assert_fired(explicit_id, id);
            assert_eq!(try_fire(&explicit, [(1, 9.0)]).unwrap(), [Some(0)]);
            assert_fired(explicit_id, id);
            assert_eq!(fire(1, 9), Some(0));
            assert_fired(id, explicit_id);
            assert_eq!(fire([(1, 9)]), [Some(0)]);
            assert_fired(id, explicit_id);
            assert_eq!(fire(&explicit, 1, 9), Some(0));
            assert_fired(explicit_id, id);
            assert_eq!(fire(&explicit, [(1, 9)]), [Some(0)]);
            assert_fired(explicit_id, id);
            // A callback returning the operation Result uses the unified at output adapter.
            rsvz_current::with_backend_shared(|access| {
                let backend = access;
                ready(backend, id)
            })
            .unwrap();
            rsvz::__run_script(|| {
                rsvz::at(1, 0, || try_fire(1, 9));
                Ok(())
            })
            .unwrap();
            assert_eq!(
                rsvz_game::timeline::dispatch_timeline_tick(WaveTimingSnapshot::minimal(100, Wave(1)), meta(100)),
                TimelineDispatchResult::Continue
            );
        })
    });
    reset();
}

fn meta(clock: i32) -> TickMeta {
    TickMeta {
        clock: Some(clock),
        phase: TickPhase::Playing,
        game_ui: Some(rsvz_model::GameUi::Playing),
        is_new_frame: true,
    }
}

#[test]
fn public_cob_lists_queries_next_and_raw_preserve_the_selected_manager_and_arguments() {
    reset();
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let grids = [
                Grid { row: 0, col: 0 },
                Grid { row: 0, col: 3 },
                Grid { row: 1, col: 0 },
                Grid { row: 1, col: 3 },
            ];
            let ids = rsvz_current::with_backend_shared(|access| {
                let backend = access;
                grids.map(|grid| add_cob(backend, grid))
            })
            .unwrap();
            let explicit = CobManager::new();
            cob::try_set_cobs([grids[0], grids[1]]).unwrap();
            cob::try_set_cobs(&explicit, [grids[2], grids[3]]).unwrap();
            assert_eq!(cob::usable_cob_list().unwrap(), ids[..2]);
            assert_eq!(cob::usable_cob_list(&explicit).unwrap(), ids[2..]);
            assert_eq!(cob::recover_cob(&explicit).unwrap(), Some(ids[2]));
            cob::set_cob_sequential_mode(CobSequentialMode::Time);
            cob::set_cob_sequential_mode(&explicit, CobSequentialMode::Priority);
            assert!(cob::set_next_cob_slot(&explicit, 1).is_err());
            assert!(cob::set_next_cob_slot(1).is_ok());
            cob::set_cob_sequential_mode(&explicit, CobSequentialMode::Time);
            cob::set_next_cob_slot(2).unwrap();
            cob::set_next_cob(&explicit, (2, 4)).unwrap();
            assert_eq!(try_fire(1, 9).unwrap(), Some(1));
            assert_fired(ids[1], ids[3]);
            assert_eq!(try_fire(&explicit, 1, 9).unwrap(), Some(1));
            assert_fired(ids[3], ids[1]);

            cob::set_cob_sequential_mode(CobSequentialMode::Priority);
            cob::move_cobs_to_list_top([(1, 4)]).unwrap();
            cob::erase_cobs_from_list(&explicit, [(2, 1)]);
            let mut default_grids = Vec::new();
            let mut explicit_grids = Vec::new();
            cob::for_each_cob_grid(|grid| default_grids.push(grid));
            cob::for_each_cob_grid(&explicit, |grid| explicit_grids.push(grid));
            assert_eq!(default_grids, [grids[1], grids[0]]);
            assert_eq!(explicit_grids, [grids[3]]);
            assert!(cob::try_set_cobs(&explicit, [(6, 8)]).is_err());
            assert_eq!(cob::usable_cob_list(&explicit).unwrap(), [ids[3]]);
            cob::set_cobs(&explicit, [(6, 8)]);
            cob::try_set_cobs(&explicit, [grids[3]]).unwrap();
            assert_eq!(cob::usable_cob(&explicit).unwrap(), Some(ids[3]));
            assert_eq!(fire(0, 9), None);
            assert_eq!(try_fire(0, 9).unwrap(), None);

            let target = cob_target_to_pixel(CobTarget::from_one_based_row(4, 8.5).unwrap()).unwrap();
            cob::try_raw_fire(2, 4, 4, 8.5).unwrap();
            rsvz_current::with_backend_shared(|access| {
                let backend = access;

                let plant = backend.plant(ids[3]).unwrap().unwrap();
                // PE plant_cob_cannon::launch stores x - 47 in cannon.x.
                assert_eq!(
                    [backend.plant_target_x(plant), backend.plant_target_y(plant)],
                    [target.x - 47, target.y]
                );
            })
            .unwrap();
            assert_fired(ids[3], ids[1]);
            cob::try_raw_fire([(1, 1, 4, 8.5)]).unwrap();
            assert_fired(ids[0], ids[3]);

            cob::try_auto_set_cobs().unwrap();
            cob::try_auto_set_cobs(&explicit, CobListOrder::Vertical).unwrap();
            assert_eq!(cob::usable_cob_list().unwrap(), ids);
            assert_eq!(
                cob::usable_cob_list(&explicit).unwrap(),
                [ids[0], ids[2], ids[1], ids[3]]
            );
        })
    });
    reset();
}

#[test]
fn delayed_current_fire_shares_reservations_and_survives_manager_drop() {
    reset();
    let mut world = PeWorldOwner::new_reset(PeWorldConfig {
        scene: pe_rs::SceneType::Roof,
        ..PeWorldConfig::default()
    })
    .unwrap()
    .install_current()
    .unwrap();
    let (id, observer) = world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let grid = Grid { row: 0, col: 0 };
            let id = rsvz_current::with_backend_shared(|access| {
                let backend = access;
                add_cob(backend, grid)
            })
            .unwrap();
            let source = CobManager::new();
            let observer = CobManager::new();
            let default_grid = Grid { row: 1, col: 3 };
            let default_id = rsvz_current::with_backend_shared(|access| {
                let backend = access;
                add_cob(backend, default_grid)
            })
            .unwrap();
            cob::try_set_cobs([default_grid]).unwrap();
            rsvz::cob::try_set_cobs(&source, [grid]).unwrap();
            rsvz::cob::try_set_cobs(&observer, [grid]).unwrap();
            assert_eq!(cob::roof_usable_cob_list(9.0).unwrap(), [default_id]);
            assert_eq!(cob::roof_usable_cob_list(&source, 9.0).unwrap(), [id]);
            assert_eq!(cob::roof_recover_cob(&source, 9.0).unwrap(), Some(id));
            assert_eq!(cob::roof_recover_cob_list(&source, 9.0).unwrap()[0].id, Some(id));
            assert_eq!(cob::roof_cob_fly_time(1, 9.0).unwrap(), 359);
            assert_eq!(try_fire(&source, 1, 9.0).unwrap(), Some(0));
            assert_eq!(try_fire(&observer, 1, 9.0).unwrap(), None);
            drop(source);
            (id, observer)
        })
    });
    // Existing roof calibration: first-column cob, column-9 drop delays by 28cs.
    for clock in 0..=28 {
        world.with_backend(|backend| {
            rsvz_pvz_emulator_backend::scope_backend(backend, || {
                rsvz_pvz_emulator_backend::with_backend(|backend| assert_eq!(backend.clock().unwrap(), clock));
                let result = rsvz_game::timeline::dispatch_timeline_tick(
                    WaveTimingSnapshot::minimal(clock, Wave(1)),
                    meta(clock),
                );
                assert_eq!(result, TimelineDispatchResult::Continue);
                if clock < 28 {
                    assert_eq!(try_fire(&observer, 1, 9.0).unwrap(), None);
                }
            })
        });
        if clock < 28 {
            world.update_world().unwrap();
        }
    }
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            rsvz_current::with_backend_shared(|access| {
                let backend = access;

                let plant = backend.plant(id).unwrap().unwrap();
                assert_eq!(backend.plant_state(plant), 38);
                ready(backend, id);
            })
            .unwrap();
            assert_eq!(try_fire(&observer, 1, 9.0).unwrap(), Some(0));
        })
    });
    reset();
}

#[test]
fn shovel_overloads_preserve_layers_and_continue_after_invalid_batch_inputs() {
    reset();
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let (main, pumpkin) = rsvz::with_backend(|backend| {
                let grid = Grid { row: 0, col: 0 };
                let main = backend
                    .add_plant(CardSelection::Plant(PlantKind::Sunflower).checked().unwrap(), grid)
                    .unwrap();
                let pumpkin = backend
                    .add_plant(CardSelection::Plant(PlantKind::Pumpkin).checked().unwrap(), grid)
                    .unwrap();
                (backend.plant_id(main), backend.plant_id(pumpkin))
            });
            let alive = |id| {
                rsvz_current::with_backend_shared(|access| {
                    let backend = access;
                    backend
                        .plant(id)
                        .unwrap()
                        .is_some_and(|plant| backend.plant_is_alive(plant))
                })
                .unwrap()
            };
            rsvz::shovel::try_shovel(1, 1, true).unwrap();
            assert!(!alive(pumpkin));
            assert!(alive(main));
            assert!(rsvz::shovel::try_shovel([(0, 1), (1, 1)]).is_err());
            assert!(!alive(main));
            rsvz::shovel::shovel(1, 1, PlantKind::CoffeeBean);
            rsvz::shovel::shovel([(1, 1)]);
        })
    });
    reset();
}

#[test]
fn plant_tick_waits_for_resources_then_finishes_with_real_pe_cards() {
    use rsvz_backend_api::{BattleEntryBackend, CardAppendSelectionBackend, SeedRuleEditBackend};
    use rsvz_schedule::TickDispatchResult;
    reset();
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            rsvz::with_backend(|backend| {
                for kind in [PlantKind::KernelPult, PlantKind::CobCannon] {
                    backend
                        .select_card(CardSelection::Plant(kind).checked().unwrap())
                        .unwrap();
                }
                backend.start_battle().unwrap();
                backend.set_seed_recharge_ignored(true).unwrap();
            });
            rsvz_game::modifier::set_sun(200).unwrap();
            rsvz_game::cob::plant_cob((1, 1)).unwrap();
            let meta = |clock| TickMeta {
                phase: TickPhase::Playing,
                game_ui: Some(rsvz_model::GameUi::Playing),
                clock: Some(clock),
                is_new_frame: true,
            };
            assert_eq!(
                rsvz_game::tick::dispatch_scheduler_tick(meta(1)),
                TickDispatchResult::Continue
            );
            rsvz_current::with_backend_shared(|access| {
                let backend = access;

                let kinds: Vec<_> = backend
                    .plants()
                    .unwrap()
                    .map(|plant| backend.plant_kind(plant).unwrap())
                    .collect();
                assert_eq!(kinds, [PlantKind::KernelPult, PlantKind::KernelPult]);
            })
            .unwrap();
            rsvz_game::modifier::set_sun(1000).unwrap();
            assert_eq!(
                rsvz_game::tick::dispatch_scheduler_tick(meta(2)),
                TickDispatchResult::Continue
            );
            rsvz_current::with_backend_shared(|access| {
                let backend = access;

                let kinds: Vec<_> = backend
                    .plants()
                    .unwrap()
                    .map(|plant| backend.plant_kind(plant).unwrap())
                    .collect();
                assert_eq!(kinds, [PlantKind::CobCannon]);
            })
            .unwrap();
            assert_eq!(
                rsvz_game::tick::dispatch_scheduler_tick(meta(3)),
                TickDispatchResult::Continue
            );
        });
    });
    reset();
}

#[test]
fn batch_abort_keeps_prior_fire_and_releases_borrows_before_the_next_callback() {
    use rsvz_game::logic::cob::{CobManagerError, IntoCobTargets};
    use std::{cell::RefCell, rc::Rc};
    struct AbortAfterFirst;
    impl IntoCobTargets for AbortAfterFirst {
        fn try_for_each_cob_target_result<E>(
            self, visit: &mut impl FnMut(Result<CobTarget, CobManagerError>) -> Result<(), E>,
        ) -> Result<(), E> {
            visit(Ok(CobTarget { row: 0, drop_col: 9.0 }))?;
            rsvz_game::diagnostics::abort_operation(rsvz::RuntimeError::new("second target access failed"));
        }
    }
    reset();
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let grids = [
                Grid { row: 0, col: 0 },
                Grid { row: 1, col: 0 },
                Grid { row: 2, col: 0 },
            ];
            let ids = rsvz_current::with_backend_shared(|access| {
                let backend = access;
                grids.map(|grid| add_cob(backend, grid))
            })
            .unwrap();
            cob::try_set_cobs(grids).unwrap();
            let reports = Rc::new(RefCell::new(Vec::new()));
            let captured = Rc::clone(&reports);
            let previous = rsvz::replace_logger(move |record: &rsvz::LogRecord<'_>| {
                captured.borrow_mut().push(record.message.to_owned())
            });
            rsvz::__run_script(|| {
                rsvz::at(1, 0, || rsvz_game::cob::try_fire_many(AbortAfterFirst));
                rsvz::at(1, 0, || {
                    assert_eq!(rsvz_game::cob::try_fire(1, 9).unwrap(), Some(1));
                });
                Ok(())
            })
            .unwrap();
            assert_eq!(
                rsvz_game::timeline::dispatch_timeline_tick_reporting(
                    WaveTimingSnapshot::minimal(0, Wave(1)),
                    meta(0),
                    &mut rsvz_game::diagnostics::report_runtime_error,
                ),
                TimelineDispatchResult::Continue
            );
            rsvz::restore_logger(previous);
            assert_eq!(reports.borrow().len(), 1);
            assert!(reports.borrow()[0].contains("second target access failed"));
            rsvz_current::with_backend_shared(|access| {
                let backend = access;

                assert_eq!(
                    ids.map(|id| backend.plant_state(backend.plant(id).unwrap().unwrap())),
                    [38, 38, 37]
                );
            })
            .unwrap();
        })
    });
    reset();
}

#[test]
fn delayed_failure_and_queue_clear_release_reservations_without_targeting_replacements() {
    use rsvz_backend_api::PlantRemoveBackend;
    reset();
    let mut world = PeWorldOwner::new_reset(PeWorldConfig {
        scene: pe_rs::SceneType::Roof,
        ..PeWorldConfig::default()
    })
    .unwrap()
    .install_current()
    .unwrap();
    let manager = CobManager::new();
    let replacement = world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let grid = Grid { row: 0, col: 0 };
            let id = rsvz_current::with_backend_shared(|access| {
                let backend = access;
                add_cob(backend, grid)
            })
            .unwrap();
            cob::try_set_cobs(&manager, [grid]).unwrap();
            assert_eq!(try_fire(&manager, 1, 9).unwrap(), Some(0));
            assert_eq!(rsvz_game::timeline::runtime_timeline_diagnostics().pending_count, 1);
            assert_eq!(
                rsvz_game::timeline::dispatch_timeline_tick(WaveTimingSnapshot::minimal(0, Wave(1)), meta(0)),
                TimelineDispatchResult::Continue
            );
            rsvz_current::with_backend_shared(|access| {
                let backend = access;

                backend.remove_plant(backend.plant(id).unwrap().unwrap()).unwrap();
                let replacement = add_cob(backend, grid);
                assert_ne!(id, replacement);
                replacement
            })
            .unwrap()
        })
    });
    for _ in 0..28 {
        world.update_world().unwrap();
    }
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let mut errors = Vec::new();
            assert_eq!(
                rsvz_game::timeline::dispatch_timeline_tick_reporting(
                    WaveTimingSnapshot::minimal(28, Wave(1)),
                    meta(28),
                    &mut |error| errors.push(error),
                ),
                TimelineDispatchResult::Continue
            );
            assert_eq!(errors.len(), 1);
            assert_eq!(rsvz_game::timeline::runtime_timeline_diagnostics().pending_count, 0);
            rsvz_current::with_backend_shared(|access| {
                let backend = access;
                assert_eq!(backend.plant_state(backend.plant(replacement).unwrap().unwrap()), 37)
            })
            .unwrap();
            assert_eq!(try_fire(&manager, 1, 9).unwrap(), Some(0));
            assert_eq!(rsvz_game::timeline::runtime_timeline_diagnostics().pending_count, 1);
            rsvz_schedule::timeline::with_timeline(|timeline| timeline.clear());
            assert_eq!(rsvz_game::timeline::runtime_timeline_diagnostics().pending_count, 0);
            assert_eq!(try_fire(&manager, 1, 9).unwrap(), Some(0));
        })
    });
    reset();
}

#[test]
fn zero_delay_recover_fire_handles_immediate_and_same_clock_deferred_execution() {
    reset();
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let grid = Grid { row: 0, col: 0 };
            let id = rsvz_current::with_backend_shared(|access| {
                let backend = access;
                add_cob(backend, grid)
            })
            .unwrap();
            cob::try_set_cobs([grid]).unwrap();
            assert_eq!(
                rsvz_game::timeline::dispatch_timeline_tick(WaveTimingSnapshot::minimal(0, Wave(1)), meta(0)),
                TimelineDispatchResult::Continue
            );
            assert_eq!(cob::try_recover_fire(1, 9).unwrap(), Some(0));
            rsvz_current::with_backend_shared(|access| {
                let backend = access;

                assert_eq!(backend.plant_state(backend.plant(id).unwrap().unwrap()), 38);
                ready(backend, id);
            })
            .unwrap();
            rsvz::__run_script(|| {
                rsvz::at(1, 0, || {
                    assert_eq!(cob::try_recover_fire(1, 9).unwrap(), Some(0));
                    assert_eq!(rsvz_game::timeline::runtime_timeline_diagnostics().pending_count, 1);
                });
                Ok(())
            })
            .unwrap();
            assert_eq!(
                rsvz_game::timeline::dispatch_timeline_tick(WaveTimingSnapshot::minimal(0, Wave(1)), meta(0)),
                TimelineDispatchResult::Continue
            );
            assert_eq!(rsvz_game::timeline::runtime_timeline_diagnostics().pending_count, 0);
            rsvz_current::with_backend_shared(|access| {
                let backend = access;
                assert_eq!(backend.plant_state(backend.plant(id).unwrap().unwrap()), 38)
            })
            .unwrap();
        })
    });
    reset();
}

#[test]
fn cannon_modes_skip_only_when_allowed_and_batches_keep_logical_failures_in_order() {
    reset();
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let grids = [
                Grid { row: 0, col: 0 },
                Grid { row: 1, col: 0 },
                Grid { row: 2, col: 0 },
            ];
            let ids = rsvz_current::with_backend_shared(|access| {
                let backend = access;
                grids.map(|grid| add_cob(backend, grid))
            })
            .unwrap();
            let manager = CobManager::new();
            for (mode, expected) in [
                (CobSequentialMode::Space, None),
                (CobSequentialMode::Time, Some(1)),
                (CobSequentialMode::Priority, Some(1)),
            ] {
                rsvz_current::with_backend_shared(|access| {
                    let backend = access;

                    for id in ids {
                        ready(backend, id);
                    }
                    backend
                        .set_plant_state(backend.plant(ids[0]).unwrap().unwrap(), 35)
                        .unwrap();
                })
                .unwrap();
                cob::try_set_cobs(&manager, grids).unwrap();
                cob::set_cob_sequential_mode(&manager, mode);
                assert_eq!(try_fire(&manager, 1, 9).unwrap(), expected);
                rsvz_current::with_backend_shared(|access| {
                    let backend = access;

                    assert_eq!(backend.plant_state(backend.plant(ids[0]).unwrap().unwrap()), 35);
                    assert_eq!(
                        backend.plant_state(backend.plant(ids[1]).unwrap().unwrap()),
                        if expected.is_some() { 38 } else { 37 }
                    );
                })
                .unwrap();
            }
            rsvz_current::with_backend_shared(|access| {
                let backend = access;

                for id in ids {
                    ready(backend, id);
                }
            })
            .unwrap();
            cob::set_cob_sequential_mode(&manager, CobSequentialMode::Time);
            cob::try_set_cobs(&manager, grids).unwrap();
            assert_eq!(
                try_fire(&manager, [(0, 9), (1, 9), (1, 9), (1, 9), (1, 9)]).unwrap(),
                [None, Some(0), Some(1), Some(2), None]
            );
            rsvz_current::with_backend_shared(|access| {
                let backend = access;
                assert_eq!(
                    ids.map(|id| backend.plant_state(backend.plant(id).unwrap().unwrap())),
                    [38; 3]
                )
            })
            .unwrap();
        })
    });
    reset();
}

#[test]
fn roof_queries_keep_adjusted_then_raw_then_list_order() {
    use rsvz_backend_api::PlantStateCountdownWriteBackend;
    reset();
    let mut world = PeWorldOwner::new_reset(PeWorldConfig {
        scene: pe_rs::SceneType::Roof,
        ..PeWorldConfig::default()
    })
    .unwrap()
    .install_current()
    .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let grids = [Grid { row: 0, col: 0 }, Grid { row: 1, col: 7 }];
            let ids = rsvz_current::with_backend_shared(|backend| grids.map(|grid| add_cob(backend, grid))).unwrap();
            let manager = CobManager::new();
            manager.set_list_unchecked(grids);
            for (raw, expected, usable) in [([20, 10], 1, true), ([50, 36], 1, false), ([10, 10], 0, true)] {
                rsvz_current::with_backend_shared(|backend| {
                    for (id, recover) in ids.into_iter().zip(raw) {
                        let plant = backend.plant(id).unwrap().unwrap();
                        backend.set_plant_state(plant, 35).unwrap();
                        backend.set_plant_state_countdown(plant, recover - 125).unwrap();
                    }
                })
                .unwrap();
                assert_eq!(manager.roof_recover_cob(9.0).unwrap(), Some(ids[expected]));
                assert_eq!(manager.roof_usable_cob(9.0).unwrap(), usable.then_some(ids[expected]));
                let expected_list = if usable {
                    vec![ids[expected], ids[1 - expected]]
                } else {
                    vec![]
                };
                assert_eq!(manager.roof_usable_list(9.0).unwrap(), expected_list);
            }
        });
    });
    reset();
}
