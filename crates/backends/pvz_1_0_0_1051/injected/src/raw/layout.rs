#![expect(
    dead_code,
    reason = "ptrs contains thin offset accessors that are wired incrementally by safe backend traits"
)]
#![expect(
    clippy::undocumented_unsafe_blocks,
    reason = "thin offset accessors share the module-level live-pointer and verified-offset contract"
)]

//! Typed access to the verified PvZ 1.0.0.1051 memory layout.
//!
//! Every accessor in this module is unsafe: its object pointer must be live and
//! its declared offset must match the running game version.

use std::ffi::{c_char, c_void};
use std::ptr::{self, NonNull};

const DATA_ARRAY_INDEX_MASK: u32 = 0xffff;
const DATA_ARRAY_KEY_MASK: u32 = 0xffff_0000;
const PLANT_TYPE_SQUASH: i32 = 17;
const PLANT_BUNGEE_STATE_GRABBED: i32 = 2;
const PLANT_STATE_SQUASH_DONE_FALLING: i32 = 7;

#[inline(always)]
unsafe fn addr_at<T>(this: *const impl Sized, offset: usize) -> *const T {
    unsafe { this.cast::<u8>().add(offset).cast::<T>() }
}

#[inline(always)]
unsafe fn addr_at_mut<T>(this: *mut impl Sized, offset: usize) -> *mut T {
    unsafe { this.cast::<u8>().add(offset).cast::<T>() }
}

#[inline(always)]
unsafe fn read_unaligned_at<T: Copy>(this: *const impl Sized, offset: usize) -> T {
    unsafe { ptr::read_unaligned(addr_at::<T>(this, offset)) }
}

#[inline(always)]
unsafe fn write_unaligned_at<T>(this: *mut impl Sized, offset: usize, value: T) {
    unsafe { ptr::write_unaligned(addr_at_mut::<T>(this, offset), value) }
}

pub(crate) const LAWN_APP_ROOT_ADDR: usize = 0x6a9ec0;
/// # Safety
/// The declared global root address must contain a valid PvZ 1.0.0.1051 object pointer.
#[inline(always)]
pub(crate) unsafe fn lawn_app() -> *mut LawnApp {
    unsafe { ptr::read_unaligned(LAWN_APP_ROOT_ADDR as *const *mut LawnApp) }
}

mod application;
mod board;
mod objects;

pub(crate) use application::*;
pub(crate) use board::*;
pub(crate) use objects::*;

#[repr(C)]
pub(crate) struct DataArray<T> {
    block: *mut T,
    max_used_count: u32,
    max_size: u32,
    free_list_head: u32,
    size: u32,
    next_key: u32,
    name: *const c_char,
}

#[inline(always)]
pub(crate) unsafe fn board() -> *mut Board {
    unsafe { LawnApp::board(lawn_app()) }
}

impl SeedPacket {
    /// Reproduces the state writes in the native `SeedPacket::Deactivate`.
    ///
    /// PvZ 1.0.0.1051 inlines this transition at `0x00488E71..=0x00488E7D`
    /// in `SeedPacket::MouseDown`.
    #[inline(always)]
    pub(crate) unsafe fn deactivate(this: *mut Self) {
        unsafe {
            Self::set_active(this, false);
            Self::set_refresh_counter(this, 0);
            Self::set_refresh_time(this, 0);
            Self::set_refreshing(this, false);
        }
    }

    #[inline(always)]
    pub(crate) unsafe fn is_usable(this: *const Self) -> bool {
        unsafe { Self::is_active(this) && !Self::is_refreshing(this) && Self::refresh_counter(this) <= 0 }
    }
}

impl Plant {
    #[inline(always)]
    pub(crate) unsafe fn is_alive(this: *const Self) -> bool {
        unsafe {
            !Self::is_dead(this)
                && !Self::is_squished(this)
                && Self::is_on_board(this)
                && Self::on_bungee_state(this) != PLANT_BUNGEE_STATE_GRABBED
                && (Self::seed_type(this) != PLANT_TYPE_SQUASH || Self::state(this) != PLANT_STATE_SQUASH_DONE_FALLING)
        }
    }
}

impl Zombie {
    #[inline(always)]
    pub(crate) unsafe fn is_alive(this: *const Self) -> bool {
        unsafe { !Self::is_disappeared(this) && !matches!(Self::phase(this), 1..=3) && Self::from_wave(this) >= 0 }
    }
}

impl<T> DataArray<T> {
    #[inline(always)]
    pub(crate) unsafe fn max_used_count(this: *const Self) -> u32 {
        unsafe { (*this).max_used_count }
    }

    #[inline(always)]
    pub(crate) unsafe fn max_size(this: *const Self) -> u32 {
        unsafe { (*this).max_size }
    }

