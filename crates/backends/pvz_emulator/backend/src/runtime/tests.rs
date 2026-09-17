use std::cell::RefCell;
use std::panic::{self, AssertUnwindSafe};

use rsvz_backend_api::backend::{
    BattleEntryBackend, BattleStatusBackend, BoardStateBackend, ClockBackend, CurrentWaveBackend,
    DancerClockWriteBackend, GameUiBackend, GridTerrainBackend, NativeEventBackend, NativeEventSink,
    PlantCreateBackend, PlantReadBackend, PlantStateBackend, PlantVisualStateBackend, RandomControlBackend,
    SceneBackend, SceneEditBackend, SpawnScheduleBackend, SunWriteBackend, WaveRefreshControlBackend,
    WaveTimingBackend, WorldResetBackend, ZombieCreateBackend, ZombieRuleEditBackend,
};
use rsvz_model::model::{
    BattleStatus, BeginPlantEffect, CardSelection, EventFrameStatus, EventInterest, EventToken, GameUi, Grid,
    HomeEntryFact, PlantEffectAttemptFact, PlantEffectOutcome, PlantKind, RandomMode, RandomStreamKind, SceneKind,
    Wave, WorldResetConfig, ZombieKind,
};

use super::*;

#[test]
fn reselection_after_round_preserves_cooldown_by_card_instead_of_slot() {
    use rsvz_backend_api::{
        CardAppendSelectionBackend, CardSelectionReadBackend, SeedBankReadBackend, SeedCooldownReadBackend,
        SeedPacketBackend,
    };
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default()).unwrap();
    owner
        .with_current(|b| {
            b.select_card(CardSelection::Plant(PlantKind::IceShroom).checked().unwrap())
                .unwrap();
            b.select_card(CardSelection::Plant(PlantKind::CherryBomb).checked().unwrap())
                .unwrap();
            let seed = b.seeds().unwrap().next().unwrap();
            b.seed_was_planted(seed).unwrap();
            let cooldown = b.seed_cooldown_remaining(seed);
            with_current_pe_state(b.context(), |s| s.record_update(PeUpdateOutcome::ObjectiveReached)).unwrap();
            assert_eq!(b.selected_card_count().unwrap(), 0);
            b.select_card(CardSelection::Plant(PlantKind::Squash).checked().unwrap())
                .unwrap();
            b.select_card(CardSelection::Plant(PlantKind::IceShroom).checked().unwrap())
                .unwrap();
            b.finish_card_selection().unwrap();
            assert_eq!(
                b.selected_card(0).unwrap().selection(),
                CardSelection::Plant(PlantKind::Squash)
            );
            assert_eq!(b.seed_cooldown_remaining(b.seeds().unwrap().nth(1).unwrap()), cooldown);
        })
        .unwrap();
}

fn with_owner_backend<R>(owner: &mut PeWorldOwner, f: impl FnOnce(&mut PeBackend) -> R) -> Result<R, PeBackendError> {
    owner.with_current(f)
}

thread_local! {
    static EVENT_BEGINS: RefCell<Vec<(u64, i32)>> = const { RefCell::new(Vec::new()) };
    static EVENT_ENDS: RefCell<Vec<EventFrameStatus>> = const { RefCell::new(Vec::new()) };
}

fn record_frame_begin(epoch: u64, counter: i32) {
    EVENT_BEGINS.with_borrow_mut(|begins| begins.push((epoch, counter)));
}

fn ignore_effect(_: PlantEffectAttemptFact) -> BeginPlantEffect {
    BeginPlantEffect::FAIL_OPEN
}

fn ignore_outcome(_: EventToken, _: PlantEffectOutcome) {}

fn ignore_home(_: HomeEntryFact) {}

fn record_frame_end(status: EventFrameStatus) {
    EVENT_ENDS.with_borrow_mut(|ends| ends.push(status));
}

#[test]
fn borrowed_entities_remain_readable_after_same_frame_death() {
    use rsvz_backend_api::{
        PlantCreateBackend, PlantReadBackend, PlantRemoveBackend, ZombieCreateBackend, ZombieReadBackend,
        ZombieRemoveBackend,
    };
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default()).unwrap();
    with_owner_backend(&mut owner, |backend| {
        for col in 0..3 {
            backend
                .add_plant(
                    CardSelection::Plant(PlantKind::Sunflower).checked().unwrap(),
                    Grid { row: 0, col },
                )
                .unwrap();
            backend.add_zombie_in_row(ZombieKind::Normal, 0, 0).unwrap();
        }
        crate::scope_backend(backend, || {
            crate::with_backend_shared(|access| {
                for plant in access.plants().unwrap() {
                    let id = access.plant_id(plant);
                    assert_eq!(access.plant_kind(plant).unwrap(), PlantKind::Sunflower);
                    assert_eq!(access.plant_hp(plant), 300);
                    access.remove_plant(plant).unwrap();
                    assert_eq!(access.plant_id(plant), id);
                    assert!(!access.plant_is_alive(plant));
                }
                for zombie in access.zombies().unwrap() {
                    let id = access.zombie_id(zombie);
                    assert_eq!(access.zombie_kind(zombie).unwrap(), ZombieKind::Normal);
                    assert_eq!(access.zombie_hp(zombie), 270);
                    access.remove_zombie(zombie).unwrap();
                    assert_eq!(access.zombie_id(zombie), id);
                    assert!(!access.zombie_is_alive(zombie));
                }
                assert_eq!(access.plants().unwrap().count(), 0);
                assert_eq!(access.zombies().unwrap().count(), 0);
            })
            .unwrap();
        });
    })
    .unwrap();
}

