use std::cell::RefCell;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::DispatchEntry;
use crate::error::{Pvz1051Error, Result};
use crate::runtime::Pvz1051Backend;

static DISPATCH_ENTRY: OnceLock<DispatchEntry> = OnceLock::new();
static BACKEND_CLAIMED: AtomicBool = AtomicBool::new(false);

thread_local! {
    static BACKEND: RefCell<Option<Pvz1051Backend>> = const { RefCell::new(None) };
}

pub fn install_dispatch_entry(entry: DispatchEntry) -> Result<()> {
    DISPATCH_ENTRY
        .set(entry)
        .map_err(|_entry| Pvz1051Error::AbiPreconditionFailed("1051 dispatch entry is already installed"))
}

pub(super) fn installed() -> bool {
    DISPATCH_ENTRY.get().is_some()
}

/// Host ingress contract: call only from the game-thread update ingress while
/// LawnApp is alive, and finish the callback before native update or teardown.
/// Board may be absent (for example in a menu); the token does not prove Board.
pub(crate) fn with_installed<R>(call: impl FnOnce(&mut Pvz1051Backend, DispatchEntry) -> R) -> Option<R> {
    let entry = *DISPATCH_ENTRY.get()?;
    BACKEND.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot =
                Some(claim_backend_token().expect("claim the unique 1051 backend token on the first game-thread hook"));
        }
        Some(call(
            slot.as_mut()
                .expect("1051 backend token must remain owned by the injected hook"),
            entry,
        ))
    })
}

fn claim_backend_token() -> Result<Pvz1051Backend> {
    BACKEND_CLAIMED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_claimed| Pvz1051Error::AbiPreconditionFailed("1051 backend token was already claimed"))?;
    // SAFETY: the process-global atomic above changes from false to true once,
    // so this is the sole safe capability created by the injected backend.
    Ok(unsafe { Pvz1051Backend::claim_runtime_owner_token() })
}
