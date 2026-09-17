fn runtime_error(error: impl std::fmt::Display) -> rsvz::runtime::RuntimeError {
    rsvz::runtime::RuntimeError::new(error.to_string())
}

#[rsvz::state_hooks]
fn install_hooks() {
    use rsvz::core::model::{I32RepresentableF32, NonNegativeI32};
    use rsvz::prelude::{
        CardSelection, Grid, ObjectEditOutcome, PlantCreateBackend as _, PlantEffectCountdownWriteBackend as _,
        PlantKind, PlantStateWriteBackend as _, ZombieCreateBackend as _, ZombieKind, ZombieXWriteBackend as _,
        fail_script, on_enter_fight,
    };

    on_enter_fight(|| {
        let result = rsvz::with_backend(|backend| -> rsvz::runtime::RuntimeResult<()> {
            let doom = backend
                .new_plant(
                    CardSelection::Plant(PlantKind::DoomShroom)
                        .checked()
                        .expect("DoomShroom is a valid card selection"),
                    Grid::new(2, 4).expect("R3C5 is a valid grid"),
                )
                .map_err(runtime_error)?;
            backend.set_plant_state(doom, 2).map_err(runtime_error)?;
            if backend
                .set_plant_effect_countdown(
                    doom,
                    NonNegativeI32::new(1).expect("one is a non-negative countdown"),
                )
                .map_err(runtime_error)?
                != ObjectEditOutcome::Applied
            {
                return Err(rsvz::runtime::RuntimeError::new(
                    "failed to normalize the Doom-shroom effect countdown",
                ));
            }

            for (row, x) in [
                (2, 72.0),
                (2, 71.0),
                (2, 614.0),
                (2, 615.0),
                (0, 560.0),
                (0, 561.0),
                (4, 109.0),
                (4, 108.0),
                (2, 360.0),
                (2, 750.0),
            ] {
                let zombie = backend
                    .add_zombie_in_row(ZombieKind::Normal, row, 0)
                    .map_err(runtime_error)?
                    .ok_or_else(|| rsvz::runtime::RuntimeError::new("failed to create a boundary zombie"))?;
                let x = I32RepresentableF32::new(x).expect("integer zombie x is exactly representable as f32");
                if backend.set_zombie_x(zombie, x).map_err(runtime_error)? != ObjectEditOutcome::Applied {
                    return Err(rsvz::runtime::RuntimeError::new(
                        "failed to set a boundary zombie position",
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
    set_zombies("普");
    set_wave_zombies(1, "普");
    select_cards(
        "NINAJ",
        [
            PlantKind::FlowerPot,
            PlantKind::SplitPea,
            PlantKind::FumeShroom,
            PlantKind::Starfruit,
            PlantKind::GraveBuster,
        ],
    );

    let mut repro = witness::WitnessRepro {
        case_id: "doom-r3c5-inclusive-circle-rect-boundaries-and-crater-frames100".to_owned(),
        scenario_seed: Some(0x5eed_1051),
        script_id: "witness-doom-cross-row-v1".to_owned(),
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
                "plants": [{"kind": "doom_shroom", "row": 3, "col": 5, "state": "doing_special", "effect_countdown": 1}],
                "zombies": [
                    {"case": "left_tangent_hit", "kind": "normal", "row": 3, "x": 72.0},
                    {"case": "left_outside_miss", "kind": "normal", "row": 3, "x": 71.0},
                    {"case": "right_tangent_hit", "kind": "normal", "row": 3, "x": 614.0},
                    {"case": "right_outside_miss", "kind": "normal", "row": 3, "x": 615.0},
                    {"case": "upper_right_boundary_hit", "kind": "normal", "row": 1, "x": 560.0},
                    {"case": "upper_right_outside_miss", "kind": "normal", "row": 1, "x": 561.0},
                    {"case": "lower_left_boundary_hit", "kind": "normal", "row": 5, "x": 109.0},
                    {"case": "lower_left_outside_miss", "kind": "normal", "row": 5, "x": 108.0},
                    {"case": "clear_hit", "kind": "normal", "row": 3, "x": 360.0},
                    {"case": "clear_horizontal_miss", "kind": "normal", "row": 3, "x": 750.0}
                ],
                "spawn": "stopped-before-frame-0"
            }"#,
        )
        .expect("static Witness setup JSON must be valid");

    witness::start(witness::WitnessOptions {
        locked_random: 7,
        capture: witness::WitnessCapture::Full,
        limit: witness::WitnessLimit::Frames(100),
        repro,
        ..witness::WitnessOptions::default()
    })?;
    Ok(())
}
