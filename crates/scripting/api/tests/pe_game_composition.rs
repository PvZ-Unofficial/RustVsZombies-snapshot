#![cfg(feature = "pvz-emulator")]

//! PE atomic capabilities composed through shared game rules.
use rsvz_backend_api::backend::{
    BoardStateBackend, PlantContactBackend, PlantCreateBackend, PlantReadBackend, ZombieContactBackend,
    ZombieCreateBackend, ZombieRawFactsBackend, ZombieReadBackend,
};
use rsvz_game::logic::{
    ZombieSpawnRequest, ZombieTypeSelection, apply_zombie_spawn_request, zombie_motion_state, zombie_state,
};
use rsvz_game::modifier::spawn_zombie;
use rsvz_model::{CardSelection, DamageRangeFlags, Grid, PlantKind, ZombieKind, ZombieMovementModel, ZombieSpawnMode};
use rsvz_pvz_emulator_backend::{PeWorldConfig, runner_internal::PeWorldOwner};

#[test]
fn raw_atoms_compose_through_core() {
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default())
        .expect("PE world")
        .install_current()
        .expect("current world");
    owner.with_backend(|backend| {
        let plant = backend
            .add_plant(
                CardSelection::Plant(PlantKind::Peashooter)
                    .checked()
                    .expect("selection"),
                Grid { row: 0, col: 4 },
            )
            .expect("plant");
        let zombie = backend
            .add_zombie_in_row(ZombieKind::Normal, 0, 0)
            .expect("zombie atom")
            .expect("zombie");
        assert!(backend.plant_hit_box(plant).expect("plant rect").is_valid());
        backend.zombie_can_attack_plant(zombie, plant, 0).expect("contact atom");
        backend
            .zombie_effected_by_damage(zombie, DamageRangeFlags::GROUND)
            .expect("damage atom");
        assert!(backend.zombie_reanim_anim_time(zombie).expect("reanimation").is_some());

        let plant_id = backend.plant_id(plant);
        let zombie_id = backend.zombie_id(zombie);
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            use rsvz_game::logic::contact;
            assert!(!matches!(
                zombie_motion_state(zombie_id).expect("live").model,
                ZombieMovementModel::Unsupported(_)
            ));

            assert!(contact::zombie_defense_bounds(zombie_id).expect("defense").is_some());
            assert!(contact::zombie_attack_bounds(zombie_id).expect("attack").is_some());
            assert_eq!(
                zombie_state(zombie_id).expect("state").expect("live").kind,
                ZombieKind::Normal
            );
            assert!(contact::plant_contact_rect(plant_id).expect("plant contact").is_some());
        });
    });
}

#[test]
fn placed_zombie_returns_its_id_directly() {
    let mut owner = PeWorldOwner::new_reset(PeWorldConfig::default())
        .expect("PE world")
        .install_current()
        .expect("current world");
    owner.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let id = spawn_zombie(ZombieKind::Normal, Grid { row: 0, col: 7 }).expect("place zombie");
            let zombie = rsvz::Zombie::from_id(id);
            assert_eq!(zombie.kind(), ZombieKind::Normal);
            assert_eq!(zombie.row(), 0);
        });
    });
}
#[test]
fn exact_spawn_writes_native_empty_tail() {
    let mut owner = PeWorldOwner::new_reset_deferred_spawn(PeWorldConfig::default())
        .expect("deferred PE reset should succeed")
        .install_current()
        .expect("current world");
    owner.with_backend(|backend| {
        let request = ZombieSpawnRequest::new(
            ZombieTypeSelection::Exact(vec![
                ZombieKind::JackInTheBox,
                ZombieKind::Ladder,
                ZombieKind::Football,
                ZombieKind::Catapult,
            ]),
            ZombieSpawnMode::Exact,
        )
        .expect("exact request");
        rsvz_pvz_emulator_backend::scope_backend(backend, || apply_zombie_spawn_request(&request, 0))
            .expect("apply exact request");

        for (slot, kind) in [
            ZombieKind::JackInTheBox,
            ZombieKind::Ladder,
            ZombieKind::Football,
            ZombieKind::Catapult,
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(backend.spawn_entry(0, slot as u32).expect("spawn entry"), kind.code());
        }
        for slot in 4..50 {
            assert_eq!(backend.spawn_entry(0, slot).expect("spawn entry"), -1, "slot {slot}");
        }
    });
}

