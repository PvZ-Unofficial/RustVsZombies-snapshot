//! 从脚本 `1-based` 坐标显式构造 core 坐标值。
//!
//! [`Grid`] 与 [`CobTarget`] 字段使用 core `0-based` 行号，而普通脚本使用
//! `1-based` 行列。优先使用本模块的 [`grid`] 和
//! [`target`] 可以把转换明确写在调用处，避免直接填写 core 字段。

use rsvz_model::model::{CobTarget, Grid};

/// 从脚本 `1-based` 行列构造 core `0-based` [`Grid`]。
///
/// `grid(2, 3)` 与普通 API 中的 `(2, 3)` 表示同一个游戏格，返回值内部为
/// `Grid { row: 1, col: 2 }`。
///
/// # Panics
///
/// Panics if `row` or `col` is not positive.
#[must_use]
pub fn grid(row: i32, col: i32) -> Grid {
    match Grid::from_one_based(row, col) {
        Ok(grid) => grid,
        Err(_error) => {
            panic!("script-facing grid coordinates must be 1-based positive values")
        }
    }
}

/// 从脚本 `1-based` 目标行和允许小数的落点列构造 [`CobTarget`]。
///
/// # Panics
///
/// Panics if `row` is not positive or `drop_col` is not finite.
#[must_use]
pub fn target(row: i32, drop_col: f32) -> CobTarget {
    assert!(
        drop_col.is_finite(),
        "script-facing cob target drop columns must be finite values"
    );
    match CobTarget::from_one_based_row(row, drop_col) {
        Ok(target) => target,
        Err(_error) => panic!("script-facing cob target rows must be 1-based positive values"),
    }
}
