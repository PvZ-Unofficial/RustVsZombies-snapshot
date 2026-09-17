//! Wave grammar context nested around the game registration transaction.

use crate::runtime::RuntimeResult;
pub(crate) use rsvz_game::registration::{is_active, record_error};
use std::cell::Cell;

thread_local! { static WAVE_BITS: Cell<Option<u32>> = const { Cell::new(None) }; }

struct WaveGuard(Option<u32>);
impl Drop for WaveGuard {
    fn drop(&mut self) {
        WAVE_BITS.set(self.0);
    }
}

pub(crate) fn set_current_wave_bits(bits: u32) {
    assert!(is_active(), "low-level DSL call has no current script context");
    WAVE_BITS.set(Some(bits));
}

pub(crate) fn require_current_wave_bits() -> Option<u32> {
    assert!(is_active(), "low-level DSL call has no current script context");
    let bits = WAVE_BITS.get();
    if bits.is_none() {
        record_error("timed DSL operation requires wave(...) or waves(...) first");
    }
    bits
}

#[doc(hidden)]
pub fn run_script(body: impl FnOnce() -> RuntimeResult<()>) -> RuntimeResult<()> {
    let _wave_guard = WaveGuard(WAVE_BITS.replace(None));
    rsvz_game::registration::run_script(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::RuntimeError;

    #[test]
    fn registration_errors_keep_source_order() {
        let error = run_script(|| {
            record_error("first");
            record_error("second");
            Err(RuntimeError::new("body"))
        })
        .expect_err("registration must fail");

        assert_eq!(error.message().as_ref(), "first; second; body");
    }

    #[test]
    fn nested_and_panicking_registration_restore_previous_context() {
        let result = run_script(|| {
            set_current_wave_bits(1);
            let panic = std::panic::catch_unwind(|| {
                let _nested = run_script(|| {
                    set_current_wave_bits(2);
                    panic!("probe");
                });
            });
            assert!(panic.is_err());
            assert_eq!(require_current_wave_bits(), Some(1));
            Ok(())
        });

        assert!(result.is_ok());
    }
}
