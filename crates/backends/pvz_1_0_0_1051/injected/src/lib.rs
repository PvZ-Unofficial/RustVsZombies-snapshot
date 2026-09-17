//! PvZ 1.0.0.1051 injected backend.

#[cfg(not(all(
    target_os = "windows",
    target_arch = "x86",
    target_env = "msvc",
    target_pointer_width = "32"
)))]
compile_error!("PvZ 1.0.0.1051 injected backend requires i686-pc-windows-msvc");

mod access;
mod dispatch;
pub(crate) mod error;
pub mod host;
pub(crate) mod impls {
    mod base;
    mod cob;
    mod contact;
    pub(crate) mod event;
    mod gameplay;
    mod handles;
    mod input;
    mod item;
    mod modifier;
    mod plant_state;
    pub(crate) mod reset;
    mod smart_remove;
    mod state;
    mod surface;
    mod timing;
}
pub(crate) mod ops {
    pub(crate) mod grid_item;
    pub(crate) mod plant;
    pub(crate) mod seed;
    pub(crate) mod timing;
}
pub(crate) mod patches {
    //! 1051-local byte patch helpers.

    mod event;
    mod imitator_morph;
    pub(crate) mod leases;
    pub(crate) mod memory;
    pub(crate) mod mods;
    mod pause;
    mod performance;
    mod random;
    mod spec;
    mod sun;
    pub(crate) mod variant;

    pub(crate) use event::EventHookGuard;
    pub(crate) use imitator_morph::ImitatorMorphHookGuard;
    pub(crate) use pause::AdvancedPausePatch;
    pub(crate) use performance::SimulationPerformanceGuard;
    pub(crate) use random::{
        ResetRandomGuard, configure as configure_random, configure_wave_seed as configure_wave_random_seed,
        effective_seed as effective_random_seed, fixed as random_fixed, level_seed as level_random_seed,
        locked as random_locked, note_reset_seed as note_random_reset_seed, restore as restore_random,
        seed as random_seed,
    };
    pub(crate) use sun::{restore as restore_sun_production, set_direct_credit};
}
pub(crate) mod raw {
    pub(crate) mod abi;
    pub(crate) mod kind;
    pub(crate) mod layout;
    pub(crate) mod types;
}
pub(crate) mod runtime {
    mod advanced_pause;
    pub(crate) mod backend;
    mod fast_forward;
    pub(crate) mod hook;
    pub(crate) mod imitator_morph;
    mod seed_chooser;

    pub use backend::Pvz1051Backend;
}

pub use dispatch::{DispatchEntry, DispatchInput, DispatchResult};
pub use error::Pvz1051Error;
pub use runtime::Pvz1051Backend;

/// Displays a runtime log record through the 1051 backend's emergency UI.
pub fn default_log_output(message: &str) {
    runtime::hook::report_runtime_error(message);
}

pub use access::{backend_access_epoch, scope_backend, try_with_backend, with_backend, with_backend_shared};