#[test]
fn lineup_validates_before_clear_and_applies_layers_and_ready_cannon() {
    use rsvz_backend_api::SceneBackend;
    use rsvz_game::lineup::*;
    use rsvz_model::SceneKind;
    let mut world = PeWorldOwner::new_reset(PeWorldConfig {
        scene: rsvz_pvz_emulator_backend::pe_rs::SceneType::Day,
        ..PeWorldConfig::default()
    })
    .unwrap()
    .install_current()
    .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let old = rsvz_game::logic::cards::new_plant(PlantKind::Sunflower, Grid { row: 0, col: 7 }).unwrap();
            let mut cells = vec![LineupCell::empty(); SceneKind::Pool.row_count() * 9];
            cells[0].main = Some(LineupPlant::plain(CardSelection::Plant(PlantKind::CobCannon)));
            cells[18].base = LineupBase::LilyPad { imitator: false };
            cells[18].main = Some(LineupPlant::plain(CardSelection::Plant(PlantKind::Peashooter)));
            cells[18].pumpkin = Some(LineupPlant::plain(CardSelection::Plant(PlantKind::Pumpkin)));
            let lineup = Lineup::new(SceneKind::Pool, cells, None).unwrap();
            assert!(matches!(
                apply_lineup(
                    &lineup,
                    LineupApplyOptions {
                        allow_scene_switch: false
                    }
                ),
                Err(ApplyLineupError::SceneMismatch { .. })
            ));
            assert!(rsvz::Plant::from_id(old).is_alive());
            let rake = Lineup::new(
                SceneKind::Day,
                vec![LineupCell::empty(); 45],
                Some(Grid { row: 0, col: 7 }),
            )
            .unwrap();
            assert!(matches!(
                apply_lineup(&rake, LineupApplyOptions::default()),
                Err(ApplyLineupError::UnsupportedGridItem(_))
            ));
            assert!(rsvz::Plant::from_id(old).is_alive());
            apply_lineup(&lineup, LineupApplyOptions::default()).unwrap();
            rsvz_current::with_backend_shared(|access| {
                let native = access;
                assert_eq!(native.scene().unwrap(), SceneKind::Pool);
                assert!(native.plant(old).unwrap().is_none());
                let kinds: Vec<_> = native
                    .plants()
                    .unwrap()
                    .map(|p| native.plant_kind(p).unwrap())
                    .collect();
                assert_eq!(
                    kinds,
                    [
                        PlantKind::LilyPad,
                        PlantKind::CobCannon,
                        PlantKind::Peashooter,
                        PlantKind::Pumpkin
                    ]
                );
                let cannon = native
                    .plants()
                    .unwrap()
                    .find(|p| native.plant_kind(*p).unwrap() == PlantKind::CobCannon)
                    .unwrap();
                assert_eq!(native.plant_state(cannon), 37);
                assert_eq!(native.plant_state_countdown(cannon), 0);
                assert_eq!(native.plant_effect_countdown(cannon), 0);
                assert_eq!(rsvz_game::modifier::clear_plants().unwrap(), 4);
                assert_eq!(rsvz_game::modifier::clear_plants().unwrap(), 0);
            })
            .unwrap();
            let mut cells = vec![LineupCell::empty(); 45];
            cells[0].main = Some(LineupPlant {
                selection: CardSelection::Plant(PlantKind::PuffShroom),
                awake: Some(false),
                grown: false,
            });
            apply_lineup(
                &Lineup::new(SceneKind::Night, cells, None).unwrap(),
                LineupApplyOptions::default(),
            )
            .unwrap();
            rsvz_current::with_backend_shared(|access| {
                let native = access;
                let plant = native.plants().unwrap().next().unwrap();
                assert!(!native.plant_is_sleeping(plant));
            })
            .unwrap();
        })
    });
}

