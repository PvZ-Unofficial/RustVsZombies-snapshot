//! 炮的语义生效时间规划。
//!
//! 这些 helper 把期望爆炸/生效时刻换算为经典 PvZ 场景中的命令时刻：普通
//! 陆路提前 `373cs`，泳池/雾夜水路提前 `378cs`，屋顶以 `387cs` 为参考并
//! 使用按炮列校准的飞行公式。本模块不访问 backend，也不实际发炮。

use super::CobManagerError;

/// 经典平地场景陆路的命令提前量。
pub const CLASSIC_POOL_LAND_COB_LEAD: i32 = 373;
/// 经典泳池/雾夜脚本第 3、4 行水路的命令提前量。
pub const CLASSIC_POOL_WATER_COB_LEAD: i32 = 378;
/// 屋顶参考飞行时间。
pub const CLASSIC_ROOF_COB_REFERENCE_TIME: i32 = 387;

/// 屋顶第 `1..=8` 列炮的 `{最小落点 x, 最短飞行时间}` 校准表。
pub const CLASSIC_ROOF_COB_FLY_TIME_DATA: [(i32, i32); 8] = [
    (515, 359),
    (499, 362),
    (515, 364),
    (499, 367),
    (515, 369),
    (499, 372),
    (511, 373),
    (511, 373),
];

/// 使用整数校准公式计算经典屋顶炮飞行时间。
///
/// `cob_col` 与 `drop_col` 使用脚本列坐标；炮列不在 `1..=8` 或落点列不是
/// 有限数时返回 [`CobManagerError::InvalidTarget`]。
pub fn classic_roof_cob_fly_time(cob_col: i32, drop_col: f32) -> Result<i32, CobManagerError> {
    if !(1..=8).contains(&cob_col) {
        return Err(CobManagerError::InvalidTarget);
    }
    let (min_drop_x, min_fly_time) = CLASSIC_ROOF_COB_FLY_TIME_DATA[(cob_col - 1) as usize];
    if !drop_col.is_finite() {
        return Err(CobManagerError::InvalidTarget);
    }
    let drop_x = (drop_col * 80.0) as i32;
    if drop_x >= min_drop_x {
        Ok(min_fly_time)
    } else {
        let delta = i64::from(drop_x) - i64::from(min_drop_x - 1);
        Ok((i64::from(min_fly_time) + 1 - delta / 32) as i32)
    }
}

/// 返回该屋顶炮相对 `387cs` 参考命令时刻可推迟多少，同时保持相同生效时刻。
pub fn classic_roof_fire_delay(cob_col: i32, drop_col: f32) -> Result<i32, CobManagerError> {
    classic_roof_cob_fly_time(cob_col, drop_col).map(|fly_time| CLASSIC_ROOF_COB_REFERENCE_TIME - fly_time)
}
