#![allow(dead_code, unused_assignments, unused_variables)]

use rsvz_abi_macros::pvz_abi_call;

unsafe fn saved_gp_return(value: u32) -> u32 {
    let out: u32;
    unsafe {
        pvz_abi_call! {
            addr: 1,
            regs: { esi = value },
            clobber: [eax, esi],
            ret: esi => out,
        }
    }
    out
}

unsafe fn saved_byte_return(value: u8) -> u8 {
    let out: u8;
    unsafe {
        pvz_abi_call! {
            addr: 1,
            regs: { bl = value },
            clobber: [eax, ebx],
            ret: bl => out,
        }
    }
    out
}

fn main() {}
