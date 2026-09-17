//! Current cheat-rule operations.
use crate::runtime::RuntimeResult;
use rsvz_backend_api::{DanceModeBackend, MaidCheatsBackend};
use rsvz_current::CurrentBackend;
use rsvz_model::MaidCheat;

/// Performs one native Dance-mode operation. Repeated calls restart eligible walkers.
pub fn set_dance_mode(enabled: bool) -> RuntimeResult<()>
where
    CurrentBackend: DanceModeBackend,
{
    crate::access::with_backend(|backend| backend.set_dance_mode(enabled)).map_err(crate::access::operation_error)
}

pub fn set_maid_cheat(cheat: MaidCheat) -> RuntimeResult<()>
where
    CurrentBackend: MaidCheatsBackend,
{
    crate::access::with_backend(|backend| backend.set_maid_cheat(cheat)).map_err(crate::access::operation_error)
}
