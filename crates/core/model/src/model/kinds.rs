//! Semantic game kind enums.

/// PvZ 内部 game scene 状态，不是地形场景。
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GameUi {
    Loading = 0,
    Menu = 1,
    LevelIntro = 2,
    Playing = 3,
    ZombiesWon = 4,
    Award = 5,
    Credit = 6,
    Challenge = 7,
}

/// 植物或卡牌类型。
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlantKind {
    Peashooter = 0,
    Sunflower = 1,
    CherryBomb = 2,
    WallNut = 3,
    PotatoMine = 4,
    SnowPea = 5,
    Chomper = 6,
    Repeater = 7,
    PuffShroom = 8,
    SunShroom = 9,
    FumeShroom = 10,
    GraveBuster = 11,
    HypnoShroom = 12,
    ScaredyShroom = 13,
    IceShroom = 14,
    DoomShroom = 15,
    LilyPad = 16,
    Squash = 17,
    Threepeater = 18,
    TangleKelp = 19,
    Jalapeno = 20,
    Spikeweed = 21,
    Torchwood = 22,
    TallNut = 23,
    SeaShroom = 24,
    Plantern = 25,
    Cactus = 26,
    Blover = 27,
    SplitPea = 28,
    Starfruit = 29,
    Pumpkin = 30,
    MagnetShroom = 31,
    CabbagePult = 32,
    FlowerPot = 33,
    KernelPult = 34,
    CoffeeBean = 35,
    Garlic = 36,
    UmbrellaLeaf = 37,
    Marigold = 38,
    MelonPult = 39,
    GatlingPea = 40,
    TwinSunflower = 41,
    GloomShroom = 42,
    Cattail = 43,
    WinterMelon = 44,
    GoldMagnet = 45,
    Spikerock = 46,
    CobCannon = 47,
    Imitator = 48,
}

/// 僵尸类型。
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ZombieKind {
    Normal = 0,
    Flag = 1,
    Conehead = 2,
    PoleVaulting = 3,
    Buckethead = 4,
    Newspaper = 5,
    ScreenDoor = 6,
    Football = 7,
    Dancing = 8,
    BackupDancer = 9,
    DuckyTube = 10,
    Snorkel = 11,
    Zomboni = 12,
    Bobsled = 13,
    DolphinRider = 14,
    JackInTheBox = 15,
    Balloon = 16,
    Digger = 17,
    Pogo = 18,
    Yeti = 19,
    Bungee = 20,
    Ladder = 21,
    Catapult = 22,
    Gargantuar = 23,
    Imp = 24,
    Boss = 25,
    PeaHead = 26,
    WallNutHead = 27,
    JalapenoHead = 28,
    GatlingHead = 29,
    SquashHead = 30,
    TallNutHead = 31,
    GigaGargantuar = 32,
}

/// Board 地形场景。
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SceneKind {
    /// Day lawn.
    Day = 0,
    /// Night lawn.
    Night = 1,
    /// Day pool.
    Pool = 2,
    /// Night pool with fog.
    Fog = 3,
    /// Day roof.
    Roof = 4,
    /// Moon-night roof scene. PvZ 1.0.0.1051 raw enum name: `BACKGROUND_6_BOSS`.
    MoonNight = 5,
    MushroomGarden = 6,
    Greenhouse = 7,
    Zombiquarium = 8,
    TreeOfWisdom = 9,
}

impl SceneKind {
    /// 当前场景可放置/出怪的行数。
    #[must_use]
    pub const fn row_count(self) -> usize {
        match self {
            Self::Pool | Self::Fog => 6,
            Self::Day
            | Self::Night
            | Self::Roof
            | Self::MoonNight
            | Self::MushroomGarden
            | Self::Greenhouse
            | Self::Zombiquarium
            | Self::TreeOfWisdom => 5,
        }
    }

    /// 是否含泳池行。
    #[must_use]
    pub const fn has_pool(self) -> bool {
        matches!(self, Self::Pool | Self::Fog)
    }

    /// 是否为屋顶场景。
    #[must_use]
    pub const fn has_roof(self) -> bool {
        matches!(self, Self::Roof | Self::MoonNight)
    }

