#![cfg(feature = "pvz-emulator")]

use rsvz::is_safe_blover;
use rsvz_backend_api::*;
use rsvz_model::{CardSelection, Grid, NonNegativeI32, PlantKind, ZombieId, ZombieKind, ZombiePhase};
use rsvz_pvz_emulator_backend::{
    PeWorldConfig, pe_rs,
    runner_internal::{PeWorldOwner, PeWorldRun},
};

const GRID: Grid = Grid { row: 2, col: 2 };

fn scoped<T>(world: &mut PeWorldRun, f: impl FnOnce() -> T) -> T {
    world.with_backend(|b| rsvz_pvz_emulator_backend::scope_backend(b, f))
}
fn world(scene: pe_rs::SceneType) -> PeWorldRun {
    rsvz::reset_runtime_state_preserving_backend();
    let mut w = PeWorldOwner::new_reset(PeWorldConfig {
        scene,
        ..Default::default()
    })
    .unwrap()
    .install_current()
    .unwrap();
    scoped(&mut w, || {
        rsvz::with_backend(|b| b.set_zombie_spawn_stopped(true).unwrap())
    });
    w
}
fn plant(w: &mut PeWorldRun, kind: PlantKind, grid: Grid) -> rsvz_model::PlantId {
    scoped(w, || {
        rsvz::with_backend(|b| {
            let p = b
                .new_plant(CardSelection::Plant(kind).checked().unwrap(), grid)
                .unwrap();
            b.plant_id(p)
        })
    })
}
fn zombie(w: &mut PeWorldRun, kind: ZombieKind, grid: Grid) -> ZombieId {
    scoped(w, || {
        rsvz::with_backend(|b| {
            let z = b.place_zombie(kind, grid).unwrap();
            b.zombie_id(z)
        })
    })
}
fn countdown(w: &mut PeWorldRun, id: ZombieId, value: i32) {
    scoped(w, || {
        rsvz::with_backend(|b| {
            b.set_zombie_phase_countdown(b.zombie(id).unwrap().unwrap(), NonNegativeI32::new(value).unwrap())
                .unwrap();
        })
    });
}
fn safe(w: &mut PeWorldRun, grid: Grid) -> bool {
    scoped(w, || is_safe_blover(grid).unwrap())
}
fn phase(w: &mut PeWorldRun, id: ZombieId) -> ZombiePhase {
    scoped(w, || {
        rsvz::with_backend(|b| b.zombie_phase(b.zombie(id).unwrap().unwrap()).unwrap())
    })
}

#[test]
fn query_is_read_only_and_does_not_require_a_card_or_air_threat() {
    let mut w = world(pe_rs::SceneType::MushroomGarden);
    scoped(&mut w, || {
        let before = rsvz::with_backend(|b| (b.clock().unwrap(), b.plants().unwrap().count(), b.sun().unwrap()));
        assert!(is_safe_blover(GRID).unwrap());
        assert!(is_safe_blover(Grid { row: 5, col: 0 }).is_err());
        assert_eq!(
            before,
            rsvz::with_backend(|b| (b.clock().unwrap(), b.plants().unwrap().count(), b.sun().unwrap()))
        );
    });
}

#[test]
fn jack_uses_countdown_and_two_dimensional_geometry() {
    let mut w = world(pe_rs::SceneType::Night);
    let id = zombie(&mut w, ZombieKind::JackInTheBox, GRID);
    assert!(safe(&mut w, GRID)); // Even a jack about to start opening still needs 110cs.
    countdown(&mut w, id, 1);
    w.update_world().unwrap();
    assert_eq!(phase(&mut w, id), ZombiePhase::JackInTheBoxPopping);
    countdown(&mut w, id, 51);
    assert!(safe(&mut w, GRID));
    countdown(&mut w, id, 20);
    assert!(!safe(&mut w, GRID));
    assert!(!safe(&mut w, Grid { row: 1, col: 2 }));
    assert!(safe(&mut w, Grid { row: 0, col: 2 }));
    assert!(safe(&mut w, Grid { row: 2, col: 8 }));
}

#[test]
fn vehicle_only_threatens_blover_through_another_layer() {
    for (kind, layer) in [
        (ZombieKind::Zomboni, PlantKind::Pumpkin),
        (ZombieKind::Zomboni, PlantKind::FlowerPot),
        (ZombieKind::Catapult, PlantKind::Pumpkin),
    ] {
        let mut w = world(pe_rs::SceneType::Day);
        zombie(&mut w, kind, Grid { row: 2, col: 3 });
        w.update_world().unwrap();
        assert!(safe(&mut w, GRID));
        plant(&mut w, layer, GRID);
        assert!(!safe(&mut w, GRID), "{layer:?}");
        assert!(safe(&mut w, Grid { row: 1, col: 2 }));
    }
}

