#![cfg(feature = "pvz-emulator")]

use rsvz_backend_api::{PlantCreateBackend, PlantReadBackend, ZombieCreateBackend, ZombieReadBackend};
use rsvz_model::{CardSelection, Grid, PlantKind, ZombieKind};
use rsvz_pvz_emulator_backend::{PeWorldConfig, runner_internal::PeWorldOwner};

#[test]
fn current_entity_values_reresolve_ids_and_remove_once() {
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let (plant_id, zombie_id) = rsvz::with_backend(|backend| {
                let plant = backend
                    .add_plant(
                        CardSelection::Plant(PlantKind::Sunflower).checked().unwrap(),
                        Grid { row: 1, col: 2 },
                    )
                    .unwrap();
                let zombie = backend.add_zombie_in_row(ZombieKind::Normal, 1, 0).unwrap().unwrap();
                (backend.plant_id(plant), backend.zombie_id(zombie))
            });
            let plant = rsvz::Plant::from_id(plant_id);
            let zombie = rsvz::Zombie::from_id(zombie_id);
            let plant_copy = plant;
            assert_eq!(plant.hp(), 300);
            rsvz_current::with_backend_shared(|_| {
                assert_eq!(plant.id(), plant_id);
                assert!(plant.is_alive());
                assert_eq!(plant.hp(), 300);
                assert_eq!(plant.grid(), Grid { row: 1, col: 2 });
                assert_eq!(plant.kind(), PlantKind::Sunflower);
                assert_eq!(zombie.id(), zombie_id);
                assert!(zombie.is_alive());
                assert_eq!(zombie.hp(), 270);
                assert_eq!(zombie.row(), 1);
                assert_eq!(zombie.kind(), ZombieKind::Normal);
                assert!(zombie.state().is_some());
            })
            .unwrap();

            let mut plants = Vec::new();
            rsvz::for_each_plant(|view| {
                assert_eq!(plant.hp(), view.hp);
                plants.push(view.id);
            });
            assert_eq!(plants, [plant_id]);
            let mut zombies = Vec::new();
            rsvz::for_each_zombie(|view| {
                assert_eq!(zombie.row(), view.row);
                zombies.push(view.id);
            });
            assert_eq!(zombies, [zombie_id]);

            assert!(plant.remove_by_id());
            assert!(!plant_copy.remove_by_id());
            assert!(!plant_copy.is_alive());
            assert_eq!(plant_copy.hp().into_option(), None);
            assert!(!plant_copy.grid().is_live());
            assert_ne!(plant_copy.hp(), 300);
            assert!(zombie.remove_by_id());
            assert!(!zombie.remove_by_id());
            assert!(!zombie.is_alive());
            assert_eq!(zombie.hp().into_option(), None);
            assert!(!zombie.kind().is_live());
            assert_ne!(zombie.hp(), 270);
        })
    });
}

#[test]
fn grid_item_id_removal_is_exact_and_missing_ids_are_noops() {
    use rsvz_backend_api::{GridItemCreateBackend, GridItemReadBackend};
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    owner.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            rsvz_current::with_backend_shared(|access| {
                let backend = access;
                let first = backend.add_ladder(Grid { row: 0, col: 2 }).unwrap();
                let second = backend.add_ladder(Grid { row: 0, col: 3 }).unwrap();
                let id = backend.grid_item_id(first);
                rsvz_game::modifier::remove_grid_item_by_id(id).unwrap();
                assert!(!backend.grid_item_is_alive(first));
                assert!(backend.grid_item_is_alive(second));
                rsvz_game::modifier::remove_grid_item_by_id(id).unwrap();
                rsvz_game::modifier::remove_grid_item_by_id(rsvz_model::GridItemId::from_raw(0)).unwrap();
                assert_eq!(backend.grid_items().unwrap().count(), 1);
            })
            .unwrap();
        });
    });
}

