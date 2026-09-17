use rsvz_backend_api::backend::{
    CobRuleEditBackend, DancerClockWriteBackend, DropRuleEditBackend, MaidCheatsBackend, PlantDamageRuleEditBackend,
    PlantEffectRuleEditBackend, PlantingRuleEditBackend, ProfileReadonlyBackend, ProjectileRuleEditBackend,
    SeedRuleEditBackend, SunCostRuleEditBackend, SunWriteBackend, VisibilityEditBackend, ZombieRuleEditBackend,
};
use rsvz_model::model::{KernelPultProjectileRule, MaidCheat, NonNegativeI32, PlantDamageRule};

use crate::error::{Pvz1051Error, Result};
use crate::patches;
use crate::patches::leases::{self, BoolPatchId};
use crate::raw::layout as ptrs;
use crate::runtime::Pvz1051Backend;

impl rsvz_backend_api::DanceModeBackend for Pvz1051Backend {
    fn set_dance_mode(&self, enabled: bool) -> Result<()> {
        let board = self.board()?;
        // SAFETY: current Board, verified BL/stack ABI; the call does not recycle entities.
        unsafe { crate::raw::abi::board_set_dance_mode(board.as_ptr(), i32::from(enabled)) };
        Ok(())
    }
}

macro_rules! bool_patch_methods {
    ($get:ident, $set:ident, $id:ident) => {
        fn $get(&self) -> Result<bool> {
            BoolPatchId::$id.enabled()
        }

        fn $set(&self, enabled: bool) -> Result<()> {
            leases::set_bool_patch(BoolPatchId::$id, enabled)
        }
    };
}

impl SunWriteBackend for Pvz1051Backend {
    fn set_sun(&self, value: u32) -> Result<()> {
        let board = self.board()?;
        // SAFETY: `board` is non-null and the scalar write is limited to Board::mSunMoney.
        unsafe { ptrs::Board::set_sun(board.as_ptr(), value.min(i32::MAX as u32) as i32) };
        Ok(())
    }
}

impl DancerClockWriteBackend for Pvz1051Backend {
    fn set_dancer_clock(&self, value: u32) -> Result<()> {
        let app = self.app();
        // SAFETY: `app` is current and this writes only LawnApp::mAppCounter, the scalar read by
        // Zombie::GetDancerFrame.
        unsafe { *ptrs::LawnApp::app_counter_mut(app.as_ptr()) = value as i32 };
        Ok(())
    }
}

impl SeedRuleEditBackend for Pvz1051Backend {
    bool_patch_methods!(seed_recharge_ignored, set_seed_recharge_ignored, SeedRechargeIgnored);
}

impl SunCostRuleEditBackend for Pvz1051Backend {
    bool_patch_methods!(sun_cost_ignored, set_sun_cost_ignored, SunCostIgnored);
}

impl VisibilityEditBackend for Pvz1051Backend {
    bool_patch_methods!(fog_revealed, set_fog_revealed, FogRevealed);
    bool_patch_methods!(vase_contents_visible, set_vase_contents_visible, VaseContentsVisible);
}

impl ProjectileRuleEditBackend for Pvz1051Backend {
    fn kernel_pult_projectile_rule(&self) -> Result<KernelPultProjectileRule> {
        patches::variant::kernel_pult_projectile_rule::rule()
    }

    fn set_kernel_pult_projectile_rule(&self, rule: KernelPultProjectileRule) -> Result<()> {
        leases::set_kernel_pult_projectile_rule(rule)
    }
}

impl PlantEffectRuleEditBackend for Pvz1051Backend {
    bool_patch_methods!(
        instant_ice_and_ash_effects,
        set_instant_ice_and_ash_effects,
        InstantIceAndAshEffects
    );
    bool_patch_methods!(mushrooms_awake, set_mushrooms_awake, MushroomsAwake);
}

impl CobRuleEditBackend for Pvz1051Backend {
    bool_patch_methods!(cob_fixed_delay, set_cob_fixed_delay, CobFixedDelay);
    bool_patch_methods!(cob_recharge_shortened, set_cob_recharge_shortened, CobRechargeShortened);
    bool_patch_methods!(cob_drift_fixed, set_cob_drift_fixed, CobDriftFixed);
}

