//! 即时炮管理 API。
//!
//! 普通调用使用当前脚本的默认炮管理器；所有涉及炮列表的全局函数都可把
//! [`&CobManager`](CobManager) 作为首个参数，从而改用显式炮列表。克隆
//! 管理器会共享同一份炮列表和炮序状态。
//!
//! 脚本 `(row, col)` 与 `(row, drop_col)` 中的行列从 `1` 开始，炮落点列
//! 允许小数。类型化的 [`Grid`](rsvz_model::Grid) 是 core `0-based` 值；普通脚本需要显式值时
//! 应优先使用 [`crate::grid::grid`]。发炮成功返回的炮列表下标从 `0` 开始。
//!
//! [`Fire`] (`fire`)、[`RecoverFire`] (`recover_fire`) 和 [`RawFire`]
//! (`raw_fire`) 是即时 API。它们在屋顶场景会自动修正不同炮列的飞行时间，
//! 不需要另一组 roof 函数。low DSL 的 [`crate::dsl::P`] (`p`)、
//! [`crate::dsl::Pp`] (`pp`) 和
//! [`crate::dsl::rp`] 只构造时间轴表达式，不会在构造时发炮。
//!
//! `fire`/`recover_fire`/`raw_fire` 与列表设置、自动发现操作中的非 `try`
//! 版本会报告错误后返回；对应 `try_*` 版本返回扁平 [`RuntimeResult`](rsvz_backend_api::error::RuntimeResult)。
//! 查询和炮序编辑操作直接返回其文档所示的 `ScriptResult`，不会偷偷展示
//! 错误。批量发炮的普通失败按项保留为 `None` 并继续；backend 故障停止
//! 后续 backend 访问，已经发射或预约的炮不会回滚。

use crate::runtime::RuntimeResult;
use crate::script::ScriptResult;
use rsvz_backend_api::backend::{BoardReadinessBackend, ClockBackend, CobFireBackend, GameUiBackend, SceneBackend};
use rsvz_game::logic::IntoGrid;
use rsvz_game::logic::cards::CardPlantingBackend;
use rsvz_game::logic::cob::CobBackend;
use rsvz_game::logic::cob::{
    CobListOrder, CobRecoverInfo, CobSequentialMode, IntoCobFireDrops, IntoCobTarget, IntoCobTargets,
};
use rsvz_model::model::{Grid, PlantId};

#[doc(hidden)]
pub fn __reset_current() {
    rsvz_game::logic::cob::current_cob_manager().reset_before_script();
}

#[doc(hidden)]
pub use rsvz_game::logic::cob::current_cob_manager as __current_core;

pub use rsvz_game::cob::CobManager;

crate::callable::callable_api! {
    /// 按炮列表顺序访问当前或显式管理器中的全部炮位。
    ///
    /// 回调收到的是 core [`Grid`]，其 `row`/`col` 字段从 `0` 开始。该函数
    /// 只读取列表，不检查相应炮是否仍然存在，也不改变 next 游标。
    pub for_each_cob_grid: ForEachCobGrid;
    where {}
    impl<F>
    where {
        F: FnMut(Grid),
    }
    call(visit: F) -> () { rsvz_game::cob::for_each_cob_grid(visit) }
    impl<F>
    where {
        F: FnMut(Grid),
    }
    call(manager: &CobManager, visit: F) -> () { rsvz_game::cob::for_each_cob_grid_with_manager(manager, visit) }
}

/// Discovers the live cobs in one script-facing tail column.
///
/// The explicit manager is replaced atomically and ordered by row. Errors
/// are reported and leave its previous list unchanged.
pub use rsvz_game::cob::set_cob_columns;

crate::callable::callable_api! {
    #[doc(alias = "SetSequentialMode")]
    /// 设置当前或显式管理器的炮序模式。
    ///
    /// [`CobSequentialMode::Space`] 严格使用当前炮位；`Time` 从当前炮位向后
    /// 查找；`Priority` 每次从列表开头查找。切换模式不会重排炮列表。
    pub set_cob_sequential_mode: SetCobSequentialMode;
    where {}
    impl<>
    where {}
    call(mode: CobSequentialMode) -> () { rsvz_game::cob::set_cob_sequential_mode(mode) }
    impl<>
    where {}
    call(manager: &CobManager, mode: CobSequentialMode) -> () { rsvz_game::cob::set_cob_sequential_mode_with_manager(manager, mode) }
}

