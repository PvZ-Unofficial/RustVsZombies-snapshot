fn runtime_error(error: impl std::fmt::Display) -> rsvz::runtime::RuntimeError {
    rsvz::runtime::RuntimeError::new(error.to_string())
}

#[rsvz::state_hooks]
fn install_hooks() {
    use rsvz::core::model::I32RepresentableF32;
    use rsvz::prelude::{
        CardSelection, Grid, ObjectEditOutcome, PlantCreateBackend as _, PlantKind, ZombieCreateBackend as _,
        ZombieKind, ZombieXWriteBackend as _, fail_script, on_enter_fight,
    };

    on_enter_fight(|| {
        let result = rsvz::with_backend(|backend| -> rsvz::runtime::RuntimeResult<()> {
            for (row, col, kind) in [
                (0, 1, PlantKind::Peashooter),
                (0, 4, PlantKind::Torchwood),
                (1, 1, PlantKind::SnowPea),
                (1, 4, PlantKind::Torchwood),
                (2, 1, PlantKind::SnowPea),
                (2, 3, PlantKind::Torchwood),
                (2, 5, PlantKind::Torchwood),
                (3, 1, PlantKind::Peashooter),
                (3, 4, PlantKind::Torchwood),
                (4, 1, PlantKind::Peashooter),
                (4, 4, PlantKind::Torchwood),
            ] {
                backend
                    .new_plant(
                        CardSelection::Plant(kind)
                            .checked()
                            .expect("ordinary plants are valid card selections"),
                        Grid::new(row, col).expect("fixed Torchwood test grid is valid"),
                    )
                    .map_err(runtime_error)?;
            }

            for (row, kind, x) in [
                (0, ZombieKind::Normal, 700.0),
                (1, ZombieKind::Normal, 700.0),
                (2, ZombieKind::Normal, 700.0),
                (3, ZombieKind::Normal, 700.0),
                (3, ZombieKind::Normal, 720.0),
                (4, ZombieKind::ScreenDoor, 700.0),
                (4, ZombieKind::Normal, 720.0),
            ] {
                let zombie = backend
                    .add_zombie_in_row(kind, row, 0)
                    .map_err(runtime_error)?
                    .ok_or_else(|| rsvz::runtime::RuntimeError::new("failed to create a Torchwood test zombie"))?;
                let x = I32RepresentableF32::new(x).expect("integer zombie x is exactly representable as f32");
                if backend.set_zombie_x(zombie, x).map_err(runtime_error)? != ObjectEditOutcome::Applied {
                    return Err(rsvz::runtime::RuntimeError::new(
                        "failed to set a Torchwood test zombie position",
                    ));
                }
            }
            Ok(())
        });
        if let Err(error) = result {
            fail_script(error);
        }
    });
}

#[rsvz::script]
fn script() -> rsvz::runtime::RuntimeResult<()> {
    use rsvz::prelude::*;
    use rsvz_witness as witness;

    rsvz::with_backend(|backend| backend.set_zombie_spawn_stopped(true).map_err(runtime_error))?;
    lineup("LMg3NPRBVFRUDlRVVQ==");
    set_zombies("普障");
    set_wave_zombies(1, "普障");
    select_cards(
        "IINAJ",
        [
            PlantKind::FlowerPot,
            PlantKind::SplitPea,
            PlantKind::FumeShroom,
            PlantKind::Starfruit,
            PlantKind::GraveBuster,
        ],
    );

    let mut repro = witness::WitnessRepro {
        case_id: "five-row-torchwood-projectile-conversion-and-splash-frames2800".to_owned(),
        scenario_seed: Some(0x5eed_1051),
        script_id: "witness-torchwood-projectiles-v1".to_owned(),
        build_id: env!("CARGO_PKG_VERSION").to_owned(),
        ..witness::WitnessRepro::default()
    };
    repro
        .set_expanded_setup_json(
            r#"{
                "scene": "night",
                "completed_rounds": 63,
                "initial_sun": 8000,
                "locked_random": 7,
                "rows": [
                    {"row": 1, "plants": [{"kind": "peashooter", "col": 2}, {"kind": "torchwood", "col": 5}], "zombies": [{"kind": "normal", "x": 700.0}], "expect": "pea_to_fireball"},
                    {"row": 2, "plants": [{"kind": "snow_pea", "col": 2}, {"kind": "torchwood", "col": 5}], "zombies": [{"kind": "normal", "x": 700.0}], "expect": "snow_pea_to_pea_without_slow"},
                    {"row": 3, "plants": [{"kind": "snow_pea", "col": 2}, {"kind": "torchwood", "col": 4}, {"kind": "torchwood", "col": 6}], "zombies": [{"kind": "normal", "x": 700.0}], "expect": "snow_pea_to_pea_to_fireball"},
                    {"row": 4, "plants": [{"kind": "peashooter", "col": 2}, {"kind": "torchwood", "col": 5}], "zombies": [{"kind": "normal", "x": 700.0}, {"kind": "normal", "x": 720.0}], "expect": "fireball_splash"},
                    {"row": 5, "plants": [{"kind": "peashooter", "col": 2}, {"kind": "torchwood", "col": 5}], "zombies": [{"kind": "screen_door", "x": 700.0}, {"kind": "normal", "x": 720.0}], "expect": "fire_resistant_target_cancels_splash"}
                ],
                "plants": [
                    {"kind": "peashooter", "row": 1, "col": 2}, {"kind": "torchwood", "row": 1, "col": 5},
                    {"kind": "snow_pea", "row": 2, "col": 2}, {"kind": "torchwood", "row": 2, "col": 5},
                    {"kind": "snow_pea", "row": 3, "col": 2}, {"kind": "torchwood", "row": 3, "col": 4}, {"kind": "torchwood", "row": 3, "col": 6},
                    {"kind": "peashooter", "row": 4, "col": 2}, {"kind": "torchwood", "row": 4, "col": 5},
                    {"kind": "peashooter", "row": 5, "col": 2}, {"kind": "torchwood", "row": 5, "col": 5}
                ],
                "zombies": [
                    {"kind": "normal", "row": 1, "x": 700.0}, {"kind": "normal", "row": 2, "x": 700.0},
                    {"kind": "normal", "row": 3, "x": 700.0}, {"kind": "normal", "row": 4, "x": 700.0},
                    {"kind": "normal", "row": 4, "x": 720.0}, {"kind": "screen_door", "row": 5, "x": 700.0},
                    {"kind": "normal", "row": 5, "x": 720.0}
                ],
                "spawn": "stopped-before-frame-0"
            }"#,
        )
        .expect("static Witness setup JSON must be valid");

    witness::start(witness::WitnessOptions {
        locked_random: 7,
        capture: witness::WitnessCapture::Full,
        limit: witness::WitnessLimit::Frames(2800),
        repro,
        ..witness::WitnessOptions::default()
    })?;
    Ok(())
}
