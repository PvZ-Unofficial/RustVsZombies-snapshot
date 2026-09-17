#![warn(
    clippy::doc_link_code,
    clippy::fn_params_excessive_bools,
    clippy::future_not_send,
    clippy::indexing_slicing,
    clippy::missing_assert_message,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::needless_pass_by_ref_mut,
    clippy::needless_pass_by_value,
    clippy::option_option,
    clippy::panic_in_result_fn,
    clippy::ref_option,
    clippy::ref_option_ref,
    clippy::return_self_not_must_use,
    clippy::string_slice,
    clippy::unnecessary_wraps,
    clippy::unwrap_in_result,
    clippy::unwrap_used,
    clippy::use_self,
    clippy::wildcard_enum_match_arm
)]
#![expect(
    clippy::missing_errors_doc,
    reason = "user APIs return uniform runtime/backend errors; detailed user-facing contracts belong in module docs and guides"
)]
#![feature(fn_traits, unboxed_closures, trivial_bounds)]
#![allow(
    incomplete_features,
    trivial_bounds,
    reason = "concrete current-backend capability bounds are checked at use"
)]

//! User-facing RustVsZombies API crate.
//!
//! Game-operation examples are checked with a selected backend, for example
//! `cargo test -p rsvz --features pvz-emulator --doc`.

extern crate self as rsvz;

pub mod auto_collect;
pub mod bench;
mod callable;
pub mod cards;
pub mod cob;
pub mod core;
pub mod dsl;
pub mod event;
pub mod grid;
pub mod ice_filler;
pub mod key;
pub mod live_value;
pub mod measure;
pub mod plant;
pub mod plant_fixer;
pub mod prelude;
mod registration;
mod report;
pub mod runtime;
pub mod runtime_frame;
pub mod script;
pub mod session;
pub mod setup;
pub mod shovel;
pub mod smart_fodder;
pub mod smart_remove;
pub mod state_hook;
pub mod tick;
pub mod time;
pub mod timeline;
pub mod zombie;

pub use live_value::LiveValue;
pub use report::log;
pub use rsvz_game::diagnostics::LogContext;
pub use rsvz_game::diagnostics::LogLevel;
pub use rsvz_game::diagnostics::LogRecord;
pub use rsvz_game::diagnostics::Logger;
pub use rsvz_game::diagnostics::LoggerHandle;
pub use rsvz_game::diagnostics::replace_logger;
pub use rsvz_game::diagnostics::reset_logger;
pub use rsvz_game::diagnostics::restore_logger;
pub use rsvz_game::diagnostics::set_logger;
pub use rsvz_game::logic::blover::is_safe_blover;
pub use rsvz_game::logic::imitator_ice::is_safe_imitator_ice;
pub use rsvz_game::session::DispatchOutcome;
pub use rsvz_game::session::SessionJobKey;
pub use rsvz_game::{
    Lineup, LineupApplyOptions, LineupBase, LineupCell, LineupParseError, LineupPlant, LineupReloadPolicy,
    SessionArtifact,
};
pub use rsvz_macros::{script, state_hooks};
pub use rsvz_model::{ResetCardCooldowns, SessionShard, WorldResetConfig};
pub use runtime::{RuntimeError, RuntimeResult};
pub use session::{
    claim_session_job, fail_script, publish_artifact, request_world_reset, session_shard, set_auto_enter, stop_script,
};
pub use zombie::zombie_state;

/// Adapts a legacy formatted-error callback into the structured logger slot.
///
/// New code should use [`set_logger`] or [`replace_logger`]. Restore the
/// returned logger with [`restore_logger`].
pub use report::replace_error_reporter;

#[doc(hidden)]
pub mod __private {
    pub use crate::__run_script as run_script;
    pub use crate::install_framework_state_hooks;
    pub use crate::state_hook::__run_state_hook_installer as run_state_hook_installer;
    pub use rsvz_current::DispatchInput;
    pub use rsvz_current::DispatchResult;
    pub use rsvz_current::{CurrentBackend, with_backend_shared};
    pub use rsvz_game::dispatch::run as runtime_dispatch;
    pub use rsvz_schedule::callback::into_error as into_callback_error;
}

#[doc(hidden)]
pub fn __run_script(body: impl FnOnce() -> crate::runtime::RuntimeResult<()>) -> crate::runtime::RuntimeResult<()> {
    let _ensure_groups = dsl::begin_script();
    registration::run_script(body)
}

pub use rsvz_game::with_backend;

pub use rsvz_schedule::timeline::with_timeline;

pub use rsvz_schedule::tick::with_scheduler;

pub use rsvz_game::lifecycle::install_framework_state_hooks;

pub use rsvz_game::timeline::runtime_timeline_diagnostics;

pub fn runtime_diagnostics() -> String {
    format!("{:?}", runtime_timeline_diagnostics())
}

pub use rsvz_game::timeline::runtime_wave_clocks;

pub use rsvz_game::timeline::with_runtime_wave_clocks;

pub use rsvz_game::timeline::discard_runtime_before_relative_time;

pub use rsvz_game::timeline::prime_runtime_total_waves;

pub use rsvz_game::frame::runtime_timed_operations_idle;

#[doc(hidden)]
pub use rsvz_game::fast_forward::register_fast_forward_window_task;

pub use rsvz_game::frame::runtime_frame_meta;

pub use rsvz_game::frame::dispatch_runtime_frame;

pub use rsvz_game::frame::dispatch_runtime_tick;

pub use rsvz_game::frame::reset_runtime_state_preserving_backend;

pub use cob::CobManager;

pub use plant::Plant;
pub use zombie::Zombie;

pub use zombie::for_each_zombie;

pub use plant::for_each_plant;

pub use rsvz_game::timeline::{at, at_frame, try_at, try_at_frame};

pub use rsvz_game::{Frame, PlantRef, ZombieRef};

pub use rsvz_game::timing::refresh_countdown;
