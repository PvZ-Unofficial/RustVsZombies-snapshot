//! Atomic native-state capabilities used by deterministic diagnostics.

use crate::backend::Backend;
use rsvz_model::model::{RandomMode, RandomStreamKind, SunProductionMode};

pub trait BackendIdentityBackend: Backend {
    fn backend_name(&self) -> &'static str;
    fn backend_version(&self) -> &'static str;
}

pub trait BoardStateBackend: Backend {
    fn natural_sun_generated(&self) -> Result<i32, Self::Error>;
    fn natural_sun_countdown(&self) -> Result<i32, Self::Error>;
    fn dancer_clock(&self) -> Result<u32, Self::Error>;
    fn ice_path_x(&self, row: u32) -> Result<i32, Self::Error>;
    fn ice_path_countdown(&self, row: u32) -> Result<u32, Self::Error>;
    fn spawn_allowed(&self, raw_zombie_kind: u32) -> Result<bool, Self::Error>;
    fn spawn_entry(&self, wave: u32, slot: u32) -> Result<i32, Self::Error>;
}

pub trait RandomControlBackend: Backend {
    fn set_random_mode(&mut self, mode: RandomMode) -> Result<(), Self::Error>;
    /// Uses an isolated Battle RNG seeded with `base_seed + zero-based wave`
    /// only while the native wave-spawn routine is running. `None` disables it.
    fn set_wave_spawn_random_seed(&mut self, base_seed: Option<u32>) -> Result<(), Self::Error>;
    fn random_seed(&self, stream: RandomStreamKind) -> Result<u32, Self::Error>;
    fn random_locked(&self, stream: RandomStreamKind) -> bool;
    fn random_fixed(&self, stream: RandomStreamKind) -> u32;
}

pub trait SunProductionModeBackend: Backend {
    fn set_sun_production_mode(&mut self, mode: SunProductionMode) -> Result<(), Self::Error>;
}
