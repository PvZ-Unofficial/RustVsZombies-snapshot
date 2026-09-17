#![cfg(feature = "pvz-emulator")]

use rsvz::cards::{CardErrorKind, card, try_card};
use rsvz_backend_api::{
    BattleEntryBackend, CardAppendSelectionBackend, PlantReadBackend, SeedRuleEditBackend, SunCostRuleEditBackend,
};
use rsvz_model::{CardSelection, Grid, PlantId, PlantKind, SeedSlot};
use rsvz_pvz_emulator_backend::{PeWorldConfig, runner_internal::PeWorldOwner};

fn grid(id: PlantId) -> Grid {
    rsvz::Plant::from_id(id).grid().expect_live("planted plant")
}

#[test]
fn current_card_adapters_preserve_slots_candidates_and_batch_results() {
    rsvz::reset_runtime_state_preserving_backend();
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            rsvz::with_backend(|backend| {
                for kind in [
                    PlantKind::Sunflower,
                    PlantKind::Peashooter,
                    PlantKind::LilyPad,
                    PlantKind::DoomShroom,
                ] {
                    backend
                        .select_card(CardSelection::Plant(kind).checked().unwrap())
                        .unwrap();
                }
                backend.start_battle().unwrap();
                backend.set_seed_recharge_ignored(true).unwrap();
            });
            rsvz::cards::try_set_sun_cost_ignored(true).unwrap();
            assert!(rsvz::with_backend(|backend| backend.sun_cost_ignored()).unwrap());
            rsvz::cards::set_sun_cost_ignored(false);
            assert!(!rsvz::with_backend(|backend| backend.sun_cost_ignored()).unwrap());
            rsvz::cards::try_set_sun_cost_ignored(true).unwrap();
            assert_eq!(
                grid(try_card(PlantKind::Sunflower, 1, 1).unwrap()),
                Grid { row: 0, col: 0 }
            );
            assert_eq!(
                grid(card(CardSelection::Plant(PlantKind::Peashooter), 1, 2).unwrap()),
                Grid { row: 0, col: 1 }
            );
            assert_eq!(
                grid(try_card(SeedSlot::from_index_unchecked(0), 1, 3).unwrap()),
                Grid { row: 0, col: 2 }
            );

            let water = try_card([PlantKind::LilyPad, PlantKind::DoomShroom], 3, 4).unwrap();
            assert!(water.iter().all(Option::is_some));
            assert_eq!(
                grid(try_card(PlantKind::Sunflower, [(1, 1), (1, 4)]).unwrap()),
                Grid { row: 0, col: 3 }
            );
            let candidates = try_card([PlantKind::Sunflower, PlantKind::Peashooter], [(2, 1), (2, 2)]).unwrap();
            assert_eq!(grid(candidates[0].unwrap()), Grid { row: 1, col: 0 });
            assert_eq!(grid(candidates[1].unwrap()), Grid { row: 1, col: 1 });
            let operations = card([(PlantKind::Sunflower, 2, 3), (PlantKind::Peashooter, 2, 4)]);
            assert_eq!(operations.len(), 2);
            assert!(operations.iter().all(Option::is_some));

            assert!(matches!(
                try_card(PlantKind::Pumpkin, 2, 5).unwrap_err().kind(),
                CardErrorKind::NotSelected { .. }
            ));
            let partial = try_card([(PlantKind::Pumpkin, 2, 5), (PlantKind::Peashooter, 2, 6)]).unwrap();
            assert!(partial[0].is_none());
            assert_eq!(grid(partial[1].unwrap()), Grid { row: 1, col: 5 });
            rsvz::__run_script(|| {
                rsvz::at(1, 0, || try_card(PlantKind::Sunflower, 2, 7).map_err(Into::into));
                Ok(())
            })
            .unwrap();
        })
    });
    rsvz::reset_runtime_state_preserving_backend();
}

