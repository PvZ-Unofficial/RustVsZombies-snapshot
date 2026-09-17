//! Current access to the host-owned backend token.

use crate::PortableBackend;
use rsvz_backend_api::access::BackendScope;

thread_local! {
    static BACKEND: BackendScope<PortableBackend> = const { BackendScope::new() };
}

/// Borrows the physical host's token for one dispatch scope.
#[doc(hidden)]
pub fn scope_backend<R>(backend: &mut PortableBackend, body: impl FnOnce() -> R) -> R {
    BACKEND.with(|scope| scope.enter(backend, body))
}

/// Calls `f` with the uniquely borrowed current backend.
pub fn with_backend<R>(f: impl for<'a> FnOnce(&'a mut PortableBackend) -> R) -> R {
    BACKEND.with(|scope| scope.with(f))
}

/// Tries an exclusive physical operation without converting borrow errors to panics.
#[doc(hidden)]
pub fn try_with_backend<R>(
    f: impl for<'a> FnOnce(&'a mut PortableBackend) -> R,
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
    f: impl for<'a> FnOnce(&'a PortableBackend) -> R,
) -> Result<R, rsvz_backend_api::access::BackendAccessError> {
    BACKEND.with(|scope| scope.with_shared(f))
}

pub(crate) fn plant_pool<'a>(world: &pvzp_rs::World<'a>) -> pvzp_rs::PlantPool<'a> {
    world.plants()
}

pub(crate) fn zombie_pool<'a>(world: &pvzp_rs::World<'a>) -> pvzp_rs::ZombiePool<'a> {
    world.zombies()
}

pub(crate) fn grid_item_pool<'a>(world: &pvzp_rs::World<'a>) -> pvzp_rs::GridItemPool<'a> {
    world.grid_items()
}

pub(crate) fn projectile_pool<'a>(world: &pvzp_rs::World<'a>) -> pvzp_rs::ProjectilePool<'a> {
    world.projectiles()
}

pub(crate) fn item_pool<'a>(world: &pvzp_rs::World<'a>) -> pvzp_rs::ItemPool<'a> {
    world.items()
}
