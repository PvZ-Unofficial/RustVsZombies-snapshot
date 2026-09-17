//! 与 backend 无关的卡片选择和语义种植。
//!
//! 本模块统一负责卡槽查找、可用性/阳光检查、落点规划、自动荷叶/花盆、
//! 覆盖规则与有序批处理。backend 只提供卡槽、阳光、落点、植物读取和删除等
//! 原子能力，不重复实现这些策略。
//!
//! core [`Grid`] 与 [`SeedSlot`] 从 `0` 开始；便捷函数 [`card`] /
//! [`card_slot`] 的独立行列参数使用脚本侧 `1-based` 坐标。单卡语义接口
//! 成功时直接返回 [`PlantId`]，失败时返回可供调用者匹配的 [`CardError`]；
//! 批量接口以 `None` 表示某一项的普通游戏规则拒绝，并继续后续项。

use std::fmt;

use crate::backend::{
    ChooserCooldownReadBackend, GameUiBackend, PlantCostBackend, PlantCreateBackend, PlantPlacementBackend,
    PlantPoolBackend, PlantRemoveBackend, PlantSleepBackend, SeedCooldownReadBackend, SeedPacketBackend,
    SunMoneyBackend,
};
use crate::backend::{PlantReadBackend, SeedBankReadBackend, pool_has_free_slot};
use crate::logic::grid::IntoGrid;
use crate::model::{
    CardSelection, CoreLogicError, Grid, MAX_SEED_SLOTS, PlantId, PlantKind, PlantRejectReason, SeedSlot,
};
use crate::model::{GameUi, Plantability};
use crate::runtime::RuntimeError;

pub fn validate_card_selection(cards: &[CardSelection]) -> Result<(), CoreLogicError> {
    if cards.len() > MAX_SEED_SLOTS {
        return Err(CoreLogicError::TooManyCards {
            selected: cards.len(),
            max: MAX_SEED_SLOTS,
        });
    }

    let mut imitator_seen = false;
    for (index, selection) in cards.iter().copied().enumerate() {
        let checked = selection
            .checked()
            .map_err(|_error| CoreLogicError::InvalidCardSelection(selection))?;
        if checked.imitator_target().is_some() {
            if imitator_seen {
                return Err(CoreLogicError::MultipleImitatorCards);
            }
            imitator_seen = true;
        }
        if cards.get(..index).is_some_and(|previous| previous.contains(&selection)) {
            return Err(CoreLogicError::DuplicateCardSelection(selection));
        }
    }

    Ok(())
}

/// 卡片 core helper 的详细错误，区分 backend 故障与纯 core 规则拒绝。
#[derive(Debug, thiserror::Error)]
pub enum CardLogicError {
    /// backend 的卡片操作失败。
    #[error("底层卡片操作失败：{0}")]
    Backend(RuntimeError),
    /// 纯 core 卡片校验失败。
    #[error("{0}")]
    Core(CoreLogicError),
    /// 种植自动容器或主体的某个组件时发生 backend 故障。
    #[error("种植卡片组件 {component:?} 时底层操作失败：{error}")]
    PlantingBackend {
        component: PlantingComponent,
        #[source]
        error: RuntimeError,
    },
    /// 种植自动容器或主体的某个组件时被 core 规则拒绝。
    #[error("种植卡片组件 {component:?} 失败：{error}")]
    PlantingCore {
        component: PlantingComponent,
        error: CoreLogicError,
    },
}

impl From<CoreLogicError> for CardLogicError {
    fn from(error: CoreLogicError) -> Self {
        Self::Core(error)
    }
}

/// Result type returned by semantic card operations.
pub type CardResult<T = ()> = Result<T, CardError>;

/// Machine-readable reason for a semantic card failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CardErrorKind {
    InvalidGrid,
    NotSelected {
        selection: CardSelection,
    },
    MissingSlot {
        slot: SeedSlot,
    },
    Cooldown {
        selection: CardSelection,
        remaining: i32,
    },
    NotEnoughSun {
        selection: CardSelection,
    },
    Unavailable {
        selection: CardSelection,
    },
    Rejected {
        selection: CardSelection,
        reason: PlantRejectReason,
    },
    NoPlantableGrid {
        selection: CardSelection,
    },
    InvalidSelection,
}

/// Structured card error with a ready-to-display Chinese message.
///
/// The kind is stable input for script control flow; the message is only for
/// humans and deliberately contains no wave or time. The scripting reporter
/// adds runtime context when it displays the error.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct CardError {
    kind: CardErrorKind,
    message: String,
}

