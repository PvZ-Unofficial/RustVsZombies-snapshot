//! session operations for the selected backend.

use crate::frame::{DISPATCH_STATE, FRAME};
use crate::runtime::{RuntimeError, RuntimeFrameState, RuntimeResult};
use crate::session::{
    ArtifactSlot, DispatchOutcomeState, RuntimeDispatchState, SessionControl, SessionJobKey, SessionJobState,
    SessionProgress, fail_script,
};
use crate::setup::{AUTO_ENTER, OPENING, OpeningState};
use rsvz_backend_api::WorldResetBackend as _;
use rsvz_backend_api::artifact::SessionArtifact;
use rsvz_model::{SessionShard, WorldResetConfig};
use std::cell::RefCell;

pub struct WorldResetRequest {
    pub config: WorldResetConfig,
    pub execute: fn(&mut rsvz_current::CurrentBackend, WorldResetConfig) -> RuntimeResult<()>,
}

pub struct WorldResetState {
    pending: Option<WorldResetRequest>,
    forbidden: bool,
    epoch: u64,
    next_opening_seed: Option<u64>,
}

impl Default for WorldResetState {
    fn default() -> Self {
        Self {
            pending: None,
            forbidden: false,
            epoch: 0,
            next_opening_seed: None,
        }
    }
}

impl WorldResetState {
    pub fn request(
        &mut self, config: WorldResetConfig,
        execute: fn(&mut rsvz_current::CurrentBackend, WorldResetConfig) -> RuntimeResult<()>,
    ) -> RuntimeResult<()> {
        if self.forbidden {
            return Err(RuntimeError::new("world reset is disabled for this session"));
        }
        if self.pending.is_some() {
            return Err(RuntimeError::new("a world reset is already pending"));
        }
        self.pending = Some(WorldResetRequest { config, execute });
        Ok(())
    }

    pub fn take_pending(&mut self) -> Option<WorldResetRequest> {
        self.pending.take()
    }

    pub const fn pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn clear_pending(&mut self) {
        self.pending = None;
    }

    pub fn forbid(&mut self) {
        self.forbidden = true;
    }

    pub fn mark_replaced(&mut self, seed: Option<u32>) {
        self.next_opening_seed = seed.map(u64::from);
        self.epoch = self.epoch.wrapping_add(1);
    }

    pub fn take_opening_seed(&mut self, fallback: u64) -> u64 {
        // A natural pass retains the world epoch but still needs a fresh type
        // selection. Keep this stream local to the life; reset reseeds it.
        let seed = self.next_opening_seed.unwrap_or(fallback);
        self.next_opening_seed = Some(seed.wrapping_add(0x9e37_79b9_7f4a_7c15));
        seed
    }

    pub const fn epoch(&self) -> u64 {
        self.epoch
    }
}

thread_local! {
    pub(crate) static RESET_STATE: RefCell<WorldResetState> =
        RefCell::new(WorldResetState::default());
    static SESSION_PROGRESS: RefCell<SessionProgress> = RefCell::new(SessionProgress::default());
    pub(crate) static ARTIFACT: RefCell<ArtifactSlot> = RefCell::new(ArtifactSlot::default());
    pub(crate) static SESSION_JOB: RefCell<SessionJobState> = RefCell::new(SessionJobState::default());
}

pub fn reset_session_control() {
    crate::session::with_session_control(|state| *state = SessionControl::default());
    RESET_STATE.with_borrow_mut(|state| *state = WorldResetState::default());
    SESSION_PROGRESS.with_borrow_mut(|state| *state = SessionProgress::default());
    ARTIFACT.with_borrow_mut(|state| *state = ArtifactSlot::default());
    SESSION_JOB.with_borrow_mut(SessionJobState::clear);
    crate::session::with_dispatch_outcome(|state| *state = DispatchOutcomeState::default());
    AUTO_ENTER.set(true);
    OPENING.with_borrow_mut(|opening| *opening = OpeningState::default());
    DISPATCH_STATE.set(RuntimeDispatchState::new());
}

