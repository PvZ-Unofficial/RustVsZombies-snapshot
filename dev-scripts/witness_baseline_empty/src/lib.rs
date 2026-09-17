#[rsvz::state_hooks]
fn install_hooks() {
    use rsvz::prelude::{ZombieRuleEditBackend as _, fail_script, on_before_script};

    on_before_script(|| {
        if let Err(error) = rsvz::with_backend(|backend| backend.set_zombie_spawn_stopped(true)) {
            fail_script(rsvz::runtime::RuntimeError::new(error.to_string()));
        }
    });
}

#[rsvz::script]
fn script() -> rsvz::runtime::RuntimeResult<()> {
    use rsvz::prelude::*;
    use rsvz_witness as witness;

    // Freeze every source of baseline variation before Witness takes frame 0.
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
        case_id: "baseline-empty-night-frames-0-3".to_owned(),
        scenario_seed: Some(0x5eed_1051),
        script_id: "witness-baseline-empty-v2".to_owned(),
        build_id: env!("CARGO_PKG_VERSION").to_owned(),
        ..witness::WitnessRepro::default()
    };
    repro
        .set_expanded_setup_json(
            r#"{
                "scene": "night",
                "completed_rounds": 63,
                "initial_sun": 8000,
                "plants": [],
                "zombies": [],
                "spawn": "stopped-before-frame-0"
            }"#,
        )
        .expect("static Witness setup JSON must be valid");

    witness::start(witness::WitnessOptions {
        locked_random: 0x5eed_1051,
        capture: witness::WitnessCapture::Full,
        limit: witness::WitnessLimit::Frames(3),
        repro,
        ..witness::WitnessOptions::default()
    })?;
    Ok(())
}