impl CardError {
    fn new(kind: CardErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// Returns the machine-readable failure reason.
    #[must_use]
    pub const fn kind(&self) -> CardErrorKind {
        self.kind
    }

    /// Returns the human-readable message without runtime context.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl From<CardError> for RuntimeError {
    fn from(error: CardError) -> Self {
        RuntimeError::new(error.message)
    }
}

#[derive(Clone, Copy)]
struct CardName(CardSelection);

impl fmt::Display for CardName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            CardSelection::Plant(kind) => f.write_str(plant_name(kind)),
            CardSelection::Imitator(kind) => write!(f, "模仿{}", plant_name(kind)),
        }
    }
}

const fn plant_name(kind: PlantKind) -> &'static str {
    match kind {
        PlantKind::Peashooter => "豌豆射手",
        PlantKind::Sunflower => "向日葵",
        PlantKind::CherryBomb => "樱桃炸弹",
        PlantKind::WallNut => "坚果",
        PlantKind::PotatoMine => "土豆地雷",
        PlantKind::SnowPea => "寒冰射手",
        PlantKind::Chomper => "大嘴花",
        PlantKind::Repeater => "双重射手",
        PlantKind::PuffShroom => "小喷菇",
        PlantKind::SunShroom => "阳光菇",
        PlantKind::FumeShroom => "大喷菇",
        PlantKind::GraveBuster => "墓碑吞噬者",
        PlantKind::HypnoShroom => "魅惑菇",
        PlantKind::ScaredyShroom => "胆小菇",
        PlantKind::IceShroom => "寒冰菇",
        PlantKind::DoomShroom => "毁灭菇",
        PlantKind::LilyPad => "荷叶",
        PlantKind::Squash => "倭瓜",
        PlantKind::Threepeater => "三发射手",
        PlantKind::TangleKelp => "缠绕海藻",
        PlantKind::Jalapeno => "火爆辣椒",
        PlantKind::Spikeweed => "地刺",
        PlantKind::Torchwood => "火炬树桩",
        PlantKind::TallNut => "高坚果",
        PlantKind::SeaShroom => "水兵菇",
        PlantKind::Plantern => "路灯花",
        PlantKind::Cactus => "仙人掌",
        PlantKind::Blover => "三叶草",
        PlantKind::SplitPea => "裂荚射手",
        PlantKind::Starfruit => "杨桃",
        PlantKind::Pumpkin => "南瓜头",
        PlantKind::MagnetShroom => "磁力菇",
        PlantKind::CabbagePult => "卷心菜投手",
        PlantKind::FlowerPot => "花盆",
        PlantKind::KernelPult => "玉米投手",
        PlantKind::CoffeeBean => "咖啡豆",
        PlantKind::Garlic => "大蒜",
        PlantKind::UmbrellaLeaf => "叶子保护伞",
        PlantKind::Marigold => "金盏花",
        PlantKind::MelonPult => "西瓜投手",
        PlantKind::GatlingPea => "机枪射手",
        PlantKind::TwinSunflower => "双子向日葵",
        PlantKind::GloomShroom => "忧郁菇",
        PlantKind::Cattail => "香蒲",
        PlantKind::WinterMelon => "冰西瓜投手",
        PlantKind::GoldMagnet => "吸金磁",
        PlantKind::Spikerock => "地刺王",
        PlantKind::CobCannon => "玉米加农炮",
        PlantKind::Imitator => "模仿者",
    }
}

enum CardTarget<'a> {
    Grid(i32, i32),
    Candidates(&'a [Grid]),
}

impl fmt::Display for CardTarget<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Grid(row, col) => write!(f, "({row}, {col})"),
            Self::Candidates(grids) => {
                if grids.is_empty() {
                    return f.write_str("候选位置列表");
                }
                f.write_str("候选位置 [")?;
                for (index, grid) in grids.iter().enumerate() {
                    if index != 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "({}, {})", grid.row + 1, grid.col + 1)?;
                }
                f.write_str("]")
            }
        }
    }
}

