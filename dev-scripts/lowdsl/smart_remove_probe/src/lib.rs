#[rsvz::script]
fn script() {
    reload(MainUiOrFightUi);
    rsvz::setup::set_game_speed(1.0);
    set_zombies("红白车篮");
    select_cards([
        pumpkin, tallnut, puff, sunflower, pot, lily, ice, doom, cherry, jalapeno,
    ]);

    rsvz::try_at(1, 1, || {
        rsvz::smart_remove::start()?;
        Ok(())
    })?;

    rsvz::try_at(1, 50, || {
        rsvz::smart_remove::set_highlight(false);
        Ok(())
    })?;
}
