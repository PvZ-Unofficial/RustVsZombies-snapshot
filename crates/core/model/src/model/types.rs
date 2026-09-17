//! Domain value types.

use crate::model::{CardSelection, KindCodeError, SceneKind, ZombieKind};

/// 最大常规卡槽数量。
pub const MAX_SEED_SLOTS: usize = 10;
/// PvZ 常规列数。
pub const DEFAULT_COL_COUNT: usize = 9;
/// PvZ 经典关卡默认波数。
pub const DEFAULT_SPAWN_WAVES: usize = 20;
/// PvZ 每波出怪槽数量。
pub const SPAWN_SLOTS_PER_WAVE: usize = 50;

/// Battle or simulation progress status reported separately from host UI state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BattleStatus {
    /// The battle is still advancing normally.
    Running,
    /// The player's defense has failed or the game has reached a losing state.
    Lost,
    /// The battle ended, but the backend cannot or should not classify the reason further.
    Ended,
    /// The script or runner objective has been reached.
    ObjectiveReached,
    /// The backend cannot reliably classify the current battle state.
    #[default]
    Unknown,
}

/// Kernel-pult projectile replacement rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KernelPultProjectileRule {
    /// Preserve vanilla kernel-pult projectile RNG.
    Normal,
    /// Force kernel-pults to throw butter.
    AlwaysButter,
    /// Force kernel-pults to throw kernels.
    AlwaysKernel,
}

/// Global plant damage rule override.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlantDamageRule {
    /// Preserve vanilla plant damage handling.
    Normal,
    /// Prevent plant damage through supported native damage paths.
    Invincible,
    /// Make supported native damage paths remove extra plant durability.
    Weak,
}

/// Backend-neutral battle entry request.
///
/// This describes the battle semantics a script needs instead of exposing a raw PvZ game-mode ID.
/// Backends map supported configs to their own startup mechanism and reject unsupported configs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BattleConfig {
    /// Start or continue a survival endless battle on the selected scene.
    Endless(EndlessBattleConfig),
}

impl BattleConfig {
    /// Classic script default: pool endless.
    pub const POOL_ENDLESS: Self = Self::Endless(EndlessBattleConfig::new(SceneKind::Pool));
}

/// Survival endless battle configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EndlessBattleConfig {
    pub scene: SceneKind,
}

impl EndlessBattleConfig {
    #[must_use]
    pub const fn new(scene: SceneKind) -> Self {
        Self { scene }
    }
}

/// core 与 backend 共用的 `0-based` 场地格子。
///
/// 普通脚本中的 `(row, col)` 元组从 `1` 开始并会在 API 边界转换；需要显式构造
/// 脚本格时优先使用 `rsvz::grid(row, col)` 或 [`Grid::from_one_based`]。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Grid {
    /// 从 `0` 开始的 core 行号。
    pub row: i32,
    /// 从 `0` 开始的 core 列号。
    pub col: i32,
}

impl Grid {
    /// 从 core `0-based` 行列构造格子，并检查坐标非负。
    pub const fn new(row: i32, col: i32) -> Result<Self, GridError> {
        if row < 0 || col < 0 {
            return Err(GridError::NegativeCoordinate);
        }
        Ok(Self { row, col })
    }

    /// 从脚本/玩家使用的 `1-based` 行列转换为 core `0-based` 格子。
    pub const fn from_one_based(row: i32, col: i32) -> Result<Self, GridError> {
        if row <= 0 || col <= 0 {
            return Err(GridError::NegativeCoordinate);
        }
        Ok(Self {
            row: row - 1,
            col: col - 1,
        })
    }

    /// 返回 1-based 行列。
    #[must_use]
    pub const fn to_one_based(self) -> (i32, i32) {
        (self.row + 1, self.col + 1)
    }

