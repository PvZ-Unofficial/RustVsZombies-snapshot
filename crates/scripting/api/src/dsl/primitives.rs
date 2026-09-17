//! Low DSL 的基础表达式操作。
//!
//! 本模块中的用户操作返回 [`Expr`]；除明确标为即时设置的函数外，调用本身
//! 不访问游戏状态，只有表达式连接到波次时间并由 timeline 派发时才执行。

use rsvz_current::CurrentBackend;
use std::rc::Rc;

use crate::runtime::{RuntimeError, RuntimeResult};
use rsvz_backend_api::backend::{
    BoardReadinessBackend, ClockBackend, CobFireBackend, GameUiBackend, MaidCheatsBackend, PlantPoolBackend,
    PlantRemoveBackend, SceneBackend,
};
use rsvz_game::logic::IntoGrid;
use rsvz_game::logic::cards::CardPlantingBackend;
use rsvz_game::logic::cob::{
    CLASSIC_POOL_LAND_COB_LEAD, CLASSIC_POOL_WATER_COB_LEAD, CLASSIC_ROOF_COB_REFERENCE_TIME, CobBackend, CobFireDrop,
    CobManager,
};
use rsvz_game::logic::shovel::{IntoShovelOp, IntoShovelTarget, ShovelContext, ShovelOp};
use rsvz_model::model::{CobTarget, Grid, MaidCheat, PlantKind};

use super::expr::{Expr, Leaf, LowExpr, PrepareContext};
use super::input::IntoWaveSet;

/// 将后续表达式的语义时间向后移动 `frames` 厘秒，不注册游戏操作。
#[must_use]
pub fn d(frames: i32) -> LowExpr {
    Expr::delay(frames)
}

struct ActionLeaf<F>(Rc<F>);

impl<F> Leaf for ActionLeaf<F>
where
    F: Fn() + 'static,
{
    fn prepare(&self, context: &mut PrepareContext<'_>) {
        let action = Rc::clone(&self.0);
        context.push(
            context.semantic_time(),
            Box::new(move || {
                action();
                Ok(())
            }),
        );
    }
}

struct TryActionLeaf<F>(Rc<F>);

impl<F> Leaf for TryActionLeaf<F>
where
    F: Fn() -> RuntimeResult<()> + 'static,
{
    fn prepare(&self, context: &mut PrepareContext<'_>) {
        let action = Rc::clone(&self.0);
        context.push(context.semantic_time(), Box::new(move || action()));
    }
}

/// 把一个可重复调用、无返回错误的 Rust 闭包包装成 low DSL 表达式。
///
/// 闭包会在表达式的语义时间执行，而不是在调用 `act` 时执行。
#[must_use]
pub fn act(action: impl Fn() + 'static) -> LowExpr {
    Expr::from_leaf(ActionLeaf(Rc::new(action)))
}

/// 把一个返回 [`RuntimeResult`] 的可重复 Rust 闭包包装成 low DSL 表达式。
///
/// 闭包会在表达式的语义时间执行。返回的 `RuntimeError` 结束当前最小
/// operation；dispatcher 报告后仍继续同帧其他 operation。
#[must_use]
pub fn try_act(action: impl Fn() -> RuntimeResult<()> + 'static) -> LowExpr {
    Expr::from_leaf(TryActionLeaf(Rc::new(action)))
}

/// 立即为一个或多个波次登记假定波长。
///
/// 这是脚本注册期设置，不返回 [`Expr`]；无效波次或波长会记录注册错误。
pub fn assume_wavelength(waves: impl IntoWaveSet, length: i32) {
    let waves = match waves.into_wave_set() {
        Ok(waves) => waves,
        Err(error) => {
            crate::registration::record_error(error);
            return;
        }
    };
    if let Err(error) =
        crate::with_timeline(|timeline| timeline.assume_wavelengths(waves.iter().map(|wave| (wave, length))))
    {
        crate::registration::record_error(error);
    }
}