crate::callable::callable_api! {
    #[doc(alias = "SetNext")]
    /// 用 `1-based` 列表序号设置下一门炮。
    ///
    /// `set_next_cob_slot(1)` 选择列表中的第一门炮；这与 [`fire`] 返回的
    /// `0-based` 下标不同。该操作只适用于 `Space` 和 `Time` 模式，在
    /// `Priority` 模式或序号越界时返回错误。
    pub set_next_cob_slot: SetNextCobSlot;
    where {}
    impl<>
    where {}
    call(slot: i32) -> ScriptResult { rsvz_game::cob::set_next_cob_slot(slot) }
    impl<>
    where {}
    call(manager: &CobManager, slot: i32) -> ScriptResult { rsvz_game::cob::set_next_cob_slot_with_manager(manager, slot) }
}

crate::callable::callable_api! {
    #[doc(alias = "SetNext")]
    /// 按炮位设置下一门炮。
    ///
    /// 元组炮位使用 `1-based` 脚本坐标；[`Grid`] 原样按 core `0-based`
    /// 坐标解释。目标炮位必须已经在列表中。该操作不适用于 `Priority` 模式。
    pub set_next_cob: SetNextCob;
    where {}
    impl<G>
    where {
        G: IntoGrid,
    }
    call(grid: G) -> ScriptResult { rsvz_game::cob::set_next_cob(grid) }
    impl<G>
    where {
        G: IntoGrid,
    }
    call(manager: &CobManager, grid: G) -> ScriptResult { rsvz_game::cob::set_next_cob_with_manager(manager, grid) }
}

crate::callable::callable_api! {
    #[doc(alias = "Skip")]
    /// 将 next 游标循环前移 `n` 个炮位。
    ///
    /// `n` 可以为负数。该操作只适用于 `Space` 和 `Time` 模式；空列表或
    /// `Priority` 模式返回错误。
    pub skip_cobs: SkipCobs;
    where {}
    impl<>
    where {}
    call(n: i32) -> ScriptResult { rsvz_game::cob::skip_cobs(n) }
    impl<>
    where {}
    call(manager: &CobManager, n: i32) -> ScriptResult { rsvz_game::cob::skip_cobs_with_manager(manager, n) }
}

crate::callable::callable_api! {
    #[doc(alias = "EraseFromList")]
    /// 从当前或显式管理器的列表中删除指定炮位。
    ///
    /// 不在列表中的炮位会被忽略。删除完成后 next 游标重置到列表开头；该
    /// 操作只修改炮列表，不会铲除场上的玉米加农炮。
    pub erase_cobs_from_list: EraseCobsFromList;
    where {}
    impl<I, G>
    where {
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    }
    call(grids: I) -> () { rsvz_game::cob::erase_cobs_from_list(grids) }
    impl<I, G>
    where {
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    }
    call(manager: &CobManager, grids: I) -> () { rsvz_game::cob::erase_cobs_from_list_with_manager(manager, grids) }
}

crate::callable::callable_api! {
    #[doc(alias = "MoveToListTop")]
    /// 在 `Priority` 模式下把指定炮位依输入顺序移到列表开头。
    ///
    /// 尚不在列表中的炮位会被加入列表。其他炮位保持相对顺序，next 游标
    /// 重置到开头；非 `Priority` 模式返回错误。
    pub move_cobs_to_list_top: MoveCobsToListTop;
    where {}
    impl<I, G>
    where {
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    }
    call(grids: I) -> ScriptResult { rsvz_game::cob::move_cobs_to_list_top(grids) }
    impl<I, G>
    where {
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    }
    call(manager: &CobManager, grids: I) -> ScriptResult { rsvz_game::cob::move_cobs_to_list_top_with_manager(manager, grids) }
}

