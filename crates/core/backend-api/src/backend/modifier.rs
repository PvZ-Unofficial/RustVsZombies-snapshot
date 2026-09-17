//! Narrow modifier-style backend capabilities.

use crate::backend::Backend;
use rsvz_model::model::{KernelPultProjectileRule, MaidCheat, NonNegativeI32, PlantDamageRule, RefreshDance};

/// Battle sun resource write capability.
pub trait SunWriteBackend: Backend {
    /// 设置阳光数量。
    fn set_sun(&self, value: u32) -> Result<(), Self::Error>;
}

/// Direct write capability for the clock used by dancing-zombie gait.
///
/// Backends may store this separately from the board's logical frame counter. Implementations
/// must change only the native state that `BoardStateBackend::dancer_clock` observes.
pub trait DancerClockWriteBackend: Backend {
    /// Sets the dancing-zombie gait clock.
    fn set_dancer_clock(&self, value: u32) -> Result<(), Self::Error>;
}

/// Profile coin scalar capability.
pub trait CoinProfileBackend: Backend {
    /// 读取金币数量。
    fn coins(&self) -> Result<u32, Self::Error>;
    /// 设置金币数量。
    fn set_coins(&self, value: u32) -> Result<(), Self::Error>;
}

/// Seed rule editing capability.
pub trait SeedRuleEditBackend: Backend {
    /// Sets whether seed recharge/cooldown checks are ignored.
    fn set_seed_recharge_ignored(&self, enabled: bool) -> Result<(), Self::Error>;

    /// Reads whether seed recharge/cooldown checks are ignored.
    fn seed_recharge_ignored(&self) -> Result<bool, Self::Error>;
}

/// Sun cost rule editing capability.
pub trait SunCostRuleEditBackend: Backend {
    /// Sets whether planting and seed selection sun costs are ignored.
    fn set_sun_cost_ignored(&self, enabled: bool) -> Result<(), Self::Error>;

    /// Reads whether planting and seed selection sun costs are ignored.
    fn sun_cost_ignored(&self) -> Result<bool, Self::Error>;
}

/// Visibility rule editing capability.
pub trait VisibilityEditBackend: Backend {
    /// Sets whether fog is revealed by backend rule edits.
    fn set_fog_revealed(&self, enabled: bool) -> Result<(), Self::Error>;

    /// Reads whether fog is revealed by backend rule edits.
    fn fog_revealed(&self) -> Result<bool, Self::Error>;

    /// Sets whether vase contents are visible.
    fn set_vase_contents_visible(&self, enabled: bool) -> Result<(), Self::Error>;

    /// Reads whether vase contents are visible.
    fn vase_contents_visible(&self) -> Result<bool, Self::Error>;
}

/// Kernel-pult projectile rule editing capability.
pub trait ProjectileRuleEditBackend: Backend {
    /// Reads the global kernel-pult projectile rule.
    fn kernel_pult_projectile_rule(&self) -> Result<KernelPultProjectileRule, Self::Error>;

    /// Sets the global kernel-pult projectile rule.
    fn set_kernel_pult_projectile_rule(&self, rule: KernelPultProjectileRule) -> Result<(), Self::Error>;
}

/// Plant and mushroom native effect rule editing capability.
pub trait PlantEffectRuleEditBackend: Backend {
    /// Reads whether the native `DoSpecial` countdown branch is patched for instant ice/ash timing.
    ///
    /// PvZ implements this through a generic special-effect countdown shared by more than only ice
    /// and ash plants; scripts that enable it must account for that native scope.
    fn instant_ice_and_ash_effects(&self) -> Result<bool, Self::Error>;

    /// Sets whether the native `DoSpecial` countdown branch is patched for instant ice/ash timing.
    ///
    /// PvZ implements this through a generic special-effect countdown shared by more than only ice
    /// and ash plants; scripts that enable it must account for that native scope.
    fn set_instant_ice_and_ash_effects(&self, enabled: bool) -> Result<(), Self::Error>;

    /// Reads whether mushrooms are treated as awake by native rules.
    fn mushrooms_awake(&self) -> Result<bool, Self::Error>;

    /// Sets whether mushrooms are treated as awake by native rules.
    fn set_mushrooms_awake(&self, enabled: bool) -> Result<(), Self::Error>;
}

/// Cob cannon native rule editing capability.
pub trait CobRuleEditBackend: Backend {
    /// Reads whether native cob impact delay is patched to a fixed value.
    fn cob_fixed_delay(&self) -> Result<bool, Self::Error>;

    /// Sets whether native cob impact delay is patched to a fixed value.
    fn set_cob_fixed_delay(&self, enabled: bool) -> Result<(), Self::Error>;

    /// Reads whether cob recharge is shortened by the native code patch.
    fn cob_recharge_shortened(&self) -> Result<bool, Self::Error>;

    /// Sets whether cob recharge is shortened by the native code patch.
    fn set_cob_recharge_shortened(&self, enabled: bool) -> Result<(), Self::Error>;

    /// Reads whether native cob drift correction is enabled.
    fn cob_drift_fixed(&self) -> Result<bool, Self::Error>;

    /// Sets whether native cob drift correction is enabled.
    fn set_cob_drift_fixed(&self, enabled: bool) -> Result<(), Self::Error>;
}

/// Item and natural sun drop rule editing capability.
pub trait DropRuleEditBackend: Backend {
    /// Reads whether item drops are disabled.
    fn item_drop_disabled(&self) -> Result<bool, Self::Error>;

