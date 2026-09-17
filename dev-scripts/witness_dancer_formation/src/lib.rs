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
            for (row, col, kind) in [(2, 0, PlantKind::WallNut), (1, 2, PlantKind::GatlingPea)] {
                backend
                    .new_plant(
                        CardSelection::Plant(kind)
                            .checked()
                            .expect("ordinary plants are valid card selections"),
                        Grid::new(row, col).expect("fixed Dancer test grid is valid"),
                    )
                    .map_err(runtime_error)?;
            }

            let dancer = backend
                .add_zombie_in_row(ZombieKind::Dancing, 2, 0)
                .map_err(runtime_error)?
                .ok_or_else(|| rsvz::runtime::RuntimeError::new("failed to create the fixed Dancing Zombie"))?;
            if backend
                .set_zombie_x(
                    dancer,
                    I32RepresentableF32::new(750.0).expect("750 is exactly representable as f32"),
                )
                .map_err(runtime_error)?
                != ObjectEditOutcome::Applied
            {
                return Err(rsvz::runtime::RuntimeError::new(
                    "failed to set the fixed Dancing Zombie position",
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
        case_id: "dancing-zombie-four-backups-upper-kill-and-resummon-frames5000".to_owned(),
        scenario_seed: Some(0x5eed_1051),
        script_id: "witness-dancer-formation-v1".to_owned(),
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
                    {"kind": "wall_nut", "row": 3, "col": 1, "role": "non_attacking_guard"},
                    {"kind": "gatling_pea", "row": 2, "col": 3, "role": "upper_backup_only"}
                ],
                "zombies": [{"kind": "dancing", "row": 3, "x": 750.0, "from_wave": 0}],
                "expected": ["moonwalk", "first_four_backup_summon", "master_follower_links", "upper_backup_death", "upper_backup_resummon"],
                "spawn": "stopped-before-frame-0"
            }"#,
        )
        .expect("static Witness setup JSON must be valid");

    witness::start(witness::WitnessOptions {
        locked_random: 7,
        capture: witness::WitnessCapture::Full,
        limit: witness::WitnessLimit::Frames(5000),
        repro,
        ..witness::WitnessOptions::default()
    })?;
    Ok(())
}
