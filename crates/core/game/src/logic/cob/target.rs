//! 脚本侧炮输入与 core 目标值之间的转换。
//!
//! 元组行号和炮位从 `1` 开始，类型化 [`Grid`] 是 core `0-based`；类型化
//! [`CobTarget`] 保留 core `0-based` 行与脚本浮点落点列的混合约定。

use crate::logic::grid::IntoGrid;
use crate::model::{CobTarget, Grid, GridError};

use super::CobManagerError;

/// 把脚本元组或类型化目标转换为 core [`CobTarget`]。
///
/// `(row, drop_col)` 使用正的 `1-based` 行和有限的整数/浮点落点列；传入
/// [`CobTarget`] 时会校验并把其行视为已经是 `0-based`。
pub trait IntoCobTarget {
    /// 尝试转换，脚本目标无效时不 panic。
    fn try_into_cob_target(self) -> Result<CobTarget, GridError>;

    /// 转换目标；脚本行/落点列无效时 panic。
    fn into_cob_target(self) -> CobTarget
    where
        Self: Sized,
    {
        match self.try_into_cob_target() {
            Ok(target) => target,
            Err(_error) => {
                panic!("script-facing cob target must use a positive row and finite drop column")
            }
        }
    }
}

/// 一项包含明确 core 炮位和落点的 raw-fire 命令。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CobFireDrop {
    /// 炮尾锚点，core `0-based` 格。
    pub cob_grid: Grid,
    /// core `0-based` 目标行与脚本浮点落点列。
    pub target: CobTarget,
}

/// 把一项脚本侧 raw-fire 输入转换为 [`CobFireDrop`]。
///
/// 支持 `(cob_row, cob_col, drop_row, drop_col)`、
/// `((cob_row, cob_col), (drop_row, drop_col))`、类型化 `(Grid, CobTarget)`
/// 以及混合形式；整数元组从 `1` 开始。
pub trait IntoCobFireDrop {
    /// 尝试转换；任一炮位或落点无效时返回 [`GridError`]。
    fn try_into_cob_fire_drop(self) -> Result<CobFireDrop, GridError>;
}

impl IntoCobFireDrop for CobFireDrop {
    fn try_into_cob_fire_drop(self) -> Result<CobFireDrop, GridError> {
        self.cob_grid.try_into_grid()?;
        self.target.try_into_cob_target()?;
        Ok(self)
    }
}

impl IntoCobFireDrop for (Grid, CobTarget) {
    fn try_into_cob_fire_drop(self) -> Result<CobFireDrop, GridError> {
        Ok(CobFireDrop {
            cob_grid: self.0.try_into_grid()?,
            target: self.1.try_into_cob_target()?,
        })
    }
}

impl IntoCobFireDrop for (Grid, (i32, f32)) {
    fn try_into_cob_fire_drop(self) -> Result<CobFireDrop, GridError> {
        Ok(CobFireDrop {
            cob_grid: self.0.try_into_grid()?,
            target: self.1.try_into_cob_target()?,
        })
    }
}

impl IntoCobFireDrop for (Grid, (i32, i32)) {
    fn try_into_cob_fire_drop(self) -> Result<CobFireDrop, GridError> {
        Ok(CobFireDrop {
            cob_grid: self.0.try_into_grid()?,
            target: self.1.try_into_cob_target()?,
        })
    }
}

impl IntoCobFireDrop for ((i32, i32), (i32, f32)) {
    fn try_into_cob_fire_drop(self) -> Result<CobFireDrop, GridError> {
        Ok(CobFireDrop {
            cob_grid: self.0.try_into_grid()?,
            target: self.1.try_into_cob_target()?,
        })
    }
}

impl IntoCobFireDrop for ((i32, i32), (i32, i32)) {
    fn try_into_cob_fire_drop(self) -> Result<CobFireDrop, GridError> {
        Ok(CobFireDrop {
            cob_grid: self.0.try_into_grid()?,
            target: self.1.try_into_cob_target()?,
        })
    }
}

impl IntoCobFireDrop for (i32, i32, i32, f32) {
    fn try_into_cob_fire_drop(self) -> Result<CobFireDrop, GridError> {
        Ok(CobFireDrop {
            cob_grid: (self.0, self.1).try_into_grid()?,
            target: (self.2, self.3).try_into_cob_target()?,
        })
    }
}

impl IntoCobFireDrop for (i32, i32, i32, i32) {
    fn try_into_cob_fire_drop(self) -> Result<CobFireDrop, GridError> {
        Ok(CobFireDrop {
            cob_grid: (self.0, self.1).try_into_grid()?,
            target: (self.2, self.3).try_into_cob_target()?,
        })
    }
}

/// 转换一项 raw-fire 命令或 `Vec`/数组/slice 批次。
///
/// 转换结果保持输入顺序，便于报告无效项而不丢弃后续普通操作。
pub trait IntoCobFireDrops {
    /// 按输入顺序转换并访问每一项，不为整个批次预先分配结果列表。
    fn try_for_each_cob_fire_drop_result<E>(
        self, f: &mut impl FnMut(Result<CobFireDrop, CobManagerError>) -> Result<(), E>,
    ) -> Result<(), E>;
}

impl IntoCobTarget for CobTarget {
    fn try_into_cob_target(self) -> Result<CobTarget, GridError> {
        if self.row < 0 || !self.drop_col.is_finite() {
            return Err(GridError::NegativeCoordinate);
        }
        Ok(self)
    }
}