/// 转换脚本侧炮落点列。
///
/// 支持 `i32`、`f32` 和 `f64`；小数列会被保留到 core 炮落点换算阶段。
pub trait IntoDropCol {
    /// 转换为 core 炮换算使用的浮点脚本列。
    fn into_drop_col(self) -> f32;
}

impl IntoDropCol for i32 {
    fn into_drop_col(self) -> f32 {
        self as f32
    }
}

impl IntoDropCol for f32 {
    fn into_drop_col(self) -> f32 {
        self
    }
}

impl IntoDropCol for f64 {
    fn into_drop_col(self) -> f32 {
        self as f32
    }
}

/// 转换 `p` 的行参数。
///
/// 单个正整数按十进制数字展开，例如 `15` 表示 `[1, 5]`、`1256` 表示
/// `[1, 2, 5, 6]`；数字 `0` 非法，重复数字不会去重。也可直接传 `[i32; N]`。
pub trait IntoCobRows {
    /// 按脚本顺序展开为从 `1` 开始的目标行。
    fn into_cob_rows(self) -> Result<Vec<i32>, String>;
}

impl IntoCobRows for i32 {
    fn into_cob_rows(self) -> Result<Vec<i32>, String> {
        if self <= 0 {
            return Err(format!("cob rows must be positive, got {self}"));
        }
        let mut value = self;
        let mut rows = Vec::new();
        while value > 0 {
            let row = value % 10;
            if row == 0 {
                return Err("packed cob rows cannot contain 0".to_owned());
            }
            rows.push(row);
            value /= 10;
        }
        rows.reverse();
        Ok(rows)
    }
}

impl<const N: usize> IntoCobRows for [i32; N] {
    fn into_cob_rows(self) -> Result<Vec<i32>, String> {
        if let Some(row) = self.iter().copied().find(|row| *row <= 0) {
            return Err(format!("cob rows must be positive, got {row}"));
        }
        Ok(self.into())
    }
}

/// 转换 `p([(row, drop_col), ...])` 的显式目标列表。
///
/// 行从 `1` 开始，落点列允许小数，目标顺序会原样保留。
pub trait IntoCobTargetsDsl {
    /// 校验并按输入顺序转换为 core 炮落点。
    fn into_cob_targets_dsl(self) -> Result<Vec<CobTarget>, String>;
}

impl<I, Row, Col> IntoCobTargetsDsl for I
where
    I: IntoIterator<Item = (Row, Col)>,
    Row: Into<i32>,
    Col: IntoDropCol,
{
    fn into_cob_targets_dsl(self) -> Result<Vec<CobTarget>, String> {
        self.into_iter()
            .map(|(row, col)| checked_cob_target(row.into(), col.into_drop_col()))
            .collect()
    }
}

fn checked_cob_target(row: i32, drop_col: f32) -> Result<CobTarget, String> {
    CobTarget::from_one_based_row(row, drop_col).map_err(|_error| format!("invalid cob target ({row}, {drop_col})"))
}

