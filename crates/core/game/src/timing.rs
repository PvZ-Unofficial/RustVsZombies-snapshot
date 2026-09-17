//! Current timing values composed from native scalar reads.
use crate::runtime::RuntimeResult;
use rsvz_backend_api::{ClockBackend, CurrentWaveBackend, WaveTimingBackend};
use rsvz_current::CurrentBackend;
use rsvz_model::WaveTimingSnapshot;

pub fn wave_timing() -> RuntimeResult<WaveTimingSnapshot>
where
    CurrentBackend: WaveTimingBackend,
{
    rsvz_current::with_backend_shared(|access| {
        let backend = access;
        Ok(WaveTimingSnapshot {
            clock: backend.clock()?,
            current_wave: backend.current_wave()?,
            total_waves: Some(crate::live_value::read_or_abort(backend.total_waves(), "total_waves")),
            refresh_countdown: Some(backend.refresh_countdown()?),
            initial_countdown: Some(backend.initial_countdown()?),
            huge_wave_countdown: Some(backend.huge_wave_countdown()?),
            level_end_countdown: Some(backend.level_end_countdown()?),
        })
    })
    .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))?
}

/// Reads the current refresh countdown; a necessary read failure ends the current callback.
pub fn refresh_countdown() -> i32
where
    CurrentBackend: WaveTimingBackend,
{
    crate::live_value::read_or_abort(
        rsvz_current::with_backend_shared(|access| access.refresh_countdown())
            .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))
            .and_then(|result| result.map_err(Into::into)),
        "failed to read refresh countdown",
    )
}