fn plant_reject_reason(reason: PlantRejectReason) -> String {
    match reason {
        PlantRejectReason::OutOfBounds => "目标格超出场地范围".to_owned(),
        PlantRejectReason::NotEnoughSun => "阳光不足".to_owned(),
        PlantRejectReason::Occupied => "目标格已被占用".to_owned(),
        PlantRejectReason::WrongTerrain => "当前地形不能种植这张卡".to_owned(),
        PlantRejectReason::RequiresPot => "目标格需要花盆".to_owned(),
        PlantRejectReason::RequiresLilypad => "目标格需要荷叶".to_owned(),
        PlantRejectReason::PoolFull => "植物数量已达到上限".to_owned(),
        PlantRejectReason::GameRule(code) => format!("游戏规则拒绝在目标格种植（代码 {code}）"),
    }
}

fn no_usable_card_reason(selection: CardSelection) -> Result<(CardErrorKind, String), String>
where
    rsvz_current::CurrentBackend: CardContext,
{
    crate::access::with_backend(|backend| {
        let checked = selection.checked().map_err(|error| format!("卡片选择无效：{error}"))?;
        let cooldown = card_cd(selection);
        if let Some(remaining) = cooldown.filter(|remaining| *remaining > 0) {
            return Ok((
                CardErrorKind::Cooldown { selection, remaining },
                format!("{}卡片还有 {remaining}cs 才能使用", CardName(selection)),
            ));
        }

        let required = backend
            .current_plant_cost(checked)
            .map_err(|error| format!("读取卡片阳光消耗失败：{error}"))?;
        if !backend
            .can_take_sun_money(required)
            .map_err(|error| format!("读取当前阳光失败：{error}"))?
        {
            return Ok((
                CardErrorKind::NotEnoughSun { selection },
                format!("阳光不足，{}需要 {} 阳光", CardName(selection), required.as_u32()),
            ));
        }

        Ok((
            CardErrorKind::Unavailable { selection },
            "卡片当前不可用，可能缺少对应的升级植物".to_owned(),
        ))
    })
}

fn core_card_error_reason(
    selection: CardSelection, component: Option<PlantingComponent>, error: &CoreLogicError,
) -> Result<(CardErrorKind, String), String>
where
    rsvz_current::CurrentBackend: CardContext,
{
    let (kind, mut reason) = match error {
        CoreLogicError::InvalidGrid => (CardErrorKind::InvalidGrid, "目标格无效".to_owned()),
        CoreLogicError::NoUsableSeed if component != Some(PlantingComponent::AutoContainer) => {
            no_usable_card_reason(selection)?
        }
        CoreLogicError::NoUsableSeed => (
            CardErrorKind::Unavailable { selection },
            "没有可用的荷叶或花盆卡片".to_owned(),
        ),
        CoreLogicError::SeedSlotNotFound(selection) => (
            CardErrorKind::NotSelected { selection: *selection },
            format!("没有选择{}卡片", CardName(*selection)),
        ),
        CoreLogicError::DuplicateCardSelection(selection) | CoreLogicError::InvalidCardSelection(selection) => (
            CardErrorKind::InvalidSelection,
            format!("{}不是有效的卡片选择", CardName(*selection)),
        ),
        CoreLogicError::MultipleImitatorCards => {
            (CardErrorKind::InvalidSelection, "同一套卡只能选择一张模仿者".to_owned())
        }
        CoreLogicError::TooManyCards { selected, max } => (
            CardErrorKind::InvalidSelection,
            format!("选择了 {selected} 张卡，超过上限 {max}"),
        ),
        CoreLogicError::PlantRejected(PlantRejectReason::NotEnoughSun) => {
            (CardErrorKind::NotEnoughSun { selection }, "阳光不足".to_owned())
        }
        CoreLogicError::PlantRejected(reason) => (
            CardErrorKind::Rejected {
                selection,
                reason: *reason,
            },
            plant_reject_reason(*reason),
        ),
        CoreLogicError::NoPlantableGrid(selection) => (
            CardErrorKind::NoPlantableGrid { selection: *selection },
            "所有候选位置都不可种植".to_owned(),
        ),
        CoreLogicError::NoReadyCob | CoreLogicError::ObjectNotActionable => {
            (CardErrorKind::Unavailable { selection }, error.to_string())
        }
    };
    if component == Some(PlantingComponent::AutoContainer) {
        reason = format!("自动补种荷叶或花盆失败：{reason}");
    }
    Ok((kind, reason))
}