    /// 是否为夜晚场景。
    #[must_use]
    pub const fn is_night(self) -> bool {
        matches!(self, Self::Night | Self::Fog | Self::MoonNight | Self::MushroomGarden)
    }

    /// Whether the scene belongs to the regular plant-side lineup family.
    #[must_use]
    pub const fn supports_regular_lineup(self) -> bool {
        matches!(
            self,
            Self::Day | Self::Night | Self::Pool | Self::Fog | Self::Roof | Self::MoonNight | Self::MushroomGarden
        )
    }
}

/// 场景行类型。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RowType {
    Grass = 0,
    Pool = 1,
    Roof = 2,
}

/// 选卡语义。模仿者目标由嵌套植物类型表达。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CardSelection {
    Plant(PlantKind),
    Imitator(PlantKind),
}

/// A card selection whose packet/imitator relation is valid by construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CheckedCardSelection(CardSelection);

impl CheckedCardSelection {
    #[must_use]
    pub const fn selection(self) -> CardSelection {
        self.0
    }

    #[must_use]
    pub const fn packet_kind(self) -> PlantKind {
        match self.0 {
            CardSelection::Plant(kind) => kind,
            CardSelection::Imitator(_) => PlantKind::Imitator,
        }
    }

    #[must_use]
    pub const fn imitator_target(self) -> Option<PlantKind> {
        match self.0 {
            CardSelection::Plant(_) => None,
            CardSelection::Imitator(kind) => Some(kind),
        }
    }

    #[must_use]
    pub const fn effective_kind(self) -> PlantKind {
        match self.0 {
            CardSelection::Plant(kind) | CardSelection::Imitator(kind) => kind,
        }
    }
}

/// Invalid packet/imitator relationship.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CardSelectionError {
    #[error("Imitator packet requires a non-Imitator target")]
    MissingImitatorTarget,
    #[error("Imitator cannot target itself")]
    ImitatorTargetsItself,
    #[error("non-Imitator packet cannot carry an imitator target")]
    UnexpectedImitatorTarget,
}

impl CardSelection {
    /// Validates this semantic selection for use at a safe backend boundary.
    pub const fn checked(self) -> Result<CheckedCardSelection, CardSelectionError> {
        match self {
            Self::Plant(PlantKind::Imitator) => Err(CardSelectionError::MissingImitatorTarget),
            Self::Imitator(PlantKind::Imitator) => Err(CardSelectionError::ImitatorTargetsItself),
            Self::Plant(_) | Self::Imitator(_) => Ok(CheckedCardSelection(self)),
        }
    }

    /// Builds a checked selection from native packet and optional target parts.
    pub const fn from_packet_parts(
        packet_kind: PlantKind, imitator_target: Option<PlantKind>,
    ) -> Result<CheckedCardSelection, CardSelectionError> {
        match (packet_kind, imitator_target) {
            (PlantKind::Imitator, None) => Err(CardSelectionError::MissingImitatorTarget),
            (PlantKind::Imitator, Some(PlantKind::Imitator)) => Err(CardSelectionError::ImitatorTargetsItself),
            (PlantKind::Imitator, Some(target)) => Ok(CheckedCardSelection(Self::Imitator(target))),
            (_, Some(_)) => Err(CardSelectionError::UnexpectedImitatorTarget),
            (kind, None) => Ok(CheckedCardSelection(Self::Plant(kind))),
        }
    }
}

impl TryFrom<CardSelection> for CheckedCardSelection {
    type Error = CardSelectionError;

    fn try_from(selection: CardSelection) -> Result<Self, Self::Error> {
        selection.checked()
    }
}

impl From<PlantKind> for CardSelection {
    fn from(kind: PlantKind) -> Self {
        Self::Plant(kind)
    }
}

/// Unknown canonical PvZ kind code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unknown {kind} code {code}")]
pub struct KindCodeError {
    kind: &'static str,
    code: i32,
}

impl KindCodeError {
    pub(crate) const fn new(kind: &'static str, code: i32) -> Self {
        Self { kind, code }
    }

    #[must_use]
    pub const fn kind(self) -> &'static str {
        self.kind
    }

    #[must_use]
    pub const fn code(self) -> i32 {
        self.code
    }
}

