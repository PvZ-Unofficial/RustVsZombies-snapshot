use rsvz::core::logic::cob::{CobListOrder, CobSequentialMode};
use rsvz::prelude::*;

// This fixture checks source compatibility; execution requires a game scope.
fn main() {
    let wind = CobManager::new();
    for_each_cob_grid(|_| ());
    for_each_cob_grid(&wind, |_| ());
    set_cob_sequential_mode(CobSequentialMode::Time);
    set_cob_sequential_mode(&wind, CobSequentialMode::Space);
    let _ = (set_next_cob_slot(1), set_next_cob_slot(&wind, 1));
    let _ = (set_next_cob((1, 1)), set_next_cob(&wind, (1, 1)));
    let _ = (skip_cobs(1), skip_cobs(&wind, 1));
    erase_cobs_from_list([(1, 1)]);
    erase_cobs_from_list(&wind, [(1, 1)]);
    let _ = (move_cobs_to_list_top([(1, 1)]), move_cobs_to_list_top(&wind, [(1, 1)]));
    let _ = (
        move_cobs_to_list_bottom([(1, 1)]),
        move_cobs_to_list_bottom(&wind, [(1, 1)]),
    );
    let _ = (recover_cob_list(), recover_cob_list(&wind));
    let _ = (usable_cob_list(), usable_cob_list(&wind));
    let _ = (recover_cob(), recover_cob(&wind));
    let _ = (usable_cob(), usable_cob(&wind));
    let _ = (roof_recover_cob_list(9.0), roof_recover_cob_list(&wind, 9.0));
    let _ = (roof_usable_cob_list(9.0), roof_usable_cob_list(&wind, 9.0));
    let _ = (roof_recover_cob(9.0), roof_recover_cob(&wind, 9.0));
    let _ = (roof_usable_cob(9.0), roof_usable_cob(&wind, 9.0));
    let _ = roof_cob_fly_time(1, 9.0);
    let _ = (try_set_cobs([(1, 1)]), try_set_cobs(&wind, [(1, 1)]));
    set_cobs([(1, 1)]);
    set_cobs(&wind, [(1, 1)]);
    let _ = (try_auto_set_cobs(), try_auto_set_cobs(CobListOrder::Horizontal));
    let _ = (
        try_auto_set_cobs(&wind),
        try_auto_set_cobs(&wind, CobListOrder::Vertical),
    );
    auto_set_cobs();
    auto_set_cobs(CobListOrder::Horizontal);
    auto_set_cobs(&wind);
    auto_set_cobs(&wind, CobListOrder::Vertical);
    let _ = (try_recover_fire(1, 9), try_recover_fire([(1, 9)]));
    let _ = (try_recover_fire(&wind, 1, 9), try_recover_fire(&wind, [(1, 9)]));
    recover_fire(1, 9);
    recover_fire([(1, 9)]);
    recover_fire(&wind, 1, 9);
    recover_fire(&wind, [(1, 9)]);
    let _ = (try_raw_fire(1, 1, 1, 9), try_raw_fire([(1, 1, 1, 9)]));
    raw_fire(1, 1, 1, 9);
    raw_fire([(1, 1, 1, 9)]);
    let _ = plant_cob((1, 1));
    let _ = (fix_latest_cob(), fix_latest_cob(&wind));
    let _ = (try_fire(1, 9), try_fire([(1, 9)]));
    let _ = (try_fire(&wind, 1, 9), try_fire(&wind, [(1, 9)]));
    fire(1, 9);
    fire([(1, 9)]);
    fire(&wind, 1, 9);
    fire(&wind, [(1, 9)]);
}
