//! Current backend selector.
//!
//! This crate owns only compile-time selection. Runtime storage and lifecycle
//! live in game/schedule; token access lives in each backend. Without a game
//! backend feature, the selected uninhabited backend exposes no game capability.

#[cfg(all(
    panic = "abort",
    any(feature = "pvz-1-0-0-1051", feature = "pvz-emulator", feature = "pvz-portable")
))]
compile_error!("RSVZ callback-local operation errors require panic=unwind");

#[cfg(any(
    all(feature = "pvz-1-0-0-1051", feature = "pvz-emulator"),
    all(feature = "pvz-1-0-0-1051", feature = "pvz-portable"),
    all(feature = "pvz-emulator", feature = "pvz-portable")
))]
compile_error!("enable only one RSVZ current backend feature");

#[cfg(all(
    feature = "pvz-1-0-0-1051",
    not(any(feature = "pvz-emulator", feature = "pvz-portable"))
))]
mod selected {
    pub use rsvz_backend_1051::{
        DispatchEntry, DispatchInput, DispatchResult, Pvz1051Backend as CurrentBackend,
        Pvz1051Error as CurrentBackendError, backend_access_epoch, default_log_output, scope_backend, try_with_backend,
        with_backend, with_backend_shared,
    };
}

#[cfg(all(
    feature = "pvz-emulator",
    not(any(feature = "pvz-1-0-0-1051", feature = "pvz-portable"))
))]
mod selected {
    pub use rsvz_pvz_emulator_backend::{
        DispatchEntry, DispatchInput, DispatchResult, PeBackend as CurrentBackend,
        PeBackendError as CurrentBackendError, backend_access_epoch, default_log_output, scope_backend,
        try_with_backend, with_backend, with_backend_shared,
    };
}

#[cfg(all(
    feature = "pvz-portable",
    not(any(feature = "pvz-1-0-0-1051", feature = "pvz-emulator"))
))]
mod selected {
    pub use rsvz_pvz_portable_backend::{
        DispatchEntry, DispatchInput, DispatchResult, PortableBackend as CurrentBackend,
        PortableBackendError as CurrentBackendError, backend_access_epoch, default_log_output, scope_backend,
        try_with_backend, with_backend, with_backend_shared,
    };
}

#[cfg(any(
    not(any(feature = "pvz-1-0-0-1051", feature = "pvz-emulator", feature = "pvz-portable")),
    all(feature = "pvz-1-0-0-1051", feature = "pvz-emulator"),
    all(feature = "pvz-1-0-0-1051", feature = "pvz-portable"),
    all(feature = "pvz-emulator", feature = "pvz-portable")
))]
mod selected {
    pub use rsvz_no_backend::{
        DispatchEntry, DispatchInput, DispatchResult, NoBackend as CurrentBackend,
        NoBackendError as CurrentBackendError, backend_access_epoch, default_log_output, scope_backend,
        try_with_backend, with_backend, with_backend_shared,
    };
}

pub use selected::*;

pub mod current_backend {
    //! Compatibility namespace for the selected current backend type.

    pub use super::CurrentBackend;
}