macro_rules! impl_kind_code {
    ($kind:ident, $label:literal, {$($code:literal => $variant:ident),+ $(,)?}) => {
        impl $kind {
            pub const ALL: [Self; [$(stringify!($variant)),+].len()] = [$(Self::$variant),+];
            pub const COUNT: usize = Self::ALL.len();

            #[must_use]
            pub const fn code(self) -> i32 {
                self as i32
            }

            pub const fn try_from_code(code: i32) -> Result<Self, KindCodeError> {
                match code {
                    $($code => Ok(Self::$variant),)+
                    _ => Err(KindCodeError::new($label, code)),
                }
            }
        }

        impl TryFrom<i32> for $kind {
            type Error = KindCodeError;

            fn try_from(code: i32) -> Result<Self, Self::Error> {
                Self::try_from_code(code)
            }
        }
    };
}

impl_kind_code!(GameUi, "game-ui", {
    0 => Loading,
    1 => Menu,
    2 => LevelIntro,
    3 => Playing,
    4 => ZombiesWon,
    5 => Award,
    6 => Credit,
    7 => Challenge,
});

impl_kind_code!(PlantKind, "plant", {
    0 => Peashooter,
    1 => Sunflower,
    2 => CherryBomb,
    3 => WallNut,
    4 => PotatoMine,
    5 => SnowPea,
    6 => Chomper,
    7 => Repeater,
    8 => PuffShroom,
    9 => SunShroom,
    10 => FumeShroom,
    11 => GraveBuster,
    12 => HypnoShroom,
    13 => ScaredyShroom,
    14 => IceShroom,
    15 => DoomShroom,
    16 => LilyPad,
    17 => Squash,
    18 => Threepeater,
    19 => TangleKelp,
    20 => Jalapeno,
    21 => Spikeweed,
    22 => Torchwood,
    23 => TallNut,
    24 => SeaShroom,
    25 => Plantern,
    26 => Cactus,
    27 => Blover,
    28 => SplitPea,
    29 => Starfruit,
    30 => Pumpkin,
    31 => MagnetShroom,
    32 => CabbagePult,
    33 => FlowerPot,
    34 => KernelPult,
    35 => CoffeeBean,
    36 => Garlic,
    37 => UmbrellaLeaf,
    38 => Marigold,
    39 => MelonPult,
    40 => GatlingPea,
    41 => TwinSunflower,
    42 => GloomShroom,
    43 => Cattail,
    44 => WinterMelon,
    45 => GoldMagnet,
    46 => Spikerock,
    47 => CobCannon,
    48 => Imitator,
});

impl PlantKind {
    const REPORT_NAMES: [&'static str; Self::COUNT] = [
        "Peashooter",
        "Sunflower",
        "CherryBomb",
        "WallNut",
        "PotatoMine",
        "SnowPea",
        "Chomper",
        "Repeater",
        "PuffShroom",
        "SunShroom",
        "FumeShroom",
        "GraveBuster",
        "HypnoShroom",
        "ScaredyShroom",
        "IceShroom",
        "DoomShroom",
        "LilyPad",
        "Squash",
        "Threepeater",
        "TangleKelp",
        "Jalapeno",
        "Spikeweed",
        "Torchwood",
        "TallNut",
        "SeaShroom",
        "Plantern",
        "Cactus",
        "Blover",
        "SplitPea",
        "Starfruit",
        "Pumpkin",
        "MagnetShroom",
        "CabbagePult",
        "FlowerPot",
        "KernelPult",
        "CoffeeBean",
        "Garlic",
        "UmbrellaLeaf",
        "Marigold",
        "MelonPult",
        "GatlingPea",
        "TwinSunflower",
        "GloomShroom",
        "Cattail",
        "WinterMelon",
        "GoldMagnet",
        "Spikerock",
        "CobCannon",
        "Imitator",
    ];

    #[must_use]
    pub const fn report_name(self) -> &'static str {
        Self::REPORT_NAMES[self as usize]
    }
}