#[test]
fn ordinary_id_queries_agree_across_nested_shared_borrows() {
    use rsvz_backend_api::{PlantCreateBackend, PlantReadBackend, ZombieCreateBackend, ZombieReadBackend};
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default()).unwrap();
    with_owner_backend(&mut owner, |backend| {
        for col in 0..3 {
            backend
                .add_plant(
                    CardSelection::Plant(PlantKind::Sunflower).checked().unwrap(),
                    Grid { row: 0, col },
                )
                .unwrap();
            backend.add_zombie_in_row(ZombieKind::Normal, 0, 0).unwrap();
        }
        crate::scope_backend(backend, || {
            for _ in 0..2 {
                crate::with_backend_shared(|outer| {
                    for plant in outer.plants().unwrap() {
                        let id = outer.plant_id(plant);
                        crate::with_backend_shared(|inner| {
                            assert_eq!(inner.plant_id(inner.plant(id).unwrap().unwrap()), id);
                        })
                        .unwrap();
                    }
                    for zombie in outer.zombies().unwrap() {
                        let id = outer.zombie_id(zombie);
                        crate::with_backend_shared(|inner| {
                            assert_eq!(inner.zombie_id(inner.zombie(id).unwrap().unwrap()), id);
                        })
                        .unwrap();
                    }
                })
                .unwrap();
            }
        });
    })
    .unwrap();
}

#[test]
fn shared_token_borrow_rejects_exclusive_reentry_and_recovers_on_unwind() {
    use crate::{scope_backend, with_backend_shared};
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("PE world");
    owner
        .with_current(|backend| {
            scope_backend(backend, || {
                let failed = panic::catch_unwind(AssertUnwindSafe(|| {
                    with_backend_shared(|access| {
                        let backend = access;
                        let clock = backend.clock_value().expect("clock");
                        assert_eq!(backend.clock_value().expect("cached world"), clock);
                        with_backend_shared(|nested| {
                            assert_eq!(nested.clock_value().expect("nested world"), clock);
                        })
                        .expect("nested proof");
                        assert!(matches!(
                            crate::try_with_backend(|_| ()),
                            Err(rsvz_backend_api::access::BackendAccessError::BorrowConflict)
                        ));
                        panic!("callback unwind");
                    })
                    .expect("world proof");
                }));
                let failure = failed.expect_err("the explicit callback panic must unwind");
                assert_eq!(failure.downcast_ref::<&str>(), Some(&"callback unwind"));
                crate::with_backend(|backend| backend.update_world()).expect("released world borrow");
                with_backend_shared(|access| assert_eq!(access.clock_value().unwrap(), 1)).unwrap();
            });
        })
        .expect("owner scope");
    assert!(with_backend_shared(|_| panic!("missing owner must not enter callback")).is_err());
}

#[test]
fn pool_iteration_keeps_its_bound_during_creation_and_death() {
    use rsvz_backend_api::backend::{PlantCreateBackend, PlantReadBackend, PlantRemoveBackend};
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("PE world");
    with_owner_backend(&mut owner, |backend| {
        crate::scope_backend(backend, || {
            crate::with_backend_shared(|access| -> Result<(), PeBackendError> {
                let backend = access;
                let selection = CardSelection::Plant(PlantKind::Peashooter)
                    .checked()
                    .expect("selection");
                let first = backend.add_plant(selection, Grid { row: 0, col: 0 })?;
                let second = backend.add_plant(selection, Grid { row: 0, col: 1 })?;
                let mut plants = backend.plants().unwrap();
                let _later = backend.add_plant(selection, Grid { row: 0, col: 2 })?;
                backend.remove_plant(second)?;
                assert_eq!(plants.next(), Some(first));
                assert_eq!(plants.next(), None);

                assert_eq!(backend.plants().unwrap().count(), 2);
                Ok(())
            })
            .expect("Board scope")
        })
    })
    .expect("world scope")
    .expect("pool iteration");
}

#[test]
fn pe_backend_is_zst() {
    assert_eq!(std::mem::size_of::<PeBackend>(), 0);
}

#[test]
fn dancer_clock_write_round_trips_through_board_state() {
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("PE world");
    with_owner_backend(&mut owner, |backend| {
        backend.set_dancer_clock(3_040)?;
        assert_eq!(backend.dancer_clock()?, 3_040);
        Ok::<_, PeBackendError>(())
    })
    .expect("PE current-world scope")
    .expect("dancer clock write");
}

