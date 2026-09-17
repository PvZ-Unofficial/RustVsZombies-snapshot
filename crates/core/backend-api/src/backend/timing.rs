//! Backend wave timing capability.

use crate::backend::{Backend, ClockBackend, CurrentWaveBackend};
use rsvz_model::model::Wave;

/// Independent wave timing values read from the current board.
pub trait WaveTimingBackend: ClockBackend + CurrentWaveBackend {
    fn total_waves(&self) -> Result<i32, Self::Error>;
    fn refresh_countdown(&self) -> Result<i32, Self::Error>;
    fn initial_countdown(&self) -> Result<i32, Self::Error>;
    fn huge_wave_countdown(&self) -> Result<i32, Self::Error>;
    fn level_end_countdown(&self) -> Result<i32, Self::Error>;
}

/// Ordinary current-wave health scalars used by refresh analysis.
pub trait WaveHealthBackend: Backend {
    fn zombie_health_wave_start(&self) -> Result<i32, Self::Error>;
    fn total_zombies_health_in_wave(&self, wave: i32) -> Result<i32, Self::Error>;
}

/// Safe-facing backend capability for atomically committing the current
/// timer-only wave-refresh state.
pub trait WaveRefreshControlBackend: Backend {
    /// Commits the timer-only refresh fields for the expected current wave.
    ///
    /// The backend validates every precondition before the first write and
    /// derives the remaining countdown from `initial_countdown`.
    fn commit_timer_only_wave_refresh(
        &self, expected_current_wave: Wave, initial_countdown: i32,
    ) -> Result<(), Self::Error>;
}
