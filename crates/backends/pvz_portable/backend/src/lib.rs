//! PvZ-Portable backend adapter for RustVsZombies.

mod access;
mod dispatch;
mod error;
mod event;
mod ffi;
mod handles;
pub mod host;
mod impls;
mod input_profile;
mod runtime;

pub use dispatch::{DispatchEntry, DispatchInput, DispatchResult};
pub use error::{PortableBackendError, Result};
#[doc(hidden)]
pub use ffi::{plugin_abi_version, plugin_initialize, plugin_shutdown};
pub use runtime::PortableBackend;

/// Writes a runtime error through the native Portable host.
pub fn default_log_output(message: &str) {
    ffi::log(message);
}

pub use access::{backend_access_epoch, scope_backend, try_with_backend, with_backend, with_backend_shared};
