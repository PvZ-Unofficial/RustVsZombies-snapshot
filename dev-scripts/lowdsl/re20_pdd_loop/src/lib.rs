#[rustfmt::skip]
#[rsvz::script]
fn script() {
    use rsvz::dsl::prelude::*;

    reload(MainUiOrFightUi);
    lineup("LI4/bIyUhNTQTQQswNxUVCU0d0FR");
    set_zombies("杆车丑梯篮白红跳");
    set_wave_zombies(20, "普杆障车丑梯篮白红跳");
    select_cards("I", [PlantKind::FlowerPot]);
    skip_seed_chooser();
    skip_until((1, -300));

    let l1 = CobManager::new();
    let l5 = CobManager::new();
    let l7 = CobManager::new();

    (1, -599) << auto_cobs_col(&l1, 1)
        + auto_cobs_col(&l5, 5)
        + auto_cobs_col(&l7, 7);

    wave(1);
    335 << p(&l7, 2, 8.5) + p(&l7, 4, 8.5);
    495 << p(&l1, 2, 8.3625) + p(&l5, 4, 8.3625);
    602 << i(3, 9) + shovel(3, 9);
    634 << p(&l1, 2, 8.7) + p(&l5, 4, 8.7125);

    (2, 700) << try_act(|| rsvz::request_world_reset(rsvz::WorldResetConfig {
        card_cooldowns: rsvz::ResetCardCooldowns::Ready,
        ..rsvz::WorldResetConfig::default()
    }));
}
