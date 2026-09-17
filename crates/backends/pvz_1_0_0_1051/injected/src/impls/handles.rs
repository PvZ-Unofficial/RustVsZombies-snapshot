use std::marker::PhantomData;
use std::ptr::NonNull;

use rsvz_model::model::SeedSlot;

use crate::raw::layout as ptrs;
use crate::runtime::Pvz1051Backend;

// Raw handles stay Copy, but their borrow marker deliberately names the unique
// token so they inherit its main-thread affinity instead of relying on pointer
// auto-trait details.
macro_rules! plain_handle {
    ($name:ident, $raw:ty) => {
        #[derive(Clone, Copy)]
        pub struct $name<'a> {
            pub(crate) ptr: NonNull<$raw>,
            pub(crate) _marker: PhantomData<&'a Pvz1051Backend>,
        }

        impl<'a> $name<'a> {
            pub(crate) const fn new(ptr: NonNull<$raw>) -> Self {
                Self {
                    ptr,
                    _marker: PhantomData,
                }
            }
        }
    };
}

plain_handle!(PvzZombieHandle, ptrs::Zombie);
plain_handle!(PvzPlantHandle, ptrs::Plant);
plain_handle!(PvzGridItemHandle, ptrs::GridItem);
plain_handle!(PvzProjectileHandle, ptrs::Projectile);
plain_handle!(PvzItemHandle, ptrs::Coin);

#[derive(Clone, Copy)]
pub struct PvzSeedHandle<'a> {
    pub(crate) ptr: NonNull<ptrs::SeedPacket>,
    pub(crate) slot: SeedSlot,
    pub(crate) _marker: PhantomData<&'a Pvz1051Backend>,
}

pub struct PvzZombieIter<'a> {
    pub(crate) backend: &'a Pvz1051Backend,
    pub(crate) next: u32,
    pub(crate) max: u32,
}

pub struct PvzPlantIter<'a> {
    pub(crate) backend: &'a Pvz1051Backend,
    pub(crate) next: u32,
    pub(crate) max: u32,
}

pub struct PvzGridItemIter<'a> {
    pub(crate) backend: &'a Pvz1051Backend,
    pub(crate) next: u32,
    pub(crate) max: u32,
}

pub struct PvzProjectileIter<'a> {
    pub(crate) backend: &'a Pvz1051Backend,
    pub(crate) next: u32,
    pub(crate) max: u32,
}

pub struct PvzItemIter<'a> {
    pub(crate) backend: &'a Pvz1051Backend,
    pub(crate) next: u32,
    pub(crate) max: u32,
}

pub struct PvzSeedIter<'a> {
    pub(crate) backend: &'a Pvz1051Backend,
    pub(crate) next: usize,
    pub(crate) count: usize,
}

impl<'a> Iterator for PvzZombieIter<'a> {
    type Item = PvzZombieHandle<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let app = self.backend.app();
        // SAFETY: iterator construction checked Board and pool availability.
        // Its backend borrow excludes reclamation or replacement until it ends.
        let array = unsafe { NonNull::new_unchecked(ptrs::Board::zombies(ptrs::LawnApp::board(app.as_ptr()))) };
        // SAFETY: this iterator owns a live Board DataArray for its backend borrow.
        unsafe {
            next_matching(array, &mut self.next, self.max, |ptr| {
                ptrs::Zombie::is_alive(ptr.as_ptr())
            })
        }
        .map(PvzZombieHandle::new)
    }
}

impl<'a> Iterator for PvzPlantIter<'a> {
    type Item = PvzPlantHandle<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let app = self.backend.app();
        // SAFETY: iterator construction checked Board and pool availability.
        // Its backend borrow excludes reclamation or replacement until it ends.
        let array = unsafe { NonNull::new_unchecked(ptrs::Board::plants(ptrs::LawnApp::board(app.as_ptr()))) };
        // SAFETY: this iterator owns a live Board DataArray for its backend borrow.
        unsafe {
            next_matching(array, &mut self.next, self.max, |ptr| {
                ptrs::Plant::is_alive(ptr.as_ptr())
            })
        }
        .map(PvzPlantHandle::new)
    }
}

