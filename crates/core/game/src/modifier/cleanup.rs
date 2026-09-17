//! Board-wide cleanup helpers built from fine-grained edit capabilities.

use crate::backend::{PlantReadBackend, PlantRemoveBackend, ZombieKillBackend, ZombieReadBackend, ZombieRemoveBackend};

/// Removes all currently live plants through the P10 atom.
pub fn clear_plants() -> Result<usize, crate::runtime::RuntimeError>
where
    rsvz_current::CurrentBackend: PlantRemoveBackend,
{
    crate::access::with_backend(|backend| {
        let mut removed = 0;
        for plant in crate::live_value::read_or_abort(backend.plants(), "plants") {
            backend.remove_plant(plant).map_err(crate::access::operation_error)?;
            removed += 1;
        }
        Ok(removed)
    })
}

/// Removes all currently live zombies without loot through Z11.
pub fn clear_zombies() -> Result<usize, crate::runtime::RuntimeError>
where
    rsvz_current::CurrentBackend: ZombieRemoveBackend,
{
    crate::access::with_backend(|backend| {
        let mut removed = 0;
        for zombie in crate::live_value::read_or_abort(backend.zombies(), "zombies") {
            backend.remove_zombie(zombie).map_err(crate::access::operation_error)?;
            removed += 1;
        }
        Ok(removed)
    })
}

/// Kills all currently live zombies through Z12 when the backend supports loot semantics.
pub fn kill_all_zombies() -> Result<usize, crate::runtime::RuntimeError>
where
    rsvz_current::CurrentBackend: ZombieKillBackend,
{
    crate::access::with_backend(|backend| {
        let mut killed = 0;
        for zombie in crate::live_value::read_or_abort(backend.zombies(), "zombies") {
            backend.kill_zombie(zombie).map_err(crate::access::operation_error)?;
            killed += 1;
        }
        Ok(killed)
    })
}

/// Removes the live grid item with this ID; a missing ID is a successful no-op.
pub fn remove_grid_item_by_id(id: rsvz_model::GridItemId) -> Result<(), crate::runtime::RuntimeError>
where
    rsvz_current::CurrentBackend: crate::backend::GridItemEditBackend,
{
    use crate::backend::{GridItemEditBackend, GridItemReadBackend};
    crate::live_value::read_or_abort(
        rsvz_current::with_backend_shared(|access| {
            let backend = access;
            for item in crate::live_value::read_or_abort(backend.grid_items(), "grid_items") {
                if backend.grid_item_id(item) == id {
                    backend.remove_grid_item(item).map_err(crate::access::operation_error)?;
                    break;
                }
            }
            Ok(())
        })
        .map_err(|error| crate::runtime::RuntimeError::new(error.to_string())),
        "failed to access grid items",
    )
}
