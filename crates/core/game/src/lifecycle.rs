//! Pure runtime lifecycle state transitions.

pub use rsvz_schedule::state_hook::StateEvent;

/// Runtime session lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    Installing,
    Running,
    Finalizing,
    Done,
}

/// Script-generation lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenerationState {
    None,
    Building(u64),
    Active(u64),
}

/// Current chooser/fight attempt lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttemptState {
    Idle,
    Chooser,
    Playing,
    Closing,
}

/// One-shot BeforeExit gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeforeExitGate {
    Pending,
    FiringOrDone,
}

/// Invalid lifecycle transition.
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
#[error("invalid runtime lifecycle transition: {operation}")]
pub struct LifecycleError {
    operation: &'static str,
}

impl LifecycleError {
    const fn new(operation: &'static str) -> Self {
        Self { operation }
    }
}

/// Backend-neutral lifecycle state; physical runners retain orchestration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuntimeLifecycle {
    session: SessionState,
    generation: GenerationState,
    attempt: AttemptState,
    before_exit: BeforeExitGate,
    next_epoch: u64,
    tick_active: bool,
    hooks_installed: bool,
}

impl RuntimeLifecycle {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            session: SessionState::Installing,
            generation: GenerationState::None,
            attempt: AttemptState::Idle,
            before_exit: BeforeExitGate::Pending,
            next_epoch: 1,
            tick_active: false,
            hooks_installed: false,
        }
    }

    pub fn finish_installation(&mut self) -> Result<StateEvent, LifecycleError> {
        if self.session != SessionState::Installing {
            return Err(LifecycleError::new("finish installation"));
        }
        self.session = SessionState::Running;
        self.hooks_installed = true;
        Ok(StateEvent::AfterAttach)
    }

    pub fn begin_generation(&mut self) -> Result<(u64, StateEvent), LifecycleError> {
        if self.session != SessionState::Running
            || self.attempt != AttemptState::Idle
            || self.tick_active
            || !matches!(self.generation, GenerationState::None | GenerationState::Active(_))
        {
            return Err(LifecycleError::new("begin script generation"));
        }
        let epoch = self.next_epoch;
        self.next_epoch = self.next_epoch.wrapping_add(1);
        self.generation = GenerationState::Building(epoch);
        Ok((epoch, StateEvent::BeforeScript))
    }

    pub fn after_script(&self, epoch: u64) -> Result<StateEvent, LifecycleError> {
        if self.generation != GenerationState::Building(epoch) {
            return Err(LifecycleError::new("dispatch after script"));
        }
        Ok(StateEvent::AfterScript)
    }

    pub fn activate_generation(&mut self, epoch: u64) -> Result<(), LifecycleError> {
        if self.generation != GenerationState::Building(epoch) {
            return Err(LifecycleError::new("activate script generation"));
        }
        self.generation = GenerationState::Active(epoch);
        Ok(())
    }

    pub fn abort_generation(&mut self, epoch: u64) -> Result<(), LifecycleError> {
        if self.generation != GenerationState::Building(epoch) {
            return Err(LifecycleError::new("abort script generation"));
        }
        self.generation = GenerationState::None;
        Ok(())
    }

    pub fn enter_chooser(&mut self) -> Result<(), LifecycleError> {
        if self.session != SessionState::Running
            || !matches!(self.generation, GenerationState::Active(_))
            || self.attempt != AttemptState::Idle
        {
            return Err(LifecycleError::new("enter chooser"));
        }
        self.attempt = AttemptState::Chooser;
        Ok(())
    }

    /// Enters Playing from chooser or directly when injected into a battle.
    pub fn enter_fight(&mut self) -> Result<Option<StateEvent>, LifecycleError> {
        if self.session != SessionState::Running || !matches!(self.generation, GenerationState::Active(_)) {
            return Err(LifecycleError::new("enter fight"));
        }
        match self.attempt {
            AttemptState::Idle | AttemptState::Chooser => {
                self.attempt = AttemptState::Playing;
                Ok(Some(StateEvent::EnterFight))
            }
            AttemptState::Playing => Ok(None),
            AttemptState::Closing => Err(LifecycleError::new("enter fight while closing")),
        }
    }

    /// Starts closing chooser or fight state. Chooser abort deliberately emits
    /// ExitFight without a preceding EnterFight.
    pub fn begin_attempt_close(&mut self) -> Result<Option<StateEvent>, LifecycleError> {
        if self.tick_active {
            return Err(LifecycleError::new("close attempt during tick"));
        }
        match self.attempt {
            AttemptState::Idle => Ok(None),
            AttemptState::Chooser | AttemptState::Playing => {
                self.attempt = AttemptState::Closing;
                Ok(Some(StateEvent::ExitFight))
            }
            AttemptState::Closing => Err(LifecycleError::new("close attempt twice")),
        }
    }

    pub fn finish_attempt_close(&mut self) -> Result<(), LifecycleError> {
        if self.attempt != AttemptState::Closing {
            return Err(LifecycleError::new("finish attempt close"));
        }
        self.attempt = AttemptState::Idle;
        Ok(())
    }

    pub fn begin_tick(&mut self) -> Result<StateEvent, LifecycleError> {
        if self.session != SessionState::Running || self.attempt != AttemptState::Playing || self.tick_active {
            return Err(LifecycleError::new("begin tick"));
        }
        self.tick_active = true;
        Ok(StateEvent::BeforeTick)
    }

    pub fn finish_tick(&mut self) -> Result<StateEvent, LifecycleError> {
        if !self.tick_active {
            return Err(LifecycleError::new("finish tick"));
        }
        self.tick_active = false;
        Ok(StateEvent::AfterTick)
    }

    /// Abandons a fatal/panicking tick without fabricating AfterTick.
    pub fn abort_tick(&mut self) -> Result<(), LifecycleError> {
        if !self.tick_active {
            return Err(LifecycleError::new("abort tick"));
        }
        self.tick_active = false;
        Ok(())
    }

    /// Begins finalization and closes the BeforeExit gate before user code.
    pub fn begin_finalize(&mut self) -> Result<Option<StateEvent>, LifecycleError> {
        if self.tick_active || self.attempt != AttemptState::Idle {
            return Err(LifecycleError::new("begin finalize before attempt closed"));
        }
        match self.session {
            SessionState::Installing | SessionState::Running => self.session = SessionState::Finalizing,
            SessionState::Finalizing | SessionState::Done => {
                return Err(LifecycleError::new("begin finalize twice"));
            }
        }
        if self.before_exit == BeforeExitGate::Pending {
            self.before_exit = BeforeExitGate::FiringOrDone;
            if self.hooks_installed {
                return Ok(Some(StateEvent::BeforeExit));
            }
        }
        Ok(None)
    }

    pub fn finish_finalize(&mut self) -> Result<(), LifecycleError> {
        if self.session != SessionState::Finalizing {
            return Err(LifecycleError::new("finish finalize"));
        }
        self.session = SessionState::Done;
        self.generation = GenerationState::None;
        Ok(())
    }

    #[must_use]
    pub const fn session(self) -> SessionState {
        self.session
    }

    #[must_use]
    pub const fn generation(self) -> GenerationState {
        self.generation
    }

    #[must_use]
    pub const fn attempt(self) -> AttemptState {
        self.attempt
    }

    #[must_use]
    pub const fn before_exit_gate(self) -> BeforeExitGate {
        self.before_exit
    }
}

