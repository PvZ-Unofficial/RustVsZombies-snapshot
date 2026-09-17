//! Backend-neutral plant runtime proxy helpers.

use rsvz_current::CurrentBackend;

use crate::script::ScriptError;
use rsvz_backend_api::backend::{PlantReadBackend, PlantRemoveBackend};
use rsvz_model::PlantId;
use rsvz_model::{Grid, GridPlantView, PlantKind};

use crate::live_value::LiveValue;
use crate::live_value::{live_or_missing, read_or_abort};

/// Id-only plant handle that re-resolves the object for every read or action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Plant {
    id: PlantId,
}

impl Plant {
    /// Creates a handle from a cross-frame plant ID.
    #[must_use]
    pub const fn from_id(id: PlantId) -> Self {
        Self { id }
    }

    /// Returns this handle's cross-frame plant ID.
    #[must_use]
    pub const fn id(&self) -> PlantId {
        self.id
    }
}

impl Plant
where
    CurrentBackend: PlantReadBackend,
{
    /// Checks whether this plant ID currently resolves to a live object.
    pub fn is_alive(&self) -> bool {
        read_or_abort(
            rsvz_current::with_backend_shared(|backend| read_or_abort(backend.plant(self.id), "plant").is_some())
                .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))
                .map_err(ScriptError::Backend),
            "failed to resolve plant id",
        )
    }

    /// Reads HP as a convenience live value.
    ///
    /// Stale or missing IDs produce `LiveValue::missing()`.
    ///
    /// A backend read failure ends the current callback through its error boundary.
    #[must_use]
    pub fn hp(&self) -> LiveValue<i32> {
        live_or_missing(
            rsvz_current::with_backend_shared(|backend| {
                read_or_abort(backend.plant(self.id), "plant").map(|plant| backend.plant_hp(plant))
            })
            .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))
            .map_err(ScriptError::Backend),
            "failed to read plant hp",
        )
    }

    /// Reads grid as a convenience live value.
    ///
    /// Stale or missing IDs produce `LiveValue::missing()`.
    ///
    /// A backend read failure ends the current callback through its error boundary.
    #[must_use]
    pub fn grid(&self) -> LiveValue<Grid> {
        live_or_missing(
            rsvz_current::with_backend_shared(|backend| {
                read_or_abort(backend.plant(self.id), "plant")
                    .map(|plant| crate::plant::grid_from_handle(backend, plant))
            })
            .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))
            .map_err(ScriptError::Backend),
            "failed to read plant grid",
        )
    }

    /// Reads kind as a convenience live value.
    ///
    /// Stale or missing IDs produce `LiveValue::missing()`.
    ///
    /// A backend read failure ends the current callback through its error boundary.
    #[must_use]
    pub fn kind(&self) -> LiveValue<PlantKind> {
        live_or_missing(
            rsvz_current::with_backend_shared(|backend| {
                read_or_abort(backend.plant(self.id), "plant")
                    .map(|plant| read_or_abort(backend.plant_kind(plant), "plant_kind"))
            })
            .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))
            .map_err(ScriptError::Backend),
            "failed to read plant kind",
        )
    }
}

impl Plant
where
    CurrentBackend: PlantRemoveBackend,
{
    /// Removes this plant if the ID currently resolves to a live object.
    ///
    /// Returns `true` when applied and `false` for a stale ID; operation failures end the callback.
    pub fn remove_by_id(&self) -> bool {
        read_or_abort(crate::modifier::remove_plant_by_id(self.id), "failed to remove plant")
    }
}

/// Visits scalar views of all currently live plants without allocating a collection.
pub fn for_each_plant(mut visit: impl FnMut(GridPlantView))
where
    CurrentBackend: PlantReadBackend,
{
    read_or_abort(
        rsvz_current::with_backend_shared(|backend| {
            for plant in crate::live_value::read_or_abort(backend.plants(), "plants") {
                visit(crate::plant::view_from_handle(backend, plant));
            }
        })
        .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))
        .map_err(ScriptError::Backend),
        "failed to visit entities",
    )
}

pub(crate) fn grid_from_handle<'a>(
    backend: &'a CurrentBackend, handle: <CurrentBackend as PlantReadBackend>::PlantHandle<'a>,
) -> Grid
where
    CurrentBackend: PlantReadBackend,
{
    Grid {
        row: backend.plant_row(handle),
        col: backend.plant_col(handle),
    }
}

pub(crate) fn view_from_handle<'a>(
    backend: &'a CurrentBackend, handle: <CurrentBackend as PlantReadBackend>::PlantHandle<'a>,
) -> GridPlantView
where
    CurrentBackend: PlantReadBackend,
{
    GridPlantView {
        id: backend.plant_id(handle),
        kind: crate::live_value::read_or_abort(backend.plant_kind(handle), "plant_kind"),
        grid: grid_from_handle(backend, handle),
        hp: backend.plant_hp(handle),
    }
}
