fn runtime_error(error: impl std::fmt::Display) -> rsvz::runtime::RuntimeError {
    rsvz::runtime::RuntimeError::new(error.to_string())
}

#[rsvz::state_hooks]
fn install_hooks() {
    use rsvz::prelude::{
        CardSelection, Grid, PlantCreateBackend as _, PlantKind, ZombieCreateBackend as _, ZombieKind, fail_script,
        on_enter_fight,
    };

    on_enter_fight(|| {
        let result = rsvz::with_backend(|backend| -> rsvz::runtime::RuntimeResult<()> {
            backend
                .new_plant(
                    CardSelection::Plant(PlantKind::WallNut)
                        .checked()
                        .expect("WallNut is a valid card selection"),
                    Grid::new(2, 1).expect("R3C2 is a valid grid"),
                )
                .map_err(runtime_error)?;
            backend
                .add_zombie_in_row(ZombieKind::Digger, 2, 0)
                .map_err(runtime_error)?
                .ok_or_else(|| rsvz::runtime::RuntimeError::new("failed to create the native-position Digger"))?;
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
    set_zombies("矿");
    set_wave_zombies(1, "矿");
    select_cards(
        "PINIJ",
        [
            PlantKind::FlowerPot,
            PlantKind::SplitPea,
            PlantKind::FumeShroom,
            PlantKind::Starfruit,
            PlantKind::GraveBuster,
        ],
    );

    let mut repro = witness::WitnessRepro {
        case_id: "digger-native-position-full-cycle-vs-wallnut-r3c2-frames4000".to_owned(),
        scenario_seed: Some(0x5eed_1051),
        script_id: "witness-digger-full-cycle-v1".to_owned(),
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
                "plants": [{"kind": "wall_nut", "row": 3, "col": 2, "hp": 4000}],
                "zombies": [{"kind": "digger", "row": 3, "x": "native", "from_wave": 0}],
                "expected_phases": ["tunneling", "drill", "rising", "dizzy", "walk_right", "eat"],
                "spawn": "stopped-before-frame-0"
            }"#,
        )
        .expect("static Witness setup JSON must be valid");

    witness::start(witness::WitnessOptions {
        locked_random: 7,
        capture: witness::WitnessCapture::Full,
        limit: witness::WitnessLimit::Frames(4000),
        repro,
        ..witness::WitnessOptions::default()
    })?;
    Ok(())
}