    /// Sets whether item drops are disabled.
    fn set_item_drop_disabled(&self, enabled: bool) -> Result<(), Self::Error>;

    /// Reads whether natural falling sun is disabled.
    fn natural_sun_drop_disabled(&self) -> Result<bool, Self::Error>;

    /// Sets whether natural falling sun is disabled.
    fn set_natural_sun_drop_disabled(&self, enabled: bool) -> Result<(), Self::Error>;

    /// Writes the native fallen-sun generation counter.
    fn set_natural_sun_generated(&self, count: NonNegativeI32) -> Result<(), Self::Error>;

    /// Writes the native countdown until the next falling sun.
    fn set_natural_sun_countdown(&self, countdown: NonNegativeI32) -> Result<(), Self::Error>;
}

/// Zombie spawn, attack, and failure rule editing capability.
pub trait ZombieRuleEditBackend: Backend {
    /// Reads whether jack-in-the-box zombie explosions are disabled.
    fn jack_explosions_disabled(&self) -> Result<bool, Self::Error>;

    /// Sets whether jack-in-the-box zombie explosions are disabled.
    fn set_jack_explosions_disabled(&self, enabled: bool) -> Result<(), Self::Error>;

    /// Reads whether pepper zombie explosions are disabled.
    fn pepper_explosions_disabled(&self) -> Result<bool, Self::Error>;

    /// Sets whether pepper zombie explosions are disabled.
    fn set_pepper_explosions_disabled(&self, enabled: bool) -> Result<(), Self::Error>;

    /// Reads whether special events such as graves, coral, and bungee events are disabled.
    fn special_events_disabled(&self) -> Result<bool, Self::Error>;

    /// Sets whether special events such as graves, coral, and bungee events are disabled.
    fn set_special_events_disabled(&self, enabled: bool) -> Result<(), Self::Error>;

    /// Reads whether automatic zombie spawning is stopped.
    fn zombie_spawn_stopped(&self) -> Result<bool, Self::Error>;

    /// Sets whether automatic zombie spawning is stopped.
    fn set_zombie_spawn_stopped(&self, enabled: bool) -> Result<(), Self::Error>;

    /// Reads whether house-entering zombies are killed instead of taking the failure branch.
    fn zombies_die_at_house(&self) -> Result<bool, Self::Error>;

    /// Sets whether house-entering zombies are killed instead of taking the failure branch.
    fn set_zombies_die_at_house(&self, enabled: bool) -> Result<(), Self::Error>;
}

/// Native planting rule editing capability.
pub trait PlantingRuleEditBackend: Backend {
    /// Reads the native `LawnApp::mEasyPlantingCheat` flag.
    fn easy_planting_cheat(&self) -> Result<bool, Self::Error>;

    /// Sets the native `LawnApp::mEasyPlantingCheat` flag.
    fn set_easy_planting_cheat(&self, enabled: bool) -> Result<(), Self::Error>;

    /// Reads whether native planting placement restrictions are ignored.
    fn planting_restrictions_ignored(&self) -> Result<bool, Self::Error>;

    /// Sets whether native planting placement restrictions are ignored.
    fn set_planting_restrictions_ignored(&self, enabled: bool) -> Result<(), Self::Error>;
}

/// Plant damage rule editing capability.
pub trait PlantDamageRuleEditBackend: Backend {
    /// Reads the global plant damage rule.
    fn plant_damage_rule(&self) -> Result<PlantDamageRule, Self::Error>;

    /// Sets the global plant damage rule.
    fn set_plant_damage_rule(&self, rule: PlantDamageRule) -> Result<(), Self::Error>;
}

/// Dance-zombie MaidCheats capability.
pub trait MaidCheatsBackend: Backend {
    /// Reads the current MaidCheats state.
    fn maid_cheat(&self) -> Result<MaidCheat, Self::Error>;

    /// Sets the current MaidCheats state.
    fn set_maid_cheat(&self, cheat: MaidCheat) -> Result<(), Self::Error>;
}

/// One native Board::SetDanceMode operation, including its immediate animation restart.
pub trait DanceModeBackend: Backend {
    /// Native Board::SetDanceMode: sets the mode and immediately restarts walking
    /// animations of eligible normal/conehead/buckethead zombies. Each call matters.
    fn set_dance_mode(&self, enabled: bool) -> Result<(), Self::Error>;
}

/// Common-zombie dance gait used by SEML refresh measurements.
pub trait CommonZombieDanceBackend: Backend {
    fn set_common_zombie_dance(&self, dance: RefreshDance) -> Result<(), Self::Error>;
}

/// Whether cob projectiles retain their natural target-dependent impact delay.
pub trait CobImpactDelayBackend: Backend {
    fn set_cob_impact_delay(&self, enabled: bool) -> Result<(), Self::Error>;
}

/// Profile trophy unlock capability.
pub trait TrophyUnlockBackend: Backend {
    /// 解锁奖杯。
    fn unlock_trophy(&self) -> Result<(), Self::Error>;
}

/// Profile hidden-mode unlock capability.
pub trait HiddenModeUnlockBackend: Backend {
    /// 解锁隐藏关卡。
    fn unlock_hidden_modes(&self) -> Result<(), Self::Error>;
}

/// Profile readonly toggle capability.
pub trait ProfileReadonlyBackend: Backend {
    /// 设置当前存档只读开关。
    fn set_profile_readonly(&self, readonly: bool) -> Result<(), Self::Error>;
}
