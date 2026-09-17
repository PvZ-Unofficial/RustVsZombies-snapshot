//! Backend-free live object state vocabulary.

use crate::model::{Grid, KindCodeError, PlantId, PlantKind, ZombieId, ZombieKind};

/// Backend-neutral automatic item collection mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AutoCollectMode {
    /// Use the backend's natural automatic collection mechanism.
    #[default]
    Normal,
    /// Periodically scan item snapshots and collect selected items.
    Click,
    /// Explicitly disable automatic collection.
    Off,
}

/// Backend-neutral scalar view of a live plant used by core logic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridPlantView {
    pub id: PlantId,
    pub kind: PlantKind,
    pub grid: Grid,
    pub hp: i32,
}

impl GridPlantView {
    #[must_use]
    pub const fn occupies_grid(&self, grid: Grid) -> bool {
        plant_occupies_grid(self.grid, self.kind, grid)
    }
}

/// Checks the native grid footprint, including the right half of a cob cannon.
#[must_use]
pub const fn plant_occupies_grid(anchor: Grid, kind: PlantKind, grid: Grid) -> bool {
    let right_half = match anchor.col.checked_add(1) {
        Some(col) => col == grid.col,
        None => false,
    };
    anchor.row == grid.row && (anchor.col == grid.col || matches!(kind, PlantKind::CobCannon) && right_half)
}

/// Exact backend-neutral current action phase for a zombie.
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ZombiePhase {
    ZombieNormal = 0,
    ZombieDying,
    ZombieBurned,
    ZombieMowered,
    BungeeDiving,
    BungeeDivingScreaming,
    BungeeAtBottom,
    BungeeGrabbing,
    BungeeRising,
    BungeeHitOuchy,
    BungeeCutscene,
    PolevaulterPreVault,
    PolevaulterInVault,
    PolevaulterPostVault,
    RisingFromGrave,
    JackInTheBoxRunning,
    JackInTheBoxPopping,
    BobsledSliding,
    BobsledBoarding,
    BobsledCrashing,
    PogoBouncing,
    PogoHighBounce1,
    PogoHighBounce2,
    PogoHighBounce3,
    PogoHighBounce4,
    PogoHighBounce5,
    PogoHighBounce6,
    PogoForwardBounce2,
    PogoForwardBounce7,
    NewspaperReading,
    NewspaperMaddening,
    NewspaperMad,
    DiggerTunneling,
    DiggerRising,
    DiggerTunnelingPauseWithoutAxe,
    DiggerRiseWithoutAxe,
    DiggerStunned,
    DiggerWalking,
    DiggerWalkingWithoutAxe,
    DiggerCutscene,
    DancerDancingIn,
    DancerSnappingFingers,
    DancerSnappingFingersWithLight,
    DancerSnappingFingersHold,
    DancerDancingLeft,
    DancerWalkToRaise,
    DancerRaiseLeft1,
    DancerRaiseRight1,
    DancerRaiseLeft2,
    DancerRaiseRight2,
    DancerRising,
    DolphinWalking,
    DolphinIntoPool,
    DolphinRiding,
    DolphinInJump,
    DolphinWalkingInPool,
    DolphinWalkingWithoutDolphin,
    SnorkelWalking,
    SnorkelIntoPool,
    SnorkelWalkingInPool,
    SnorkelUpToEat,
    SnorkelEatingInPool,
    SnorkelDownFromEat,
    ZombiquariumAccel,
    ZombiquariumDrift,
    ZombiquariumBackAndForth,
    ZombiquariumBite,
    CatapultLaunching,
    CatapultReloading,
    GargantuarThrowing,
    GargantuarSmashing,
    ImpGettingThrown,
    ImpLanding,
    BalloonFlying,
    BalloonPopping,
    BalloonWalking,
    LadderCarrying,
    LadderPlacing,
    BossEnter,
    BossIdle,
    BossSpawning,
    BossStomping,
    BossBungeesEnter,
    BossBungeesDrop,
    BossBungeesLeave,
    BossDropRv,
    BossHeadEnter,
    BossHeadIdleBeforeSpit,
    BossHeadIdleAfterSpit,
    BossHeadSpit,
    BossHeadLeave,
    YetiRunning,
    SquashPreLaunch,
    SquashRising,
    SquashFalling,
    SquashDoneFalling,
}