#[test]
fn opening_keeps_initial_lineup_history_and_ordered_card_prefix() {
    use rsvz_backend_api::CardAppendSelectionBackend;
    use rsvz_game::{lineup::*, setup::*};
    use rsvz_model::SceneKind;
    let mut world = PeWorldOwner::new_reset_deferred_spawn(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let desired = vec![
                CardSelection::Plant(PlantKind::Peashooter),
                CardSelection::Imitator(PlantKind::Pumpkin),
            ];
            let mut cells = vec![LineupCell::empty(); 54];
            cells[0].main = Some(LineupPlant::plain(CardSelection::Plant(PlantKind::Sunflower)));
            let setup = ScriptSetup {
                lineup: Some(PendingLineup::new(
                    Lineup::new(SceneKind::Pool, cells, None).unwrap(),
                    LineupApplyOptions::default(),
                    LineupReloadPolicy::InitialOnly,
                    "test",
                )),
                desired_cards: Some(desired.clone()),
                ..ScriptSetup::default()
            };
            let mut state = OpeningState::default();
            prepare_script_opening(&setup, 7, &mut state).unwrap();
            let first = rsvz_current::with_backend_shared(|a| a.plant_id(a.plants().unwrap().next().unwrap())).unwrap();
            prepare_script_opening(&setup, 7, &mut state).unwrap();
            assert!(rsvz::Plant::from_id(first).is_alive());
            state.reset_world();
            prepare_script_opening(&setup, 7, &mut state).unwrap();
            assert!(!rsvz::Plant::from_id(first).is_alive());
            rsvz_current::with_backend_shared(|b| b.select_card(desired[0].checked().unwrap()))
                .unwrap()
                .unwrap();
            assert!(matches!(
                verify_selected_cards(&desired),
                Err(VerifySelectedCardsError::TooFew)
            ));
            finish_script_opening(&setup).unwrap();
            verify_selected_cards(&desired).unwrap();
            finish_script_opening(&setup).unwrap();
            let mismatch = ScriptSetup {
                desired_cards: Some(vec![CardSelection::Plant(PlantKind::Sunflower)]),
                ..ScriptSetup::default()
            };
            assert!(matches!(
                finish_script_opening(&mismatch),
                Err(ApplyScriptOpeningError::CardMismatch { index: 0, .. })
            ));
            rsvz_current::with_backend_shared(|b| {
                b.select_card(CardSelection::Plant(PlantKind::Sunflower).checked().unwrap())
            })
            .unwrap()
            .unwrap();
            finish_script_opening(&setup).unwrap();
            finish_script_opening(&ScriptSetup::default()).unwrap();
            assert!(matches!(
                verify_selected_cards(&desired),
                Err(VerifySelectedCardsError::TooMany)
            ));
        })
    });
}

