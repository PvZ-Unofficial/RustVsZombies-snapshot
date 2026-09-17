use rsvz_backend_api::artifact::SessionArtifact;
use rsvz_backend_api::error::RuntimeError;
use rsvz_model::SessionShard;

use crate::Pvz1051Backend;

#[derive(Clone, Copy)]
pub struct DispatchInput {
    pub stop_requested: bool,
    pub completed_rounds: u64,
    pub opening_ready: bool,
}

impl DispatchInput {
    #[must_use]
    pub const fn session_shard(self) -> SessionShard {
        SessionShard {
            index: 0,
            count: 1,
            seed_base: 0,
        }
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
        self.opening_ready
    }

    #[must_use]
    pub const fn registration_safe_point_required(self) -> bool {
        false
    }

    #[must_use]
    pub const fn opening_transition_requires_native_update(self) -> bool {
        false
    }

    #[must_use]
    pub const fn main_ui_reload_supported(self) -> bool {
        true
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

pub type DispatchEntry = fn(&mut Pvz1051Backend, DispatchInput) -> DispatchResult;
