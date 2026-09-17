//! 即时种卡 API。
//!
//! [`Card`] (`card`) 和 [`TryCard`] (`try_card`) 在被调用时立即尝试种卡；
//! 需要构造 low DSL 时间轴时，应使用 [`crate::dsl::DslCard`] (`dsl::card`)。
//! 脚本形式的 `(row, col)` 从 `1`
//! 开始，而 [`rsvz_model::model::Grid`] 是 core 使用的 `0-based` 值。
//!
//! # 输入与返回值
//!
//! | 输入 | 返回值 | 行为 |
//! | --- | --- | --- |
//! | [`PlantKind`] / [`CardSelection`] | 单卡结果 | 按卡片语义查找卡槽；需要时自动补种荷叶或花盆 |
//! | [`SeedSlot`] | 单卡结果 | 严格使用指定的 `0-based` 卡槽，不自动补种容器 |
//! | `Vec<T>`、`[T; N]`、`&[T]` | `Vec<Option<PlantId>>` | 按输入顺序种下多张 `PlantKind` / `CardSelection` |
//! | `[(source, row, col), ...]` | `Vec<Option<PlantId>>` | 每个 `PlantKind` / `CardSelection` 使用自己的目标格；不支持多卡槽形式 |
//!
//! 将一张或多张卡与候选格列表一起传入时，每张卡都会从候选列表开头
//! 依次尝试；前一张卡造成的场地变化会影响后一张卡。
//!
//! 按卡片类型调用时会根据场景需要自动补种荷叶或花盆；显式 [`SeedSlot`]
//! 则严格使用指定卡槽，不自动补种容器。所有操作都通过 backend 语义执行，
//! 不移动鼠标。
//!
//! # 错误模型
//!
//! 单卡 `try_card` 成功时返回 `PlantId`，失败时返回可匹配的 [`CardError`]。
//! 单卡 `card` 报告规则拒绝并返回 `Option<PlantId>`。批量普通失败以
//! `None` 表示且不会阻止后续卡片；必要读取或访问故障结束当前 callback，
//! 已经种下的植物不会回滚。

use rsvz_current::CurrentBackend;
pub use rsvz_game::cards::set_plant_active_time;
use rsvz_game::cards::{report_card, report_cards};
pub use rsvz_game::logic::cards::{CardError, CardErrorKind, CardResult};
use rsvz_game::logic::cards::{CardOp, IntoCardSelection};
use rsvz_game::logic::{IntoGrid, cards::CardContext};
use rsvz_model::model::{CardSelection, PlantId, PlantKind, SeedSlot};

/// 将脚本侧 `(selection, row, col)` 转换成一项批量种卡操作。
///
/// `row` 和 `col` 都是从 `1` 开始的脚本坐标。通常直接传三元组即可，
/// 不需要手动调用这个 trait。
pub trait IntoCardOp {
    /// 转换为一项使用脚本侧 `1-based` 行列的种卡操作。
    fn into_card_op(self) -> CardOp;
}

impl IntoCardOp for CardOp {
    fn into_card_op(self) -> CardOp {
        self
    }
}

impl<S> IntoCardOp for (S, i32, i32)
where
    S: IntoCardSelection,
{
    fn into_card_op(self) -> CardOp {
        let (selection, row, col) = self;
        CardOp {
            selection: selection.into_card_selection(),
            row,
            col,
        }
    }
}

/// [`Card`] (`card`) 与 [`TryCard`] (`try_card`) 的卡片输入。
///
/// 内置实现及其返回值见本模块的“输入与返回值”表。这个 trait 同时负责把
/// 单卡与多卡输入映射到相应的输出形状。
pub trait CardSource {
    /// `try_card` 的成功类型。单卡为 `PlantId`，多卡为
    /// `Vec<Option<PlantId>>`。
    type TryOutput;

    /// `card` 的返回类型。单卡为 `Option<PlantId>`，多卡为
    /// `Vec<Option<PlantId>>`。
    type Output;

    /// 在一个 `1-based` 脚本格立即尝试使用输入卡片。
    fn try_at(self, row: i32, col: i32) -> CardResult<Self::TryOutput>
    where
        CurrentBackend: CardContext;

    /// 在有序候选格中立即尝试使用输入卡片。
    fn try_any<I, G>(self, grids: I) -> CardResult<Self::TryOutput>
    where
        CurrentBackend: CardContext,
        I: IntoIterator<Item = G>,
        G: IntoGrid;

    /// 实现非 `try_` 接口的统一错误报告与默认返回值。
    fn report(result: CardResult<Self::TryOutput>) -> Self::Output;
}