impl IntoCobTarget for (i32, f32) {
    fn try_into_cob_target(self) -> Result<CobTarget, GridError> {
        if !self.1.is_finite() {
            return Err(GridError::NegativeCoordinate);
        }
        CobTarget::from_one_based_row(self.0, self.1)
    }
}

impl IntoCobTarget for (i32, i32) {
    fn try_into_cob_target(self) -> Result<CobTarget, GridError> {
        CobTarget::from_one_based_row(self.0, self.1 as f32)
    }
}

/// 把单目标或 `Vec`/数组/slice 批次转换为 core 目标。
///
/// 元组目标行从 `1` 开始，类型化 [`CobTarget`] 行已经是 `0-based`；逐项转换
/// 结果保持输入顺序。
pub trait IntoCobTargets {
    /// 按输入顺序转换并访问每一项，不为整个批次预先分配结果列表。
    fn try_for_each_cob_target_result<E>(
        self, f: &mut impl FnMut(Result<CobTarget, CobManagerError>) -> Result<(), E>,
    ) -> Result<(), E>;
}

macro_rules! impl_single_cob_targets {
    ($($ty:ty),* $(,)?) => {
        $(
            impl IntoCobTargets for $ty {
                fn try_for_each_cob_target_result<E>(
                    self,
                    f: &mut impl FnMut(Result<CobTarget, CobManagerError>) -> Result<(), E>,
                ) -> Result<(), E> {
                    visit_cob_targets([self], f)
                }
            }
        )*
    };
}

impl_single_cob_targets!(CobTarget, (i32, f32), (i32, i32));

impl<T> IntoCobTargets for Vec<T>
where
    T: IntoCobTarget,
{
    fn try_for_each_cob_target_result<E>(
        self, f: &mut impl FnMut(Result<CobTarget, CobManagerError>) -> Result<(), E>,
    ) -> Result<(), E> {
        visit_cob_targets(self, f)
    }
}

impl<T, const N: usize> IntoCobTargets for [T; N]
where
    T: IntoCobTarget,
{
    fn try_for_each_cob_target_result<E>(
        self, f: &mut impl FnMut(Result<CobTarget, CobManagerError>) -> Result<(), E>,
    ) -> Result<(), E> {
        visit_cob_targets(self, f)
    }
}

impl<T> IntoCobTargets for &[T]
where
    T: IntoCobTarget + Copy,
{
    fn try_for_each_cob_target_result<E>(
        self, f: &mut impl FnMut(Result<CobTarget, CobManagerError>) -> Result<(), E>,
    ) -> Result<(), E> {
        visit_cob_targets(self.iter().copied(), f)
    }
}

macro_rules! impl_single_cob_fire_drops {
    ($($ty:ty),* $(,)?) => {
        $(
            impl IntoCobFireDrops for $ty {
                fn try_for_each_cob_fire_drop_result<E>(
                    self,
                    f: &mut impl FnMut(Result<CobFireDrop, CobManagerError>) -> Result<(), E>,
                ) -> Result<(), E> {
                    visit_cob_fire_drops([self], f)
                }
            }
        )*
    };
}

impl_single_cob_fire_drops!(
    CobFireDrop,
    (Grid, CobTarget),
    (Grid, (i32, f32)),
    (Grid, (i32, i32)),
    ((i32, i32), (i32, f32)),
    ((i32, i32), (i32, i32)),
    (i32, i32, i32, f32),
    (i32, i32, i32, i32),
);

impl<T> IntoCobFireDrops for Vec<T>
where
    T: IntoCobFireDrop,
{
    fn try_for_each_cob_fire_drop_result<E>(
        self, f: &mut impl FnMut(Result<CobFireDrop, CobManagerError>) -> Result<(), E>,
    ) -> Result<(), E> {
        visit_cob_fire_drops(self, f)
    }
}

impl<T, const N: usize> IntoCobFireDrops for [T; N]
where
    T: IntoCobFireDrop,
{
    fn try_for_each_cob_fire_drop_result<E>(
        self, f: &mut impl FnMut(Result<CobFireDrop, CobManagerError>) -> Result<(), E>,
    ) -> Result<(), E> {
        visit_cob_fire_drops(self, f)
    }
}

impl<T> IntoCobFireDrops for &[T]
where
    T: IntoCobFireDrop + Copy,
{
    fn try_for_each_cob_fire_drop_result<E>(
        self, f: &mut impl FnMut(Result<CobFireDrop, CobManagerError>) -> Result<(), E>,
    ) -> Result<(), E> {
        visit_cob_fire_drops(self.iter().copied(), f)
    }
}

fn visit_cob_targets<T, E>(
    targets: impl IntoIterator<Item = T>, f: &mut impl FnMut(Result<CobTarget, CobManagerError>) -> Result<(), E>,
) -> Result<(), E>
where
    T: IntoCobTarget,
{
    for target in targets {
        f(target
            .try_into_cob_target()
            .map_err(|_error| CobManagerError::InvalidTarget))?;
    }
    Ok(())
}

fn visit_cob_fire_drops<T, E>(
    drops: impl IntoIterator<Item = T>, f: &mut impl FnMut(Result<CobFireDrop, CobManagerError>) -> Result<(), E>,
) -> Result<(), E>
where
    T: IntoCobFireDrop,
{
    for drop in drops {
        f(drop
            .try_into_cob_fire_drop()
            .map_err(|_error| CobManagerError::InvalidTarget))?;
    }
    Ok(())
}