impl_kind_code!(ZombieKind, "zombie", {
    0 => Normal,
    1 => Flag,
    2 => Conehead,
    3 => PoleVaulting,
    4 => Buckethead,
    5 => Newspaper,
    6 => ScreenDoor,
    7 => Football,
    8 => Dancing,
    9 => BackupDancer,
    10 => DuckyTube,
    11 => Snorkel,
    12 => Zomboni,
    13 => Bobsled,
    14 => DolphinRider,
    15 => JackInTheBox,
    16 => Balloon,
    17 => Digger,
    18 => Pogo,
    19 => Yeti,
    20 => Bungee,
    21 => Ladder,
    22 => Catapult,
    23 => Gargantuar,
    24 => Imp,
    25 => Boss,
    26 => PeaHead,
    27 => WallNutHead,
    28 => JalapenoHead,
    29 => GatlingHead,
    30 => SquashHead,
    31 => TallNutHead,
    32 => GigaGargantuar,
});

impl_kind_code!(SceneKind, "scene", {
    0 => Day,
    1 => Night,
    2 => Pool,
    3 => Fog,
    4 => Roof,
    5 => MoonNight,
    6 => MushroomGarden,
    7 => Greenhouse,
    8 => Zombiquarium,
    9 => TreeOfWisdom,
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_discriminants_match_pvz_semantics() {
        assert_eq!(PlantKind::CobCannon as i32, 47);
        assert_eq!(PlantKind::Imitator as i32, 48);
        assert_eq!(ZombieKind::Digger as i32, 17);
        assert_eq!(GameUi::Playing as i32, 3);
        assert_eq!(SceneKind::Fog as i32, 3);
        assert_eq!(SceneKind::MoonNight as i32, 5);
    }

    #[test]
    fn canonical_codes_round_trip_without_transmute() {
        for code in 0..=7 {
            let ui = GameUi::try_from_code(code).expect("known game UI");
            assert_eq!(ui.code(), code);
        }
        for code in 0..=48 {
            let kind = PlantKind::try_from_code(code).expect("known plant");
            assert_eq!(kind.code(), code);
        }
        for code in 0..=32 {
            let kind = ZombieKind::try_from_code(code).expect("known zombie");
            assert_eq!(kind.code(), code);
        }
        for code in 0..=9 {
            let scene = SceneKind::try_from_code(code).expect("known scene");
            assert_eq!(scene.code(), code);
        }
        assert!(GameUi::try_from_code(8).is_err());
        assert!(PlantKind::try_from_code(49).is_err());
        assert!(ZombieKind::try_from_code(-1).is_err());
        assert!(SceneKind::try_from_code(10).is_err());
    }

    #[test]
    fn checked_card_selection_rejects_all_invalid_packet_relations() {
        let ordinary = CardSelection::from_packet_parts(PlantKind::Sunflower, None)
            .expect("ordinary packet without target is valid");
        assert_eq!(ordinary.selection(), CardSelection::Plant(PlantKind::Sunflower));
        assert_eq!(
            CardSelection::from_packet_parts(PlantKind::Imitator, None),
            Err(CardSelectionError::MissingImitatorTarget)
        );
        assert_eq!(
            CardSelection::Plant(PlantKind::Imitator).checked(),
            Err(CardSelectionError::MissingImitatorTarget)
        );
        assert_eq!(
            CardSelection::Imitator(PlantKind::Imitator).checked(),
            Err(CardSelectionError::ImitatorTargetsItself)
        );
        assert_eq!(
            CardSelection::from_packet_parts(PlantKind::Sunflower, Some(PlantKind::Blover)),
            Err(CardSelectionError::UnexpectedImitatorTarget)
        );
        assert_eq!(
            CardSelection::from_packet_parts(PlantKind::Imitator, Some(PlantKind::Imitator)),
            Err(CardSelectionError::ImitatorTargetsItself)
        );
        let checked =
            CardSelection::from_packet_parts(PlantKind::Imitator, Some(PlantKind::Blover)).expect("valid imitator");
        assert_eq!(checked.selection(), CardSelection::Imitator(PlantKind::Blover));
        assert_eq!(checked.packet_kind(), PlantKind::Imitator);
        assert_eq!(checked.imitator_target(), Some(PlantKind::Blover));
        assert_eq!(checked.effective_kind(), PlantKind::Blover);
    }

    #[test]
    fn moon_night_keeps_roof_and_night_semantics() {
        assert_eq!(SceneKind::MoonNight.row_count(), 5);
        assert!(SceneKind::MoonNight.has_roof());
        assert!(SceneKind::MoonNight.is_night());
        assert!(!SceneKind::MoonNight.has_pool());
    }
}
