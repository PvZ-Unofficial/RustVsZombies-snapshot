use std::fmt;

use crate::model::GameUi;

/// Selects which backend dispatches may run a task.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TickWhen {
    /// Run once per new frame while the battle board is ready.
    PlayingFrame,
    /// Run while either the level-intro or battle phase is ready.
    ActivePhase,
    /// Run on every scheduler dispatch, including menu/not-ready phases.
    AnyDispatch,
}

impl TickWhen {
    pub(crate) const fn availability(self) -> TickAvailability {
        match self {
            Self::PlayingFrame => TickAvailability::Playing,
            Self::ActivePhase => TickAvailability::Active,
            Self::AnyDispatch => TickAvailability::AnyDispatch,
        }
    }

    pub(crate) const fn frame_gate(self) -> TickFrameGate {
        match self {
            Self::PlayingFrame => TickFrameGate::NewPlayingFrame,
            Self::ActivePhase | Self::AnyDispatch => TickFrameGate::EveryDispatch,
        }
    }
}

/// Phase availability that decides which dispatches may run a task.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TickAvailability {
    /// Run on every scheduler dispatch; game objects might not be ready.
    AnyDispatch,
    /// Run while either level intro or playing state is ready.
    Active,
    /// Run only while the playing board is ready.
    Playing,
}

impl TickAvailability {
    pub(crate) const COUNT: usize = 3;
    pub(crate) const ALL: [Self; Self::COUNT] = [Self::AnyDispatch, Self::Active, Self::Playing];

    pub(crate) const fn index(self) -> usize {
        match self {
            Self::AnyDispatch => 0,
            Self::Active => 1,
            Self::Playing => 2,
        }
    }
}

/// Lifecycle boundary that removes a task.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TickLifetime {
    /// Keep the task until the runtime session exits.
    Session,
    /// Keep the task until the next script generation starts.
    #[default]
    Script,
    /// Keep the task until the current fight attempt exits.
    Fight,
}

/// Frame-level gate applied after a task's availability is ready.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TickFrameGate {
    /// Run on every backend dispatch while the availability is ready.
    EveryDispatch,
    /// Run only on a new playing frame.
    NewPlayingFrame,
}

/// Callback trigger mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TickTrigger {
    /// Run every time availability and frame gate are eligible.
    Repeating,
    /// Run once when the requested phase is ready, then stop.
    OnceReady(TickPhase),
}

/// Coarse stage within a single tick dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TickLane {
    /// Framework-owned tasks, such as future timeline drivers.
    System,
    /// Observation and cache update tasks.
    Observe,
    /// Ordinary game actions. This is the default user lane.
    Act,
    /// Cleanup, reporting, and low-priority maintenance.
    After,
}

impl TickLane {
    pub(crate) const COUNT: usize = 4;
    pub(crate) const ALL: [Self; Self::COUNT] = [Self::System, Self::Observe, Self::Act, Self::After];

    pub(crate) const fn index(self) -> usize {
        match self {
            Self::System => 0,
            Self::Observe => 1,
            Self::Act => 2,
            Self::After => 3,
        }
    }
}

/// Control returned by a tick callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TickControl {
    /// Keep the task registered.
    Continue,
    /// Keep the task registered but pause it until its handle is resumed.
    Pause,
    /// Stop the task after the current callback returns.
    Stop,
}

/// Current task lifecycle state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TickTaskState {
    /// The task is eligible to run when its phase gate matches.
    Running,
    /// The task is retained but skipped.
    Paused,
    /// The task has stopped or the handle no longer resolves.
    Stopped,
}

/// Priority outside the supported `-20..=20` bucket range.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TickPriorityError {
    value: i8,
}

impl TickPriorityError {
    /// Returns the rejected priority value.
    #[must_use]
    pub const fn value(self) -> i8 {
        self.value
    }
}