    /// 检查格子是否在指定行列数内。
    #[must_use]
    pub fn is_in_bounds(self, row_count: usize, col_count: usize) -> bool {
        let Ok(row) = usize::try_from(self.row) else {
            return false;
        };
        let Ok(col) = usize::try_from(self.col) else {
            return false;
        };
        row < row_count && col < col_count
    }
}

/// Grid 构造错误。
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum GridError {
    #[error("grid coordinate must be non-negative")]
    NegativeCoordinate,
}

/// 像素或世界坐标。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Position {
    pub x: f32,
    pub y: f32,
}

/// 像素坐标。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PixelPos {
    pub x: i32,
    pub y: i32,
}

/// 矩形区域。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// 波次。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Wave(pub i32);

/// 当前场地信息 snapshot。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldInfo {
    pub scene: SceneKind,
    pub row_count: usize,
    pub col_count: usize,
    pub has_pool: bool,
    pub has_roof: bool,
    pub is_night: bool,
    pub has_graves: bool,
}

impl FieldInfo {
    /// 从场景生成常规场地信息。
    #[must_use]
    pub const fn from_scene(scene: SceneKind) -> Self {
        Self {
            scene,
            row_count: scene.row_count(),
            col_count: DEFAULT_COL_COUNT,
            has_pool: scene.has_pool(),
            has_roof: scene.has_roof(),
            is_night: scene.is_night(),
            has_graves: matches!(scene, SceneKind::Night | SceneKind::Fog),
        }
    }

    #[must_use]
    pub fn contains_grid(self, grid: Grid) -> bool {
        grid.is_in_bounds(self.row_count, self.col_count)
    }

    #[must_use]
    pub fn contains_row(self, row: i32) -> bool {
        usize::try_from(row).is_ok_and(|row| row < self.row_count)
    }
}

/// 从 `0` 开始的卡槽索引。
///
/// `SeedSlot::new(0)` 表示第一张卡；按植物类型用卡时通常无需直接构造
/// `SeedSlot`。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SeedSlot(usize);

impl SeedSlot {
    /// 从 `0-based` 索引构造卡槽；有效范围为 `0..MAX_SEED_SLOTS`。
    pub const fn new(index: usize) -> Result<Self, SeedSlotError> {
        if index >= MAX_SEED_SLOTS {
            return Err(SeedSlotError::OutOfRange);
        }
        Ok(Self(index))
    }

    /// 构造后端已验证的卡槽索引。
    #[doc(hidden)]
    #[inline(always)]
    pub const fn from_index_unchecked(index: usize) -> Self {
        Self(index)
    }

    /// 返回 0-based 索引。
    #[must_use]
    #[inline(always)]
    pub const fn index(self) -> usize {
        self.0
    }
}

/// 卡槽错误。
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum SeedSlotError {
    #[error("seed slot is out of range")]
    OutOfRange,
}

/// 种植判定结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plantability {
    Allowed,
    Rejected(PlantRejectReason),
}

impl Plantability {
    /// 是否允许种植。
    #[must_use]
    pub const fn is_allowed(self) -> bool {
        matches!(self, Self::Allowed)
    }

    /// Converts the native PvZ placement result code into safe game semantics.
    #[must_use]
    pub const fn from_game_rule_code(code: i32) -> Self {
        if code == 0 {
            Self::Allowed
        } else {
            Self::Rejected(PlantRejectReason::GameRule(code))
        }
    }
}

/// 种植拒绝原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlantRejectReason {
    OutOfBounds,
    NotEnoughSun,
    Occupied,
    WrongTerrain,
    RequiresPot,
    RequiresLilypad,
    PoolFull,
    GameRule(i32),
}

/// 植物层级。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlantLayer {
    Below,
    Main,
    Pumpkin,
    Flying,
}

/// Board-anchored non-plant grid object kind.
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GridItemKind {
    Grave = 1,
    Crater = 2,
    Ladder = 3,
    Rake = 11,
}