#[doc(hidden)]
pub fn request_world_reset_with(
    config: WorldResetConfig, execute: fn(&mut rsvz_current::CurrentBackend, WorldResetConfig) -> RuntimeResult<()>,
) -> RuntimeResult<()> {
    RESET_STATE.with_borrow_mut(|state| state.request(config, execute))
}

#[doc(hidden)]
pub fn forbid_additional_world_resets() {
    RESET_STATE.with_borrow_mut(WorldResetState::forbid);
}

#[must_use]
pub fn world_reset_pending() -> bool {
    RESET_STATE.with_borrow(WorldResetState::pending)
}

pub fn execute_pending_reset(backend: &mut rsvz_current::CurrentBackend) -> RuntimeResult<bool> {
    let Some(reset) = RESET_STATE.with_borrow_mut(WorldResetState::take_pending) else {
        return Ok(false);
    };
    if let Err(error) = (reset.execute)(backend, reset.config) {
        let returned = RuntimeError::new(error.to_string());
        fail_script(error);
        return Err(returned);
    }
    mark_world_replaced_with_seed(Some(reset.config.seed));
    Ok(true)
}

pub fn mark_world_replaced() {
    mark_world_replaced_with_seed(None);
}

fn mark_world_replaced_with_seed(seed: Option<u32>) {
    FRAME.with_borrow_mut(|frame| *frame = RuntimeFrameState::default());
    crate::diagnostics::clear_report_time();
    OPENING.with_borrow_mut(OpeningState::reset_world);
    crate::session::with_dispatch_outcome(|state| state.set(None));
    RESET_STATE.with_borrow_mut(|state| state.mark_replaced(seed));
}

#[doc(hidden)]
pub fn take_next_opening_seed(fallback: u64) -> u64 {
    RESET_STATE.with_borrow_mut(|state| state.take_opening_seed(fallback))
}

pub fn install_session_shard(shard: SessionShard) -> RuntimeResult<()> {
    SESSION_PROGRESS.with_borrow_mut(|state| state.install_shard(shard))
}

#[must_use]
pub fn session_shard() -> SessionShard {
    SESSION_PROGRESS.with_borrow(SessionProgress::shard)
}

pub fn set_session_artifact(artifact: SessionArtifact) -> RuntimeResult<()> {
    ARTIFACT.with_borrow_mut(|slot| slot.set(artifact))
}

pub fn take_session_artifact() -> Option<SessionArtifact> {
    ARTIFACT.with_borrow_mut(ArtifactSlot::take)
}

/// Returns `true` when this call installed the session job and `false` when
/// the same job was already installed by an earlier script generation.
pub fn claim_session_job(key: SessionJobKey) -> RuntimeResult<bool> {
    let crate::lifecycle::GenerationState::Building(generation) =
        crate::lifecycle::with_lifecycle(|lifecycle| lifecycle.generation())
    else {
        return Err(RuntimeError::new(
            "session jobs may only be claimed during script registration",
        ));
    };
    SESSION_JOB.with_borrow_mut(|state| state.claim(key, generation))
}

#[must_use]
pub fn world_epoch() -> u64 {
    RESET_STATE.with_borrow(WorldResetState::epoch)
}

pub fn record_completed_rounds(count: u64) {
    SESSION_PROGRESS.with_borrow_mut(|state| state.record_completed_rounds(count));
}

#[must_use]
pub fn completed_rounds() -> u64 {
    SESSION_PROGRESS.with_borrow(SessionProgress::completed_rounds)
}

/// Requests a fresh world reset at the existing dispatch safe point.
pub fn request_world_reset(config: WorldResetConfig) -> RuntimeResult<()>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::WorldResetBackend,
{
    fn execute(backend: &mut rsvz_current::CurrentBackend, config: WorldResetConfig) -> RuntimeResult<()>
    where
        rsvz_current::CurrentBackend: rsvz_backend_api::WorldResetBackend,
    {
        backend
            .reset_world(config)
            .map_err(|error| RuntimeError::new(error.to_string()))
    }
    request_world_reset_with(config, execute)
}