fn card_error(selection: CardSelection, target: CardTarget<'_>, error: &CardLogicError) -> CardError
where
    rsvz_current::CurrentBackend: CardContext,
{
    let details = match error {
        CardLogicError::Backend(error) => Err(format!("底层卡片操作失败：{error}")),
        CardLogicError::Core(error) => core_card_error_reason(selection, None, error),
        CardLogicError::PlantingBackend { component, error } => Err(match component {
            PlantingComponent::AutoContainer => format!("自动补种荷叶或花盆时底层操作失败：{error}"),
            PlantingComponent::Main => format!("种植主体植物时底层操作失败：{error}"),
        }),
        CardLogicError::PlantingCore { component, error } => core_card_error_reason(selection, Some(*component), error),
    };
    match details {
        Ok((kind, reason)) => CardError::new(kind, format!("种植{}到 {target} 失败：{reason}", CardName(selection))),
        Err(reason) => crate::diagnostics::abort_operation(RuntimeError::new(format!(
            "种植{}到 {target} 失败：{reason}",
            CardName(selection)
        ))),
    }
}

fn card_error_at(selection: CardSelection, row: i32, col: i32, error: &CardLogicError) -> CardError
where
    rsvz_current::CurrentBackend: CardContext,
{
    card_error(selection, CardTarget::Grid(row, col), error)
}

fn card_error_any(selection: CardSelection, grids: &[Grid], error: &CardLogicError) -> CardError
where
    rsvz_current::CurrentBackend: CardContext,
{
    card_error(selection, CardTarget::Candidates(grids), error)
}

/// 一项使用脚本侧 `1-based` 行列的种卡操作。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CardOp {
    /// 要使用的普通或模仿者卡片。
    pub selection: CardSelection,
    /// 从 `1` 开始的脚本行号。
    pub row: i32,
    /// 从 `1` 开始的脚本列号。
    pub col: i32,
}

/// The component created by a recorded card planting operation.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlantingComponent {
    AutoContainer,
    Main,
}

/// Complete result of one shared card planting operation.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlantingReceipt {
    pub selection: CardSelection,
    pub slot: SeedSlot,
    pub grid: Grid,
    pub main: PlantId,
    pub auto_container: Option<PlantId>,
}

/// core 语义种卡组合所需的最小 backend 能力集合。
pub trait CardPlantingBackend:
    SeedPacketBackend
    + PlantCostBackend
    + SunMoneyBackend
    + PlantPlacementBackend
    + PlantPoolBackend
    + PlantCreateBackend
    + PlantRemoveBackend
    + PlantSleepBackend
{
}

impl<T> CardPlantingBackend for T where
    T: SeedPacketBackend
        + PlantCostBackend
        + SunMoneyBackend
        + PlantPlacementBackend
        + PlantPoolBackend
        + PlantCreateBackend
        + PlantRemoveBackend
        + PlantSleepBackend
{
}

/// Backend context required by user-facing semantic card operations.
pub trait CardContext:
    CardPlantingBackend + GameUiBackend + SeedCooldownReadBackend + ChooserCooldownReadBackend
{
}

impl<T> CardContext for T where
    T: CardPlantingBackend + GameUiBackend + SeedCooldownReadBackend + ChooserCooldownReadBackend
{
}

/// 把脚本侧卡片简写转换为明确的普通/模仿者选择。
pub trait IntoCardSelection {
    /// 转换为明确的普通/模仿者卡片选择。
    fn into_card_selection(self) -> CardSelection;
}

impl IntoCardSelection for CardSelection {
    fn into_card_selection(self) -> CardSelection {
        self
    }
}

impl IntoCardSelection for PlantKind {
    fn into_card_selection(self) -> CardSelection {
        CardSelection::Plant(self)
    }
}

/// 把单张卡或卡片列表转换为明确的卡片选择列表。
pub trait IntoCardSelections {
    /// 按输入顺序转换为明确的卡片选择列表。
    fn into_card_selections(self) -> Vec<CardSelection>;
}

impl IntoCardSelections for CardSelection {
    fn into_card_selections(self) -> Vec<CardSelection> {
        vec![self]
    }
}

impl IntoCardSelections for PlantKind {
    fn into_card_selections(self) -> Vec<CardSelection> {
        vec![CardSelection::Plant(self)]
    }
}

impl<T, const N: usize> IntoCardSelections for [T; N]
where
    T: IntoCardSelection,
{
    fn into_card_selections(self) -> Vec<CardSelection> {
        self.into_iter().map(IntoCardSelection::into_card_selection).collect()
    }
}

impl<T> IntoCardSelections for Vec<T>
where
    T: IntoCardSelection,
{
    fn into_card_selections(self) -> Vec<CardSelection> {
        self.into_iter().map(IntoCardSelection::into_card_selection).collect()
    }
}