impl GridItemKind {
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
    }

    pub const fn try_from_code(code: i32) -> Result<Self, KindCodeError> {
        match code {
            1 => Ok(Self::Grave),
            2 => Ok(Self::Crater),
            3 => Ok(Self::Ladder),
            11 => Ok(Self::Rake),
            _ => Err(KindCodeError::new("grid-item", code)),
        }
    }
}

impl TryFrom<i32> for GridItemKind {
    type Error = KindCodeError;

    fn try_from(code: i32) -> Result<Self, Self::Error> {
        Self::try_from_code(code)
    }
}

/// 僵尸死亡掉落行为。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LootMode {
    WithLoot,
    NoLoot,
}

/// A falling/collectible item kind matching PvZ `CoinType` values.
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ItemKind {
    SilverCoin = 1,
    GoldCoin = 2,
    Diamond = 3,
    Sun = 4,
    SmallSun = 5,
    LargeSun = 6,
    FinalSeedPacket = 7,
    Trophy = 8,
    Shovel = 9,
    Almanac = 10,
    CarKeys = 11,
    Vase = 12,
    WateringCan = 13,
    Taco = 14,
    Note = 15,
    UsableSeedPacket = 16,
    PresentPlant = 17,
    AwardMoneyBag = 18,
    AwardPresent = 19,
    AwardBagDiamond = 20,
    AwardSilverSunflower = 21,
    AwardGoldSunflower = 22,
    Chocolate = 23,
    AwardChocolate = 24,
    PresentMinigames = 25,
    PresentPuzzleMode = 26,
    PresentSurvivalMode = 27,
}

impl ItemKind {
    #[must_use]
    #[inline(always)]
    pub const fn raw(self) -> i32 {
        self as i32
    }

    #[must_use]
    #[inline(always)]
    pub const fn is_sun(self) -> bool {
        matches!(self, Self::Sun | Self::SmallSun | Self::LargeSun)
    }

    #[must_use]
    #[inline(always)]
    pub const fn changes_cursor(self) -> bool {
        matches!(self, Self::UsableSeedPacket)
    }

    #[must_use]
    pub const fn from_raw(raw: i32) -> Option<Self> {
        match raw {
            1 => Some(Self::SilverCoin),
            2 => Some(Self::GoldCoin),
            3 => Some(Self::Diamond),
            4 => Some(Self::Sun),
            5 => Some(Self::SmallSun),
            6 => Some(Self::LargeSun),
            7 => Some(Self::FinalSeedPacket),
            8 => Some(Self::Trophy),
            9 => Some(Self::Shovel),
            10 => Some(Self::Almanac),
            11 => Some(Self::CarKeys),
            12 => Some(Self::Vase),
            13 => Some(Self::WateringCan),
            14 => Some(Self::Taco),
            15 => Some(Self::Note),
            16 => Some(Self::UsableSeedPacket),
            17 => Some(Self::PresentPlant),
            18 => Some(Self::AwardMoneyBag),
            19 => Some(Self::AwardPresent),
            20 => Some(Self::AwardBagDiamond),
            21 => Some(Self::AwardSilverSunflower),
            22 => Some(Self::AwardGoldSunflower),
            23 => Some(Self::Chocolate),
            24 => Some(Self::AwardChocolate),
            25 => Some(Self::PresentMinigames),
            26 => Some(Self::PresentPuzzleMode),
            27 => Some(Self::PresentSurvivalMode),
            _ => None,
        }
    }
}

impl TryFrom<i32> for ItemKind {
    type Error = i32;

    fn try_from(raw: i32) -> Result<Self, Self::Error> {
        Self::from_raw(raw).ok_or(raw)
    }
}

/// 僵尸生成行为。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpawnMode {
    Direct,
    Wave,
}

/// Strategy used to build the board's zombie spawn schedule.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ZombieSpawnMode {
    /// Evenly repeat the requested zombie types across the complete schedule.
    #[default]
    Average,
    /// Preserve the requested order and count in every wave.
    Exact,
    /// Let the game build its native weighted schedule from the allowed types.
    Natural,
}

