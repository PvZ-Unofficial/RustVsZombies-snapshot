//! Fixed native-to-runtime event instrumentation boundary.

use rsvz_model::model::{
    BeginPlantEffect, EventFrameStatus, EventInterest, EventToken, GargantuarAshHitFact, GargantuarSpawnedFact,
    HomeEntryFact, ImpThrownFact, PlantEffectAttemptFact, PlantEffectOutcome,
};

use crate::backend::Backend;

/// Copy-only callback table installed for one fight attempt.
///
/// These callbacks are runtime-owned function items. Backends must not retain
/// native object references or call them outside the installing thread.
#[derive(Clone, Copy)]
pub struct NativeEventSink {
    pub interest: EventInterest,
    pub begin_logic_frame: fn(board_epoch: u64, main_counter: i32),
    pub begin_plant_effect: fn(PlantEffectAttemptFact) -> BeginPlantEffect,
    pub finish_plant_effect: fn(EventToken, PlantEffectOutcome),
    pub emit_home_entry: fn(HomeEntryFact),
    pub emit_gargantuar_spawned: fn(GargantuarSpawnedFact),
    pub emit_imp_thrown: fn(ImpThrownFact),
    pub emit_gargantuar_ash_hit: fn(GargantuarAshHitFact),
    pub end_logic_frame: fn(EventFrameStatus),
}

/// Physical hook ownership needed by the generic runtime event model.
pub trait NativeEventBackend: Backend {
    fn install_native_event_sink(&mut self, sink: NativeEventSink) -> Result<(), Self::Error>;
    fn remove_native_event_sink(&mut self) -> Result<(), Self::Error>;
}
