#![allow(dead_code, unused_variables)]

use rsvz_abi_macros::pvz_abi_call;

unsafe fn pre_new_game(app: usize, look_for_saved_game: i32, game_mode: i32) {
    unsafe {
        pvz_abi_call! {
            addr: 0x44f560,
            this: esi = app,
            stack: [look_for_saved_game, game_mode],
            clobber: [eax, ecx, edx],
        }
    }
}

fn main() {}
