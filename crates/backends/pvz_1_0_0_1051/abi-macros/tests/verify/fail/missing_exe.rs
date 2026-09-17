use rsvz_abi_macros::pvz_abi_fn;

pvz_abi_fn! {
    retained_wrapper() {
        addr: 0x401000,
    }
}

fn main() {
    let _ = retained_wrapper as unsafe fn();
}
