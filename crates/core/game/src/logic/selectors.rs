//! Early target and grid selectors.
//!
//! These helpers are compatibility placeholders. They keep old call sites compiling while more
//! precise selectors live in the domain-specific logic that needs them.

use crate::backend::{SceneBackend, ZombieReadBackend};
use crate::logic::zombies::is_flying_threat;
use crate::model::Grid;
use crate::model::{PlantKind, ZombieId};

/// Returns in-bounds field grids for `kind`.
///
/// This placeholder only filters by board bounds; callers that need real tactical safety should
/// layer stronger checks on top.
pub fn safe_grids_for_plant(_kind: PlantKind) -> Result<Vec<Grid>, crate::runtime::RuntimeError>
where
    rsvz_current::CurrentBackend: SceneBackend,
{
    crate::access::with_backend(|backend| {
        let field = rsvz_model::FieldInfo::from_scene(crate::live_value::read_or_abort(backend.scene(), "scene"));
        let mut grids = Vec::new();
        for row in 0..field.row_count {
            for col in 0..field.col_count {
                grids.push(Grid {
                    row: row as i32,
                    col: col as i32,
                });
            }
        }
        Ok(grids)
    })
}

/// Returns the first flying/drop threat in backend iteration order.
pub fn nearest_threat() -> Result<Option<ZombieId>, crate::runtime::RuntimeError>
where
    rsvz_current::CurrentBackend: ZombieReadBackend,
{
    crate::access::with_backend(|backend| {
        for zombie in crate::live_value::read_or_abort(backend.zombies(), "zombies") {
            if is_flying_threat(crate::live_value::read_or_abort(
                backend.zombie_kind(zombie),
                "zombie_kind",
            )) {
                return Ok(Some(backend.zombie_id(zombie)));
            }
        }
        Ok(None)
    })
}

/// Returns the first zombie on `row` in backend iteration order.
pub fn frontmost_zombie_by_row(row: i32) -> Result<Option<ZombieId>, crate::runtime::RuntimeError>
where
    rsvz_current::CurrentBackend: ZombieReadBackend,
{
    crate::access::with_backend(|backend| {
        for zombie in crate::live_value::read_or_abort(backend.zombies(), "zombies") {
            if backend.zombie_row(zombie) == row {
                return Ok(Some(backend.zombie_id(zombie)));
            }
        }
        Ok(None)
    })
}
