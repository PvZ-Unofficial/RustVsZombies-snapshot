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
            let selection = CardSelection::Plant(PlantKind::Peashooter)
                .checked()
                .expect("Peashooter is a valid card selection");
            let grid = Grid::new(2, 2).expect("R3C3 is a valid grid");
            let _plant = backend.new_plant(selection, grid).map_err(runtime_error)?;

            let zombie = backend
                .add_zombie_in_row(ZombieKind::Normal, 2, 0)
                .map_err(runtime_error)?
                .ok_or_else(|| rsvz::runtime::RuntimeError::new("failed to create the fixed normal zombie"))?;
            let x = I32RepresentableF32::new(780.0).expect("780 is exactly representable as f32");
            if backend.set_zombie_x(zombie, x).map_err(runtime_error)? != ObjectEditOutcome::Applied {
                return Err(rsvz::runtime::RuntimeError::new(
                    "failed to set the fixed normal zombie position",
                ));
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
        "PINAJ",
        [
            PlantKind::FlowerPot,
            PlantKind::SplitPea,
            PlantKind::FumeShroom,
            PlantKind::Starfruit,
            PlantKind::GraveBuster,
        ],
    );

    let mut repro = witness::WitnessRepro {
        case_id: "peashooter-r3c3-vs-normal-r3-x780-frames2500".to_owned(),
        scenario_seed: Some(0x5eed_1051),
        script_id: "witness-peashooter-normal-v1".to_owned(),
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
                "plants": [{"kind": "peashooter", "row": 3, "col": 3}],
                "zombies": [{"kind": "normal", "row": 3, "x": 780.0, "from_wave": 0}],
                "spawn": "stopped-before-frame-0"
            }"#,
        )
        .expect("static Witness setup JSON must be valid");

    witness::start(witness::WitnessOptions {
        locked_random: 7,
        capture: witness::WitnessCapture::Full,
        limit: witness::WitnessLimit::Frames(2500),
        repro,
        ..witness::WitnessOptions::default()
    })?;
    Ok(())
}
