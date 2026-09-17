#![cfg(feature = "pvz-emulator")]

use rsvz::is_safe_imitator_ice;
use rsvz_backend_api::*;
use rsvz_model::{
    CardSelection, Grid, I32RepresentableF32, NonNegativeI32, PlantKind, ZombieId, ZombieKind, ZombiePhase,
};
use rsvz_pvz_emulator_backend::{
    PeWorldConfig, pe_rs,
    runner_internal::{PeWorldOwner, PeWorldRun},
};

const GRID: Grid = Grid { row: 2, col: 2 };
fn scoped<T>(w: &mut PeWorldRun, f: impl FnOnce() -> T) -> T {
    w.with_backend(|b| rsvz_pvz_emulator_backend::scope_backend(b, f))
}
fn world() -> PeWorldRun {
    rsvz::reset_runtime_state_preserving_backend();
    let mut w = PeWorldOwner::new_reset(PeWorldConfig {
        scene: pe_rs::SceneType::MushroomGarden,
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
fn zombie(w: &mut PeWorldRun, kind: ZombieKind, row: i32, x: f32) -> ZombieId {
    scoped(w, || {
        rsvz::with_backend(|b| {
            let z = b.place_zombie(kind, Grid { row, col: 8 }).unwrap();
            b.set_zombie_x(z, I32RepresentableF32::new(x).unwrap()).unwrap();
            b.zombie_id(z)
        })
    })
}
fn move_to(w: &mut PeWorldRun, id: ZombieId, x: f32) {
    scoped(w, || {
        rsvz::with_backend(|b| {
            b.set_zombie_x(b.zombie(id).unwrap().unwrap(), I32RepresentableF32::new(x).unwrap())
                .unwrap();
        })
    });
}
fn phase_countdown(w: &mut PeWorldRun, id: ZombieId, n: i32) {
    scoped(w, || {
        rsvz::with_backend(|b| {
            b.set_zombie_phase_countdown(b.zombie(id).unwrap().unwrap(), NonNegativeI32::new(n).unwrap())
                .unwrap();
        })
    });
}
fn safe(w: &mut PeWorldRun) -> bool {
    scoped(w, || is_safe_imitator_ice(GRID).unwrap())
}
fn slowed(w: &mut PeWorldRun, ids: &[ZombieId], remaining: i32) {
    scoped(w, || {
        rsvz::with_backend(|b| {
            let p = b
                .new_plant(
                    CardSelection::Plant(PlantKind::IceShroom).checked().unwrap(),
                    Grid { row: 4, col: 8 },
                )
                .unwrap();
            b.set_plant_effect_countdown(p, NonNegativeI32::new(1).unwrap())
                .unwrap();
        })
    });
    for _ in 0..101 {
        w.update_world().unwrap();
    }
    scoped(w, || {
        rsvz::with_backend(|b| assert!(b.zombie_chilled_countdown(b.zombie(ids[0]).unwrap().unwrap()) > 0))
    });
    for tick in 0..2100 {
        let done = scoped(w, || {
            rsvz::with_backend(|b| {
                let z = b.zombie(ids[0]).unwrap().unwrap();
                b.zombie_chilled_countdown(z) <= remaining && b.zombie_frozen_countdown(z) == 0
            })
        });
        if done {
            return;
        }
        if tick % 100 == 0 {
            for &id in ids {
                move_to(w, id, 850.0);
            }
        }
        w.update_world().unwrap();
    }
    panic!("slow countdown did not reach requested value");
}
fn plant_and_check_effect(w: &mut PeWorldRun) -> bool {
    let witness = zombie(w, ZombieKind::Normal, 0, 850.0);
    scoped(w, || {
        rsvz::with_backend(|b| {
            b.new_plant(CardSelection::Imitator(PlantKind::IceShroom).checked().unwrap(), GRID)
                .unwrap();
        })
    });
    for _ in 0..425 {
        w.update_world().unwrap();
    }
    scoped(w, || {
        rsvz::with_backend(|b| b.zombie_frozen_countdown(b.zombie(witness).unwrap().unwrap()) > 0)
    })
}

#[test]
fn query_is_read_only_and_freeze_is_a_global_error() {
    let mut w = world();
    scoped(&mut w, || {
        let before = rsvz::with_backend(|b| (b.clock().unwrap(), b.sun().unwrap(), b.plants().unwrap().count()));
        assert!(is_safe_imitator_ice(GRID).unwrap());
        assert!(is_safe_imitator_ice(Grid { row: 5, col: 2 }).is_err());
        assert_eq!(
            before,
            rsvz::with_backend(|b| (b.clock().unwrap(), b.sun().unwrap(), b.plants().unwrap().count()))
        );
    });
    zombie(&mut w, ZombieKind::Zomboni, 2, 220.0); // An unsafe result must not hide the later freeze error.
    zombie(&mut w, ZombieKind::Normal, 4, 850.0);
    scoped(&mut w, || {
        rsvz::with_backend(|b| {
            let p = b
                .new_plant(
                    CardSelection::Plant(PlantKind::IceShroom).checked().unwrap(),
                    Grid { row: 0, col: 8 },
                )
                .unwrap();
            b.set_plant_effect_countdown(p, NonNegativeI32::new(1).unwrap())
                .unwrap();
        })
    });
    for _ in 0..101 {
        w.update_world().unwrap();
    }
    scoped(&mut w, || {
        assert!(is_safe_imitator_ice(GRID).unwrap_err().to_string().contains("thawed"))
    });
}

#[test]
fn slowed_biter_can_be_safe_but_two_can_eat_the_placeholder() {
    for count in [1, 2] {
        let mut w = world();
        let ids: Vec<_> = (0..count)
            .map(|_| zombie(&mut w, ZombieKind::Normal, 2, 850.0))
            .collect();
        slowed(&mut w, &ids, 1000);
        for &id in &ids {
            move_to(&mut w, id, 180.0);
        }
        assert_eq!(safe(&mut w), count == 1);
        assert_eq!(plant_and_check_effect(&mut w), count == 1);
    }
}

#[test]
fn normal_speed_bite_and_late_contact_match_native_survival() {
    for x in [180.0, 240.0, 300.0, 400.0] {
        let mut w = world();
        zombie(&mut w, ZombieKind::Football, 2, x);
        let prediction = safe(&mut w);
        let activated = plant_and_check_effect(&mut w);
        assert!(!prediction || activated, "false safe football at x={x}");
        if x == 180.0 {
            assert!(!prediction);
            assert!(!activated);
        }
        if x == 400.0 {
            assert!(prediction);
        }
    }
}

#[test]
fn walking_giant_predictions_survive_native_morph_and_slow_boundaries() {
    let mut accepted = 0;
    let mut rejected = 0;
    for kind in [ZombieKind::Gargantuar, ZombieKind::GigaGargantuar] {
        for slow in [0, 50, 1000] {
            for x in 260..=380 {
                let x = x as f32;
                let mut w = world();
                let id = zombie(&mut w, kind, 2, 850.0);
                if slow > 0 {
                    slowed(&mut w, &[id], slow);
                }
                move_to(&mut w, id, x);
                let prediction = safe(&mut w);
                let activated = plant_and_check_effect(&mut w);
                assert!(!prediction || activated, "false safe {kind:?}, slow={slow}, x={x}");
                if prediction {
                    accepted += 1;
                } else {
                    rejected += 1;
                }
            }
        }
    }
    assert!(accepted > 0 && rejected > 0);
}

#[test]
fn unopened_jack_does_not_use_its_hidden_countdown() {
    let mut w = world();
    let id = zombie(&mut w, ZombieKind::JackInTheBox, 2, 300.0);
    phase_countdown(&mut w, id, 1);
    let near = safe(&mut w);
    phase_countdown(&mut w, id, 10000);
    assert_eq!(near, safe(&mut w));
    assert!(!near);
    move_to(&mut w, id, 900.0);
    assert!(safe(&mut w));
}

#[test]
fn opened_jack_can_destroy_the_already_morphed_ice() {
    let mut w = world();
    let id = zombie(&mut w, ZombieKind::JackInTheBox, 2, 230.0);
    phase_countdown(&mut w, id, 1);
    w.update_world().unwrap();
    scoped(&mut w, || {
        rsvz::with_backend(|b| {
            assert_eq!(
                b.zombie_phase(b.zombie(id).unwrap().unwrap()).unwrap(),
                ZombiePhase::JackInTheBoxPopping
            )
        })
    });
    phase_countdown(&mut w, id, 350);
    assert!(!safe(&mut w));
    assert!(!plant_and_check_effect(&mut w));
}

#[test]
fn vehicle_safety_uses_the_pre_morph_window() {
    for x in [240.0, 320.0, 450.0] {
        let mut w = world();
        zombie(&mut w, ZombieKind::Zomboni, 2, x);
        let prediction = safe(&mut w);
        assert!(!prediction || plant_and_check_effect(&mut w));
        if x == 240.0 {
            assert!(!prediction);
        }
        if x == 450.0 {
            assert!(prediction);
        }
    }
}

#[test]
fn pumpkin_protects_against_bites_but_extends_hammer_contact() {
    let mut w = world();
    scoped(&mut w, || {
        rsvz::with_backend(|b| {
            b.new_plant(CardSelection::Plant(PlantKind::Pumpkin).checked().unwrap(), GRID)
                .unwrap();
        })
    });
    zombie(&mut w, ZombieKind::Normal, 2, 180.0);
    assert!(safe(&mut w));
    assert!(plant_and_check_effect(&mut w));
    drop(w);

    let mut w = world();
    let id = zombie(&mut w, ZombieKind::Gargantuar, 2, 850.0);
    slowed(&mut w, &[id], 1000);
    move_to(&mut w, id, 300.0);
    assert!(safe(&mut w));
    scoped(&mut w, || {
        rsvz::with_backend(|b| {
            b.new_plant(CardSelection::Plant(PlantKind::Pumpkin).checked().unwrap(), GRID)
                .unwrap();
        })
    });
    assert!(!safe(&mut w));
}

#[test]
fn bungee_grab_is_checked_until_effect_not_only_morph() {
    for grab_in in [350, 450] {
        let mut w = world();
        let id = scoped(&mut w, || {
            rsvz::with_backend(|b| {
                let z = b.place_zombie(ZombieKind::Bungee, GRID).unwrap();
                b.zombie_id(z)
            })
        });
        for _ in 0..1000 {
            let landed = scoped(&mut w, || {
                rsvz::with_backend(|b| {
                    b.zombie_phase(b.zombie(id).unwrap().unwrap()).unwrap() == ZombiePhase::BungeeAtBottom
                })
            });
            if landed {
                break;
            }
            w.update_world().unwrap();
        }
        phase_countdown(&mut w, id, grab_in);
        assert_eq!(safe(&mut w), grab_in > 420);
        assert_eq!(plant_and_check_effect(&mut w), grab_in > 420);
    }
}

#[test]
fn missing_dancers_share_the_bite_budget() {
    let mut w = world();
    zombie(&mut w, ZombieKind::Dancing, 1, 210.0);
    assert!(safe(&mut w));
    zombie(&mut w, ZombieKind::Dancing, 3, 210.0);
    assert!(!safe(&mut w));
}

#[test]
fn daytime_coffee_and_roof_trajectories_are_not_claimed_supported() {
    for scene in [pe_rs::SceneType::Day, pe_rs::SceneType::MoonNight] {
        rsvz::reset_runtime_state_preserving_backend();
        let mut w = PeWorldOwner::new_reset(PeWorldConfig {
            scene,
            ..Default::default()
        })
        .unwrap()
        .install_current()
        .unwrap();
        scoped(&mut w, || assert!(is_safe_imitator_ice(GRID).is_err()));
    }
}
