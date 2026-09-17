fn runtime_error(error: impl std::fmt::Display) -> rsvz::runtime::RuntimeError {
    rsvz::runtime::RuntimeError::new(error.to_string())
}

#[rsvz::state_hooks]
fn install_hooks() {
    use rsvz::core::model::I32RepresentableF32;
    use rsvz::prelude::{
        CardSelection, Grid, ObjectEditOutcome, PlantCreateBackend as _, PlantKind, ZombieCreateBackend as _,
        ZombieKind, ZombiePositionWriteBackend as _, ZombieXWriteBackend as _, fail_script, on_enter_fight,
    };

    on_enter_fight(|| {
        let result = rsvz::with_backend(|backend| -> rsvz::runtime::RuntimeResult<()> {
            for (col, kind) in [(4, PlantKind::WallNut), (3, PlantKind::UmbrellaLeaf)] {
                backend
                    .new_plant(
                        CardSelection::Plant(kind)
                            .checked()
                            .expect("ordinary plants are valid card selections"),
                        Grid::new(2, col).expect("fixed R3 Bungee test grid is valid"),
                    )
                    .map_err(runtime_error)?;
            }

            let unprotected = backend
                .place_zombie(
                    ZombieKind::Bungee,
                    Grid::new(0, 7).expect("R1C8 is a valid Bungee target"),
                )
                .map_err(runtime_error)?;
            if backend
                .set_zombie_x(
                    unprotected,
                    I32RepresentableF32::new(600.0).expect("R1C8 x is exactly representable as f32"),
                )
                .map_err(runtime_error)?
                != ObjectEditOutcome::Applied
            {
                return Err(rsvz::runtime::RuntimeError::new(
                    "failed to normalize the unprotected Bungee x cache",
                ));
            }
            backend
                .set_zombie_row_and_y(
                    unprotected,
                    0,
                    I32RepresentableF32::new(50.0).expect("R1 y is exactly representable as f32"),
                )
                .map_err(runtime_error)?;
            let protected = backend
                .place_zombie(
                    ZombieKind::Bungee,
                    Grid::new(2, 4).expect("R3C5 is a valid Bungee target"),
                )
                .map_err(runtime_error)?;
            if backend
                .set_zombie_x(
                    protected,
                    I32RepresentableF32::new(360.0).expect("R3C5 x is exactly representable as f32"),
                )
                .map_err(runtime_error)?
                != ObjectEditOutcome::Applied
            {
                return Err(rsvz::runtime::RuntimeError::new(
                    "failed to normalize the protected Bungee x cache",
                ));
            }
            backend
                .set_zombie_row_and_y(
                    protected,
                    2,
                    I32RepresentableF32::new(250.0).expect("R3 y is exactly representable as f32"),
                )
                .map_err(runtime_error)?;

            for kind in [PlantKind::WallNut, PlantKind::Pumpkin] {
                backend
                    .new_plant(
                        CardSelection::Plant(kind)
                            .checked()
                            .expect("ordinary plants are valid card selections"),
                        Grid::new(0, 7).expect("R1C8 is a valid layered plant grid"),
                    )
                    .map_err(runtime_error)?;
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

    rsvz::with_backend(|backend| -> rsvz::runtime::RuntimeResult<()> {
        backend.set_zombie_spawn_stopped(true).map_err(runtime_error)?;
        backend.set_special_events_disabled(true).map_err(runtime_error)?;
        Ok(())
    })?;
    lineup("LMg3NPRBVFRUDlRVVQ==");
    set_zombies("普");
    set_wave_zombies(1, "普");
    select_cards(
        "PUNIJ",
        [
            PlantKind::FlowerPot,
            PlantKind::SplitPea,
            PlantKind::FumeShroom,
            PlantKind::Starfruit,
            PlantKind::GraveBuster,
        ],
    );

    let mut repro = witness::WitnessRepro {
        case_id: "two-bungees-layered-pumpkin-and-umbrella-protection-frames2500".to_owned(),
        scenario_seed: Some(0x5eed_1051),
        script_id: "witness-bungee-layers-v1".to_owned(),
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
                    {"kind": "wall_nut", "row": 1, "col": 8, "layer": "content", "expect": "stolen"},
                    {"kind": "pumpkin", "row": 1, "col": 8, "layer": "pumpkin", "expect": "retained"},
                    {"kind": "wall_nut", "row": 3, "col": 5, "expect": "umbrella_protected"},
                    {"kind": "umbrella_leaf", "row": 3, "col": 4}
                ],
                "zombies": [
                    {"kind": "bungee", "target": {"row": 1, "col": 8}, "placement": "place_zombie", "position_cache": "normalized_to_target"},
                    {"kind": "bungee", "target": {"row": 3, "col": 5}, "placement": "place_zombie", "position_cache": "normalized_to_target"}
                ],
                "spawn": "stopped-before-frame-0",
                "special_events": "disabled"
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