#[test]
fn plant_visual_state_does_not_alias_recently_eaten_countdown() {
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("PE world reset should succeed");
    with_owner_backend(&mut owner, |backend| -> Result<(), PeBackendError> {
        let selection = CardSelection::Plant(PlantKind::Peashooter)
            .checked()
            .expect("valid peashooter selection");
        let plant = backend.add_plant(selection, Grid { row: 0, col: 0 })?;
        assert_eq!(backend.plant_recently_eaten_countdown(plant), 0);
        backend.set_plant_eating_flash_counter(plant, 60)?;
        backend.update_plant_reanim_color(plant)?;
        assert_eq!(backend.plant_recently_eaten_countdown(plant), 0);
        Ok(())
    })
    .expect("PE current-world scope should enter")
    .expect("PE cosmetic no-op should preserve logic state");
}

#[test]
fn timing_atoms_observe_edits_and_recover_after_unwind() {
    use crate::{scope_backend, with_backend_shared};
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default()).unwrap();
    owner
        .with_current(|backend| {
            scope_backend(backend, || {
                let failed = panic::catch_unwind(AssertUnwindSafe(|| {
                    with_backend_shared(|access| {
                        let backend = access;
                        let wave = backend.current_wave().unwrap();
                        backend.refresh_countdown().unwrap();
                        with_backend_shared(|_| backend.initial_countdown().unwrap()).unwrap();
                        backend.huge_wave_countdown().unwrap();
                        backend.level_end_countdown().unwrap();
                        backend.commit_timer_only_wave_refresh(wave, 950).unwrap();
                        assert_eq!(backend.refresh_countdown().unwrap(), 949);
                        panic!("test scope unwind");
                    })
                    .unwrap();
                }));
                assert!(failed.is_err());
                with_backend_shared(|access| access.refresh_countdown().unwrap()).unwrap();
            })
        })
        .unwrap();
}

#[test]
fn indexed_plant_visits_preserve_layer_order_and_propagate_callback_errors() {
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default()).unwrap();
    with_owner_backend(&mut owner, |backend| -> Result<(), PeBackendError> {
        let grid = Grid { row: 2, col: 0 };
        for kind in [PlantKind::LilyPad, PlantKind::Peashooter, PlantKind::Pumpkin] {
            backend.new_plant(CardSelection::Plant(kind).checked().unwrap(), grid)?;
        }
        let mut kinds = Vec::new();
        backend.for_each_plant_at_grid(grid, |plant| {
            kinds.push(backend.plant_kind(plant)?);
            Ok(())
        })?;
        assert_eq!(kinds, [PlantKind::Pumpkin, PlantKind::LilyPad, PlantKind::Peashooter]);
        let mut calls = 0;
        let error = backend
            .for_each_plant_at_grid(grid, |_| {
                calls += 1;
                Err(PeBackendError::OperationRejected("stop visit"))
            })
            .unwrap_err();
        assert!(matches!(error, PeBackendError::OperationRejected("stop visit")));
        assert_eq!(calls, 1);
        backend.for_each_plant_at_anchor_grid(grid, |_| {
            calls += 1;
            Ok(())
        })?;
        assert_eq!(calls, 4);
        Ok(())
    })
    .unwrap()
    .unwrap();
}

#[test]
fn timer_only_wave_refresh_control_updates_pe_countdowns() {
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("PE world reset should succeed");
    let timing = with_owner_backend(&mut owner, |backend| {
        let wave = backend.current_wave()?;
        backend.commit_timer_only_wave_refresh(wave, 950)?;
        Ok::<_, PeBackendError>((backend.refresh_countdown()?, backend.initial_countdown()?))
    })
    .expect("PE current-world scope should enter")
    .expect("PE wave refresh control should succeed");

    assert_eq!(timing.0, 949);
    assert_eq!(timing.1, 950);
    let threshold = with_owner_backend(&mut owner, |backend| backend.current_wave_threshold_health())
        .expect("PE current-world scope should enter")
        .expect("PE threshold should read");
    assert_eq!(threshold, -1);
}

