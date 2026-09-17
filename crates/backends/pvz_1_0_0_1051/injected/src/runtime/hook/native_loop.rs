use std::panic::{self, AssertUnwindSafe};
use std::ptr;

use crate::error::Result;
use crate::raw::abi as asm;
use crate::runtime::fast_forward::{FastForwardLoopDecision, SeedChooserFastForwardLoopDecision};

use super::{control, lifecycle, profiler};

/// Native PvZ main-loop trampoline stored in the 0x667bc0 slot.
///
/// The physical order is deliberately visible here: runner dispatch, actual
/// native update, then the same pair for every fast-forward update.
pub(super) extern "C" fn script_hook() {
    if panic::catch_unwind(AssertUnwindSafe(script_hook_inner)).is_err() {
        lifecycle::fail_and_request_unload();
    }
}

fn script_hook_inner() {
    let _active = lifecycle::ActiveHookGuard::enter();

    if control::ensure_native_observers().is_err() {
        lifecycle::fail_and_request_unload();
        call_original_update();
        return;
    }
    match dispatch_catching() {
        crate::host::NativeUpdate::Continue => {
            if lifecycle::unload_requested() {
                lifecycle::drive_unload_from_game_thread();
                return;
            }
        }
        crate::host::NativeUpdate::Skip => {
            if lifecycle::unload_requested() {
                lifecycle::drive_unload_from_game_thread();
            }
            return;
        }
    }

    call_original_update();
    if control::update_advanced_pause_frame().is_err() {
        lifecycle::fail_and_request_unload();
        return;
    }

    if run_seed_chooser_fast_forward()
        .and_then(|()| run_battle_fast_forward())
        .is_err()
    {
        lifecycle::fail_and_request_unload();
    }
}

fn dispatch_catching() -> crate::host::NativeUpdate {
    panic::catch_unwind(AssertUnwindSafe(crate::host::dispatch_frame)).unwrap_or_else(|_| {
        lifecycle::fail_and_request_unload();
        crate::host::NativeUpdate::Skip
    })
}

fn call_original_update() {
    let _update_rate = NativeUpdateRateGuard::single_frame();
    crate::runtime::imitator_morph::begin_update_batch();
    // SAFETY: this is the verified original LawnApp::UpdateFrames entry at
    // 0x452650, called directly rather than through the patched slot.
    unsafe { asm::lawn_app_update_frames() };
}

const SLOW_MO_ADDR: usize = 0x6a9eaa;
const FAST_MO_ADDR: usize = 0x6a9eab;

struct NativeUpdateRateGuard {
    slow: *mut u8,
    fast: *mut u8,
    saved_slow: u8,
    saved_fast: u8,
}

impl NativeUpdateRateGuard {
    fn single_frame() -> Self {
        // SAFETY: these are the verified writable gSlowMo/gFastMo bytes in PvZ
        // 1.0.0.1051. The native runner owns their update-rate effect while it
        // maps one host dispatch to one LawnApp::UpdateFrames update.
        unsafe { Self::at(SLOW_MO_ADDR as *mut u8, FAST_MO_ADDR as *mut u8) }
    }

    unsafe fn at(slow: *mut u8, fast: *mut u8) -> Self {
        // SAFETY: the caller guarantees both pointers remain readable and
        // writable until this guard is dropped.
        let saved_slow = unsafe { ptr::read_volatile(slow) };
        // SAFETY: same pointer contract as above.
        let saved_fast = unsafe { ptr::read_volatile(fast) };
        // SAFETY: the caller granted exclusive game-thread ownership here.
        unsafe {
            ptr::write_volatile(slow, 0);
            ptr::write_volatile(fast, 0);
        }
        Self {
            slow,
            fast,
            saved_slow,
            saved_fast,
        }
    }
}

impl Drop for NativeUpdateRateGuard {
    fn drop(&mut self) {
        // SAFETY: `at` requires both pointers to outlive the guard.
        unsafe {
            ptr::write_volatile(self.slow, self.saved_slow);
            ptr::write_volatile(self.fast, self.saved_fast);
        }
    }
}

fn run_seed_chooser_fast_forward() -> Result<()> {
    loop {
        if !matches!(
            control::seed_chooser_before_update()?,
            SeedChooserFastForwardLoopDecision::Continue
        ) {
            return Ok(());
        };

        match dispatch_catching() {
            crate::host::NativeUpdate::Continue => {}
            crate::host::NativeUpdate::Skip => {
                if lifecycle::unload_requested() {
                    lifecycle::drive_unload_from_game_thread();
                }
                return Ok(());
            }
        }
        if lifecycle::unload_requested() {
            lifecycle::drive_unload_from_game_thread();
            return Ok(());
        }
        if !matches!(
            control::advance_seed_chooser_update()?,
            SeedChooserFastForwardLoopDecision::Continue
        ) {
            return Ok(());
        };
    }
}

fn run_battle_fast_forward() -> Result<()> {
    loop {
        let continue_loop = profiler::measure_loop_iteration(|| -> Result<bool> {
            if !matches!(
                control::fast_forward_before_update()?,
                FastForwardLoopDecision::Continue
            ) {
                return Ok(false);
            }

            if profiler::measure_run_total(dispatch_catching) == crate::host::NativeUpdate::Skip
                || lifecycle::unload_requested()
            {
                lifecycle::drive_unload_from_game_thread();
                return Ok(false);
            }
            Ok(matches!(
                control::advance_fast_forward_update()?,
                FastForwardLoopDecision::Continue
            ))
        })?;
        if !continue_loop {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_update_rate_guard_forces_one_update_and_restores_flags() {
        for (saved_slow, saved_fast) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            let mut slow = saved_slow;
            let mut fast = saved_fast;
            {
                // SAFETY: both local bytes outlive the guard and are exclusively borrowed.
                let _guard = unsafe { NativeUpdateRateGuard::at(&raw mut slow, &raw mut fast) };
                assert_eq!((slow, fast), (0, 0));
            }
            assert_eq!((slow, fast), (saved_slow, saved_fast));
        }
    }
}
