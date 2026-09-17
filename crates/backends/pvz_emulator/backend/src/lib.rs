//! PvZ-Emulator backend adapter for RustVsZombies.

use std::io::Write as _;

mod access;
pub mod convert;
mod dispatch;
pub mod error;
mod event;
pub mod handles;
#[cfg(feature = "host")]
pub mod host;
mod impls;
mod runtime;

pub use dispatch::{DispatchEntry, DispatchInput, DispatchResult};
pub use error::{PeBackendError, Result};
pub use pe_rs;
pub use runtime::{PeBackend, PeUpdateOutcome, PeWorldConfig};

/// Writes a runtime log record to the emulator process's standard error.
pub fn default_log_output(message: &str) {
    let _written = writeln!(std::io::stderr().lock(), "{message}");
}

#[doc(hidden)]
pub mod runner_internal {
    pub use crate::runtime::{PeWorldOwner, PeWorldRun};
}

pub use access::{backend_access_epoch, scope_backend, try_with_backend, with_backend, with_backend_shared};
