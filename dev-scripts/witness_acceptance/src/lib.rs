#[rustfmt::skip]
#[rsvz::script]
fn script() -> rsvz::runtime::RuntimeResult<()> {
    use rsvz::core::runtime::{RuntimeError};
    use rsvz::prelude::*;
    use rsvz_witness as witness;

    reload(MainUiOrFightUi);
    lineup("LI43bJyUlNTYBS00RdPXWnxsNHHS3FlXRbJUVHQGQ8pW");
    rsvz::with_backend(|backend| backend.set_sun_cost_ignored(true))
        .map_err(|error| RuntimeError::new(error.to_string()))?;

    set_zombies("普杆车豚丑矿梯偷跳舞白红");
    set_wave_zombies(1, "普杆车豚丑矿梯跳舞白红");
    select_cards("IIKAWPCCCC");
    rsvz::setup::skip_seed_chooser();
    rsvz::setup::skip_until((20, 10_000));

    assume_wavelength(1..=8, 601);
    assume_wavelength(10..=18, 601);

    rsvz::try_at(1, -599, || {
        rsvz::plant_fixer::start_plant_fixer(
            rsvz::prelude::PlantKind::Pumpkin,
            [(3, 9), (4, 9)],
        )?;
        rsvz::plant_fixer::set_plant_fixer_hp(500)?;
        Ok(())
    })?;

    wave(1);
    (-599) << auto_cobs() + set_ice([(4, 9)]);

    waves([1, 4, 7, 11, 14, 17]);
    359 << pp() + d(107) + p(15, 7.8);

    waves([2, 5, 8, 12, 15, 18]);
    250 << pp() + d(106) + ci();

    waves([3, 6, 9, 10, 13, 16, 19, 20]);
    318 << pp() + pp() + d(110) + p(15, 8.8);

    waves([9, 19, 20]);
    1000 << pp();

    wave(20);
    230 << p(4, 7.5875);

    let mut repro = witness::WitnessRepro {
        case_id: "acceptance-pe24-three-rounds".to_owned(),
        scenario_seed: Some(0x5eed_1051),
        script_id: "witness-acceptance-pe24-v1".to_owned(),
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
                "wave_spawn_random": true,
                "lineup": "PE24",
                "stop": "completed-rounds-3-before-ui-transport",
                "plants": [],
                "zombies": [],
                "spawn": "explicit-average-20-waves"
            }"#,
        )
        .expect("static Witness setup JSON must be valid");

    witness::start(witness::WitnessOptions {
        locked_random: 7,
        wave_spawn_random: true,
        capture: witness::WitnessCapture::Digest,
        limit: witness::WitnessLimit::CompletedRounds(3),
        repro,
        ..witness::WitnessOptions::default()
    })?;
    Ok(())
}