/// 鼠标按键。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
}

/// 游戏音效 ID。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SoundId(pub u32);

/// 游戏模式 ID。具体后端负责校验该模式是否存在。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct GameMode(pub i32);

/// Outcome of direct object edit helpers that target a cross-frame ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObjectEditOutcome {
    /// The target was re-resolved as a current live object and edited.
    Applied,
    /// The target ID no longer resolves to a current live object.
    Missing,
}

/// Dance-zombie MaidCheats state request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaidCheat {
    Stop,
    CallPartner,
    Dancing,
    Move,
}

/// One configured spawn wave.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SpawnWave {
    len: usize,
    zombies: [ZombieKind; SPAWN_SLOTS_PER_WAVE],
}

impl SpawnWave {
    const fn new() -> Self {
        Self {
            len: 0,
            zombies: [ZombieKind::Normal; SPAWN_SLOTS_PER_WAVE],
        }
    }

    fn set(&mut self, zombies: impl IntoIterator<Item = ZombieKind>) {
        self.len = 0;
        for zombie in zombies {
            if self.len < SPAWN_SLOTS_PER_WAVE {
                self.zombies[self.len] = zombie;
            }
            self.len = self.len.saturating_add(1);
        }
        for zombie in self.zombies.iter_mut().skip(self.len.min(SPAWN_SLOTS_PER_WAVE)) {
            *zombie = ZombieKind::Normal;
        }
    }

    fn as_slice(&self) -> &[ZombieKind] {
        &self.zombies[..self.len.min(SPAWN_SLOTS_PER_WAVE)]
    }
}

/// Spawn-list configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnList {
    waves: Vec<SpawnWave>,
    allowed_types: Vec<ZombieKind>,
}

impl SpawnList {
    /// 新建空出怪列表。
    #[must_use]
    pub fn new() -> Self {
        Self {
            waves: Vec::new(),
            allowed_types: Vec::new(),
        }
    }

    /// 设置某一波的僵尸类型。
    pub fn set_wave(&mut self, wave: usize, zombies: impl IntoIterator<Item = ZombieKind>) {
        if self.waves.len() <= wave {
            self.waves.resize_with(wave + 1, SpawnWave::new);
        }
        if let Some(slot) = self.waves.get_mut(wave) {
            slot.set(zombies);
        }
        self.rebuild_allowed_types_from_waves();
    }

    /// 读取某一波。
    #[must_use]
    pub fn wave(&self, wave: usize) -> Option<&[ZombieKind]> {
        self.waves.get(wave).map(SpawnWave::as_slice)
    }

    /// 所有波次。
    #[must_use]
    pub fn waves(&self) -> impl ExactSizeIterator<Item = &[ZombieKind]> + '_ {
        self.waves.iter().map(SpawnWave::as_slice)
    }

    /// Configured length of one wave, including invalid over-capacity input.
    #[must_use]
    pub fn wave_len(&self, wave: usize) -> Option<usize> {
        self.waves.get(wave).map(|wave| wave.len)
    }

    /// 当前出怪类型允许表。
    #[must_use]
    pub fn allowed_types(&self) -> &[ZombieKind] {
        &self.allowed_types
    }

    /// 设置出怪类型允许表，保留传入顺序并去重。
    pub fn set_allowed_types(&mut self, kinds: impl IntoIterator<Item = ZombieKind>) {
        self.allowed_types.clear();
        for kind in kinds {
            push_unique(&mut self.allowed_types, kind);
        }
    }

    fn rebuild_allowed_types_from_waves(&mut self) {
        let waves = &self.waves;
        let allowed_types = &mut self.allowed_types;
        allowed_types.clear();
        for wave in waves {
            for kind in wave.as_slice() {
                push_unique(allowed_types, *kind);
            }
        }
    }

    /// 出怪列表是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.waves.is_empty()
    }

    /// 出怪列表包含的波数。
    #[must_use]
    pub fn wave_count(&self) -> usize {
        self.waves.len()
    }
}

