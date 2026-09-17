#[rsvz::script]
fn script() {
    use rsvz::dsl::prelude::*;

    reload(MainUiOrFightUi);
    skip_seed_chooser();
    lineup("LI75nGVZVFRUluStI9RIyMWkNNRJb94MuhtGbQNSl1U=");
    set_zombies(
        [
            ZombieKind::JackInTheBox,
            ZombieKind::JackInTheBox,
            ZombieKind::JackInTheBox,
            ZombieKind::JackInTheBox,
            ZombieKind::JackInTheBox,
            ZombieKind::Ladder,
            ZombieKind::Ladder,
            ZombieKind::Ladder,
            ZombieKind::Ladder,
            ZombieKind::Ladder,
            ZombieKind::Football,
            ZombieKind::Football,
            ZombieKind::Football,
            ZombieKind::Football,
            ZombieKind::Football,
            ZombieKind::Catapult,
            ZombieKind::Catapult,
            ZombieKind::Catapult,
            ZombieKind::Catapult,
            ZombieKind::Catapult,
        ],
        Exact,
    );
    select_cards("I");
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
    // W1 zombies are created during the native update that reaches time 0.
    // Freeze at 1 so the effect cannot run before those same-frame spawns.
    1 << i(3, 3);

    skip_until((1, 1500));
    rsvz::measure::end_at((1, 1500));
    rsvz::measure::damage_narrow_trials(10_000);
}
