//! Direct plant state modifier helpers.

use crate::backend::{PlantEffectCountdownWriteBackend, PlantHealthWriteBackend, PlantReadBackend, PlantRemoveBackend};
use crate::model::{Grid, NonNegativeI32, ObjectEditOutcome, PositiveHp};
use crate::model::{PlantId, PlantKind};

use super::value::ModifierValueError;

const PLANT_STATE_DOING_SPECIAL: i32 = 2;

#[derive(Debug, thiserror::Error)]
pub enum PlantEffectCountdownError {
    #[error("effect countdown normalization supports IceShroom and DoomShroom")]
    UnsupportedKind,
    #[error("effect countdown target must be non-negative")]
    NegativeTarget,
    #[error("effect plant not found: {0:?}")]
    PlantNotFound(PlantKind),
    #[error("effect plant is not active: {0:?}")]
    PlantNotActive(PlantId),
}

/// Sets current health for a live plant ID.
pub fn set_plant_hp(id: PlantId, hp: i32) -> Result<ObjectEditOutcome, ModifierValueError>
where
    rsvz_current::CurrentBackend: PlantReadBackend + PlantHealthWriteBackend,
{
    crate::access::with_backend(|backend| {
        let Some(handle) = crate::live_value::read_or_abort(backend.plant(id), "plant") else {
            return Ok(ObjectEditOutcome::Missing);
        };
        let hp = PositiveHp::new(hp).map_err(ModifierValueError::InvalidValue)?;
        Ok(backend
            .set_plant_hp(handle, hp)
            .map_err(|error| ModifierValueError::Rejected(crate::access::operation_error(error)))?)
    })
}

/// Sets current health for the first live plant at `grid`.
pub fn set_plant_hp_by_grid(grid: Grid, hp: i32) -> Result<ObjectEditOutcome, ModifierValueError>
where
    rsvz_current::CurrentBackend: PlantReadBackend + PlantHealthWriteBackend,
{
    crate::access::with_backend(|backend| {
        let hp = PositiveHp::new(hp).map_err(ModifierValueError::InvalidValue)?;
        for plant in crate::live_value::read_or_abort(backend.plants(), "plants") {
            if crate::plant::grid_from_handle(backend, plant) == grid {
                return Ok(backend
                    .set_plant_hp(plant, hp)
                    .map_err(|error| ModifierValueError::Rejected(crate::access::operation_error(error)))?);
            }
        }
        Ok(ObjectEditOutcome::Missing)
    })
}

/// Sets current health for every live plant and returns the number of applied edits.
pub fn set_all_plants_hp(hp: i32) -> Result<usize, ModifierValueError>
where
    rsvz_current::CurrentBackend: PlantReadBackend + PlantHealthWriteBackend,
{
    crate::access::with_backend(|backend| {
        let hp = PositiveHp::new(hp).map_err(ModifierValueError::InvalidValue)?;
        let mut applied = 0;
        for plant in crate::live_value::read_or_abort(backend.plants(), "plants") {
            if backend
                .set_plant_hp(plant, hp)
                .map_err(|error| ModifierValueError::Rejected(crate::access::operation_error(error)))?
                == ObjectEditOutcome::Applied
            {
                applied += 1;
            }
        }
        Ok(applied)
    })
}

/// Removes a live plant by ID, returning whether the ID still resolved.
pub fn remove_plant_by_id(id: PlantId) -> Result<bool, crate::runtime::RuntimeError>
where
    rsvz_current::CurrentBackend: PlantRemoveBackend,
{
    crate::access::with_backend(|backend| {
        let Some(handle) = crate::live_value::read_or_abort(backend.plant(id), "plant") else {
            return Ok(false);
        };
        backend.remove_plant(handle).map_err(crate::access::operation_error)?;
        Ok(true)
    })
}

/// Removes the first live plant of `kind` at `grid`.
pub fn remove_plant_kind_at(kind: PlantKind, grid: Grid) -> Result<Option<PlantId>, crate::runtime::RuntimeError>
where
    rsvz_current::CurrentBackend: PlantRemoveBackend,
{
    crate::access::with_backend(|backend| {
        let mut found = None;
        crate::live_value::read_or_abort(
            backend.for_each_plant_at_grid(grid, |plant| {
                if crate::live_value::read_or_abort(backend.plant_kind(plant), "plant_kind") == kind && found.is_none()
                {
                    found = Some(backend.plant_id(plant));
                }
                Ok(())
            }),
            "failed to find plant at grid",
        );
        if let Some(id) = found
            && let Some(handle) = crate::live_value::read_or_abort(backend.plant(id), "plant")
        {
            backend.remove_plant(handle).map_err(crate::access::operation_error)?;
            return Ok(Some(id));
        }
        Ok(None)
    })
}