    #[inline(always)]
    pub(crate) unsafe fn active_count(this: *const Self) -> u32 {
        unsafe { (*this).size }
    }

    #[inline(always)]
    pub(crate) unsafe fn free_list_head(this: *const Self) -> u32 {
        unsafe { (*this).free_list_head }
    }

    #[inline(always)]
    pub(crate) unsafe fn next_key(this: *const Self) -> u32 {
        unsafe { (*this).next_key }
    }

    /// Removes empty-pool free-list provenance without changing the generation key.
    pub(crate) unsafe fn reset_empty_allocator(this: *mut Self) -> bool {
        unsafe {
            if this.is_null() || (*this).size != 0 {
                return false;
            }
            (*this).max_used_count = 0;
            (*this).free_list_head = 0;
            true
        }
    }

    #[inline(always)]
    unsafe fn item_at(this: *const Self, index: u32) -> *mut T {
        unsafe { (*this).block.add(index as usize) }
    }

    #[inline(always)]
    pub(crate) unsafe fn next_allocation(this: *const Self) -> Option<NonNull<T>> {
        unsafe {
            if this.is_null()
                || (*this).block.is_null()
                || (*this).size >= (*this).max_size
                || (*this).free_list_head > (*this).max_used_count
                || (*this).free_list_head >= (*this).max_size
            {
                return None;
            }
            NonNull::new(Self::item_at(this, (*this).free_list_head))
        }
    }

    #[inline(always)]
    unsafe fn id_at(this: *const Self, index: u32) -> u32 {
        unsafe { Self::item_id(Self::item_at(this, index)) }
    }

    #[inline(always)]
    pub(crate) unsafe fn item_id(item: *const T) -> u32 {
        unsafe { Self::item_id_with_offset_from_end(item, size_of::<u32>()) }
    }

    #[inline(always)]
    unsafe fn item_id_with_offset_from_end(item: *const T, offset_from_end: usize) -> u32 {
        unsafe { ptr::read_unaligned(item.cast::<u8>().add(size_of::<T>() - offset_from_end).cast::<u32>()) }
    }

    #[inline(always)]
    pub(crate) unsafe fn occupied_item_at(this: *const Self, index: u32) -> Option<NonNull<T>> {
        unsafe {
            if this.is_null()
                || (*this).block.is_null()
                || index >= Self::max_used_count(this)
                || index >= Self::max_size(this)
            {
                return None;
            }
            let id = Self::id_at(this, index);
            if id & DATA_ARRAY_KEY_MASK == 0 {
                return None;
            }
            NonNull::new(Self::item_at(this, index))
        }
    }

    #[inline(always)]
    pub(crate) unsafe fn try_to_get(this: *const Self, id: u32) -> Option<NonNull<T>> {
        unsafe {
            if this.is_null() || (*this).block.is_null() || id & DATA_ARRAY_KEY_MASK == 0 {
                return None;
            }
            let index = id & DATA_ARRAY_INDEX_MASK;
            if index >= Self::max_used_count(this) || index >= Self::max_size(this) {
                return None;
            }
            let item_id = Self::id_at(this, index);
            // The requested ID already has a nonzero key; full equality implies it here.
            if item_id != id {
                return None;
            }
            NonNull::new(Self::item_at(this, index))
        }
    }
}

impl DataArray<LawnMower> {
    /// Reclaims one occupied mower slot after `LawnMower::Die` has released its reanimation.
    ///
    /// This is the mower-only portion of the source-level `DataArrayFree` used by
    /// `Board::ProcessDeleteQueue`. The 1051 mower type has no destructor beyond the explicit
    /// `Die` action, so freeing the slot only relinks the DataArray free list and decrements size.
    #[inline(always)]
    pub(crate) unsafe fn free_lawn_mower(this: *mut Self, mower: NonNull<LawnMower>) {
        unsafe {
            let id = Self::item_id(mower.as_ptr());
            debug_assert!(Self::try_to_get(this, id) == Some(mower));
            let index = id & DATA_ARRAY_INDEX_MASK;
            let id_ptr = mower
                .as_ptr()
                .cast::<u8>()
                .add(size_of::<LawnMower>() - size_of::<u32>())
                .cast::<u32>();
            ptr::write_unaligned(id_ptr, (*this).free_list_head);
            (*this).free_list_head = index;
            (*this).size -= 1;
        }
    }
}

impl DataArray<Coin> {
    const COIN_ID_OFFSET_FROM_END: usize = 8;

    #[inline(always)]
    unsafe fn coin_id_at(this: *const Self, index: u32) -> u32 {
        unsafe { Self::coin_item_id(Self::item_at(this, index)) }
    }

