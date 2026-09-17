//! User script prelude.
//!
//! The low-level DSL remains available through `rsvz::dsl::prelude::*` and is
//! automatically imported inside `#[rsvz::script]` bodies.

pub use crate::cards::{Card, CardError, CardErrorKind, CardResult, CardSource, TryCard};
pub use crate::cards::{
    card, card_cd, normalize_card_effect, set_sun_cost_ignored, try_card, try_normalize_card_effect,
    try_set_sun_cost_ignored,
};
pub use crate::cob::{
    AutoSetCobs, EraseCobsFromList, Fire, FixLatestCob, ForEachCobGrid, MoveCobsToListBottom, MoveCobsToListTop,
    RawFire, RecoverCob, RecoverCobList, RecoverFire, RoofRecoverCob, RoofRecoverCobList, RoofUsableCob,
    RoofUsableCobList, SetCobSequentialMode, SetCobs, SetNextCob, SetNextCobSlot, SkipCobs, TryAutoSetCobs, TryFire,
    TryRawFire, TryRecoverFire, TrySetCobs, UsableCob, UsableCobList,
};
pub use crate::cob::{
    auto_set_cobs, erase_cobs_from_list, fire, fix_latest_cob, for_each_cob_grid, move_cobs_to_list_bottom,
    move_cobs_to_list_top, plant_cob, raw_fire, recover_cob, recover_cob_list, recover_fire, roof_cob_fly_time,
    roof_recover_cob, roof_recover_cob_list, roof_usable_cob, roof_usable_cob_list, set_cob_columns,
    set_cob_sequential_mode, set_cobs, set_next_cob, set_next_cob_slot, skip_cobs, try_auto_set_cobs, try_fire,
    try_raw_fire, try_recover_fire, try_set_cobs, usable_cob, usable_cob_list,
};
pub use crate::core::modifier;
pub use crate::grid::{grid, target};
pub use crate::is_safe_blover;
pub use crate::is_safe_imitator_ice;
pub use crate::key::{KeyBindError, KeyBindOptions};
pub use crate::live_value::LiveValue;
pub use crate::runtime::{RuntimeError, RuntimeResult};
pub use crate::script::*;
pub use crate::setup::{self, IntoZombieTypeSelection, LineupCommand, SetZombies, random_zombie_types};
pub use crate::setup::{lineup, set_zombies};
pub use crate::shovel::{Shovel, TryShovel};
pub use crate::shovel::{shovel, try_shovel};
pub use crate::smart_fodder::SmartFodderSpec;
pub use crate::smart_fodder::{ScheduledFodder, predict_c9_remove_by, smart_fodder, try_smart_fodder};
pub use crate::state_hook::{
    OnAfterAttach, OnAfterScript, OnAfterTick, OnBeforeExit, OnBeforeScript, OnBeforeTick, OnEnterFight, OnExitFight,
    StateHookCommandOutcome, StateHookHandle,
};
pub use crate::state_hook::{
    on_after_attach, on_after_script, on_after_tick, on_before_exit, on_before_script, on_before_tick, on_enter_fight,
    on_exit_fight, remove_state_hook,
};
pub use crate::time::time;
pub use crate::zombie::{ensure_zombie_row, try_ensure_zombie_row};
pub use crate::{CobManager, Plant, Zombie, for_each_plant, for_each_zombie};
pub use crate::{
    LogContext, LogLevel, LogRecord, Logger, ResetCardCooldowns, SessionJobKey, SessionShard, WorldResetConfig, log,
    reset_logger, set_logger,
};
pub use crate::{
    auto_collect, bench, cards, cob, ice_filler, key, measure, plant, plant_fixer, shovel, smart_fodder, smart_remove,
    state_hook, tick, timeline, zombie,
};
pub use crate::{
    claim_session_job, fail_script, publish_artifact, request_world_reset, session_shard, set_auto_enter, stop_script,
};
pub use rsvz_backend_api::backend::{
    AdvancedPauseBackend, AudioBackend, AutoCollectBackend, Backend, BattleEntryBackend, BattleStatusBackend,
    BoardSupportBackend, CardAppendSelectionBackend, ChooserCooldownReadBackend, ClockBackend, CobFireBackend,
    CobRuleEditBackend, CoinProfileBackend, CurrentWaveBackend, CursorQueryBackend, DisplayBackend,
    DropRuleEditBackend, FastForwardBackend, GameSpeedHintBackend, GameUiBackend, GardenBackend, GridGeometryBackend,
    GridItemCreateBackend, GridItemEditBackend, GridTerrainBackend, HiddenModeUnlockBackend, InputBackend,
    ItemClickCollectBackend, ItemReadBackend, KeyboardStateBackend, MaidCheatsBackend, MainMenuBackend,
    PlantCostBackend, PlantCreateBackend, PlantDamageRuleEditBackend, PlantEffectCountdownWriteBackend,
    PlantEffectRuleEditBackend, PlantHealthWriteBackend, PlantIdleAnimationBackend, PlantPlacementBackend,
    PlantReadBackend, PlantRemoveBackend, PlantSleepBackend, PlantStateCountdownWriteBackend, PlantStateWriteBackend,
    PlantVisualStateBackend, PlantingRuleEditBackend, ProfileReadonlyBackend, ProjectileRuleEditBackend, SceneBackend,
    SceneEditBackend, SeedBankReadBackend, SeedChooserFastForwardBackend, SeedCooldownReadBackend, SeedPacketBackend,
    SeedRuleEditBackend, SpawnScheduleBackend, SunCostRuleEditBackend, SunMoneyBackend, SunQueryBackend,
    SunWriteBackend, TrophyUnlockBackend, VisibilityEditBackend, WaveRefreshControlBackend, WaveTimingBackend,
    WorldResetBackend, ZombieBodyHealthWriteBackend, ZombieContactBackend, ZombieCreateBackend, ZombieKillBackend,
    ZombiePhaseCountdownWriteBackend, ZombiePositionWriteBackend, ZombieRawFactsBackend, ZombieReadBackend,
    ZombieRemoveBackend, ZombieRuleEditBackend, ZombieVerticalPositionBackend, ZombieXWriteBackend,
};
pub use rsvz_game::logic::cards::{CardContext, CardOp, CardPlantingBackend, IntoCardSelection, IntoCardSelections};
pub use rsvz_game::logic::cleanup::{clear_plants, clear_zombies, kill_all_zombies};
pub use rsvz_game::logic::cob::{CobBackend, CobListOrder, CobSequentialMode, IntoCobTarget};
pub use rsvz_game::logic::shovel::{IntoShovelOp, IntoShovelTarget, ShovelContext, ShovelOp, ShovelTarget};
pub use rsvz_game::logic::{
    AdvancedPauseMaskColor, AdvancedPauseOptions, ContactGeometryBackend, FastForwardOptions, FastForwardPerformance,
    FastForwardRequestExt, FastForwardStopReason, FastForwardWindowExt, IntoGrid, PlantFixer, PlantFixerBackend,
    PlantFixerError, PlantFixerGridError, SeedChooserFastForwardOptions, predicted_zombie_attack_bounds,
    zombie_defense_draw_pose, zombie_defense_geometry, zombie_defense_geometry_from_state,
    zombie_geometry_input_from_profile,
};
pub use rsvz_game::{
    Lineup, LineupApplyBackend, LineupApplyOptions, LineupBase, LineupCell, LineupParseError, LineupPlant,
    LineupReloadPolicy, SessionArtifact,
};
pub use rsvz_model::model::ZombieSpawnMode::{Average, Exact, Natural};
pub use rsvz_model::model::{
    AbsoluteContactRange, AssumedWavelength, BattleConfig, BattleStatus, CardSelection, CobTarget, ContactCircle,
    ContactRect, DamageRangeFlags, EndlessBattleConfig, Grid, GridExplosionKind, KernelPultProjectileRule, KeyCode,
    KeyCodeError, MaidCheat, MeasureMode, MeasurementSetup, ObjectEditOutcome, PlantContactRect, PlantDamageRule,
    PlantDefenseBounds, PlantDefenseKind, PlantId, PlantKind, PlantThreatKind, PlantThreatShape,
    PredictedZombieAttackBounds, ProtectTarget, ProtectionEdit, ProtectionPolicy, RefreshDance, RefreshMeasureConfig,
    RelativeContactRange, RelativeTime, ReloadMode, SeedSlot, SmartRemoveOptions, StaticCobImpactRequest, Wave,
    ZombieAttackBounds, ZombieContactProfile, ZombieDefenseBounds, ZombieDefenseDrawPose, ZombieDefenseGeometry,
    ZombieDefensePhase, ZombieDefenseRowY, ZombieDefenseSpec, ZombieDefenseState, ZombieGeometryError,
    ZombieGeometryInput, ZombieGeometryOrientation, ZombieHeightState, ZombieId, ZombieKind, ZombiePhase,
    ZombiePlantThreatKind, ZombiePlantThreatShape, ZombieSpawnMode, ZombieState, ZombieThreatCandidateShape,
};
pub use rsvz_schedule::timeline::{IntoAssumedWavelength, IntoRelativeTime};

pub use rsvz_game::timeline::{at, at_frame, try_at, try_at_frame};

pub use crate::{Frame, PlantRef, ZombieRef};

pub use rsvz_game::timing::refresh_countdown;