/// Sets the first active ice/ash plant effect countdown to `target_countdown`.
pub fn normalize_first_effect_countdown(
    kind: PlantKind, target_countdown: i32,
) -> Result<PlantId, PlantEffectCountdownError>
where
    rsvz_current::CurrentBackend: PlantReadBackend + PlantEffectCountdownWriteBackend,
{
    crate::access::with_backend(|backend| {
        if !matches!(kind, PlantKind::IceShroom | PlantKind::DoomShroom) {
            return Err(PlantEffectCountdownError::UnsupportedKind);
        }
        let target_countdown =
            NonNegativeI32::new(target_countdown).map_err(|_error| PlantEffectCountdownError::NegativeTarget)?;
        for plant in crate::live_value::read_or_abort(backend.plants(), "plants") {
            let plant_kind = backend
                .plant_kind(plant)
                .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
            let id = backend.plant_id(plant);
            if plant_kind == kind && backend.plant_state(plant) == PLANT_STATE_DOING_SPECIAL {
                backend
                    .set_plant_effect_countdown(plant, target_countdown)
                    .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
                return Ok(id);
            }
        }
        Err(PlantEffectCountdownError::PlantNotFound(kind))
    })
}

/// Sets the effect countdown for the plant of `kind` at one exact grid.
pub fn normalize_effect_countdown_by_grid(
    kind: PlantKind, grid: Grid, target_countdown: i32,
) -> Result<ObjectEditOutcome, PlantEffectCountdownError>
where
    rsvz_current::CurrentBackend: PlantReadBackend + PlantEffectCountdownWriteBackend,
{
    crate::access::with_backend(|backend| {
        if !matches!(kind, PlantKind::IceShroom | PlantKind::DoomShroom) {
            return Err(PlantEffectCountdownError::UnsupportedKind);
        }
        let target_countdown =
            NonNegativeI32::new(target_countdown).map_err(|_error| PlantEffectCountdownError::NegativeTarget)?;
        for plant in crate::live_value::read_or_abort(backend.plants(), "plants") {
            if crate::plant::grid_from_handle(backend, plant) == grid
                && backend
                    .plant_kind(plant)
                    .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()))
                    == kind
            {
                let id = backend.plant_id(plant);
                if backend.plant_state(plant) != PLANT_STATE_DOING_SPECIAL {
                    return Err(PlantEffectCountdownError::PlantNotActive(id));
                }
                return Ok(backend
                    .set_plant_effect_countdown(plant, target_countdown)
                    .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into())));
            }
        }
        Err(PlantEffectCountdownError::PlantNotFound(kind))
    })
}

/// Sets the effect countdown for one exact generation-checked plant.
///
/// Missing IDs are a silent no-op so cleanup/replacement between placement and effect time cannot
/// redirect the edit to another plant of the same kind.
pub fn normalize_effect_countdown_by_id(
    id: PlantId, target_countdown: i32,
) -> Result<ObjectEditOutcome, PlantEffectCountdownError>
where
    rsvz_current::CurrentBackend: PlantReadBackend + PlantEffectCountdownWriteBackend,
{
    crate::access::with_backend(|backend| {
        let target_countdown =
            NonNegativeI32::new(target_countdown).map_err(|_error| PlantEffectCountdownError::NegativeTarget)?;
        let Some(plant) = crate::live_value::read_or_abort(backend.plant(id), "plant") else {
            return Ok(ObjectEditOutcome::Missing);
        };
        let kind = backend
            .plant_kind(plant)
            .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
        if !matches!(kind, PlantKind::IceShroom | PlantKind::DoomShroom) {
            return Err(PlantEffectCountdownError::UnsupportedKind);
        }
        if backend.plant_state(plant) != PLANT_STATE_DOING_SPECIAL {
            return Err(PlantEffectCountdownError::PlantNotActive(id));
        }
        Ok(backend
            .set_plant_effect_countdown(plant, target_countdown)
            .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into())))
    })
}
