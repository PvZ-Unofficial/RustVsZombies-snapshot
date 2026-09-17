//! Backend-neutral state owned by one script session.

use crate::{
    SessionArtifact,
    runtime::{RuntimeError, RuntimeResult},
};
use rsvz_model::{GameUi, ReloadBoundary, ReloadMode, SessionShard};

/// Stable identity used by the single session-job slot.
#[derive(Clone, Copy, Debug)]
pub struct SessionJobKey(&'static u8);

impl SessionJobKey {
    #[must_use]
    pub const fn new(identity: &'static u8) -> Self {
        Self(identity)
    }

    fn same(self, other: Self) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}

#[derive(Clone, Copy, Debug)]
struct SessionJobClaim {
    key: SessionJobKey,
    generation: u64,
}

#[derive(Default)]
pub struct SessionJobState(Option<SessionJobClaim>);

#[derive(Clone, Copy, Debug)]
pub struct SessionJobCheckpoint(Option<SessionJobClaim>);

impl SessionJobState {
    pub const fn is_empty(&self) -> bool {
        self.0.is_none()
    }
}

impl SessionJobState {
    pub fn claim(&mut self, key: SessionJobKey, generation: u64) -> RuntimeResult<bool> {
        match self.0 {
            None => {
                self.0 = Some(SessionJobClaim { key, generation });
                Ok(true)
            }
            Some(existing) if existing.key.same(key) && existing.generation != generation => Ok(false),
            Some(existing) if existing.key.same(key) => Err(RuntimeError::new(
                "the same session job was declared twice in one script generation",
            )),
            Some(_) => Err(RuntimeError::new("a different session job is already installed")),
        }
    }

    pub fn clear(&mut self) {
        self.0 = None;
    }

    pub const fn checkpoint(&self) -> SessionJobCheckpoint {
        SessionJobCheckpoint(self.0)
    }

    pub fn restore(&mut self, checkpoint: SessionJobCheckpoint) {
        self.0 = checkpoint.0;
    }
}

#[derive(Default)]
pub struct SessionControl {
    stop_requested: bool,
    fatal_error: Option<RuntimeError>,
}

impl SessionControl {
    pub fn stop(&mut self) {
        self.stop_requested = true;
    }

    pub const fn stop_requested(&self) -> bool {
        self.stop_requested
    }

    pub fn set_stop_requested(&mut self, requested: bool) {
        self.stop_requested = requested;
    }

    pub fn fail(&mut self, error: RuntimeError) {
        if self.fatal_error.is_none() {
            self.fatal_error = Some(error);
        }
        self.stop();
    }

    pub fn take_fatal_error(&mut self) -> Option<RuntimeError> {
        self.fatal_error.take()
    }
}

#[derive(Default)]
pub struct ArtifactSlot(Option<SessionArtifact>);

impl ArtifactSlot {
    pub fn set(&mut self, artifact: SessionArtifact) -> RuntimeResult<()> {
        if self.0.is_some() {
            return Err(RuntimeError::new("session artifact is already set"));
        }
        self.0 = Some(artifact);
        Ok(())
    }

    pub fn take(&mut self) -> Option<SessionArtifact> {
        self.0.take()
    }

    pub const fn is_present(&self) -> bool {
        self.0.is_some()
    }

    pub fn clear(&mut self) {
        self.0 = None;
    }
}

#[derive(Default)]
pub struct SessionProgress {
    shard: SessionShard,
    completed_rounds: u64,
}

impl SessionProgress {
    pub fn install_shard(&mut self, shard: SessionShard) -> RuntimeResult<()> {
        if shard.count == 0 || shard.index >= shard.count {
            return Err(RuntimeError::new("invalid session shard"));
        }
        self.shard = shard;
        Ok(())
    }

    pub const fn shard(&self) -> SessionShard {
        self.shard
    }

    pub fn record_completed_rounds(&mut self, count: u64) {
        self.completed_rounds = self.completed_rounds.saturating_add(count);
    }

    pub const fn completed_rounds(&self) -> u64 {
        self.completed_rounds
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchOutcome {
    RecoverableError,
    TimingViolation,
}

#[derive(Default)]
pub struct DispatchOutcomeState(Option<DispatchOutcome>);

impl DispatchOutcomeState {
    pub fn record(&mut self, outcome: DispatchOutcome) {
        if self.0 != Some(DispatchOutcome::TimingViolation) {
            self.0 = Some(outcome);
        }
    }

    pub fn take(&mut self) -> Option<DispatchOutcome> {
        self.0.take()
    }

    pub const fn get(&self) -> Option<DispatchOutcome> {
        self.0
    }

    pub fn set(&mut self, outcome: Option<DispatchOutcome>) {
        self.0 = outcome;
    }
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct RuntimeDispatchState {
    pub started: bool,
    pub registered: bool,
    pub opening_prepared: bool,
    pub opening_applied: bool,
    pub post_update_dispatched: bool,
    pub last_ui: Option<GameUi>,
    pub ever_played: bool,
}

impl RuntimeDispatchState {
    pub const fn new() -> Self {
        Self {
            started: false,
            registered: false,
            opening_prepared: false,
            opening_applied: false,
            post_update_dispatched: false,
            last_ui: None,
            ever_played: false,
        }
    }

    pub fn retain_current_ui_dispatch(&mut self, current_ui: Option<GameUi>) {
        self.post_update_dispatched &= self.last_ui == current_ui;
        self.last_ui = current_ui;
        self.ever_played |= current_ui == Some(GameUi::Playing);
    }

    pub fn prepare_reload(&mut self, mode: ReloadMode, boundary: ReloadBoundary) -> bool {
        if !mode.reloads_at(boundary) {
            return false;
        }
        self.registered = false;
        self.opening_prepared = false;
        self.opening_applied = false;
        self.post_update_dispatched = false;
        true
    }
}

#[must_use]
pub const fn waits_for_main_ui_reload(mode: ReloadMode, supported: bool) -> bool {
    supported && mode.reloads_at(ReloadBoundary::MainUi)
}

impl Default for RuntimeDispatchState {
    fn default() -> Self {
        Self::new()
    }
}

thread_local! {
    static SESSION_CONTROL: std::cell::RefCell<SessionControl> = std::cell::RefCell::new(SessionControl::default());
    static DISPATCH_OUTCOME: std::cell::RefCell<DispatchOutcomeState> = std::cell::RefCell::new(DispatchOutcomeState::default());
}

pub(crate) fn with_session_control<R>(f: impl FnOnce(&mut SessionControl) -> R) -> R {
    SESSION_CONTROL.with_borrow_mut(f)
}
pub(crate) fn with_session_control_ref<R>(f: impl FnOnce(&SessionControl) -> R) -> R {
    SESSION_CONTROL.with_borrow(f)
}
pub(crate) fn with_dispatch_outcome<R>(f: impl FnOnce(&mut DispatchOutcomeState) -> R) -> R {
    DISPATCH_OUTCOME.with_borrow_mut(f)
}
pub(crate) fn with_dispatch_outcome_ref<R>(f: impl FnOnce(&DispatchOutcomeState) -> R) -> R {
    DISPATCH_OUTCOME.with_borrow(f)
}

pub fn stop_script() {
    SESSION_CONTROL.with_borrow_mut(SessionControl::stop);
}

#[must_use]
pub fn script_stop_requested() -> bool {
    SESSION_CONTROL.with_borrow(SessionControl::stop_requested)
}

pub fn fail_script(error: RuntimeError) {
    SESSION_CONTROL.with_borrow_mut(|state| state.fail(error));
}

pub fn take_fatal_session_error() -> Option<RuntimeError> {
    SESSION_CONTROL.with_borrow_mut(SessionControl::take_fatal_error)
}

pub(crate) fn fatal_session_error() -> Option<RuntimeError> {
    SESSION_CONTROL.with_borrow(|state| {
        state
            .fatal_error
            .as_ref()
            .map(|error| RuntimeError::new(error.message().clone()))
    })
}

pub fn record_dispatch_outcome(outcome: DispatchOutcome) {
    DISPATCH_OUTCOME.with_borrow_mut(|state| state.record(outcome));
}

pub fn take_dispatch_outcome() -> Option<DispatchOutcome> {
    DISPATCH_OUTCOME.with_borrow_mut(DispatchOutcomeState::take)
}

mod current;
pub(crate) use current::{ARTIFACT, RESET_STATE, SESSION_JOB};
pub use current::{
    WorldResetRequest, WorldResetState, claim_session_job, completed_rounds, execute_pending_reset,
    forbid_additional_world_resets, install_session_shard, mark_world_replaced, record_completed_rounds,
    request_world_reset, request_world_reset_with, reset_session_control, session_shard, set_session_artifact,
    take_session_artifact, world_epoch, world_reset_pending,
};

#[doc(hidden)]
pub use current::take_next_opening_seed;
