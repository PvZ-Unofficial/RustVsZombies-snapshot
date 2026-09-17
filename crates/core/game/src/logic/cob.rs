//! 与 backend 无关的炮列表、选择、恢复、屋顶时序和补炮逻辑。
//!
//! [`CobManager`] 持有有序炮位、模式/next 状态、延迟发炮 reservation 和最近
//! 发炮修补状态；backend 只提供时钟、植物、坐标、场景和发炮原子能力。
//! 脚本元组的行/格从 `1` 开始，类型化 [`Grid`](crate::model::Grid) 是 core
//! `0-based`；类型化 [`CobTarget`](crate::model::CobTarget) 的行从 `0` 开始，
//! 落点列仍保留脚本浮点语义。
//!
//! 扁平 [`fire`]、[`recover_fire`]、[`raw_fire`] 是使用当前 backend 的 core
//! 入口，从不展示错误。`fire`/`recover_fire` 以 `None` 表示普通的“没有炮
//! 能满足目标”；`raw_fire` 没有逐项结果，因此把这类失败汇总成错误。
//! 绑定及规则拒绝保留返回值；必要读取或访问故障结束当前回调，之前成功的副作用不回滚。

use crate::runtime::RuntimeResult;
use rsvz_backend_api::backend::{ClockBackend, SceneBackend};

mod error;
mod helpers;
mod manager;
mod plant;
mod target;
pub mod timing;

#[cfg(test)]
mod tests;

pub use error::{CobManagerCallError, CobManagerError};
pub use helpers::{cob_recover_time, find_cob_at, find_plant_at_kind};
pub use helpers::{cob_target_to_pixel, fire_cob};
pub use manager::{CobListOrder, CobManager, CobRecoverInfo, CobSequentialMode, NO_EXIST_RECOVER_TIME};
pub use plant::try_plant_cob;
pub use target::{CobFireDrop, IntoCobFireDrop, IntoCobFireDrops, IntoCobTarget, IntoCobTargets};
pub use timing::{
    CLASSIC_POOL_LAND_COB_LEAD, CLASSIC_POOL_WATER_COB_LEAD, CLASSIC_ROOF_COB_FLY_TIME_DATA,
    CLASSIC_ROOF_COB_REFERENCE_TIME, classic_roof_cob_fly_time, classic_roof_fire_delay,
};

/// 使用指定 manager 发射或预约一个目标。
///
/// `row` 是脚本侧 `1-based` 行，`col` 接受整数或浮点落点列。`Some(index)`
/// 是所用炮在 manager 列表中的 `0-based` 下标。目标无效或没有合适炮时返回
/// `Ok(None)`；屋顶场景在完成飞行时间修正后的延迟预约时返回，并不等待炮弹真正
/// 射出。
pub fn fire<C>(manager: &CobManager, row: i32, col: C) -> RuntimeResult<Option<i32>>
where
    rsvz_current::CurrentBackend: ClockBackend
        + CobBackend
        + SceneBackend
        + 'static
        + rsvz_backend_api::BoardReadinessBackend
        + rsvz_backend_api::GameUiBackend,
    (i32, C): IntoCobTarget,
{
    manager.try_fire(row, col)
}

/// 按输入顺序发射或预约 manager 目标。
///
/// 普通的无效/不可满足目标变为 `None`，后续目标继续；backend 或 manager
/// 结构错误返回 `Err` 并停止后续 backend 访问，之前的发射/reservation 保留。
pub fn fire_many<T>(manager: &CobManager, targets: T) -> RuntimeResult<Vec<Option<i32>>>
where
    rsvz_current::CurrentBackend: ClockBackend
        + CobBackend
        + SceneBackend
        + 'static
        + rsvz_backend_api::BoardReadinessBackend
        + rsvz_backend_api::GameUiBackend,
    T: IntoCobTargets,
{
    manager.try_fire_many(targets)
}