macro_rules! impl_selection_card_source {
    ($source:ty) => {
        impl CardSource for $source {
            type TryOutput = PlantId;
            type Output = Option<PlantId>;

            fn try_at(self, row: i32, col: i32) -> CardResult<Self::TryOutput>
            where
                CurrentBackend: CardContext,
            {
                rsvz_game::cards::try_card(self.into_card_selection(), row, col)
            }

            fn try_any<I, G>(self, grids: I) -> CardResult<Self::TryOutput>
            where
                CurrentBackend: CardContext,
                I: IntoIterator<Item = G>,
                G: IntoGrid,
            {
                rsvz_game::cards::try_card_any(self.into_card_selection(), grids)
            }

            fn report(result: CardResult<Self::TryOutput>) -> Self::Output {
                report_card(result)
            }
        }
    };
}

impl_selection_card_source!(PlantKind);
impl_selection_card_source!(CardSelection);

impl CardSource for SeedSlot {
    type TryOutput = PlantId;
    type Output = Option<PlantId>;

    fn try_at(self, row: i32, col: i32) -> CardResult<Self::TryOutput>
    where
        CurrentBackend: CardContext,
    {
        rsvz_game::cards::try_card_slot(self, row, col)
    }

    fn try_any<I, G>(self, grids: I) -> CardResult<Self::TryOutput>
    where
        CurrentBackend: CardContext,
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    {
        rsvz_game::cards::try_card_slot_any(self, grids)
    }

    fn report(result: CardResult<Self::TryOutput>) -> Self::Output {
        report_card(result)
    }
}

impl<T> CardSource for Vec<T>
where
    T: IntoCardSelection,
{
    type TryOutput = Vec<Option<PlantId>>;
    type Output = Vec<Option<PlantId>>;

    fn try_at(self, row: i32, col: i32) -> CardResult<Self::TryOutput>
    where
        CurrentBackend: CardContext,
    {
        rsvz_game::cards::try_cards(self.into_iter().map(|selection| CardOp {
            selection: selection.into_card_selection(),
            row,
            col,
        }))
    }

    fn try_any<I, G>(self, grids: I) -> CardResult<Self::TryOutput>
    where
        CurrentBackend: CardContext,
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    {
        rsvz_game::cards::try_cards_any_iter(self, grids)
    }

    fn report(result: CardResult<Self::TryOutput>) -> Self::Output {
        report_cards(result)
    }
}

impl<T, const N: usize> CardSource for [T; N]
where
    T: IntoCardSelection,
{
    type TryOutput = Vec<Option<PlantId>>;
    type Output = Vec<Option<PlantId>>;

    fn try_at(self, row: i32, col: i32) -> CardResult<Self::TryOutput>
    where
        CurrentBackend: CardContext,
    {
        rsvz_game::cards::try_cards(self.into_iter().map(|selection| CardOp {
            selection: selection.into_card_selection(),
            row,
            col,
        }))
    }

    fn try_any<I, G>(self, grids: I) -> CardResult<Self::TryOutput>
    where
        CurrentBackend: CardContext,
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    {
        rsvz_game::cards::try_cards_any_iter(self, grids)
    }

    fn report(result: CardResult<Self::TryOutput>) -> Self::Output {
        report_cards(result)
    }
}

impl<T> CardSource for &[T]
where
    T: IntoCardSelection + Copy,
{
    type TryOutput = Vec<Option<PlantId>>;
    type Output = Vec<Option<PlantId>>;

    fn try_at(self, row: i32, col: i32) -> CardResult<Self::TryOutput>
    where
        CurrentBackend: CardContext,
    {
        rsvz_game::cards::try_cards(self.iter().copied().map(|selection| CardOp {
            selection: selection.into_card_selection(),
            row,
            col,
        }))
    }

    fn try_any<I, G>(self, grids: I) -> CardResult<Self::TryOutput>
    where
        CurrentBackend: CardContext,
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    {
        rsvz_game::cards::try_cards_any_iter(self.iter().copied(), grids)
    }

    fn report(result: CardResult<Self::TryOutput>) -> Self::Output {
        report_cards(result)
    }
}

/// Normalizes the active ice- or doom-shroom at one exact grid.
pub use rsvz_game::cards::try_normalize_card_effect;

/// Normalizes an active ice- or doom-shroom at one grid, reporting failures.
pub use rsvz_game::cards::normalize_card_effect;

/// Reads native cooldown in battle or Survival repick. Required read failures
/// end the current callback; no fallback cooldown is returned after an error.
pub use rsvz_game::cards::card_cd;

