use super::*;

impl CobFireBackend for Pvz1051Backend {
    fn fire_cob<'a>(&'a self, cob: Self::PlantHandle<'a>, target: PixelPos) -> Result<()> {
        let plant = cob.ptr;
        if plant_effective_seed_type(plant)? != PvzPlantType::CobCannon {
            return Err(Pvz1051Error::KindUnavailable("plant is not CobCannon"));
        }
        // SAFETY: `plant` is current; CobCannon state 37 is the native ready state.
        if unsafe { ptrs::Plant::state(plant.as_ptr()) } != 37 {
            return Err(Pvz1051Error::AbiPreconditionFailed("cob cannon is not ready"));
        }
        // SAFETY: the borrowed handle is live, was checked as a ready CobCannon, and target
        // coordinates are copied scalar arguments to the objdump-verified native action.
        unsafe { crate::raw::abi::plant_cob_cannon_fire(plant.as_ptr(), target.x, target.y) };
        Ok(())
    }
}
