//! Backend-free values whose invariants are checked once at API boundaries.

use crate::model::SPAWN_SLOTS_PER_WAVE;

/// Failure while constructing a checked scalar game value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CheckedValueError {
    #[error("health must be positive")]
    NonPositiveHealth,
    #[error("value must be non-negative")]
    NegativeValue,
    #[error("floating-point value must be finite")]
    NonFinite,
    #[error("floating-point value must be positive and finite")]
    NonPositiveFinite,
    #[error("floating-point coordinate must be representable by PvZ's i32 coordinate")]
    CoordinateOutOfRange,
    #[error("sun amount exceeds PvZ's i32 scalar range")]
    SunAmountOutOfRange,
    #[error("spawn slot is outside one wave's spawn table")]
    SpawnSlotOutOfRange,
}

/// Positive plant or zombie health.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PositiveHp(i32);

impl PositiveHp {
    pub const fn new(value: i32) -> Result<Self, CheckedValueError> {
        if value > 0 {
            Ok(Self(value))
        } else {
            Err(CheckedValueError::NonPositiveHealth)
        }
    }

    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

/// A non-negative native countdown or counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NonNegativeI32(i32);

impl NonNegativeI32 {
    pub const fn new(value: i32) -> Result<Self, CheckedValueError> {
        if value >= 0 {
            Ok(Self(value))
        } else {
            Err(CheckedValueError::NegativeValue)
        }
    }

    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

/// A finite `f32` scalar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FiniteF32(f32);

impl FiniteF32 {
    pub fn new(value: f32) -> Result<Self, CheckedValueError> {
        if value.is_finite() {
            Ok(Self(value))
        } else {
            Err(CheckedValueError::NonFinite)
        }
    }

    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

/// A positive finite `f32` scalar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositiveFiniteF32(f32);

impl PositiveFiniteF32 {
    pub fn new(value: f32) -> Result<Self, CheckedValueError> {
        if value.is_finite() && value > 0.0 {
            Ok(Self(value))
        } else {
            Err(CheckedValueError::NonPositiveFinite)
        }
    }

    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }
}

/// A finite `f32` whose truncation is representable by an `i32`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct I32RepresentableF32(f32);

impl I32RepresentableF32 {
    const I32_MIN_F32: f32 = -2_147_483_648.0;
    const I32_MAX_EXCLUSIVE_F32: f32 = 2_147_483_648.0;

    pub fn new(value: f32) -> Result<Self, CheckedValueError> {
        if value.is_finite() && (Self::I32_MIN_F32..Self::I32_MAX_EXCLUSIVE_F32).contains(&value) {
            Ok(Self(value))
        } else {
            Err(CheckedValueError::CoordinateOutOfRange)
        }
    }

    #[must_use]
    pub const fn get(self) -> f32 {
        self.0
    }

    #[must_use]
    pub fn truncated(self) -> i32 {
        self.0 as i32
    }
}

/// A sun amount representable by PvZ's signed native scalar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SunAmount(i32);

impl SunAmount {
    pub fn new(value: u32) -> Result<Self, CheckedValueError> {
        i32::try_from(value)
            .map(Self)
            .map_err(|_error| CheckedValueError::SunAmountOutOfRange)
    }

    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }

    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0 as u32
    }
}

/// Zero-based slot inside one wave's fixed spawn table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SpawnWaveSlot(usize);

impl SpawnWaveSlot {
    pub const fn new(index: usize) -> Result<Self, CheckedValueError> {
        if index < SPAWN_SLOTS_PER_WAVE {
            Ok(Self(index))
        } else {
            Err(CheckedValueError::SpawnSlotOutOfRange)
        }
    }

    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_integer_values_reject_invalid_boundaries() {
        assert!(PositiveHp::new(0).is_err());
        assert!(PositiveHp::new(1).is_ok());
        assert!(NonNegativeI32::new(-1).is_err());
        assert_eq!(NonNegativeI32::new(0).expect("zero").get(), 0);
        assert!(SunAmount::new(i32::MAX as u32).is_ok());
        assert!(SunAmount::new(i32::MAX as u32 + 1).is_err());
    }

    #[test]
    fn checked_float_values_reject_non_finite_and_out_of_range_values() {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(FiniteF32::new(value).is_err());
            assert!(PositiveFiniteF32::new(value).is_err());
            assert!(I32RepresentableF32::new(value).is_err());
        }
        assert!(PositiveFiniteF32::new(0.0).is_err());
        assert_eq!(FiniteF32::new(0.0).expect("finite zero").get(), 0.0);
        assert!(PositiveFiniteF32::new(0.1).is_ok());
        assert!(I32RepresentableF32::new(-2_147_483_648.0).is_ok());
        assert_eq!(
            I32RepresentableF32::new(-12.75).expect("representable").truncated(),
            -12
        );
        assert!(I32RepresentableF32::new(2_147_483_648.0).is_err());
    }

    #[test]
    fn spawn_slot_uses_the_canonical_wave_width() {
        assert!(SpawnWaveSlot::new(SPAWN_SLOTS_PER_WAVE - 1).is_ok());
        assert!(SpawnWaveSlot::new(SPAWN_SLOTS_PER_WAVE).is_err());
    }
}
