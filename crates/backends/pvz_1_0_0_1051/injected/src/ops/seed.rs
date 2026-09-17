use std::ptr::{self, NonNull};

use crate::raw::kind::PvzPlantType;
use crate::raw::{abi, layout as ptrs};

pub(crate) const SEED_NONE: i32 = -1;

const IMITATOR_DIALOG_STATE_OFFSET: usize = 0x0c08;
const IMITATOR_DIALOG_TARGET_OFFSET: usize = 0x0c18;
const IMITATOR_DIALOG_STATE_PICKING_TARGET: u8 = 3;
const CHOSEN_SEED_SOURCE_PTR_OFFSET: usize = 0x0a0;
const CHOSEN_SEED_COPY_SOURCE_OFFSET: usize = 0x8;
const CHOSEN_SEED_COPY_TARGET_OFFSET: usize = 0x0be4;
const CHOSEN_SEED_COPY_LEN: usize = 8;

/// Selects a normal card in the PvZ 1.0.0.1051 seed chooser.
pub(crate) unsafe fn choose_card(seed_chooser: NonNull<ptrs::SeedChooserScreen>, card_type: PvzPlantType) {
    // SAFETY: the caller guarantees an active seed chooser; card_type indexes its fixed seed array.
    unsafe {
        let chosen_seed = ptrs::SeedChooserScreen::chosen_seed(seed_chooser.as_ptr(), card_type.raw() as usize);
        abi::seed_chooser_clicked_seed_in_chooser(chosen_seed);
    }
}

/// Selects an imitator target in the PvZ 1.0.0.1051 seed chooser.
pub(crate) unsafe fn choose_imitator_card(seed_chooser: NonNull<ptrs::SeedChooserScreen>, target: PvzPlantType) {
    debug_assert_ne!(target, PvzPlantType::Imitator);
    // SAFETY: the caller guarantees an active 1.0.0.1051 seed chooser; all offsets and copy lengths
    // below describe fields within that object and its selected-seed storage.
    unsafe {
        let seed_chooser = seed_chooser.as_ptr().cast::<u8>();
        write_seed_chooser_zero_extended_byte(
            seed_chooser,
            IMITATOR_DIALOG_STATE_OFFSET,
            IMITATOR_DIALOG_STATE_PICKING_TARGET,
        );
        write_seed_chooser_zero_extended_byte(seed_chooser, IMITATOR_DIALOG_TARGET_OFFSET, target.raw() as u8);

        let source = ptr::read_unaligned(seed_chooser.add(CHOSEN_SEED_SOURCE_PTR_OFFSET).cast::<*const u8>());
        let chosen_seed = seed_chooser.add(CHOSEN_SEED_COPY_TARGET_OFFSET);
        ptr::copy_nonoverlapping(
            source.add(CHOSEN_SEED_COPY_SOURCE_OFFSET),
            chosen_seed,
            CHOSEN_SEED_COPY_LEN,
        );

        abi::seed_chooser_clicked_seed_in_chooser(chosen_seed.cast::<ptrs::ChosenSeed>());
        abi::seed_chooser_update_imitater_button();
    }
}

unsafe fn write_seed_chooser_zero_extended_byte(seed_chooser: *mut u8, offset: usize, value: u8) {
    // SAFETY: the caller supplies a live seed chooser and a verified in-object byte-field offset.
    unsafe {
        *seed_chooser.add(offset) = value;
        seed_chooser.add(offset + 1).write_bytes(0, 3);
    }
}
