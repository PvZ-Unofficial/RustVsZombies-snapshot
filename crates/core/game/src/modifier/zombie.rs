//! Direct zombie state modifier helpers.

use crate::backend::{ZombieBodyHealthWriteBackend, ZombieCreateBackend, ZombieReadBackend, ZombieXWriteBackend};
use crate::model::{Grid, I32RepresentableF32, ObjectEditOutcome, PositiveHp, ZombieId, ZombieKind};

use super::value::ModifierValueError;

/// Failure from the backend-neutral user-facing zombie placement composition.
#[derive(Debug, thiserror::Error)]
pub enum SpawnZombieError {
    #[error("zombie placement rejected: {0}")]
    Rejected(crate::runtime::RuntimeError),
}

/// Places one zombie at a grid cell and returns the newly allocated cross-frame ID.
pub fn spawn_zombie(kind: ZombieKind, grid: Grid) -> Result<ZombieId, SpawnZombieError>
where
    rsvz_current::CurrentBackend: ZombieCreateBackend,
{
    crate::access::with_backend(|backend| {
        let zombie = backend
            .place_zombie(kind, grid)
            .map_err(|error| SpawnZombieError::Rejected(crate::access::operation_error(error)))?;
        Ok(backend.zombie_id(zombie))
    })
}

/// Sets main body health for a live zombie ID.
pub fn set_zombie_body_hp(id: ZombieId, hp: i32) -> Result<ObjectEditOutcome, ModifierValueError>
where
    rsvz_current::CurrentBackend: ZombieReadBackend + ZombieBodyHealthWriteBackend,
{
    crate::access::with_backend(|backend| {
        let Some(handle) = crate::live_value::read_or_abort(backend.zombie(id), "zombie") else {
            return Ok(ObjectEditOutcome::Missing);
        };
        let hp = PositiveHp::new(hp).map_err(ModifierValueError::InvalidValue)?;
        Ok(backend
            .set_zombie_body_hp(handle, hp)
            .map_err(|error| ModifierValueError::Rejected(crate::access::operation_error(error)))?)
    })
}

/// Sets main body health for live zombies of `kind` and returns the number of applied edits.
pub fn set_zombie_body_hp_by_kind(kind: ZombieKind, hp: i32) -> Result<usize, ModifierValueError>
where
    rsvz_current::CurrentBackend: ZombieReadBackend + ZombieBodyHealthWriteBackend,
{
    crate::access::with_backend(|backend| {
        let hp = PositiveHp::new(hp).map_err(ModifierValueError::InvalidValue)?;
        let mut applied = 0;
        for zombie in crate::live_value::read_or_abort(backend.zombies(), "zombies") {
            if backend
                .zombie_kind(zombie)
                .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()))
                == kind
                && backend
                    .set_zombie_body_hp(zombie, hp)
                    .map_err(|error| ModifierValueError::Rejected(crate::access::operation_error(error)))?
                    == ObjectEditOutcome::Applied
            {
                applied += 1;
            }
        }
        Ok(applied)
    })
}

/// Sets main body health for every live zombie and returns the number of applied edits.
pub fn set_all_zombies_body_hp(hp: i32) -> Result<usize, ModifierValueError>
where
    rsvz_current::CurrentBackend: ZombieReadBackend + ZombieBodyHealthWriteBackend,
{
    crate::access::with_backend(|backend| {
        let hp = PositiveHp::new(hp).map_err(ModifierValueError::InvalidValue)?;
        let mut applied = 0;
        for zombie in crate::live_value::read_or_abort(backend.zombies(), "zombies") {
            if backend
                .set_zombie_body_hp(zombie, hp)
                .map_err(|error| ModifierValueError::Rejected(crate::access::operation_error(error)))?
                == ObjectEditOutcome::Applied
            {
                applied += 1;
            }
        }
        Ok(applied)
    })
}

/// Sets the horizontal coordinate for a live zombie ID.
pub fn set_zombie_x(id: ZombieId, x: f32) -> Result<ObjectEditOutcome, ModifierValueError>
where
    rsvz_current::CurrentBackend: ZombieReadBackend + ZombieXWriteBackend,
{
    crate::access::with_backend(|backend| {
        let x = I32RepresentableF32::new(x).map_err(ModifierValueError::InvalidValue)?;
        let Some(handle) = crate::live_value::read_or_abort(backend.zombie(id), "zombie") else {
            return Ok(ObjectEditOutcome::Missing);
        };
        Ok(backend
            .set_zombie_x(handle, x)
            .map_err(|error| ModifierValueError::Rejected(crate::access::operation_error(error)))?)
    })
}