crate::callable::callable_api! {
    #[doc(alias = "MoveToListBottom")]
    /// 在 `Priority` 模式下把指定炮位依输入顺序移到列表末尾。
    ///
    /// 尚不在列表中的炮位会被加入列表。其他炮位保持相对顺序，next 游标
    /// 重置到开头；非 `Priority` 模式返回错误。
    pub move_cobs_to_list_bottom: MoveCobsToListBottom;
    where {}
    impl<I, G>
    where {
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    }
    call(grids: I) -> ScriptResult { rsvz_game::cob::move_cobs_to_list_bottom(grids) }
    impl<I, G>
    where {
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    }
    call(manager: &CobManager, grids: I) -> ScriptResult { rsvz_game::cob::move_cobs_to_list_bottom_with_manager(manager, grids) }
}

crate::callable::callable_api! {
    #[doc(alias = "GetRecoverList")]
    /// 返回当前或显式列表中各炮的恢复信息，并按恢复值升序排列。
    ///
    /// [`CobRecoverInfo::recover_time`] 为 `0` 表示立即可用，正数表示还需
    /// 等待的厘秒，负数表示当前不可用或已经被延迟发炮预约；`id == None`
    /// 表示相应炮位没有玉米加农炮。返回的 [`Grid`] 为 core `0-based` 值。
    pub recover_cob_list: RecoverCobList;
    where {
        rsvz_current::CurrentBackend: CobBackend,
    }
    impl<>
    where {}
    call() -> ScriptResult<Vec<CobRecoverInfo>> { rsvz_game::cob::recover_cob_list() }
    impl<>
    where {}
    call(manager: &CobManager) -> ScriptResult<Vec<CobRecoverInfo>> { rsvz_game::cob::recover_cob_list_with_manager(manager) }
}

crate::callable::callable_api! {
    #[doc(alias = "GetUsableList")]
    /// 返回当前或显式列表中此刻可以立即发射的炮 ID。
    ///
    /// 结果顺序与 [`recover_cob_list`] 的恢复时间排序一致，不改变炮序游标。
    pub usable_cob_list: UsableCobList;
    where {
        rsvz_current::CurrentBackend: CobBackend,
    }
    impl<>
    where {}
    call() -> ScriptResult<Vec<PlantId>> { rsvz_game::cob::usable_cob_list() }
    impl<>
    where {}
    call(manager: &CobManager) -> ScriptResult<Vec<PlantId>> { rsvz_game::cob::usable_cob_list_with_manager(manager) }
}

crate::callable::callable_api! {
    #[doc(alias = "GetRecoverPtr")]
    /// 返回恢复时间最短且仍可恢复的炮 ID。
    ///
    /// 没有可恢复的炮时返回 `Ok(None)`。该查询不预约炮，也不改变 next 游标。
    pub recover_cob: RecoverCob;
    where {
        rsvz_current::CurrentBackend: CobBackend,
    }
    impl<>
    where {}
    call() -> ScriptResult<Option<PlantId>> { rsvz_game::cob::recover_cob() }
    impl<>
    where {}
    call(manager: &CobManager) -> ScriptResult<Option<PlantId>> { rsvz_game::cob::recover_cob_with_manager(manager) }
}

crate::callable::callable_api! {
    #[doc(alias = "GetUsablePtr")]
    /// 返回此刻可以立即发射的第一门炮 ID。
    ///
    /// 没有可用炮时返回 `Ok(None)`。该查询不预约炮，也不改变 next 游标。
    pub usable_cob: UsableCob;
    where {
        rsvz_current::CurrentBackend: CobBackend,
    }
    impl<>
    where {}
    call() -> ScriptResult<Option<PlantId>> { rsvz_game::cob::usable_cob() }
    impl<>
    where {}
    call(manager: &CobManager) -> ScriptResult<Option<PlantId>> { rsvz_game::cob::usable_cob_with_manager(manager) }
}

