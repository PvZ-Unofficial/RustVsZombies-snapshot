use rsvz_abi_macros::pvz_abi_fn;

use crate::raw::layout as ptrs;
use crate::raw::layout::{Board, Coin, CutScene, GridItem, LawnMower, Projectile};
use crate::raw::types::RawRect;

// ABI wrappers for small board-owned objects: CursorObject, CursorPreview, Coin,
// LawnMower, GridItem, CutScene, and related DataArray helpers.
pvz_abi_fn! {
    // cursor_*

    cursor_object_update() {
        addr: 0x438780,
        this: esi = [0x6a9ec0, 0x768, 0x138],
        clobber: [eax, ecx, edx],
    }

    cursor_preview_update() {
        addr: 0x438da0,
        this: edi = [0x6a9ec0, 0x768, 0x13c],
        clobber: [eax, ecx, edx],
    }

    // coin_*

    coin_collect(coin: *mut Coin) {
        addr: 0x432060,
        this: ecx = coin,
        clobber: [eax, ecx, edx],
    }

    coin_play_collect_sound(coin: *mut Coin) {
        addr: 0x432b00,
        regs: { edx = coin },
        clobber: [eax, ecx, edx],
    }

    // cut_scene_*

    cut_scene_place_lawn_items(cut_scene: *mut CutScene) {
        addr: 0x43a690,
        this: esi = cut_scene,
        clobber: [eax, ecx, edx],
    }

    // data_array_*

    data_array_alloc_grid_item(board: *mut Board) -> *mut GridItem {
        addr: 0x41e1c0,
        this: esi = ptrs::Board::grid_items(board),
        clobber: [eax, ecx, edx],
        ret: eax,
    }

    // Objdump call sites load the exclusive upper bound into EAX and receive Rand(max) in EAX.
    tod_rand(max: i32) -> i32 {
        addr: 0x5af400,
        regs: { eax = max },
        clobber: [eax, ecx, edx],
        ret: eax,
    }

    // Objdump 0x5AF420 tail-jumps to MTRand::SRand with seed in ECX and the
    // process-global battle stream in EAX.
    sexy_srand(seed: u32) {
        addr: 0x5af420,
        regs: { ecx = seed },
        clobber: [eax, ecx, edx],
    }

    // grid_item_*

    grid_item_die(grid_item: *mut GridItem) {
        addr: 0x44d000,
        this: esi = grid_item,
        clobber: [eax, ecx, edx],
    }

    lawn_mower_die(mower: *mut LawnMower) {
        addr: 0x458d10,
        this: eax = mower,
        clobber: [eax, ecx, edx],
    }

    // Objdump 0x46ebc0: ESI=Projectile*, ECX=out Rect, EAX returns the same out pointer.
    projectile_get_projectile_rect(projectile: *mut Projectile, out_rect: *mut RawRect) -> *mut RawRect {
        addr: 0x46ebc0,
        this: esi = projectile,
        regs: { ecx = out_rect },
        clobber: [eax, ecx, edx],
        ret: eax,
    }
}
