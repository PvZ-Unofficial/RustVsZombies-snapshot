use std::{marker::PhantomData, ptr::NonNull};

use rsvz_model::model::SeedSlot;

use crate::PeBackend;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeSeedHandle<'a> {
    pub(crate) slot: SeedSlot,
    pub(crate) ptr: NonNull<pe_rs::raw::pe_rs_card>,
    pub(crate) _marker: PhantomData<&'a PeBackend>,
}

pub struct PeSeedIter<'a> {
    backend: &'a PeBackend,
    next: u32,
    limit: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PePlantHandle<'a> {
    pub(crate) ptr: NonNull<pe_rs::raw::pe_rs_plant>,
    pub(crate) _marker: PhantomData<&'a PeBackend>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeGridItemHandle<'a> {
    pub(crate) ptr: NonNull<pe_rs::raw::pe_rs_griditem>,
    pub(crate) _marker: PhantomData<&'a PeBackend>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeProjectileHandle<'a> {
    pub(crate) ptr: NonNull<pe_rs::raw::pe_rs_projectile>,
    pub(crate) _marker: PhantomData<&'a PeBackend>,
}

pub struct PePlantIter<'a> {
    backend: &'a PeBackend,
    next: u32,
    limit: u32,
}

pub struct PeGridItemIter<'a> {
    backend: &'a PeBackend,
    next: u32,
    limit: u32,
}

pub struct PeProjectileIter<'a> {
    backend: &'a PeBackend,
    next: u32,
    limit: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeZombieHandle<'a> {
    pub(crate) ptr: NonNull<pe_rs::raw::pe_rs_zombie>,
    pub(crate) _marker: PhantomData<&'a PeBackend>,
}

pub struct PeZombieIter<'a> {
    backend: &'a PeBackend,
    next: u32,
    limit: u32,
}

macro_rules! handle {
    ($name:ident, $raw:ty) => {
        impl<'a> $name<'a> {
            #[must_use]
            pub(crate) fn new(ptr: NonNull<$raw>) -> Self {
                Self {
                    ptr,
                    _marker: PhantomData,
                }
            }

            #[must_use]
            pub(crate) const fn as_ptr(self) -> *const $raw {
                self.ptr.as_ptr()
            }
        }
    };
}

handle!(PePlantHandle, pe_rs::raw::pe_rs_plant);
handle!(PeGridItemHandle, pe_rs::raw::pe_rs_griditem);
handle!(PeProjectileHandle, pe_rs::raw::pe_rs_projectile);
handle!(PeZombieHandle, pe_rs::raw::pe_rs_zombie);

impl PePlantHandle<'_> {
    #[must_use]
    pub(crate) const fn as_mut_ptr(self) -> *mut pe_rs::raw::pe_rs_plant {
        self.ptr.as_ptr()
    }
}

impl PeZombieHandle<'_> {
    #[must_use]
    pub(crate) const fn as_mut_ptr(self) -> *mut pe_rs::raw::pe_rs_zombie {
        self.ptr.as_ptr()
    }
}

impl<'a> PeSeedHandle<'a> {
    #[must_use]
    pub(crate) fn new(slot: SeedSlot, ptr: NonNull<pe_rs::raw::pe_rs_card>) -> Self {
        Self {
            slot,
            ptr,
            _marker: PhantomData,
        }
    }

    #[must_use]
    pub(crate) const fn as_ptr(self) -> *const pe_rs::raw::pe_rs_card {
        self.ptr.as_ptr()
    }
}

impl<'a> PePlantIter<'a> {
    #[must_use]
    pub(crate) fn new(backend: &'a PeBackend, limit: u32) -> Self {
        Self {
            backend,
            next: 0,
            limit,
        }
    }
}

impl<'a> PeGridItemIter<'a> {
    #[must_use]
    pub(crate) fn new(backend: &'a PeBackend, limit: u32) -> Self {
        Self {
            backend,
            next: 0,
            limit,
        }
    }
}

impl<'a> PeProjectileIter<'a> {
    #[must_use]
    pub(crate) fn new(backend: &'a PeBackend, limit: u32) -> Self {
        Self {
            backend: backend,
            next: 0,
            limit,
        }
    }
}

impl<'a> PeZombieIter<'a> {
    #[must_use]
    pub(crate) fn new(backend: &'a PeBackend, limit: u32) -> Self {
        Self {
            backend,
            next: 0,
            limit,
        }
    }
}