#[test]
fn current_spawn_configuration_validates_before_writes_and_resets_flags() {
    use rsvz_game::logic::{ApplySpawnListError, apply_spawn_list};
    use rsvz_model::{DEFAULT_SPAWN_WAVES, SpawnList};
    let mut world = PeWorldOwner::new_reset_deferred_spawn(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let requested = [ZombieKind::JackInTheBox, ZombieKind::JackInTheBox, ZombieKind::Ladder];
            let exact = ZombieSpawnRequest::new(ZombieTypeSelection::Exact(requested.to_vec()), ZombieSpawnMode::Exact)
                .unwrap();
            apply_zombie_spawn_request(&exact, 7).unwrap();
            assert!(matches!(
                apply_spawn_list(&SpawnList::new()),
                Err(ApplySpawnListError::Empty)
            ));
            let mut incomplete = SpawnList::new();
            incomplete.set_wave(0, [ZombieKind::Normal]);
            assert!(matches!(
                apply_spawn_list(&incomplete),
                Err(ApplySpawnListError::MissingWaves { .. })
            ));
            rsvz_current::with_backend_shared(|a| {
                let native = a;
                for wave in 0..DEFAULT_SPAWN_WAVES {
                    for slot in 0..50 {
                        assert_eq!(
                            native.spawn_entry(wave as u32, slot).unwrap(),
                            requested.get(slot as usize).map_or(-1, |k| k.code())
                        );
                    }
                }
                for kind in 0..33 {
                    assert_eq!(
                        native.spawn_allowed(kind).unwrap(),
                        kind == ZombieKind::JackInTheBox.code() as u32 || kind == ZombieKind::Ladder.code() as u32
                    );
                }
            })
            .unwrap();
            let natural = ZombieSpawnRequest::new(
                ZombieTypeSelection::Exact(vec![ZombieKind::Gargantuar]),
                ZombieSpawnMode::Natural,
            )
            .unwrap();
            apply_zombie_spawn_request(&natural, 7).unwrap();
            rsvz_current::with_backend_shared(|a| {
                let native = a;
                for kind in 0..33 {
                    assert_eq!(
                        native.spawn_allowed(kind).unwrap(),
                        kind == ZombieKind::Normal.code() as u32 || kind == ZombieKind::Gargantuar.code() as u32
                    );
                }
                assert!((0..50).all(|slot| native.spawn_entry(0, slot).unwrap() >= 0));
            })
            .unwrap();
            let mut explicit = SpawnList::new();
            for wave in 0..DEFAULT_SPAWN_WAVES {
                explicit.set_wave(wave, [ZombieKind::Normal, ZombieKind::Buckethead]);
            }
            apply_spawn_list(&explicit).unwrap();
            rsvz_current::with_backend_shared(|a| {
                let native = a;
                for kind in 0..33 {
                    assert_eq!(
                        native.spawn_allowed(kind).unwrap(),
                        kind == ZombieKind::Normal.code() as u32 || kind == ZombieKind::Buckethead.code() as u32
                    );
                }
            })
            .unwrap();
        })
    });
}

#[test]
fn refresh_capture_reads_current_health_only_when_due_and_records_once() {
    use rsvz_backend_api::WaveHealthBackend;
    use rsvz_game::measure::{MeasurementState, capture_refresh_after_update};
    use rsvz_model::{Wave, WaveClockState, WaveTimingSnapshot, WavelengthDeclaration};
    let mut state = MeasurementState::new_refresh();
    state.start_trial();
    let mut clocks = WaveClockState::new();
    clocks
        .declare_wavelength_unchecked_bounds(WavelengthDeclaration::assumed(Wave(1), 1000))
        .unwrap();
    clocks.record_refresh_clock(Wave(1), 0);
    // A not-due sample has no backend or world installed at all.
    assert!(
        capture_refresh_after_update(&mut state, WaveTimingSnapshot::minimal(799, Wave(1)), &clocks)
            .unwrap()
            .is_none()
    );
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            rsvz_current::with_backend_shared(|access| {
                let native = access;
                let sample =
                    capture_refresh_after_update(&mut state, WaveTimingSnapshot::minimal(800, Wave(1)), &clocks)
                        .unwrap()
                        .unwrap();
                assert_eq!(sample.initial_hp, native.zombie_health_wave_start().unwrap() as u32);
                assert_eq!(
                    sample.current_hp,
                    native.total_zombies_health_in_wave(0).unwrap() as u32
                );
                state.record_refresh_sample(sample, false).unwrap();
            })
            .unwrap();
        })
    });
    assert!(
        capture_refresh_after_update(&mut state, WaveTimingSnapshot::minimal(801, Wave(1)), &clocks)
            .unwrap()
            .is_none()
    );
}

#[test]
fn refresh_observer_returns_access_failure_to_its_session_failure_handler() {
    use rsvz_game::measure::{ObserveRefreshError, RefreshTask};
    use rsvz_model::{MeasureLimit, RefreshMeasureConfig, SessionShard, WaveClockState};
    let mut task = RefreshTask::new(
        MeasureLimit::trials(1).unwrap(),
        RefreshMeasureConfig::default(),
        63,
        SessionShard {
            index: 0,
            count: 1,
            seed_base: 0,
        },
        0,
    );
    let clocks = WaveClockState::new();
    task.observe_after_update(&clocks).unwrap();
    task.tick(0, 0, None).unwrap();
    assert!(matches!(
        task.observe_after_update(&clocks),
        Err(ObserveRefreshError::Backend(_))
    ));
}
