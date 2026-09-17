use std::ptr::NonNull;

use crate::error::Result;
use crate::raw::kind::PvzPlantType;
use crate::raw::layout as ptrs;

pub(crate) fn effective_seed_type(plant: NonNull<ptrs::Plant>) -> Result<PvzPlantType> {
    // SAFETY: `plant` came from current Board plant storage; fields are copied scalars.
    let seed_type = PvzPlantType::from_raw(unsafe { ptrs::Plant::seed_type(plant.as_ptr()) })?;
    if seed_type != PvzPlantType::Imitator {
        return Ok(seed_type);
    }
    // SAFETY: same verified plant pointer.
    let imitater_type = unsafe { ptrs::Plant::imitater_type(plant.as_ptr()) };
    if imitater_type == super::seed::SEED_NONE {
        return Ok(seed_type);
    }
    PvzPlantType::from_raw(imitater_type)
}