crate::callable::callable_api! {
    #[doc(alias = "P")]
    /// 构造按炮序发炮的命中时间表达式。
    ///
    /// 调用 `p(...)` 本身不会发炮。表达式的语义时间是炮弹命中参考时间；
    /// DSL 会把实际发炮 operation 提前：普通陆路 `373cs`，泳池水路
    /// `378cs`，屋顶使用 `387cs` 参考并再按炮列修正实际发射。
    ///
    /// # 调用形式
    ///
    /// ```rust,no_run
    /// # #[cfg(any(feature = "pvz-emulator", feature = "pvz-1-0-0-1051", feature = "pvz-portable"))]
    /// # {
    /// # use rsvz::dsl::prelude::*;
    /// let one = p(2, 9.0);
    /// let packed_rows = p(1256, 9.0);
    /// let targets = p([(2, 9.0), (4, 8.75)]);
    /// let wind = rsvz::CobManager::new();
    /// let explicit = p(&wind, 2, 9.0);
    /// # let _ = (one, packed_rows, targets, explicit);
    /// # }
    /// ```
    ///
    /// 炮的选择服从相应管理器的
    /// [`CobSequentialMode`](rsvz_game::logic::cob::CobSequentialMode)。默认使用当前炮管理器，
    /// 首参 `&CobManager` 使用显式列表。目标按输入顺序处理；一个普通 manager
    /// 失败会在运行时报告，但不会阻止同批次后续目标。
    pub p: P;

    where {
        CurrentBackend: ClockBackend + CobBackend + SceneBackend + GameUiBackend + BoardReadinessBackend + 'static,
    }

    impl<Rows, Col>
    where {
        Rows: IntoCobRows,
        Col: IntoDropCol,
    }
    call(rows: Rows, col: Col) -> Expr {
        cob_rows_expr(None, false, rows, col)
    }

    impl<Targets>
    where {
        Targets: IntoCobTargetsDsl,
    }
    call(targets: Targets) -> Expr {
        cob_targets_expr(None, false, targets.into_cob_targets_dsl())
    }

    impl<Rows, Col>
    where {
        Rows: IntoCobRows,
        Col: IntoDropCol,
    }
    call(manager: &crate::cob::CobManager, rows: Rows, col: Col) -> Expr {
        cob_rows_expr(Some(manager.clone()), false, rows, col)
    }

    impl<Targets>
    where {
        Targets: IntoCobTargetsDsl,
    }
    call(manager: &crate::cob::CobManager, targets: Targets) -> Expr {
        cob_targets_expr(Some(manager.clone()), false, targets.into_cob_targets_dsl())
    }
}

crate::callable::callable_api! {
    #[doc(alias = "RecoverFire")]
    /// 构造允许等待炮恢复的命中时间表达式。
    ///
    /// 调用形式与 [`p`] 相同。到预定发炮 operation 时若选中的炮尚未恢复，
    /// 会预约到其可发射时刻，而不是阻塞线程；因此实际命中可能晚于表达式的
    /// 语义命中时间。屋顶场景同时应用恢复等待和飞行时间修正。
    pub recover_p: RecoverP;

    where {
        CurrentBackend: ClockBackend + CobBackend + SceneBackend + GameUiBackend + BoardReadinessBackend + 'static,
    }

    impl<Rows, Col>
    where {
        Rows: IntoCobRows,
        Col: IntoDropCol,
    }
    call(rows: Rows, col: Col) -> Expr {
        cob_rows_expr(None, true, rows, col)
    }

    impl<Targets>
    where {
        Targets: IntoCobTargetsDsl,
    }
    call(targets: Targets) -> Expr {
        cob_targets_expr(None, true, targets.into_cob_targets_dsl())
    }

    impl<Rows, Col>
    where {
        Rows: IntoCobRows,
        Col: IntoDropCol,
    }
    call(manager: &crate::cob::CobManager, rows: Rows, col: Col) -> Expr {
        cob_rows_expr(Some(manager.clone()), true, rows, col)
    }

    impl<Targets>
    where {
        Targets: IntoCobTargetsDsl,
    }
    call(manager: &crate::cob::CobManager, targets: Targets) -> Expr {
        cob_targets_expr(Some(manager.clone()), true, targets.into_cob_targets_dsl())
    }
}

fn cob_rows_expr(manager: Option<CobManager>, recover: bool, rows: impl IntoCobRows, col: impl IntoDropCol) -> Expr
where
    CurrentBackend: ClockBackend + CobBackend + SceneBackend + GameUiBackend + BoardReadinessBackend + 'static,
{
    let col = col.into_drop_col();
    let targets = rows
        .into_cob_rows()
        .and_then(|rows| rows.into_iter().map(|row| checked_cob_target(row, col)).collect());
    cob_targets_expr(manager, recover, targets)
}