#[test]
fn timer_only_wave_refresh_is_exact_for_ordinary_and_flag_waves() {
    fn updates_until_next_wave(owner: &mut PeWorldOwner, current: i32, initial: i32) -> usize {
        with_owner_backend(owner, |backend| {
            backend.commit_timer_only_wave_refresh(Wave(current), initial)
        })
        .expect("PE current-world scope")
        .expect("commit refresh control");
        for elapsed in 1..=6_000 {
            let wave = with_owner_backend(owner, |backend| {
                backend.update_world()?;
                backend.current_wave()
            })
            .expect("PE current-world scope")
            .expect("update world");
            if wave.0 != current {
                return elapsed;
            }
        }
        panic!("next wave did not refresh");
    }

    let mut ordinary = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("ordinary world");
    with_owner_backend(&mut ordinary, PeBackend::update_world)
        .expect("PE current-world scope")
        .expect("advance to wave time 1");
    assert_eq!(updates_until_next_wave(&mut ordinary, 0, 601), 600);

    let mut flag = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("flag world");
    with_owner_backend(&mut flag, |backend| {
        let spawn = backend.with_current_world(|world| world.scene().spawn_data().as_ptr())?;
        // SAFETY: the scalar belongs to the current test world.
        unsafe { std::ptr::addr_of_mut!((*spawn).wave).write(9) };
        Ok::<_, PeBackendError>(())
    })
    .expect("PE current-world scope")
    .expect("prepare flag wave");
    assert_eq!(updates_until_next_wave(&mut flag, 9, 601), 1_345);

    with_owner_backend(&mut flag, |backend| {
        let spawn = backend.with_current_world(|world| world.scene().spawn_data().as_ptr())?;
        // SAFETY: the scalar belongs to the current test world.
        unsafe { std::ptr::addr_of_mut!((*spawn).wave).write(20) };
        backend.commit_timer_only_wave_refresh(Wave(20), 601)
    })
    .expect("PE current-world scope")
    .expect_err("final wave must be rejected");
}

#[test]
fn reset_seed_wins_over_seeded_mode_and_locked_mode_persists() {
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("PE world");
    with_owner_backend(&mut owner, |backend| {
        backend.set_random_mode(RandomMode::Seeded(11))?;
        backend.reset_world(WorldResetConfig {
            seed: 22,
            completed_rounds: 63,
            ..WorldResetConfig::default()
        })?;
        assert_eq!(backend.random_seed(RandomStreamKind::Battle)?, 22);
        assert_eq!(backend.random_seed(RandomStreamKind::Level)?, 22);

        backend.set_random_mode(RandomMode::Locked(7))?;
        backend.reset_world(WorldResetConfig {
            seed: 33,
            completed_rounds: 63,
            ..WorldResetConfig::default()
        })?;
        for stream in [RandomStreamKind::Battle, RandomStreamKind::Level] {
            assert_eq!(backend.random_seed(stream)?, 33);
            assert!(crate::scope_backend(backend, || crate::with_backend_shared(|access| {
                access.random_locked(stream)
            })
            .expect("Board scope")));
            assert_eq!(
                crate::scope_backend(backend, || crate::with_backend_shared(
                    |access| access.random_fixed(stream)
                )
                .expect("Board scope")),
                7
            );
        }
        Ok::<_, PeBackendError>(())
    })
    .expect("PE current-world scope")
    .expect("reset seeds");
}

#[test]
fn low_round_reset_is_typed_unsupported() {
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("PE world");
    let error = with_owner_backend(&mut owner, |backend| {
        backend.reset_world(WorldResetConfig {
            completed_rounds: 62,
            ..WorldResetConfig::default()
        })
    })
    .expect("PE current-world scope")
    .expect_err("low rounds must be unsupported");
    assert!(matches!(error, PeBackendError::Unsupported(_)));
}

#[test]
fn scene_switch_preserves_opening_world_state_and_is_rejected_after_battle_start() {
    let mut owner = PeWorldOwner::new_reset_deferred_spawn(PeWorldConfig::default()).expect("PE world");
    with_owner_backend(&mut owner, |backend| {
        backend.pick_spawn_list()?;
        backend.set_random_mode(RandomMode::Locked(7))?;
        backend.set_zombie_spawn_stopped(true)?;
        backend.with_current_world(|world| world.scene().set_main_counter(17))??;
        let epoch = with_current_pe_state(backend.context(), |state| state.world_epoch)?;
        let battle_rng = (
            backend.random_seed(RandomStreamKind::Battle)?,
            crate::scope_backend(backend, || {
                crate::with_backend_shared(|access| access.random_locked(RandomStreamKind::Battle))
                    .expect("Board scope")
            }),
            crate::scope_backend(backend, || {
                crate::with_backend_shared(|access| access.random_fixed(RandomStreamKind::Battle)).expect("Board scope")
            }),
        );

        backend.set_scene(SceneKind::Roof)?;
        assert_eq!(backend.game_ui()?, GameUi::Playing);
        assert_eq!(backend.clock()?, 17);
        assert!(backend.zombie_spawn_stopped()?);
        assert_eq!(
            (
                backend.random_seed(RandomStreamKind::Battle)?,
                crate::scope_backend(backend, || crate::with_backend_shared(
                    |access| access.random_locked(RandomStreamKind::Battle)
                )
                .expect("Board scope")),
                crate::scope_backend(backend, || crate::with_backend_shared(
                    |access| access.random_fixed(RandomStreamKind::Battle)
                )
                .expect("Board scope"))
            ),
            battle_rng
        );
        assert_eq!(
            with_current_pe_state(backend.context(), |state| state.world_epoch)?,
            epoch
        );
        assert_eq!(backend.config()?.scene, pe_rs::SceneType::Roof);

        backend.set_zombie_spawn_stopped(false)?;
        backend.set_random_mode(RandomMode::Seeded(5489))?;
        backend.start_battle()?;
        assert_eq!(backend.current_wave()?.0, 0);
        assert!(matches!(
            backend.set_scene(SceneKind::Roof),
            Err(PeBackendError::OperationRejected(_))
        ));
        assert!(matches!(
            backend.set_scene(SceneKind::Day),
            Err(PeBackendError::OperationRejected(_))
        ));
        for _ in 0..600 {
            backend.update_world()?;
        }
        assert_eq!(backend.current_wave()?.0, 1);
        assert!(matches!(
            backend.set_scene(SceneKind::Day),
            Err(PeBackendError::OperationRejected(_))
        ));
        Ok::<_, PeBackendError>(())
    })
    .expect("PE current-world scope")
    .expect("scene switch lifecycle");
}

