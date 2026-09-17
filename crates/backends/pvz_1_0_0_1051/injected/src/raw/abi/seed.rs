use rsvz_abi_macros::pvz_abi_fn;

use crate::raw::layout::{ChosenSeed, SeedPacket};

use crate::raw::layout as ptrs;

pvz_abi_fn! {
    // seed_* / seed_chooser_* / seed_packet_*

    seed_bank_refresh_all_packets() {
        addr: 0x489d50,
        stack: [ptrs::Board::seed_bank(ptrs::board())],
        clobber: [eax, ecx, edx],
    }

    seed_chooser_clicked_seed_in_chooser(chosen_seed: *mut ChosenSeed) {
        addr: 0x486030,
        this: eax = [0x6a9ec0, 0x774],
        stack: [chosen_seed],
        clobber: [eax, ecx, edx],
    }

    seed_chooser_close_seed_chooser() {
        addr: 0x486d20,
        this: ebx = [0x6a9ec0, 0x774],
        clobber: [eax, ecx, edx],
    }

    seed_chooser_pick_random_seeds() {
        addr: 0x4859b0,
        stack: [ptrs::LawnApp::seed_chooser(ptrs::lawn_app())],
        clobber: [eax, ecx, edx],
    }

    seed_chooser_update_imitater_button() {
        addr: 0x4866e0,
        stack: [ptrs::LawnApp::seed_chooser(ptrs::lawn_app())],
        clobber: [eax, ecx, edx],
    }

    seed_packet_can_pick_up(seed_packet: *mut SeedPacket) -> u8 {
        addr: 0x488500,
        this: esi = seed_packet,
        clobber: [eax, edx],
        ret: al,
    }

    // Objdump 0x00488EC0: reads SeedPacket* from eax and returns normally without stack cleanup.
    seed_packet_was_planted(seed_packet: *mut SeedPacket) {
        addr: 0x488ec0,
        this: eax = seed_packet,
        clobber: [eax, ecx, edx],
    }
}
