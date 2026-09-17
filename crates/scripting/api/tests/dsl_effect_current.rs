#![cfg(feature = "pvz-emulator")]

use rsvz::dsl::{card, i, keep, mi, to};
use rsvz_backend_api::{
    BattleEntryBackend, CardAppendSelectionBackend, ClockBackend, PlantReadBackend, SeedRuleEditBackend,
    SunCostRuleEditBackend, ZombieRuleEditBackend,
};
use rsvz_model::{CardSelection, GameUi, Grid, PlantId, PlantKind, Wave, WaveTimingSnapshot};
use rsvz_pvz_emulator_backend::{
    PeWorldConfig, pe_rs,
    runner_internal::{PeWorldOwner, PeWorldRun},
};
use rsvz_schedule::tick::{TickMeta, TickPhase};

fn world(scene: pe_rs::SceneType) -> PeWorldRun {
    world_with_cards(
        scene,
        &[
            CardSelection::Plant(PlantKind::IceShroom),
            CardSelection::Imitator(PlantKind::IceShroom),
            CardSelection::Plant(PlantKind::CoffeeBean),
            CardSelection::Plant(PlantKind::FlowerPot),
            CardSelection::Plant(PlantKind::Sunflower),
        ],
    )
}

fn world_with_cards(scene: pe_rs::SceneType, selections: &[CardSelection]) -> PeWorldRun {
    rsvz::reset_runtime_state_preserving_backend();
    let mut world = PeWorldOwner::new_reset(PeWorldConfig {
        scene,
        ..PeWorldConfig::default()
    })
    .unwrap()
    .install_current()
    .unwrap();
    scoped(&mut world, || {
        rsvz::with_backend(|backend| {
            for &selection in selections {
                backend.select_card(selection.checked().unwrap()).unwrap();
            }
            backend.start_battle().unwrap();
            backend.set_seed_recharge_ignored(true).unwrap();
            backend.set_sun_cost_ignored(true).unwrap();
            backend.set_zombie_spawn_stopped(true).unwrap();
        })
    });
    world
}

fn scoped<T>(world: &mut PeWorldRun, f: impl FnOnce() -> T) -> T {
    world.with_backend(|backend| rsvz_pvz_emulator_backend::scope_backend(backend, f))
}

fn at_clock(world: &mut PeWorldRun, clock: i32) {
    at_clock_in_wave(world, clock, 1);
}

fn at_clock_in_wave(world: &mut PeWorldRun, clock: i32, wave: i32) {
    while world.with_backend(|backend| backend.clock().unwrap()) < clock {
        world.update_world().unwrap();
    }
    scoped(world, || {
        let result = rsvz_game::timeline::dispatch_timeline_tick(
            WaveTimingSnapshot::minimal(clock, Wave(wave)),
            TickMeta {
                phase: TickPhase::Playing,
                game_ui: Some(GameUi::Playing),
                clock: Some(clock),
                is_new_frame: true,
            },
        );
        assert_eq!(result, rsvz_schedule::TimelineDispatchResult::Continue, "clock {clock}");
    });
}

fn plants(world: &mut PeWorldRun) -> Vec<(PlantId, PlantKind)> {
    scoped(world, || {
        rsvz_current::with_backend_shared(|access| {
            let backend = access;

            backend
                .plants()
                .unwrap()
                .map(|plant| (backend.plant_id(plant), backend.plant_kind(plant).unwrap()))
                .collect()
        })
        .unwrap()
    })
}

#[test]
fn ordinary_and_imitator_ice_keep_day_night_placement_and_normalization_times() {
    for (scene, imitator, placement) in [
        (pe_rs::SceneType::Day, false, 701),
        (pe_rs::SceneType::Night, false, 900),
        (pe_rs::SceneType::Day, true, 381),
        (pe_rs::SceneType::Night, true, 580),
    ] {
        let mut world = world(scene);
        scoped(&mut world, || {
            rsvz::__run_script(|| {
                (1, 1000) << if imitator { mi(2, 9) } else { i(2, 9) };
                Ok(())
            })
            .unwrap()
        });
        at_clock(&mut world, 0);
        for clock in 1..placement {
            at_clock(&mut world, clock);
        }
        assert!(plants(&mut world).is_empty());
        at_clock(&mut world, placement);
        assert!(!plants(&mut world).is_empty());
        for clock in placement + 1..=990 {
            at_clock(&mut world, clock);
        }
        let ice = plants(&mut world)
            .into_iter()
            .find(|(_, kind)| *kind == PlantKind::IceShroom)
            .expect("ice successor")
            .0;
        let countdown = scoped(&mut world, || {
            rsvz_current::with_backend_shared(|access| {
                let backend = access;
                backend.plant_effect_countdown(backend.plant(ice).unwrap().unwrap())
            })
            .unwrap()
        });
        assert_eq!(countdown, 10);
    }
}

