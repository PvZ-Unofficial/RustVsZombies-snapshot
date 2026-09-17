#![allow(dead_code)]

use rsvz_abi_macros::pvz_abi_fn;

pvz_abi_fn! {
    board_stage_has_pool(board: *mut u8) -> u8 {
        addr: 0x41c0d0,
        this: eax = board,
        clobber: [eax],
        ret: al,
    }
}

fn main() {
    std::hint::black_box(board_stage_has_pool as unsafe fn(*mut u8) -> u8);
}