crate::callable::callable_api! {
    #[doc(alias = "GetRoofRecoverList")]
    /// 返回面向指定落点列的屋顶炮恢复信息，并按修正后恢复值排序。
    ///
    /// 修正值考虑屋顶炮飞行时间差：炮只要能在统一 `387cs` 命中参考
    /// 内按时发射，就可表现为恢复时间 `0`。`drop_col` 使用脚本落点列语义。
    pub roof_recover_cob_list: RoofRecoverCobList;
    where {
        rsvz_current::CurrentBackend: CobBackend,
    }
    impl<>
    where {}
    call(drop_col: f32) -> ScriptResult<Vec<CobRecoverInfo>> { rsvz_game::cob::roof_recover_cob_list(drop_col) }
    impl<>
    where {}
    call(manager: &CobManager, drop_col: f32) -> ScriptResult<Vec<CobRecoverInfo>> { rsvz_game::cob::roof_recover_cob_list_with_manager(manager, drop_col) }
}

crate::callable::callable_api! {
    #[doc(alias = "GetRoofUsableList")]
    /// 返回能面向指定落点列按屋顶修正时序发射的炮 ID。
    ///
    /// 这是 [`roof_recover_cob_list`] 中修正后恢复时间为 `0` 的项目。
    pub roof_usable_cob_list: RoofUsableCobList;
    where {
        rsvz_current::CurrentBackend: CobBackend,
    }
    impl<>
    where {}
    call(drop_col: f32) -> ScriptResult<Vec<PlantId>> { rsvz_game::cob::roof_usable_cob_list(drop_col) }
    impl<>
    where {}
    call(manager: &CobManager, drop_col: f32) -> ScriptResult<Vec<PlantId>> { rsvz_game::cob::roof_usable_cob_list_with_manager(manager, drop_col) }
}

crate::callable::callable_api! {
    #[doc(alias = "GetRoofRecoverPtr")]
    /// 返回面向指定落点列时恢复时间最短的屋顶炮 ID。
    ///
    /// 没有可恢复炮时返回 `Ok(None)`；查询不预约炮或推进炮序。
    pub roof_recover_cob: RoofRecoverCob;
    where {
        rsvz_current::CurrentBackend: CobBackend,
    }
    impl<>
    where {}
    call(drop_col: f32) -> ScriptResult<Option<PlantId>> { rsvz_game::cob::roof_recover_cob(drop_col) }
    impl<>
    where {}
    call(manager: &CobManager, drop_col: f32) -> ScriptResult<Option<PlantId>> { rsvz_game::cob::roof_recover_cob_with_manager(manager, drop_col) }
}

crate::callable::callable_api! {
    #[doc(alias = "GetRoofUsablePtr")]
    /// 返回能面向指定落点列按屋顶修正时序发射的第一门炮 ID。
    ///
    /// 没有可用炮时返回 `Ok(None)`；查询不预约炮或推进炮序。
    pub roof_usable_cob: RoofUsableCob;
    where {
        rsvz_current::CurrentBackend: CobBackend,
    }
    impl<>
    where {}
    call(drop_col: f32) -> ScriptResult<Option<PlantId>> { rsvz_game::cob::roof_usable_cob(drop_col) }
    impl<>
    where {}
    call(manager: &CobManager, drop_col: f32) -> ScriptResult<Option<PlantId>> { rsvz_game::cob::roof_usable_cob_with_manager(manager, drop_col) }
}

#[doc(alias = "GetRoofFlyTime")]
/// 返回屋顶炮飞行时间，单位为厘秒。
///
/// `cob_col` 是从 `1` 开始的炮尾所在列，`drop_col` 是允许小数的脚本
/// 落点列。该纯计算不读取 backend；炮列不在 `1..=8` 或落点非有限数时
/// 返回 core 错误。
pub use rsvz_game::cob::roof_cob_fly_time;

crate::callable::callable_api! {
    #[doc(alias = "SetList")]
    /// 校验并替换当前或显式管理器的有序炮列表，不自动报告错误。
    ///
    /// 元组炮位使用从 `1` 开始的脚本坐标。输入顺序就是 `Space`/`Time`
    /// 模式的基础炮序；列表替换成功后 next 游标重置到第一项。所有炮位都会
    /// 先完成校验，任一位置没有炮时返回错误并保留原列表。
    pub try_set_cobs: TrySetCobs;
    where {
        rsvz_current::CurrentBackend: CobFireBackend,
    }
    impl<I, G>
    where {
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    }
    call(grids: I) -> RuntimeResult<()> { rsvz_game::cob::try_set_cobs(grids) }
    impl<I, G>
    where {
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    }
    call(manager: &CobManager, grids: I) -> RuntimeResult<()> { rsvz_game::cob::try_set_cobs_with_manager(manager, grids) }
}

