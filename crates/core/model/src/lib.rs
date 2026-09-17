//! Backend-free model vocabulary shared by RSVZ layers.

pub mod model {
    pub mod cob;
    pub mod contact;
    pub mod error;
    pub mod event;
    pub mod fast_forward;
    pub mod ids;
    pub mod input;
    pub mod kinds;
    pub mod measure;
    pub mod native_state;
    pub mod objects;
    pub mod script;
    pub mod smart_remove;
    pub mod snapshot;
    pub mod timing;
    pub mod types;
    pub mod values;
    pub mod zombie_motion;

    pub use cob::CobTarget;
    pub use contact::{
        AbsoluteContactRange, CHERRY_BOMB_RADIUS, ContactCircle, ContactRect, DOOM_SHROOM_RADIUS, DamageRangeFlags,
        GridExplosionKind, PlantContactRect, PlantDefenseBounds, PlantDefenseKind, PlantThreatKind, PlantThreatShape,
        PlantWeapon, PredictedZombieAttackBounds, RelativeContactRange, RelativeContactRect, StaticCobImpactRequest,
        ZombieAttackBounds, ZombieContactProfile, ZombieDefenseBounds, ZombieDefenseDrawPose, ZombieDefenseGeometry,
        ZombieDefensePhase, ZombieDefenseRowY, ZombieDefenseSpec, ZombieDefenseState, ZombieGeometryError,
        ZombieGeometryInput, ZombieGeometryOrientation, ZombieHeightState, ZombiePlantThreatKind,
        ZombiePlantThreatShape, ZombieReanimationFacts, ZombieThreatCandidateFacts, ZombieThreatCandidateShape,
    };
    pub use error::{BackendErrorKind, CoreLogicError};
    pub use event::{
        BeginPlantEffect, EffectOutcomeFact, EventDecision, EventDecisionOrigin, EventFrameStatus, EventInterest,
        EventToken, GameEvent, GargantuarAshHitFact, GargantuarSpawnedFact, HomeEntryEvent, HomeEntryFact,
        ImpThrownFact, PlantEffect, PlantEffectAttemptFact, PlantEffectEvent, PlantEffectKey, PlantEffectOutcome,
        PlantEffectSource,
    };
    pub use fast_forward::{
        AdvancedPauseMaskColor, AdvancedPauseOptions, FastForwardOptions, FastForwardPerformance, FastForwardRequest,
        FastForwardStopReason, FastForwardUntil, FastForwardWindow, SeedChooserFastForwardOptions,
    };
    pub use ids::{GridItemId, ItemId, PlantId, ProjectileId, ZombieId};
    pub use input::{KeyCode, KeyCodeError};
    pub use kinds::{
        CardSelection, CardSelectionError, CheckedCardSelection, GameUi, KindCodeError, PlantKind, RowType, SceneKind,
        ZombieKind,
    };
    pub use measure::{
        ImpLeakDiagnosticConfig, MeasureLimit, MeasureLimitError, MeasureMode, MeasureModeParseError,
        MeasureTrialOutcome, MeasurementEnd, MeasurementSetup, MeasurementTrialCounts, ProtectTarget, ProtectionEdit,
        ProtectionPolicy, RefreshDance, RefreshMeasureConfig, RefreshSample, RefreshTimingFact,
    };
    pub use native_state::{RandomMode, RandomStreamKind, SunProductionMode};
    pub use objects::{AutoCollectMode, GridPlantView, ZombiePhase, ZombieState, plant_occupies_grid};
    pub use script::{
        DEFAULT_RESET_INITIAL_SUN, ReloadBoundary, ReloadMode, ResetCardCooldowns, ScriptPhase, SessionShard,
        WorldResetConfig,
    };
    pub use smart_remove::{SmartRemoveGridPlantLayer, SmartRemoveOptions};
    pub use snapshot::{
        BattleSnapshot, GridItemSnapshot, PlantSnapshot, ProjectileSnapshot, SeedSnapshot, SimulationSnapshot,
        ZombieSnapshot, ZombieSpawnSnapshot,
    };
    pub use timing::{
        AssumedWavelength, RelativeTime, WaveClockState, WaveTimingError, WaveTimingSnapshot, WavelengthCheck,
        WavelengthDeclaration, WavelengthDeclarationOutcome, WavelengthMode, WavelengthValidation,
    };
    pub use types::{
        BattleConfig, BattleStatus, DEFAULT_SPAWN_WAVES, EndlessBattleConfig, FieldInfo, GameMode, Grid, GridError,
        GridItemKind, ItemKind, KernelPultProjectileRule, LootMode, MAX_SEED_SLOTS, MaidCheat, MouseButton,
        ObjectEditOutcome, PixelPos, PlantDamageRule, PlantLayer, PlantRejectReason, Plantability, Position, Rect,
        SPAWN_SLOTS_PER_WAVE, SeedSlot, SoundId, SpawnList, SpawnMode, Wave, ZombieSpawnMode,
    };
    pub use values::{
        CheckedValueError, FiniteF32, I32RepresentableF32, NonNegativeI32, PositiveFiniteF32, PositiveHp,
        SpawnWaveSlot, SunAmount,
    };
    pub use zombie_motion::{
        UniformChillPolicy, UniformZombieMotion, UnsupportedZombieMotionReason, ZombieMotionCallError,
        ZombieMotionCounters, ZombieMotionDirection, ZombieMotionError, ZombieMotionState, ZombieMovementModel,
        ZombieTrackProfile,
    };
}

pub use model::*;