/// 使用明确指定位置的玉米炮发射。
///
/// 参数依次为 `cob_row, cob_col, drop_row, drop_col`，均使用从 `1` 开始
/// 的脚本行列约定，落点列允许小数。炮不必存在于任何管理器列表，也不
/// 推进任何管理器的 next 游标。
///
/// ```rust,no_run
/// # #[cfg(any(feature = "pvz-emulator", feature = "pvz-1-0-0-1051", feature = "pvz-portable"))]
/// # {
/// # use rsvz::dsl::prelude::*;
/// let exact = rp(1, 1, 2, 9.0);
/// # let _ = exact;
/// # }
/// ```
///
/// 把返回的表达式注册到时间线后，会按命中时间自动换算实际发炮时刻；
/// 单独构造并丢弃表达式不会执行。
#[must_use]
pub fn rp(cob_row: i32, cob_col: i32, row: i32, col: impl IntoDropCol) -> Expr
where
    CurrentBackend: ClockBackend + CobBackend + SceneBackend + GameUiBackend + BoardReadinessBackend + 'static,
{
    let grid = (cob_row, cob_col)
        .try_into_grid()
        .map_err(|_error| format!("invalid raw cob grid ({cob_row}, {cob_col})"));
    let target = checked_cob_target(row, col.into_drop_col());
    match (grid, target) {
        (Ok(cob_grid), Ok(target)) => Expr::from_leaf(RawCobLeaf {
            drop: CobFireDrop { cob_grid, target },
        }),
        (Err(grid), Err(target)) => Expr::error(grid) + Expr::error(target),
        (Err(error), Ok(_)) | (Ok(_), Err(error)) => Expr::error(error),
    }
}

fn cob_targets_expr(manager: Option<CobManager>, recover: bool, targets: Result<Vec<CobTarget>, String>) -> Expr
where
    CurrentBackend: ClockBackend + CobBackend + SceneBackend + GameUiBackend + BoardReadinessBackend + 'static,
{
    match targets {
        Ok(targets) if targets.is_empty() => Expr::error("cob target list cannot be empty"),
        Ok(targets) => Expr::from_leaf(CobLeaf {
            manager,
            recover,
            targets: Rc::from(targets.into_boxed_slice()),
        }),
        Err(error) => Expr::error(error),
    }
}

struct CobLeaf {
    manager: Option<CobManager>,
    recover: bool,
    targets: Rc<[CobTarget]>,
}

struct RawCobLeaf {
    drop: CobFireDrop,
}

impl Leaf for RawCobLeaf
where
    CurrentBackend: ClockBackend + CobBackend + SceneBackend + GameUiBackend + BoardReadinessBackend + 'static,
{
    fn prepare(&self, context: &mut PrepareContext<'_>) {
        if !cob_times_fit(context, true) {
            return;
        }
        let manager = crate::cob::__current_core();
        let semantic_time = context.semantic_time();
        rsvz_game::cob::impact::prepare_raw(manager, self.drop, |lead, callback| {
            context.push(semantic_time - i128::from(lead), callback);
        });
    }
}

impl Leaf for CobLeaf
where
    CurrentBackend: ClockBackend + CobBackend + SceneBackend + GameUiBackend + BoardReadinessBackend + 'static,
{
    fn prepare(&self, context: &mut PrepareContext<'_>) {
        if !cob_times_fit(context, true) {
            return;
        }
        let manager = self.manager.clone().unwrap_or_else(crate::cob::__current_core);
        let semantic_time = context.semantic_time();
        rsvz_game::cob::impact::prepare(manager, &self.targets, self.recover, |lead, callback| {
            context.push(semantic_time - i128::from(lead), callback);
        });
    }
}

fn cob_times_fit(context: &mut PrepareContext<'_>, include_pool_water: bool) -> bool {
    let mut valid = true;
    for lead in [
        CLASSIC_POOL_LAND_COB_LEAD,
        CLASSIC_ROOF_COB_REFERENCE_TIME,
        if include_pool_water {
            CLASSIC_POOL_WATER_COB_LEAD
        } else {
            CLASSIC_POOL_LAND_COB_LEAD
        },
    ] {
        let time = context.semantic_time().checked_sub(i128::from(lead));
        if time.and_then(|time| i32::try_from(time).ok()).is_none() {
            context.error(format!(
                "wave {} cob command time is outside the i32 range",
                context.wave()
            ));
            valid = false;
            break;
        }
    }
    valid
}