crate::callable::callable_api! {
    #[doc(alias = "SetList")]
    /// 校验并替换有序炮列表，报告错误后返回。
    ///
    /// 行为与 [`try_set_cobs`] 相同，但错误不会传播。默认管理器形式为
    /// `set_cobs([(1, 1), (2, 1)])`；显式形式为
    /// `set_cobs(&manager, [(1, 1), (2, 1)])`。
    pub set_cobs: SetCobs;
    where {
        rsvz_current::CurrentBackend: CobFireBackend,
    }
    impl<I, G>
    where {
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    }
    call(grids: I) -> () { rsvz_game::cob::set_cobs(grids) }
    impl<I, G>
    where {
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    }
    call(manager: &CobManager, grids: I) -> () { rsvz_game::cob::set_cobs_with_manager(manager, grids) }
}

crate::callable::callable_api! {
    #[doc(alias = "AutoSetList")]
    /// 扫描场上全部玉米加农炮并替换炮列表，不自动报告错误。
    ///
    /// 无参数时使用 [`CobListOrder::Horizontal`]；也可传入排序方式。显式
    /// 管理器放在第一个参数。发现结果会完全替换旧列表并把 next 重置到开头。
    pub try_auto_set_cobs: TryAutoSetCobs;
    where {
        rsvz_current::CurrentBackend: CobFireBackend,
    }
    impl<>
    where {}
    call() -> RuntimeResult<()> { rsvz_game::cob::try_auto_set_cobs() }
    impl<>
    where {}
    call(order: CobListOrder) -> RuntimeResult<()> { rsvz_game::cob::try_auto_set_cobs_by(order) }
    impl<>
    where {}
    call(manager: &CobManager) -> RuntimeResult<()> { rsvz_game::cob::try_auto_set_cobs_with_manager(manager) }
    impl<>
    where {}
    call(manager: &CobManager, order: CobListOrder) -> RuntimeResult<()> { rsvz_game::cob::try_auto_set_cobs_by_with_manager(manager, order) }
}

crate::callable::callable_api! {
    #[doc(alias = "AutoSetList")]
    /// 扫描并排序场上全部玉米加农炮，报告错误后返回。
    ///
    /// 行为与 [`try_auto_set_cobs`] 相同。常用形式为 `auto_set_cobs()`、
    /// `auto_set_cobs(CobListOrder::Vertical)` 和 `auto_set_cobs(&manager)`。
    pub auto_set_cobs: AutoSetCobs;
    where {
        rsvz_current::CurrentBackend: CobFireBackend,
    }
    impl<>
    where {}
    call() -> () { rsvz_game::cob::auto_set_cobs() }
    impl<>
    where {}
    call(order: CobListOrder) -> () { rsvz_game::cob::auto_set_cobs_by(order) }
    impl<>
    where {}
    call(manager: &CobManager) -> () { rsvz_game::cob::auto_set_cobs_with_manager(manager) }
    impl<>
    where {}
    call(manager: &CobManager, order: CobListOrder) -> () { rsvz_game::cob::auto_set_cobs_by_with_manager(manager, order) }
}

