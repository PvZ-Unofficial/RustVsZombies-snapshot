//! 与 backend 无关的语义铲除操作。
//!
//! 这些 helper 读取植物事实并直接删除选中植物 handle，从不移动鼠标或模拟
//! 铲子点击，同时保留原生铲除可观察的目标层语义。core [`Grid`] 从
//! `0` 开始；接受独立 `row, col` 整数的便捷函数使用脚本侧 `1-based` 坐标。

use std::fmt::{Display, Write as _};

use crate::backend::PlantReadBackend;
use crate::backend::{PlantCreateBackend, PlantRemoveBackend};
use crate::logic::cards::plant_counts_as_on_lawn;
use crate::logic::grid::IntoGrid;
use crate::model::PlantId;
use crate::model::{CardSelection, Grid, GridError, PlantKind};
use crate::runtime::{RuntimeError, RuntimeResult};

pub trait ShovelContext: PlantCreateBackend + PlantRemoveBackend {}

impl<T> ShovelContext for T where T: PlantCreateBackend + PlantRemoveBackend {}

/// 选择语义铲除要删除的植物层。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShovelTarget {
    /// 使用中心铲除优先级：主体、南瓜、荷叶/花盆、咖啡豆。
    #[default]
    Default,
    /// 只删除指定普通/模仿者卡片谱系，不回退到其他层。
    Plant(CardSelection),
}

/// 转换明确铲除目标或其脚本简写。
///
/// [`PlantKind`] 选择普通卡片谱系，[`CardSelection`] 还可选择模仿者；`true`
/// 选择南瓜，`false` 选择 [`ShovelTarget::Default`]。
pub trait IntoShovelTarget {
    /// 转换为明确的语义铲除目标。
    fn into_shovel_target(self) -> ShovelTarget;
}

impl IntoShovelTarget for ShovelTarget {
    fn into_shovel_target(self) -> ShovelTarget {
        self
    }
}

impl IntoShovelTarget for PlantKind {
    fn into_shovel_target(self) -> ShovelTarget {
        ShovelTarget::Plant(CardSelection::Plant(self))
    }
}

impl IntoShovelTarget for CardSelection {
    fn into_shovel_target(self) -> ShovelTarget {
        ShovelTarget::Plant(self)
    }
}

impl IntoShovelTarget for bool {
    fn into_shovel_target(self) -> ShovelTarget {
        if self {
            PlantKind::Pumpkin.into_shovel_target()
        } else {
            ShovelTarget::Default
        }
    }
}

/// core `0-based` 格上的一项语义铲除操作。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShovelOp {
    /// 要操作的 core `0-based` 格。
    pub grid: Grid,
    /// 要删除的植物层。
    pub target: ShovelTarget,
}

/// 把类型化或脚本侧值转换为 [`ShovelOp`]。
///
/// [`Grid`] 已经是 `0-based`；`(row, col)` 与 `(row, col, target)` 元组使用
/// `1-based` 脚本坐标，`(Grid, target)` 保留类型化格。
pub trait IntoShovelOp {
    /// 尝试转换；脚本坐标无效时返回 [`GridError`]。
    fn try_into_shovel_op(self) -> Result<ShovelOp, GridError>;
}

impl IntoShovelOp for ShovelOp {
    fn try_into_shovel_op(self) -> Result<ShovelOp, GridError> {
        Ok(self)
    }
}

impl IntoShovelOp for Grid {
    fn try_into_shovel_op(self) -> Result<ShovelOp, GridError> {
        Ok(ShovelOp {
            grid: self,
            target: ShovelTarget::Default,
        })
    }
}

impl IntoShovelOp for (i32, i32) {
    fn try_into_shovel_op(self) -> Result<ShovelOp, GridError> {
        Ok(ShovelOp {
            grid: self.try_into_grid()?,
            target: ShovelTarget::Default,
        })
    }
}

impl<T> IntoShovelOp for (Grid, T)
where
    T: IntoShovelTarget,
{
    fn try_into_shovel_op(self) -> Result<ShovelOp, GridError> {
        Ok(ShovelOp {
            grid: self.0,
            target: self.1.into_shovel_target(),
        })
    }
}

impl<T> IntoShovelOp for (i32, i32, T)
where
    T: IntoShovelTarget,
{
    fn try_into_shovel_op(self) -> Result<ShovelOp, GridError> {
        Ok(ShovelOp {
            grid: Grid::from_one_based(self.0, self.1)?,
            target: self.2.into_shovel_target(),
        })
    }
}