#[test]
fn conflicting_id_read_aborts_its_callback_without_returning_a_default() {
    use std::cell::Cell;
    use std::rc::Rc;
    rsvz_game::frame::reset_runtime_state_preserving_backend();
    rsvz_game::session::reset_session_control();
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
            let after_read = Rc::new(Cell::new(false));
            let reached = Rc::clone(&after_read);
            let reports = Rc::new(std::cell::RefCell::new(Vec::new()));
            let captured = Rc::clone(&reports);
            let logger = rsvz::replace_logger(move |record: &rsvz::LogRecord<'_>| {
                captured.borrow_mut().push(record.message.to_owned())
            });
            rsvz::__run_script(|| {
                rsvz::at(1, 0, move || {
                    rsvz::with_backend(|_| {
                        let _value = rsvz::Plant::from_id(rsvz_model::PlantId::from_raw(0)).hp();
                    });
                    reached.set(true);
                });
                rsvz::at(1, 0, || rsvz_game::modifier::set_sun(123));
                Ok(())
            })
            .unwrap();
            assert_eq!(
                rsvz_game::timeline::dispatch_timeline_tick_reporting(
                    rsvz_model::WaveTimingSnapshot::minimal(current_clock, rsvz_model::Wave(1)),
                    rsvz_schedule::TickMeta {
                        phase: rsvz_schedule::TickPhase::Playing,
                        game_ui: Some(rsvz_model::GameUi::Playing),
                        clock: Some(current_clock),
                        is_new_frame: true
                    },
                    &mut rsvz_game::diagnostics::report_runtime_error,
                ),
                rsvz_schedule::TimelineDispatchResult::Continue
            );
            assert!(!after_read.get());
            assert_eq!(reports.borrow().len(), 1);
            assert!(reports.borrow()[0].contains("current backend access conflicts"));
            assert_eq!(
                rsvz_current::with_backend_shared(|access| rsvz_backend_api::SunQueryBackend::sun(access).unwrap())
                    .unwrap(),
                123
            );
            assert!(!rsvz_game::session::script_stop_requested());
            rsvz::restore_logger(logger);
        });
    });
    rsvz_game::frame::reset_runtime_state_preserving_backend();
    rsvz_game::session::reset_session_control();
}

#[test]
fn unbound_any_dispatch_read_aborts_locally_and_keeps_repeating_tasks() {
    use rsvz_schedule::tick::{TickControl, TickDispatchResult, TickMeta, TickOptions, TickPhase, TickTaskState};
    use std::cell::Cell;
    use std::rc::Rc;

    rsvz_game::frame::reset_runtime_state_preserving_backend();
    rsvz_game::session::reset_session_control();
    let attempted = Rc::new(Cell::new(0));
    let reached_after_read = Rc::new(Cell::new(false));
    let subsequent = Rc::new(Cell::new(0));
    let failed = rsvz_game::tick::spawn(TickOptions::any_dispatch(), {
        let attempted = Rc::clone(&attempted);
        let reached_after_read = Rc::clone(&reached_after_read);
        move |_| {
            attempted.set(attempted.get() + 1);
            let _ = rsvz_game::modifier::clear_plants();
            reached_after_read.set(true);
            Ok(TickControl::Continue)
        }
    });
    let successful = rsvz_game::tick::spawn(TickOptions::any_dispatch(), {
        let subsequent = Rc::clone(&subsequent);
        move |_| {
            subsequent.set(subsequent.get() + 1);
            Ok(TickControl::Continue)
        }
    });
    let meta = TickMeta {
        phase: TickPhase::Unavailable,
        game_ui: None,
        clock: None,
        is_new_frame: false,
    };
    let mut errors = Vec::new();
    for count in 1..=2 {
        let result = rsvz_game::tick::dispatch_scheduler_tick_reporting(meta, &mut |error| {
            errors.push(error.to_string());
        });
        assert_eq!(result, TickDispatchResult::Continue);
        assert_eq!(attempted.get(), count);
        assert_eq!(subsequent.get(), count);
        assert_eq!(errors.len(), count);
        assert!(!reached_after_read.get());
        assert!(!rsvz_game::session::script_stop_requested());
        rsvz_schedule::tick::with_scheduler(|scheduler| {
            assert_eq!(scheduler.state(failed), TickTaskState::Running);
            assert_eq!(scheduler.state(successful), TickTaskState::Running);
        });
    }
    assert!(
        errors
            .iter()
            .all(|error| error.contains("current backend is not scoped"))
    );
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    owner.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            assert_eq!(
                rsvz_game::tick::dispatch_scheduler_tick_reporting(meta, &mut |error| errors.push(error.to_string())),
                TickDispatchResult::Continue,
            );
            assert!(reached_after_read.get());
            assert_eq!(attempted.get(), 3);
            assert_eq!(subsequent.get(), 3);
            assert_eq!(errors.len(), 2);
        });
    });
    rsvz_game::frame::reset_runtime_state_preserving_backend();
}

