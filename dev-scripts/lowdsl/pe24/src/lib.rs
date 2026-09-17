#[rustfmt::skip]
#[rsvz::script]
fn script() {
    use rsvz::core::runtime::{RuntimeError};
    use rsvz::prelude::SunCostRuleEditBackend;

    reload(MainUiOrFightUi);
    // rsvz::setup::set_game_speed(3.0);
    // rsvz::setup::skip_seed_chooser();
    // rsvz::setup::skip_until((20, 100_000));
    rsvz::bench::start(std::time::Duration::from_secs(120));
    lineup("LI43bJyUlNTYBS00RdPXWnxsNHHS3FlXRbJUVHQGQ8pW");
    rsvz::with_backend(|backend| backend.set_sun_cost_ignored(true))
        .map_err(|error| RuntimeError::new(error.to_string()))?;

    set_zombies("普杆车豚丑矿梯偷跳舞白红");
    select_cards("IIKAWPCCCC");

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
}
