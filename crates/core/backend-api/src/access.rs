//! Short-lived access to a backend token owned by its physical host.

use std::cell::{Cell, RefCell};

/// Failure to enter a host-owned backend borrow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BackendAccessError {
    #[error("current backend is not scoped on this thread")]
    NotInstalled,
    #[error("current backend access conflicts with an active borrow")]
    BorrowConflict,
}

/// Borrow protection shared by backend-local TLS instances.
///
/// The scope owns no backend and cannot transfer access to another thread.
/// A callback may return an owned result, but cannot retain the token borrow:
///
/// ```
/// use rsvz_backend_api::access::BackendScope;
/// let scope = BackendScope::new();
/// let mut backend = 1;
/// assert_eq!(scope.enter(&mut backend, || scope.with(|b| *b)), 1);
/// ```
///
/// ```compile_fail
/// use rsvz_backend_api::access::BackendScope;
/// let scope = BackendScope::new();
/// let mut backend = 1;
/// scope.enter(&mut backend, || scope.with(|b| b));
/// ```
pub struct BackendScope<B> {
    backend: Cell<*mut B>,
    borrowed: RefCell<()>,
    access_epoch: Cell<u64>,
}

impl<B> Default for BackendScope<B> {
    fn default() -> Self {
        Self::new()
    }
}

impl<B> BackendScope<B> {
    pub const fn new() -> Self {
        Self {
            backend: Cell::new(std::ptr::null_mut()),
            borrowed: RefCell::new(()),
            access_epoch: Cell::new(0),
        }
    }

    /// Exposes the host's exclusive token borrow only until `body` returns.
    pub fn enter<R>(&self, backend: &mut B, body: impl FnOnce() -> R) -> R {
        assert!(
            self.backend.get().is_null(),
            "nested current-backend scopes are not supported"
        );
        struct ScopeGuard<'a, B>(&'a Cell<*mut B>);
        impl<B> Drop for ScopeGuard<'_, B> {
            fn drop(&mut self) {
                self.0.set(std::ptr::null_mut());
            }
        }
        self.backend.set(std::ptr::from_mut(backend));
        self.access_epoch.set(self.access_epoch.get().wrapping_add(1));
        let _guard = ScopeGuard(&self.backend);
        body()
    }

    /// Shared borrows may nest; exclusive physical operations cannot overlap them.
    pub fn with_shared<R>(&self, f: impl for<'a> FnOnce(&'a B) -> R) -> Result<R, BackendAccessError> {
        let backend = self.backend.get();
        if backend.is_null() {
            return Err(BackendAccessError::NotInstalled);
        }
        let _guard = self
            .borrowed
            .try_borrow()
            .map_err(|_| BackendAccessError::BorrowConflict)?;
        // SAFETY: enter holds the host token, the guard excludes mutable access,
        // and HRTB prevents this reference or a derived borrow from escaping.
        Ok(f(unsafe { &*backend }))
    }

    pub fn try_with<R>(&self, f: impl for<'a> FnOnce(&'a mut B) -> R) -> Result<R, BackendAccessError> {
        let backend = self.backend.get();
        if backend.is_null() {
            return Err(BackendAccessError::NotInstalled);
        }
        let _guard = self
            .borrowed
            .try_borrow_mut()
            .map_err(|_| BackendAccessError::BorrowConflict)?;
        self.access_epoch.set(self.access_epoch.get().wrapping_add(1));
        // SAFETY: enter holds the host token and the guard excludes all other
        // borrows. HRTB prevents the callback from retaining the mutable reference.
        Ok(f(unsafe { &mut *backend }))
    }

    pub fn with<R>(&self, f: impl for<'a> FnOnce(&'a mut B) -> R) -> R {
        self.try_with(f).unwrap_or_else(|error| panic!("{error}"))
    }

    /// Identifies host entry and successful exclusive accesses, including unwinds.
    /// Used only at callback boundaries to discard samples that may be stale.
    pub fn access_epoch(&self) -> Option<u64> {
        (!self.backend.get().is_null()).then(|| self.access_epoch.get())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn shared_access_nests_but_excludes_mutation_and_recovers_after_unwind() {
        let scope = BackendScope::new();
        let mut backend = 7;
        assert_eq!(scope.with_shared(|_| ()), Err(BackendAccessError::NotInstalled));
        scope.enter(&mut backend, || {
            scope
                .with_shared(|outer| {
                    assert_eq!(scope.with_shared(|inner| *inner), Ok(*outer));
                    assert_eq!(scope.try_with(|_| ()), Err(BackendAccessError::BorrowConflict));
                    assert!(
                        catch_unwind(AssertUnwindSafe(|| {
                            scope.with_shared(|_| panic!("shared callback"))
                        }))
                        .is_err()
                    );
                    assert_eq!(scope.try_with(|_| ()), Err(BackendAccessError::BorrowConflict));
                })
                .unwrap();
            scope.with(|value| {
                assert_eq!(scope.with_shared(|_| ()), Err(BackendAccessError::BorrowConflict));
                *value += 1;
            });
        });
        assert_eq!(backend, 8);
    }

    #[test]
    fn epoch_tracks_only_successful_exclusive_access_and_host_entry() {
        let scope = BackendScope::new();
        let mut backend = 0;
        assert_eq!(scope.access_epoch(), None);
        let epoch = scope.enter(&mut backend, || {
            let initial = scope.access_epoch();
            scope
                .with_shared(|_| {
                    assert_eq!(scope.try_with(|_| ()), Err(BackendAccessError::BorrowConflict));
                })
                .unwrap();
            assert_eq!(scope.access_epoch(), initial);
            assert!(catch_unwind(AssertUnwindSafe(|| scope.with(|_| panic!("physical operation")))).is_err());
            assert_ne!(scope.access_epoch(), initial);
            scope.access_epoch()
        });
        assert_eq!(scope.access_epoch(), None);
        scope.enter(&mut backend, || assert_ne!(scope.access_epoch(), epoch));
    }

    #[test]
    fn scoped_access_restores_after_reentry_and_unwind() {
        let scope = BackendScope::new();
        let mut backend = 0;
        assert!(catch_unwind(AssertUnwindSafe(|| scope.with(|_| ()))).is_err());
        scope.enter(&mut backend, || {
            scope.with(|b| {
                *b = 1;
                assert!(catch_unwind(AssertUnwindSafe(|| scope.with(|_| ()))).is_err());
                // A rejected nested call must not release this outer borrow.
                assert!(catch_unwind(AssertUnwindSafe(|| scope.with(|_| ()))).is_err());
            });
            assert!(catch_unwind(AssertUnwindSafe(|| scope.with(|_| panic!("borrow")))).is_err());
            assert_eq!(scope.with(|b| *b), 1);
            let mut other = 2;
            assert!(catch_unwind(AssertUnwindSafe(|| scope.enter(&mut other, || ()))).is_err());
            assert_eq!(scope.with(|b| *b), 1);
        });
        assert!(catch_unwind(AssertUnwindSafe(|| scope.enter(&mut backend, || panic!("scope")))).is_err());
        assert!(catch_unwind(AssertUnwindSafe(|| scope.with(|_| ()))).is_err());
        scope.enter(&mut backend, || scope.with(|b| *b += 1));
        assert_eq!(backend, 2);
    }
}
