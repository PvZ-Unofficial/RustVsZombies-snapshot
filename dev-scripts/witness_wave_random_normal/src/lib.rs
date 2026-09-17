fn runtime_error(error: impl std::fmt::Display) -> rsvz::runtime::RuntimeError {
    rsvz::runtime::RuntimeError::new(error.to_string())
}

#[rsvz::state_hooks]
fn install_hooks() {
    use rsvz::prelude::{CardSelection, Grid, PlantCreateBackend as _, PlantKind, fail_script, on_enter_fight};

    on_enter_fight(|| {
        let result = rsvz::with_backend(|backend| -> rsvz::runtime::RuntimeResult<()> {
            let selection = CardSelection::Plant(PlantKind::WallNut)
                .checked()
                .expect("WallNut is a valid card selection");
            for row in 0..5 {
                let grid = Grid::new(row, 4).expect("R1C5 through R5C5 are valid grids");
                backend.new_plant(selection, grid).map_err(runtime_error)?;
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
        case_id: "wallnuts-c5-vs-natural-normal-wave1-private-mt-frames4000".to_owned(),
        scenario_seed: Some(0x5eed_1051),
        script_id: "witness-wave-random-normal-v2".to_owned(),
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
                "wave_spawn_random": true,
                "plants": [
                    {"kind": "wall_nut", "row": 1, "col": 5},
                    {"kind": "wall_nut", "row": 2, "col": 5},
                    {"kind": "wall_nut", "row": 3, "col": 5},
                    {"kind": "wall_nut", "row": 4, "col": 5},
                    {"kind": "wall_nut", "row": 5, "col": 5}
                ],
                "zombies": [],
                "spawn": {
                    "mode": "natural",
                    "wave_1": ["normal"]
                }
            }"#,
        )
        .expect("static Witness setup JSON must be valid");

    witness::start(witness::WitnessOptions {
        locked_random: 7,
        wave_spawn_random: true,
        capture: witness::WitnessCapture::Full,
        limit: witness::WitnessLimit::Frames(4000),
        repro,
        ..witness::WitnessOptions::default()
    })?;
    Ok(())
}
