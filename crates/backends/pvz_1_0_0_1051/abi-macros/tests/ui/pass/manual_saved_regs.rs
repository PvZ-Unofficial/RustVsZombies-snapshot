#![allow(dead_code, unused_variables)]

use rsvz_abi_macros::pvz_abi_call;

unsafe fn saved_regs(row: i32, col: i32, lane: i32, challenge: i32) {
    unsafe {
        pvz_abi_call! {
            addr: 0x426620,
            regs: { edi = row, ebx = col, esi = lane },
            stack: [challenge],
            clobber: [eax, ecx, edx],
        }
    }
}

fn main() {}
