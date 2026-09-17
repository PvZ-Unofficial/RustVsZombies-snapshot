//! Battle gameplay capability traits.

use crate::backend::{Backend, SceneBackend};
use rsvz_model::model::{SceneKind, SpawnWaveSlot, SunAmount, WorldResetConfig, ZombieKind};

/// Native sun-money admission and commit actions.
pub trait SunMoneyBackend: Backend {
    fn can_take_sun_money(&self, amount: SunAmount) -> Result<bool, Self::Error>;
    fn take_sun_money(&self, amount: SunAmount) -> Result<bool, Self::Error>;
}

/// Removes all lawn-mower objects while preserving the native bonus-mower stock.
///
/// Backends whose world model has no lawn mowers satisfy this capability with a no-op.
pub trait LawnMowerClearBackend: Backend {
    fn clear_lawn_mowers(&self) -> Result<(), Self::Error>;
}

/// Capability for checking whether destructive board writes are currently supported.
pub trait BoardSupportBackend: Backend {
    fn ensure_board_supported(&self) -> Result<(), Self::Error>;
}

/// Opening scene-switch capability.
///
/// Implementations may rebuild scene-derived terrain and clear scene objects,
/// but a scene switch is not a full world reset. Unrelated clock, randomness,
/// spawn-rule, card, and modifier state must remain intact.
pub trait SceneEditBackend: SceneBackend {
    fn set_scene(&mut self, scene: SceneKind) -> Result<(), Self::Error>;
}

/// Spawn schedule read/write capability.
pub trait SpawnScheduleBackend: Backend {
    fn spawn_wave_count(&self) -> Result<usize, Self::Error>;
    fn set_spawn_type_allowed(&self, kind: ZombieKind, allowed: bool) -> Result<(), Self::Error>;
    /// Writes one schedule slot; `None` is the game's native empty sentinel.
    fn set_spawn_slot(&self, wave: usize, slot: SpawnWaveSlot, kind: Option<ZombieKind>) -> Result<(), Self::Error>;
    /// Runs the game's native weighted spawn-list builder using the current
    /// allowed-type flags.
    fn pick_spawn_list(&self) -> Result<(), Self::Error>;
    /// Rebuilds any native level-intro preview after the schedule changes.
    fn refresh_spawn_preview(&self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Rebuilds a fresh world without advancing its first frame.
pub trait WorldResetBackend: Backend {
    fn reset_world(&mut self, config: WorldResetConfig) -> Result<(), Self::Error>;
}

/// Whether a native object pool can allocate one ordinary slot.
#[must_use]
pub const fn pool_has_free_slot(active: u32, capacity: u32) -> bool {
    active < capacity
}

/// Whether one slot remains reserved after the next native allocation.
///
/// Some composite native atoms allocate and immediately dereference an object
/// while their inner allocator deliberately keeps the final slot unused.
#[must_use]
pub const fn pool_has_reserved_slot(active: u32, capacity: u32) -> bool {
    match active.checked_add(1) {
        Some(next) => next < capacity,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::{pool_has_free_slot, pool_has_reserved_slot};

    #[test]
    fn pool_capacity_predicates_cover_empty_full_reserved_and_overflow_boundaries() {
        assert!(pool_has_free_slot(0, 1));
        assert!(!pool_has_free_slot(1, 1));
        assert!(pool_has_reserved_slot(0, 2));
        assert!(!pool_has_reserved_slot(1, 2));
        assert!(!pool_has_reserved_slot(0, 1));
        assert!(!pool_has_reserved_slot(u32::MAX, u32::MAX));
    }
}