impl<T> IntoCardSelections for &[T]
where
    T: IntoCardSelection + Copy,
{
    fn into_card_selections(self) -> Vec<CardSelection> {
        self.iter()
            .copied()
            .map(IntoCardSelection::into_card_selection)
            .collect()
    }
}

impl From<(CardSelection, i32, i32)> for CardOp {
    fn from((selection, row, col): (CardSelection, i32, i32)) -> Self {
        Self { selection, row, col }
    }
}

/// 查找第一张可用卡。
pub fn find_usable_seed(candidates: impl IntoIterator<Item = CardSelection>) -> Option<SeedSlot>
where
    rsvz_current::CurrentBackend: SeedPacketBackend,
{
    for candidate in candidates {
        if let Some((slot, true)) = find_seed_for_selection(candidate) {
            return Some(slot);
        }
    }
    None
}

fn find_seed_for_selection(selection: CardSelection) -> Option<(SeedSlot, bool)>
where
    rsvz_current::CurrentBackend: SeedPacketBackend,
{
    crate::access::with_backend(|backend| {
        for seed in crate::live_value::read_or_abort(backend.seeds(), "seeds") {
            if crate::live_value::read_or_abort(backend.seed_selection(seed), "seed_selection").selection() == selection
            {
                return Some((
                    backend.seed_slot(seed),
                    crate::live_value::read_or_abort(backend.seed_can_pick_up(seed), "seed_can_pick_up"),
                ));
            }
        }
        None
    })
}

/// Reads a card's current cooldown; an unselected card has no value.
/// Required native read failures end the current callback.
pub fn card_cd(selection: CardSelection) -> Option<i32>
where
    rsvz_current::CurrentBackend: GameUiBackend + SeedCooldownReadBackend + ChooserCooldownReadBackend,
{
    use crate::live_value::read_or_abort;
    crate::access::with_backend(|backend| {
        let checked = read_or_abort(selection.checked(), "invalid cooldown card selection");
        match read_or_abort(backend.game_ui(), "failed to read cooldown phase") {
            GameUi::Playing => {
                for seed in read_or_abort(backend.seeds(), "failed to read seed bank").take(MAX_SEED_SLOTS) {
                    if read_or_abort(backend.seed_selection(seed), "failed to read seed selection") == checked {
                        return Some(backend.seed_cooldown_remaining(seed).max(0));
                    }
                }
                None
            }
            GameUi::LevelIntro => read_or_abort(
                backend.chooser_cooldown_remaining(checked),
                "failed to read chooser cooldown",
            ),
            _ => None,
        }
    })
}

/// 根据借用的卡槽事实查找指定卡片对应的 `0-based` 卡槽。
pub fn find_seed_slot(selection: CardSelection) -> Option<SeedSlot>
where
    rsvz_current::CurrentBackend: SeedPacketBackend,
{
    find_seed_for_selection(selection).map(|(slot, _)| slot)
}

/// 根据借用的卡槽事实读取指定 `0-based` 卡槽当前是否可用。
pub fn seed_slot_usable(slot: SeedSlot) -> bool
where
    rsvz_current::CurrentBackend: SeedPacketBackend,
{
    crate::access::with_backend(|backend| {
        seed_for_slot(backend, slot)
            .is_some_and(|seed| crate::live_value::read_or_abort(backend.seed_can_pick_up(seed), "seed_can_pick_up"))
    })
}

/// 在 core 中解释普通/模仿者卡片并计算当前阶段的卡片阳光消耗。
pub fn seed_slot_current_cost(slot: SeedSlot) -> Option<u32>
where
    rsvz_current::CurrentBackend: SeedBankReadBackend + PlantCostBackend,
{
    crate::live_value::read_or_abort(
        crate::access::with_backend(|backend| -> Result<_, rsvz_current::CurrentBackendError> {
            for seed in crate::live_value::read_or_abort(backend.seeds(), "seeds") {
                if backend.seed_slot(seed) != slot {
                    continue;
                }
                return backend
                    .current_plant_cost(crate::live_value::read_or_abort(
                        backend.seed_selection(seed),
                        "seed_selection",
                    ))
                    .map(|cost| Some(cost.as_u32()));
            }
            Ok(None)
        }),
        "failed to read seed_slot_current_cost",
    )
}