impl Default for RuntimeLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push(trace: &mut Vec<StateEvent>, event: Option<StateEvent>) {
        trace.extend(event);
    }

    #[test]
    fn normal_session_has_the_complete_golden_trace() {
        let mut lifecycle = RuntimeLifecycle::new();
        let mut trace = vec![lifecycle.finish_installation().expect("install")];
        let (epoch, before_script) = lifecycle.begin_generation().expect("generation");
        trace.push(before_script);
        trace.push(lifecycle.after_script(epoch).expect("after script"));
        lifecycle.activate_generation(epoch).expect("activate");
        lifecycle.enter_chooser().expect("chooser");
        push(&mut trace, lifecycle.enter_fight().expect("fight"));
        trace.push(lifecycle.begin_tick().expect("before tick"));
        trace.push(lifecycle.finish_tick().expect("after tick"));
        push(&mut trace, lifecycle.begin_attempt_close().expect("exit fight"));
        lifecycle.finish_attempt_close().expect("close");
        push(&mut trace, lifecycle.begin_finalize().expect("before exit"));
        lifecycle.finish_finalize().expect("done");

        assert_eq!(
            trace,
            [
                StateEvent::AfterAttach,
                StateEvent::BeforeScript,
                StateEvent::AfterScript,
                StateEvent::EnterFight,
                StateEvent::BeforeTick,
                StateEvent::AfterTick,
                StateEvent::ExitFight,
                StateEvent::BeforeExit,
            ]
        );
    }

    #[test]
    fn chooser_abort_multi_attempt_and_failed_generation_keep_fixed_semantics() {
        let mut lifecycle = RuntimeLifecycle::new();
        let _after_attach = lifecycle.finish_installation().expect("install");
        let (failed_epoch, _) = lifecycle.begin_generation().expect("failed generation");
        lifecycle.abort_generation(failed_epoch).expect("abort");
        let (epoch, _) = lifecycle.begin_generation().expect("next generation");
        assert!(epoch > failed_epoch);
        let _after_script = lifecycle.after_script(epoch).expect("after script");
        lifecycle.activate_generation(epoch).expect("activate");

        lifecycle.enter_chooser().expect("chooser");
        assert_eq!(
            lifecycle.begin_attempt_close().expect("chooser close"),
            Some(StateEvent::ExitFight)
        );
        lifecycle.finish_attempt_close().expect("chooser closed");

        assert_eq!(
            lifecycle.enter_fight().expect("already playing injection"),
            Some(StateEvent::EnterFight)
        );
        assert_eq!(lifecycle.enter_fight().expect("same fight"), None);
        assert_eq!(
            lifecycle.begin_attempt_close().expect("trial close"),
            Some(StateEvent::ExitFight)
        );
        lifecycle.finish_attempt_close().expect("trial closed");
        assert_eq!(lifecycle.session(), SessionState::Running);
        assert_eq!(lifecycle.before_exit_gate(), BeforeExitGate::Pending);
    }

    #[test]
    fn before_exit_is_suppressed_for_failed_install_and_never_retried() {
        let mut failed_install = RuntimeLifecycle::new();
        assert_eq!(failed_install.begin_finalize().expect("finalize"), None);
        failed_install.finish_finalize().expect("done");

        let mut installed = RuntimeLifecycle::new();
        let _after_attach = installed.finish_installation().expect("install");
        assert_eq!(
            installed.begin_finalize().expect("first finalize"),
            Some(StateEvent::BeforeExit)
        );
        assert!(installed.begin_finalize().is_err());
        assert_eq!(installed.before_exit_gate(), BeforeExitGate::FiringOrDone);
    }
}

