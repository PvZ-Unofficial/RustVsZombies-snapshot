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
            for row in [0, 2, 4] {
                let garlic = CardSelection::Plant(PlantKind::Garlic)
                    .checked()
                    .expect("garlic is a valid card selection");
                let grid = Grid::new(row, 3).expect("R1C4, R3C4, and R5C4 are valid grids");
                backend.new_plant(garlic, grid).map_err(runtime_error)?;

                let zombie = backend
                    .add_zombie_in_row(ZombieKind::Normal, row, 0)
                    .map_err(runtime_error)?
                    .ok_or_else(|| rsvz::runtime::RuntimeError::new("failed to create a fixed normal zombie"))?;
                let x = I32RepresentableF32::new(450.0).expect("450 is exactly representable as f32");
                if backend.set_zombie_x(zombie, x).map_err(runtime_error)? != ObjectEditOutcome::Applied {
                    return Err(rsvz::runtime::RuntimeError::new(
                        "failed to set a fixed normal zombie position",
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
        case_id: "garlic-lane-change-r1-r3-r5-c4-frames3000".to_owned(),
        scenario_seed: Some(0x5eed_1051),
        script_id: "witness-garlic-lane-change-v1".to_owned(),
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
                "plants": [
                    {"kind": "garlic", "row": 1, "col": 4},
                    {"kind": "garlic", "row": 3, "col": 4},
                    {"kind": "garlic", "row": 5, "col": 4}
                ],
                "zombies": [
                    {"kind": "normal", "row": 1, "x": 450.0, "from_wave": 0},
                    {"kind": "normal", "row": 3, "x": 450.0, "from_wave": 0},
                    {"kind": "normal", "row": 5, "x": 450.0, "from_wave": 0}
                ],
                "spawn": "stopped-before-frame-0"
            }"#,
        )
        .expect("static Witness setup JSON must be valid");

    witness::start(witness::WitnessOptions {
        locked_random: 7,
        capture: witness::WitnessCapture::Full,
        limit: witness::WitnessLimit::Frames(3000),
        repro,
        ..witness::WitnessOptions::default()
    })?;
    Ok(())
}