#[test]
fn spawn_pick_rejects_invalid_candidate_sets_without_entering_native_sampling() {
    let mut empty = PeWorldOwner::new_reset_deferred_spawn(PeWorldConfig::default()).expect("PE world");
    with_owner_backend(&mut empty, |backend| {
        backend.set_spawn_type_allowed(ZombieKind::Normal, false)?;
        assert!(matches!(
            backend.pick_spawn_list(),
            Err(PeBackendError::OperationRejected(_))
        ));
        Ok::<_, PeBackendError>(())
    })
    .expect("PE current-world scope")
    .expect("empty candidates should be rejected");

    for (scene, kind) in [
        (pe_rs::SceneType::Day, ZombieKind::Snorkel),
        (pe_rs::SceneType::Roof, ZombieKind::Dancing),
    ] {
        let mut owner = PeWorldOwner::new_reset_deferred_spawn(PeWorldConfig {
            scene,
            ..PeWorldConfig::default()
        })
        .expect("PE world");
        with_owner_backend(&mut owner, |backend| {
            backend.set_spawn_type_allowed(kind, true)?;
            assert!(matches!(
                backend.pick_spawn_list(),
                Err(PeBackendError::OperationRejected(_))
            ));
            Ok::<_, PeBackendError>(())
        })
        .expect("PE current-world scope")
        .expect("invalid candidates should be rejected");
    }
}

#[test]
fn add_zombie_in_row_rejects_negative_from_wave_before_pe_factory() {
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("PE world reset should succeed");
    let result = with_owner_backend(&mut owner, |backend| {
        backend.add_zombie_in_row(ZombieKind::Normal, 0, -1).map(|_| ())
    })
    .expect("PE current-world scope should enter");

    assert!(matches!(
        result,
        Err(PeBackendError::Unsupported(
            "negative AddZombieInRow from_wave sentinel"
        ))
    ));
}

#[test]
fn pe_grid_map_override_matches_the_default_live_pool_semantics() {
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("PE world reset should succeed");
    let result = with_owner_backend(&mut owner, |backend| {
        crate::scope_backend(backend, || {
            crate::with_backend_shared(|access| -> Result<(), PeBackendError> {
                let backend = access;
                let peashooter = CardSelection::Plant(PlantKind::Peashooter)
                    .checked()
                    .expect("valid selection");
                let cob = CardSelection::Plant(PlantKind::CobCannon)
                    .checked()
                    .expect("valid selection");
                backend.new_plant(peashooter, Grid { row: 0, col: 0 })?;
                backend.new_plant(cob, Grid { row: 1, col: 2 })?;

                for grid in [
                    Grid { row: 0, col: 0 },
                    Grid { row: 1, col: 2 },
                    Grid { row: 1, col: 3 },
                    Grid { row: 4, col: 8 },
                ] {
                    let mut map_ids = Vec::new();
                    backend.for_each_plant_at_grid(grid, |plant| {
                        map_ids.push(backend.plant_id(plant));
                        Ok(())
                    })?;

                    let mut scan_ids = Vec::new();
                    for plant in backend.plants().unwrap() {
                        let anchor = Grid {
                            row: backend.plant_row(plant),
                            col: backend.plant_col(plant),
                        };
                        if rsvz_model::plant_occupies_grid(anchor, backend.plant_kind(plant)?, grid) {
                            scan_ids.push(backend.plant_id(plant));
                        }
                    }
                    map_ids.sort_unstable();
                    scan_ids.sort_unstable();
                    assert_eq!(map_ids, scan_ids, "grid-map mismatch at {grid:?}");
                }
                Ok(())
            })
            .expect("Board scope")
        })
    });

    result
        .expect("PE current-world scope should enter")
        .expect("grid-map and pool scan should agree");
}

#[test]
fn grid_and_sun_boundaries_match_native_representable_inputs() {
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("PE world");
    with_owner_backend(&mut owner, |backend| {
        assert!(matches!(
            backend.is_pool_square(-1, 2),
            Err(PeBackendError::InvalidGrid)
        ));
        assert!(matches!(backend.is_pool_square(0, 6), Err(PeBackendError::InvalidGrid)));
        backend.set_sun(i32::MAX as u32)?;
        assert!(matches!(
            backend.set_sun(i32::MAX as u32 + 1),
            Err(PeBackendError::NumericOutOfRange("sun"))
        ));
        Ok::<_, PeBackendError>(())
    })
    .expect("PE current-world scope")
    .expect("boundary validation");
}

