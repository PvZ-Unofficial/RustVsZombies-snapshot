//! Keyboard subscriptions with compile-time backend capability checks.

pub use rsvz_game::key::{KeyBindError, KeyBindOptions};
pub use rsvz_game::key::{
    on_press, on_press_with_options, on_release, on_release_with_options, try_on_press, try_on_press_unique,
    try_on_press_unique_with_options, try_on_press_with_options, try_on_release, try_on_release_unique,
    try_on_release_unique_with_options, try_on_release_with_options,
};
