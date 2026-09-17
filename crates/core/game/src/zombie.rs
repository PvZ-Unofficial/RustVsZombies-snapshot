//! Backend-neutral zombie runtime proxy helpers.

use rsvz_current::CurrentBackend;

use crate::runtime::{RuntimeError, RuntimeResult};
use crate::script::ScriptError;
use rsvz_backend_api::backend::{
    GridTerrainBackend, SceneBackend, ZombieKillBackend, ZombiePositionWriteBackend, ZombieRawFactsBackend,
    ZombieReadBackend, ZombieRemoveBackend, ZombieVerticalPositionBackend,
};
use rsvz_model::ZombieId;
use rsvz_model::{ZombieKind, ZombieSpawnSnapshot, ZombieState};

use crate::live_value::LiveValue;
use crate::live_value::{live_or_missing, read_or_abort};

pub fn try_ensure_zombie_row(kind: ZombieKind, row: i32) -> RuntimeResult<()>
where
    CurrentBackend: SceneBackend + GridTerrainBackend + ZombiePositionWriteBackend,
{
    crate::logic::zombies::ensure_zombie_rows_one_based(kind, [row]).map_err(runtime_error)
}

pub fn ensure_zombie_row(kind: ZombieKind, row: i32)
where
    CurrentBackend: SceneBackend + GridTerrainBackend + ZombiePositionWriteBackend,
{
    if let Err(error) = try_ensure_zombie_row(kind, row) {
        crate::diagnostics::report_operation_error(error);
    }
}

/// Id-only zombie handle that re-resolves the object for every read or action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Zombie {
    id: ZombieId,
}

impl Zombie {
    /// Creates a handle from a cross-frame zombie ID.
    #[must_use]
    pub const fn from_id(id: ZombieId) -> Self {
        Self { id }
    }

    /// Returns this handle's cross-frame zombie ID.
    #[must_use]
    pub const fn id(&self) -> ZombieId {
        self.id
    }
}

impl Zombie
where
    CurrentBackend: ZombieReadBackend,
{
    /// Checks whether this zombie ID currently resolves to a live object.
    pub fn is_alive(&self) -> bool {
        read_or_abort(
            rsvz_current::with_backend_shared(|backend| read_or_abort(backend.zombie(self.id), "zombie").is_some())
                .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))
                .map_err(ScriptError::Backend),
            "failed to resolve zombie id",
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
                read_or_abort(backend.zombie(self.id), "zombie").map(|zombie| backend.zombie_hp(zombie))
            })
            .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))
            .map_err(ScriptError::Backend),
            "failed to read zombie hp",
        )
    }

    /// Reads row as a convenience live value.
    ///
    /// Stale or missing IDs produce `LiveValue::missing()`.
    ///
    /// A backend read failure ends the current callback through its error boundary.
    #[must_use]
    pub fn row(&self) -> LiveValue<i32> {
        live_or_missing(
            rsvz_current::with_backend_shared(|backend| {
                read_or_abort(backend.zombie(self.id), "zombie").map(|zombie| backend.zombie_row(zombie))
            })
            .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))
            .map_err(ScriptError::Backend),
            "failed to read zombie row",
        )
    }

    /// Reads kind as a convenience live value.
    ///
    /// Stale or missing IDs produce `LiveValue::missing()`.
    ///
    /// A backend read failure ends the current callback through its error boundary.
    #[must_use]
    pub fn kind(&self) -> LiveValue<ZombieKind> {
        live_or_missing(
            rsvz_current::with_backend_shared(|backend| {
                read_or_abort(backend.zombie(self.id), "zombie")
                    .map(|zombie| read_or_abort(backend.zombie_kind(zombie), "zombie_kind"))
            })
            .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))
            .map_err(ScriptError::Backend),
            "failed to read zombie kind",
        )
    }
}

impl Zombie
where
    CurrentBackend: ZombieKillBackend,
{
    /// Kills this zombie if the ID currently resolves to a live object.
    ///
    /// Returns `true` when applied and `false` for a stale ID; operation failures end the callback.
    pub fn kill_by_id(&self) -> bool {
        read_or_abort(
            rsvz_current::with_backend_shared(|access| -> RuntimeResult<bool> {
                let backend = access;
                let Some(zombie) = crate::live_value::read_or_abort(backend.zombie(self.id), "zombie") else {
                    return Ok(false);
                };
                backend.kill_zombie(zombie).map_err(crate::access::rejected_error)?;
                Ok(true)
            })
            .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))
            .and_then(|value| value),
            "failed to edit zombie",
        )
    }
}

