//! Current tick scheduling.

pub use rsvz_game::tick::{on_frame, spawn, spawn_frame};
pub use rsvz_schedule::tick::{
    TickAvailability, TickCommandOutcome, TickControl, TickFrameGate, TickHandle, TickLane, TickLifetime, TickMeta,
    TickOptions, TickPhase, TickPriority, TickPriorityError, TickTaskState, TickTrigger, TickWhen,
};
