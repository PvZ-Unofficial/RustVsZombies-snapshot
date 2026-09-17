use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

const DLL_PROCESS_DETACH: u32 = 0;
const DLL_PROCESS_ATTACH: u32 = 1;

static MODULE: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static ATTACHED: AtomicBool = AtomicBool::new(false);

/// Loader-lock-safe lifecycle recording. This performs only atomic stores.
pub(crate) fn record_loader_event(module: *mut c_void, reason: u32) {
    match reason {
        DLL_PROCESS_ATTACH => {
            MODULE.store(module, Ordering::Release);
            ATTACHED.store(true, Ordering::Release);
        }
        DLL_PROCESS_DETACH => ATTACHED.store(false, Ordering::Release),
        _ => {}
    }
}

pub(crate) fn loader_attached() -> bool {
    ATTACHED.load(Ordering::Acquire)
}

#[cfg(test)]
fn loader_module() -> *mut c_void {
    MODULE.load(Ordering::Acquire)
}

// Define the CRT's ordinary DllMain symbol in assembly so rustc does not add it
// to the cdylib export list. The jump target remains ordinary Rust.
core::arch::global_asm!(
    ".globl _DllMain@12",
    "_DllMain@12:",
    "jmp {record}",
    record = sym record_dll_main,
);

#[inline(never)]
extern "system" fn record_dll_main(module: *mut c_void, reason: u32, _reserved: *mut c_void) -> i32 {
    record_loader_event(module, reason);
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loader_entry_only_records_atomic_state() {
        let module = std::ptr::dangling_mut::<c_void>();
        record_loader_event(module, DLL_PROCESS_ATTACH);
        assert!(loader_attached());
        assert_eq!(loader_module(), module);
        record_loader_event(module, DLL_PROCESS_DETACH);
        assert!(!loader_attached());
    }
}