impl Zombie
where
    CurrentBackend: ZombieRemoveBackend,
{
    /// Removes this zombie if the ID currently resolves to a live object.
    ///
    /// Returns `true` when applied and `false` for a stale ID; operation failures end the callback.
    pub fn remove_by_id(&self) -> bool {
        read_or_abort(
            rsvz_current::with_backend_shared(|access| -> RuntimeResult<bool> {
                let backend = access;
                let Some(zombie) = crate::live_value::read_or_abort(backend.zombie(self.id), "zombie") else {
                    return Ok(false);
                };
                backend.remove_zombie(zombie).map_err(crate::access::rejected_error)?;
                Ok(true)
            })
            .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))
            .and_then(|value| value),
            "failed to edit zombie",
        )
    }
}

impl Zombie
where
    CurrentBackend: ZombieRawFactsBackend,
{
    /// Reads full zombie state after re-resolving this zombie ID.
    pub fn state(&self) -> Option<ZombieState> {
        read_or_abort(
            rsvz_current::with_backend_shared(|backend| {
                read_or_abort(backend.zombie(self.id), "zombie").map(|handle| state_from_handle(backend, handle))
            })
            .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))
            .map_err(ScriptError::Backend),
            "failed to read zombie state",
        )
    }
}

pub fn zombie_state(id: ZombieId) -> Option<ZombieState>
where
    CurrentBackend: ZombieRawFactsBackend,
{
    Zombie::from_id(id).state()
}

/// Visits scalar views of all currently live zombies without allocating a collection.
pub fn for_each_zombie(mut visit: impl FnMut(ZombieSpawnSnapshot))
where
    CurrentBackend: ZombieReadBackend,
{
    read_or_abort(
        rsvz_current::with_backend_shared(|backend| {
            for zombie in crate::live_value::read_or_abort(backend.zombies(), "zombies") {
                visit(ZombieSpawnSnapshot {
                    id: backend.zombie_id(zombie),
                    kind: crate::live_value::read_or_abort(backend.zombie_kind(zombie), "zombie_kind"),
                    row: backend.zombie_row(zombie),
                });
            }
        })
        .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))
        .map_err(ScriptError::Backend),
        "failed to visit entities",
    )
}

fn runtime_error(error: impl std::fmt::Display) -> RuntimeError {
    RuntimeError::new(error.to_string())
}

pub(crate) fn reanimation_from_handle<'a>(
    access: &'a rsvz_current::CurrentBackend, zombie: <CurrentBackend as ZombieReadBackend>::ZombieHandle<'a>,
) -> Result<Option<rsvz_model::ZombieReanimationFacts>, rsvz_current::CurrentBackendError>
where
    CurrentBackend: ZombieRawFactsBackend,
{
    let Some(anim_time) = access.zombie_reanim_anim_time(zombie)? else {
        return Ok(None);
    };
    let Some(last_time) = access.zombie_reanim_last_time(zombie)? else {
        return Ok(None);
    };
    let Some(rate) = access.zombie_reanim_rate(zombie)? else {
        return Ok(None);
    };
    let Some(frame_start) = access.zombie_reanim_frame_start(zombie)? else {
        return Ok(None);
    };
    let Some(frame_count) = access.zombie_reanim_frame_count(zombie)? else {
        return Ok(None);
    };
    let Some(loop_type) = access.zombie_reanim_loop_type(zombie)? else {
        return Ok(None);
    };
    Ok(Some(rsvz_model::ZombieReanimationFacts {
        anim_time,
        last_time,
        rate,
        frame_start,
        frame_count,
        loop_type,
    }))
}

pub(crate) fn state_from_handle<'a>(
    backend: &'a CurrentBackend, handle: <CurrentBackend as ZombieReadBackend>::ZombieHandle<'a>,
) -> ZombieState
where
    CurrentBackend: ZombieRawFactsBackend,
{
    ZombieState {
        id: backend.zombie_id(handle),
        kind: crate::live_value::read_or_abort(backend.zombie_kind(handle), "zombie_kind"),
        row: backend.zombie_row(handle),
        x: backend.zombie_pos_x(handle),
        y: backend.zombie_pos_y(handle),
        hp: backend.zombie_hp(handle),
        alive: backend.zombie_is_alive(handle),
        phase: crate::live_value::read_or_abort(backend.zombie_phase(handle), "zombie_phase"),
    }
}
