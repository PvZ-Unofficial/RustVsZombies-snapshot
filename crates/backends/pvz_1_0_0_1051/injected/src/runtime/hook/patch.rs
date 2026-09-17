use std::mem::size_of;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::error::{Pvz1051Error, Result};
use crate::patches::memory::with_writable_range;

const MAIN_LOOP_SLOT: usize = 0x667bc0;
const ORIGINAL_GAME_TOTAL_LOOP: u32 = 0x452650;
static INSTALLED: AtomicBool = AtomicBool::new(false);

pub(super) fn install() -> Result<()> {
    if INSTALLED.load(Ordering::Acquire) {
        return Err(Pvz1051Error::AbiPreconditionFailed(
            "1051 main-loop hook is already installed",
        ));
    }
    let hook = script_hook_ptr();
    let observed = compare_exchange_slot(ORIGINAL_GAME_TOTAL_LOOP, hook)?;
    if observed != ORIGINAL_GAME_TOTAL_LOOP {
        return Err(Pvz1051Error::AbiPreconditionFailed(
            "1051 main-loop slot is owned by an unknown third-party hook",
        ));
    }
    INSTALLED.store(true, Ordering::Release);
    Ok(())
}

pub(super) fn uninstall() -> Result<()> {
    if !INSTALLED.load(Ordering::Acquire) {
        return Ok(());
    }
    let hook = script_hook_ptr();
    let observed = compare_exchange_slot(hook, ORIGINAL_GAME_TOTAL_LOOP)?;
    if observed != hook {
        return Err(Pvz1051Error::AbiPreconditionFailed(
            "1051 main-loop patch ownership was lost before restore",
        ));
    }
    INSTALLED.store(false, Ordering::Release);
    Ok(())
}

pub(super) fn is_installed() -> bool {
    INSTALLED.load(Ordering::Acquire)
}

fn script_hook_ptr() -> u32 {
    super::native_loop::script_hook as *const () as usize as u32
}

fn compare_exchange_slot(expected: u32, replacement: u32) -> Result<u32> {
    with_writable_range(MAIN_LOOP_SLOT, size_of::<u32>(), || {
        // SAFETY: the writable guard covers the known aligned 32-bit function pointer slot.
        unsafe { compare_exchange(expected, replacement) }
    })
}

unsafe fn compare_exchange(expected: u32, replacement: u32) -> u32 {
    // SAFETY: caller holds the writable-slot guard and passes the known
    // aligned 32-bit function-pointer location.
    let slot = unsafe { &*(MAIN_LOOP_SLOT as *const AtomicU32) };
    slot.compare_exchange(expected, replacement, Ordering::AcqRel, Ordering::Acquire)
        .unwrap_or_else(|observed| observed)
}
