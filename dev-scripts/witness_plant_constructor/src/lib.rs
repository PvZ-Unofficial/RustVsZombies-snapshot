#[rsvz::state_hooks]
fn install_hooks() {
    use rsvz::prelude::{
        CardSelection, Grid, PlantCreateBackend as _, PlantKind, ZombieRuleEditBackend as _, fail_script,
        on_before_script, on_enter_fight,
    };

    on_before_script(|| {
        if let Err(error) = rsvz::with_backend(|backend| backend.set_zombie_spawn_stopped(true)) {
            fail_script(rsvz::runtime::RuntimeError::new(error.to_string()));
        }
    });
    on_enter_fight(|| {
        let selection = CardSelection::Plant(PlantKind::Peashooter)
            .checked()
            .expect("Peashooter is a valid card selection");
        let grid = Grid::new(0, 2).expect("R1C3 is a valid grid");
        let result = rsvz::with_backend(|backend| {
            backend.set_zombie_spawn_stopped(true)?;
            backend.new_plant(selection, grid).map(|_plant| ())
        });
        if let Err(error) = result {
            fail_script(rsvz::runtime::RuntimeError::new(error.to_string()));
        }
    });
}

#[rsvz::script]
fn script() -> rsvz::runtime::RuntimeResult<()> {
    use rsvz::dsl::prelude::*;
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
        case_id: "constructor-peashooter-r1c3".to_owned(),
        scenario_seed: Some(0x5eed_1051),
        script_id: "witness-plant-constructor-v1".to_owned(),
        build_id: env!("CARGO_PKG_VERSION").to_owned(),
        ..witness::WitnessRepro::default()
    };
    repro
        .set_expanded_setup_json(
            r#"{
                "plants": [{"kind": "peashooter", "row": 1, "col": 3}],
                "zombies": [],
                "spawn": "stopped-before-frame-0"
            }"#,
        )
        .expect("static Witness setup JSON must be valid");

    witness::start(witness::WitnessOptions {
        capture: witness::WitnessCapture::Full,
        limit: witness::WitnessLimit::Frames(5),
        locked_random: 7,
        repro,
        ..witness::WitnessOptions::default()
    })?;
    Ok(())
}