#[test]
fn canonical_resource_functions_share_access_and_apply_native_policy() {
    use rsvz_backend_api::{BoardStateBackend, DropRuleEditBackend, SunQueryBackend};
    use rsvz_game::modifier::{set_sun, stabilize_natural_sun_drop, stabilize_natural_sun_drop_aging};
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let set: fn(u32) -> rsvz::RuntimeResult<()> = set_sun;
            let sample: fn() -> rsvz::RuntimeResult<rsvz_game::runtime::RuntimeFrameFacts> =
                rsvz_game::runtime::runtime_frame_facts;
            // A legal token can perform the first Board query directly.
            rsvz_pvz_emulator_backend::with_backend_shared(|backend| backend.natural_sun_generated().unwrap()).unwrap();
            rsvz_pvz_emulator_backend::with_backend_shared(|access| {
                let backend = access;
                set(1234).unwrap();
                assert_eq!(backend.sun().unwrap(), 1234);
                stabilize_natural_sun_drop_aging().unwrap();
                assert_eq!(backend.natural_sun_generated().unwrap(), 52);
                stabilize_natural_sun_drop().unwrap();
                assert!(backend.natural_sun_drop_disabled().unwrap());
                assert_eq!(backend.natural_sun_countdown().unwrap(), 425);
                sample().unwrap();
            })
            .unwrap();
            // A live exclusive physical borrow still excludes ordinary operations.
            let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                rsvz::with_backend(|_| {
                    let _ = set(100);
                    panic!("a conflicting access must not return a recoverable result");
                });
            }))
            .unwrap_err();
            let error = rsvz::__private::into_callback_error(payload).expect("callback-local access failure");
            assert!(error.to_string().contains("active borrow"));
            set(100).unwrap();
        });
    });
}

fn with_selected_cards(
    scene: rsvz_pvz_emulator_backend::pe_rs::SceneType, selections: &[CardSelection], body: impl FnOnce(),
) {
    rsvz::reset_runtime_state_preserving_backend();
    let mut world = PeWorldOwner::new_reset(PeWorldConfig {
        scene,
        ..PeWorldConfig::default()
    })
    .unwrap()
    .install_current()
    .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            rsvz::with_backend(|backend| {
                for selection in selections {
                    backend.select_card(selection.checked().unwrap()).unwrap();
                }
                backend.start_battle().unwrap();
            });
            body();
        })
    });
    rsvz::reset_runtime_state_preserving_backend();
}

