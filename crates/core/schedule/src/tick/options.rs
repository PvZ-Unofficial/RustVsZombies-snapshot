use super::{TickAvailability, TickFrameGate, TickLane, TickLifetime, TickPhase, TickPriority, TickTrigger, TickWhen};

/// Task registration options.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TickOptions {
    /// Dispatch availability.
    pub availability: TickAvailability,
    /// Lifecycle boundary that removes the task.
    pub lifetime: TickLifetime,
    /// Frame-level gate.
    pub frame_gate: TickFrameGate,
    /// Trigger mode.
    pub trigger: TickTrigger,
    /// Dispatch lane.
    pub lane: TickLane,
    /// Bucket priority. Larger values run earlier.
    pub priority: TickPriority,
    /// Optional diagnostic name.
    pub name: Option<&'static str>,
    /// Whether this task should be ignored when checking scheduler idleness.
    pub idle_neutral: bool,
}

impl TickOptions {
    const fn default_dispatch(availability: TickAvailability, frame_gate: TickFrameGate, trigger: TickTrigger) -> Self {
        Self {
            availability,
            lifetime: TickLifetime::Script,
            frame_gate,
            trigger,
            lane: TickLane::Act,
            priority: TickPriority::NORMAL,
            name: None,
            idle_neutral: false,
        }
    }

    /// Creates default playing-frame options.
    #[must_use]
    pub const fn new() -> Self {
        Self::playing_frame()
    }

    /// Runs once per new playing frame.
    #[must_use]
    pub const fn playing_frame() -> Self {
        Self::default_dispatch(
            TickAvailability::Playing,
            TickFrameGate::NewPlayingFrame,
            TickTrigger::Repeating,
        )
    }

    /// Runs in ready level-intro and playing phases.
    #[must_use]
    pub const fn active_phase() -> Self {
        Self::default_dispatch(
            TickAvailability::Active,
            TickFrameGate::EveryDispatch,
            TickTrigger::Repeating,
        )
    }

    /// Runs on every backend dispatch.
    #[must_use]
    pub const fn any_dispatch() -> Self {
        Self::default_dispatch(
            TickAvailability::AnyDispatch,
            TickFrameGate::EveryDispatch,
            TickTrigger::Repeating,
        )
    }

    /// Runs once when level intro is ready after registration.
    #[must_use]
    pub const fn once_level_intro_ready() -> Self {
        Self::default_dispatch(
            TickAvailability::Active,
            TickFrameGate::EveryDispatch,
            TickTrigger::OnceReady(TickPhase::LevelIntro),
        )
    }

    /// Runs once when playing is ready after registration.
    #[must_use]
    pub const fn once_playing_ready() -> Self {
        Self::default_dispatch(
            TickAvailability::Active,
            TickFrameGate::EveryDispatch,
            TickTrigger::OnceReady(TickPhase::Playing),
        )
    }

    /// Changes dispatch eligibility using the legacy combined selector.
    #[must_use]
    pub const fn when(mut self, when: TickWhen) -> Self {
        self.availability = when.availability();
        self.frame_gate = when.frame_gate();
        self
    }

    /// Changes dispatch availability.
    #[must_use]
    pub const fn availability(mut self, availability: TickAvailability) -> Self {
        self.availability = availability;
        self
    }

    /// Changes the task lifetime.
    #[must_use]
    pub const fn lifetime(mut self, lifetime: TickLifetime) -> Self {
        self.lifetime = lifetime;
        self
    }

    /// Changes frame-level gate.
    #[must_use]
    pub const fn frame_gate(mut self, frame_gate: TickFrameGate) -> Self {
        self.frame_gate = frame_gate;
        self
    }

    /// Changes trigger mode.
    #[must_use]
    pub const fn trigger(mut self, trigger: TickTrigger) -> Self {
        self.trigger = trigger;
        self
    }

    /// Changes dispatch lane.
    #[must_use]
    pub const fn lane(mut self, lane: TickLane) -> Self {
        self.lane = lane;
        self
    }

    /// Changes priority.
    #[must_use]
    pub const fn priority(mut self, priority: TickPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Changes diagnostic name.
    #[must_use]
    pub const fn name(mut self, name: &'static str) -> Self {
        self.name = Some(name);
        self
    }

    /// Marks the task as not blocking scheduler idle.
    #[must_use]
    pub const fn idle_neutral(mut self) -> Self {
        self.idle_neutral = true;
        self
    }
}

impl Default for TickOptions {
    fn default() -> Self {
        Self::new()
    }
}
