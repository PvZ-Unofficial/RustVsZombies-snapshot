//! Wave-relative time helper.

use rsvz_model::model::{RelativeTime, Wave};

#[must_use]
pub const fn time(wave: i32, t: i32) -> RelativeTime {
    RelativeTime::new(Wave(wave), t)
}
