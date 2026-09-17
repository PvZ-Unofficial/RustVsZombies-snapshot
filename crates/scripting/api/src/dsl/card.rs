//! Low DSL 种卡表达式、植物别名与定时清理。
//!
//! 本模块的 [`card`] 和 [`try_card`] 返回 [`Expr`]，构造时不会种卡。表达式
//! 的语义时间是实际执行种卡的时刻，而不是植物爆炸、冰冻等效果生效时刻；
//! `a`、`j`、`n`、`i` 等效果别名会在此基础上自行应用生效时间偏移。
//!
//! [`PlantKind`]、[`CardSelection`] 与普通植物别名按卡片语义查找卡槽，并在
//! 地形需要时自动补种荷叶或花盆；[`SeedSlot`] 从 `0` 开始，严格使用指定
//! 卡槽且不自动补种容器。脚本行列从 `1` 开始，类型化 [`Grid`] 则是 core
//! `0-based` 值。
//!
//! `card` 把普通种植失败作为当前 operation 的错误报告；`try_card` 把缺卡、
//! 不可用和无可种位置等普通失败视为成功的空操作。backend 故障在两者中
//! 都是运行错误。多卡会降低为同一语义时刻的多个 operation，因此其中一张
//! 运行失败不会阻止同帧后续卡片。

use rsvz_current::CurrentBackend;
use std::rc::Rc;

use crate::runtime::RuntimeResult;
use rsvz_backend_api::backend::ImitatorMorphBackend;
use rsvz_game::logic::IntoGrid;
pub use rsvz_game::logic::card_timing::Retention;
use rsvz_game::logic::cards::{CardContext, CardOp, CardResult, IntoCardSelection};
use rsvz_model::model::{CardSelection, Grid, PlantKind, SeedSlot};

use crate::core as core_cards;

use super::expr::{Expr, Leaf, PrepareContext};

/// 在种卡表达式的语义时间后 `frames` 厘秒删除本次新种下的植物与自动容器。
///
/// 清理时间早于种植时间或发生时间溢出会成为 DSL 注册错误。
#[must_use]
pub const fn keep(frames: i32) -> Retention {
    Retention::keep(frames)
}

/// 在所绑定波次的绝对波内时间 `time` 删除本次新种下的植物与自动容器。
///
/// 例如 `star(to(749), 2, 9)` 在表达式语义时间种杨桃，并在该波 `749cs`
/// 清理。清理时间早于种植时间会成为 DSL 注册错误。
#[must_use]
pub const fn to(time: i32) -> Retention {
    Retention::to(time)
}

/// 一个可调用的 DSL 卡片别名。
///
/// 普通和模仿者别名使用相同类型；调用 `alias(row, col)` 构造固定格种卡
/// 表达式，调用 `alias(retention, row, col)` 可附加 [`keep`] 或 [`to`]
/// 清理策略。别名调用本身不会立即种卡。
pub struct CardAlias {
    name: &'static str,
    selection: CardSelection,
    plant: Option<PlantKind>,
}

impl Clone for CardAlias {
    fn clone(&self) -> Self {
        *self
    }
}

impl Copy for CardAlias {}

impl std::fmt::Debug for CardAlias {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CardAlias")
            .field("name", &self.name)
            .field("selection", &self.selection)
            .finish()
    }
}

impl CardAlias {
    const fn ordinary(name: &'static str, kind: PlantKind) -> Self {
        Self {
            name,
            selection: CardSelection::Plant(kind),
            plant: Some(kind),
        }
    }

    const fn imitator(name: &'static str, kind: PlantKind) -> Self {
        Self {
            name,
            selection: CardSelection::Imitator(kind),
            plant: None,
        }
    }

    /// 返回该别名表示的普通或模仿者卡片选择。
    #[must_use]
    pub const fn selection(self) -> CardSelection {
        self.selection
    }

    pub(super) fn plant_kind(self) -> Result<PlantKind, String> {
        self.plant
            .ok_or_else(|| format!("imitator alias {} has no plain PlantKind semantics", self.name))
    }
}

