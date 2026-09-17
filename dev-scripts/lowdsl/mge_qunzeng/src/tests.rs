use super::*;
use rsvz_pvz_emulator_backend::{PeWorldConfig, pe_rs, runner_internal::PeWorldOwner};

pub(super) fn scene(f: impl FnOnce()) {
    scene_with_setup(|| {}, f);
}
pub(super) fn scene_with_setup(setup: impl FnOnce(), f: impl FnOnce()) {
    rsvz::reset_runtime_state_preserving_backend();
    let mut world = PeWorldOwner::new_reset(PeWorldConfig {
        scene: pe_rs::SceneType::MushroomGarden,
        ..Default::default()
    })
    .unwrap()
    .install_current()
    .unwrap();
    world.with_backend(|b| rsvz_pvz_emulator_backend::scope_backend(b, setup));
    world.update_world().unwrap();
    world.with_backend(|b| {
        rsvz_pvz_emulator_backend::scope_backend(b, || {
            rsvz::with_backend(|b| {
                b.set_sun(8000).unwrap();
                b.select_card(sel(Blover).checked().unwrap()).unwrap();
                b.select_card(MIMIC_ICE.checked().unwrap()).unwrap();
                b.finish_card_selection().unwrap();
            });
            f();
        })
    });
}
pub(super) fn put(kind: PlantKind, pos: Pos) {
    rsvz::with_backend(|b| {
        b.new_plant(sel(kind).checked().unwrap(), grid(pos)).unwrap();
    });
}

#[test]
fn unsafe_planting_does_not_spend_card_or_sun_and_next_grid_is_tried() {
    scene_with_setup(
        || {
            rsvz::with_backend(|b| {
                let z = b.place_zombie(Z::Zomboni, grid((1, 6))).unwrap();
                assert!(b.zombie_is_alive(z));
            });
        },
        || {
            put(Pumpkin, (1, 5));
            assert!(!rsvz::is_safe_blover(grid((1, 5))).unwrap());
            assert!(!play(sel(Blover), (1, 5)));
            assert_eq!(sun(), 8000);
            assert!(usable(sel(Blover)));
            assert!(try_positions(sel(Blover), &[(1, 5), (5, 5)]));
            assert!(has(Blover, (5, 5)));
        },
    );
}

#[test]
fn card_choice_only_changes_gloom_threshold() {
    for count in [8, 9] {
        for balance in [1999, 2000, 4999, 5000] {
            for jack in [false, true] {
                scene(|| {
                    for col in 1..=count {
                        put(GloomShroom, (3, col));
                    }
                    rsvz::with_backend(|b| {
                        b.set_sun(balance).unwrap();
                        b.set_spawn_type_allowed(Z::JackInTheBox, jack).unwrap();
                    });
                    let cards = choose_cards().unwrap();
                    assert_eq!(cards.len(), 10);
                    assert_eq!(
                        cards.contains(&sel(GloomShroom)),
                        count < 9 || balance >= 5000 || jack && balance >= 2000
                    );
                    assert_eq!(
                        &cards[..6],
                        &[
                            sel(IceShroom),
                            MIMIC_ICE,
                            sel(CherryBomb),
                            sel(Squash),
                            sel(Pumpkin),
                            sel(FumeShroom)
                        ]
                    );
                });
            }
        }
    }
}