#[test]
fn retained_imitators_clean_up_before_at_and_after_the_morph_boundary() {
    for delay in [319, 320, 321] {
        let mut world = world(pe_rs::SceneType::Day);
        scoped(&mut world, || {
            rsvz::__run_script(|| {
                (1, 100) << card(keep(delay), CardSelection::Imitator(PlantKind::IceShroom), 2, 9);
                Ok(())
            })
            .unwrap()
        });
        for clock in 0..100 + delay {
            at_clock(&mut world, clock);
        }
        assert!(!plants(&mut world).is_empty(), "before cleanup {delay}");
        at_clock(&mut world, 100 + delay);
        assert!(plants(&mut world).is_empty(), "cleanup {delay}");
    }
}

#[test]
fn expression_reuse_allocates_distinct_receipts_and_preserves_existing_containers() {
    use rsvz_backend_api::PlantCreateBackend;
    let mut world = world(pe_rs::SceneType::Roof);
    let existing = scoped(&mut world, || {
        rsvz::with_backend(|backend| {
            let plant = backend
                .add_plant(
                    CardSelection::Plant(PlantKind::FlowerPot).checked().unwrap(),
                    Grid { row: 1, col: 8 },
                )
                .unwrap();
            backend.plant_id(plant)
        })
    });
    scoped(&mut world, || {
        rsvz::__run_script(|| {
            let expression = card(keep(10), PlantKind::Sunflower, 2, 9);
            (1, 100) << &expression;
            (1, 200) << &expression;
            (1, 300) << card(to(310), PlantKind::Sunflower, 3, 9);
            Ok(())
        })
        .unwrap()
    });
    for clock in 0..=310 {
        at_clock(&mut world, clock);
        if [110, 210, 310].contains(&clock) {
            assert_eq!(plants(&mut world), [(existing, PlantKind::FlowerPot)]);
        }
    }
}

#[test]
fn effect_retention_cleans_on_each_side_of_the_day_and_night_morph_boundary() {
    for (scene, boundary) in [(pe_rs::SceneType::Day, 701), (pe_rs::SceneType::Night, 900)] {
        for cleanup in [boundary - 1, boundary, boundary + 1] {
            let mut world = world(scene);
            scoped(&mut world, || {
                rsvz::__run_script(|| {
                    (1, 1000) << mi(to(cleanup), 2, 9);
                    Ok(())
                })
                .unwrap()
            });
            for clock in 0..=cleanup {
                at_clock(&mut world, clock);
            }
            assert!(
                plants(&mut world)
                    .iter()
                    .all(|(_, kind)| *kind == PlantKind::CoffeeBean)
            );
            for clock in cleanup + 1..=1001 {
                at_clock(&mut world, clock);
            }
            assert!(
                plants(&mut world)
                    .iter()
                    .all(|(_, kind)| *kind == PlantKind::CoffeeBean)
            );
        }
    }
}

#[test]
fn explicit_coffee_only_wakes_the_requested_stored_ice() {
    use rsvz_backend_api::PlantCreateBackend;
    let mut world = world(pe_rs::SceneType::Day);
    let (other, target) = scoped(&mut world, || {
        rsvz::with_backend(|backend| {
            let selection = CardSelection::Plant(PlantKind::IceShroom).checked().unwrap();
            let other = backend.add_plant(selection, Grid { row: 1, col: 8 }).unwrap();
            let target = backend.add_plant(selection, Grid { row: 3, col: 8 }).unwrap();
            (backend.plant_id(other), backend.plant_id(target))
        })
    });
    scoped(&mut world, || {
        rsvz::__run_script(|| {
            (1, 1000) << rsvz::dsl::ci(4, 9);
            Ok(())
        })
        .unwrap()
    });
    for clock in 0..=990 {
        at_clock(&mut world, clock);
    }
    scoped(&mut world, || {
        rsvz_current::with_backend_shared(|access| {
            let backend = access;

            let other = backend.plant(other).unwrap().unwrap();
            let target = backend.plant(target).unwrap().unwrap();
            assert!(backend.plant_is_sleeping(other));
            assert!(!backend.plant_is_sleeping(target));
            assert_eq!(backend.plant_effect_countdown(target), 10);
        })
        .unwrap()
    });
}

#[test]
fn explicit_coffee_failures_keep_the_core_card_diagnostic() {
    let mut world = world(pe_rs::SceneType::Roof);
    scoped(&mut world, || {
        rsvz::__run_script(|| {
            (1, 1000) << rsvz::dsl::ci(4, 9);
            Ok(())
        })
        .unwrap()
    });
    for clock in 0..=700 {
        at_clock(&mut world, clock);
    }
    world.update_world().unwrap();
    let result = scoped(&mut world, || {
        rsvz_game::timeline::dispatch_timeline_tick(
            WaveTimingSnapshot::minimal(701, Wave(1)),
            TickMeta {
                phase: TickPhase::Playing,
                game_ui: Some(GameUi::Playing),
                clock: Some(701),
                is_new_frame: true,
            },
        )
    });
    let rsvz_schedule::TimelineDispatchResult::OperationError(error) = result else {
        panic!("expected card error")
    };
    assert!(error.message().starts_with("种植咖啡豆到 (4, 9) 失败"), "{error}");
}

