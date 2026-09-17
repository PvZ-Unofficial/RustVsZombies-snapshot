//! Current access to the host-owned backend token.

use crate::PeBackend;
use rsvz_backend_api::access::BackendScope;

thread_local! {
    static BACKEND: BackendScope<PeBackend> = const { BackendScope::new() };
}

/// Borrows the physical host's token for one dispatch scope.
#[doc(hidden)]
pub fn scope_backend<R>(backend: &mut PeBackend, body: impl FnOnce() -> R) -> R {
    BACKEND.with(|scope| scope.enter(backend, body))
}

/// Runs `f` with the uniquely installed backend token.
///
/// The callback borrow is intentionally unable to escape in `R`:
///
/// ```compile_fail
/// use rsvz_backend_api::backend::PlantReadBackend;
/// let _handle = rsvz_pvz_emulator_backend::with_backend(|backend| {
///     backend.plants().unwrap().next().unwrap()
/// });
/// ```
///
/// The token, its handles, and its iterators inherit backend thread affinity:
///
/// ```compile_fail
/// rsvz_pvz_emulator_backend::with_backend(|backend| {
///     std::thread::scope(|scope| {
///         scope.spawn(move || { std::hint::black_box(backend); });
///     });
/// });
/// ```
///
/// ```compile_fail
/// rsvz_pvz_emulator_backend::with_backend(|backend| {
///     std::thread::spawn(move || { std::hint::black_box(backend); });
/// });
/// ```
///
/// ```compile_fail
/// use rsvz_backend_api::backend::PlantReadBackend;
/// rsvz_pvz_emulator_backend::with_backend(|backend| {
///     let handle = backend.plants().unwrap().next().unwrap();
///     std::thread::scope(|scope| {
///         scope.spawn(move || drop(handle));
///     });
/// });
/// ```
///
/// ```compile_fail
/// use rsvz_backend_api::backend::PlantReadBackend;
/// rsvz_pvz_emulator_backend::with_backend(|backend| {
///     let plants = backend.plants().unwrap();
///     std::thread::scope(|scope| {
///         scope.spawn(move || drop(plants));
///     });
/// });
/// ```
///
/// ```compile_fail
/// use rsvz_backend_api::backend::PlantReadBackend;
/// rsvz_pvz_emulator_backend::with_backend(|backend| {
///     let plants = backend.plants().unwrap();
///     std::thread::spawn(move || drop(plants));
/// });
/// ```
///
/// ```compile_fail
/// use rsvz_backend_api::backend::PlantReadBackend;
/// rsvz_pvz_emulator_backend::with_backend(|backend| {
///     let handle = backend.plants().unwrap().next().unwrap();
///     std::thread::spawn(move || drop(handle));
/// });
/// ```
/// PE invalidating operations require an exclusive token borrow, so a raw handle
/// cannot survive an update, reset, or scene replacement:
///
/// ```compile_fail
/// use pe_rs::{SceneType, World};
/// let mut world = World::new_deterministic(SceneType::Pool, 1, 2, 3).unwrap();
/// let scene = world.scene();
/// world.update().unwrap();
/// drop(scene);
/// ```
///
/// ```compile_fail
/// use rsvz_backend_api::backend::{PlantReadBackend, WorldResetBackend};
/// rsvz_pvz_emulator_backend::with_backend(|backend| {
///     let handle = backend.plants().unwrap().next().unwrap();
///     backend.reset_world(rsvz_model::WorldResetConfig::default()).unwrap();
///     drop(handle);
/// });
/// ```
///
/// ```compile_fail
/// use rsvz_backend_api::backend::{PlantReadBackend, SceneEditBackend};
/// use rsvz_model::SceneKind;
/// rsvz_pvz_emulator_backend::with_backend(|backend| {
///     let handle = backend.plants().unwrap().next().unwrap();
///     backend.set_scene(SceneKind::Roof).unwrap();
///     drop(handle);
/// });
/// ```
///
/// Owned values and same-thread borrows remain usable:
///
/// ```no_run
/// use rsvz_backend_api::backend::PlantReadBackend;
/// use rsvz_pvz_emulator_backend::with_backend_shared;
/// let _id = with_backend_shared(|backend| {
///     let plant = backend.plants().unwrap().next().unwrap();
///     backend.plant_id(plant)
/// }).unwrap();
/// with_backend_shared(|backend| {
///     for plant in backend.plants().unwrap() { std::hint::black_box(plant); }
///     std::hint::black_box(backend);
/// }).unwrap();
/// ```
///
/// End the handle borrow before operations that invalidate the world:
///
/// ```no_run
/// use rsvz_backend_api::backend::{PlantReadBackend, SceneEditBackend, WorldResetBackend};
/// use rsvz_model::SceneKind;
/// rsvz_pvz_emulator_backend::with_backend_shared(|backend| {
///     let plant = backend.plants().unwrap().next().unwrap();
///     std::hint::black_box(plant);
/// }).unwrap();
/// rsvz_pvz_emulator_backend::with_backend(|backend| {
///     backend.reset_world(rsvz_model::WorldResetConfig::default()).unwrap();
///     backend.set_scene(SceneKind::Roof).unwrap();
/// });
/// ```
///
/// ```no_run
/// use pe_rs::{SceneType, World};
/// let mut world = World::new_deterministic(SceneType::Pool, 1, 2, 3).unwrap();
/// { let scene = world.scene(); std::hint::black_box(scene); }
/// world.update().unwrap();
/// ```
///
/// ```compile_fail
/// rsvz_pvz_emulator_backend::with_backend(|backend| backend);
/// ```
///
/// ```compile_fail
/// use rsvz_backend_api::backend::PlantReadBackend;
/// rsvz_pvz_emulator_backend::with_backend(|backend| backend.plants().unwrap());
/// ```
pub fn with_backend<R>(f: impl for<'a> FnOnce(&'a mut PeBackend) -> R) -> R {
    BACKEND.with(|scope| scope.with(f))
}

/// Tries an exclusive physical operation without converting borrow errors to panics.
#[doc(hidden)]
pub fn try_with_backend<R>(
    f: impl for<'a> FnOnce(&'a mut PeBackend) -> R,
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
    f: impl for<'a> FnOnce(&'a PeBackend) -> R,
) -> Result<R, rsvz_backend_api::access::BackendAccessError> {
    BACKEND.with(|scope| scope.with_shared(f))
}

pub(crate) fn plant_pool(world: &pe_rs::World) -> pe_rs::PlantPool<'_> {
    world.scene().plants()
}

pub(crate) fn zombie_pool(world: &pe_rs::World) -> pe_rs::ZombiePool<'_> {
    world.scene().zombies()
}

pub(crate) fn grid_item_pool(world: &pe_rs::World) -> pe_rs::GridItemPool<'_> {
    world.scene().griditems()
}

pub(crate) fn projectile_pool(world: &pe_rs::World) -> pe_rs::ProjectilePool<'_> {
    world.scene().projectiles()
}

pub(crate) fn spawn_data(world: &pe_rs::World) -> pe_rs::SpawnDataRef<'_> {
    world.scene().spawn_data()
}