#[test]
fn auto_container_commit_is_kept_when_the_main_card_becomes_unaffordable() {
    use rsvz_pvz_emulator_backend::pe_rs::SceneType;
    with_selected_cards(
        SceneType::Roof,
        &[
            CardSelection::Plant(PlantKind::FlowerPot),
            CardSelection::Plant(PlantKind::Sunflower),
        ],
        || {
            rsvz_game::modifier::set_sun(50).unwrap();
            let first = Grid { row: 0, col: 0 };
            let second = Grid { row: 0, col: 1 };
            let error = rsvz_game::logic::cards::card_any_prepared(
                CardSelection::Plant(PlantKind::Sunflower),
                &[first, second],
            )
            .unwrap_err();
            assert!(matches!(error.kind(), CardErrorKind::NotEnoughSun { .. }), "{error:?}");
            let plants = rsvz_current::with_backend_shared(|access| {
                let backend = access;

                backend
                    .plants()
                    .unwrap()
                    .map(|plant| {
                        (
                            backend.plant_kind(plant).unwrap(),
                            Grid {
                                row: backend.plant_row(plant),
                                col: backend.plant_col(plant),
                            },
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap();
            assert_eq!(plants, [(PlantKind::FlowerPot, first)]);
            use rsvz_backend_api::SunQueryBackend;
            assert_eq!(
                rsvz_current::with_backend_shared(|access| {
                    let backend = access;
                    backend.sun().unwrap()
                })
                .unwrap(),
                25
            );
        },
    );
}

#[test]
fn recorder_runs_after_creation_and_before_seed_post_processing_even_when_the_callback_aborts() {
    use rsvz_game::logic::cards::PlantingComponent;
    use rsvz_model::{GameUi, Wave, WaveTimingSnapshot};
    use rsvz_pvz_emulator_backend::pe_rs::SceneType;
    use rsvz_schedule::{TickMeta, TickPhase, TimelineDispatchResult};
    use std::{cell::RefCell, rc::Rc};
    with_selected_cards(SceneType::Day, &[CardSelection::Plant(PlantKind::Sunflower)], || {
        rsvz_game::modifier::set_sun(100).unwrap();
        let recorded = Rc::new(RefCell::new(Vec::new()));
        let saved = Rc::clone(&recorded);
        rsvz::__run_script(|| {
            rsvz::at(1, 0, move || {
                rsvz_game::logic::cards::card_recording(
                    CardSelection::Plant(PlantKind::Sunflower),
                    1,
                    1,
                    |component, id| {
                        saved.borrow_mut().push((component, id));
                        rsvz_game::diagnostics::abort_operation(rsvz::RuntimeError::new("recorded plant"));
                    },
                )
                .map_err(rsvz::RuntimeError::from)
            });
            Ok(())
        })
        .unwrap();
        let mut errors = Vec::new();
        assert_eq!(
            rsvz_game::timeline::dispatch_timeline_tick_reporting(
                WaveTimingSnapshot::minimal(0, Wave(1)),
                TickMeta {
                    phase: TickPhase::Playing,
                    game_ui: Some(GameUi::Playing),
                    clock: Some(0),
                    is_new_frame: true
                },
                &mut |error| errors.push(error.to_string()),
            ),
            TimelineDispatchResult::Continue
        );
        assert_eq!(errors, ["recorded plant"]);
        let records = recorded.borrow();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].0, PlantingComponent::Main);
        assert_eq!(grid(records[0].1), Grid { row: 0, col: 0 });
        assert_eq!(
            rsvz_game::logic::cards::card_cd(CardSelection::Plant(PlantKind::Sunflower)),
            Some(0)
        );
    });
}

#[test]
fn gloom_upgrade_restores_the_base_sleep_state_and_pending_coffee_counter() {
    use rsvz_backend_api::PlantSleepBackend;
    use rsvz_pvz_emulator_backend::pe_rs::SceneType;
    for asleep in [false, true] {
        with_selected_cards(SceneType::Day, &[CardSelection::Plant(PlantKind::GloomShroom)], || {
            rsvz_game::modifier::set_sun(1000).unwrap();
            let at = Grid { row: 0, col: 0 };
            let base = rsvz_game::logic::cards::new_plant(PlantKind::FumeShroom, at).unwrap();
            rsvz_current::with_backend_shared(|access| {
                let backend = access;

                let plant = backend.plant(base).unwrap().unwrap();
                backend.set_plant_sleeping(plant, asleep).unwrap();
                if asleep {
                    backend.set_plant_wake_up_counter(plant, 37).unwrap();
                }
            })
            .unwrap();
            let upgraded = try_card(PlantKind::GloomShroom, 1, 1).unwrap();
            rsvz_current::with_backend_shared(|access| {
                let backend = access;

                assert!(backend.plant(base).unwrap().is_none());
                let plant = backend.plant(upgraded).unwrap().unwrap();
                assert_eq!(backend.plant_is_sleeping(plant), asleep);
                if asleep {
                    assert_eq!(backend.plant_wake_up_counter(plant), 37);
                }
            })
            .unwrap();
        });
    }
}

#[test]
fn shovel_preserves_exact_lineage_and_restores_lily_under_a_protected_cattail() {
    use rsvz_pvz_emulator_backend::pe_rs::SceneType;
    with_selected_cards(SceneType::Day, &[], || {
        let normal = rsvz_game::logic::cards::new_plant(PlantKind::IceShroom, Grid { row: 0, col: 0 }).unwrap();
        let imitator = rsvz_game::logic::cards::new_plant_from_selection(
            CardSelection::Imitator(PlantKind::IceShroom),
            Grid { row: 0, col: 1 },
        )
        .unwrap();
        rsvz::shovel::try_shovel(1, 1, CardSelection::Imitator(PlantKind::IceShroom)).unwrap();
        rsvz::shovel::try_shovel(1, 2, PlantKind::IceShroom).unwrap();
        assert!(
            rsvz_current::with_backend_shared(|access| {
                let backend = access;
                backend.plant(normal).unwrap().is_some() && backend.plant(imitator).unwrap().is_some()
            })
            .unwrap()
        );
        rsvz::shovel::try_shovel(1, 2, CardSelection::Imitator(PlantKind::IceShroom)).unwrap();
        assert!(
            rsvz_current::with_backend_shared(|access| {
                let backend = access;
                backend.plant(normal).unwrap().is_some() && backend.plant(imitator).unwrap().is_none()
            })
            .unwrap()
        );
    });
    with_selected_cards(
        SceneType::Pool,
        &[
            CardSelection::Plant(PlantKind::LilyPad),
            CardSelection::Plant(PlantKind::Cattail),
            CardSelection::Plant(PlantKind::Pumpkin),
        ],
        || {
            rsvz_game::modifier::set_sun(1000).unwrap();
            try_card(PlantKind::LilyPad, 3, 1).unwrap();
            let cattail = try_card(PlantKind::Cattail, 3, 1).unwrap();
            let pumpkin = try_card(PlantKind::Pumpkin, 3, 1).unwrap();
            rsvz::shovel::try_shovel(3, 1, PlantKind::Cattail).unwrap();
            rsvz_current::with_backend_shared(|access| {
                let backend = access;

                assert!(backend.plant(cattail).unwrap().is_none());
                assert!(backend.plant(pumpkin).unwrap().is_some());
                assert_eq!(
                    backend
                        .plants()
                        .unwrap()
                        .filter(|plant| backend.plant_kind(*plant).unwrap() == PlantKind::LilyPad)
                        .count(),
                    1
                );
            })
            .unwrap();
        },
    );
}

#[test]
fn shovel_skips_airborne_squashes_but_removes_a_grounded_squash() {
    use rsvz_backend_api::PlantStateWriteBackend;
    use rsvz_pvz_emulator_backend::pe_rs::SceneType;
    with_selected_cards(SceneType::Day, &[], || {
        for (col, state) in [5, 6, 7].into_iter().enumerate() {
            let at = Grid {
                row: 0,
                col: col as i32,
            };
            let id = rsvz_game::logic::cards::new_plant(PlantKind::Squash, at).unwrap();
            rsvz::__private::with_backend_shared(|access| {
                let backend = access;
                let plant = backend.plant(id).unwrap().unwrap();
                backend.set_plant_state(plant, state).unwrap();
                rsvz_game::logic::shovel::shovel_at(at).unwrap();
                rsvz::shovel::try_shovel(1, col as i32 + 1, PlantKind::Squash).unwrap();
                // The crushed state is excluded by live-ID lookup; restoring only the state
                // verifies that neither shovel path marked the held object dead.
                backend.set_plant_state(plant, 4).unwrap();
                assert!(backend.plant(id).unwrap().is_some());
                rsvz::shovel::try_shovel(1, col as i32 + 1, PlantKind::Squash).unwrap();
                assert!(backend.plant(id).unwrap().is_none());
            })
            .unwrap();
        }
    });
}

#[test]
fn cooldown_rejection_keeps_native_remaining_time_and_target_diagnostics() {
    use rsvz_pvz_emulator_backend::pe_rs::SceneType;
    with_selected_cards(SceneType::Day, &[CardSelection::Plant(PlantKind::Sunflower)], || {
        rsvz_game::modifier::set_sun(1000).unwrap();
        try_card(PlantKind::Sunflower, 1, 1).unwrap();
        let remaining = rsvz_game::logic::cards::card_cd(CardSelection::Plant(PlantKind::Sunflower)).unwrap();
        assert!(remaining > 0);
        let error = try_card(PlantKind::Sunflower, 1, 2).unwrap_err();
        assert!(matches!(error.kind(), CardErrorKind::Cooldown { remaining: value, .. } if value == remaining));
        assert!(error.message().contains("(1, 2)"));
        assert!(error.message().contains("向日葵"));
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
