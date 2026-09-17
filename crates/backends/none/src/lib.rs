//! Compile-time placeholder for builds that do not select a game backend.
//!
//! No token, Board, host, TLS or game state can be created here. Game capability
//! traits deliberately remain unimplemented; pure code can still be compiled
//! and tested, while calling a capability-bound operation is a compile error.

use rsvz_backend_api::Backend;
use rsvz_backend_api::access::BackendAccessError;
use rsvz_backend_api::artifact::SessionArtifact;
use rsvz_backend_api::error::RuntimeError;
use rsvz_model::SessionShard;
use std::io::Write as _;

/// No backend token exists when no game backend is selected.
#[derive(Clone, Copy, Debug)]
#[allow(
    clippy::empty_enums,
    reason = "the placeholder must have no constructible backend token"
)]
pub enum NoBackend {}

impl Backend for NoBackend {
    type Error = NoBackendError;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoBackendError;

impl std::fmt::Display for NoBackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("no game backend selected")
    }
}

impl std::error::Error for NoBackendError {}

impl NoBackendError {
    pub const fn is_board_unavailable(&self) -> bool {
        true
    }

    pub const fn is_operation_rejection(&self) -> bool {
        false
    }
}

impl From<NoBackendError> for RuntimeError {
    fn from(error: NoBackendError) -> Self {
        Self::new(error.to_string())
    }
}

pub type Result<T> = std::result::Result<T, NoBackendError>;

pub fn scope_backend<R>(_backend: &mut NoBackend, _body: impl FnOnce() -> R) -> R {
    panic!("{NoBackendError}")
}

pub fn with_backend<R>(_f: impl for<'a> FnOnce(&'a mut NoBackend) -> R) -> R {
    panic!("{NoBackendError}")
}

pub fn try_with_backend<R>(
    _f: impl for<'a> FnOnce(&'a mut NoBackend) -> R,
) -> std::result::Result<R, BackendAccessError> {
    Err(BackendAccessError::NotInstalled)
}

pub fn with_backend_shared<R>(
    _f: impl for<'a> FnOnce(&'a NoBackend) -> R,
) -> std::result::Result<R, BackendAccessError> {
    Err(BackendAccessError::NotInstalled)
}

pub const fn backend_access_epoch() -> Option<u64> {
    None
}

pub fn default_log_output(message: &str) {
    let _written = writeln!(std::io::stderr().lock(), "{message}");
}

/// The empty private token prevents any backend-free host input from existing.
#[derive(Clone, Copy)]
pub struct DispatchInput {
    pub stop_requested: bool,
    unavailable: NoBackend,
}

impl DispatchInput {
    pub const fn session_shard(self) -> SessionShard {
        match self.unavailable {}
    }

    pub const fn world_replaced(self) -> bool {
        match self.unavailable {}
    }

    pub const fn completed_rounds(self) -> u64 {
        match self.unavailable {}
    }

    pub const fn opening_ready(self) -> bool {
        match self.unavailable {}
    }

    pub const fn registration_safe_point_required(self) -> bool {
        match self.unavailable {}
    }

    pub const fn opening_transition_requires_native_update(self) -> bool {
        match self.unavailable {}
    }

    pub const fn main_ui_reload_supported(self) -> bool {
        match self.unavailable {}
    }
}

pub enum DispatchResult {
    Continue,
    SkipUpdate,
    Stop {
        artifact: Option<SessionArtifact>,
        error: Option<RuntimeError>,
    },
}

pub type DispatchEntry = fn(&mut NoBackend, DispatchInput) -> DispatchResult;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_cannot_run_callbacks_or_create_a_board() {
        assert!(std::panic::catch_unwind(|| with_backend(|_| ())).is_err());
        assert!(matches!(
            try_with_backend(|_| panic!("must not enter a missing backend")),
            Err(BackendAccessError::NotInstalled)
        ));
        assert!(matches!(
            with_backend_shared(|_| panic!("must not enter a missing backend")),
            Err(BackendAccessError::NotInstalled)
        ));
        assert_eq!(backend_access_epoch(), None);
        assert!(NoBackendError.is_board_unavailable());
        assert!(!NoBackendError.is_operation_rejection());
    }
}
