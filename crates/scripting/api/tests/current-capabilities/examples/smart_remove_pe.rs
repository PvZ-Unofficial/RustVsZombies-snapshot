use rsvz::core::backend::{
    PlantCreateBackend, PlantEffectCountdownWriteBackend, PlantReadBackend, PlantVisualStateBackend,
    ZombieCreateBackend, ZombieRawFactsBackend, ZombieReadBackend, ZombieXWriteBackend,
};
use rsvz::core::model::{
    CardSelection, GameUi, Grid, I32RepresentableF32, NonNegativeI32, PlantKind, SmartRemoveOptions, Wave,
    WaveTimingSnapshot, ZombieKind,
};
use rsvz::core::tick::{TickMeta, TickPhase};
use rsvz_pvz_emulator_backend::{PeWorldConfig, runner_internal::PeWorldOwner};

fn main() {
    for (highlight, explicit_plain) in [(false, false), (true, false), (true, true)] {
        rsvz::reset_runtime_state_preserving_backend();
        rsvz::smart_remove::reset();
        let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
            .unwrap()
            .install_current()
            .unwrap();
        world.with_backend(|backend| {
            rsvz_pvz_emulator_backend::scope_backend(backend, || {
                let pumpkin = rsvz::with_backend(|backend| {
                    let grid = Grid { row: 0, col: 0 };
                    backend
                        .add_plant(CardSelection::Plant(PlantKind::FlowerPot).checked().unwrap(), grid)
                        .unwrap();
                    let plant = backend
                        .add_plant(CardSelection::Plant(PlantKind::Pumpkin).checked().unwrap(), grid)
                        .unwrap();
                    let id = backend.plant_id(plant);
                    let zombie = backend.add_zombie_in_row(ZombieKind::Zomboni, 0, 0).unwrap().unwrap();
                    backend
                        .set_zombie_x(zombie, I32RepresentableF32::new(90.0).unwrap())
                        .unwrap();
                    id
                });
                rsvz::smart_remove::start_with_options(SmartRemoveOptions { highlight }).unwrap();
                if highlight {
                    rsvz_current::with_backend_shared(|access| {
                        let backend = access;
                        let plant = backend.plant(pumpkin).unwrap().unwrap();
                        backend.set_plant_eating_flash_counter(plant, 59).unwrap();
                        if explicit_plain {
                            let mut state = rsvz::core::logic::smart_remove::SmartRemoveState::default();
                            state.set_highlight(highlight);
                            rsvz::core::logic::smart_remove::tick_smart_remove(&mut state).unwrap();
                        } else {
                            rsvz::smart_remove::tick().unwrap();
                        }
                        assert!(!backend.plant_is_alive(plant));
                        assert_eq!(backend.plant_eating_flash_counter(plant), 0);
                    })
                    .unwrap();
                }

                let result = rsvz::dispatch_runtime_tick(
                    WaveTimingSnapshot::minimal(0, Wave(1)),
                    TickMeta {
                        clock: Some(0),
                        phase: TickPhase::Playing,
                        game_ui: Some(GameUi::Playing),
                        is_new_frame: true,
                    },
                );
                assert!(
                    matches!(result, rsvz::runtime_frame::RuntimeFrameDispatch::Continue),
                    "{result:?}"
                );
                let alive = rsvz_current::with_backend_shared(|access| {
                    let backend = access;
                    backend
                        .plant(pumpkin)
                        .unwrap()
                        .is_some_and(|plant| backend.plant_is_alive(plant))
                })
                .unwrap();
                assert!(
                    !alive,
                    "scheduled SmartRemove must remove the pumpkin before the crusher"
                );
            })
        });
    }
    disappearing_right_plant_keeps_its_container();
    earlier_removal_changes_the_later_right_target();
    println!("SmartRemove PE scheduled, explicit and right-container fallback passed");
}

