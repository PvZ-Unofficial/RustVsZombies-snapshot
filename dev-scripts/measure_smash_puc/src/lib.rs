#[rsvz::script]
fn script() {
    use rsvz::dsl::prelude::*;

    reload(MainUiOrFightUi);
    skip_seed_chooser();
    lineup("LI75nGVZVFRUluStI9RIyMWkNNRJb94MuhtGbQNSl1U=");
    set_zombies([ZombieKind::GigaGargantuar], Exact);
    select_cards("U");
    set_sun_cost_ignored(true);
    rsvz::measure::completed_rounds(63);
    rsvz::measure::protect_only([
        rsvz::measure::protect::grid(1, 8),
        rsvz::measure::protect::grid(2, 8),
        rsvz::measure::protect::grid(3, 8),
        rsvz::measure::protect::grid(4, 8),
        rsvz::measure::protect::grid(5, 8),
    ]);

    wave(1);
    (-599) << auto_cobs();
    225 << p([(2, 9.0), (5, 9.0)]);
    580 << try_act(|| {
        let mut giga_row = Option::<i32>::None;
        rsvz::zombie::for_each_zombie(|zombie| {
            if zombie.kind == ZombieKind::GigaGargantuar {
                giga_row = Some(zombie.row + 1);
            }
        });

        let row = giga_row.ok_or_else(|| rsvz::runtime::RuntimeError::new("missing giga gargantuar"))?;
        rsvz::cards::try_card(PlantKind::UmbrellaLeaf, row, 9)
            .map(|_| ())
            .map_err(|error| rsvz::runtime::RuntimeError::new(error.to_string()))
    });

    skip_until((1, 1300));
    rsvz::measure::end_at((1, 1300));
    rsvz::measure::smash_trials(3000);
}