crate::callable::callable_api! {
    #[doc(alias = "RecoverFire")]
    #[doc(alias = "RecoverRoofFire")]
    /// 选择仍可恢复的炮并预约到可发射时刻，不自动报告错误。
    ///
    /// 该函数不会阻塞 Rust 线程。单目标成功返回 `Some(0-based list index)`；
    /// `Some` 表示发炮已经成功预约，不保证炮弹已在当前时刻射出。屋顶场景
    /// 同时应用恢复等待与屋顶飞行时间修正。
    ///
    /// # 调用形式
    ///
    /// ```rust,no_run
    /// # #[cfg(any(feature = "pvz-emulator", feature = "pvz-1-0-0-1051", feature = "pvz-portable"))]
    /// # {
    /// # use rsvz::prelude::*;
    /// let one = try_recover_fire(2, 9.0);
    /// let many = try_recover_fire([(2, 9.0), (4, 9.0)]);
    /// let manager = CobManager::new();
    /// let explicit = try_recover_fire(&manager, 2, 9.0);
    /// # let _ = (one, many, explicit);
    /// # }
    /// ```
    ///
    /// 无效目标或没有可恢复炮时返回 `Ok(None)`，批量中作为 `None` 保留并
    /// 继续。backend、timeline 注册或管理器结构错误返回 `Err`，停止后续
    /// backend 访问；此前已完成的发射或预约不会回滚。
    pub try_recover_fire: TryRecoverFire;
    where {
        rsvz_current::CurrentBackend: ClockBackend + CobBackend + SceneBackend + GameUiBackend + BoardReadinessBackend + 'static,
    }
    impl<T>
    where {
        T: IntoCobTargets,
    }
    call(targets: T) -> RuntimeResult<Vec<Option<i32>>> { rsvz_game::cob::try_recover_fire_many(targets) }
    impl<C>
    where {
        (i32, C): IntoCobTarget,
    }
    call(row: i32, col: C) -> RuntimeResult<Option<i32>> { rsvz_game::cob::try_recover_fire(row, col) }
    impl<T>
    where {
        T: IntoCobTargets,
    }
    call(manager: &CobManager, targets: T) -> RuntimeResult<Vec<Option<i32>>> { rsvz_game::cob::try_recover_fire_many_with_manager(manager, targets) }
    impl<C>
    where {
        (i32, C): IntoCobTarget,
    }
    call(manager: &CobManager, row: i32, col: C) -> RuntimeResult<Option<i32>> { rsvz_game::cob::try_recover_fire_with_manager(manager, row, col) }
}

crate::callable::callable_api! {
    #[doc(alias = "RecoverFire")]
    #[doc(alias = "RecoverRoofFire")]
    /// 选择仍可恢复的炮并预约发射，报告失败后返回。
    ///
    /// 调用形式和返回值与 [`try_recover_fire`] 相同。每个 `None` 会被逐项
    /// 报告，运行时错误也会报告但不传播，因此当前 callback 会继续。批量
    /// 遇到运行时错误时，此前预约仍保留，但返回空 `Vec`，因为无法构造完整
    /// 的一一对应结果。
    pub recover_fire: RecoverFire;
    where {
        rsvz_current::CurrentBackend: ClockBackend + CobBackend + SceneBackend + GameUiBackend + BoardReadinessBackend + 'static,
    }
    impl<T>
    where {
        T: IntoCobTargets,
    }
    call(targets: T) -> Vec<Option<i32>> { rsvz_game::cob::recover_fire_many(targets) }
    impl<C>
    where {
        (i32, C): IntoCobTarget,
    }
    call(row: i32, col: C) -> Option<i32> { rsvz_game::cob::recover_fire(row, col) }
    impl<T>
    where {
        T: IntoCobTargets,
    }
    call(manager: &CobManager, targets: T) -> Vec<Option<i32>> { rsvz_game::cob::recover_fire_many_with_manager(manager, targets) }
    impl<C>
    where {
        (i32, C): IntoCobTarget,
    }
    call(manager: &CobManager, row: i32, col: C) -> Option<i32> { rsvz_game::cob::recover_fire_with_manager(manager, row, col) }
}

crate::callable::callable_api! {
    /// 使用明确指定的炮位向目标发炮，不自动报告错误。
    ///
    /// 四参数形式依次为 `cob_row, cob_col, drop_row, drop_col`，炮位与目标行
    /// 都从 `1` 开始。也可传入一项或一批 [`rsvz_game::logic::cob::CobFireDrop`]
    /// 兼容输入。
    ///
    /// raw fire 不要求炮位存在于任何管理器列表，也不推进任何 next 游标。
    /// 屋顶延迟发炮的 reservation 由当前 runtime 共享，所有炮管理器都会
    /// 避开已经预约的炮。屋顶场景自动应用飞行时间修正。
    ///
    /// 普通的无效输入、找不到炮或炮未恢复会汇总成 `Err(RuntimeError)`；
    /// 批量会继续处理这类 manager 错误。backend 故障停止后续访问，已经
    /// 发射或预约的炮不会回滚。
    pub try_raw_fire: TryRawFire;
    where {
        rsvz_current::CurrentBackend: ClockBackend + CobBackend + SceneBackend + GameUiBackend + BoardReadinessBackend + 'static,
    }
    impl<D>
    where {
        D: IntoCobFireDrops,
    }
    call(drops: D) -> RuntimeResult<()> { rsvz_game::cob::try_raw_fire(drops) }
    impl<C>
    where {
        (i32, i32, i32, C): IntoCobFireDrops,
    }
    call(cob_row: i32, cob_col: i32, row: i32, col: C) -> RuntimeResult<()> { rsvz_game::cob::try_raw_fire_coordinates(cob_row, cob_col, row, col) }
}

