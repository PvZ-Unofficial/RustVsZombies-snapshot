use rsvz_abi_macros::pvz_abi_fn;

pvz_abi_fn! {
    board_update(board: *mut u8) {
        addr: 1,
    }

    board_bad(_: *mut u8) {
        addr: 2,
    }
}

fn main() {}
