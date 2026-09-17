fn runtime_error(error: impl std::fmt::Display) -> rsvz::runtime::RuntimeError {
    rsvz::runtime::RuntimeError::new(error.to_string())
}

#[rsvz::state_hooks]
fn install_hooks() {
    use rsvz::prelude::{CardSelection, Grid, PlantCreateBackend as _, PlantKind, fail_script, on_enter_fight};

    on_enter_fight(|| {
        let result = rsvz::with_backend(|backend| -> rsvz::runtime::RuntimeResult<()> {
            let selection = CardSelection::Plant(PlantKind::Peashooter)
                .checked()
                .expect("Peashooter is a valid card selection");
            let grid = Grid::new(2, 2).expect("R3C3 is a valid grid");
            backend.new_plant(selection, grid).map_err(runtime_error)?;
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
        case_id: "peashooter-r3c3-vs-natural-normal-locked-mid-frames3500".to_owned(),
        scenario_seed: Some(0x5eed_1051),
        script_id: "witness-peashooter-natural-normal-v1".to_owned(),
        build_id: env!("CARGO_PKG_VERSION").to_owned(),
        ..witness::WitnessRepro::default()
    };
    repro
        .set_expanded_setup_json(
            r#"{
                "scene": "night",
                "completed_rounds": 63,
                "initial_sun": 8000,
                "locked_random": 1073741824,
                "plants": [{"kind": "peashooter", "row": 3, "col": 3}],
                "zombies": [],
                "spawn": {
                    "mode": "natural",
                    "wave_1": ["normal"]
                }
            }"#,
        )
        .expect("static Witness setup JSON must be valid");

    witness::start(witness::WitnessOptions {
        locked_random: 0x4000_0000,
        capture: witness::WitnessCapture::Full,
        limit: witness::WitnessLimit::Frames(3500),
        repro,
        ..witness::WitnessOptions::default()
    })?;
    Ok(())
}