impl Default for SpawnList {
    fn default() -> Self {
        Self::new()
    }
}

fn push_unique(kinds: &mut Vec<ZombieKind>, kind: ZombieKind) {
    if !kinds.contains(&kind) {
        kinds.push(kind);
    }
}

impl From<CardSelection> for Plantability {
    fn from(_selection: CardSelection) -> Self {
        Self::Allowed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_converts_from_one_based() {
        let grid = Grid { row: 1, col: 2 };
        assert_eq!(Grid::from_one_based(2, 3), Ok(grid));
        assert_eq!(grid.to_one_based(), (2, 3));
    }

    #[test]
    fn seed_slot_checks_bounds() {
        assert_eq!(SeedSlot::new(0).map(SeedSlot::index), Ok(0));
        assert_eq!(SeedSlot::new(MAX_SEED_SLOTS), Err(SeedSlotError::OutOfRange));
    }

    #[test]
    fn field_info_contains_only_its_snapshot_bounds() {
        let field = FieldInfo::from_scene(SceneKind::Day);
        assert!(field.contains_grid(Grid { row: 0, col: 0 }));
        assert!(field.contains_grid(Grid { row: 4, col: 8 }));
        assert!(!field.contains_grid(Grid { row: -1, col: 0 }));
        assert!(!field.contains_grid(Grid { row: 5, col: 0 }));
        assert!(!field.contains_grid(Grid { row: 0, col: 9 }));
        assert!(field.contains_row(4));
        assert!(!field.contains_row(-1));
        assert!(!field.contains_row(5));
    }

    #[test]
    fn plantability_and_grid_item_codes_use_canonical_native_semantics() {
        assert_eq!(Plantability::from_game_rule_code(0), Plantability::Allowed);
        assert_eq!(
            Plantability::from_game_rule_code(-7),
            Plantability::Rejected(PlantRejectReason::GameRule(-7))
        );
        for kind in [
            GridItemKind::Grave,
            GridItemKind::Crater,
            GridItemKind::Ladder,
            GridItemKind::Rake,
        ] {
            assert_eq!(GridItemKind::try_from_code(kind.code()), Ok(kind));
        }
        assert!(GridItemKind::try_from_code(0).is_err());
        assert!(GridItemKind::try_from_code(4).is_err());
    }

    #[test]
    fn spawn_list_resizes_on_set_wave() {
        let mut spawn = SpawnList::new();
        spawn.set_wave(2, vec![ZombieKind::Digger]);
        assert_eq!(spawn.wave(2), Some([ZombieKind::Digger].as_slice()));
        assert_eq!(spawn.wave(1), Some([].as_slice()));
        assert_eq!(spawn.allowed_types(), [ZombieKind::Digger].as_slice());
    }

    #[test]
    fn spawn_list_replaces_allowed_types_on_set_wave() {
        let mut spawn = SpawnList::new();
        spawn.set_wave(0, [ZombieKind::Normal, ZombieKind::Conehead]);
        spawn.set_wave(0, [ZombieKind::Buckethead]);
        assert_eq!(spawn.wave(0), Some([ZombieKind::Buckethead].as_slice()));
        assert_eq!(spawn.allowed_types(), [ZombieKind::Buckethead].as_slice());
    }

    #[test]
    fn spawn_list_replacement_clears_stale_slots() {
        let mut expected = SpawnList::new();
        expected.set_wave(0, [ZombieKind::Normal]);

        let mut actual = SpawnList::new();
        actual.set_wave(0, [ZombieKind::Normal, ZombieKind::Conehead]);
        actual.set_wave(0, [ZombieKind::Normal]);

        assert_eq!(actual, expected);
    }
}
