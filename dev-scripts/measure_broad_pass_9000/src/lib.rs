#[rsvz::script]
fn script() {
    use rsvz::dsl::prelude::*;

    reload(MainUiOrFightUi);
    set_zombies("普", Natural);
    select_cards("P");
    rsvz::measure::completed_rounds(63);
    rsvz::measure::end_at((1, 9000));
    rsvz::measure::broad_pass_trials(2);
}
