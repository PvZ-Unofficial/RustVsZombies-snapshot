//! Session control APIs available to user scripts and state hooks.

pub use rsvz_game::session::{
    claim_session_job, request_world_reset, session_shard, set_session_artifact as publish_artifact, stop_script,
};
pub use rsvz_game::setup::set_auto_enter;

/// Converts a script diagnostic into the canonical fatal session error.
pub fn fail_script(error: impl std::fmt::Display) {
    rsvz_game::session::fail_script(rsvz_backend_api::error::RuntimeError::new(error.to_string()));
}

pub use rsvz_game::session::{completed_rounds, forbid_additional_world_resets, take_dispatch_outcome, world_epoch};
