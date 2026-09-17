//! Convenience wrapper for values read from re-resolved live object IDs.

/// Value returned by convenience live-object field methods.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LiveValue<T> {
    value: Option<T>,
}

impl<T> LiveValue<T> {
    /// Creates a present live value.
    #[must_use]
    pub const fn live(value: T) -> Self {
        Self { value: Some(value) }
    }

    /// Creates a missing value for a stale or absent object ID.
    #[must_use]
    pub const fn missing() -> Self {
        Self { value: None }
    }

    /// Returns whether the object was live when this value was read.
    #[must_use]
    pub const fn is_live(&self) -> bool {
        self.value.is_some()
    }

    /// Borrows the underlying optional value.
    #[must_use]
    pub const fn as_option(&self) -> Option<&T> {
        self.value.as_ref()
    }

    /// Returns the underlying optional value.
    #[must_use]
    pub fn into_option(self) -> Option<T> {
        self.value
    }

    /// Returns the live value or `default` when the object was missing.
    #[must_use]
    pub fn unwrap_or(self, default: T) -> T {
        self.value.unwrap_or(default)
    }

    /// Returns the live value, panicking with `message` when the object was missing.
    ///
    /// # Panics
    ///
    /// Panics with `message` if this value is missing.
    #[must_use]
    pub fn expect_live(self, message: &str) -> T {
        self.value.unwrap_or_else(|| panic!("{message}"))
    }
}

impl<T: PartialEq> PartialEq<T> for LiveValue<T> {
    fn eq(&self, other: &T) -> bool {
        self.value.as_ref().is_some_and(|value| value == other)
    }
}

pub(crate) fn read_or_abort<T, E: std::fmt::Display>(result: Result<T, E>, context: &str) -> T {
    result.unwrap_or_else(|error| {
        crate::diagnostics::abort_operation(crate::runtime::RuntimeError::new(format!("{context}: {error}")))
    })
}

pub(crate) fn live_or_missing<T, E: std::fmt::Display>(result: Result<Option<T>, E>, context: &str) -> LiveValue<T> {
    LiveValue {
        value: read_or_abort(result, context),
    }
}
