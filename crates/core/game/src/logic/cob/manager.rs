use rsvz_backend_api::PlantReadBackend;
use rsvz_current::{CurrentBackend, CurrentBackendError};
use std::cell::RefCell;
use std::fmt::{Display, Write as _};
use std::rc::Rc;

use super::try_plant_cob;
use crate::backend::{ClockBackend, CobFireBackend, SceneBackend};
use crate::logic::cards::CardPlantingBackend;
use crate::logic::grid::IntoGrid;
use crate::logic::shovel::shovel_at;
use crate::model::{CobTarget, PixelPos, PlantKind};
use crate::model::{Grid, PlantId};
use crate::runtime::{RuntimeError, RuntimeResult};
use rsvz_schedule::{TickControl, TickOptions, TickScheduler};

use super::CobManagerError;
use super::helpers::{CobFireAttempt, fire_cob_attempt};
use super::{
    CobBackend, CobFireDrop, CobManagerCallError, IntoCobFireDrops, IntoCobTarget, IntoCobTargets,
    classic_roof_fire_delay, cob_recover_time, cob_target_to_pixel, find_cob_at, find_plant_at_kind, fire_cob,
};

/// 从有序 manager 列表选择炮的策略。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CobSequentialMode {
    /// 只使用当前 `next` 项，不跳过不可用炮。
    Space,
    /// 从 `next` 向后扫描可用炮，使用后推进游标。
    #[default]
    Time,
    /// 每次从列表开头扫描，以列表顺序作为优先级。
    Priority,
}

/// 自动发现炮时使用的排序方式。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CobListOrder {
    /// 按 core `(row, col)` 排序：先行后列。
    #[default]
    Horizontal,
    /// 按 core `(col, row)` 排序：先列后行。
    Vertical,
}

impl CobListOrder {
    /// 返回该排序方式对应的 core 排序键。
    #[must_use]
    pub const fn sort_key(self, grid: Grid) -> (i32, i32) {
        match self {
            Self::Horizontal => (grid.row, grid.col),
            Self::Vertical => (grid.col, grid.row),
        }
    }
}

/// manager 中一门炮的当前恢复观测。
///
/// `grid` 是 core `0-based`。`recover_time == 0` 表示立即可用，正数是剩余
/// 厘秒，负数是不可用/reserved 状态；`id == None` 表示配置炮位已没有炮。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CobRecoverInfo {
    /// 配置的 core 炮位。
    pub grid: Grid,
    /// 配置炮位上当前存活炮的 ID（若存在）。
    pub id: Option<PlantId>,
    /// [`CobRecoverInfo`] 所述的恢复/不可用数值。
    pub recover_time: i32,
}

/// 配置炮位没有存活炮时的恢复值哨兵。
pub const NO_EXIST_RECOVER_TIME: i32 = i32::MIN;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CobEntry {
    grid: Grid,
    cached_id: Option<PlantId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LatestCobFire {
    index: Option<usize>,
    time: i32,
    writable: bool,
}

impl LatestCobFire {
    const fn new() -> Self {
        Self {
            index: None,
            time: 0,
            writable: true,
        }
    }
}

impl Default for LatestCobFire {
    fn default() -> Self {
        Self::new()
    }
}

/// Latest-fire repair task. The grid is read from `list_index` when the task executes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CobFixLatestTask {
    list_index: usize,
    due_delay: i32,
    start_clock: i32,
    plant_grid: Option<Grid>,
}

struct LatestUnlock {
    manager: CobManager,
    armed: bool,
}

impl Drop for LatestUnlock {
    fn drop(&mut self) {
        if self.armed {
            self.manager.0.borrow_mut().latest.writable = true;
        }
    }
}

fn push_error_line(errors: &mut Option<String>, error: impl Display) {
    let message = errors.get_or_insert_default();
    if !message.is_empty() {
        message.push('\n');
    }
    if write!(message, "{error}").is_err() {
        unreachable!("writing to a String cannot fail");
    }
}

fn push_indexed_fire_error(errors: &mut Option<String>, operation: &str, target_index: usize, error: impl Display) {
    push_error_line(
        errors,
        format_args!("{operation}第 {} 个目标失败：{error}", target_index + 1),
    );
}

fn push_raw_fire_error(errors: &mut Option<String>, drop_index: usize, error: impl Display) {
    push_error_line(errors, format_args!("明确发炮第 {} 项失败：{error}", drop_index + 1));
}

