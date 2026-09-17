#[rsvz::script]
fn script() {
    use rsvz::dsl::prelude::*;

    reload(MainUiOrFightUi);
    skip_seed_chooser();
    set_zombies("普", Natural);
    select_cards("P");
    // PE requires at least 126 flags, i.e. 63 completed endless rounds.
    rsvz::measure::completed_rounds(63);
    assume_wavelength(1, 601);
    skip_until((1, 402));
    rsvz::measure::end_at((1, 402));
    rsvz::measure::refresh_trials(2);
}
