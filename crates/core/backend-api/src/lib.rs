//! Safe-facing backend capability traits.
//!
//! Infallible Board reads without an entity handle require the concrete backend's
//! active Board access scope. Scope acquisition is fallible; bypassing it is a
//! low-level contract error and must panic safely, never dereference a null pointer.
//! Game-facing operations establish that scope and preserve callback-local failure
//! handling. Entity borrows retain their lifetime constraints; absent IDs return
//! `None`. UI restrictions, invalid values and genuine native failures retain `Result`.

pub mod access;
pub mod artifact;
pub mod error;
pub mod opening;

pub mod backend {
    mod base;

    pub mod advanced_pause;
    pub mod event;
    pub mod fast_forward;
    pub mod gameplay;
    pub mod grid_item;
    pub mod item;
    pub mod projectile;
    pub mod seed;

    pub mod keyboard;
    pub mod modifier;
    pub mod plant;
    pub mod read;
    pub mod state;
    pub mod surface;
    pub mod timing;
    pub mod zombie;

    pub use advanced_pause::AdvancedPauseBackend;
    pub use base::{Backend, BattleStatusBackend};

    pub use event::{NativeEventBackend, NativeEventSink};
    pub use fast_forward::{FastForwardBackend, SeedChooserFastForwardBackend};
    pub use gameplay::{
        BoardSupportBackend, LawnMowerClearBackend, SceneEditBackend, SpawnScheduleBackend, SunMoneyBackend,
        WorldResetBackend, pool_has_free_slot, pool_has_reserved_slot,
    };
    pub use grid_item::{GridItemCreateBackend, GridItemEditBackend, GridItemReadBackend, GridItemStateBackend};
    pub use item::{AutoCollectBackend, ItemClickCollectBackend, ItemReadBackend};
    pub use keyboard::KeyboardStateBackend;
    pub use modifier::{
        CobImpactDelayBackend, CobRuleEditBackend, CoinProfileBackend, CommonZombieDanceBackend, DanceModeBackend,
        DancerClockWriteBackend, DropRuleEditBackend, HiddenModeUnlockBackend, MaidCheatsBackend,
        PlantDamageRuleEditBackend, PlantEffectRuleEditBackend, PlantingRuleEditBackend, ProfileReadonlyBackend,
        ProjectileRuleEditBackend, SeedRuleEditBackend, SunCostRuleEditBackend, SunWriteBackend, TrophyUnlockBackend,
        VisibilityEditBackend, ZombieRuleEditBackend,
    };
    pub use plant::{
        CobFireBackend, ImitatorMorphBackend, PlantContactBackend, PlantCostBackend, PlantCreateBackend,
        PlantEffectCountdownWriteBackend, PlantHealthWriteBackend, PlantIdleAnimationBackend, PlantPlacementBackend,
        PlantPoolBackend, PlantReadBackend, PlantRemoveBackend, PlantSleepBackend, PlantStateBackend,
        PlantStateCountdownWriteBackend, PlantStateWriteBackend, PlantVisualStateBackend,
    };
    pub use projectile::ProjectileReadBackend;
    pub use read::{
        BoardReadinessBackend, ClockBackend, CurrentWaveBackend, CursorQueryBackend, GameUiBackend,
        GridGeometryBackend, GridTerrainBackend, SceneBackend, SunQueryBackend,
    };
    pub use seed::{
        CardAppendSelectionBackend, CardSelectionReadBackend, ChooserCooldownReadBackend, SeedBankReadBackend,
        SeedCooldownReadBackend, SeedPacketBackend,
    };
    pub use state::{BackendIdentityBackend, BoardStateBackend, RandomControlBackend, SunProductionModeBackend};
    pub use surface::{
        AudioBackend, BattleEntryBackend, DisplayBackend, GameSpeedHintBackend, GardenBackend, InputBackend,
        MainMenuBackend,
    };
    pub use timing::{WaveHealthBackend, WaveRefreshControlBackend, WaveTimingBackend};
    pub use zombie::{
        ZombieBodyHealthWriteBackend, ZombieContactBackend, ZombieCreateBackend, ZombieKillBackend,
        ZombiePhaseCountdownWriteBackend, ZombiePositionWriteBackend, ZombieRawFactsBackend, ZombieReadBackend,
        ZombieRemoveBackend, ZombieStateBackend, ZombieVerticalPositionBackend, ZombieXWriteBackend,
    };
}

pub use backend::*;