    #[inline(always)]
    pub(crate) unsafe fn coin_item_id(item: *const Coin) -> u32 {
        // PvZ 1.0.0.1051's Coin DataArray slots use a 0xd8 stride, but the live Coin ID is
        // stored at slot+0xd0 (AvZ AItem::Id = sizeof(AItem)-8). The final dword at slot+0xd4
        // is not the DataArray generation ID and may be zero or unrelated scratch/free-list data.
        unsafe { Self::item_id_with_offset_from_end(item, Self::COIN_ID_OFFSET_FROM_END) }
    }

    #[inline(always)]
    pub(crate) unsafe fn occupied_coin_at(this: *const Self, index: u32) -> Option<NonNull<Coin>> {
        unsafe {
            if this.is_null()
                || (*this).block.is_null()
                || index >= Self::max_used_count(this)
                || index >= Self::max_size(this)
            {
                return None;
            }
            let id = Self::coin_id_at(this, index);
            if id & DATA_ARRAY_KEY_MASK == 0 {
                return None;
            }
            NonNull::new(Self::item_at(this, index))
        }
    }

    #[inline(always)]
    pub(crate) unsafe fn try_to_get_coin(this: *const Self, id: u32) -> Option<NonNull<Coin>> {
        unsafe {
            if this.is_null() || (*this).block.is_null() || id & DATA_ARRAY_KEY_MASK == 0 {
                return None;
            }
            let index = id & DATA_ARRAY_INDEX_MASK;
            if index >= Self::max_used_count(this) || index >= Self::max_size(this) {
                return None;
            }
            let item_id = Self::coin_id_at(this, index);
            // The requested ID already has a nonzero key; full equality implies it here.
            if item_id != id {
                return None;
            }
            NonNull::new(Self::item_at(this, index))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[repr(C)]
    struct TestDataArraySlot {
        value: u32,
        id: u32,
    }

    fn test_data_array(
        slots: &mut [TestDataArraySlot], max_used_count: u32, max_size: u32,
    ) -> DataArray<TestDataArraySlot> {
        DataArray {
            block: slots.as_mut_ptr(),
            max_used_count,
            max_size,
            free_list_head: 0,
            size: 0,
            next_key: 0,
            name: ptr::null(),
        }
    }

    #[test]
    fn empty_allocator_reset_discards_only_slot_provenance() {
        let mut slots = [TestDataArraySlot { value: 7, id: 0 }];
        let mut array = test_data_array(&mut slots, 1, 1);
        array.free_list_head = 1;
        array.next_key = 77;

        // SAFETY: `array` is a live empty test DataArray.
        assert!(unsafe { DataArray::reset_empty_allocator(&raw mut array) });
        assert_eq!(array.max_used_count, 0);
        assert_eq!(array.free_list_head, 0);
        assert_eq!(array.next_key, 77);

        array.size = 1;
        array.max_used_count = 1;
        // SAFETY: the method must reject this deliberately nonempty fixture.
        assert!(!unsafe { DataArray::reset_empty_allocator(&raw mut array) });
        assert_eq!(array.max_used_count, 1);
    }

    #[test]
    fn seed_packet_deactivate_matches_native_mouse_down_transition() {
        // SAFETY: every stored field in this raw layout is an integer, float, raw pointer, or
        // byte-backed boolean, for which the all-zero bit pattern is valid.
        let mut packet = unsafe { std::mem::MaybeUninit::<SeedPacket>::zeroed().assume_init() };
        let packet = &raw mut packet;

        // SAFETY: `packet` remains live and aligned for the duration of these scalar accesses.
        unsafe {
            SeedPacket::set_active(packet, true);
            SeedPacket::set_refresh_counter(packet, 123);
            SeedPacket::set_refresh_time(packet, 456);
            SeedPacket::set_refreshing(packet, true);

            SeedPacket::deactivate(packet);

            assert!(!SeedPacket::is_active(packet));
            assert_eq!(SeedPacket::refresh_counter(packet), 0);
            assert_eq!(SeedPacket::refresh_time(packet), 0);
            assert!(!SeedPacket::is_refreshing(packet));
        }
    }

    #[test]
    fn safe_generation_lookup_rejects_untrusted_or_unoccupied_ids() {
        const LIVE_ID: u32 = 0x0001_0000;
        let mut slots = [
            TestDataArraySlot { value: 7, id: LIVE_ID },
            TestDataArraySlot { value: 9, id: 1 },
        ];
        let array = test_data_array(&mut slots, 2, 2);

        // SAFETY: `array` and its two backing slots remain alive for this test.
        unsafe {
            assert_eq!(
                DataArray::try_to_get(&raw const array, LIVE_ID).map(NonNull::as_ptr),
                Some(slots.as_mut_ptr())
            );
            assert!(DataArray::try_to_get(&raw const array, 0).is_none());
            assert!(DataArray::try_to_get(&raw const array, 1).is_none());
            assert!(DataArray::try_to_get(&raw const array, 0x0002_0000).is_none());
        }

        let mut free_slots = [TestDataArraySlot { value: 7, id: 0 }];
        let free_array = test_data_array(&mut free_slots, 1, 1);
        // SAFETY: `free_array` and its backing slot remain alive; the slot models a free-list entry
        // with no generation key.
        assert!(unsafe { DataArray::try_to_get(&raw const free_array, LIVE_ID) }.is_none());
    }

    #[test]
    fn safe_generation_lookup_checks_scan_limit_before_capacity() {
        const SECOND_SLOT_ID: u32 = 0x0001_0001;
        let mut slots = [
            TestDataArraySlot {
                value: 7,
                id: 0x0001_0000,
            },
            TestDataArraySlot {
                value: 9,
                id: SECOND_SLOT_ID,
            },
        ];
        let scan_limited = test_data_array(&mut slots, 1, 2);
        // SAFETY: both slots are allocated, but index 1 is outside the occupied scan limit.
        assert!(unsafe { DataArray::try_to_get(&raw const scan_limited, SECOND_SLOT_ID) }.is_none());

        let capacity_limited = test_data_array(&mut slots, 2, 1);
        // SAFETY: both slots are allocated for the test; the DataArray capacity deliberately excludes index 1.
        assert!(unsafe { DataArray::try_to_get(&raw const capacity_limited, SECOND_SLOT_ID) }.is_none());
    }

    #[test]
    fn coin_generation_lookup_uses_coin_id_offset_and_full_bounds() {
        const LIVE_ID: u32 = 0x0001_0000;
        let mut slots: [Coin; 1] = std::array::from_fn(|_| unsafe { std::mem::MaybeUninit::zeroed().assume_init() });
        unsafe {
            ptr::write_unaligned(
                slots
                    .as_mut_ptr()
                    .cast::<u8>()
                    .add(size_of::<Coin>() - DataArray::<Coin>::COIN_ID_OFFSET_FROM_END)
                    .cast::<u32>(),
                LIVE_ID,
            );
        }
        let array = DataArray {
            block: slots.as_mut_ptr(),
            max_used_count: 1,
            max_size: 1,
            free_list_head: 0,
            size: 1,
            next_key: 2,
            name: ptr::null(),
        };

        unsafe {
            assert!(DataArray::try_to_get_coin(&raw const array, LIVE_ID).is_some());
            assert!(DataArray::try_to_get_coin(&raw const array, 0).is_none());
            assert!(DataArray::try_to_get_coin(&raw const array, 1).is_none());
            assert!(DataArray::try_to_get_coin(&raw const array, 0x0002_0000).is_none());
        }

        let scan_limited = DataArray {
            max_used_count: 0,
            ..array
        };
        assert!(unsafe { DataArray::try_to_get_coin(&raw const scan_limited, LIVE_ID) }.is_none());
    }

    #[test]
    fn mower_free_reclaims_only_the_selected_slot() {
        const FIRST_ID: u32 = 0x0001_0000;
        const SECOND_ID: u32 = 0x0002_0001;
        let mut slots: [LawnMower; 2] = std::array::from_fn(|_| {
            // SAFETY: LawnMower is an opaque byte-backed raw layout, so all-zero is valid test data.
            unsafe { std::mem::MaybeUninit::zeroed().assume_init() }
        });
        for (slot, id) in slots.iter_mut().zip([FIRST_ID, SECOND_ID]) {
            // SAFETY: the trailing dword is the already-established mower DataArray generation ID.
            unsafe {
                ptr::write_unaligned(
                    (slot as *mut LawnMower)
                        .cast::<u8>()
                        .add(size_of::<LawnMower>() - size_of::<u32>())
                        .cast::<u32>(),
                    id,
                );
            }
        }
        let mut array = DataArray {
            block: slots.as_mut_ptr(),
            max_used_count: 2,
            max_size: 2,
            free_list_head: 2,
            size: 2,
            next_key: 3,
            name: ptr::null(),
        };
        let first = NonNull::from(&mut slots[0]);

        // SAFETY: `first` is the occupied slot identified by FIRST_ID in this live test array.
        unsafe { DataArray::free_lawn_mower(&raw mut array, first) };

        assert_eq!(array.size, 1);
        assert_eq!(array.free_list_head, 0);
        assert_eq!(unsafe { DataArray::item_id(first.as_ptr()) }, 2);
        assert!(unsafe { DataArray::try_to_get(&raw const array, FIRST_ID) }.is_none());
        assert_eq!(
            unsafe { DataArray::try_to_get(&raw const array, SECOND_ID) }.map(NonNull::as_ptr),
            Some(&raw mut slots[1])
        );
    }
}
