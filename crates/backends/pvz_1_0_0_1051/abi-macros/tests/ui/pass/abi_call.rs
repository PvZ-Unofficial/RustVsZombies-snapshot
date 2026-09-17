#![allow(dead_code, unused_assignments, unused_variables)]

use rsvz_abi_macros::pvz_abi_call;

unsafe fn direct_call(board: *mut u8, row: i32) -> i32 {
    let result: i32;
    unsafe {
        pvz_abi_call! {
            addr: 0x401000,
            this: ecx = board,
            regs: { eax = row },
            stack: [7i32],
            clobber: [eax, ecx, edx],
            ret: eax => result,
        }
    }
    result
}

fn main() {}
