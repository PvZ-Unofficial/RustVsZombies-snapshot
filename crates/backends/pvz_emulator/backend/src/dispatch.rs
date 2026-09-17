use rsvz_backend_api::artifact::SessionArtifact;
use rsvz_backend_api::error::RuntimeError;
use rsvz_model::SessionShard;

use crate::PeBackend;

#[derive(Clone, Copy)]
pub struct DispatchInput {
    pub shard: SessionShard,
    pub stop_requested: bool,
    pub completed_rounds: u64,
}

impl DispatchInput {
    #[must_use]
    pub const fn session_shard(self) -> SessionShard {
        self.shard
    }

    #[must_use]
    pub const fn world_replaced(self) -> bool {
        false
    }

    #[must_use]
    pub const fn completed_rounds(self) -> u64 {
        self.completed_rounds
    }

    #[must_use]
    pub const fn opening_ready(self) -> bool {
        true
    }

    #[must_use]
    pub const fn registration_safe_point_required(self) -> bool {
        false
    }

    #[must_use]
    pub const fn opening_transition_requires_native_update(self) -> bool {
        true
    }

    #[must_use]
    pub const fn main_ui_reload_supported(self) -> bool {
        false
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

pub type DispatchEntry = fn(&mut PeBackend, DispatchInput) -> DispatchResult;
