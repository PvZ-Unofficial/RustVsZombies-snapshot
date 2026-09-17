//! Backend-neutral script result types.

use std::borrow::Cow;

use crate::logic::active_time::ActiveTimeError;
use crate::logic::cob::CobManagerCallError;
use crate::logic::ice_filler::IceFillerError;
use crate::logic::plant_fixer::PlantFixerError;
use crate::logic::shovel::ShovelError;
use crate::logic::zombies::EnsureZombieRowsError;
use crate::runtime::RuntimeError;

/// Error type returned by backend-neutral scripts.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("backend script operation failed: {0}")]
    Backend(RuntimeError),
    #[error("timeline script operation failed: {0}")]
    Timeline(Cow<'static, str>),
    #[error("tick script operation failed: {0}")]
    Tick(Cow<'static, str>),
    #[error("core script operation failed: {0}")]
    Core(Cow<'static, str>),
}

impl ScriptError {
    pub fn timeline(message: impl Into<Cow<'static, str>>) -> Self {
        Self::Timeline(message.into())
    }

    pub fn tick(message: impl Into<Cow<'static, str>>) -> Self {
        Self::Tick(message.into())
    }

    pub fn core(message: impl Into<Cow<'static, str>>) -> Self {
        Self::Core(message.into())
    }
}

impl From<rsvz_current::CurrentBackendError> for ScriptError {
    fn from(error: rsvz_current::CurrentBackendError) -> Self {
        Self::Backend(RuntimeError::new(error.to_string()))
    }
}

impl From<CobManagerCallError> for ScriptError {
    fn from(error: CobManagerCallError) -> Self {
        match error {
            CobManagerCallError::Backend(error) => Self::Backend(error),
            CobManagerCallError::Manager(error) => Self::Core(error.to_string().into()),
        }
    }
}

impl From<ActiveTimeError> for ScriptError {
    fn from(error: ActiveTimeError) -> Self {
        match error {
            ActiveTimeError::InvalidDelay => Self::Core("active-time delay must be at least 10 frames".into()),
        }
    }
}

impl From<IceFillerError> for ScriptError {
    fn from(error: IceFillerError) -> Self {
        match error {
            IceFillerError::Backend(error) => Self::Backend(error),
            error => Self::Core(error.to_string().into()),
        }
    }
}

impl From<PlantFixerError> for ScriptError {
    fn from(error: PlantFixerError) -> Self {
        match error {
            PlantFixerError::Backend(error) => Self::Backend(error),
            error @ (PlantFixerError::InvalidGrid { .. }
            | PlantFixerError::ImitatorTarget
            | PlantFixerError::InvalidThreshold(_)
            | PlantFixerError::InvalidRunInterval(_)
            | PlantFixerError::MissingSeed(_)
            | PlantFixerError::CostOverflow) => Self::Core(error.to_string().into()),
        }
    }
}

impl From<ShovelError> for ScriptError {
    fn from(error: ShovelError) -> Self {
        match error {
            ShovelError::InvalidGrid => Self::Core("invalid shovel grid".into()),
            ShovelError::Backend(error) => Self::Backend(error),
        }
    }
}

impl From<EnsureZombieRowsError> for ScriptError {
    fn from(error: EnsureZombieRowsError) -> Self {
        match error {
            EnsureZombieRowsError::Backend(error) => Self::Backend(RuntimeError::new(error.to_string())),
            EnsureZombieRowsError::UnsupportedKind(kind) => {
                Self::Core(format!("unsupported ensure-exist zombie kind: {kind:?}").into())
            }
            EnsureZombieRowsError::InvalidRow { row, max_row } => {
                Self::Core(format!("invalid ensure-exist row {row}; expected 1..={max_row}").into())
            }
            EnsureZombieRowsError::RowDisallowed { kind, row, scene } => {
                Self::Core(format!("zombie kind {kind:?} is disallowed on row {row} in scene {scene:?}").into())
            }
            EnsureZombieRowsError::InsufficientZombies { kind, row } => {
                Self::Core(format!("insufficient fresh {kind:?} zombies to ensure row {row}").into())
            }
            EnsureZombieRowsError::InvalidCoordinate(error) => {
                Self::Core(format!("computed zombie y coordinate is invalid: {error}").into())
            }
        }
    }
}

impl From<ScriptError> for RuntimeError {
    fn from(error: ScriptError) -> Self {
        Self::new(error.to_string())
    }
}

/// Result shorthand for script operations.
pub type ScriptResult<T = ()> = std::result::Result<T, ScriptError>;