/// 从一个 core 格删除默认中心铲除目标。
///
/// 选择顺序为主体、南瓜、荷叶/花盆、咖啡豆，不选择墓碑吞噬者。没有目标时
/// 是成功空操作；删除受南瓜保护的香蒲时补回荷叶，以保留原生铲子行为。
pub fn shovel_at(grid: Grid) -> Result<(), rsvz_current::CurrentBackendError>
where
    rsvz_current::CurrentBackend: ShovelContext,
{
    shovel_target_at(grid, ShovelTarget::Default)
}

/// 不移动/点击鼠标，删除一个明确语义目标。
///
/// 通过植物 raw kind 区分普通与模仿者谱系。指定目标不存在时不删除其他层，
/// 并返回 `Ok(())`。
pub fn shovel_target_at(grid: Grid, target: ShovelTarget) -> Result<(), rsvz_current::CurrentBackendError>
where
    rsvz_current::CurrentBackend: ShovelContext,
{
    crate::access::with_backend(|backend| {
        let mut selected = None;
        let mut pumpkin = None;
        let mut under = None;
        let mut coffee = None;
        for plant in crate::live_value::read_or_abort(backend.plants(), "plants") {
            if !plant_counts_as_on_lawn(backend, plant)? {
                continue;
            }
            let view = crate::plant::view_from_handle(backend, plant);
            if !view.occupies_grid(grid) {
                continue;
            }
            if view.kind == PlantKind::Pumpkin {
                pumpkin.get_or_insert(view.id);
            }
            if target == ShovelTarget::Default {
                match view.kind {
                    PlantKind::CoffeeBean => {
                        coffee.get_or_insert(view.id);
                    }
                    PlantKind::LilyPad | PlantKind::FlowerPot => {
                        under.get_or_insert(view.id);
                    }
                    PlantKind::Pumpkin | PlantKind::GraveBuster => {}
                    _ => {
                        selected.get_or_insert(view.id);
                    }
                }
            } else if let ShovelTarget::Plant(selection) = target
                && selected.is_none()
            {
                let target_kind = match selection {
                    CardSelection::Plant(kind) | CardSelection::Imitator(kind) => kind,
                };
                if view.kind != target_kind {
                    continue;
                }
                let raw = crate::live_value::read_or_abort(backend.plant_raw_kind(plant), "plant_raw_kind");
                let matches = match selection {
                    CardSelection::Plant(_) => raw != PlantKind::Imitator,
                    CardSelection::Imitator(_) => raw == PlantKind::Imitator,
                };
                if matches {
                    selected = Some(view.id);
                }
            }
        }
        if target == ShovelTarget::Default {
            selected = selected.or(pumpkin).or(under).or(coffee);
        }
        remove_shovel_target(grid, selected, pumpkin)
    })
}

fn remove_shovel_target(
    grid: Grid, target: Option<PlantId>, pumpkin: Option<PlantId>,
) -> Result<(), rsvz_current::CurrentBackendError>
where
    rsvz_current::CurrentBackend: ShovelContext,
{
    crate::access::with_backend(|backend| {
        let Some(target) = target else {
            return Ok(());
        };
        let Some(handle) = crate::live_value::read_or_abort(backend.plant(target), "plant") else {
            return Ok(());
        };
        let kind = crate::live_value::read_or_abort(backend.plant_kind(handle), "plant_kind");
        backend.remove_plant(handle).map_err(crate::access::rejected_error)?;

        if kind == PlantKind::Cattail && pumpkin.is_some() {
            let lily = CardSelection::Plant(PlantKind::LilyPad)
                .checked()
                .expect("lily pad is a valid ordinary card selection");
            let _lily = crate::live_value::read_or_abort(backend.new_plant(lily, grid), "new_plant");
        }
        Ok(())
    })
}

/// 脚本风格铲除 helper 返回的错误。
#[derive(Debug, thiserror::Error)]
pub enum ShovelError {
    #[error("invalid shovel grid")]
    InvalidGrid,
    #[error("backend shovel operation failed: {0}")]
    Backend(crate::runtime::RuntimeError),
}