crate::callable::callable_api! {
    /// 使用明确指定的炮位发炮，汇总并报告失败后返回。
    ///
    /// 调用形式与 [`try_raw_fire`] 相同。该函数没有逐项返回值；批量普通
    /// manager 错误会按输入顺序合并成一次多行报告。
    ///
    /// ```rust,no_run
    /// # #[cfg(any(feature = "pvz-emulator", feature = "pvz-1-0-0-1051", feature = "pvz-portable"))]
    /// # {
    /// # use rsvz::prelude::*;
    /// raw_fire(1, 1, 2, 9.0);
    /// raw_fire([(1, 1, 2, 9.0), (2, 1, 4, 9.0)]);
    /// # }
    /// ```
    pub raw_fire: RawFire;
    where {
        rsvz_current::CurrentBackend: ClockBackend + CobBackend + SceneBackend + GameUiBackend + BoardReadinessBackend + 'static,
    }
    impl<D>
    where {
        D: IntoCobFireDrops,
    }
    call(drops: D) -> () { rsvz_game::cob::raw_fire(drops) }
    impl<C>
    where {
        (i32, i32, i32, C): IntoCobFireDrops,
    }
    call(cob_row: i32, cob_col: i32, row: i32, col: C) -> () { rsvz_game::cob::raw_fire_coordinates(cob_row, cob_col, row, col) }
}

#[doc(alias = "Plant")]
/// 注册一个逐 tick 补种指定炮位的任务。
///
/// 脚本炮位从 `1` 开始。任务会等待两张玉米投手和玉米加农炮卡可用，
/// 按游戏规则依次种植，直到炮完成；本次调用不会立即保证炮已经出现，
/// 也不会自动把炮位加入任何炮管理器列表。
pub use rsvz_game::cob::plant_cob;

crate::callable::callable_api! {
    #[doc(alias = "FixLatest")]
    /// 注册修补当前或显式管理器最近一次发射炮位的任务。
    ///
    /// 任务等待炮发射后可安全铲除的时机，再在原位置通过 [`plant_cob`] 同类
    /// 状态机补炮；只支持原地铲种。尚无最近发炮记录或管理器状态无效时返回
    /// 错误。调用后到修补完成前，新的最近发炮记录会被暂时锁定。
    pub fix_latest_cob: FixLatestCob;
    where {
        rsvz_current::CurrentBackend: ClockBackend + CardPlantingBackend + 'static,
    }
    impl<>
    where {}
    call() -> ScriptResult { rsvz_game::cob::fix_latest_cob() }
    impl<>
    where {}
    call(manager: &CobManager) -> ScriptResult { rsvz_game::cob::fix_latest_cob_with_manager(manager) }
}

