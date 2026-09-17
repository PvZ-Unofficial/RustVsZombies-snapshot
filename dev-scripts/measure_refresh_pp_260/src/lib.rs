#[rsvz::script]
fn script() {
    use rsvz::dsl::prelude::*;

    reload(MainUiOrFightUi);
    lineup("LI7hmGVVVFRU1mSu4+bNVewN1GV+zlsJvFXXVg==");
    set_zombies(
        random_zombie_types([ZombieKind::Gargantuar, ZombieKind::GigaGargantuar], []),
        Natural,
    );
    select_cards("P");
    rsvz::measure::completed_rounds(63);
    rsvz::measure::refresh_activate(true);
    assume_wavelength(1, 601);

    wave(1);
    (-599) << auto_cobs();
    260 << pp();

    rsvz::measure::end_at((1, 402));
    rsvz::measure::refresh_trials(5000);
}