fn push_shovel_error(errors: &mut Option<String>, index: usize, error: impl Display) {
    let message = errors.get_or_insert_default();
    if !message.is_empty() {
        message.push('\n');
    }
    if write!(message, "shovel operation {} failed: {error}", index + 1).is_err() {
        unreachable!("writing to a String cannot fail");
    }
}

/// 删除脚本侧 `1-based` 行列的默认目标。
pub fn shovel_one_based(row: i32, col: i32) -> Result<(), ShovelError>
where
    rsvz_current::CurrentBackend: ShovelContext,
{
    let grid = Grid::from_one_based(row, col).map_err(|_error| ShovelError::InvalidGrid)?;
    shovel_at(grid).map_err(|error| ShovelError::Backend(error.into()))
}

/// 删除一个脚本侧 `1-based` 格的默认目标。
///
/// 无效坐标和 backend 故障变成扁平 [`RuntimeError`]；空格返回 `Ok(())`。
pub fn shovel(row: i32, col: i32) -> RuntimeResult<()>
where
    rsvz_current::CurrentBackend: ShovelContext,
{
    shovel_one_based(row, col).map_err(|error| RuntimeError::new(error.to_string()))
}

/// 删除一个脚本侧 `1-based` 格的明确目标。
///
/// 目标通过 [`IntoShovelTarget`] 选择。目标不存在是成功空操作；无效坐标和
/// backend 故障变成扁平 runtime 错误。
pub fn shovel_target(row: i32, col: i32, target: impl IntoShovelTarget) -> RuntimeResult<()>
where
    rsvz_current::CurrentBackend: ShovelContext,
{
    {
        let grid = Grid::from_one_based(row, col).map_err(|_error| RuntimeError::new("invalid shovel grid"))?;
        shovel_target_at(grid, target.into_shovel_target())
            .map_err(|error| RuntimeError::new(format!("backend shovel operation failed: {error}")))
    }
}

/// 按输入顺序删除多个格的默认目标。
///
/// 单项无效格被汇总且后续操作继续；backend 故障被汇总并停止后续访问。
pub fn shovel_many<I, G>(grids: I) -> RuntimeResult<()>
where
    rsvz_current::CurrentBackend: ShovelContext,
    I: IntoIterator<Item = G>,
    G: IntoGrid,
{
    {
        let mut errors = None;
        for (index, grid) in grids.into_iter().enumerate() {
            let grid = match grid.try_into_grid() {
                Ok(grid) => grid,
                Err(_error) => {
                    push_shovel_error(&mut errors, index, "invalid shovel grid");
                    continue;
                }
            };
            if let Err(error) = shovel_at(grid) {
                push_shovel_error(
                    &mut errors,
                    index,
                    format_args!("backend shovel operation failed: {error}"),
                );
                break;
            }
        }
        errors.map_or_else(|| Ok(()), |message| Err(RuntimeError::new(message)))
    }
}

/// 按输入顺序执行语义铲除操作。
///
/// 元组输入从 `1-based` 脚本坐标转换。无效项被汇总，后续有效操作继续；
/// backend 故障被汇总并停止后续访问，此前铲除保留。
pub fn shovel_ops<I, O>(ops: I) -> RuntimeResult<()>
where
    rsvz_current::CurrentBackend: ShovelContext,
    I: IntoIterator<Item = O>,
    O: IntoShovelOp,
{
    {
        let mut errors = None;
        for (index, op) in ops.into_iter().enumerate() {
            let op = match op.try_into_shovel_op() {
                Ok(op) => op,
                Err(_error) => {
                    push_shovel_error(&mut errors, index, "invalid shovel grid");
                    continue;
                }
            };
            if let Err(error) = shovel_target_at(op.grid, op.target) {
                push_shovel_error(
                    &mut errors,
                    index,
                    format_args!("backend shovel operation failed: {error}"),
                );
                break;
            }
        }
        errors.map_or_else(|| Ok(()), |message| Err(RuntimeError::new(message)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inputs_preserve_coordinates_and_explicit_target_lineage() {
        let op = (1, 2).try_into_shovel_op().unwrap();
        assert_eq!(op.grid, Grid { row: 0, col: 1 });
        assert_eq!(op.target, ShovelTarget::Default);
        let selection = CardSelection::Imitator(PlantKind::IceShroom);
        assert_eq!(
            (1, 2, selection).try_into_shovel_op().unwrap().target,
            ShovelTarget::Plant(selection)
        );
        assert!((0, 1).try_into_shovel_op().is_err());
    }
}
