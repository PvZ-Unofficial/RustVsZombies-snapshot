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
            for (row, kind) in [
                (0, PlantKind::WallNut),
                (1, PlantKind::WallNut),
                (2, PlantKind::WallNut),
                (3, PlantKind::Spikeweed),
                (4, PlantKind::WallNut),
            ] {
                let selection = CardSelection::Plant(kind)
                    .checked()
                    .expect("contact plants are valid card selections");
                let grid = Grid::new(row, 4).expect("R1C5 through R5C5 are valid grids");
                backend.new_plant(selection, grid).map_err(runtime_error)?;
            }

            let x = I32RepresentableF32::new(650.0).expect("650 is exactly representable as f32");
            for (row, kind) in [
                (0, ZombieKind::Football),
                (1, ZombieKind::PoleVaulting),
                (2, ZombieKind::Ladder),
                (3, ZombieKind::Zomboni),
                (4, ZombieKind::Gargantuar),
            ] {
                let zombie = backend
                    .add_zombie_in_row(kind, row, 0)
                    .map_err(runtime_error)?
                    .ok_or_else(|| rsvz::runtime::RuntimeError::new("failed to create a fixed contact zombie"))?;
                if backend.set_zombie_x(zombie, x).map_err(runtime_error)? != ObjectEditOutcome::Applied {
                    return Err(rsvz::runtime::RuntimeError::new(
                        "failed to set a fixed contact zombie position",
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
        case_id: "five-row-contact-state-machines-x650-frames2500".to_owned(),
        scenario_seed: Some(0x5eed_1051),
        script_id: "witness-contact-state-machines-v1".to_owned(),
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
                    {"kind": "wall_nut", "row": 1, "col": 5},
                    {"kind": "wall_nut", "row": 2, "col": 5},
                    {"kind": "wall_nut", "row": 3, "col": 5},
                    {"kind": "spikeweed", "row": 4, "col": 5},
                    {"kind": "wall_nut", "row": 5, "col": 5}
                ],
                "zombies": [
                    {"kind": "football", "row": 1, "x": 650.0, "from_wave": 0},
                    {"kind": "pole_vaulting", "row": 2, "x": 650.0, "from_wave": 0},
                    {"kind": "ladder", "row": 3, "x": 650.0, "from_wave": 0},
                    {"kind": "zomboni", "row": 4, "x": 650.0, "from_wave": 0},
                    {"kind": "gargantuar", "row": 5, "x": 650.0, "from_wave": 0}
                ],
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
