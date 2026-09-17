use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::error::{Pvz1051Error, Result};

use super::dispatch_entry;
use super::{control, patch};

const UNLOAD_WAIT_STEPS: usize = 5_000;

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static UNLOAD_REQUESTED: AtomicBool = AtomicBool::new(false);
static RUNNER_CLEANED: AtomicBool = AtomicBool::new(false);
static UNLOAD_FAILED: AtomicBool = AtomicBool::new(false);
static ACTIVE_HOOK_DEPTH: AtomicUsize = AtomicUsize::new(0);

pub fn initialize() -> Result<()> {
    if INITIALIZED.swap(true, Ordering::AcqRel) {
        return Err(Pvz1051Error::AbiPreconditionFailed(
            "1051 injected runtime is already initialized",
        ));
    }
    RUNNER_CLEANED.store(false, Ordering::Release);
    UNLOAD_FAILED.store(false, Ordering::Release);
    UNLOAD_REQUESTED.store(false, Ordering::Release);
    if !crate::host::lifecycle::loader_attached() {
        mark_unowned_initialization_failure();
        return Err(Pvz1051Error::AbiPreconditionFailed("1051 DLL attach was not recorded"));
    }
    if let Err(error) = patch::install() {
        mark_unowned_initialization_failure();
        return Err(error);
    }
    RUNNER_CLEANED.store(!dispatch_entry::installed(), Ordering::Release);
    Ok(())
}

pub(crate) fn mark_unowned_initialization_failure() {
    if patch::is_installed() {
        return;
    }
    INITIALIZED.store(false, Ordering::Release);
    RUNNER_CLEANED.store(true, Ordering::Release);
}

pub fn unload_requested() -> bool {
    UNLOAD_REQUESTED.load(Ordering::Acquire)
}

pub(super) fn accepting_dispatch() -> bool {
    INITIALIZED.load(Ordering::Acquire) && !unload_requested()
}

pub fn mark_runner_cleanup_complete() {
    RUNNER_CLEANED.store(true, Ordering::Release);
}

/// Removes native observers before the runner clears any state they can call.
///
/// This must run on the game thread immediately before runner cleanup.
pub fn prepare_runner_cleanup() -> Result<()> {
    let observer_result = control::restore_observers();
    let control_result = control::restore_all();
    if observer_result.is_err() || control_result.is_err() {
        UNLOAD_FAILED.store(true, Ordering::Release);
    }
    observer_result.and(control_result)
}

pub fn request_unload_and_wait() -> bool {
    request_unload();
    for _ in 0..UNLOAD_WAIT_STEPS {
        if unload_ready() {
            return true;
        }
        if UNLOAD_FAILED.load(Ordering::Acquire) {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    false
}

pub(crate) fn request_unload() {
    UNLOAD_REQUESTED.store(true, Ordering::Release);
}

pub(crate) fn fail_and_request_unload() {
    UNLOAD_FAILED.store(true, Ordering::Release);
    UNLOAD_REQUESTED.store(true, Ordering::Release);
}

pub(super) fn drive_unload_from_game_thread() {
    if !should_restore_physical_owner(
        unload_requested(),
        patch::is_installed(),
        RUNNER_CLEANED.load(Ordering::Acquire),
    ) {
        return;
    }
    let observers = control::restore_observers();
    let controls = control::restore_all();
    let hook = patch::uninstall();
    if observers.is_err() || controls.is_err() || hook.is_err() {
        UNLOAD_FAILED.store(true, Ordering::Release);
    }
}

fn should_restore_physical_owner(unload_requested: bool, hook_installed: bool, runner_cleaned: bool) -> bool {
    unload_requested && hook_installed && runner_cleaned
}

fn unload_ready() -> bool {
    unload_ready_from_facts(
        UNLOAD_REQUESTED.load(Ordering::Acquire),
        RUNNER_CLEANED.load(Ordering::Acquire),
        ACTIVE_HOOK_DEPTH.load(Ordering::Acquire),
        patch::is_installed(),
        UNLOAD_FAILED.load(Ordering::Acquire),
    )
}

fn unload_ready_from_facts(
    unload_requested: bool, runner_cleaned: bool, active_hook_depth: usize, hook_installed: bool, unload_failed: bool,
) -> bool {
    unload_requested && runner_cleaned && active_hook_depth == 0 && !hook_installed && !unload_failed
}

pub(super) struct ActiveHookGuard;

impl ActiveHookGuard {
    pub(super) fn enter() -> Self {
        ACTIVE_HOOK_DEPTH.fetch_add(1, Ordering::AcqRel);
        Self
    }
}

impl Drop for ActiveHookGuard {
    fn drop(&mut self) {
        let previous = ACTIVE_HOOK_DEPTH.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unload_becomes_ready_only_after_the_active_hook_returns() {
        assert!(!unload_ready_from_facts(true, true, 1, false, false));
        assert!(unload_ready_from_facts(true, true, 0, false, false));
        assert!(!unload_ready_from_facts(false, true, 0, false, false));
        assert!(!unload_ready_from_facts(true, false, 0, false, false));
        assert!(!unload_ready_from_facts(true, true, 0, true, false));
        assert!(!unload_ready_from_facts(true, true, 0, false, true));
    }

    #[test]
    fn physical_restore_waits_for_runner_cleanup_after_a_mid_dispatch_request() {
        assert!(!should_restore_physical_owner(true, true, false));
        assert!(should_restore_physical_owner(true, true, true));
        assert!(!should_restore_physical_owner(false, true, true));
        assert!(!should_restore_physical_owner(true, false, true));
    }
}