fn seed_for_slot<'a>(
    backend: &'a rsvz_current::CurrentBackend, slot: SeedSlot,
) -> Option<<rsvz_current::CurrentBackend as SeedBankReadBackend>::SeedHandle<'a>>
where
    rsvz_current::CurrentBackend: SeedBankReadBackend,
{
    for seed in crate::live_value::read_or_abort(backend.seeds(), "seeds") {
        if backend.seed_slot(seed) == slot {
            return Some(seed);
        }
    }
    None
}

/// Direct C07 plant creation with CardSelection interpretation kept in core.
pub fn new_plant_from_selection(selection: CardSelection, grid: Grid) -> Result<PlantId, CardLogicError>
where
    rsvz_current::CurrentBackend: PlantCreateBackend,
{
    crate::access::with_backend(|backend| {
        let selection = selection
            .checked()
            .map_err(|_error| CoreLogicError::InvalidCardSelection(selection))?;
        let plant = backend
            .new_plant(selection, grid)
            .map_err(|error| CardLogicError::Backend(crate::access::operation_error(error)))?;
        Ok(backend.plant_id(plant))
    })
}

pub fn new_plant(kind: PlantKind, grid: Grid) -> Result<PlantId, CardLogicError>
where
    rsvz_current::CurrentBackend: PlantCreateBackend,
{
    crate::access::with_backend(|backend| {
        let selection = CardSelection::Plant(kind)
            .checked()
            .map_err(|_error| CoreLogicError::InvalidCardSelection(CardSelection::Plant(kind)))?;
        let plant = backend
            .new_plant(selection, grid)
            .map_err(|error| CardLogicError::Backend(crate::access::operation_error(error)))?;
        Ok(backend.plant_id(plant))
    })
}

fn require_usable_seed(selection: CardSelection) -> Result<SeedSlot, CardLogicError>
where
    rsvz_current::CurrentBackend: SeedPacketBackend,
{
    let (slot, usable) = find_seed_for_selection(selection).ok_or(CoreLogicError::SeedSlotNotFound(selection))?;
    if !usable {
        return Err(CoreLogicError::NoUsableSeed.into());
    }
    Ok(slot)
}

/// 种下一张卡，并把已提交的主体或自动容器逐项交给调用者记录。
///
/// 记录器会在每个组件提交后立即运行，因此即使后续主体种植失败，调用者也
/// 能保留已经种下的自动容器 ID。所有失败均转换成与 [`card`] 相同的
/// 结构化 [`CardError`]。
#[doc(hidden)]
pub fn card_recording<F>(selection: CardSelection, row: i32, col: i32, recorder: F) -> CardResult<PlantingReceipt>
where
    rsvz_current::CurrentBackend: CardContext,
    F: FnMut(PlantingComponent, PlantId),
{
    card_detailed_recording(selection, row, col, recorder).map_err(|error| card_error_at(selection, row, col, &error))
}

/// 在脚本侧 `1-based` 格立即尝试种下一张指定卡。
///
/// 成功返回主体植物 ID；所有失败均保留为结构化 [`CardError`]。自动容器
/// 已经成功种下而主体提交失败时不会回滚容器。
pub fn card(selection: CardSelection, row: i32, col: i32) -> CardResult<PlantId>
where
    rsvz_current::CurrentBackend: CardContext,
{
    card_recording(selection, row, col, |_component, _id| {}).map(|receipt| receipt.main)
}

/// 在脚本侧 `1-based` 格尝试使用明确的 `0-based` 卡槽。
///
/// 与 [`card`] 不同，本函数不会按卡片类型查找卡槽，也不会自动补种荷叶或
/// 花盆。所有失败均返回结构化 [`CardError`]。
pub fn card_slot(slot: SeedSlot, row: i32, col: i32) -> CardResult<PlantId>
where
    rsvz_current::CurrentBackend: CardContext,
{
    crate::access::with_backend(|backend| {
        let grid = match Grid::from_one_based(row, col) {
            Ok(grid) => grid,
            Err(_error) => {
                return Err(CardError::new(
                    CardErrorKind::InvalidGrid,
                    format!("种植第 {} 张卡到 ({row}, {col}) 失败：目标格无效", slot.index() + 1),
                ));
            }
        };
        let Some(seed) = seed_for_slot(backend, slot) else {
            return Err(CardError::new(
                CardErrorKind::MissingSlot { slot },
                format!("第 {} 张卡不存在", slot.index() + 1),
            ));
        };
        let selection = backend.seed_selection(seed).unwrap_or_else(|error| {
            crate::diagnostics::abort_operation(RuntimeError::new(format!(
                "读取第 {} 张卡失败：{error}",
                slot.index() + 1
            )))
        });
        plant_seed(slot, grid).map_err(|error| card_error_at(selection.selection(), row, col, &error))
    })
}

