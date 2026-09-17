use rsvz_backend_api::backend::{PlantCostBackend, SeedPacketBackend, SunMoneyBackend};
use rsvz_model::model::{CheckedCardSelection, SunAmount};

use crate::error::{Pvz1051Error, Result};
use crate::raw::kind::PvzPlantType;
use crate::raw::layout as ptrs;
use crate::runtime::Pvz1051Backend;

impl SeedPacketBackend for Pvz1051Backend {
    fn seed_can_pick_up<'a>(&'a self, seed: Self::SeedHandle<'a>) -> Result<bool> {
        self.ensure_playing()?;
        // SAFETY: the handle points into the current SeedBank and CanPickUp's ESI ABI was
        // objdump-verified. The return is the native AL boolean.
        Ok(unsafe { crate::raw::abi::seed_packet_can_pick_up(seed.ptr.as_ptr()) } != 0)
    }

    fn seed_was_planted<'a>(&'a self, seed: Self::SeedHandle<'a>) -> Result<()> {
        self.ensure_playing()?;
        let seed = seed.ptr.as_ptr();
        // SAFETY: the handle points into the current SeedBank. Native mouse planting first
        // deactivates the packet at 0x00488E71..=0x00488E7D, then WasPlanted starts its new
        // recharge. WasPlanted takes SeedPacket* in EAX with no stack arguments.
        unsafe {
            ptrs::SeedPacket::deactivate(seed);
            crate::raw::abi::seed_packet_was_planted(seed);
        }
        Ok(())
    }
}

impl PlantCostBackend for Pvz1051Backend {
    fn current_plant_cost(&self, selection: CheckedCardSelection) -> Result<SunAmount> {
        self.ensure_playing()?;
        let packet_type = PvzPlantType::from(selection.packet_kind());
        let imitater_type = selection
            .imitator_target()
            .map_or(-1, |target| PvzPlantType::from(target).raw());
        // SAFETY: packet/imitater kinds were validated and the raw EDI/EAX/EDX mapping was
        // objdump-verified for the current Board.
        let cost = unsafe { crate::raw::abi::board_get_current_plant_cost(packet_type.raw(), imitater_type) };
        let cost =
            u32::try_from(cost).map_err(|_error| Pvz1051Error::InvariantViolated("current plant cost is negative"))?;
        SunAmount::new(cost).map_err(|_error| Pvz1051Error::InvariantViolated("current plant cost exceeds i32"))
    }
}

impl SunMoneyBackend for Pvz1051Backend {
    fn can_take_sun_money(&self, amount: SunAmount) -> Result<bool> {
        self.ensure_playing()?;
        let board = self.board()?;
        // SAFETY: board is current and amount is representable; EDX/stack/AL ABI was objdump-verified.
        Ok(unsafe { crate::raw::abi::board_can_take_sun_money(board.as_ptr(), amount.get()) } != 0)
    }

    fn take_sun_money(&self, amount: SunAmount) -> Result<bool> {
        self.ensure_playing()?;
        let board = self.board()?;
        // SAFETY: board is current and amount is representable; EDI/EBX/AL ABI was objdump-verified.
        Ok(unsafe { crate::raw::abi::board_take_sun_money(board.as_ptr(), amount.get()) } != 0)
    }
}
