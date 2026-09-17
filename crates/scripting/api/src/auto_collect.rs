//! Automatic item collection.

pub use rsvz_game::auto_collect::{
    AutoCollectMode, ItemCollector, ItemCollectorConfigError, ItemCollectorError, click, mode, normal, off, reset,
    set_allow_cursor_side_effects, set_interval, set_mode, set_play_sound, set_type_list, set_types,
};
pub use rsvz_game::auto_collect::{register_click_tick, register_tick, tick, tick_click};
