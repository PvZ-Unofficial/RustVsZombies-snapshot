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
        let result = rsvz::core::modifier::stabilize_natural_sun_drop().and_then(|()| rsvz::with_backend(|backend| -> rsvz::runtime::RuntimeResult<()> {

            for (row, col, kinds) in [
                (0, 8, [Some(PlantKind::WallNut), None]),
                (1, 4, [Some(PlantKind::WallNut), None]),
                (2, 7, [Some(PlantKind::LilyPad), Some(PlantKind::WallNut)]),
            ] {
                for kind in kinds.into_iter().flatten() {
                    backend
                        .new_plant(
                            CardSelection::Plant(kind)
                                .checked()
                                .expect("ordinary plants are valid card selections"),
                            Grid::new(row, col).expect("fixed phase-test plant grid is valid"),
                        )
                        .map_err(runtime_error)?;
                }
            }

            for (row, kind, x) in [
                (0, ZombieKind::PoleVaulting, 890.0),
                (1, ZombieKind::Pogo, 1000.0),
                (2, ZombieKind::DolphinRider, 720.0),
                (3, ZombieKind::Snorkel, 850.0),
                (4, ZombieKind::Digger, 2000.0),
                (4, ZombieKind::Zomboni, 1000.0),
                (5, ZombieKind::Balloon, 1000.0),
            ] {
                let zombie = backend
                    .add_zombie_in_row(kind, row, 0)
                    .map_err(runtime_error)?
                    .ok_or_else(|| rsvz::runtime::RuntimeError::new("failed to create a phase-test zombie"))?;
                let x = I32RepresentableF32::new(x).expect("integer zombie x is exactly representable as f32");
                if backend.set_zombie_x(zombie, x).map_err(runtime_error)? != ObjectEditOutcome::Applied {
                    return Err(rsvz::runtime::RuntimeError::new(
                        "failed to set a phase-test zombie position",
                    ));
                }
            }
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

    rsvz::with_backend(|backend| -> rsvz::runtime::RuntimeResult<()> {
        backend.set_zombie_spawn_stopped(true).map_err(runtime_error)?;
        backend.set_mushrooms_awake(true).map_err(runtime_error)?;
        Ok(())
    })?;
    lineup("LI43NPRLVFRUOFRVVg==");
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

    let _ice_task = rsvz::tick::spawn(rsvz::tick::TickOptions::playing_frame(), |meta| {
        if meta.clock != Some(350) {
            return Ok(rsvz::tick::TickControl::Continue);
        }
        rsvz::core::logic::cards::new_plant(PlantKind::IceShroom, Grid::new(5, 0).unwrap())
            .map_err(runtime_error)?;
        Ok(rsvz::tick::TickControl::Stop)
    });

    let mut repro = witness::WitnessRepro {
        case_id: "pool-ice-shroom-phase-immunity-seven-fixed-zombies-frames1000".to_owned(),
        scenario_seed: Some(0x5eed_1051),
        script_id: "witness-ice-phase-immunity-v1".to_owned(),
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
                "mushrooms": "awake",
                "ice_shroom": {"row": 6, "col": 1, "created_at_frame": 350, "expected_effect_frame": 450},
                "plants": [
                    {"kind": "wall_nut", "row": 1, "col": 9},
                    {"kind": "wall_nut", "row": 2, "col": 5},
                    {"kind": "lily_pad", "row": 3, "col": 8},
                    {"kind": "wall_nut", "row": 3, "col": 8},
                    {"kind": "ice_shroom", "row": 6, "col": 1, "created_at_frame": 350}
                ],
                "zombies": [
                    {"kind": "pole_vaulter", "row": 1, "x": 890.0, "expect_at_ice": "in_vault"},
                    {"kind": "pogo", "row": 2, "x": 1000.0, "expect_at_ice": "bouncing_with_stick"},
                    {"kind": "dolphin_rider", "row": 3, "x": 720.0, "expect_at_ice": "dolphin_jump"},
                    {"kind": "snorkel", "row": 4, "x": 850.0, "expect_at_ice": "entering_pool"},
                    {"kind": "digger", "row": 5, "x": 2000.0, "expect_at_ice": "underground"},
                    {"kind": "zomboni", "row": 5, "x": 1000.0, "expect_at_ice": "driving"},
                    {"kind": "balloon", "row": 6, "x": 1000.0, "expect_at_ice": "flying"}
                ],
                "spawn": "stopped-before-frame-0"
            }"#,
        )
        .expect("static Witness setup JSON must be valid");

    witness::start(witness::WitnessOptions {
        locked_random: 7,
        capture: witness::WitnessCapture::Full,
        limit: witness::WitnessLimit::Frames(1000),
        repro,
        ..witness::WitnessOptions::default()
    })?;
    Ok(())
}