fn optional_fire_outcome(
    operation: &str, target_index: usize, result: Result<i32, CobManagerError>,
) -> RuntimeResult<Option<i32>> {
    match result {
        Ok(index) => Ok(Some(index)),
        Err(
            CobManagerError::EmptyList
            | CobManagerError::InvalidTarget
            | CobManagerError::NoReadyCob
            | CobManagerError::NoRecoverableCob,
        ) => Ok(None),
        Err(error) => Err(RuntimeError::new(format!(
            "{operation}第 {} 个目标失败：{error}",
            target_index + 1
        ))),
    }
}

/// 与 backend 无关、共享状态的炮列表与炮序 handle。
///
/// 克隆值通过 `Rc` 共享列表、mode/next 和最近发炮。表达式/callback
/// 需要保留同一 manager 时可直接克隆；需要独立列表时调用
/// [`CobManager::new`] 或 [`CobManager::with_mode`]。同一脚本的多个列表共享
/// 延迟发炮 reservation，避免重复预约同一门炮。
#[derive(Clone, Debug)]
pub struct CobManager(Rc<RefCell<CobManagerState>>);

/// 同一 current runtime 内共享的延迟发炮 reservation。
#[derive(Clone, Debug, Default)]
struct CobRuntimeState(Rc<RefCell<Vec<PlantId>>>);

impl CobRuntimeState {
    fn contains(&self, cob: PlantId) -> bool {
        self.0.borrow().contains(&cob)
    }

    fn reserve(&self, cob: PlantId) -> Result<CobReservation, CobManagerError> {
        let mut state = self.0.borrow_mut();
        if state.contains(&cob) {
            return Err(CobManagerError::NoReadyCob);
        }
        state.push(cob);
        Ok(CobReservation {
            runtime: self.clone(),
            cob,
        })
    }

    fn release(&self, cob: PlantId) {
        let mut state = self.0.borrow_mut();
        if let Some(index) = state.iter().position(|reserved| *reserved == cob) {
            state.swap_remove(index);
        }
    }

    fn reset(&self) {
        self.0.borrow_mut().clear();
    }
}

struct CobReservation {
    runtime: CobRuntimeState,
    cob: PlantId,
}

impl Drop for CobReservation {
    fn drop(&mut self) {
        self.runtime.release(self.cob);
    }
}