impl DropRuleEditBackend for Pvz1051Backend {
    bool_patch_methods!(item_drop_disabled, set_item_drop_disabled, ItemDropDisabled);
    bool_patch_methods!(
        natural_sun_drop_disabled,
        set_natural_sun_drop_disabled,
        NaturalSunDropDisabled
    );

    fn set_natural_sun_generated(&self, count: NonNegativeI32) -> Result<()> {
        let board = self.board()?;
        // SAFETY: `board` is non-null and the write is limited to `Board::mNumSunsFallen`.
        unsafe { ptrs::Board::set_num_suns_fallen(board.as_ptr(), count.get()) };
        Ok(())
    }

    fn set_natural_sun_countdown(&self, countdown: NonNegativeI32) -> Result<()> {
        let board = self.board()?;
        // SAFETY: `board` is non-null and the write is limited to `Board::mSunCountDown`.
        unsafe { ptrs::Board::set_sun_countdown(board.as_ptr(), countdown.get()) };
        Ok(())
    }
}

impl ZombieRuleEditBackend for Pvz1051Backend {
    bool_patch_methods!(
        jack_explosions_disabled,
        set_jack_explosions_disabled,
        JackExplosionsDisabled
    );
    bool_patch_methods!(
        pepper_explosions_disabled,
        set_pepper_explosions_disabled,
        PepperExplosionsDisabled
    );
    bool_patch_methods!(
        special_events_disabled,
        set_special_events_disabled,
        SpecialEventsDisabled
    );
    bool_patch_methods!(zombie_spawn_stopped, set_zombie_spawn_stopped, ZombieSpawnStopped);
    bool_patch_methods!(zombies_die_at_house, set_zombies_die_at_house, ZombiesDieAtHouse);
}

impl PlantingRuleEditBackend for Pvz1051Backend {
    fn easy_planting_cheat(&self) -> Result<bool> {
        let app = self.app();
        // SAFETY: `app` is non-null and the read is limited to `LawnApp::mEasyPlantingCheat`.
        Ok(unsafe { ptrs::LawnApp::is_easy_planting_cheat_enabled(app.as_ptr()) })
    }

    fn set_easy_planting_cheat(&self, enabled: bool) -> Result<()> {
        let app = self.app();
        // SAFETY: `app` is non-null and the write is limited to `LawnApp::mEasyPlantingCheat`,
        // verified from SeedPacket::CanPickUp/MouseDown and decomp field layout.
        unsafe { ptrs::LawnApp::set_easy_planting_cheat(app.as_ptr(), enabled) };
        Ok(())
    }

    bool_patch_methods!(
        planting_restrictions_ignored,
        set_planting_restrictions_ignored,
        PlantingRestrictionsIgnored
    );
}

impl PlantDamageRuleEditBackend for Pvz1051Backend {
    fn plant_damage_rule(&self) -> Result<PlantDamageRule> {
        crate::impls::event::damage_rule()
    }

    fn set_plant_damage_rule(&self, rule: PlantDamageRule) -> Result<()> {
        leases::set_plant_damage_rule(rule)
    }
}

impl ProfileReadonlyBackend for Pvz1051Backend {
    fn set_profile_readonly(&self, readonly: bool) -> Result<()> {
        if !readonly && crate::impls::reset::profile_isolation_active() {
            return Err(Pvz1051Error::AbiPreconditionFailed(
                "profile readonly is held by an active world-reset session",
            ));
        }
        leases::set_bool_patch(BoolPatchId::ProfileReadonly, readonly)
    }
}

impl MaidCheatsBackend for Pvz1051Backend {
    fn maid_cheat(&self) -> Result<MaidCheat> {
        patches::variant::maid_cheat::state()
    }

    fn set_maid_cheat(&self, cheat: MaidCheat) -> Result<()> {
        leases::set_maid_cheat(cheat)
    }
}
