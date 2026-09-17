use std::marker::PhantomData;
use std::ptr::NonNull;

use crate::PortableBackend;

macro_rules! handle {
    ($name:ident, $raw:ty) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub struct $name<'a> {
            ptr: NonNull<$raw>,
            _borrow: PhantomData<&'a PortableBackend>,
        }

        impl<'a> $name<'a> {
            pub(crate) fn new(ptr: NonNull<$raw>) -> Self {
                Self {
                    ptr,
                    _borrow: PhantomData,
                }
            }

            pub(crate) const fn as_ptr(self) -> *const $raw {
                self.ptr.as_ptr()
            }

            pub(crate) const fn as_non_null(self) -> NonNull<$raw> {
                self.ptr
            }
        }
    };
}

handle!(PortablePlantHandle, pvzp_rs::raw::pvzp_rs_plant);
handle!(PortableZombieHandle, pvzp_rs::raw::pvzp_rs_zombie);
handle!(PortableProjectileHandle, pvzp_rs::raw::pvzp_rs_projectile);
handle!(PortableGridItemHandle, pvzp_rs::raw::pvzp_rs_grid_item);
handle!(PortableItemHandle, pvzp_rs::raw::pvzp_rs_item);
handle!(PortableSeedHandle, pvzp_rs::raw::pvzp_rs_seed);

impl PortablePlantHandle<'_> {
    pub(crate) const fn as_mut_ptr(self) -> *mut pvzp_rs::raw::pvzp_rs_plant {
        self.ptr.as_ptr()
    }
}

impl PortableZombieHandle<'_> {
    pub(crate) const fn as_mut_ptr(self) -> *mut pvzp_rs::raw::pvzp_rs_zombie {
        self.ptr.as_ptr()
    }
}

macro_rules! iterator {
    ($name:ident, $handle:ident, $pool:ident, $method:ident, $dead:ident) => {
        pub struct $name<'a> {
            backend: &'a PortableBackend,
            next: u32,
            limit: u32,
        }

        impl<'a> $name<'a> {
            pub(crate) const fn new(backend: &'a PortableBackend, limit: u32) -> Self {
                Self {
                    backend: backend,
                    next: 0,
                    limit,
                }
            }
        }

        impl<'a> Iterator for $name<'a> {
            type Item = $handle<'a>;

            fn next(&mut self) -> Option<Self::Item> {
                while self.next < self.limit {
                    let index = self.next;
                    self.next += 1;
                    let ptr = self
                        .backend
                        .world()
                        .expect("iterator retains a live Board borrow")
                        .$method()
                        .get(index as i32)
                        .expect("validated iterator index and pool")
                        .map(pvzp_rs::Borrowed::as_non_null);
                    let Some(ptr) = ptr else {
                        continue;
                    };
                    // SAFETY: the pointer belongs to the current live pool and
                    // this reads one bindgen-verified scalar without a Rust reference.
                    let dead = unsafe { std::ptr::addr_of!((*ptr.as_ptr()).$dead).read() };
                    if !dead {
                        return Some($handle::new(ptr));
                    }
                }
                None
            }
        }
    };
}

iterator!(
    PortableGridItemIter,
    PortableGridItemHandle,
    GridItemPool,
    grid_items,
    mDead
);
iterator!(PortableItemIter, PortableItemHandle, ItemPool, items, mDead);

macro_rules! live_iterator {
    ($name:ident, $handle:ident, $pool:ident, $method:ident, $is_live:expr) => {
        pub struct $name<'a> {
            backend: &'a PortableBackend,
            next: u32,
            limit: u32,
        }

        impl<'a> $name<'a> {
            pub(crate) const fn new(backend: &'a PortableBackend, limit: u32) -> Self {
                Self {
                    backend: backend,
                    next: 0,
                    limit,
                }
            }
        }

        impl<'a> Iterator for $name<'a> {
            type Item = $handle<'a>;

            fn next(&mut self) -> Option<Self::Item> {
                while self.next < self.limit {
                    let index = self.next;
                    self.next += 1;
                    let ptr = self
                        .backend
                        .world()
                        .expect("iterator retains a live Board borrow")
                        .$method()
                        .get(index as i32)
                        .expect("validated iterator index and pool")
                        .map(pvzp_rs::Borrowed::as_non_null);
                    let Some(ptr) = ptr else {
                        continue;
                    };
                    let is_live = $is_live(self.backend, ptr);
                    if is_live {
                        return Some($handle::new(ptr));
                    }
                }
                None
            }
        }
    };
}

live_iterator!(
    PortableProjectileIter,
    PortableProjectileHandle,
    ProjectilePool,
    projectiles,
    |backend: &PortableBackend, ptr| {
        let handle = PortableProjectileHandle::new(ptr);
        // SAFETY: this handle names an occupied entry in the borrowed Board.
        !unsafe { std::ptr::addr_of!((*handle.as_ptr()).mDead).read() }
            && (rsvz_backend_api::ProjectileReadBackend::projectile_motion(backend, handle) != 9
                || rsvz_backend_api::ZombieReadBackend::zombie(
                    backend,
                    rsvz_model::ZombieId::from_raw(
                        rsvz_backend_api::ProjectileReadBackend::projectile_target_zombie_id(backend, handle),
                    ),
                )
                .expect("projectile iterator retains its Board")
                .is_some())
    }
);

live_iterator!(PortablePlantIter, PortablePlantHandle, PlantPool, plants, |_, ptr| {
    PortablePlantHandle::new(ptr).is_live()
});

live_iterator!(
    PortableZombieIter,
    PortableZombieHandle,
    ZombiePool,
    zombies,
    |_, ptr| { PortableZombieHandle::new(ptr).is_live() }
);

pub struct PortableSeedIter<'a> {
    backend: &'a PortableBackend,
    next: u32,
    count: u32,
}

impl<'a> PortableSeedIter<'a> {
    pub(crate) const fn new(backend: &'a PortableBackend, count: u32) -> Self {
        Self {
            backend: backend,
            next: 0,
            count,
        }
    }
}

impl<'a> Iterator for PortableSeedIter<'a> {
    type Item = PortableSeedHandle<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.count {
            return None;
        }
        let index = self.next;
        self.next += 1;
        let seed = self
            .backend
            .world()
            .expect("seed iterator retains a live Board borrow")
            .seed_at(index)
            .expect("seed iterator retains its bounded bank");
        Some(PortableSeedHandle::new(seed.as_non_null()))
    }
}

impl PortablePlantHandle<'_> {
    pub(crate) fn is_live(self) -> bool {
        let ptr = self.as_ptr();
        // SAFETY: handles name occupied slots for the current backend borrow.
        unsafe {
            !std::ptr::addr_of!((*ptr).mDead).read()
                && !std::ptr::addr_of!((*ptr).mSquished).read()
                && std::ptr::addr_of!((*ptr).mIsOnBoard).read()
                && std::ptr::addr_of!((*ptr).mOnBungeeState).read() != 2
                && !(std::ptr::addr_of!((*ptr).mSeedType).read() == 17 && std::ptr::addr_of!((*ptr).mState).read() == 7)
        }
    }
}

impl PortableZombieHandle<'_> {
    pub(crate) fn is_live(self) -> bool {
        let ptr = self.as_ptr();
        // SAFETY: handles name occupied slots for the current backend borrow.
        unsafe {
            !std::ptr::addr_of!((*ptr).mDead).read()
                && !matches!(std::ptr::addr_of!((*ptr).mZombiePhase).read(), 1..=3)
                && std::ptr::addr_of!((*ptr).mFromWave).read() >= 0
        }
    }
}
