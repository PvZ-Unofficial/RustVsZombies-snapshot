#![allow(dead_code, unused_assignments, unused_variables)]

use rsvz_abi_macros::pvz_abi_call;

unsafe fn x87_call(app: usize, x: f64, y: f64) -> f32 {
    let ret: f32;
    unsafe {
        pvz_abi_call! {
            addr: 0x6398b0,
            this: esi = app,
            regs: { st1<f64> = x, st0<f64> = y },
            ret: st0<f32> => ret,
        }
    }
    ret
}

fn main() {}
