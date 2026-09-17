//! Shared validation error for scalar modifier helpers.

use crate::model::CheckedValueError;

/// Failure while validating or applying a scalar modifier value.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ModifierValueError {
    #[error("invalid modifier value: {0}")]
    InvalidValue(CheckedValueError),
    #[error("modifier operation rejected: {0}")]
    Rejected(crate::runtime::RuntimeError),
}
