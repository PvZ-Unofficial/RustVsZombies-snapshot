//! Physical 1051 patch, native-loop, and unload infrastructure.

mod control;
pub(crate) mod dispatch_entry;
mod lifecycle;
mod native_loop;
mod patch;
pub(crate) mod profiler;

pub(crate) use control::prepare_world_reset;
pub(crate) use control::restore_game_speed;
pub(crate) use control::{
    advanced_pause_active, fast_forward_active, request_seed_chooser_fast_forward, set_advanced_pause, set_game_speed,
    start_fast_forward, stop_fast_forward,
};
pub(crate) use dispatch_entry::install_dispatch_entry;
pub(crate) use lifecycle::fail_and_request_unload;
pub(crate) use lifecycle::mark_unowned_initialization_failure;
pub(crate) use lifecycle::{
    initialize, mark_runner_cleanup_complete, prepare_runner_cleanup, request_unload, request_unload_and_wait,
    unload_requested,
};

/// Displays a recoverable user-script error outside loader lock.
pub(crate) fn report_runtime_error(message: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};

    const TITLE: [u16; 17] = [
        82, 117, 115, 116, 86, 115, 90, 111, 109, 98, 105, 101, 115, 32, 38_169, 35_823, 0,
    ];
    let message: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: both UTF-16 buffers are NUL-terminated for the synchronous call.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            TITLE.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}
