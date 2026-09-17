fn runtime_error(error: impl std::fmt::Display) -> rsvz::runtime::RuntimeError {
    rsvz::runtime::RuntimeError::new(error.to_string())
}

#[rsvz::state_hooks]
fn install_hooks() {
    use rsvz::prelude::{
        CardSelection, Grid, PlantCreateBackend as _, PlantKind, ZombieCreateBackend as _, ZombieKind,
    };
    use rsvz::prelude::{fail_script, on_enter_fight};

    on_enter_fight(|| {
        let result = rsvz::core::modifier::stabilize_natural_sun_drop().and_then(|()| rsvz::with_backend(|backend| -> rsvz::runtime::RuntimeResult<()> {
            for (row, col) in [(2, 5), (2, 2), (3, 4)] {
                for kind in [PlantKind::LilyPad, PlantKind::WallNut] {
                    let selection = CardSelection::Plant(kind)
                        .checked()
                        .expect("ordinary plants are valid card selections");
                    let grid = Grid::new(row, col).expect("fixed pool plant grid is valid");
                    backend.new_plant(selection, grid).map_err(runtime_error)?;
                }
            }

            backend
                .add_zombie_in_row(ZombieKind::DolphinRider, 2, 0)
                .map_err(runtime_error)?
                .ok_or_else(|| rsvz::runtime::RuntimeError::new("failed to create fixed DolphinRider"))?;
            backend
                .add_zombie_in_row(ZombieKind::Snorkel, 3, 0)
                .map_err(runtime_error)?
                .ok_or_else(|| rsvz::runtime::RuntimeError::new("failed to create fixed Snorkel"))?;
            Ok(())
        }));
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
    lineup("LI43NPRLVFRUOFRVVg==");
    set_zombies("潜豚");
    set_wave_zombies(1, "潜豚");
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
        case_id: "pool-dolphin-r3-snorkel-r4-basic-state-machines-frames3500".to_owned(),
        scenario_seed: Some(0x5eed_1051),
        script_id: "witness-pool-dolphin-snorkel-basics-v1".to_owned(),
        build_id: env!("CARGO_PKG_VERSION").to_owned(),
        ..witness::WitnessRepro::default()
    };
    repro
        .set_expanded_setup_json(
            r#"{
                "scene": "pool",
                "completed_rounds": 63,
                "initial_sun": 8000,
                "locked_random": 7,
                "natural_sun": "disabled-stable-52-425",
                "plants": [
                    {"kind": "lily_pad", "row": 3, "col": 6},
                    {"kind": "wall_nut", "row": 3, "col": 6},
                    {"kind": "lily_pad", "row": 3, "col": 3},
                    {"kind": "wall_nut", "row": 3, "col": 3},
                    {"kind": "lily_pad", "row": 4, "col": 5},
                    {"kind": "wall_nut", "row": 4, "col": 5}
                ],
                "zombies": [
                    {"kind": "dolphin_rider", "row": 3, "x": "native", "from_wave": 0},
                    {"kind": "snorkel", "row": 4, "x": "native", "from_wave": 0}
                ],
                "spawn": "stopped-before-frame-0"
            }"#,
        )
        .expect("static Witness setup JSON must be valid");

    witness::start(witness::WitnessOptions {
        locked_random: 7,
        capture: witness::WitnessCapture::Full,
        limit: witness::WitnessLimit::Frames(3500),
        repro,
        ..witness::WitnessOptions::default()
    })?;
    Ok(())
}