#[test]
fn timed_cleanup_does_not_remove_a_replacement_at_the_same_grid() {
    use rsvz_backend_api::PlantCreateBackend;
    let mut world = world(pe_rs::SceneType::Day);
    scoped(&mut world, || {
        rsvz::__run_script(|| {
            (1, 100) << card(keep(10), PlantKind::Sunflower, 2, 9);
            Ok(())
        })
        .unwrap()
    });
    for clock in 0..=100 {
        at_clock(&mut world, clock);
    }
    let old = plants(&mut world)[0].0;
    let replacement = scoped(&mut world, || {
        assert!(rsvz::Plant::from_id(old).remove_by_id());
        rsvz::with_backend(|backend| {
            let plant = backend
                .add_plant(
                    CardSelection::Plant(PlantKind::Sunflower).checked().unwrap(),
                    Grid { row: 1, col: 8 },
                )
                .unwrap();
            backend.plant_id(plant)
        })
    });
    for clock in 101..=110 {
        at_clock(&mut world, clock);
    }
    assert_eq!(plants(&mut world), [(replacement, PlantKind::Sunflower)]);
}

#[test]
fn multiwave_bindings_of_one_expression_keep_separate_receipts() {
    let mut world = world(pe_rs::SceneType::Day);
    scoped(&mut world, || {
        rsvz::__run_script(|| {
            let expression = card(keep(10), PlantKind::Sunflower, 2, 9);
            rsvz::dsl::waves([1, 2]);
            100 << expression;
            Ok(())
        })
        .unwrap()
    });
    let mut first = None;
    for clock in 0..=1110 {
        at_clock_in_wave(&mut world, clock, if clock < 1000 { 1 } else { 2 });
        if clock == 100 {
            first = Some(plants(&mut world)[0].0);
        }
        if clock == 1100 {
            assert_ne!(first.unwrap(), plants(&mut world)[0].0);
        }
        if clock == 110 || clock == 1110 {
            assert!(plants(&mut world).is_empty());
        }
    }
}

