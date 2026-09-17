#![allow(dead_code)]

use rsvz_abi_macros::pvz_abi_fn;

pvz_abi_fn! {
    board_row_can_have_zombies(board: *mut u8, row: i32) -> u8 {
        addr: 0x416110,
        this: ecx = board,
        regs: { eax = row },
        clobber: [eax, edx],
        ret: al,
    }
}

fn main() {}