fn disappearing_right_plant_keeps_its_container() {
    for (with_container, giant_count, ice_next, expected_alive) in [
        (false, 1, false, false),
        (true, 1, false, true),
        (true, 2, false, false),
        (false, 2, true, true),
    ] {
        rsvz::reset_runtime_state_preserving_backend();
        rsvz::smart_remove::reset();
        let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
            .unwrap()
            .install_current()
            .unwrap();
        let (pumpkin, cherry, giant, ice) = world.with_backend(|backend| {
            let right = Grid { row: 0, col: 1 };
            // Allocate the right container before the pumpkin: it is a valid lower-slot target.
            if with_container {
                backend
                    .add_plant(CardSelection::Plant(PlantKind::FlowerPot).checked().unwrap(), right)
                    .unwrap();
            }
            let cherry = backend
                .add_plant(CardSelection::Plant(PlantKind::CherryBomb).checked().unwrap(), right)
                .unwrap();
            backend
                .set_plant_effect_countdown(cherry, NonNegativeI32::new(10_000).unwrap())
                .unwrap();
            let cherry = backend.plant_id(cherry);
            let left = Grid { row: 0, col: 0 };
            backend
                .add_plant(CardSelection::Plant(PlantKind::FlowerPot).checked().unwrap(), left)
                .unwrap();
            let pumpkin = backend
                .add_plant(CardSelection::Plant(PlantKind::Pumpkin).checked().unwrap(), left)
                .unwrap();
            let pumpkin = backend.plant_id(pumpkin);
            let giant = backend
                .add_zombie_in_row(ZombieKind::Gargantuar, 0, 0)
                .unwrap()
                .unwrap();
            backend
                .set_zombie_x(giant, I32RepresentableF32::new(140.0).unwrap())
                .unwrap();
            for _ in 1..giant_count {
                let other = backend
                    .add_zombie_in_row(ZombieKind::Gargantuar, 0, 0)
                    .unwrap()
                    .unwrap();
                backend
                    .set_zombie_x(other, I32RepresentableF32::new(140.0).unwrap())
                    .unwrap();
            }
            let ice = if ice_next {
                let ice = backend
                    .add_plant(
                        CardSelection::Plant(PlantKind::IceShroom).checked().unwrap(),
                        Grid { row: 5, col: 8 },
                    )
                    .unwrap();
                backend
                    .set_plant_effect_countdown(ice, NonNegativeI32::new(10_000).unwrap())
                    .unwrap();
                Some(backend.plant_id(ice))
            } else {
                None
            };
            (pumpkin, cherry, backend.zombie_id(giant), ice)
        });
        let mut checked = false;
        for _ in 0..300 {
            world.update_world().unwrap();
            checked = world.with_backend(|backend| {
                let giant = backend.zombie(giant).unwrap().unwrap();
                let progress = backend.zombie_reanim_anim_time(giant).unwrap().unwrap();
                let last = backend.zombie_reanim_last_time(giant).unwrap().unwrap();
                if !(0.643 < progress && progress < 0.645 && 0.639 < last && last < 0.641) {
                    return false;
                }
                backend
                    .set_plant_effect_countdown(
                        backend.plant(cherry).unwrap().unwrap(),
                        NonNegativeI32::new(1).unwrap(),
                    )
                    .unwrap();
                if let Some(ice) = ice {
                    backend
                        .set_plant_effect_countdown(
                            backend.plant(ice).unwrap().unwrap(),
                            NonNegativeI32::new(1).unwrap(),
                        )
                        .unwrap();
                }
                rsvz_pvz_emulator_backend::scope_backend(backend, || {
                    let mut state = rsvz::core::logic::smart_remove::SmartRemoveState::default();
                    rsvz::core::logic::smart_remove::tick_smart_remove(&mut state).unwrap();
                });
                assert_eq!(
                    backend.plant(pumpkin).unwrap().is_some(),
                    expected_alive,
                    "right-container, simultaneous hammers and next-frame ice must keep their original decisions"
                );
                true
            });
            if checked {
                break;
            }
        }
        assert!(checked, "giant must reach the SmartRemove hammer window");
    }
}

// The right pumpkin has a lower pool slot than the left pumpkin, but its pot has a higher slot.
// Removing the right pumpkin first must expose that pot to the later left-cell decision.
fn earlier_removal_changes_the_later_right_target() {
    rsvz::reset_runtime_state_preserving_backend();
    rsvz::smart_remove::reset();
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    let (left_id, right_id, giant_id) = world.with_backend(|backend| {
        let left = Grid { row: 0, col: 0 };
        let right = Grid { row: 0, col: 1 };
        let right_id = backend.plant_id(
            backend
                .add_plant(CardSelection::Plant(PlantKind::Pumpkin).checked().unwrap(), right)
                .unwrap(),
        );
        backend
            .add_plant(CardSelection::Plant(PlantKind::FlowerPot).checked().unwrap(), left)
            .unwrap();
        let left_id = backend.plant_id(
            backend
                .add_plant(CardSelection::Plant(PlantKind::Pumpkin).checked().unwrap(), left)
                .unwrap(),
        );
        backend
            .add_plant(CardSelection::Plant(PlantKind::FlowerPot).checked().unwrap(), right)
            .unwrap();
        let giant = backend
            .add_zombie_in_row(ZombieKind::Gargantuar, 0, 0)
            .unwrap()
            .unwrap();
        backend
            .set_zombie_x(giant, I32RepresentableF32::new(140.0).unwrap())
            .unwrap();
        (left_id, right_id, backend.zombie_id(giant))
    });
    for _ in 0..300 {
        world.update_world().unwrap();
        let checked = world.with_backend(|backend| {
            let giant = backend.zombie(giant_id).unwrap().unwrap();
            let current = backend.zombie_reanim_anim_time(giant).unwrap().unwrap();
            let last = backend.zombie_reanim_last_time(giant).unwrap().unwrap();
            if !(0.643 < current && current < 0.645 && 0.639 < last && last < 0.641) {
                return false;
            }
            rsvz_pvz_emulator_backend::scope_backend(backend, || {
                let mut state = rsvz::core::logic::smart_remove::SmartRemoveState::default();
                rsvz::core::logic::smart_remove::tick_smart_remove(&mut state).unwrap();
            });
            assert!(backend.plant(right_id).unwrap().is_none());
            assert!(
                backend.plant(left_id).unwrap().is_none(),
                "later decisions must observe earlier pumpkin removals"
            );
            true
        });
        if checked {
            return;
        }
    }
    panic!("giant must reach the hammer window");
}