impl ZombiePhase {
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
    }

    pub const fn try_from_code(code: i32) -> Result<Self, KindCodeError> {
        use ZombiePhase::*;

        Ok(match code {
            0 => ZombieNormal,
            1 => ZombieDying,
            2 => ZombieBurned,
            3 => ZombieMowered,
            4 => BungeeDiving,
            5 => BungeeDivingScreaming,
            6 => BungeeAtBottom,
            7 => BungeeGrabbing,
            8 => BungeeRising,
            9 => BungeeHitOuchy,
            10 => BungeeCutscene,
            11 => PolevaulterPreVault,
            12 => PolevaulterInVault,
            13 => PolevaulterPostVault,
            14 => RisingFromGrave,
            15 => JackInTheBoxRunning,
            16 => JackInTheBoxPopping,
            17 => BobsledSliding,
            18 => BobsledBoarding,
            19 => BobsledCrashing,
            20 => PogoBouncing,
            21 => PogoHighBounce1,
            22 => PogoHighBounce2,
            23 => PogoHighBounce3,
            24 => PogoHighBounce4,
            25 => PogoHighBounce5,
            26 => PogoHighBounce6,
            27 => PogoForwardBounce2,
            28 => PogoForwardBounce7,
            29 => NewspaperReading,
            30 => NewspaperMaddening,
            31 => NewspaperMad,
            32 => DiggerTunneling,
            33 => DiggerRising,
            34 => DiggerTunnelingPauseWithoutAxe,
            35 => DiggerRiseWithoutAxe,
            36 => DiggerStunned,
            37 => DiggerWalking,
            38 => DiggerWalkingWithoutAxe,
            39 => DiggerCutscene,
            40 => DancerDancingIn,
            41 => DancerSnappingFingers,
            42 => DancerSnappingFingersWithLight,
            43 => DancerSnappingFingersHold,
            44 => DancerDancingLeft,
            45 => DancerWalkToRaise,
            46 => DancerRaiseLeft1,
            47 => DancerRaiseRight1,
            48 => DancerRaiseLeft2,
            49 => DancerRaiseRight2,
            50 => DancerRising,
            51 => DolphinWalking,
            52 => DolphinIntoPool,
            53 => DolphinRiding,
            54 => DolphinInJump,
            55 => DolphinWalkingInPool,
            56 => DolphinWalkingWithoutDolphin,
            57 => SnorkelWalking,
            58 => SnorkelIntoPool,
            59 => SnorkelWalkingInPool,
            60 => SnorkelUpToEat,
            61 => SnorkelEatingInPool,
            62 => SnorkelDownFromEat,
            63 => ZombiquariumAccel,
            64 => ZombiquariumDrift,
            65 => ZombiquariumBackAndForth,
            66 => ZombiquariumBite,
            67 => CatapultLaunching,
            68 => CatapultReloading,
            69 => GargantuarThrowing,
            70 => GargantuarSmashing,
            71 => ImpGettingThrown,
            72 => ImpLanding,
            73 => BalloonFlying,
            74 => BalloonPopping,
            75 => BalloonWalking,
            76 => LadderCarrying,
            77 => LadderPlacing,
            78 => BossEnter,
            79 => BossIdle,
            80 => BossSpawning,
            81 => BossStomping,
            82 => BossBungeesEnter,
            83 => BossBungeesDrop,
            84 => BossBungeesLeave,
            85 => BossDropRv,
            86 => BossHeadEnter,
            87 => BossHeadIdleBeforeSpit,
            88 => BossHeadIdleAfterSpit,
            89 => BossHeadSpit,
            90 => BossHeadLeave,
            91 => YetiRunning,
            92 => SquashPreLaunch,
            93 => SquashRising,
            94 => SquashFalling,
            95 => SquashDoneFalling,
            _ => return Err(KindCodeError::new("zombie phase", code)),
        })
    }
}

impl TryFrom<i32> for ZombiePhase {
    type Error = KindCodeError;

    fn try_from(code: i32) -> Result<Self, Self::Error> {
        Self::try_from_code(code)
    }
}

/// Backend-neutral current live zombie state queried by stable zombie ID.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZombieState {
    pub id: ZombieId,
    pub kind: ZombieKind,
    pub row: i32,
    pub x: f32,
    pub y: f32,
    pub hp: i32,
    pub alive: bool,
    pub phase: ZombiePhase,
}