crate::callable::callable_api! {
    #[doc(alias = "PP")]
    /// 构造常规双边炮的命中时间表达式。
    ///
    /// 五行场景目标为第 `2、4` 行，六行泳池/雾夜场景为第 `2、5` 行；
    /// `pp()` 默认落点列为 `9.0`，`pp(col)` 指定共同落点列。两发都使用
    /// 当前默认炮管理器，并按 [`p`] 相同的命中时间规则换算发炮时刻。
    ///
    /// ```rust,no_run
    /// # #[cfg(any(feature = "pvz-emulator", feature = "pvz-1-0-0-1051", feature = "pvz-portable"))]
    /// # {
    /// # use rsvz::dsl::prelude::*;
    /// let default_pair = pp();
    /// let shifted_pair = pp(8.75);
    /// # let _ = (default_pair, shifted_pair);
    /// # }
    /// ```
    pub pp: Pp;

    where {
        CurrentBackend: ClockBackend + CobBackend + SceneBackend + GameUiBackend + BoardReadinessBackend + 'static,
    }

    impl<>
    where {}
    call() -> Expr {
        pp_expr(9.0)
    }

    impl<C>
    where {
        C: IntoDropCol,
    }
    call(col: C) -> Expr {
        pp_expr(col.into_drop_col())
    }
}

fn pp_expr(col: f32) -> Expr
where
    CurrentBackend: ClockBackend + CobBackend + SceneBackend + GameUiBackend + BoardReadinessBackend + 'static,
{
    if let Err(error) = checked_cob_target(2, col) {
        Expr::error(error)
    } else {
        Expr::from_leaf(DefaultPpLeaf { col })
    }
}

struct DefaultPpLeaf {
    col: f32,
}

impl Leaf for DefaultPpLeaf
where
    CurrentBackend: ClockBackend + CobBackend + SceneBackend + GameUiBackend + BoardReadinessBackend + 'static,
{
    fn prepare(&self, context: &mut PrepareContext<'_>) {
        if !cob_times_fit(context, false) {
            return;
        }
        let manager = crate::cob::__current_core();
        let semantic_time = context.semantic_time();
        rsvz_game::cob::impact::prepare_default_pair(manager, self.col, |lead, callback| {
            context.push(semantic_time - i128::from(lead), callback);
        });
    }
}

/// Constructs a cob-list scan at the expression's scheduled time.
#[must_use]
pub fn auto_cobs() -> Expr
where
    CurrentBackend: CobFireBackend + PlantPoolBackend + 'static,
{
    Expr::from_leaf(AutoCobsLeaf)
}

struct AutoCobsLeaf;

impl Leaf for AutoCobsLeaf
where
    CurrentBackend: CobFireBackend + PlantPoolBackend + 'static,
{
    fn prepare(&self, context: &mut PrepareContext<'_>) {
        let manager = crate::cob::__current_core();
        context.on_commit(manager.prepare_auto_set_list_capacity());
        context.push(
            context.semantic_time(),
            Box::new(move || {
                let manager = crate::cob::__current_core();
                manager.auto_set_list().map_err(runtime_error)
            }),
        );
    }
}

/// Constructs a runtime cob scan restricted to one script-facing tail column.
///
/// The scan runs at the expression's scheduled time, so it may safely follow
/// registered lineup application.
#[must_use]
pub fn auto_cobs_col(manager: &crate::cob::CobManager, col: i32) -> Expr
where
    CurrentBackend: CobFireBackend + PlantPoolBackend + 'static,
{
    match Grid::from_one_based(1, col) {
        Ok(grid) => Expr::from_leaf(AutoCobsColLeaf {
            manager: manager.clone(),
            col: grid.col,
        }),
        Err(_error) => Expr::error(format!("invalid cob tail column {col}")),
    }
}

