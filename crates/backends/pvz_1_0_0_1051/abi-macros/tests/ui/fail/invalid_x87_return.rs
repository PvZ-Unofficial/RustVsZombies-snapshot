use rsvz_abi_macros::pvz_abi_call;

fn main() {
    let out: f32;
    unsafe {
        pvz_abi_call! {
            addr: 0x41c6c0,
            ret: st0 => out,
        }
    }
}
