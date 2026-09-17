fn runtime_error(error: impl std::fmt::Display) -> rsvz::runtime::RuntimeError {
    rsvz::runtime::RuntimeError::new(error.to_string())
}

#[rsvz::state_hooks]
fn install_hooks() {
    use rsvz::core::model::PositiveHp;
    use rsvz::prelude::{
        CardSelection, Grid, ObjectEditOutcome, PlantCreateBackend as _, PlantHealthWriteBackend as _, PlantKind,
        fail_script, on_enter_fight,
    };

    on_enter_fight(|| {
        let result = rsvz::with_backend(|backend| -> rsvz::runtime::RuntimeResult<()> {
            let fixture_hp = PositiveHp::new(100_000).expect("fixture HP is positive");
            for row in 0..5 {
                for (col, kind) in [(0, PlantKind::Torchwood), (3, PlantKind::Garlic)] {
                    let selection = CardSelection::Plant(kind)
                        .checked()
                        .expect("ordinary plants are valid card selections");
                    let grid = Grid::new(row, col).expect("C1 and C4 are valid grids");
                    let plant = backend.new_plant(selection, grid).map_err(runtime_error)?;
                    if backend.set_plant_hp(plant, fixture_hp).map_err(runtime_error)?
                        != ObjectEditOutcome::Applied
                    {
                        return Err(rsvz::runtime::RuntimeError::new("failed to set fixture plant HP"));
                    }
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

    lineup("LMg3NPRBVFRUDlRVVQ==");
    set_zombies("普橄梯");
    set_wave_zombies(1, "普橄梯");
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
        case_id: "garlic-c4-torchwood-c1-natural-normal-football-ladder-private-mt-frames5000".to_owned(),
        scenario_seed: Some(0x5eed_1051),
        script_id: "witness-garlic-natural-mixed-v1".to_owned(),
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
                    {"kind": "torchwood", "row": 1, "col": 1, "hp": 100000},
                    {"kind": "garlic", "row": 1, "col": 4, "hp": 100000},
                    {"kind": "torchwood", "row": 2, "col": 1, "hp": 100000},
                    {"kind": "garlic", "row": 2, "col": 4, "hp": 100000},
                    {"kind": "torchwood", "row": 3, "col": 1, "hp": 100000},
                    {"kind": "garlic", "row": 3, "col": 4, "hp": 100000},
                    {"kind": "torchwood", "row": 4, "col": 1, "hp": 100000},
                    {"kind": "garlic", "row": 4, "col": 4, "hp": 100000},
                    {"kind": "torchwood", "row": 5, "col": 1, "hp": 100000},
                    {"kind": "garlic", "row": 5, "col": 4, "hp": 100000}
                ],
                "zombies": [],
                "spawn": {
                    "mode": "natural",
                    "wave_1": ["normal", "football", "ladder"]
                }
            }"#,
        )
        .expect("static Witness setup JSON must be valid");

    witness::start(witness::WitnessOptions {
        locked_random: 7,
        wave_spawn_random: true,
        capture: witness::WitnessCapture::Full,
        limit: witness::WitnessLimit::Frames(5000),
        repro,
        ..witness::WitnessOptions::default()
    })?;
    Ok(())
}