/// 按输入顺序执行脚本坐标种卡操作。
///
/// 每项普通拒绝变为 `None`，不阻止后续卡片；backend 故障停止批次，之前
/// 成功种下的植物保留。
pub fn cards<I, T>(ops: I) -> CardResult<Vec<Option<PlantId>>>
where
    rsvz_current::CurrentBackend: CardContext,
    I: IntoIterator<Item = T>,
    T: Into<CardOp>,
{
    let mut plants = Vec::new();
    for op in ops {
        let op = op.into();
        let plant = match card(op.selection, op.row, op.col) {
            Ok(plant) => Some(plant),
            Err(_) => None,
        };
        plants.push(plant);
    }
    Ok(plants)
}

/// 在已经解析为 core [`Grid`] 的候选格中使用一张卡。
///
/// 本函数不再分配或转换候选列表。候选顺序、错误模型和 [`card_any`]
/// 完全相同。
#[doc(hidden)]
pub fn card_any_prepared(selection: CardSelection, grids: &[Grid]) -> CardResult<PlantId>
where
    rsvz_current::CurrentBackend: CardContext,
{
    card_any_prepared_detailed(selection, grids).map_err(|error| card_error_any(selection, grids, &error))
}

/// 按输入顺序在脚本候选格尝试一张指定卡。
///
/// 元组候选从 `1` 开始，类型化 [`Grid`] 候选是 core `0-based`。某一候选
/// 一旦提交成功就不会重试或回滚；所有失败均返回结构化 [`CardError`]。
pub fn card_any<I, G>(selection: impl IntoCardSelection, grids: I) -> CardResult<PlantId>
where
    rsvz_current::CurrentBackend: CardContext,
    I: IntoIterator<Item = G>,
    G: IntoGrid,
{
    let selection = selection.into_card_selection();
    let grids = match parse_candidate_grids(grids) {
        Ok(grids) => grids,
        Err(error) => {
            return Err(card_error_any(selection, &[], &error));
        }
    };
    card_any_prepared(selection, &grids)
}

/// 按候选顺序尝试一个明确的 `0-based` 卡槽。
///
/// 本形式不自动补容器；落点拒绝会继续下一个候选。所有候选失败时返回结构化
/// [`CardError`]。
pub fn card_slot_any<I, G>(slot: SeedSlot, grids: I) -> CardResult<PlantId>
where
    rsvz_current::CurrentBackend: CardContext,
    I: IntoIterator<Item = G>,
    G: IntoGrid,
{
    let mut parsed = Vec::new();
    for grid in grids {
        match grid.try_into_grid() {
            Ok(grid) => parsed.push(grid),
            Err(_error) => {
                return Err(CardError::new(
                    CardErrorKind::InvalidGrid,
                    format!("种植第 {} 张卡失败：候选位置中存在无效坐标", slot.index() + 1),
                ));
            }
        }
    }
    card_slot_any_prepared(slot, &parsed)
}

/// 在已经解析为 core [`Grid`] 的候选格中尝试一个明确卡槽。
///
/// 本函数不再分配或转换候选列表；其他语义与 [`card_slot_any`] 相同。
#[doc(hidden)]
pub fn card_slot_any_prepared(slot: SeedSlot, parsed: &[Grid]) -> CardResult<PlantId>
where
    rsvz_current::CurrentBackend: CardContext,
{
    crate::access::with_backend(|backend| {
        let Some(seed) = seed_for_slot(backend, slot) else {
            return Err(CardError::new(
                CardErrorKind::MissingSlot { slot },
                format!("第 {} 张卡不存在", slot.index() + 1),
            ));
        };
        let selection = backend.seed_selection(seed).unwrap_or_else(|error| {
            crate::diagnostics::abort_operation(RuntimeError::new(format!(
                "读取第 {} 张卡失败：{error}",
                slot.index() + 1
            )))
        });
        let mut last_rejection = None;
        for grid in parsed.iter().copied() {
            match try_plant_seed(slot, grid) {
                PlantSeedOutcome::Planted(plant) => return Ok(plant),
                PlantSeedOutcome::Unusable => {
                    return Err(card_error(
                        selection.selection(),
                        CardTarget::Candidates(parsed),
                        &CardLogicError::Core(CoreLogicError::NoUsableSeed),
                    ));
                }
                PlantSeedOutcome::Rejected(reason) => last_rejection = Some(reason),
            }
        }
        Err(card_error(
            selection.selection(),
            CardTarget::Candidates(parsed),
            &CardLogicError::Core(last_rejection.map_or_else(
                || CoreLogicError::NoPlantableGrid(selection.selection()),
                CoreLogicError::PlantRejected,
            )),
        ))
    })
}