#[test]
fn unsupported_spawn_flag_can_be_cleared_but_not_enabled() {
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("PE world reset should succeed");
    let cleared = with_owner_backend(&mut owner, |backend| -> Result<bool, PeBackendError> {
        backend.set_spawn_type_allowed(ZombieKind::Bobsled, false)?;
        let cleared =
            !backend.with_current_world(|world| world.scene().spawn_flag(ZombieKind::Bobsled.code() as u32))??;
        assert!(matches!(
            backend.set_spawn_type_allowed(ZombieKind::Bobsled, true),
            Err(PeBackendError::UnsupportedKind {
                kind: "zombie",
                name: "Bobsled"
            })
        ));
        Ok(cleared)
    })
    .expect("PE current-world scope should enter")
    .expect("unsupported spawn flag should clear");

    assert!(cleared);
}

#[test]
fn jack_rule_is_real_but_pepper_rule_is_typed_unsupported() {
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("PE world reset should succeed");
    let result = with_owner_backend(&mut owner, |backend| -> Result<(), PeBackendError> {
        assert!(!backend.jack_explosions_disabled()?);
        backend.set_jack_explosions_disabled(true)?;
        assert!(backend.jack_explosions_disabled()?);

        assert!(matches!(
            backend.pepper_explosions_disabled(),
            Err(PeBackendError::Unsupported("PE has no pepper zombie explosion rule"))
        ));
        assert!(matches!(
            backend.set_pepper_explosions_disabled(true),
            Err(PeBackendError::Unsupported("PE has no pepper zombie explosion rule"))
        ));
        assert!(backend.jack_explosions_disabled()?);
        Ok(())
    });

    result
        .expect("PE current-world scope should enter")
        .expect("split zombie explosion rules should behave exactly");
}

#[test]
fn current_world_scope_restores_after_panic() {
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("PE world reset should succeed");
    let panic_result = panic::catch_unwind(AssertUnwindSafe(|| {
        let _scope = owner.with_current(|_| {
            assert!(with_current_pe_world(|_| ()).is_ok());
            panic!("scope restoration probe");
        });
    }));

    assert!(panic_result.is_err());
    assert!(matches!(
        with_current_pe_world(|_| ()),
        Err(PeBackendError::NotInitialized)
    ));
}

#[test]
fn shared_world_access_nests_and_excludes_physical_mutation() {
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("PE world reset should succeed");
    let result = owner
        .with_current(|_| with_current_pe_world(|_| with_current_pe_world(|_| ())))
        .expect("outer PE current-world scope should enter");

    assert!(matches!(result, Ok(Ok(()))));
    owner
        .with_current(|backend| {
            with_current_pe_world(|_| {
                assert!(matches!(
                    with_current_pe_mut(backend.context(), |_, _| ()),
                    Err(PeBackendError::WorldScopeConflict)
                ));
            })
            .expect("shared borrow");
            with_current_pe_mut(backend.context(), |_, _| {
                assert!(matches!(
                    with_current_pe_world(|_| ()),
                    Err(PeBackendError::WorldScopeConflict)
                ));
            })
            .expect("exclusive borrow");
        })
        .expect("owner scope");
}

#[test]
fn current_world_scope_rejects_same_thread_second_owner() {
    let mut first = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("first PE world reset should succeed");
    let mut second = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("second PE world reset should succeed");

    let result = first
        .with_current(|_| second.with_current(|_| ()))
        .expect("first PE current-world scope should enter");

    assert!(matches!(result, Err(PeBackendError::WorldScopeConflict)));
}

#[test]
fn current_state_is_owner_scoped() {
    let first_config = PeWorldConfig {
        scene: pe_rs::SceneType::Day,
        battle_seed: 1,
        ..PeWorldConfig::default()
    };
    let second_config = PeWorldConfig {
        scene: pe_rs::SceneType::Roof,
        battle_seed: 2,
        ..PeWorldConfig::default()
    };
    let mut first = PeWorldOwner::new_reset(first_config).expect("first PE world reset should succeed");
    let mut second = PeWorldOwner::new_reset(second_config).expect("second PE world reset should succeed");

    with_owner_backend(&mut first, |backend| {
        assert_eq!(backend.config().expect("config should read"), first_config);
        backend.update_world().expect("PE frame should update");
        assert_eq!(backend.clock_value().expect("clock should read"), 1);
    })
    .expect("first scope should enter");
    with_owner_backend(&mut second, |backend| {
        assert_eq!(backend.config().expect("config should read"), second_config);
        assert_eq!(backend.clock_value().expect("clock should read"), 0);
    })
    .expect("second scope should enter");
    with_owner_backend(&mut first, |backend| {
        assert_eq!(backend.config().expect("config should read"), first_config);
        assert_eq!(backend.clock_value().expect("clock should read"), 1);
    })
    .expect("first scope should re-enter");
}