struct AutoCobsColLeaf {
    manager: CobManager,
    col: i32,
}

impl Leaf for AutoCobsColLeaf
where
    CurrentBackend: CobFireBackend + PlantPoolBackend + 'static,
{
    fn prepare(&self, context: &mut PrepareContext<'_>) {
        let manager = self.manager.clone();
        context.on_commit(manager.clone().prepare_auto_set_list_capacity());
        let col = self.col;
        context.push(
            context.semantic_time(),
            Box::new(move || manager.auto_set_list_in_column(col).map_err(runtime_error)),
        );
    }
}

/// Constructs an ice-filler setup at the expression's scheduled time.
#[must_use]
pub fn set_ice<I, G>(grids: I) -> Expr
where
    CurrentBackend: CardPlantingBackend + SceneBackend + 'static,
    I: IntoIterator<Item = G>,
    G: IntoGrid,
{
    let grids = grids
        .into_iter()
        .map(|grid| {
            grid.try_into_grid()
                .map_err(|_error| "invalid ice filler grid".to_owned())
        })
        .collect::<Result<Vec<_>, _>>();
    match grids {
        Ok(grids) if grids.is_empty() => Expr::error("ice filler grid list cannot be empty"),
        Ok(grids) => Expr::from_leaf(SetIceLeaf {
            grids: Rc::from(grids.into_boxed_slice()),
        }),
        Err(error) => Expr::error(error),
    }
}

struct SetIceLeaf {
    grids: Rc<[Grid]>,
}

impl Leaf for SetIceLeaf
where
    CurrentBackend: CardPlantingBackend + SceneBackend + 'static,
{
    fn prepare(&self, context: &mut PrepareContext<'_>) {
        let mut grids = Some(self.grids.iter().copied().collect::<Vec<_>>());
        context.on_commit(|| {
            rsvz_schedule::tick::with_scheduler(rsvz_game::ice_filler::register_tick_with_scheduler);
        });
        context.push(
            context.semantic_time(),
            Box::new(move || {
                let grids = grids.take().expect("timeline callbacks execute at most once");
                rsvz_game::ice_filler::start_prepared(grids)
            }),
        );
    }
}

/// Constructs a dancing cheat change at the expression's scheduled time.
#[must_use]
pub fn dancing() -> Expr
where
    CurrentBackend: MaidCheatsBackend + 'static,
{
    Expr::from_leaf(DancingLeaf)
}

struct DancingLeaf;

impl Leaf for DancingLeaf
where
    CurrentBackend: MaidCheatsBackend + 'static,
{
    fn prepare(&self, context: &mut PrepareContext<'_>) {
        context.push(
            context.semantic_time(),
            Box::new(move || rsvz_game::modifier::set_maid_cheat(MaidCheat::Dancing)),
        );
    }
}

crate::callable::callable_api! {
    #[doc(alias = "Shovel")]
    #[doc(alias = "AShovel")]
    /// 构造在表达式语义时间执行的语义铲除操作。
    ///
    /// 调用本身不会铲除植物。脚本行列从 `1` 开始；省略目标使用主体层优先
    /// 的默认规则，`true` 只铲南瓜，[`PlantKind`] /
    /// [`CardSelection`](rsvz_model::model::CardSelection) 可精确区分目标和
    /// 模仿者。操作通过 backend 语义删除，不移动或点击鼠标。
    ///
    /// ```rust,no_run
    /// # #[cfg(any(feature = "pvz-emulator", feature = "pvz-1-0-0-1051", feature = "pvz-portable"))]
    /// # {
    /// # use rsvz::dsl::prelude::*;
    /// let one = shovel(4, 6);
    /// let remove_pumpkin = shovel(4, 6, true);
    /// let exact = shovel(4, 6, PlantKind::CoffeeBean);
    /// let batch = shovel([(3, 6), (4, 6)]);
    /// # let _ = (one, remove_pumpkin, exact, batch);
    /// # }
    /// ```
    ///
    /// 构造时发现任一无效坐标会产生注册错误，不注册该批次的部分操作。
    /// 运行时每一项是独立 operation，因此某项 backend 错误被报告后，同帧
    /// 后续铲除仍会继续。目标不存在是成功的空操作。
    pub shovel: DslShovel;

    where {
        CurrentBackend: ShovelContext + 'static,
    }

    impl<>
    where {}
    call(row: i32, col: i32) -> Expr {
        shovel_expr([(row, col)])
    }

    impl<T>
    where {
        T: IntoShovelTarget,
    }
    call(row: i32, col: i32, target: T) -> Expr {
        shovel_expr([(row, col, target)])
    }

    impl<I, O>
    where {
        I: IntoIterator<Item = O>,
        O: IntoShovelOp,
    }
    call(ops: I) -> Expr {
        shovel_expr(ops)
    }
}