/// Enables or disables native sun-cost checks without reporting failures.
pub use rsvz_game::cards::try_set_sun_cost_ignored;

/// Enables or disables native sun-cost checks, reporting failures and returning.
pub use rsvz_game::cards::set_sun_cost_ignored;

crate::callable::callable_api! {
    #[doc(alias = "ACard")]
    /// 立即尝试使用一张或多张卡片，不自动报告失败。
    ///
    /// # 调用形式
    ///
    /// ```rust,no_run
    /// # #[cfg(any(feature = "pvz-emulator", feature = "pvz-1-0-0-1051", feature = "pvz-portable"))]
    /// # {
    /// # use rsvz::prelude::*;
    /// let one: CardResult<PlantId> =
    ///     try_card(PlantKind::CherryBomb, 2, 9);
    /// let same_grid: CardResult<Vec<Option<PlantId>>> =
    ///     try_card([PlantKind::LilyPad, PlantKind::DoomShroom], 3, 4);
    /// let candidates: CardResult<PlantId> =
    ///     try_card(PlantKind::CherryBomb, [(2, 9), (5, 9)]);
    /// let separate: CardResult<Vec<Option<PlantId>>> =
    ///     try_card([(PlantKind::CherryBomb, 2, 9), (PlantKind::Jalapeno, 5, 9)]);
    ///
    /// // SeedSlot 从 0 开始：0 表示第一张卡。
    /// let first_slot = SeedSlot::new(0).unwrap();
    /// let exact: CardResult<PlantId> = try_card(first_slot, 2, 9);
    /// # let _ = (one, same_grid, candidates, separate, exact);
    /// # }
    /// ```
    ///
    /// # 返回值与错误
    ///
    /// 单卡成功返回 `PlantId`，失败返回可匹配的 [`CardError`]。批量操作把
    /// 普通单项失败保留为 `None` 并继续；必要读取或访问故障直接结束当前
    /// callback，不返回可恢复错误，也不回滚此前成功种下的植物。
    ///
    /// 需要自动报告失败并继续当前 callback 时使用 [`card`]。
    pub try_card: TryCard;

    where {
        CurrentBackend: CardContext,
    }


    impl<S>
    where {
        S: CardSource,
    }
    call(source: S, row: i32, col: i32) -> CardResult<S::TryOutput> {
        source.try_at(row, col)
    }

    impl<I, T>
    where {
        I: IntoIterator<Item = T>,
        T: IntoCardOp,
    }
    call(ops: I) -> CardResult<Vec<Option<PlantId>>> {
        rsvz_game::cards::try_cards(ops.into_iter().map(IntoCardOp::into_card_op))
    }

    impl<S, I, G>
    where {
        S: CardSource,
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    }
    call(source: S, grids: I) -> CardResult<S::TryOutput> {
        source.try_any(grids)
    }
}

crate::callable::callable_api! {
    #[doc(alias = "ACard")]
    /// 立即使用一张或多张卡片，报告失败并返回种植结果。
    ///
    /// 输入形式、候选格遍历顺序和返回形状与 [`try_card`] 相同。普通规则拒绝
    /// 逐项报告并返回 `None`，当前 callback 可以继续；必要读取或访问故障
    /// 结束当前 callback，不以空 Vec 掩盖，已完成的种植不会回滚。
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # #[cfg(any(feature = "pvz-emulator", feature = "pvz-1-0-0-1051", feature = "pvz-portable"))]
    /// # {
    /// # use rsvz::prelude::*;
    /// let plant: Option<PlantId> = card(PlantKind::CherryBomb, 2, 9);
    /// let plants: Vec<Option<PlantId>> =
    ///     card([PlantKind::LilyPad, PlantKind::DoomShroom], 3, 4);
    /// let first_plantable: Option<PlantId> =
    ///     card(PlantKind::CherryBomb, [(2, 9), (5, 9)]);
    /// # let _ = (plant, plants, first_plantable);
    /// # }
    /// ```
    pub card: Card;

    where {
        CurrentBackend: CardContext,
    }

    impl<S>
    where {
        S: CardSource,
    }
    call(source: S, row: i32, col: i32) -> S::Output {
        S::report(TryCard::new()(source, row, col))
    }

    impl<I, T>
    where {
        I: IntoIterator<Item = T>,
        T: IntoCardOp,
    }
    call(ops: I) -> Vec<Option<PlantId>> {
        report_cards(TryCard::new()(ops))
    }

    impl<S, I, G>
    where {
        S: CardSource,
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    }
    call(source: S, grids: I) -> S::Output {
        S::report(TryCard::new()(source, grids))
    }
}
