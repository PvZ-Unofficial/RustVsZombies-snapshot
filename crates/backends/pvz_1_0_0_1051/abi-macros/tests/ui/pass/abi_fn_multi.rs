#![allow(dead_code)]

use rsvz_abi_macros::pvz_abi_fn;

pvz_abi_fn! {
    board_update(board: *mut u8) {
        addr: 0x415d40,
        this: ecx = board,
        clobber: [eax, ecx, edx],
    }

    board_stage_has_pool(board: *mut u8) -> u8 {
        addr: 0x41c0d0,
        this: eax = board,
        clobber: [eax],
        ret: al,
    }
}

fn main() {}