#[test]
fn deferred_reset_resets_state_and_scene() {
    let mut config = PeWorldConfig {
        scene: pe_rs::SceneType::Pool,
        ..PeWorldConfig::default()
    };
    let mut owner = PeWorldOwner::new_reset(config).expect("PE world reset should succeed");

    with_owner_backend(&mut owner, |backend| {
        backend.update_world().expect("PE frame should update");
        assert_eq!(backend.clock_value().expect("clock should read"), 1);

        config.scene = pe_rs::SceneType::Roof;
        backend
            .reset_with_config_deferred_spawn(config)
            .expect("PE world should reset");
        assert_eq!(backend.clock_value().expect("clock should reset"), 0);
        assert_eq!(
            backend.config().expect("config should read").scene,
            pe_rs::SceneType::Roof
        );
        assert_eq!(
            SceneBackend::scene(backend).expect("scene should read"),
            SceneKind::Roof
        );
    })
    .expect("scope should enter");
}

#[test]
fn update_world_advances_clock_and_keeps_running_status() {
    let config = PeWorldConfig::default();
    let mut owner = PeWorldOwner::new_reset(config).expect("PE world reset should succeed");

    with_owner_backend(&mut owner, |backend| {
        assert_eq!(ClockBackend::clock(backend).expect("clock should read"), 0);
        assert_eq!(
            BattleStatusBackend::battle_status(backend).expect("battle status should read"),
            BattleStatus::Running
        );

        backend.update_world().expect("PE frame should update");
        assert_eq!(ClockBackend::clock(backend).expect("clock should read"), 1);
        assert_eq!(
            BattleStatusBackend::battle_status(backend).expect("battle status should read"),
            BattleStatus::Running
        );
    })
    .expect("scope should enter");
}

#[test]
fn game_over_is_a_sticky_normal_update_outcome() {
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("PE world");
    with_owner_backend(&mut owner, |backend| {
        let zombie = backend
            .add_zombie_in_row(ZombieKind::Normal, 0, 0)?
            .expect("zombie should spawn");
        // SAFETY: the handle is tied to this current world; these scalar writes
        // place the fixture beyond the native home-entry boundary.
        unsafe {
            std::ptr::addr_of_mut!((*zombie.as_mut_ptr()).int_x).write(-100);
            std::ptr::addr_of_mut!((*zombie.as_mut_ptr()).x).write(-100.0);
            std::ptr::addr_of_mut!((*zombie.as_mut_ptr()).dx).write(0.0);
        }
        assert_eq!(backend.update_world()?, PeUpdateOutcome::GameOver);
        assert_eq!(backend.update_world()?, PeUpdateOutcome::GameOver);
        Ok::<_, PeBackendError>(())
    })
    .expect("PE current-world scope")
    .expect("game over outcome");
}

#[test]
fn native_sink_wraps_exactly_each_world_update_and_detaches() {
    EVENT_BEGINS.with_borrow_mut(Vec::clear);
    EVENT_ENDS.with_borrow_mut(Vec::clear);
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("PE world reset should succeed");

    with_owner_backend(&mut owner, |backend| {
        backend
            .install_native_event_sink(NativeEventSink {
                interest: EventInterest::BITE,
                begin_logic_frame: record_frame_begin,
                begin_plant_effect: ignore_effect,
                finish_plant_effect: ignore_outcome,
                emit_home_entry: ignore_home,
                emit_gargantuar_spawned: |_| {},
                emit_imp_thrown: |_| {},
                emit_gargantuar_ash_hit: |_| {},
                end_logic_frame: record_frame_end,
            })
            .expect("install event sink");
        for _ in 0..20 {
            backend.update_world().expect("PE frame should update");
        }
        backend.remove_native_event_sink().expect("remove event sink");
        backend.update_world().expect("unobserved PE frame should update");
    })
    .expect("scope should enter");

    EVENT_BEGINS.with_borrow(|begins| {
        assert_eq!(begins.len(), 20);
        assert!(begins.iter().map(|(_, counter)| *counter).eq(0..20));
        assert!(begins.iter().all(|(epoch, _)| *epoch == begins[0].0));
    });
    EVENT_ENDS.with_borrow(|ends| assert_eq!(ends.as_slice(), [BattleStatus::Running; 20]));
}

