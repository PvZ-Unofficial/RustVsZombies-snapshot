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
            let ice = CardSelection::Plant(PlantKind::IceShroom)
                .checked()
                .expect("IceShroom is a valid card selection");
            backend
                .new_plant(ice, Grid::new(2, 8).expect("R3C9 is a valid grid"))
                .map_err(runtime_error)?;

            for (row, kind, x) in [
                (0, ZombieKind::Normal, 1000.0),
                (0, ZombieKind::Football, 1000.0),
                (1, ZombieKind::Ladder, 1000.0),
                (1, ZombieKind::Catapult, 1000.0),
                (2, ZombieKind::Gargantuar, 1000.0),
                (3, ZombieKind::Balloon, 1000.0),
                (3, ZombieKind::Pogo, 1000.0),
                (4, ZombieKind::Digger, 2000.0),
                (4, ZombieKind::Zomboni, 1000.0),
            ] {
                let zombie = backend
                    .add_zombie_in_row(kind, row, 0)
                    .map_err(runtime_error)?
                    .ok_or_else(|| rsvz::runtime::RuntimeError::new("failed to create a fixed freeze-test zombie"))?;
                let x = I32RepresentableF32::new(x).expect("fixed zombie x is exactly representable as f32");
                if backend.set_zombie_x(zombie, x).map_err(runtime_error)? != ObjectEditOutcome::Applied {
                    return Err(rsvz::runtime::RuntimeError::new(
                        "failed to set a fixed freeze-test zombie position",
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
        case_id: "ice-shroom-global-freeze-nine-fixed-zombies-digger-x2000-frames2300".to_owned(),
        scenario_seed: Some(0x5eed_1051),
        script_id: "witness-ice-shroom-global-freeze-v1".to_owned(),
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
                    {"kind": "ice_shroom", "row": 3, "col": 9, "created_at": 0}
                ],
                "zombies": [
                    {"kind": "normal", "row": 1, "x": 1000.0, "from_wave": 0},
                    {"kind": "football", "row": 1, "x": 1000.0, "from_wave": 0},
                    {"kind": "ladder", "row": 2, "x": 1000.0, "from_wave": 0},
                    {"kind": "catapult", "row": 2, "x": 1000.0, "from_wave": 0},
                    {"kind": "gargantuar", "row": 3, "x": 1000.0, "from_wave": 0},
                    {"kind": "balloon", "row": 4, "x": 1000.0, "from_wave": 0},
                    {"kind": "pogo", "row": 4, "x": 1000.0, "from_wave": 0},
                    {"kind": "digger", "row": 5, "x": 2000.0, "from_wave": 0},
                    {"kind": "zomboni", "row": 5, "x": 1000.0, "from_wave": 0}
                ],
                "spawn": "stopped-before-frame-0"
            }"#,
        )
        .expect("static Witness setup JSON must be valid");

    witness::start(witness::WitnessOptions {
        locked_random: 7,
        capture: witness::WitnessCapture::Full,
        limit: witness::WitnessLimit::Frames(2300),
        repro,
        ..witness::WitnessOptions::default()
    })?;
    Ok(())
}