fn schedule_cob_fire(
    runtime: &CobRuntimeState, delay: i32, cob: PlantId, fire_target: PixelPos,
) -> Result<(), CobManagerCallError>
where
    rsvz_current::CurrentBackend:
        CobFireBackend + rsvz_backend_api::BoardReadinessBackend + rsvz_backend_api::GameUiBackend,
{
    let reservation = runtime.reserve(cob).map_err(CobManagerCallError::Manager)?;
    let registration = rsvz_schedule::timeline::with_timeline(|timeline| {
        timeline.after(delay, move || {
            let _keep_alive = &reservation;
            match fire_cob_attempt(reservation.cob, fire_target).unwrap_or_else(|error| {
                crate::diagnostics::abort_operation(RuntimeError::new(format!("延迟发炮底层操作失败：{error}")))
            }) {
                CobFireAttempt::Fired => Ok(()),
                CobFireAttempt::NotReady => Err(RuntimeError::new("延迟发炮到点时原炮尚未恢复")),
                CobFireAttempt::Unavailable => Err(RuntimeError::new("延迟发炮到点时原炮已失效")),
            }
        })
    })
    .map_err(|error| CobManagerCallError::Backend(RuntimeError::new(format!("注册延迟发炮失败：{error}"))))?;

    crate::timeline::consume_registration(registration);
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SelectedFireCob {
    index: usize,
    cob: PlantId,
    delay: i32,
    fire_target: PixelPos,
}

/// Backend-neutral cob sequence state.
#[derive(Debug)]
struct CobManagerState {
    entries: Vec<CobEntry>,
    auto_entries: Vec<CobEntry>,
    next: usize,
    mode: CobSequentialMode,
    latest: LatestCobFire,
    runtime: CobRuntimeState,
}

impl CobManager {
    /// Creates a separate list with the default Time selection mode.
    /// All current-script managers share delayed-fire reservations.
    #[must_use]
    pub fn new() -> Self {
        Self::with_mode(CobSequentialMode::Time)
    }

    #[must_use]
    pub fn with_mode(mode: CobSequentialMode) -> Self {
        super::current_cob_manager().with_shared_runtime(mode)
    }

    pub(crate) fn new_root() -> Self {
        Self::with_runtime(CobSequentialMode::Time, CobRuntimeState::default())
    }

    fn with_runtime(mode: CobSequentialMode, runtime: CobRuntimeState) -> Self {
        Self(Rc::new(RefCell::new(CobManagerState::new(mode, runtime))))
    }

    /// Creates another list manager that shares this run's delayed-fire state.
    #[must_use]
    pub fn with_shared_runtime(&self, mode: CobSequentialMode) -> Self {
        Self::with_runtime(mode, self.0.borrow().runtime.clone())
    }

    /// 修改选择模式，不重排列表。
    pub fn set_sequential_mode(&self, mode: CobSequentialMode) {
        self.0.borrow_mut().set_sequential_mode(mode);
    }

    /// 返回原始 core `0-based` next 下标；空列表时该值可能无效。
    #[must_use]
    pub fn next_index_raw(&self) -> i32 {
        self.0.borrow().next_index_raw()
    }

    /// 按列表顺序访问配置的 core `0-based` 炮位，不做存在性校验。
    pub fn for_each_grid(&self, mut visit: impl FnMut(Grid)) {
        self.0.borrow().for_each_grid(&mut visit);
    }

    /// 替换列表，不检查各格是否真的有炮。
    ///
    /// 脚本元组从 `1-based` 转换，类型化 [`Grid`] 是 core `0-based`；无效元组
    /// 会经 [`IntoGrid::into_grid`] panic。next 重置为零。对不可信/用户输入
    /// 应优先使用 [`CobManager::set_checked_list_validated`]。
    pub fn set_list_unchecked<I, G>(&self, grids: I)
    where
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    {
        self.0.borrow_mut().set_list_unchecked(grids);
    }

    /// 从列表删除指定 core 炮位，并把 next 重置到开头。
    ///
    /// 不在列表中的项被忽略；本操作不会删除场上植物。
    pub fn erase_from_list(&self, grids: &[Grid]) {
        self.0.borrow_mut().erase_from_list(grids);
    }

    /// 在 [`CobSequentialMode::Priority`] 中把炮位移动/加入列表开头。
    ///
    /// 其他模式返回 [`CobManagerError::InvalidModeForOperation`]。
    pub fn move_to_list_top(&self, grids: &[Grid]) -> Result<(), CobManagerError> {
        self.0.borrow_mut().move_to_list_top(grids)
    }

    /// 在 [`CobSequentialMode::Priority`] 中把炮位移动/加入列表末尾。
    ///
    /// 其他模式返回 [`CobManagerError::InvalidModeForOperation`]。
    pub fn move_to_list_bottom(&self, grids: &[Grid]) -> Result<(), CobManagerError> {
        self.0.borrow_mut().move_to_list_bottom(grids)
    }

    /// 在 Space/Time 模式用 `1-based` 列表序号设置 next。
    ///
    /// 这与发炮结果的 `0-based` 列表下标有意不同。Priority 模式或序号越界
    /// 返回错误。
    pub fn set_next_slot(&self, slot: i32) -> Result<(), CobManagerError> {
        self.0.borrow_mut().set_next_slot(slot)
    }

    /// 在 Space/Time 模式按已有 core 炮位设置 next。
    pub fn set_next_grid(&self, grid: Grid) -> Result<(), CobManagerError> {
        self.0.borrow_mut().set_next_grid(grid)
    }

    /// 在 Space/Time 模式让 next 循环前移有符号项数。
    pub fn skip(&self, n: i32) -> Result<(), CobManagerError> {
        self.0.borrow_mut().skip(n)
    }

    /// 保留模式和配置列表，重置一次脚本运行的瞬态状态。
    ///
    /// 清除该 manager 的 next/latest，并清除其所属运行状态中的
    /// 延迟发炮 reservation；供 runtime 生命周期重置使用。
    pub fn reset_before_script(&self) {
        self.0.borrow_mut().reset_before_script();
    }

    /// 原子地校验并替换配置列表。
    ///
    /// backend 或 manager 校验失败时保留旧列表。
    pub fn set_checked_list_validated<I, G>(&self, grids: I) -> Result<(), CobManagerCallError>
    where
        CurrentBackend: PlantReadBackend,
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    {
        self.0.borrow_mut().set_checked_list_validated(grids)
    }

    /// 发现全部存活炮并按 [`CobListOrder::Horizontal`] 排序。
    pub fn auto_set_list(&self) -> Result<(), CurrentBackendError>
    where
        CurrentBackend: PlantReadBackend,
    {
        Ok(self.0.borrow_mut().auto_set_list())
    }

    /// 发现全部存活炮、排序并替换列表，然后重置 next。
    pub fn auto_set_list_by(&self, order: CobListOrder) -> Result<(), CurrentBackendError>
    where
        CurrentBackend: PlantReadBackend,
    {
        Ok(self.0.borrow_mut().auto_set_list_by(order))
    }

    /// Discovers live cobs whose tail occupies one core `0-based` column.
    pub fn auto_set_list_in_column(&self, col: i32) -> Result<(), CurrentBackendError>
    where
        CurrentBackend: PlantReadBackend,
    {
        Ok(self.0.borrow_mut().auto_set_list_in_column(col))
    }

    /// Reads capacity during preparation, reserving both buffers only on commit.
    #[doc(hidden)]
    pub fn prepare_auto_set_list_capacity(self) -> impl FnOnce() + 'static
    where
        CurrentBackend: rsvz_backend_api::PlantPoolBackend,
    {
        use rsvz_backend_api::PlantPoolBackend;
        let capacity = crate::access::with_backend(|backend| {
            crate::live_value::read_or_abort(backend.plant_pool_capacity(), "plant_pool_capacity")
        }) as usize;
        move || self.0.borrow_mut().reserve_auto_set_list_capacity(capacity)
    }

    /// 检查每个配置炮位当前是否都有存活炮。
    ///
    /// 错误指出 backend 读取故障或第一个缺失/无效 manager 项。
    pub fn validate_current_list(&self) -> Result<(), CobManagerCallError>
    where
        CurrentBackend: PlantReadBackend,
    {
        self.0.borrow_mut().validate_current_list()
    }

    /// 注册一个在 `grid` 补种玉米加农炮的 tick 任务。
    ///
    /// 任务会等待所需玉米投手和玉米加农炮卡可用，而不是立即执行鼠标操作。
    /// `grid` 遵循 [`IntoGrid`]：元组是 `1-based` 脚本坐标，显式 [`Grid`]
    /// 是 `0-based` core 值。
    pub fn plant<G>(&self, scheduler: &mut TickScheduler, grid: G)
    where
        rsvz_current::CurrentBackend: CardPlantingBackend + 'static,
        G: IntoGrid,
    {
        register_plant_tick(grid.into_grid(), scheduler);
    }

    /// 重建该 manager 中最晚恢复可用的炮。
    ///
    /// 选择/backend 故障在任务注册前返回；任务执行中的故障使用
    /// scheduler 的普通 runtime 错误路径。
    pub fn fix_latest(&self, scheduler: &mut TickScheduler) -> Result<(), CobManagerCallError>
    where
        rsvz_current::CurrentBackend: ClockBackend + CardPlantingBackend + 'static,
    {
        let task = self.0.borrow_mut().fix_latest_task()?;
        self.register_fix_latest_tick(task, scheduler);
        Ok(())
    }

    /// 返回 manager 每个列表项的恢复信息，并按恢复时间升序排序。
    ///
    /// 已没有存活炮的项以 [`NO_EXIST_RECOVER_TIME`] 保留；reserved 炮报告
    /// `-1`，并被自动选择跳过。
    pub fn recover_list(&self) -> Result<Vec<CobRecoverInfo>, CurrentBackendError>
    where
        CurrentBackend: PlantReadBackend,
    {
        Ok(self.0.borrow_mut().recover_list())
    }

    /// 返回针对屋顶落点列修正后的恢复列表。
    ///
    /// 每个非负恢复时间减去与炮列相关的屋顶命令提前量，再夹紧到零。因此零
    /// 表示现在下达命令仍能与参考炮在同一生效时刻命中。`drop_col` 使用脚本
    /// 落点列语义。
    pub fn roof_recover_list(&self, drop_col: f32) -> Result<Vec<CobRecoverInfo>, CobManagerCallError>
    where
        CurrentBackend: PlantReadBackend,
    {
        let mut recover = self.0.borrow_mut().recover_list();
        for info in &mut recover {
            if info.recover_time >= 0 {
                let lead = classic_roof_fire_delay(info.grid.col + 1, drop_col)?;
                info.recover_time = info.recover_time.saturating_sub(lead).max(0);
            }
        }
        recover.sort_by_key(|info| info.recover_time);
        Ok(recover)
    }

    /// 按列表顺序返回当前全部可用且未 reserved 的炮。
    pub fn usable_list(&self) -> Result<Vec<PlantId>, CurrentBackendError>
    where
        CurrentBackend: PlantReadBackend,
    {
        Ok(self.0.borrow_mut().usable_list())
    }

    /// 按修正后恢复顺序返回面向屋顶 `drop_col` 可用且未 reserved 的全部炮。
    pub fn roof_usable_list(&self, drop_col: f32) -> Result<Vec<PlantId>, CobManagerCallError>
    where
        CurrentBackend: PlantReadBackend,
    {
        let mut ready = Vec::new();
        self.0.borrow_mut().visit_available_recover(Some(drop_col), |key, id| {
            if key.0 == 0 {
                ready.push((key, id));
            }
        })?;
        ready.sort_unstable_by_key(|(key, _)| *key);
        Ok(ready.into_iter().map(|(_, id)| id).collect())
    }

    /// 返回最早恢复且未 reserved 的 manager 炮。
    ///
    /// 没有存活且可选炮时返回 `None`。
    pub fn recover_cob(&self) -> Result<Option<PlantId>, CurrentBackendError>
    where
        CurrentBackend: PlantReadBackend,
    {
        Ok(self.0.borrow_mut().recover_cob())
    }

    /// 返回面向指定屋顶脚本落点列最早可下令且未 reserved 的炮。
    pub fn roof_recover_cob(&self, drop_col: f32) -> Result<Option<PlantId>, CobManagerCallError>
    where
        CurrentBackend: PlantReadBackend,
    {
        Ok(self.0.borrow_mut().first_recover(Some(drop_col), false)?)
    }

    /// 返回列表顺序中第一门当前可用且未 reserved 的炮。
    pub fn usable_cob(&self) -> Result<Option<PlantId>, CurrentBackendError>
    where
        CurrentBackend: PlantReadBackend,
    {
        Ok(self.0.borrow_mut().usable_cob())
    }

    /// 返回面向指定屋顶脚本落点列当前可用的第一门炮。
    pub fn roof_usable_cob(&self, drop_col: f32) -> Result<Option<PlantId>, CobManagerCallError>
    where
        CurrentBackend: PlantReadBackend,
    {
        Ok(self.0.borrow_mut().first_recover(Some(drop_col), true)?)
    }

    fn register_fix_latest_tick(&self, mut task: CobFixLatestTask, scheduler: &mut TickScheduler)
    where
        rsvz_current::CurrentBackend: ClockBackend + CardPlantingBackend + 'static,
    {
        let lifetime_unlock = LatestUnlock {
            manager: self.clone(),
            armed: true,
        };
        scheduler.spawn(
            TickOptions::playing_frame().idle_neutral().name("cob_fix"),
            move |_meta| {
                let _keep_alive = &lifetime_unlock;
                // Dropped after the state borrow, including local callback aborts.
                let mut on_failure = LatestUnlock {
                    manager: lifetime_unlock.manager.clone(),
                    armed: true,
                };
                let result = task.tick(&mut on_failure.manager.0.borrow_mut());
                if let Ok(control) = result {
                    on_failure.armed = control == TickControl::Stop;
                    Ok(control)
                } else {
                    result.map_err(|error| RuntimeError::from(error.to_string()))
                }
            },
        );
    }
}

impl CobManager
where
    CurrentBackend: ClockBackend
        + CobBackend
        + SceneBackend
        + 'static
        + rsvz_backend_api::BoardReadinessBackend
        + rsvz_backend_api::GameUiBackend,
{
    /// 发射一个目标并返回所用炮的 `0-based` 列表下标。
    ///
    /// 目标无效或没有可用炮表示为 `None`；backend 故障仍是错误，成功发炮
    /// 永不回滚。
    pub(super) fn try_fire<C>(&self, row: i32, col: C) -> RuntimeResult<Option<i32>>
    where
        (i32, C): IntoCobTarget,
    {
        self.0.borrow_mut().try_fire(row, col)
    }

    /// 按输入顺序发炮，并为每个目标返回一个列表下标。
    ///
    /// 无效目标/无可用炮变为 `None`，不停止后续目标；backend 错误停止后续
    /// backend 访问，之前成功发炮保留。
    pub(super) fn try_fire_many<T>(&self, targets: T) -> RuntimeResult<Vec<Option<i32>>>
    where
        T: IntoCobTargets,
    {
        self.0.borrow_mut().try_fire_many(targets)
    }

    /// 预约一门仍可恢复的炮，并返回其 `0-based` 列表下标。
    pub(super) fn try_recover_fire<C>(&self, row: i32, col: C) -> RuntimeResult<Option<i32>>
    where
        (i32, C): IntoCobTarget,
    {
        self.0.borrow_mut().try_recover_fire(row, col)
    }

    /// 按输入顺序预约可恢复炮，并为每个目标返回一个列表下标。无效/不可满足
    /// 目标变为 `None`；backend 故障停止批次但不回滚此前工作。
    pub(super) fn try_recover_fire_many<T>(&self, targets: T) -> RuntimeResult<Vec<Option<i32>>>
    where
        T: IntoCobTargets,
    {
        self.0.borrow_mut().try_recover_fire_many(targets)
    }

    /// 按输入顺序发射明确指定的炮，并汇总普通转换/manager 错误，不返回逐项
    /// 结果。
    ///
    /// backend 错误停止批次；成功发炮不回滚。
    pub(super) fn try_raw_fire<D>(&self, drops: D) -> RuntimeResult<()>
    where
        D: IntoCobFireDrops,
    {
        self.0.borrow_mut().try_raw_fire(drops)
    }

    /// 发射已在注册期校验的目标，不分配结果列表。
    ///
    /// 目标按输入顺序处理；普通 manager 错误被汇总且后续目标继续，backend
    /// 错误停止后续访问。错误前完成的发炮保留。
    pub(super) fn fire_prepared(&self, targets: &[CobTarget]) -> RuntimeResult<()> {
        self.0.borrow_mut().fire_prepared(targets, false, "发炮")
    }

    /// 发射已在注册期校验的目标，必要时等待炮恢复。
    pub(super) fn recover_fire_prepared(&self, targets: &[CobTarget]) -> RuntimeResult<()> {
        self.0.borrow_mut().fire_prepared(targets, true, "恢复发炮")
    }
}

impl CobManagerState {
    fn new(mode: CobSequentialMode, runtime: CobRuntimeState) -> Self {
        Self {
            entries: Vec::new(),
            auto_entries: Vec::new(),
            next: 0,
            mode,
            latest: LatestCobFire::new(),
            runtime,
        }
    }

    fn set_sequential_mode(&mut self, mode: CobSequentialMode) {
        self.mode = mode;
    }

    #[must_use]
    fn next_index_raw(&self) -> i32 {
        self.next as i32
    }

    fn for_each_grid(&self, visit: &mut impl FnMut(Grid)) {
        self.entries.iter().for_each(|entry| visit(entry.grid));
    }

    /// Replaces the list without requiring all entries to resolve immediately.
    fn set_list_unchecked<I, G>(&mut self, grids: I)
    where
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    {
        self.next = 0;
        self.entries.clear();
        self.entries.extend(
            grids
                .into_iter()
                .map(IntoGrid::into_grid)
                .map(|grid| CobEntry { grid, cached_id: None }),
        );
    }

    fn erase_from_list(&mut self, grids: &[Grid]) {
        self.next = 0;
        self.entries.retain(|entry| !grids.contains(&entry.grid));
    }

    fn move_to_list_top(&mut self, grids: &[Grid]) -> Result<(), CobManagerError> {
        if self.mode != CobSequentialMode::Priority {
            return Err(CobManagerError::InvalidModeForOperation);
        }
        let mut next = Vec::with_capacity(self.entries.len() + grids.len());
        next.extend_from_slice(grids);
        next.extend(
            self.entries
                .iter()
                .filter(|entry| !grids.contains(&entry.grid))
                .map(|entry| entry.grid),
        );
        self.set_list_unchecked(next);
        Ok(())
    }

    fn move_to_list_bottom(&mut self, grids: &[Grid]) -> Result<(), CobManagerError> {
        if self.mode != CobSequentialMode::Priority {
            return Err(CobManagerError::InvalidModeForOperation);
        }
        let mut next = Vec::with_capacity(self.entries.len() + grids.len());
        next.extend(
            self.entries
                .iter()
                .filter(|entry| !grids.contains(&entry.grid))
                .map(|entry| entry.grid),
        );
        next.extend_from_slice(grids);
        self.set_list_unchecked(next);
        Ok(())
    }

    fn set_next_slot(&mut self, slot: i32) -> Result<(), CobManagerError> {
        if self.mode == CobSequentialMode::Priority {
            return Err(CobManagerError::InvalidModeForOperation);
        }
        let len = self.entries_len_i32()?;
        if !(1..=len).contains(&slot) {
            return Err(CobManagerError::InvalidNext);
        }
        self.next = (slot - 1) as usize;
        Ok(())
    }

    fn set_next_grid(&mut self, grid: Grid) -> Result<(), CobManagerError> {
        if self.mode == CobSequentialMode::Priority {
            return Err(CobManagerError::InvalidModeForOperation);
        }
        let Some(index) = self.entries.iter().position(|entry| entry.grid == grid) else {
            return Err(CobManagerError::CobNotInList(grid));
        };
        i32::try_from(index).map_err(|_error| CobManagerError::InvalidNext)?;
        self.next = index;
        Ok(())
    }

    fn skip(&mut self, n: i32) -> Result<(), CobManagerError> {
        if self.mode == CobSequentialMode::Priority {
            return Err(CobManagerError::InvalidModeForOperation);
        }
        self.skip_unchecked(n)
    }

    fn reset_before_script(&mut self) {
        self.next = 0;
        self.latest = LatestCobFire::default();
        self.runtime.reset();
    }

    fn set_checked_list_validated<I, G>(&mut self, grids: I) -> Result<(), CobManagerCallError>
    where
        CurrentBackend: PlantReadBackend,
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    {
        {
            let grids = validate_grids(grids)?;

            let mut entries = Vec::with_capacity(grids.len());
            for grid in grids {
                let Some(id) = find_cob_at(grid) else {
                    return Err(CobManagerError::CobNotFound(grid).into());
                };
                entries.push(CobEntry {
                    grid,
                    cached_id: Some(id),
                });
            }

            self.next = 0;
            self.entries = entries;
            Ok(())
        }
    }

    fn auto_set_list(&mut self)
    where
        CurrentBackend: PlantReadBackend,
    {
        self.auto_set_list_by(CobListOrder::Horizontal)
    }

    fn auto_set_list_by(&mut self, order: CobListOrder)
    where
        CurrentBackend: PlantReadBackend,
    {
        crate::access::with_backend(|backend| {
            self.auto_entries.clear();
            for plant in crate::live_value::read_or_abort(backend.plants(), "plants") {
                if crate::live_value::read_or_abort(backend.plant_kind(plant), "plant_kind") == PlantKind::CobCannon {
                    self.auto_entries.push(CobEntry {
                        grid: crate::plant::grid_from_handle(backend, plant),
                        cached_id: Some(backend.plant_id(plant)),
                    });
                }
            }
            self.auto_entries.sort_by_key(|entry| order.sort_key(entry.grid));
            self.next = 0;
            std::mem::swap(&mut self.entries, &mut self.auto_entries);
        })
    }

    fn auto_set_list_in_column(&mut self, col: i32)
    where
        CurrentBackend: PlantReadBackend,
    {
        crate::access::with_backend(|backend| {
            self.auto_entries.clear();
            for plant in crate::live_value::read_or_abort(backend.plants(), "plants") {
                let grid = crate::plant::grid_from_handle(backend, plant);
                if grid.col == col
                    && crate::live_value::read_or_abort(backend.plant_kind(plant), "plant_kind") == PlantKind::CobCannon
                {
                    self.auto_entries.push(CobEntry {
                        grid,
                        cached_id: Some(backend.plant_id(plant)),
                    });
                }
            }
            self.auto_entries.sort_by_key(|entry| entry.grid.row);
            self.next = 0;
            std::mem::swap(&mut self.entries, &mut self.auto_entries);
        })
    }

    fn reserve_auto_set_list_capacity(&mut self, capacity: usize) {
        self.entries.reserve(capacity.saturating_sub(self.entries.len()));
        self.auto_entries
            .reserve(capacity.saturating_sub(self.auto_entries.len()));
    }

    fn validate_current_list(&mut self) -> Result<(), CobManagerCallError>
    where
        CurrentBackend: PlantReadBackend,
    {
        for index in 0..self.entries.len() {
            let grid = self.entries[index].grid;
            if self.refresh_entry_at(index).is_none() {
                return Err(CobManagerError::CobNotFound(grid).into());
            }
        }
        Ok(())
    }

    fn visit_recover(&mut self, mut visit: impl FnMut(usize, CobRecoverInfo))
    where
        CurrentBackend: PlantReadBackend,
    {
        let runtime = self.runtime.clone();
        let reserved = runtime.0.borrow();
        for index in 0..self.entries.len() {
            let grid = self.entries[index].grid;
            let id = self.refresh_entry_at(index);
            let recover_time = match id {
                None => NO_EXIST_RECOVER_TIME,
                Some(id) if reserved.contains(&id) => -1,
                Some(id) => cob_recover_time(id),
            };
            visit(index, CobRecoverInfo { grid, id, recover_time });
        }
    }

    fn recover_list(&mut self) -> Vec<CobRecoverInfo>
    where
        CurrentBackend: PlantReadBackend,
    {
        let mut result = Vec::with_capacity(self.entries.len());
        self.visit_recover(|_, info| result.push(info));
        result.sort_by_key(|info| info.recover_time);
        result
    }

    fn usable_list(&mut self) -> Vec<PlantId>
    where
        CurrentBackend: PlantReadBackend,
    {
        let mut result = Vec::new();
        self.visit_recover(|_, info| {
            if info.recover_time == 0 {
                result.extend(info.id);
            }
        });
        result
    }

    fn visit_available_recover(
        &mut self, drop_col: Option<f32>, mut visit: impl FnMut((i32, i32, usize), PlantId),
    ) -> Result<(), CobManagerError>
    where
        CurrentBackend: PlantReadBackend,
    {
        let mut error = None;
        self.visit_recover(|index, info| {
            let raw = info.recover_time;
            let Some(id) = info.id.filter(|_| raw >= 0) else {
                return;
            };
            match drop_col
                .map(|col| classic_roof_fire_delay(info.grid.col + 1, col))
                .transpose()
            {
                Ok(lead) => visit((raw.saturating_sub(lead.unwrap_or(0)).max(0), raw, index), id),
                Err(cause) => {
                    // Finish native reads first, then report the first error in the old raw-sorted order.
                    if error.as_ref().is_none_or(|(key, _)| (raw, index) < *key) {
                        error = Some(((raw, index), cause));
                    }
                }
            }
        });
        error.map_or(Ok(()), |(_, error)| Err(error))
    }

    fn first_recover(&mut self, drop_col: Option<f32>, usable: bool) -> Result<Option<PlantId>, CobManagerError>
    where
        CurrentBackend: PlantReadBackend,
    {
        let mut best = None;
        self.visit_available_recover(drop_col, |key, id| {
            if (!usable || key.0 == 0) && best.as_ref().is_none_or(|(previous, _)| key < *previous) {
                best = Some((key, id));
            }
        })?;
        Ok(best.map(|(_, id)| id))
    }

    fn recover_cob(&mut self) -> Option<PlantId>
    where
        CurrentBackend: PlantReadBackend,
    {
        self.first_recover(None, false).expect("no roof timing conversion")
    }

    fn usable_cob(&mut self) -> Option<PlantId>
    where
        CurrentBackend: PlantReadBackend,
    {
        self.first_recover(None, true).expect("no roof timing conversion")
    }

    fn refresh_entry_at(&mut self, index: usize) -> Option<PlantId>
    where
        CurrentBackend: PlantReadBackend,
    {
        crate::access::with_backend(|backend| {
            let entry = self.entries[index];

            if let Some(id) = entry.cached_id
                && let Some(plant) = crate::live_value::read_or_abort(backend.plant(id), "plant")
                && crate::plant::grid_from_handle(backend, plant) == entry.grid
                && crate::live_value::read_or_abort(backend.plant_kind(plant), "plant_kind") == PlantKind::CobCannon
            {
                return Some(id);
            }

            let id = find_plant_at_kind(entry.grid, PlantKind::CobCannon);
            self.entries[index].cached_id = id;
            id
        })
    }

    fn entries_len_i32(&self) -> Result<i32, CobManagerError> {
        if self.entries.is_empty() {
            return Err(CobManagerError::EmptyList);
        }
        i32::try_from(self.entries.len()).map_err(|_error| CobManagerError::InvalidNext)
    }

    fn current_index(&self) -> Result<usize, CobManagerError> {
        let len = self.entries_len_i32()?;
        if self.next >= len as usize {
            return Err(CobManagerError::InvalidNext);
        }
        Ok(self.next)
    }

    fn skip_unchecked(&mut self, n: i32) -> Result<(), CobManagerError> {
        let len = self.entries_len_i32()?;
        self.next = (self.next as i64 + i64::from(n)).rem_euclid(i64::from(len)) as usize;
        Ok(())
    }

    fn record_latest(&mut self, time: i32, index: usize) {
        if self.latest.writable {
            self.latest.index = Some(index);
            self.latest.time = time;
        }
    }
}

impl Default for CobManager {
    fn default() -> Self {
        Self::new()
    }
}

fn register_plant_tick(grid: Grid, scheduler: &mut TickScheduler)
where
    rsvz_current::CurrentBackend: CardPlantingBackend + 'static,
{
    scheduler.spawn(
        TickOptions::playing_frame().idle_neutral().name("cob_plant"),
        move |_meta| match try_plant_cob(grid) {
            false => Ok(TickControl::Continue),
            true => Ok(TickControl::Stop),
        },
    );
}

fn validate_grid(grid: Grid) -> Result<Grid, CobManagerError> {
    if grid.row < 0 || grid.col < 0 {
        return Err(CobManagerError::InvalidTarget);
    }
    Ok(grid)
}

fn validate_grids<I, G>(grids: I) -> Result<Vec<Grid>, CobManagerError>
where
    I: IntoIterator<Item = G>,
    G: IntoGrid,
{
    grids
        .into_iter()
        .map(|grid| validate_grid(grid.try_into_grid().map_err(|_error| CobManagerError::InvalidTarget)?))
        .collect()
}

fn validate_target(target: CobTarget) -> Result<(), CobManagerError> {
    if target.row < 0 || !target.drop_col.is_finite() {
        return Err(CobManagerError::InvalidTarget);
    }
    Ok(())
}

#[cfg(test)]
mod tests;

mod fire;

mod repair;