impl<'a> Iterator for PvzGridItemIter<'a> {
    type Item = PvzGridItemHandle<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let app = self.backend.app();
        // SAFETY: iterator construction checked Board and pool availability.
        // Its backend borrow excludes reclamation or replacement until it ends.
        let array = unsafe { NonNull::new_unchecked(ptrs::Board::grid_items(ptrs::LawnApp::board(app.as_ptr()))) };
        // SAFETY: this iterator owns a live Board DataArray for its backend borrow.
        unsafe {
            next_matching(array, &mut self.next, self.max, |ptr| {
                !ptrs::GridItem::is_dead(ptr.as_ptr())
                    && crate::ops::grid_item::grid_item_kind_from_raw(ptrs::GridItem::grid_item_type(ptr.as_ptr()))
                        .is_some()
            })
        }
        .map(PvzGridItemHandle::new)
    }
}

impl<'a> Iterator for PvzProjectileIter<'a> {
    type Item = PvzProjectileHandle<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let app = self.backend.app();
        // SAFETY: iterator construction checked Board and pool availability.
        // Its backend borrow excludes reclamation or replacement until it ends.
        let array = unsafe { NonNull::new_unchecked(ptrs::Board::projectiles(ptrs::LawnApp::board(app.as_ptr()))) };
        // SAFETY: this iterator owns a live Board DataArray for its backend borrow.
        unsafe {
            next_matching(array, &mut self.next, self.max, |ptr| {
                let handle = PvzProjectileHandle::new(ptr);
                !ptrs::Projectile::dead(ptr.as_ptr())
                    && (rsvz_backend_api::ProjectileReadBackend::projectile_motion(self.backend, handle) != 9
                        || rsvz_backend_api::ZombieReadBackend::zombie(
                            self.backend,
                            rsvz_model::ZombieId::from_raw(
                                rsvz_backend_api::ProjectileReadBackend::projectile_target_zombie_id(
                                    self.backend,
                                    handle,
                                ),
                            ),
                        )
                        .expect("projectile iterator retains its Board")
                        .is_some())
            })
        }
        .map(PvzProjectileHandle::new)
    }
}

impl<'a> Iterator for PvzItemIter<'a> {
    type Item = PvzItemHandle<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let app = self.backend.app();
        // SAFETY: iterator construction checked Board and pool availability.
        // Its backend borrow excludes reclamation or replacement until it ends.
        let array = unsafe { NonNull::new_unchecked(ptrs::Board::coins(ptrs::LawnApp::board(app.as_ptr()))) };
        while self.next < self.max {
            let index = self.next;
            self.next += 1;
            // SAFETY: `array` was obtained from a live board. `occupied_coin_at` bounds-checks the
            // DataArray slot and returns only occupied entries.
            let Some(ptr) = (unsafe { ptrs::DataArray::occupied_coin_at(array.as_ptr(), index) }) else {
                continue;
            };
            // SAFETY: `ptr` is an occupied DataArray item for the current board.
            if unsafe { ptrs::Coin::is_dead(ptr.as_ptr()) } {
                continue;
            }
            return Some(PvzItemHandle::new(ptr));
        }
        None
    }
}

unsafe fn next_matching<T>(
    array: NonNull<ptrs::DataArray<T>>, next: &mut u32, max: u32, mut matches: impl FnMut(NonNull<T>) -> bool,
) -> Option<NonNull<T>> {
    while *next < max {
        let index = *next;
        *next += 1;
        // SAFETY: the caller supplies a live DataArray and its captured scan bound.
        let Some(ptr) = (unsafe { ptrs::DataArray::occupied_item_at(array.as_ptr(), index) }) else {
            continue;
        };
        if matches(ptr) {
            return Some(ptr);
        }
    }
    None
}

impl<'a> Iterator for PvzSeedIter<'a> {
    type Item = PvzSeedHandle<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let app = self.backend.app();
        // SAFETY: iterator construction checked Board and pool availability.
        // Its backend borrow excludes reclamation or replacement until it ends.
        let bank = unsafe { NonNull::new_unchecked(ptrs::Board::seed_bank(ptrs::LawnApp::board(app.as_ptr()))) };
        if self.next >= self.count {
            return None;
        }
        let slot_index = self.next;
        self.next += 1;
        // SAFETY: `slot_index < count`, and `count` was clamped to the supported seed bank size.
        let ptr = unsafe { ptrs::SeedBank::packet(bank.as_ptr(), slot_index) };
        // SAFETY: packet is an embedded field and slot_index is bounded above.
        let ptr = unsafe { NonNull::new_unchecked(ptr) };
        Some(PvzSeedHandle {
            ptr,
            slot: SeedSlot::from_index_unchecked(slot_index),
            _marker: PhantomData,
        })
    }
}