#[test]
fn immediate_first_board_read_without_an_owner_ends_only_that_callback() {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    rsvz_game::frame::reset_runtime_state_preserving_backend();
    rsvz_game::session::reset_session_control();
    let meta = rsvz_schedule::TickMeta {
        phase: rsvz_schedule::TickPhase::Playing,
        game_ui: Some(rsvz_model::GameUi::Playing),
        clock: Some(100),
        is_new_frame: true,
    };
    // Timing metadata may prime the queue, but cannot manufacture a Board proof.
    let mut dispatch_errors = Vec::new();
    assert_eq!(
        rsvz_game::timeline::dispatch_timeline_tick_reporting(
            rsvz_model::WaveTimingSnapshot::minimal(100, rsvz_model::Wave(1)),
            meta,
            &mut |error| dispatch_errors.push(error.to_string()),
        ),
        rsvz_schedule::timeline::TimelineDispatchResult::Continue,
    );
    assert!(dispatch_errors.is_empty());
    let after_read = Rc::new(Cell::new(false));
    let reports = Rc::new(RefCell::new(Vec::new()));
    let captured = Rc::clone(&reports);
    let logger = rsvz::replace_logger(move |record: &rsvz::LogRecord<'_>| {
        captured.borrow_mut().push(record.message.to_owned());
    });
    let reached = Rc::clone(&after_read);
    assert!(
        rsvz::at(1, 0, move || {
            let _ = rsvz_game::modifier::clear_plants();
            reached.set(true);
        })
        .is_some()
    );
    let next_ran = Rc::new(Cell::new(false));
    let reached = Rc::clone(&next_ran);
    assert!(rsvz::at(1, 0, move || reached.set(true)).is_some());
    assert!(!after_read.get());
    assert!(next_ran.get());
    assert_eq!(reports.borrow().len(), 1);
    assert!(reports.borrow()[0].contains("current backend is not scoped"));
    assert!(!rsvz_game::session::script_stop_requested());
    rsvz::restore_logger(logger);
    rsvz_game::frame::reset_runtime_state_preserving_backend();
}

