//! 玉米加农炮目标值。

use crate::model::GridError;

/// core 使用的逻辑炮落点。
///
/// [`CobTarget::row`] 从 `0` 开始；[`CobTarget::drop_col`] 保留 PvZ 脚本的
/// 落点列语义并允许小数。普通脚本应传 `(1-based row, drop_col)` 元组或使用
/// [`CobTarget::from_one_based_row`]，而不是直接填写字段。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CobTarget {
    /// 从 `0` 开始的 core 目标行。
    pub row: i32,
    /// 允许小数的 PvZ 脚本落点列。
    pub drop_col: f32,
}

impl CobTarget {
    /// 把用户侧 `1-based` 目标行和脚本落点列转换为 core 炮落点。
    pub const fn from_one_based_row(row: i32, drop_col: f32) -> Result<Self, GridError> {
        if row <= 0 || !drop_col.is_finite() {
            return Err(GridError::NegativeCoordinate);
        }
        Ok(Self { row: row - 1, drop_col })
    }

    /// 转换为用户侧 `(1-based row, drop_col)`。
    #[must_use]
    pub const fn to_one_based_row(self) -> (i32, f32) {
        (self.row + 1, self.drop_col)
    }
}
