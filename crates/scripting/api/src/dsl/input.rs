//! Registration-time wave selection.

pub use rsvz_game::logic::waves::{IntoWaveSet, WaveSet};

/// Selects one wave for subsequent `time << expression` bindings.
pub fn wave(value: i32) {
    set_current(value);
}

/// Selects waves for subsequent `time << expression` bindings.
pub fn waves(values: impl IntoWaveSet) {
    match values.into_wave_set() {
        Ok(values) => crate::registration::set_current_wave_bits(values.bits()),
        Err(error) => crate::registration::record_error(error),
    }
}

fn set_current(value: i32) {
    match WaveSet::single(value) {
        Ok(value) => crate::registration::set_current_wave_bits(value.bits()),
        Err(error) => crate::registration::record_error(error),
    }
}

#[cfg(test)]
mod tests {
    use crate::runtime::RuntimeResult;

    use super::*;

    #[test]
    fn invalid_selection_is_collected_without_replacing_current_waves() {
        let result = crate::registration::run_script(|| -> RuntimeResult<()> {
            wave(3);
            waves([0, 1]);
            assert_eq!(crate::registration::require_current_wave_bits(), Some(1 << 2));
            Ok(())
        });

        assert_eq!(
            result.expect_err("invalid wave").message().as_ref(),
            "wave must be in 1..=20, got 0"
        );
    }
}