#[test]
fn current_modifiers_preserve_validation_order_and_target_selection() {
    use rsvz_game::modifier::*;
    use rsvz_model::{ObjectEditOutcome, PlantId, ZombieId};
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            rsvz_current::with_backend_shared(|access| {
                let native = access;
                let grid = Grid { row: 1, col: 2 };
                let plant = native
                    .new_plant(CardSelection::Plant(PlantKind::Sunflower).checked().unwrap(), grid)
                    .unwrap();
                let id = native.plant_id(plant);
                assert_eq!(
                    set_plant_hp(PlantId::from_raw(0), 0).unwrap(),
                    ObjectEditOutcome::Missing
                );
                assert!(matches!(set_plant_hp(id, 0), Err(ModifierValueError::InvalidValue(_))));
                assert_eq!(native.plant_hp(plant), 300);
                assert_eq!(set_plant_hp_by_grid(grid, 120).unwrap(), ObjectEditOutcome::Applied);
                assert_eq!(native.plant_hp(plant), 120);
                assert_eq!(
                    set_plant_hp_by_grid(Grid { row: 0, col: 0 }, 120).unwrap(),
                    ObjectEditOutcome::Missing
                );
                assert!(matches!(set_all_plants_hp(0), Err(ModifierValueError::InvalidValue(_))));
                assert_eq!(set_all_plants_hp(400).unwrap(), 1);
                keep_plant_hp_tick(id, 350).unwrap();
                assert_eq!(native.plant_hp(plant), 350);
                let normal = spawn_zombie(ZombieKind::Normal, grid).unwrap();
                let cone = spawn_zombie(ZombieKind::Conehead, grid).unwrap();
                assert_ne!(normal, cone);
                assert_eq!(
                    set_zombie_body_hp(ZombieId::from_raw(0), 0).unwrap(),
                    ObjectEditOutcome::Missing
                );
                assert!(matches!(
                    set_zombie_body_hp(normal, 0),
                    Err(ModifierValueError::InvalidValue(_))
                ));
                assert_eq!(set_zombie_body_hp_by_kind(ZombieKind::Normal, 100).unwrap(), 1);
                assert_eq!(rsvz::Zombie::from_id(normal).hp(), 100);
                assert_eq!(rsvz::Zombie::from_id(cone).hp(), 270);
                assert!(matches!(
                    set_zombie_x(ZombieId::from_raw(0), f32::NAN),
                    Err(ModifierValueError::InvalidValue(_))
                ));
                assert_eq!(
                    set_zombie_x(ZombieId::from_raw(0), 80.0).unwrap(),
                    ObjectEditOutcome::Missing
                );
                assert_eq!(set_all_zombies_body_hp(200).unwrap(), 2);
                assert_eq!(rsvz::Zombie::from_id(cone).hp(), 200);
                pin_zombie_x_tick(normal, 400.0).unwrap();
                assert_eq!(rsvz::Zombie::from_id(normal).state().unwrap().x, 400.0);
            })
            .unwrap();
        })
    });
}

#[test]
fn effect_modifiers_edit_exact_targets_and_retention_skips_stale_ids() {
    use rsvz_backend_api::PlantStateWriteBackend;
    use rsvz_game::logic::card_timing::receipt::{RetentionState, cleanup_retained};
    use rsvz_game::modifier::*;
    use rsvz_model::{ObjectEditOutcome, PlantId};
    use std::cell::Cell;
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| rsvz_pvz_emulator_backend::scope_backend(backend, || {
        rsvz_current::with_backend_shared(|access| {
            let native = access;
            let first_grid = Grid { row: 0, col: 8 };
            let second_grid = Grid { row: 3, col: 8 };
            let first = native.new_plant(CardSelection::Plant(PlantKind::IceShroom).checked().unwrap(), first_grid).unwrap();
            let second = native.new_plant(CardSelection::Plant(PlantKind::IceShroom).checked().unwrap(), second_grid).unwrap();
            native.set_plant_state(first, 2).unwrap();
            native.set_plant_state(second, 2).unwrap();
            let first_id = native.plant_id(first);
            let second_id = native.plant_id(second);
            normalize_effect_countdown_by_id(first_id, 50).unwrap();
            normalize_effect_countdown_by_grid(PlantKind::IceShroom, second_grid, 10).unwrap();
            assert_eq!(native.plant_effect_countdown(first), 50);
            assert_eq!(native.plant_effect_countdown(second), 10);
            assert_eq!(normalize_effect_countdown_by_id(PlantId::from_raw(0), 10).unwrap(), ObjectEditOutcome::Missing);
            assert!(matches!(normalize_effect_countdown_by_id(first_id, -1), Err(PlantEffectCountdownError::NegativeTarget)));
            native.set_plant_state(first, 0).unwrap();
            assert!(matches!(normalize_effect_countdown_by_id(first_id, 10), Err(PlantEffectCountdownError::PlantNotActive(id)) if id == first_id));
            let retained = Cell::new(RetentionState { main: Some(PlantId::from_raw(0)), container: Some(second_id) });
            cleanup_retained(&retained).unwrap();
            assert!(native.plant(first_id).unwrap().is_some());
            assert!(native.plant(second_id).unwrap().is_none());
            assert!(retained.get().main.is_none() && retained.get().container.is_none());
            assert_eq!(remove_plant_kind_at(PlantKind::IceShroom, first_grid).unwrap(), Some(first_id));
            assert!(!remove_plant_by_id(first_id).unwrap());
        }).unwrap();
    }));
}