impl IntoCardSelection for CardAlias {
    fn into_card_selection(self) -> CardSelection {
        self.selection
    }
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DslCardSource {
    Selection(CardSelection),
    Slot(SeedSlot),
}

/// 转换 DSL 单张卡来源。
///
/// 内置支持 [`PlantKind`]、[`CardSelection`]、[`SeedSlot`] 和 [`CardAlias`]。
/// 普通用户通常无需手动实现或调用该 trait。
pub trait IntoDslCardSource {
    /// 转换为按卡片语义查找的选择，或明确的 `0-based` 卡槽。
    fn into_dsl_card_source(self) -> DslCardSource;
}

impl IntoDslCardSource for CardSelection {
    fn into_dsl_card_source(self) -> DslCardSource {
        DslCardSource::Selection(self)
    }
}

impl IntoDslCardSource for PlantKind {
    fn into_dsl_card_source(self) -> DslCardSource {
        DslCardSource::Selection(self.into_card_selection())
    }
}

impl IntoDslCardSource for SeedSlot {
    fn into_dsl_card_source(self) -> DslCardSource {
        DslCardSource::Slot(self)
    }
}

impl IntoDslCardSource for CardAlias {
    fn into_dsl_card_source(self) -> DslCardSource {
        DslCardSource::Selection(self.selection)
    }
}

/// 转换 DSL 单卡或同质卡片列表。
///
/// 支持单个 [`IntoDslCardSource`]，以及 `Vec<T>`、`[T; N]`、`&[T]`。
/// 输入顺序就是同一语义时刻的 operation 顺序。
pub trait IntoDslCardSources {
    /// 按输入顺序转换为 DSL 卡片来源列表。
    fn into_dsl_card_sources(self) -> Vec<DslCardSource>;
}

impl<T> IntoDslCardSources for T
where
    T: IntoDslCardSource,
{
    fn into_dsl_card_sources(self) -> Vec<DslCardSource> {
        vec![self.into_dsl_card_source()]
    }
}

impl<T, const N: usize> IntoDslCardSources for [T; N]
where
    T: IntoDslCardSource,
{
    fn into_dsl_card_sources(self) -> Vec<DslCardSource> {
        self.into_iter().map(IntoDslCardSource::into_dsl_card_source).collect()
    }
}

impl<T> IntoDslCardSources for Vec<T>
where
    T: IntoDslCardSource,
{
    fn into_dsl_card_sources(self) -> Vec<DslCardSource> {
        self.into_iter().map(IntoDslCardSource::into_dsl_card_source).collect()
    }
}

impl<T> IntoDslCardSources for &[T]
where
    T: IntoDslCardSource + Copy,
{
    fn into_dsl_card_sources(self) -> Vec<DslCardSource> {
        self.iter()
            .copied()
            .map(IntoDslCardSource::into_dsl_card_source)
            .collect()
    }
}

crate::callable::impl_callable! {
    impl<> CardAlias
    where {
        CurrentBackend: CardContext + 'static,
    }
    call_as(alias; row: i32, col: i32) -> Expr {
        fixed_card(alias.selection, row, col)
    }
}

crate::callable::impl_callable! {
    impl<> CardAlias
    where {
        CurrentBackend: CardContext + ImitatorMorphBackend + 'static,
    }
    call_as(alias; retention: Retention, row: i32, col: i32) -> Expr {
        retained_card(alias.selection, row, col, retention)
    }
}

macro_rules! card_alias_table {
    ($mac:ident) => {
        $mac! {
            pea, m_pea => Peashooter;
            sunflower, m_sunflower => Sunflower;
            cherry, m_cherry => CherryBomb;
            wallnut, m_wallnut => WallNut;
            mine, m_mine => PotatoMine;
            snow, m_snow => SnowPea;
            chomper, m_chomper => Chomper;
            repeater, m_repeater => Repeater;
            puff, m_puff => PuffShroom;
            sunshroom, m_sunshroom => SunShroom;
            fume, m_fume => FumeShroom;
            grave, m_grave => GraveBuster;
            hypno, m_hypno => HypnoShroom;
            scaredy, m_scaredy => ScaredyShroom;
            ice, m_ice => IceShroom;
            doom, m_doom => DoomShroom;
            lily, m_lily => LilyPad;
            squash, m_squash => Squash;
            threepeater, m_threepeater => Threepeater;
            kelp, m_kelp => TangleKelp;
            jalapeno, m_jalapeno => Jalapeno;
            spike, m_spike => Spikeweed;
            torch, m_torch => Torchwood;
            tallnut, m_tallnut => TallNut;
            sea, m_sea => SeaShroom;
            plantern, m_plantern => Plantern;
            cactus, m_cactus => Cactus;
            blover, m_blover => Blover;
            split, m_split => SplitPea;
            star, m_star => Starfruit;
            pumpkin, m_pumpkin => Pumpkin;
            magnet, m_magnet => MagnetShroom;
            cabbage, m_cabbage => CabbagePult;
            pot, m_pot => FlowerPot;
            kernel, m_kernel => KernelPult;
            coffee, m_coffee => CoffeeBean;
            garlic, m_garlic => Garlic;
            umbrella, m_umbrella => UmbrellaLeaf;
            marigold, m_marigold => Marigold;
            melon, m_melon => MelonPult;
            gatling, m_gatling => GatlingPea;
            twin_sun, m_twin_sun => TwinSunflower;
            gloom, m_gloom => GloomShroom;
            cattail, m_cattail => Cattail;
            winter, m_winter => WinterMelon;
            gold, m_gold => GoldMagnet;
            spikerock, m_spikerock => Spikerock;
            cob, m_cob => CobCannon;
        }
    };
}

macro_rules! define_card_aliases {
    ($($ordinary:ident, $imitator:ident => $kind:ident;)+) => {
        $(
            #[allow(non_upper_case_globals, reason = "script DSL aliases intentionally use lowercase names")]
            pub const $ordinary: CardAlias =
                CardAlias::ordinary(stringify!($ordinary), PlantKind::$kind);
            #[allow(non_upper_case_globals, reason = "script DSL aliases intentionally use lowercase names")]
            pub const $imitator: CardAlias =
                CardAlias::imitator(stringify!($imitator), PlantKind::$kind);
        )+

        #[doc(hidden)]
        pub const ORDINARY_ALIASES: [CardAlias; 48] = [$($ordinary),+];
        #[doc(hidden)]
        pub const IMITATOR_ALIASES: [CardAlias; 48] = [$($imitator),+];
    };
}

card_alias_table!(define_card_aliases);

crate::callable::callable_api! {
    #[doc(alias = "Card")]
    /// 构造必需成功的种卡表达式。
    ///
    /// 调用本身不会种卡；表达式必须连接到波次时间。其语义时间是实际种卡
    /// 时刻。普通种植失败会在执行时报告为当前 operation 错误，同帧其他
    /// operation 仍继续。
    ///
    /// # 调用形式
    ///
    /// ```rust,no_run
    /// # #[cfg(any(feature = "pvz-emulator", feature = "pvz-1-0-0-1051", feature = "pvz-portable"))]
    /// # {
    /// # use rsvz::dsl::prelude::*;
    /// let fixed = card(PlantKind::CherryBomb, 2, 9);
    /// let stacked = card([PlantKind::LilyPad, PlantKind::DoomShroom], 3, 4);
    /// let candidates = card(PlantKind::CherryBomb, [(2, 9), (5, 9)]);
    /// let separate = card([
    ///     (PlantKind::CherryBomb, 2, 9),
    ///     (PlantKind::Jalapeno, 5, 9),
    /// ]);
    /// let retained = card(to(749), PlantKind::Starfruit, 2, 9);
    /// let alias_retained = star(keep(300), 2, 9);
    /// # let _ = (fixed, stacked, candidates, separate, retained, alias_retained);
    /// # }
    /// ```
    ///
    /// 多张卡共用候选格时，每张卡都从候选列表开头依次尝试；前一张卡造成的
    /// 场地变化会影响后一张卡。构造时发现的无效卡片或坐标会使该表达式只
    /// 产生注册错误，不会注册其中一部分种卡操作。
    pub card: DslCard;

    where {
        CurrentBackend: CardContext + 'static,
    }

    impl<S>
    where {
        S: IntoDslCardSources,
    }
    call(source: S, row: i32, col: i32) -> Expr {
        fixed_card_sources(source.into_dsl_card_sources(), row, col)
    }

    impl<S>
    where {
        CurrentBackend: ImitatorMorphBackend,
        S: IntoCardSelection,
    }
    call(retention: Retention, selection: S, row: i32, col: i32) -> Expr {
        retained_card(selection.into_card_selection(), row, col, retention)
    }

    impl<S, I, G>
    where {
        S: IntoDslCardSources,
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    }
    call(source: S, grids: I) -> Expr {
        candidate_card_sources(source.into_dsl_card_sources(), grids, CandidateMode::Required)
    }

    impl<I, T>
    where {
        I: IntoIterator<Item = T>,
        T: crate::cards::IntoCardOp,
    }
    call(ops: I) -> Expr {
        card_batch(ops)
    }
}

crate::callable::callable_api! {
    /// 构造允许普通种植失败的可选种卡表达式。
    ///
    /// 该函数仍返回 [`Expr`]，不是 `RuntimeResult`。缺卡、卡片不可用、位置
    /// 不可种或候选格全部失败时，执行结果是成功的空操作；backend 故障仍会
    /// 报告为运行错误。支持固定格和候选格两种形式，也支持单卡或卡片列表。
    ///
    /// ```rust,no_run
    /// # #[cfg(any(feature = "pvz-emulator", feature = "pvz-1-0-0-1051", feature = "pvz-portable"))]
    /// # {
    /// # use rsvz::dsl::prelude::*;
    /// let optional = try_card(PlantKind::PuffShroom, 1, 9);
    /// let candidates = try_card(PlantKind::CherryBomb, [(2, 9), (5, 9)]);
    /// # let _ = (optional, candidates);
    /// # }
    /// ```
    pub try_card: DslTryCard;

    where {
        CurrentBackend: CardContext + 'static,
    }

    impl<S>
    where {
        S: IntoDslCardSources,
    }
    call(source: S, row: i32, col: i32) -> Expr {
        candidate_card_sources(source.into_dsl_card_sources(), [(row, col)], CandidateMode::Optional)
    }

    impl<S, I, G>
    where {
        S: IntoDslCardSources,
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    }
    call(source: S, grids: I) -> Expr {
        candidate_card_sources(source.into_dsl_card_sources(), grids, CandidateMode::Optional)
    }
}

fn fixed_card(selection: CardSelection, row: i32, col: i32) -> Expr
where
    CurrentBackend: CardContext + 'static,
{
    if let Some(error) = fixed_card_error(selection, row, col) {
        return error;
    }
    Expr::from_leaf(CardLeaf { selection, row, col })
}

fn fixed_card_sources(sources: Vec<DslCardSource>, row: i32, col: i32) -> Expr
where
    CurrentBackend: CardContext + 'static,
{
    let mut errors = sources
        .iter()
        .filter_map(|source| validate_dsl_source(*source).err())
        .collect::<Vec<_>>();
    if Grid::from_one_based(row, col).is_err() {
        errors.push(format!("invalid card grid ({row}, {col})"));
    }
    if !errors.is_empty() {
        return errors
            .into_iter()
            .fold(Expr::empty(), |expr, error| expr + Expr::error(error));
    }
    if sources.is_empty() {
        Expr::empty()
    } else {
        Expr::from_leaf(CardSourcesLeaf {
            sources: Rc::from(sources.into_boxed_slice()),
            row,
            col,
        })
    }
}

fn retained_card(selection: CardSelection, row: i32, col: i32, retention: Retention) -> Expr
where
    CurrentBackend: CardContext + ImitatorMorphBackend + 'static,
{
    if let Some(error) = fixed_card_error(selection, row, col) {
        return error;
    }
    Expr::from_leaf(RetainedCardLeaf(rsvz_game::cards::effect::RetainedCardEffect {
        selection,
        row,
        col,
        retention,
    }))
}

fn fixed_card_error(selection: CardSelection, row: i32, col: i32) -> Option<Expr> {
    let selection = validate_dsl_selection(selection).err();
    let grid = Grid::from_one_based(row, col)
        .err()
        .map(|_error| format!("invalid card grid ({row}, {col})"));
    match (selection, grid) {
        (Some(selection), Some(grid)) => Some(Expr::error(selection) + Expr::error(grid)),
        (Some(error), None) | (None, Some(error)) => Some(Expr::error(error)),
        (None, None) => None,
    }
}

struct CardLeaf {
    selection: CardSelection,
    row: i32,
    col: i32,
}

impl Leaf for CardLeaf
where
    CurrentBackend: CardContext + 'static,
{
    fn prepare(&self, context: &mut PrepareContext<'_>) {
        let selection = self.selection;
        let row = self.row;
        let col = self.col;
        context.push(
            context.semantic_time(),
            Box::new(move || core_cards::card(selection, row, col).map(|_id| ()).map_err(Into::into)),
        );
    }
}

struct CardSourcesLeaf {
    sources: Rc<[DslCardSource]>,
    row: i32,
    col: i32,
}

impl Leaf for CardSourcesLeaf
where
    CurrentBackend: CardContext + 'static,
{
    fn prepare(&self, context: &mut PrepareContext<'_>) {
        for source in self.sources.iter().copied() {
            let row = self.row;
            let col = self.col;
            context.push(
                context.semantic_time(),
                Box::new(move || plant_source_at(source, row, col, CandidateMode::Required)),
            );
        }
    }
}

struct RetainedCardLeaf(rsvz_game::cards::effect::RetainedCardEffect);

impl Leaf for RetainedCardLeaf
where
    CurrentBackend: CardContext + ImitatorMorphBackend + 'static,
{
    fn prepare(&self, context: &mut PrepareContext<'_>) {
        let result = self
            .0
            .prepare(context.semantic_time(), |time, callback| context.push(time, callback));
        super::effect::report_time_errors(context, result);
    }
}

#[derive(Clone, Copy)]
enum CandidateMode {
    Required,
    Optional,
}

fn candidate_card_sources<I, G>(sources: Vec<DslCardSource>, grids: I, mode: CandidateMode) -> Expr
where
    CurrentBackend: CardContext + 'static,
    I: IntoIterator<Item = G>,
    G: IntoGrid,
{
    let mut errors = sources
        .iter()
        .filter_map(|source| validate_dsl_source(*source).err())
        .collect::<Vec<_>>();
    let mut parsed_grids = Vec::new();
    let mut saw_grid = false;
    for grid in grids {
        saw_grid = true;
        match grid.try_into_grid() {
            Ok(grid) => parsed_grids.push(grid),
            Err(_error) => errors.push("invalid candidate card grid".to_owned()),
        }
    }
    if !saw_grid {
        errors.push("candidate card grid list cannot be empty".to_owned());
    }
    if errors.is_empty() && !sources.is_empty() {
        Expr::from_leaf(CandidateCardsLeaf {
            sources: Rc::from(sources.into_boxed_slice()),
            grids: Rc::from(parsed_grids.into_boxed_slice()),
            mode,
        })
    } else if errors.is_empty() {
        Expr::empty()
    } else {
        errors
            .into_iter()
            .fold(Expr::empty(), |expr, error| expr + Expr::error(error))
    }
}

struct CandidateCardsLeaf {
    sources: Rc<[DslCardSource]>,
    grids: Rc<[Grid]>,
    mode: CandidateMode,
}

impl Leaf for CandidateCardsLeaf
where
    CurrentBackend: CardContext + 'static,
{
    fn prepare(&self, context: &mut PrepareContext<'_>) {
        for source in self.sources.iter().copied() {
            let grids = Rc::clone(&self.grids);
            let mode = self.mode;
            context.push(
                context.semantic_time(),
                Box::new(move || plant_source_any(source, &grids, mode)),
            );
        }
    }
}

fn plant_source_at(source: DslCardSource, row: i32, col: i32, mode: CandidateMode) -> RuntimeResult<()>
where
    CurrentBackend: CardContext,
{
    match source {
        DslCardSource::Selection(selection) => card_result(core_cards::card(selection, row, col), mode),
        DslCardSource::Slot(slot) => card_result(core_cards::card_slot(slot, row, col), mode),
    }
}

fn plant_source_any(source: DslCardSource, grids: &[Grid], mode: CandidateMode) -> RuntimeResult<()>
where
    CurrentBackend: CardContext,
{
    match source {
        DslCardSource::Selection(selection) => card_result(core_cards::card_any_prepared(selection, grids), mode),
        DslCardSource::Slot(slot) => card_result(core_cards::card_slot_any_prepared(slot, grids), mode),
    }
}

fn card_result<T>(result: CardResult<T>, mode: CandidateMode) -> RuntimeResult<()> {
    match result {
        Ok(_) => Ok(()),
        Err(_) if matches!(mode, CandidateMode::Optional) => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn validate_dsl_source(source: DslCardSource) -> Result<(), String> {
    match source {
        DslCardSource::Selection(selection) => validate_dsl_selection(selection),
        DslCardSource::Slot(_) => Ok(()),
    }
}

fn card_batch<I, T>(ops: I) -> Expr
where
    CurrentBackend: CardContext + 'static,
    I: IntoIterator<Item = T>,
    T: crate::cards::IntoCardOp,
{
    let ops = ops
        .into_iter()
        .map(crate::cards::IntoCardOp::into_card_op)
        .collect::<Vec<_>>();
    let errors = ops
        .iter()
        .flat_map(|op| {
            let selection = validate_dsl_selection(op.selection).err();
            let grid = Grid::from_one_based(op.row, op.col)
                .err()
                .map(|_error| format!("invalid card grid ({}, {})", op.row, op.col));
            selection.into_iter().chain(grid)
        })
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        return errors
            .into_iter()
            .fold(Expr::empty(), |expr, error| expr + Expr::error(error));
    }
    if ops.is_empty() {
        Expr::empty()
    } else {
        Expr::from_leaf(CardBatchLeaf {
            ops: Rc::from(ops.into_boxed_slice()),
        })
    }
}

fn validate_dsl_selection(selection: CardSelection) -> Result<(), String> {
    core_cards::validate_card_selection(&[selection]).map_err(|error| error.to_string())
}

struct CardBatchLeaf {
    ops: Rc<[CardOp]>,
}

impl Leaf for CardBatchLeaf
where
    CurrentBackend: CardContext + 'static,
{
    fn prepare(&self, context: &mut PrepareContext<'_>) {
        for op in self.ops.iter().copied() {
            context.push(
                context.semantic_time(),
                Box::new(move || {
                    core_cards::card(op.selection, op.row, op.col)
                        .map(|_plant| ())
                        .map_err(Into::into)
                }),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alias_tables_are_complete_and_share_one_type() {
        assert_eq!(ORDINARY_ALIASES.len(), 48);
        assert_eq!(IMITATOR_ALIASES.len(), 48);
        assert_eq!(ice.selection(), CardSelection::Plant(PlantKind::IceShroom));
        assert_eq!(m_ice.selection(), CardSelection::Imitator(PlantKind::IceShroom));
        assert_eq!(split.plant_kind(), Ok(PlantKind::SplitPea));
        assert!(m_ice.plant_kind().is_err());
    }
    #[test]
    fn retention_is_const_and_uses_absolute_or_relative_time() {
        const SHORT: Retention = keep(266);
        const ABSOLUTE: Retention = to(1715);
        assert_eq!(SHORT.cleanup_time(100, 100), Ok(366));
        assert_eq!(ABSOLUTE.cleanup_time(900, 900), Ok(1715));
        assert!(keep(-1).cleanup_time(100, 100).is_err());
    }
    #[test]
    fn dsl_card_selection_rejects_bare_and_nested_imitator() {
        assert!(validate_dsl_selection(CardSelection::Plant(PlantKind::Imitator)).is_err());
        assert!(validate_dsl_selection(CardSelection::Imitator(PlantKind::Imitator)).is_err());
        assert!(validate_dsl_selection(CardSelection::Imitator(PlantKind::FumeShroom)).is_ok());
    }
}