#[test]
fn reset_sequence_is_deterministic_for_the_same_level_seed() {
    fn signature(owner: &mut PeWorldOwner) -> (u32, u32, u32, u32, [i32; 4]) {
        owner
            .with_current(|_| {
                with_current_pe_world(|world| -> pe_rs::Result<_> {
                    let scene = world.scene();
                    let sun = scene.sun_data().as_ptr();
                    let spawn = scene.spawn_data().as_ptr();
                    // SAFETY: Both pointers are current-world borrows and
                    // only setup scalars are copied into the test value.
                    let (sun_money, sun_countdown, total_flags) = unsafe {
                        (
                            std::ptr::addr_of!((*sun).sun).read(),
                            std::ptr::addr_of!((*sun).natural_sun_countdown).read(),
                            std::ptr::addr_of!((*spawn).total_flags).read(),
                        )
                    };
                    let entries = [
                        scene.spawn_entry(0, 0)?.to_raw(),
                        scene.spawn_entry(0, 1)?.to_raw(),
                        scene.spawn_entry(1, 0)?.to_raw(),
                        scene.spawn_entry(19, 49)?.to_raw(),
                    ];
                    Ok((sun_money, sun_countdown, scene.dancer_clock(), total_flags, entries))
                })
            })
            .expect("PE current-world scope should enter")
            .expect("PE world borrow should enter")
            .expect("PE signature should read")
    }

    let config = PeWorldConfig {
        level_seed: 0x1234_5678,
        total_flags: 1_260,
        initial_sun: 1_234,
        ..PeWorldConfig::default()
    };
    let mut first = PeWorldOwner::new_reset(config).expect("first PE reset should succeed");
    let mut second = PeWorldOwner::new_reset(config).expect("second PE reset should succeed");

    assert_eq!(signature(&mut first), signature(&mut second));
    let signature = signature(&mut first);
    assert_eq!(signature.0, config.initial_sun);
    assert_eq!(signature.3, config.total_flags);
}

#[test]
fn deferred_spawn_reset_does_not_consume_spawn_rng() {
    fn spawn_entries(owner: &mut PeWorldOwner) -> [i32; 4] {
        owner
            .with_current(|_| {
                with_current_pe_world(|world| -> pe_rs::Result<_> {
                    let scene = world.scene();
                    Ok([
                        scene.spawn_entry(0, 0)?.to_raw(),
                        scene.spawn_entry(0, 1)?.to_raw(),
                        scene.spawn_entry(1, 0)?.to_raw(),
                        scene.spawn_entry(19, 49)?.to_raw(),
                    ])
                })
            })
            .expect("PE current-world scope should enter")
            .expect("PE world borrow should enter")
            .expect("PE spawn signature should read")
    }

    let config = PeWorldConfig {
        level_seed: 0x91a2_b3c4,
        ..PeWorldConfig::default()
    };
    let mut ordinary = PeWorldOwner::new_reset(config).expect("ordinary PE reset should succeed");
    let mut deferred = PeWorldOwner::new_reset_deferred_spawn(config).expect("deferred PE reset should succeed");

    with_owner_backend(&mut deferred, |backend| {
        backend.pick_spawn_list().expect("single deferred B13 should succeed");
    })
    .expect("deferred PE scope should enter");

    assert_eq!(spawn_entries(&mut deferred), spawn_entries(&mut ordinary));
}

#[test]
fn natural_spawn_pick_preserves_written_flags_and_skips_flag_rng() {
    let mut owner =
        PeWorldOwner::new_reset_deferred_spawn(PeWorldConfig::default()).expect("deferred PE reset should succeed");
    with_owner_backend(&mut owner, |backend| -> Result<(), PeBackendError> {
        backend.set_spawn_type_allowed(ZombieKind::Normal, true)?;
        backend.set_spawn_type_allowed(ZombieKind::Conehead, false)?;
        backend.set_spawn_type_allowed(ZombieKind::Gargantuar, true)?;
        backend.pick_spawn_list()?;
        let (normal, cone, garg) = backend.with_current_world(|world| {
            let scene = world.scene();
            Ok::<_, pe_rs::Error>((
                scene.spawn_flag(ZombieKind::Normal.code() as u32)?,
                scene.spawn_flag(ZombieKind::Conehead.code() as u32)?,
                scene.spawn_flag(ZombieKind::Gargantuar.code() as u32)?,
            ))
        })??;
        assert!(normal && !cone && garg);
        assert!(backend.spawn_ready()?);
        Ok(())
    })
    .expect("PE current-world scope should enter")
    .expect("natural spawn pick should preserve configured flags");
}

#[test]
fn repeated_default_spawn_pick_keeps_the_initialized_allowed_set() {
    let mut owner =
        PeWorldOwner::new_reset_deferred_spawn(PeWorldConfig::default()).expect("deferred PE reset should succeed");
    with_owner_backend(&mut owner, |backend| -> Result<(), PeBackendError> {
        backend.pick_spawn_list()?;
        let first = backend.with_current_world(|world| {
            let scene = world.scene();
            let mut flags = [false; 33];
            for (index, flag) in flags.iter_mut().enumerate() {
                *flag = scene.spawn_flag(index as u32)?;
            }
            Ok::<_, pe_rs::Error>(flags)
        })??;
        backend.pick_spawn_list()?;
        let second = backend.with_current_world(|world| {
            let scene = world.scene();
            let mut flags = [false; 33];
            for (index, flag) in flags.iter_mut().enumerate() {
                *flag = scene.spawn_flag(index as u32)?;
            }
            Ok::<_, pe_rs::Error>(flags)
        })??;
        assert_eq!(second, first, "a later generation must not repick allowed types");
        Ok(())
    })
    .expect("PE current-world scope should enter")
    .expect("repeated default spawn pick should preserve allowed types");
}