/// 让每张卡依次尝试同一个脚本候选列表。
///
/// 每张卡从第一个候选重新开始；普通失败保留为 `None` 且后续卡片继续，
/// backend 故障停止批次但不回滚此前植物。
pub fn cards_any<C, I, G>(cards: C, grids: I) -> CardResult<Vec<Option<PlantId>>>
where
    rsvz_current::CurrentBackend: CardContext,
    C: IntoCardSelections,
    I: IntoIterator<Item = G>,
    G: IntoGrid,
{
    cards_any_iter(cards.into_card_selections(), grids)
}

/// [`cards_any`] 的迭代器形式，不预先收集卡片选择。
///
/// 因为每张卡都要扫描同一有序列表，候选格仍会统一校验并只收集一次。候选
/// 无效时每张输入卡对应 `None`；backend 故障停止迭代。
pub fn cards_any_iter<C, S, I, G>(cards: C, grids: I) -> CardResult<Vec<Option<PlantId>>>
where
    rsvz_current::CurrentBackend: CardContext,
    C: IntoIterator<Item = S>,
    S: IntoCardSelection,
    I: IntoIterator<Item = G>,
    G: IntoGrid,
{
    let grids = match parse_candidate_grids(grids) {
        Ok(grids) => grids,
        Err(_error) => {
            return Ok(cards.into_iter().map(|_selection| None).collect());
        }
    };
    let cards = cards.into_iter();
    let mut planted = Vec::with_capacity(cards.size_hint().0);
    for selection in cards {
        let selection = selection.into_card_selection();
        let outcome = match card_any_prepared(selection, &grids) {
            Ok(plant) => Some(plant),
            Err(_) => None,
        };
        planted.push(outcome);
    }
    Ok(planted)
}

fn parse_candidate_grids<I, G>(grids: I) -> Result<Vec<Grid>, CardLogicError>
where
    I: IntoIterator<Item = G>,
    G: IntoGrid,
{
    grids
        .into_iter()
        .map(IntoGrid::try_into_grid)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_error| CardLogicError::Core(CoreLogicError::InvalidGrid))
}

/// 在一个目标格子上尝试种下第一张可用卡。
pub fn plant_first_available(
    candidates: impl IntoIterator<Item = CardSelection>, grid: Grid,
) -> Result<Option<SeedSlot>, CardLogicError>
where
    rsvz_current::CurrentBackend: CardPlantingBackend,
{
    for candidate in candidates {
        let Some((slot, usable)) = find_seed_for_selection(candidate) else {
            continue;
        };
        if !usable {
            continue;
        }
        if matches!(can_plant_seed(slot, grid), Plantability::Allowed) {
            plant_seed(slot, grid)?;
            return Ok(Some(slot));
        }
    }
    Ok(None)
}

/// 按顺序在每个当前可种的 core 格尝试使用 `selection`。
///
/// 卡片变为不可用或不存在时停止；不可种的格会跳过。返回实际成功种植的
/// `0-based` 格，已完成的种植不会因后续失败而回滚。
pub fn plant_all_available(
    selection: CardSelection, grids: impl IntoIterator<Item = Grid>,
) -> Result<Vec<Grid>, CardLogicError>
where
    rsvz_current::CurrentBackend: CardPlantingBackend,
{
    let mut planted = Vec::new();
    for grid in grids {
        let Some((slot, usable)) = find_seed_for_selection(selection) else {
            break;
        };
        if !usable {
            break;
        }
        if matches!(can_plant_seed(slot, grid), Plantability::Allowed) {
            plant_seed(slot, grid)?;
            planted.push(grid);
        }
    }
    Ok(planted)
}

#[cfg(test)]
mod tests;

mod planting;
pub use planting::{PlantSeedOutcome, can_plant_seed, plant_seed, try_plant_seed};
use planting::{card_any_prepared_detailed, card_detailed_recording};

pub(crate) use planting::plant_counts_as_on_lawn;