impl fmt::Display for TickPriorityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "tick priority {} is outside the supported range {}..={}",
            self.value,
            TickPriority::MIN,
            TickPriority::MAX
        )
    }
}

impl std::error::Error for TickPriorityError {}

/// Ordered task priority. Larger values run earlier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TickPriority(i8);

impl TickPriority {
    /// Lowest supported priority.
    pub const MIN: i8 = -20;
    /// Highest supported priority.
    pub const MAX: i8 = 20;
    pub(crate) const FINALIZER: Self = Self(Self::MIN - 1);
    pub(crate) const BUCKETS: usize = (Self::MAX - Self::FINALIZER.0 + 1) as usize;

    /// Low task priority.
    pub const LOW: Self = Self(-10);
    /// Default task priority.
    pub const NORMAL: Self = Self(0);
    /// High task priority.
    pub const HIGH: Self = Self(10);
    /// Highest task priority.
    pub const CRITICAL: Self = Self(20);

    /// Migration alias for the old lower-first API.
    pub const EARLY: Self = Self::HIGH;
    /// Migration alias for the old default priority.
    pub const DEFAULT: Self = Self::NORMAL;
    /// Migration alias for the old lower-first API.
    pub const LATE: Self = Self::LOW;

    /// Creates a priority if the value is within `-20..=20`.
    pub const fn try_new(value: i8) -> Result<Self, TickPriorityError> {
        if value < Self::MIN || value > Self::MAX {
            Err(TickPriorityError { value })
        } else {
            Ok(Self(value))
        }
    }

    /// Creates a priority, panicking when the value is outside `-20..=20`.
    ///
    /// # Panics
    ///
    /// Panics if `value` is outside the supported priority range.
    #[must_use]
    pub const fn new(value: i8) -> Self {
        match Self::try_new(value) {
            Ok(priority) => priority,
            Err(_) => panic!("tick priority is outside the supported range -20..=20"),
        }
    }

    /// Returns the raw priority value.
    #[must_use]
    pub const fn value(self) -> i8 {
        self.0
    }

    pub(crate) const fn bucket_index(self) -> usize {
        (Self::MAX - self.0) as usize
    }

    pub(crate) const fn from_bucket_index(index: usize) -> Self {
        Self(Self::MAX - index as i8)
    }
}

/// Backend-neutral phase visible to tick tasks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TickPhase {
    /// No app/root object is currently available.
    Unavailable,
    /// A known UI exists but its phase-local data is not ready yet.
    NotReady,
    /// Menu or loading UI.
    Menu,
    /// Level-intro board is ready.
    LevelIntro,
    /// Battle board is ready.
    Playing,
    /// A completed round is being settled; no battle frame or ready opening is dispatched.
    RoundComplete,
    /// The current battle has finished.
    Finished,
    /// Any backend-specific phase that has no core meaning.
    Other,
}

/// Metadata supplied by a backend driver for one dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TickMeta {
    /// Current backend-neutral phase.
    pub phase: TickPhase,
    /// Current PvZ UI when the backend has one.
    pub game_ui: Option<GameUi>,
    /// Current frame clock when the backend has one.
    pub clock: Option<i32>,
    /// Whether this dispatch observes a new logical frame.
    pub is_new_frame: bool,
}

impl TickMeta {
    /// Metadata for tests or drivers with no active game state.
    #[must_use]
    pub const fn unavailable() -> Self {
        Self {
            phase: TickPhase::Unavailable,
            game_ui: None,
            clock: None,
            is_new_frame: false,
        }
    }
}

/// Outcome of a scheduler command.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TickCommandOutcome {
    /// The command changed task state.
    Applied,
    /// No slot exists for the handle.
    NotFound,
    /// The slot exists but now belongs to a newer generation.
    StaleHandle,
    /// The task was already paused.
    AlreadyPaused,
    /// The task was already running.
    AlreadyRunning,
    /// The task was already stopped.
    AlreadyStopped,
    /// The command was rejected by the host runtime.
    Rejected,
}