#[test]
fn roof_geometry_and_container_are_used() {
    let mut w = world(pe_rs::SceneType::Roof);
    let grid = Grid { row: 0, col: 6 };
    zombie(&mut w, ZombieKind::Zomboni, Grid { row: 0, col: 7 });
    w.update_world().unwrap();
    plant(&mut w, PlantKind::FlowerPot, grid);
    assert!(!safe(&mut w, grid));
    assert!(safe(&mut w, Grid { row: 1, col: 6 }));
}

#[test]
fn a_new_smash_is_safe_but_a_pending_smash_is_not() {
    for kind in [ZombieKind::Gargantuar, ZombieKind::GigaGargantuar] {
        let mut w = world(pe_rs::SceneType::Day);
        let id = zombie(&mut w, kind, Grid { row: 2, col: 3 });
        plant(&mut w, PlantKind::WallNut, GRID);
        w.update_world().unwrap();
        assert!(safe(&mut w, GRID));
        let mut reached = false;
        for _ in 0..160 {
            w.update_world().unwrap();
            let t = scoped(&mut w, || {
                rsvz::with_backend(|b| {
                    b.zombie_reanim_anim_time(b.zombie(id).unwrap().unwrap())
                        .unwrap()
                        .unwrap()
                })
            });
            if phase(&mut w, id) == ZombiePhase::GargantuarSmashing && t >= 0.5 && t < 0.6 {
                reached = true;
                assert!(!safe(&mut w, GRID));
                break;
            }
        }
        assert!(reached);
        for _ in 0..60 {
            w.update_world().unwrap();
        }
        assert!(safe(&mut w, GRID));
        let p = plant(&mut w, PlantKind::Blover, GRID);
        for _ in 0..50 {
            w.update_world().unwrap();
        }
        scoped(&mut w, || {
            rsvz::with_backend(|b| {
                assert_eq!(b.plant_state(b.plant(p).unwrap().unwrap()), 2);
            })
        });
    }
}

#[test]
fn bungee_only_blocks_its_imminent_grab_square() {
    let mut w = world(pe_rs::SceneType::Night);
    let id = zombie(&mut w, ZombieKind::Bungee, GRID);
    assert!(safe(&mut w, GRID));
    for _ in 0..1000 {
        if phase(&mut w, id) == ZombiePhase::BungeeAtBottom {
            break;
        }
        w.update_world().unwrap();
    }
    assert_eq!(phase(&mut w, id), ZombiePhase::BungeeAtBottom);
    countdown(&mut w, id, 20);
    assert!(!safe(&mut w, GRID));
    assert!(safe(&mut w, Grid { row: 2, col: 3 }));
    // Adding an umbrella after landing does not undo the existing grab opportunity.
    plant(&mut w, PlantKind::UmbrellaLeaf, Grid { row: 1, col: 2 });
    assert!(!safe(&mut w, GRID));
    let p = plant(&mut w, PlantKind::Blover, GRID);
    for _ in 0..50 {
        w.update_world().unwrap();
    }
    scoped(&mut w, || {
        rsvz::with_backend(|b| {
            assert_ne!(b.plant_on_bungee_state(b.plant(p).unwrap().unwrap()), 0);
        })
    });
}

#[test]
fn safe_predictions_survive_native_smash_and_freeze_windows() {
    for age in [1, 40, 70, 90, 110, 130, 145] {
        for thaw_in in [None, Some(60), Some(20), Some(1)] {
            let mut w = world(pe_rs::SceneType::Night);
            let id = zombie(&mut w, ZombieKind::GigaGargantuar, Grid { row: 2, col: 3 });
            let wall = plant(&mut w, PlantKind::WallNut, GRID);
            for _ in 0..age {
                w.update_world().unwrap();
            }
            if let Some(thaw_in) = thaw_in {
                let ice = plant(&mut w, PlantKind::IceShroom, Grid { row: 4, col: 8 });
                scoped(&mut w, || {
                    rsvz::with_backend(|b| {
                        b.set_plant_effect_countdown(b.plant(ice).unwrap().unwrap(), NonNegativeI32::new(1).unwrap())
                            .unwrap();
                    })
                });
                w.update_world().unwrap();
                while scoped(&mut w, || {
                    rsvz::with_backend(|b| b.zombie_frozen_countdown(b.zombie(id).unwrap().unwrap()))
                }) > thaw_in
                {
                    w.update_world().unwrap();
                }
            }
            scoped(&mut w, || {
                rsvz::with_backend(|b| {
                    if let Some(p) = b.plant(wall).unwrap() {
                        b.remove_plant(p).unwrap();
                    }
                })
            });
            let prediction = safe(&mut w, GRID);
            let p = plant(&mut w, PlantKind::Blover, GRID);
            for _ in 0..50 {
                w.update_world().unwrap();
            }
            let blew = scoped(&mut w, || {
                rsvz::with_backend(|b| {
                    b.plant(p)
                        .unwrap()
                        .is_some_and(|p| !b.plant_is_squished(p) && b.plant_state(p) == 2)
                })
            });
            assert!(!prediction || blew, "unsafe positive: age={age} thaw_in={thaw_in:?}");
        }
    }
}
