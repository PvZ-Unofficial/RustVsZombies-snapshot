//! Core lineup application orchestration.

use crate::backend::{
    BoardSupportBackend, GridItemCreateBackend, LawnMowerClearBackend, PlantCreateBackend,
    PlantEffectCountdownWriteBackend, PlantIdleAnimationBackend, PlantRemoveBackend, PlantSleepBackend,
    PlantStateCountdownWriteBackend, PlantStateWriteBackend, SceneEditBackend,
};
use crate::model::{GridItemKind, SceneKind};
use crate::modifier::cleanup::clear_plants;
use crate::{
    backend::{GridItemEditBackend, GridItemReadBackend, SceneBackend},
    model::{Grid, NonNegativeI32, PlantKind, PositiveFiniteF32},
};

use super::validate::should_apply_mushroom_awake_state;
use super::{Lineup, LineupPlant};

const COB_CANNON_READY_STATE: i32 = 37;
const COB_CANNON_IDLE_FPS: f32 = 12.0;

/// Options controlling lineup application.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineupApplyOptions {
    /// Allow the backend to switch the board to the lineup's scene before applying it.
    pub allow_scene_switch: bool,
}

impl Default for LineupApplyOptions {
    fn default() -> Self {
        Self {
            allow_scene_switch: true,
        }
    }
}

/// Failures surfaced by backend-neutral lineup application.
#[derive(Debug, thiserror::Error)]
pub enum ApplyLineupError {
    #[error("lineup board is unsupported: {0}")]
    UnsupportedBoard(crate::runtime::RuntimeError),
    #[error("lineup scene mismatch: current {current:?}, target {target:?}")]
    SceneMismatch { current: SceneKind, target: SceneKind },
    #[error("lineup backend operation failed: {0}")]
    Backend(crate::runtime::RuntimeError),
    #[error("lineup grid item is unsupported by strict creation atoms: {0:?}")]
    UnsupportedGridItem(GridItemKind),
}

/// Backend capabilities required to apply a lineup to the current board.
pub trait LineupApplyBackend:
    BoardSupportBackend
    + SceneEditBackend
    + PlantCreateBackend
    + PlantRemoveBackend
    + PlantSleepBackend
    + PlantIdleAnimationBackend
    + PlantStateWriteBackend
    + PlantStateCountdownWriteBackend
    + PlantEffectCountdownWriteBackend
    + GridItemCreateBackend
    + LawnMowerClearBackend
{
}

impl<T> LineupApplyBackend for T where
    T: BoardSupportBackend
        + SceneEditBackend
        + PlantCreateBackend
        + PlantRemoveBackend
        + PlantSleepBackend
        + PlantIdleAnimationBackend
        + PlantStateWriteBackend
        + PlantStateCountdownWriteBackend
        + PlantEffectCountdownWriteBackend
        + GridItemCreateBackend
        + LawnMowerClearBackend
{
}

/// Applies a validated lineup through a safe backend capability.
pub fn apply_lineup(lineup: &Lineup, options: LineupApplyOptions) -> Result<(), ApplyLineupError>
where
    rsvz_current::CurrentBackend: LineupApplyBackend,
{
    let scene = lineup.scene();
    let current = crate::access::with_backend(|backend| -> Result<_, ApplyLineupError> {
        backend
            .ensure_board_supported()
            .map_err(|error| ApplyLineupError::UnsupportedBoard(crate::access::operation_error(error)))?;
        Ok(crate::live_value::read_or_abort(backend.scene(), "scene"))
    })?;
    let scene_switched = current != scene;
    if scene_switched && !options.allow_scene_switch {
        return Err(ApplyLineupError::SceneMismatch { current, target: scene });
    }
    if lineup.rake_grid().is_some() {
        return Err(ApplyLineupError::UnsupportedGridItem(GridItemKind::Rake));
    }
    if scene_switched {
        crate::with_backend(|backend| backend.set_scene(scene))
            .map_err(|error| ApplyLineupError::Backend(crate::access::operation_error(error)))?;
    }
    clear_existing_board_content()?;

    for (grid, cell) in lineup.iter() {
        if let Some(selection) = cell.base.plant_selection() {
            apply_plant(grid, LineupPlant::plain(selection), scene)?;
        }
    }
    for (grid, cell) in lineup.iter() {
        if let Some(plant) = cell.main {
            apply_plant(grid, plant, scene)?;
        }
    }
    for (grid, cell) in lineup.iter() {
        if let Some(plant) = cell.pumpkin {
            apply_plant(grid, plant, scene)?;
        }
    }
    for (grid, cell) in lineup.iter() {
        if let Some(plant) = cell.coffee {
            apply_plant(grid, plant, scene)?;
        }
    }
    for (grid, cell) in lineup.iter() {
        if cell.base.is_grave() {
            crate::access::with_backend(|backend| backend.add_gravestone(grid).map(|_| ()))
                .map_err(|error| ApplyLineupError::Backend(crate::access::operation_error(error)))?;
        }
    }
    for (grid, cell) in lineup.iter() {
        if cell.ladder {
            crate::access::with_backend(|backend| backend.add_ladder(grid).map(|_| ()))
                .map_err(|error| ApplyLineupError::Backend(crate::access::operation_error(error)))?;
        }
    }

    Ok(())
}

