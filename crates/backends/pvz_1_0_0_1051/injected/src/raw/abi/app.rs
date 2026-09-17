use rsvz_abi_macros::pvz_abi_fn;

use crate::raw::layout as ptrs;

pvz_abi_fn! {
    // lawn_app_* / lawn_*

    lawn_app_check_for_game_end() {
        addr: 0x4524f0,
        this: eax = [0x6a9ec0],
        clobber: [eax, ecx, edx],
    }

    lawn_app_do_back_to_main() {
        addr: 0x44feb0,
        this: eax = [0x6a9ec0],
        clobber: [eax, edx],
    }

    lawn_app_kill_challenge_screen() {
        addr: 0x44fd00,
        this: esi = [0x6a9ec0],
        clobber: [eax, ecx, edx],
    }

    lawn_app_kill_game_selector() {
        addr: 0x44f9e0,
        this: esi = [0x6a9ec0],
        clobber: [eax, ecx, edx],
    }

    lawn_app_play_foley(foley_type: i32) {
        addr: 0x453630,
        this: eax = ptrs::lawn_app(),
        regs: { esi = foley_type },
        clobber: [eax, ecx, edx, esi],
    }

    lawn_app_play_music(music_id: i32) {
        addr: 0x45b750,
        this: eax = ptrs::LawnApp::music(ptrs::lawn_app()),
        regs: { edi = music_id },
        clobber: [eax, ecx, edx],
    }

    lawn_app_loading_completed() {
        addr: 0x452cb0,
        this: ecx = [0x6a9ec0],
        clobber: [eax, ecx, edx],
    }

    lawn_app_play_sample(idx: i32) {
        addr: 0x4560c0,
        this: ecx = [0x6a9ec0],
        stack: [idx],
        clobber: [eax, ecx, edx],
    }

    lawn_app_button_depress(button_id: i32) {
        addr: 0x4531e0,
        this: ecx = [0x6a9ec0],
        stack: [button_id],
        clobber: [eax, ecx, edx],
    }

    lawn_app_kill_dialog(dialog_id: i32) -> u8 {
        addr: 0x451800,
        this: ecx = [0x6a9ec0],
        stack: [dialog_id],
        clobber: [eax, ecx, edx],
        ret: al,
    }

    lawn_app_pre_new_game(look_for_saved_game: i32, game_mode: i32) {
        addr: 0x44f560,
        this: esi = [0x6a9ec0],
        stack: [look_for_saved_game, game_mode],
        clobber: [eax, ecx, edx],
    }

    // Objdump 0x44f890: EAX=LawnApp*. This is the board-replacing
    // NewGame body; unlike PreNewGame(false), it does not erase a save.
    lawn_app_new_game() {
        addr: 0x44f890,
        this: eax = [0x6a9ec0],
        clobber: [eax, ecx, edx],
    }

    lawn_app_start_playing() {
        addr: 0x44f6b0,
        this: esi = [0x6a9ec0],
        clobber: [eax, ecx, edx],
    }

    lawn_app_try_load_file() -> u8 {
        addr: 0x44f7a0,
        stack: [ptrs::lawn_app()],
        clobber: [eax, ecx, edx],
        ret: al,
    }

    lawn_app_update_app() {
        addr: 0x453a50,
        this: ecx = [0x6a9ec0],
        clobber: [eax, ecx, edx],
    }

    lawn_app_update_frames() {
        addr: 0x452650,
        this: ecx = [0x6a9ec0],
        clobber: [eax, ecx, edx],
    }
}