fn shovel_expr<I, O>(ops: I) -> Expr
where
    CurrentBackend: ShovelContext + 'static,
    I: IntoIterator<Item = O>,
    O: IntoShovelOp,
{
    let ops = ops
        .into_iter()
        .map(|op| {
            op.try_into_shovel_op()
                .map_err(|_error| "invalid shovel grid".to_owned())
        })
        .collect::<Result<Vec<_>, _>>();
    match ops {
        Ok(ops) if ops.is_empty() => Expr::empty(),
        Ok(ops) => Expr::from_leaf(ShovelLeaf {
            ops: Rc::from(ops.into_boxed_slice()),
        }),
        Err(error) => Expr::error(error),
    }
}

struct ShovelLeaf {
    ops: Rc<[ShovelOp]>,
}

impl Leaf for ShovelLeaf
where
    CurrentBackend: ShovelContext + 'static,
{
    fn prepare(&self, context: &mut PrepareContext<'_>) {
        for op in self.ops.iter().copied() {
            context.push(
                context.semantic_time(),
                Box::new(move || rsvz_game::logic::shovel::shovel_target_at(op.grid, op.target).map_err(runtime_error)),
            );
        }
    }
}

/// Converts ordinary plant semantics for typed `rm`.
pub trait IntoPlantArg {
    fn into_plant_arg(self) -> Result<PlantKind, String>;
}

impl IntoPlantArg for PlantKind {
    fn into_plant_arg(self) -> Result<PlantKind, String> {
        Ok(self)
    }
}

impl IntoPlantArg for super::card::CardAlias {
    fn into_plant_arg(self) -> Result<PlantKind, String> {
        self.plant_kind()
    }
}

/// Constructs removal of one plant kind at a script-facing grid.
#[must_use]
pub fn rm<K>(kind: K, row: i32, col: i32) -> Expr
where
    CurrentBackend: PlantRemoveBackend + 'static,
    K: IntoPlantArg,
{
    let kind = kind.into_plant_arg();
    let grid = Grid::from_one_based(row, col).map_err(|_error| format!("invalid remove grid ({row}, {col})"));
    match (kind, grid) {
        (Ok(kind), Ok(grid)) => Expr::from_leaf(RemoveKindLeaf { kind, grid }),
        (Err(kind), Err(grid)) => Expr::error(kind) + Expr::error(grid),
        (Err(error), Ok(_)) | (Ok(_), Err(error)) => Expr::error(error),
    }
}

struct RemoveKindLeaf {
    kind: PlantKind,
    grid: Grid,
}

impl Leaf for RemoveKindLeaf
where
    CurrentBackend: PlantRemoveBackend + 'static,
{
    fn prepare(&self, context: &mut PrepareContext<'_>) {
        let kind = self.kind;
        let grid = self.grid;
        context.push(
            context.semantic_time(),
            Box::new(move || {
                rsvz_game::modifier::remove_plant_kind_at(kind, grid)
                    .map(|_id| ())
                    .map_err(runtime_error)
            }),
        );
    }
}

fn runtime_error(error: impl std::fmt::Display) -> RuntimeError {
    RuntimeError::new(error.to_string())
}
