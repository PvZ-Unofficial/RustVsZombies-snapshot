use rsvz_backend_api::backend::PlantEffectCountdownWriteBackend;
use rsvz_model::model::{NonNegativeI32, ObjectEditOutcome};

use crate::error::Result;
use crate::raw::layout as ptrs;
use crate::runtime::Pvz1051Backend;

impl PlantEffectCountdownWriteBackend for Pvz1051Backend {
    fn set_plant_effect_countdown<'a>(
        &'a self, handle: Self::PlantHandle<'a>, target_countdown: NonNegativeI32,
    ) -> Result<ObjectEditOutcome> {
        // SAFETY: handle was constructed from a live current-frame plant DataArray slot; this writes
        // only Plant::mDoSpecialCountdown.
        unsafe { ptrs::Plant::set_do_special_countdown(handle.ptr.as_ptr(), target_countdown.get()) };
        Ok(ObjectEditOutcome::Applied)
    }
}
