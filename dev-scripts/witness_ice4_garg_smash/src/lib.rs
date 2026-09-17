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
            backend
                .new_plant(
                    CardSelection::Plant(PlantKind::WallNut)
                        .checked()
                        .expect("WallNut is a valid card selection"),
                    Grid::new(2, 4).expect("R3C5 is a valid grid"),
                )
                .map_err(runtime_error)?;

            let gargantuar = backend
                .add_zombie_in_row(ZombieKind::Gargantuar, 2, 0)
                .map_err(runtime_error)?
                .ok_or_else(|| rsvz::runtime::RuntimeError::new("failed to create the fixed Gargantuar"))?;
            if backend
                .set_zombie_x(
                    gargantuar,
                    I32RepresentableF32::new(430.0).expect("430 is exactly representable as f32"),
                )
                .map_err(runtime_error)?
                != ObjectEditOutcome::Applied
            {
                return Err(rsvz::runtime::RuntimeError::new(
                    "failed to set the fixed Gargantuar position",
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
    set_zombies("白");
    set_wave_zombies(1, "白");
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

    let _ice_task = rsvz::tick::spawn(rsvz::tick::TickOptions::playing_frame(), |meta| {
        if meta.clock != Some(34) {
            return Ok(rsvz::tick::TickControl::Continue);
        }
        rsvz::core::logic::cards::new_plant(PlantKind::IceShroom, Grid::new(0, 0).unwrap())
            .map_err(runtime_error)?;
        Ok(rsvz::tick::TickControl::Stop)
    });

    let mut repro = witness::WitnessRepro {
        case_id: "ice4-gargantuar-smash-cancel-wallnut-frames1500".to_owned(),
        scenario_seed: Some(0x5eed_1051),
        script_id: "witness-ice4-garg-smash-v1".to_owned(),
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
                    {"kind": "wall_nut", "row": 3, "col": 5},
                    {"kind": "ice_shroom", "row": 1, "col": 1, "created_at_frame": 34, "expected_effect_frame": 134}
                ],
                "zombies": [{"kind": "gargantuar", "row": 3, "x": 430.0}],
                "baseline": {"smash_start_frame": 1, "smash_effect_frame": 134},
                "spawn": "stopped-before-frame-0"
            }"#,
        )
        .expect("static Witness setup JSON must be valid");

    witness::start(witness::WitnessOptions {
        locked_random: 7,
        capture: witness::WitnessCapture::Full,
        limit: witness::WitnessLimit::Frames(1500),
        repro,
        ..witness::WitnessOptions::default()
    })?;
    Ok(())
}