#[test]
fn row_moves_preserve_vertical_offset_and_partial_ensure_progress() {
    use rsvz_backend_api::{ZombiePositionWriteBackend, ZombieVerticalPositionBackend};
    use rsvz_game::logic::zombies::{EnsureZombieRowsError, ensure_zombie_rows_one_based, move_zombie_to_row_by_id};
    use rsvz_game::modifier::{clear_zombies, spawn_zombie};
    use rsvz_model::{I32RepresentableF32, ZombieId};
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            rsvz_current::with_backend_shared(|access| {
                let native = access;
                let id = spawn_zombie(ZombieKind::GigaGargantuar, Grid { row: 4, col: 7 }).unwrap();
                let zombie = native.zombie(id).unwrap().expect("spawned zombie");
                let original_base = native.zombie_pos_y_based_on_row(zombie, 4).unwrap();
                native
                    .set_zombie_row_and_y(zombie, 4, I32RepresentableF32::new(original_base + 13.0).unwrap())
                    .unwrap();
                let target_base = native.zombie_pos_y_based_on_row(zombie, 0).unwrap();
                assert!(move_zombie_to_row_by_id(id, 0).unwrap());
                assert_eq!(native.zombie_row(zombie), 0);
                assert_eq!(native.zombie_pos_y(zombie), target_base + 13.0);
                assert!(!move_zombie_to_row_by_id(ZombieId::from_raw(0), 0).unwrap());
                ensure_zombie_rows_one_based(ZombieKind::GigaGargantuar, [1, 1]).unwrap();
                assert_eq!(native.zombie_pos_y(zombie), target_base + 13.0);
                assert!(matches!(
                    ensure_zombie_rows_one_based(ZombieKind::GigaGargantuar, [3]),
                    Err(EnsureZombieRowsError::RowDisallowed { .. })
                ));
                assert_eq!(native.zombie_row(zombie), 0);
                clear_zombies().unwrap();
                let id = spawn_zombie(ZombieKind::GigaGargantuar, Grid { row: 4, col: 7 }).unwrap();
                assert!(matches!(
                    ensure_zombie_rows_one_based(ZombieKind::GigaGargantuar, [1, 2]),
                    Err(EnsureZombieRowsError::InsufficientZombies { row: 2, .. })
                ));
                assert_eq!(rsvz::Zombie::from_id(id).row(), 0);
                let surplus = spawn_zombie(ZombieKind::GigaGargantuar, Grid { row: 0, col: 7 }).unwrap();
                ensure_zombie_rows_one_based(ZombieKind::GigaGargantuar, [1, 2]).unwrap();
                let rows = [
                    rsvz::Zombie::from_id(id).row().expect_live("first"),
                    rsvz::Zombie::from_id(surplus).row().expect_live("second"),
                ];
                assert!(rows.contains(&0) && rows.contains(&1));
                assert_eq!(clear_zombies().unwrap(), 2);
            })
            .unwrap();
        })
    });
}

#[test]
fn legacy_selectors_keep_iteration_order_and_board_bounds() {
    use rsvz_game::logic::selectors::*;
    use rsvz_game::modifier::{set_zombie_x, spawn_zombie};
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let first = spawn_zombie(ZombieKind::Normal, Grid { row: 0, col: 7 }).unwrap();
            let second = spawn_zombie(ZombieKind::Normal, Grid { row: 0, col: 7 }).unwrap();
            set_zombie_x(first, 700.0).unwrap();
            set_zombie_x(second, 300.0).unwrap();
            assert_eq!(frontmost_zombie_by_row(0).unwrap(), Some(first));
            assert_eq!(frontmost_zombie_by_row(5).unwrap(), None);
            assert_eq!(nearest_threat().unwrap(), None);
            let balloon = spawn_zombie(ZombieKind::Balloon, Grid { row: 1, col: 7 }).unwrap();
            assert_eq!(nearest_threat().unwrap(), Some(balloon));
            let grids = safe_grids_for_plant(PlantKind::Peashooter).unwrap();
            assert_eq!(grids.len(), 54);
            assert_eq!(grids[0], Grid { row: 0, col: 0 });
            assert_eq!(grids[53], Grid { row: 5, col: 8 });
        })
    });
}