/// 选择一门仍可恢复的炮并预约目标。
///
/// 本函数不阻塞线程。`Some(0-based index)` 表示延迟发炮已被接受；没有可恢复
/// 炮时返回 `Ok(None)`。屋顶修正与恢复等待会自动组合。
pub fn recover_fire<C>(manager: &CobManager, row: i32, col: C) -> RuntimeResult<Option<i32>>
where
    rsvz_current::CurrentBackend: ClockBackend
        + CobBackend
        + SceneBackend
        + 'static
        + rsvz_backend_api::BoardReadinessBackend
        + rsvz_backend_api::GameUiBackend,
    (i32, C): IntoCobTarget,
{
    manager.try_recover_fire(row, col)
}

/// 按输入顺序预约可恢复炮目标。
///
/// 普通失败变为 `None` 且不阻止后续目标；runtime 错误停止后续 backend 访问，
/// 但不回滚此前工作。
pub fn recover_fire_many<T>(manager: &CobManager, targets: T) -> RuntimeResult<Vec<Option<i32>>>
where
    rsvz_current::CurrentBackend: ClockBackend
        + CobBackend
        + SceneBackend
        + 'static
        + rsvz_backend_api::BoardReadinessBackend
        + rsvz_backend_api::GameUiBackend,
    T: IntoCobTargets,
{
    manager.try_recover_fire_many(targets)
}

/// 按输入顺序使用明确指定的炮位/目标对发炮。
///
/// raw fire 绕过列表选择且不推进 manager 的 next 游标，但屋顶延迟炮仍共享其
/// reservation 状态。普通转换、缺炮和未恢复错误按顺序汇总为一个
/// [`RuntimeError`]；必要读取或访问故障结束当前回调。
pub fn raw_fire<D>(manager: &CobManager, drops: D) -> RuntimeResult<()>
where
    rsvz_current::CurrentBackend: ClockBackend
        + CobBackend
        + SceneBackend
        + 'static
        + rsvz_backend_api::BoardReadinessBackend
        + rsvz_backend_api::GameUiBackend,
    D: IntoCobFireDrops,
{
    manager.try_raw_fire(drops)
}

/// 发射已由脚本注册期转换完成的目标，不构造逐项返回列表。
#[doc(hidden)]
pub fn fire_prepared(manager: &CobManager, targets: &[crate::model::CobTarget]) -> RuntimeResult<()>
where
    rsvz_current::CurrentBackend: ClockBackend
        + CobBackend
        + SceneBackend
        + 'static
        + rsvz_backend_api::BoardReadinessBackend
        + rsvz_backend_api::GameUiBackend,
{
    manager.fire_prepared(targets)
}

/// 恢复发射已由脚本注册期转换完成的目标，不构造逐项返回列表。
#[doc(hidden)]
pub fn recover_fire_prepared(manager: &CobManager, targets: &[crate::model::CobTarget]) -> RuntimeResult<()>
where
    rsvz_current::CurrentBackend: ClockBackend
        + CobBackend
        + SceneBackend
        + 'static
        + rsvz_backend_api::BoardReadinessBackend
        + rsvz_backend_api::GameUiBackend,
{
    manager.recover_fire_prepared(targets)
}

thread_local! {
    static COBS: std::cell::RefCell<crate::resources::CobResource> = std::cell::RefCell::new(crate::resources::CobResource::default());
}

/// Returns the default manager, preserving its identity across script resets.
pub fn current_cob_manager() -> CobManager {
    crate::resources::ensure_script_reset(&COBS);
    COBS.with_borrow(|resource| resource.core.clone())
}

/// Atomic capability combination required by cannon operations.
pub trait CobBackend: rsvz_backend_api::CobFireBackend + rsvz_backend_api::GridGeometryBackend {}
impl<T> CobBackend for T where T: rsvz_backend_api::CobFireBackend + rsvz_backend_api::GridGeometryBackend {}