mod current;
pub(crate) use current::lifecycle_error;
pub use current::{
    abort_logic_tick, begin_logic_tick, close_attempt, enter_chooser, enter_fight, finalize_session,
    finish_hook_installation, finish_logic_tick,
};

use std::cell::RefCell;

thread_local! {
    static LIFECYCLE: RefCell<RuntimeLifecycle> = const { RefCell::new(RuntimeLifecycle::new()) };
}

pub fn with_lifecycle<R>(f: impl FnOnce(&mut RuntimeLifecycle) -> R) -> R {
    LIFECYCLE.with_borrow_mut(f)
}

pub fn reset_session_resources() {
    crate::diagnostics::reset_logger();
    LIFECYCLE.with_borrow_mut(|lifecycle| *lifecycle = RuntimeLifecycle::new());
}

#[doc(hidden)]
pub fn install_framework_state_hooks() -> crate::runtime::RuntimeResult<()>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::AutoCollectBackend
        + rsvz_backend_api::ClockBackend
        + rsvz_backend_api::FastForwardBackend
        + rsvz_backend_api::ItemClickCollectBackend
        + rsvz_backend_api::ItemReadBackend
        + rsvz_backend_api::WaveTimingBackend,
{
    let _handle = crate::state_hook::register_fallible(
        rsvz_schedule::state_hook::StateEvent::AfterScript,
        -1_000,
        crate::fast_forward::register_fast_forward_window_task,
    );
    if !<rsvz_current::CurrentBackend as crate::backend::AutoCollectBackend>::AUTO_COLLECT_IS_NOOP
        || <rsvz_current::CurrentBackend as crate::backend::ItemReadBackend>::ITEMS_CAN_EXIST
    {
        let _handle =
            crate::state_hook::register_fallible(rsvz_schedule::state_hook::StateEvent::AfterScript, -900, || {
                crate::auto_collect::register_tick()?;
                crate::auto_collect::register_click_tick()
            });
    }
    Ok(())
}
