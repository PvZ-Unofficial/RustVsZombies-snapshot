//! Shared callback and backend boundary error.

use std::borrow::Cow;

/// Result shorthand used by backend-free runtime callbacks.
pub type RuntimeResult<T = ()> = Result<T, RuntimeError>;

/// Backend-neutral runtime error for callbacks that no longer carry a backend error type.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct RuntimeError {
    message: Cow<'static, str>,
}

impl RuntimeError {
    #[must_use]
    pub fn new(message: impl Into<Cow<'static, str>>) -> Self {
        Self {
            message: message.into(),
        }
    }

    #[must_use]
    pub const fn message(&self) -> &Cow<'static, str> {
        &self.message
    }
}

impl From<Cow<'static, str>> for RuntimeError {
    fn from(message: Cow<'static, str>) -> Self {
        Self::new(message)
    }
}

impl From<&'static str> for RuntimeError {
    fn from(message: &'static str) -> Self {
        Self::new(message)
    }
}

impl From<String> for RuntimeError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<rsvz_model::ZombieMotionCallError> for RuntimeError {
    fn from(error: rsvz_model::ZombieMotionCallError) -> Self {
        Self::new(error.to_string())
    }
}
