use crate::logic::cards::{CardPlantingBackend, PlantSeedOutcome, find_seed_slot, try_plant_seed};
use crate::model::{CardSelection, Grid, PlantKind};

use super::find_plant_at_kind;

/// 在 core `0-based` 炮位推进一次补炮状态机。
///
/// 返回 `true` 表示炮已经补完；缺卡、冷却或落点暂不可用时返回 `false`，供
/// 调用者下个 tick 再试。必要读取或访问故障结束当前回调。
pub fn try_plant_cob(grid: Grid) -> bool
where
    rsvz_current::CurrentBackend: CardPlantingBackend,
{
    let Some(right_col) = grid.col.checked_add(1) else {
        return false;
    };
    let cob = CardSelection::Plant(PlantKind::CobCannon);
    let kernel = CardSelection::Plant(PlantKind::KernelPult);

    let Some(cob_slot) = find_seed_slot(cob) else {
        return false;
    };
    let Some(kernel_slot) = find_seed_slot(kernel) else {
        return false;
    };

    for kernel_grid in [
        grid,
        Grid {
            row: grid.row,
            col: right_col,
        },
    ] {
        if find_plant_at_kind(kernel_grid, PlantKind::KernelPult).is_some() {
            continue;
        }
        if !matches!(try_plant_seed(kernel_slot, kernel_grid), PlantSeedOutcome::Planted(_)) {
            return false;
        }
    }

    matches!(try_plant_seed(cob_slot, grid), PlantSeedOutcome::Planted(_))
}
