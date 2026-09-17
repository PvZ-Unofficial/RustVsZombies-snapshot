use rsvz_abi_macros::pvz_abi_fn;

pvz_abi_fn! {
    board_update(_: *mut u8) {
        addr: 1,
    }
}

fn main() {}