fn apply_plant(grid: Grid, plant: LineupPlant, scene: SceneKind) -> Result<(), ApplyLineupError>
where
    rsvz_current::CurrentBackend: PlantCreateBackend
        + PlantSleepBackend
        + PlantIdleAnimationBackend
        + PlantStateWriteBackend
        + PlantStateCountdownWriteBackend
        + PlantEffectCountdownWriteBackend,
{
    crate::access::with_backend(|backend| {
        let selection = plant.selection.checked().expect("validated lineup selection");
        let handle = backend
            .new_plant(selection, grid)
            .map_err(|error| ApplyLineupError::Backend(crate::access::operation_error(error)))?;
        // PT/PTK finish imitation during placement, before sleep/growth state
        // and before ladders are installed. A lineup is not a newly played card.
        let handle = if selection.imitator_target().is_some() {
            backend
                .morph_imitator(handle)
                .map_err(|error| ApplyLineupError::Backend(crate::access::operation_error(error)))?
        } else {
            handle
        };
        if let Some(awake) = plant.awake.filter(|_awake| should_apply_mushroom_awake_state(scene)) {
            backend
                .set_plant_sleeping(handle, !awake)
                .map_err(|error| ApplyLineupError::Backend(crate::access::operation_error(error)))?;
        }
        if plant.grown {
            backend
                .set_plant_state_countdown(handle, 1)
                .map_err(|error| ApplyLineupError::Backend(crate::access::operation_error(error)))?;
        }
        if selection.effective_kind() == PlantKind::CobCannon {
            backend
                .set_plant_state(handle, COB_CANNON_READY_STATE)
                .map_err(|error| ApplyLineupError::Backend(crate::access::operation_error(error)))?;
            backend
                .set_plant_state_countdown(handle, 0)
                .map_err(|error| ApplyLineupError::Backend(crate::access::operation_error(error)))?;
            backend
                .set_plant_effect_countdown(
                    handle,
                    NonNegativeI32::new(0).expect("zero is a valid effect countdown"),
                )
                .map_err(|error| ApplyLineupError::Backend(crate::access::operation_error(error)))?;
            backend
                .play_plant_idle_animation(
                    handle,
                    PositiveFiniteF32::new(COB_CANNON_IDLE_FPS).expect("cob idle FPS is positive and finite"),
                )
                .map_err(|error| ApplyLineupError::Backend(crate::access::operation_error(error)))?;
        }
        Ok(())
    })
}

fn clear_existing_board_content() -> Result<(), ApplyLineupError>
where
    rsvz_current::CurrentBackend: PlantRemoveBackend + GridItemEditBackend + LawnMowerClearBackend,
{
    crate::access::with_backend(|backend| {
        backend
            .clear_lawn_mowers()
            .map_err(|error| ApplyLineupError::Backend(crate::access::operation_error(error)))?;
        clear_plants().map_err(ApplyLineupError::Backend)?;
        for item in crate::live_value::read_or_abort(backend.grid_items(), "grid_items") {
            backend
                .remove_grid_item(item)
                .map_err(|error| ApplyLineupError::Backend(crate::access::operation_error(error)))?;
        }
        Ok(())
    })
}
