use rsvz_abi_macros::pvz_abi_call;

fn main() {
    let result: i32;
    unsafe {
        pvz_abi_call! {
            addr: 0x401000,
            call: eax,
            ret: eax => result,
        }
    }
}
