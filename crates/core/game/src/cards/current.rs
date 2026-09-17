//! Current card operations.
use crate::logic::cards as core;
use crate::logic::cards::IntoCardSelection;
use crate::runtime::{RuntimeError, RuntimeResult};
use rsvz_backend_api::{
    ChooserCooldownReadBackend, GameUiBackend, PlantEffectCountdownWriteBackend, PlantReadBackend,
    SeedCooldownReadBackend, SunCostRuleEditBackend,
};
use rsvz_current::CurrentBackend;
use rsvz_model::{Grid, PlantKind};

pub fn set_sun_cost_ignored(enabled: bool)
where
    CurrentBackend: SunCostRuleEditBackend,
{
    if let Err(error) = try_set_sun_cost_ignored(enabled) {
        if crate::registration::is_active() {
            crate::registration::record_error(error);
        } else {
            crate::diagnostics::report_operation_error(error);
        }
    }
}

pub use crate::modifier::set_sun_cost_ignored as try_set_sun_cost_ignored;

pub fn card_cd<S>(selection: S) -> i32
where
    CurrentBackend: GameUiBackend + SeedCooldownReadBackend + ChooserCooldownReadBackend,
    S: IntoCardSelection,
{
    let selection = selection.into_card_selection();
    let cooldown = core::card_cd(selection);
    crate::live_value::read_or_abort(
        super::cooldown_value(selection, cooldown, crate::registration::is_active()),
        "failed to read card cooldown",
    )
}

pub fn normalize_card_effect(kind: PlantKind, row: i32, col: i32)
where
    CurrentBackend: PlantReadBackend + PlantEffectCountdownWriteBackend,
{
    if let Err(error) = try_normalize_card_effect(kind, row, col) {
        crate::diagnostics::report_operation_error(error);
    }
}

pub fn try_normalize_card_effect(kind: PlantKind, row: i32, col: i32) -> RuntimeResult<()>
where
    CurrentBackend: PlantReadBackend + PlantEffectCountdownWriteBackend,
{
    let grid =
        Grid::from_one_based(row, col).map_err(|_error| RuntimeError::new(format!("invalid grid ({row}, {col})")))?;
    crate::modifier::normalize_effect_countdown_by_grid(kind, grid, crate::logic::card_timing::EFFECT_COUNTDOWN_TARGET)
        .map(|_outcome| ())
        .map_err(|error| RuntimeError::new(error.to_string()))
}

pub use crate::logic::cards::{
    card as try_card, card_any as try_card_any, card_slot as try_card_slot, card_slot_any as try_card_slot_any,
    cards as try_cards, cards_any_iter as try_cards_any_iter,
};

/// Adjusts the first active mushroom at `delay` frames from now, using AvZ's
/// late normalization and three-frame tolerance rather than changing activation early.
pub fn set_plant_active_time(kind: PlantKind, delay: i32) -> RuntimeResult<crate::timeline::TimeHandle>
where
    CurrentBackend: PlantReadBackend
        + PlantEffectCountdownWriteBackend
        + rsvz_backend_api::WaveTimingBackend
        + rsvz_backend_api::BoardReadinessBackend
        + GameUiBackend,
{
    if !matches!(kind, PlantKind::IceShroom | PlantKind::DoomShroom) {
        return Err(RuntimeError::new(
            "active time normalization requires ice or doom shroom",
        ));
    }
    let offset = crate::logic::active_time::active_time_normalize_delay(delay)
        .map_err(|error| RuntimeError::new(error.to_string()))?;
    let timing = crate::timing::wave_timing()?;
    let refresh = rsvz_schedule::timeline::with_timeline_ref(|timeline| {
        timeline
            .wave_clocks()
            .refresh_clock(timing.current_wave)
            .or_else(|| rsvz_schedule::timeline::current_wave_refresh_clock(timing))
    })
    .ok_or_else(|| RuntimeError::new("current wave refresh clock is unknown"))?;
    crate::timeline::try_at(timing.current_wave.0, timing.clock - refresh + offset, move || {
        crate::access::with_backend(|backend| -> RuntimeResult<()> {
            for plant in crate::live_value::read_or_abort(backend.plants(), "plants") {
                if crate::live_value::read_or_abort(backend.plant_raw_kind(plant), "plant kind") == kind
                    && backend.plant_state(plant) == 2
                {
                    let remaining = backend.plant_effect_countdown(plant);
                    if (i64::from(remaining) - 10).abs() > 3 {
                        // Native interactions (e.g. bungee grabbing coffee) can delay
                        // activation. AvZ logs and skips this correction, then continues.
                        let message = format!(
                            "active_time_skipped worker={} epoch={} kind={kind:?} id={} countdown={remaining}",
                            crate::session::session_shard().index,
                            crate::session::world_epoch(),
                            backend.plant_id(plant).raw(),
                        );
                        crate::diagnostics::emit_log(&crate::diagnostics::LogRecord::new(
                            crate::diagnostics::LogLevel::Debug,
                            &message,
                            crate::diagnostics::LogContext::Unscoped,
                        ));
                        return Ok(());
                    }
                    backend
                        .set_plant_effect_countdown(
                            plant,
                            rsvz_model::NonNegativeI32::new(10).expect("positive countdown"),
                        )
                        .map_err(|error| RuntimeError::new(error.to_string()))?;
                    break;
                }
            }
            Ok(())
        })
    })
}