#[test]
fn ice_filler_uses_each_seed_once_and_preserves_partial_planting_when_sun_runs_out() {
    use rsvz_game::logic::ice_filler::{IceFiller, IceFillerPriority};
    for (sun, expected) in [(75, 1), (150, 2)] {
        let mut world = world(pe_rs::SceneType::Day);
        scoped(&mut world, || {
            rsvz::with_backend(|backend| backend.set_sun_cost_ignored(false).unwrap());
            rsvz_game::modifier::set_sun(sun).unwrap();
            let grids = [Grid { row: 0, col: 0 }, Grid { row: 1, col: 0 }];
            let mut filler = IceFiller::new();
            filler.start(grids).unwrap();
            filler.set_priority_mode(IceFillerPriority::Hp);
            filler.tick().unwrap();
            let planted = rsvz_current::with_backend_shared(|access| {
                let backend = access;

                backend
                    .plants()
                    .unwrap()
                    .map(|plant| {
                        (
                            Grid {
                                row: backend.plant_row(plant),
                                col: backend.plant_col(plant),
                            },
                            backend.plant_raw_kind(plant).unwrap(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap();
            assert_eq!(planted.len(), expected);
            assert_eq!(planted[0], (grids[0], PlantKind::IceShroom));
            if expected == 2 {
                assert_eq!(planted[1], (grids[1], PlantKind::Imitator));
            }
            filler.pause();
            rsvz_game::modifier::set_sun(1000).unwrap();
            filler.tick().unwrap();
            assert_eq!(
                rsvz_current::with_backend_shared(|access| {
                    let backend = access;
                    backend.plants().unwrap().count()
                })
                .unwrap(),
                expected
            );
        });
    }
}

#[test]
fn ice_filler_coffee_prioritizes_temporary_positions_then_low_hp_with_reverse_ties() {
    use rsvz_backend_api::{PlantCreateBackend, PlantHealthWriteBackend};
    use rsvz_game::logic::ice_filler::{IceFiller, IceFillerPriority};
    for (hp, temporary, expected) in [(100, false, 1), (1, false, 0), (1, true, 1)] {
        let mut world = world(pe_rs::SceneType::Day);
        scoped(&mut world, || {
            let grids = [Grid { row: 0, col: 0 }, Grid { row: 1, col: 0 }];
            rsvz::with_backend(|backend| {
                for (grid, hp) in [(grids[0], hp), (grids[1], 100)] {
                    let plant = backend
                        .add_plant(CardSelection::Plant(PlantKind::IceShroom).checked().unwrap(), grid)
                        .unwrap();
                    backend
                        .set_plant_hp(plant, rsvz_model::PositiveHp::new(hp).unwrap())
                        .unwrap();
                }
            });
            let mut filler = IceFiller::new();
            filler.set_priority_mode(IceFillerPriority::Hp);
            if temporary {
                filler.set_list([grids[0]]);
                filler.set_temp_positions([grids[1]]);
            } else {
                filler.set_list(grids);
            }
            assert_eq!(filler.coffee().unwrap(), Some(grids[expected]));
            if temporary {
                assert_eq!(filler.coffee().unwrap(), Some(grids[0]));
            }
        });
    }
}

#[test]
fn ice_filler_start_failure_leaves_the_previous_list_and_activity_unchanged() {
    use rsvz_game::logic::ice_filler::{IceFiller, IceFillerConfigError, IceFillerError};
    let mut world = world(pe_rs::SceneType::Day);
    scoped(&mut world, || {
        let a = Grid { row: 0, col: 0 };
        let b = Grid { row: 1, col: 0 };
        let mut filler = IceFiller::new();
        filler.start([a]).unwrap();
        filler
            .set_ice_seed_list([CardSelection::Imitator(PlantKind::DoomShroom)])
            .unwrap();
        let failure = filler.start([b]);
        assert!(
            matches!(
                failure,
                Err(IceFillerError::Config(IceFillerConfigError::MissingIceSeed))
            ),
            "{failure:?}"
        );
        filler
            .set_ice_seed_list([CardSelection::Plant(PlantKind::IceShroom)])
            .unwrap();
        filler.tick().unwrap();
        assert_eq!(
            rsvz_current::with_backend_shared(|access| {
                let backend = access;
                backend
                    .plants()
                    .unwrap()
                    .map(|plant| Grid {
                        row: backend.plant_row(plant),
                        col: backend.plant_col(plant),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap(),
            [a]
        );
    });
}

#[test]
fn plant_fixer_repairs_equal_low_hp_in_order_and_retries_when_sun_is_available() {
    use rsvz_backend_api::{PlantCreateBackend, PlantHealthWriteBackend};
    use rsvz_game::logic::plant_fixer::PlantFixer;
    let mut world = world(pe_rs::SceneType::Day);
    scoped(&mut world, || {
        let grids = [
            Grid { row: 0, col: 0 },
            Grid { row: 1, col: 0 },
            Grid { row: 2, col: 0 },
        ];
        let ids = rsvz::with_backend(|backend| {
            backend.set_sun_cost_ignored(false).unwrap();
            grids.map(|grid| {
                let plant = backend
                    .add_plant(CardSelection::Plant(PlantKind::Sunflower).checked().unwrap(), grid)
                    .unwrap();
                if grid.row < 2 {
                    backend
                        .set_plant_hp(plant, rsvz_model::PositiveHp::new(100).unwrap())
                        .unwrap();
                }
                backend.plant_id(plant)
            })
        });
        let mut fixer = PlantFixer::new();
        fixer.start_with(PlantKind::Sunflower, grids, 200.0, false).unwrap();
        rsvz_game::modifier::set_sun(0).unwrap();
        fixer.tick().unwrap();
        assert!(
            rsvz_current::with_backend_shared(|access| {
                let backend = access;
                ids.iter().all(|id| backend.plant(*id).unwrap().is_some())
            })
            .unwrap()
        );
        // Native cost bypass must not bypass the fixer's own sun budget.
        rsvz::with_backend(|backend| backend.set_sun_cost_ignored(true).unwrap());
        fixer.tick().unwrap();
        rsvz::with_backend(|backend| {
            assert!(ids.iter().all(|id| backend.plant(*id).unwrap().is_some()));
            backend.set_sun_cost_ignored(false).unwrap();
        });
        rsvz_game::modifier::set_sun(50).unwrap();
        fixer.tick().unwrap();
        assert_eq!(
            rsvz_current::with_backend_shared(|access| {
                let backend = access;
                ids.map(|id| backend.plant(id).unwrap().is_some())
            })
            .unwrap(),
            [false, true, true]
        );
        rsvz_game::modifier::set_sun(50).unwrap();
        fixer.tick().unwrap();
        assert_eq!(
            rsvz_current::with_backend_shared(|access| {
                let backend = access;
                ids.map(|id| backend.plant(id).unwrap().is_some())
            })
            .unwrap(),
            [false, false, true]
        );
        assert_eq!(
            rsvz_current::with_backend_shared(|access| {
                let backend = access;
                backend.plants().unwrap().count()
            })
            .unwrap(),
            3
        );
    });
}

#[test]
fn plant_fixer_skips_crater_and_repairs_other_low_hp_target() {
    use rsvz_backend_api::{GridItemCreateBackend, PlantCreateBackend, PlantHealthWriteBackend};
    use rsvz_game::logic::plant_fixer::PlantFixer;
    let mut world = world(pe_rs::SceneType::Day);
    scoped(&mut world, || {
        let blocked = Grid { row: 0, col: 0 };
        let damaged = Grid { row: 1, col: 0 };
        let id = rsvz::with_backend(|backend| {
            backend.add_crater(blocked).unwrap();
            let p = backend
                .add_plant(CardSelection::Plant(PlantKind::Sunflower).checked().unwrap(), damaged)
                .unwrap();
            backend
                .set_plant_hp(p, rsvz_model::PositiveHp::new(100).unwrap())
                .unwrap();
            backend.plant_id(p)
        });
        let mut fixer = PlantFixer::new();
        fixer
            .start_with(PlantKind::Sunflower, [blocked, damaged], 200.0, false)
            .unwrap();
        fixer.tick().unwrap();
        rsvz::with_backend(|backend| {
            assert!(backend.plant(id).unwrap().is_none());
            assert_eq!(backend.plants().unwrap().count(), 1);
            let p = backend.plants().unwrap().next().unwrap();
            assert_eq!(backend.plant_hp(p), 300);
            assert_eq!(backend.plant_row(p), 1);
        });
    });
}

#[test]
fn dynamic_cards_observe_prepared_spawn_and_fixed_cards_replace_callback() {
    use rsvz_backend_api::BoardStateBackend;
    use std::cell::Cell;
    thread_local! { static CALLS: Cell<u32> = const { Cell::new(0) }; }
    fn choose() -> rsvz::runtime::RuntimeResult<Vec<CardSelection>> {
        CALLS.set(CALLS.get() + 1);
        let red = rsvz::with_backend(|backend| backend.spawn_allowed(32).unwrap());
        Ok(vec![CardSelection::Plant(if red {
            PlantKind::CherryBomb
        } else {
            PlantKind::Sunflower
        })])
    }
    let mut world = world(pe_rs::SceneType::Day);
    scoped(&mut world, || {
        CALLS.set(0);
        rsvz_game::setup::reset_script_setup();
        rsvz::setup::select_cards_with(choose);
        rsvz_game::setup::set_zombies(
            rsvz_game::logic::zombies::random_zombie_types([rsvz_model::ZombieKind::GigaGargantuar], []),
            rsvz_model::ZombieSpawnMode::Natural,
        );
        rsvz_game::setup::prepare_current_opening(123).unwrap();
        assert_eq!(CALLS.get(), 1);
        rsvz_game::setup::with_script_setup(|setup| {
            assert_eq!(
                setup.desired_cards,
                Some(vec![CardSelection::Plant(PlantKind::CherryBomb)])
            )
        });
        rsvz_game::setup::select_cards(vec![CardSelection::Plant(PlantKind::Sunflower)]);
        rsvz_game::setup::prepare_current_opening(124).unwrap();
        assert_eq!(CALLS.get(), 1);
    });
}

#[test]
fn independent_fixers_keep_their_targets_and_skip_only_the_blocked_hole() {
    use rsvz_backend_api::GridItemCreateBackend;
    use rsvz_game::logic::plant_fixer::PlantFixer;
    let mut world = world(pe_rs::SceneType::Day);
    scoped(&mut world, || {
        let blocked = Grid { row: 0, col: 0 };
        let free = Grid { row: 0, col: 1 };
        let pot = Grid { row: 1, col: 0 };
        rsvz::with_backend(|backend| {
            backend.add_crater(blocked).unwrap();
        });
        let mut flowers = PlantFixer::new();
        let mut pots = PlantFixer::new();
        flowers
            .start_with(PlantKind::Sunflower, [blocked, free], 0.0, false)
            .unwrap();
        pots.start_with(PlantKind::FlowerPot, [pot], 0.0, false).unwrap();
        flowers.tick().unwrap();
        pots.tick().unwrap();
        rsvz::with_backend(|backend| {
            let actual = backend
                .plants()
                .unwrap()
                .map(|p| {
                    (
                        backend.plant_kind(p).unwrap(),
                        backend.plant_row(p),
                        backend.plant_col(p),
                    )
                })
                .collect::<Vec<_>>();
            assert_eq!(actual, [(PlantKind::Sunflower, 0, 1), (PlantKind::FlowerPot, 1, 0)]);
        });
    });
}

#[test]
fn plant_fixer_uses_imitator_after_normal_cooldown_and_resumes_after_both_cool_down() {
    use rsvz_backend_api::{PlantCreateBackend, PlantHealthWriteBackend};
    use rsvz_game::logic::plant_fixer::PlantFixer;
    let mut world = world_with_cards(
        pe_rs::SceneType::Day,
        &[
            CardSelection::Plant(PlantKind::Sunflower),
            CardSelection::Imitator(PlantKind::Sunflower),
        ],
    );
    scoped(&mut world, || {
        let grids = [
            Grid { row: 0, col: 0 },
            Grid { row: 1, col: 0 },
            Grid { row: 2, col: 0 },
        ];
        let ids = rsvz::with_backend(|backend| {
            backend.set_seed_recharge_ignored(false).unwrap();
            grids.map(|grid| {
                let plant = backend
                    .add_plant(CardSelection::Plant(PlantKind::Sunflower).checked().unwrap(), grid)
                    .unwrap();
                backend
                    .set_plant_hp(plant, rsvz_model::PositiveHp::new(100).unwrap())
                    .unwrap();
                backend.plant_id(plant)
            })
        });
        rsvz_game::modifier::set_sun(500).unwrap();
        let mut fixer = PlantFixer::new();
        fixer.start_with(PlantKind::Sunflower, [grids[0]], 200.0, true).unwrap();
        fixer.tick().unwrap();
        fixer.set_list([grids[1]]);
        fixer.tick().unwrap();
        rsvz::with_backend(|backend| {
            assert!(backend.plant(ids[0]).unwrap().is_none());
            assert!(backend.plant(ids[1]).unwrap().is_none());
        });
        fixer.set_list([grids[2]]);
        fixer.tick().unwrap();
        rsvz::with_backend(|backend| {
            assert!(backend.plant(ids[2]).unwrap().is_some());
            backend.set_seed_recharge_ignored(true).unwrap();
        });
        fixer.tick().unwrap();
        rsvz::with_backend(|backend| assert!(backend.plant(ids[2]).unwrap().is_none()));
    });
}

#[test]
fn planting_keeps_native_rejection_and_allows_native_pumpkin_repair() {
    use rsvz_backend_api::{PlantCreateBackend, PlantHealthWriteBackend, PlantPlacementBackend};
    use rsvz_game::logic::cards::{PlantSeedOutcome, can_plant_seed, find_seed_slot, try_plant_seed};
    let selection = CardSelection::Plant(PlantKind::Pumpkin);
    let mut world = world_with_cards(pe_rs::SceneType::Day, &[selection]);
    scoped(&mut world, || {
        let grid = Grid { row: 0, col: 0 };
        let id = rsvz::with_backend(|backend| {
            let plant = backend.add_plant(selection.checked().unwrap(), grid).unwrap();
            backend.plant_id(plant)
        });
        let slot = find_seed_slot(selection).unwrap();
        let native = rsvz::with_backend(|backend| backend.can_plant_at(selection.checked().unwrap(), grid).unwrap());
        assert!(matches!(native, rsvz_model::Plantability::Rejected(_)));
        assert_eq!(can_plant_seed(slot, grid), native);
        assert!(matches!(try_plant_seed(slot, grid), PlantSeedOutcome::Rejected(_)));
        rsvz::with_backend(|backend| {
            let plant = backend.plant(id).unwrap().unwrap();
            assert_eq!(backend.plant_hp(plant), 4000);
            backend
                .set_plant_hp(plant, rsvz_model::PositiveHp::new(1000).unwrap())
                .unwrap();
            assert_eq!(
                backend.can_plant_at(selection.checked().unwrap(), grid).unwrap(),
                rsvz_model::Plantability::Allowed
            );
        });
        assert!(matches!(try_plant_seed(slot, grid), PlantSeedOutcome::Planted(new_id) if new_id != id));
        rsvz::with_backend(|backend| assert!(backend.plant(id).unwrap().is_none()));
    });
}

#[test]
fn plant_fixer_failed_restart_preserves_the_existing_target_and_list() {
    use rsvz_game::logic::plant_fixer::{PlantFixer, PlantFixerError};
    let mut world = world(pe_rs::SceneType::Day);
    scoped(&mut world, || {
        let a = Grid { row: 0, col: 0 };
        let b = Grid { row: 1, col: 0 };
        let mut fixer = PlantFixer::new();
        fixer.start(PlantKind::Sunflower, [a]).unwrap();
        assert!(matches!(
            fixer.start(PlantKind::Imitator, [b]),
            Err(PlantFixerError::ImitatorTarget)
        ));
        assert_eq!(fixer.list(), [a]);
        fixer.tick().unwrap();
        assert_eq!(
            rsvz_current::with_backend_shared(|access| {
                let backend = access;
                backend
                    .plants()
                    .unwrap()
                    .map(|plant| {
                        (
                            Grid {
                                row: backend.plant_row(plant),
                                col: backend.plant_col(plant),
                            },
                            backend.plant_kind(plant).unwrap(),
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap(),
            [(a, PlantKind::Sunflower)]
        );
    });
}

#[test]
fn plant_fixer_large_unsigned_interval_does_not_wrap_into_every_frame() {
    use rsvz_game::logic::plant_fixer::PlantFixer;
    let mut world = world(pe_rs::SceneType::Day);
    let mut fixer = scoped(&mut world, || {
        let mut fixer = PlantFixer::new();
        fixer.start(PlantKind::Sunflower, [(1, 1)]).unwrap();
        fixer.set_run_interval(u32::MAX).unwrap();
        fixer
    });
    while world.with_backend(|backend| backend.clock().unwrap()) < 1 {
        world.update_world().unwrap();
    }
    scoped(&mut world, || {
        fixer.tick().unwrap();
        assert_eq!(
            rsvz_current::with_backend_shared(|access| {
                let backend = access;
                backend.plants().unwrap().count()
            })
            .unwrap(),
            0
        );
        fixer.set_run_interval(1).unwrap();
        fixer.tick().unwrap();
        assert_eq!(
            rsvz_current::with_backend_shared(|access| {
                let backend = access;
                backend.plants().unwrap().count()
            })
            .unwrap(),
            1
        );
    });
}

#[test]
fn plant_fixer_rebuilds_upgrade_through_its_base_and_respects_covered_bottom() {
    use rsvz_backend_api::{PlantCreateBackend, PlantHealthWriteBackend, PlantRemoveBackend};
    use rsvz_game::logic::plant_fixer::PlantFixer;
    {
        let mut world = world_with_cards(
            pe_rs::SceneType::Night,
            &[
                CardSelection::Plant(PlantKind::FumeShroom),
                CardSelection::Plant(PlantKind::GloomShroom),
            ],
        );
        scoped(&mut world, || {
            rsvz_game::modifier::set_sun(10000).unwrap();
            let grid = Grid { row: 0, col: 0 };
            let original = rsvz::with_backend(|backend| {
                let plant = backend
                    .add_plant(CardSelection::Plant(PlantKind::GloomShroom).checked().unwrap(), grid)
                    .unwrap();
                backend
                    .set_plant_hp(plant, rsvz_model::PositiveHp::new(100).unwrap())
                    .unwrap();
                backend.plant_id(plant)
            });
            let mut fixer = PlantFixer::new();
            fixer.start_with(PlantKind::GloomShroom, [grid], 200.0, false).unwrap();
            fixer.tick().unwrap();
            assert!(
                rsvz_current::with_backend_shared(|access| {
                    let backend = access;
                    backend.plant(original).unwrap().is_some()
                })
                .unwrap()
            );
            // Native upgrade packets require a live base plant before they can be picked up.
            rsvz::with_backend(|backend| {
                backend
                    .add_plant(
                        CardSelection::Plant(PlantKind::FumeShroom).checked().unwrap(),
                        Grid { row: 1, col: 0 },
                    )
                    .unwrap();
            });
            fixer.tick().unwrap();
            let kinds = || {
                rsvz_current::with_backend_shared(|access| {
                    let backend = access;

                    backend
                        .plants()
                        .unwrap()
                        .filter(|plant| backend.plant_row(*plant) == grid.row && backend.plant_col(*plant) == grid.col)
                        .map(|plant| backend.plant_kind(plant).unwrap())
                        .collect::<Vec<_>>()
                })
                .unwrap()
            };
            assert_eq!(kinds(), [PlantKind::FumeShroom]);
            fixer.tick().unwrap();
            assert_eq!(kinds(), [PlantKind::GloomShroom]);
            assert!(
                rsvz_current::with_backend_shared(|access| {
                    let backend = access;
                    backend.plant(original).unwrap().is_none()
                })
                .unwrap()
            );
        });
    }
    {
        let mut world = world_with_cards(pe_rs::SceneType::Pool, &[CardSelection::Plant(PlantKind::LilyPad)]);
        scoped(&mut world, || {
            rsvz_game::modifier::set_sun(10000).unwrap();
            let grid = Grid { row: 2, col: 0 };
            let original = rsvz::with_backend(|backend| {
                let lily = backend
                    .add_plant(CardSelection::Plant(PlantKind::LilyPad).checked().unwrap(), grid)
                    .unwrap();
                backend
                    .set_plant_hp(lily, rsvz_model::PositiveHp::new(100).unwrap())
                    .unwrap();
                let id = backend.plant_id(lily);
                backend
                    .add_plant(CardSelection::Plant(PlantKind::Sunflower).checked().unwrap(), grid)
                    .unwrap();
                id
            });
            let mut fixer = PlantFixer::new();
            fixer.start_with(PlantKind::LilyPad, [grid], 200.0, false).unwrap();
            fixer.tick().unwrap();
            assert!(
                rsvz_current::with_backend_shared(|access| {
                    let backend = access;
                    backend.plant(original).unwrap().is_some()
                })
                .unwrap()
            );
            fixer.set_skip_covered_bottom(false);
            fixer.tick().unwrap();
            assert!(
                rsvz_current::with_backend_shared(|access| {
                    let backend = access;
                    backend.plant(original).unwrap().is_none()
                })
                .unwrap()
            );
            assert_eq!(
                rsvz_current::with_backend_shared(|access| {
                    let backend = access;
                    backend
                        .plants()
                        .unwrap()
                        .filter(|plant| backend.plant_kind(*plant).unwrap() == PlantKind::LilyPad)
                        .count()
                })
                .unwrap(),
                0
            );
            rsvz_current::with_backend_shared(|access| {
                let backend = access;

                for plant in backend.plants().unwrap() {
                    if backend.plant_kind(plant).unwrap() == PlantKind::Sunflower {
                        backend.remove_plant(plant).unwrap();
                    }
                }
            })
            .unwrap();
            fixer.tick().unwrap();
            assert_eq!(
                rsvz_current::with_backend_shared(|access| {
                    let backend = access;
                    backend
                        .plants()
                        .unwrap()
                        .filter(|plant| backend.plant_kind(*plant).unwrap() == PlantKind::LilyPad)
                        .count()
                })
                .unwrap(),
                1
            );
        });
    }
}

#[test]
fn plant_fixer_observes_the_cannon_recovery_window_before_rebuilding_both_cells() {
    use rsvz_backend_api::{
        PlantCreateBackend, PlantHealthWriteBackend, PlantStateCountdownWriteBackend, PlantStateWriteBackend,
    };
    use rsvz_game::logic::plant_fixer::PlantFixer;
    let mut world = world_with_cards(
        pe_rs::SceneType::Day,
        &[
            CardSelection::Plant(PlantKind::KernelPult),
            CardSelection::Plant(PlantKind::CobCannon),
        ],
    );
    scoped(&mut world, || {
        rsvz_game::modifier::set_sun(10000).unwrap();
        let target = Grid { row: 0, col: 0 };
        let original = rsvz::with_backend(|backend| {
            for col in 0..2 {
                backend
                    .add_plant(
                        CardSelection::Plant(PlantKind::KernelPult).checked().unwrap(),
                        Grid { row: 1, col },
                    )
                    .unwrap();
            }
            let cob = backend
                .add_plant(CardSelection::Plant(PlantKind::CobCannon).checked().unwrap(), target)
                .unwrap();
            backend
                .set_plant_hp(cob, rsvz_model::PositiveHp::new(100).unwrap())
                .unwrap();
            backend.set_plant_state(cob, 37).unwrap();
            backend.plant_id(cob)
        });
        let mut fixer = PlantFixer::new();
        fixer.start_with(PlantKind::CobCannon, [target], 200.0, false).unwrap();
        fixer.tick().unwrap();
        assert!(
            rsvz_current::with_backend_shared(|access| {
                let backend = access;
                backend.plant(original).unwrap().is_some()
            })
            .unwrap()
        );
        rsvz_current::with_backend_shared(|access| {
            let backend = access;

            let cob = backend.plant(original).unwrap().unwrap();
            backend.set_plant_state(cob, 35).unwrap();
            backend.set_plant_state_countdown(cob, 1875).unwrap();
        })
        .unwrap();
        fixer.tick().unwrap();
        let row_kinds = || {
            rsvz_current::with_backend_shared(|access| {
                let backend = access;

                backend
                    .plants()
                    .unwrap()
                    .filter(|plant| backend.plant_row(*plant) == 0)
                    .map(|plant| backend.plant_kind(plant).unwrap())
                    .collect::<Vec<_>>()
            })
            .unwrap()
        };
        assert_eq!(row_kinds(), [PlantKind::KernelPult; 2]);
        fixer.tick().unwrap();
        assert_eq!(row_kinds(), [PlantKind::CobCannon]);
    });
}

#[test]
fn inactive_fixer_and_invalid_configuration_do_not_require_backend_access() {
    use rsvz_game::logic::plant_fixer::{PlantFixer, PlantFixerError};
    let mut fixer = PlantFixer::new();
    fixer.tick().unwrap();
    assert!(matches!(
        fixer.start(PlantKind::Imitator, [(1, 1)]),
        Err(PlantFixerError::ImitatorTarget)
    ));
    assert!(matches!(
        fixer.start_with(PlantKind::Sunflower, [(1, 1)], f32::NAN, false),
        Err(PlantFixerError::InvalidThreshold(_))
    ));
    assert!(matches!(
        fixer.start(PlantKind::Sunflower, [(0, 1)]),
        Err(PlantFixerError::InvalidGrid { .. })
    ));
}
