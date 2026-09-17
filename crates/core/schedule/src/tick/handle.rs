use std::num::NonZeroU64;

use super::{TickAvailability, TickLane, TickPriority};

/// Stable token identifying a task in a scheduler.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TickHandle {
    raw: TickTaskKey,
}

impl TickHandle {
    pub(crate) const fn new(
        availability: TickAvailability, lane: TickLane, priority: TickPriority, slot: usize, generation: NonZeroU64,
    ) -> Self {
        Self {
            raw: TickTaskKey {
                availability,
                lane,
                priority,
                slot,
                generation,
            },
        }
    }

    pub(crate) const fn key(self) -> TickTaskKey {
        self.raw
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TickTaskKey {
    pub(crate) availability: TickAvailability,
    pub(crate) lane: TickLane,
    pub(crate) priority: TickPriority,
    pub(crate) slot: usize,
    pub(crate) generation: NonZeroU64,
}
