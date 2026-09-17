//! Current access to the host-owned backend token.

use crate::Pvz1051Backend;
use rsvz_backend_api::access::BackendScope;

thread_local! {
    static BACKEND: BackendScope<Pvz1051Backend> = const { BackendScope::new() };
}

/// Borrows the physical host's token for one dispatch scope.
#[doc(hidden)]
pub fn scope_backend<R>(backend: &mut Pvz1051Backend, body: impl FnOnce() -> R) -> R {
    BACKEND.with(|scope| scope.enter(backend, body))
}

/// Calls `f` with the uniquely borrowed current backend.
pub fn with_backend<R>(f: impl for<'a> FnOnce(&'a mut Pvz1051Backend) -> R) -> R {
    BACKEND.with(|scope| scope.with(f))
}

/// Tries an exclusive physical operation without converting borrow errors to panics.
#[doc(hidden)]
pub fn try_with_backend<R>(
    f: impl for<'a> FnOnce(&'a mut Pvz1051Backend) -> R,
) -> Result<R, rsvz_backend_api::access::BackendAccessError> {
    BACKEND.with(|scope| scope.try_with(f))
}

#[doc(hidden)]
pub fn backend_access_epoch() -> Option<u64> {
    BACKEND.with(BackendScope::access_epoch)
}

/// Borrows the current token without excluding other shared operations.
#[doc(hidden)]
pub fn with_backend_shared<R>(
    f: impl for<'a> FnOnce(&'a Pvz1051Backend) -> R,
) -> Result<R, rsvz_backend_api::access::BackendAccessError> {
    BACKEND.with(|scope| scope.with_shared(f))
}
