use std::ptr::NonNull;

use crate::raw::layout as ptrs;

/// Reads PvZ's global frame tick duration.
pub(crate) unsafe fn tick_ms(app: NonNull<ptrs::LawnApp>) -> i32 {
    // SAFETY: the caller supplies the current LawnApp.
    unsafe { ptrs::LawnApp::tick_ms(app.as_ptr()) }
}

/// Writes PvZ's global frame tick duration.
pub(crate) unsafe fn set_tick_ms(app: NonNull<ptrs::LawnApp>, ms: i32) {
    // SAFETY: the caller supplies the current LawnApp.
    unsafe { ptrs::LawnApp::set_tick_ms(app.as_ptr(), ms) }
}

/// Reads PvZ's global update multiplier.
pub(crate) unsafe fn update_multiplier(app: NonNull<ptrs::LawnApp>) -> f64 {
    // SAFETY: the caller supplies the current LawnApp.
    unsafe { ptrs::LawnApp::update_multiplier(app.as_ptr()) }
}

/// Writes PvZ's global update multiplier.
pub(crate) unsafe fn set_update_multiplier(app: NonNull<ptrs::LawnApp>, multiplier: f64) {
    // SAFETY: the caller supplies the current LawnApp.
    unsafe { ptrs::LawnApp::set_update_multiplier(app.as_ptr(), multiplier) }
}