crate::callable::callable_api! {
    #[doc(alias = "Fire")]
    #[doc(alias = "RoofFire")]
    /// 按炮管理器的炮序向一个或多个目标发炮，不自动报告失败。
    ///
    /// 默认使用当前炮管理器；首参传入 [`&CobManager`](CobManager) 时使用该
    /// 显式列表。目标行从 `1` 开始，落点列允许小数，批量按输入顺序处理。
    /// 普通场景在调用时实际发炮；屋顶场景会预约经过飞行时间修正的延迟
    /// 发射，使从调用参考时刻到命中统一为 `387cs`。
    ///
    /// # 调用形式
    ///
    /// ```rust,no_run
    /// # #[cfg(any(feature = "pvz-emulator", feature = "pvz-1-0-0-1051", feature = "pvz-portable"))]
    /// # {
    /// # use rsvz::prelude::*;
    /// let one = try_fire(2, 9.0);
    /// let many = try_fire([(2, 9.0), (4, 9.0)]);
    /// let wind = CobManager::new();
    /// let explicit_one = try_fire(&wind, 2, 9.0);
    /// let explicit_many = try_fire(&wind, [(2, 9.0), (4, 9.0)]);
    /// # let _ = (one, many, explicit_one, explicit_many);
    /// # }
    /// ```
    ///
    /// 单目标返回 `RuntimeResult<Option<i32>>`，其中下标从 `0` 开始；批量
    /// 返回一一对应的 `Vec<Option<i32>>`。无效目标、空列表或没有可用炮是
    /// `Ok(None)`，并不自动显示。批量普通失败继续后续目标。backend、timeline
    /// 注册或管理器结构错误返回 `Err` 并停止后续 backend 访问；此前成功
    /// 的发炮或预约不会回滚。
    pub try_fire: TryFire;
    where {
        rsvz_current::CurrentBackend: ClockBackend + CobBackend + SceneBackend + GameUiBackend + BoardReadinessBackend + 'static,
    }
    impl<T>
    where { T: IntoCobTargets, }
    call(targets: T) -> RuntimeResult<Vec<Option<i32>>> { rsvz_game::cob::try_fire_many(targets) }
    impl<C>
    where { (i32, C): IntoCobTarget, }
    call(row: i32, col: C) -> RuntimeResult<Option<i32>> { rsvz_game::cob::try_fire(row, col) }
    impl<T>
    where { T: IntoCobTargets, }
    call(manager: &CobManager, targets: T) -> RuntimeResult<Vec<Option<i32>>> { rsvz_game::cob::try_fire_many_with_manager(manager, targets) }
    impl<C>
    where { (i32, C): IntoCobTarget, }
    call(manager: &CobManager, row: i32, col: C) -> RuntimeResult<Option<i32>> { rsvz_game::cob::try_fire_with_manager(manager, row, col) }
}

crate::callable::callable_api! {
    #[doc(alias = "Fire")]
    #[doc(alias = "RoofFire")]
    /// 按炮管理器炮序发炮，报告失败并返回所用炮的列表下标。
    ///
    /// 调用形式、屋顶时序和返回形状与 [`try_fire`] 相同。普通失败会逐项
    /// 报告并返回 `None`；运行时错误会报告但不传播，所以当前 callback 中
    /// 本次调用之后的语句仍会执行。批量遇到运行时错误时此前发射仍保留，
    /// 但返回空 `Vec`。返回值不需要时可以直接忽略。
    ///
    /// ```rust,no_run
    /// # #[cfg(any(feature = "pvz-emulator", feature = "pvz-1-0-0-1051", feature = "pvz-portable"))]
    /// # {
    /// # use rsvz::prelude::*;
    /// let index: Option<i32> = fire(2, 9.0);
    /// let indices: Vec<Option<i32>> = fire([(2, 9.0), (4, 9.0)]);
    /// let wind = CobManager::new();
    /// let wind_index = fire(&wind, 2, 9.0);
    /// # let _ = (index, indices, wind_index);
    /// # }
    /// ```
    pub fire: Fire;
    where {
        rsvz_current::CurrentBackend: ClockBackend + CobBackend + SceneBackend + GameUiBackend + BoardReadinessBackend + 'static,
    }
    impl<T>
    where { T: IntoCobTargets, }
    call(targets: T) -> Vec<Option<i32>> { rsvz_game::cob::fire_many(targets) }
    impl<C>
    where { (i32, C): IntoCobTarget, }
    call(row: i32, col: C) -> Option<i32> { rsvz_game::cob::fire(row, col) }
    impl<T>
    where { T: IntoCobTargets, }
    call(manager: &CobManager, targets: T) -> Vec<Option<i32>> { rsvz_game::cob::fire_many_with_manager(manager, targets) }
    impl<C>
    where { (i32, C): IntoCobTarget, }
    call(manager: &CobManager, row: i32, col: C) -> Option<i32> { rsvz_game::cob::fire_with_manager(manager, row, col) }
}