impl<'a> PeSeedIter<'a> {
    #[must_use]
    pub(crate) const fn new(backend: &'a PeBackend) -> Self {
        Self {
            backend: backend,
            next: 0,
            limit: rsvz_model::MAX_SEED_SLOTS as u32,
        }
    }
}

impl<'a> Iterator for PePlantIter<'a> {
    type Item = PePlantHandle<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.next < self.limit {
            let index = self.next;
            self.next += 1;
            let ptr = self
                .backend
                .with_current_world(|world| {
                    world
                        .scene()
                        .plants()
                        .get(index as i32)
                        .map(pe_rs::Borrowed::as_non_null)
                })
                .expect("PE iterator retains a valid shared backend borrow");
            let Some(ptr) = ptr else {
                continue;
            };
            let handle = PePlantHandle::new(ptr);
            if self.backend.pe_plant_is_live(handle) {
                return Some(handle);
            }
        }
        None
    }
}

impl<'a> Iterator for PeGridItemIter<'a> {
    type Item = PeGridItemHandle<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.next < self.limit {
            let index = self.next;
            self.next += 1;
            let ptr = self
                .backend
                .with_current_world(|world| {
                    world
                        .scene()
                        .griditems()
                        .get(index as i32)
                        .map(pe_rs::Borrowed::as_non_null)
                })
                .expect("PE iterator retains a valid shared backend borrow");
            let Some(ptr) = ptr else {
                continue;
            };
            let handle = PeGridItemHandle::new(ptr);
            if self.backend.pe_grid_item_is_live(handle) {
                return Some(handle);
            }
        }
        None
    }
}

impl<'a> Iterator for PeProjectileIter<'a> {
    type Item = PeProjectileHandle<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.next < self.limit {
            let index = self.next;
            self.next += 1;
            let ptr = self
                .backend
                .with_current_world(|world| {
                    world
                        .scene()
                        .projectiles()
                        .get(index as i32)
                        .map(pe_rs::Borrowed::as_non_null)
                })
                .expect("PE iterator retains a valid shared backend borrow");
            let Some(ptr) = ptr else {
                continue;
            };
            // PE keeps disappearing projectiles occupied until its next shrink pass.
            // SAFETY: `ptr` came from the occupied PE object pool entry above.
            let disappeared = unsafe { std::ptr::addr_of!((*ptr.as_ptr()).is_disappeared).read() };
            let handle = PeProjectileHandle::new(ptr);
            let has_target = rsvz_backend_api::ProjectileReadBackend::projectile_motion(self.backend, handle) != 9
                || rsvz_backend_api::ZombieReadBackend::zombie(
                    self.backend,
                    rsvz_model::ZombieId::from_raw(
                        rsvz_backend_api::ProjectileReadBackend::projectile_target_zombie_id(self.backend, handle),
                    ),
                )
                .expect("projectile iterator retains its world")
                .is_some();
            if !disappeared && has_target {
                return Some(handle);
            }
        }
        None
    }
}

impl<'a> Iterator for PeZombieIter<'a> {
    type Item = PeZombieHandle<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.next < self.limit {
            let index = self.next;
            self.next += 1;
            let ptr = self
                .backend
                .with_current_world(|world| {
                    world
                        .scene()
                        .zombies()
                        .get(index as i32)
                        .map(pe_rs::Borrowed::as_non_null)
                })
                .expect("PE iterator retains a valid shared backend borrow");
            let Some(ptr) = ptr else {
                continue;
            };
            let handle = PeZombieHandle::new(ptr);
            if self.backend.pe_zombie_is_live(handle) {
                return Some(handle);
            }
        }
        None
    }
}

impl<'a> Iterator for PeSeedIter<'a> {
    type Item = PeSeedHandle<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.next < self.limit {
            let slot_index = self.next;
            self.next += 1;
            let ptr = self
                .backend
                .with_current_world(|world| world.scene().card_at(slot_index).map(pe_rs::Borrowed::as_non_null))
                .expect("PE seed iterator retains a valid shared backend borrow")
                .unwrap_or_else(|error| panic!("PE seed iterator bridge invariant failed: {error}"));

            // SAFETY: the card pointer is current and its scalar kind field is
            // copied without creating a Rust reference into PE storage.
            let plant_type = unsafe { std::ptr::addr_of!((*ptr.as_ptr()).type_).read() };
            if plant_type == pe_rs::raw::pvz_emulator_object_plant_type_none {
                continue;
            }
            let slot = SeedSlot::from_index_unchecked(slot_index as usize);
            return Some(PeSeedHandle::new(slot, ptr));
        }
        None
    }
}
