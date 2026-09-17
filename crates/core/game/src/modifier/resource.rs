//! Resource operations for the selected backend.

/// PvZ natural-sun aging value used by the stable-drop policy.
pub const STABLE_NATURAL_SUN_GENERATED: i32 = 52;
/// Canonical disabled natural-sun countdown used by deterministic worlds.
pub const STABLE_NATURAL_SUN_COUNTDOWN: i32 = 425;

use crate::runtime::{RuntimeError, RuntimeResult};
use rsvz_backend_api::{DropRuleEditBackend, SunCostRuleEditBackend, SunWriteBackend};
use rsvz_current::{CurrentBackend, with_backend_shared};
use rsvz_model::NonNegativeI32;

/// Sets the current battle sun resource.
pub fn set_sun(value: u32) -> RuntimeResult<()>
where
    CurrentBackend: SunWriteBackend,
{
    i32::try_from(value).map_err(|_| RuntimeError::new("sun must fit in a non-negative i32"))?;
    with_backend_shared(|access| access.set_sun(value))
        .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))
        .unwrap_or_else(|error| crate::diagnostics::abort_operation(error))
        .map_err(crate::access::operation_error)
}

/// Enables or disables native sun-cost checks.
pub fn set_sun_cost_ignored(enabled: bool) -> RuntimeResult<()>
where
    CurrentBackend: SunCostRuleEditBackend,
{
    with_backend_shared(|access| access.set_sun_cost_ignored(enabled))
        .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))
        .unwrap_or_else(|error| crate::diagnostics::abort_operation(error))
        .map_err(crate::access::operation_error)
}

/// Stabilizes natural-sun aging using the shared PvZ policy value.
pub fn stabilize_natural_sun_drop_aging() -> RuntimeResult<()>
where
    CurrentBackend: DropRuleEditBackend,
{
    let count = NonNegativeI32::new(STABLE_NATURAL_SUN_GENERATED).expect("non-negative policy constant");
    with_backend_shared(|access| access.set_natural_sun_generated(count))
        .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))
        .unwrap_or_else(|error| crate::diagnostics::abort_operation(error))
        .map_err(crate::access::operation_error)
}

/// Disables natural falling sun and gives backends the same dormant state.
pub fn stabilize_natural_sun_drop() -> RuntimeResult<()>
where
    CurrentBackend: DropRuleEditBackend,
{
    with_backend_shared(|access| {
        access
            .set_natural_sun_drop_disabled(true)
            .map_err(crate::access::operation_error)?;
        stabilize_natural_sun_drop_aging()?;
        let countdown = NonNegativeI32::new(STABLE_NATURAL_SUN_COUNTDOWN).expect("non-negative policy constant");
        access
            .set_natural_sun_countdown(countdown)
            .map_err(crate::access::operation_error)
    })
    .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))
    .unwrap_or_else(|error| crate::diagnostics::abort_operation(error))
}
