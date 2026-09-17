use rsvz_backend_api::backend::CardAppendSelectionBackend;
use rsvz_model::model::CheckedCardSelection;

use crate::error::Result;
use crate::raw::kind::PvzPlantType;
use crate::runtime::Pvz1051Backend;

impl CardAppendSelectionBackend for Pvz1051Backend {
    fn select_card(&self, selection: CheckedCardSelection) -> Result<()> {
        self.ensure_level_intro_ready()?;
        let seed_chooser = self.seed_chooser()?;

        match selection.imitator_target() {
            None => {
                // SAFETY: UI state and seed chooser readiness were checked above; the wrapper
                // receives a typed PvZ 1051 seed enum.
                unsafe { crate::ops::seed::choose_card(seed_chooser, PvzPlantType::from(selection.packet_kind())) };
            }
            Some(kind) => {
                // SAFETY: UI state and seed chooser readiness were checked above; the wrapper
                // receives a typed PvZ 1051 imitator target enum.
                unsafe { crate::ops::seed::choose_imitator_card(seed_chooser, PvzPlantType::from(kind)) };
            }
        }
        Ok(())
    }
}
