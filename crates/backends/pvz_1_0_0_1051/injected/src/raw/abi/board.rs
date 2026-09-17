use rsvz_abi_macros::pvz_abi_fn;

use crate::raw::layout::{Board, GridItem, Plant, Zombie};

use crate::raw::layout as ptrs;

pvz_abi_fn! {
    // board_*

    // Objdump 0x41AFD0: BL=enabled, Board* at stack@4, ret 4; EBX is preserved.
    board_set_dance_mode(board: *mut Board, enabled: i32) {
        addr: 0x41afd0,
        regs: { ebx = enabled },
        stack: [board],
        clobber: [eax, ecx, edx],
    }

    // Objdump 0x0040D120: reads row from eax, board/col/type/imitater from stack, moves the new
    // Plant* into eax before `ret 10h`.
    board_add_plant_raw(row: i32, col: i32, plant_type: i32, imitator_type: i32) -> u32 {
        addr: 0x40d120,
        regs: { eax = row },
        stack: [imitator_type, plant_type, col, ptrs::board()],
        clobber: [eax, ecx, edx],
        ret: eax,
    }

    board_add_ladder(row: i32, col: i32) -> *mut GridItem {
        addr: 0x408f40,
        this: eax = ptrs::board(),
        regs: { edi = row },
        stack: [col],
        clobber: [eax, ecx, edx],
        ret: eax,
    }

    board_add_crater(row: i32, col: i32) -> *mut GridItem {
        addr: 0x408f80,
        this: eax = ptrs::board(),
        regs: { edi = row },
        stack: [col],
        clobber: [eax, ecx, edx],
        ret: eax,
    }

    board_new_plant(row: i32, col: i32, plant_type: i32, imitator_type: i32) -> *mut Plant {
        addr: 0x40ce20,
        this: eax = ptrs::board(),
        stack: [imitator_type, plant_type, row, col],
        clobber: [eax, ecx, edx],
        ret: eax,
    }

    // Objdump 0x40ddc0: EAX=Board*, EBX=from_wave, stack@4=type, stack@8=row,
    // nullable Zombie* in EAX, `ret 8`.
    board_add_zombie_in_row(board: *mut Board, zombie_type: i32, row: i32, from_wave: i32) -> *mut Zombie {
        addr: 0x40ddc0,
        this: eax = board,
        regs: { ebx = from_wave },
        stack: [row, zombie_type],
        clobber: [eax, ecx, edx],
        ret: eax,
    }

    board_can_plant_at(card_type: i32, row: i32, col: i32) -> i32 {
        addr: 0x40e020,
        regs: { eax = row },
        stack: [card_type, col, ptrs::board()],
        clobber: [eax, ecx, edx],
        ret: eax,
    }

    board_get_current_plant_cost(plant_type: i32, imitator_type: i32) -> i32 {
        addr: 0x41dae0,
        this: edi = [0x6a9ec0, 0x768],
        regs: { eax = plant_type, edx = imitator_type },
        clobber: [eax, ecx, edx],
        ret: eax,
    }

    // Objdump 0x0041BA60: reads amount from ebx and Board* from edi, returns success in al.
    board_take_sun_money(board: *mut Board, amount: i32) -> u8 {
        addr: 0x41ba60,
        regs: { ebx = amount, edi = board },
        clobber: [eax, ecx, edx],
        ret: al,
    }

    // Objdump 0x0041BAB0: reads Board* from edx and amount from stack@4, returns al, `ret 4`.
    board_can_take_sun_money(board: *mut Board, amount: i32) -> u8 {
        addr: 0x41bab0,
        this: edx = board,
        stack: [amount],
        clobber: [ecx],
        ret: al,
    }

    board_grid_to_pixel_x(row: i32, col: i32) -> i32 {
        addr: 0x41c680,
        this: ecx = [0x6a9ec0, 0x768],
        regs: { eax = col, esi = row },
        clobber: [eax, ecx, edx],
        ret: eax,
    }

    board_grid_to_pixel_y(row: i32, col: i32) -> i32 {
        addr: 0x41c740,
        this: ebx = [0x6a9ec0, 0x768],
        regs: { ecx = col, eax = row },
        clobber: [eax, ecx, edx],
        ret: eax,
    }

    // Objdump 0x40ce00: EDX=Board*, EAX=grid_x, ECX=grid_y, boolean in AL.
    board_is_pool_square(board: *mut Board, grid_x: i32, grid_y: i32) -> u8 {
        addr: 0x40ce00,
        this: edx = board,
        regs: { eax = grid_x, ecx = grid_y },
        clobber: [eax, ecx],
        ret: al,
    }

    // Objdump 0x41c6c0: EAX=Board*, ECX=row, stack@4=float pos_x, x87 ST0 return.
    board_get_pos_y_based_on_row(board: *mut Board, pos_x: f32, row: i32) -> f32 {
        addr: 0x41c6c0,
        this: eax = board,
        regs: { ecx = row },
        stack: [pos_x],
        clobber: [eax, ecx, edx],
        ret: st0,
    }

    board_pixel_to_grid_x_keep_on_board(board: *mut Board, x: i32, y: i32) -> i32 {
        addr: 0x41c530,
        this: ebx = board,
        regs: { esi = x, eax = y },
        clobber: [eax, ecx, edx],
        ret: eax,
    }

    board_pixel_to_grid_y_keep_on_board(board: *mut Board, x: i32, y: i32) -> i32 {
        addr: 0x41c650,
        this: ebx = board,
        regs: { eax = x, edi = y },
        clobber: [eax, ecx, edx],
        ret: eax,
    }

    board_init_lawn_mowers(board: *mut Board) {
        addr: 0x40bc70,
        stack: [board],
        clobber: [eax, ecx, edx],
    }

    // Objdump 0x40abb0: EAX=Board*. Rebuilds allowed zombie types,
    // the spawn list, and the initial wave countdowns.
    board_init_zombie_waves(board: *mut Board) {
        addr: 0x40abb0,
        this: eax = board,
        clobber: [eax, ecx, edx],
    }

    board_mouse_down(board: *mut Board, x: i32, y: i32, click_count: i32) {
        addr: 0x411f20,
        this: ecx = board,
        stack: [click_count, y, x],
        clobber: [eax, ecx, edx],
    }

    board_mouse_down_with_plant(x: i32, y: i32, click_count: i32) {
        addr: 0x40fd30,
        regs: { ecx = click_count },
        stack: [y, x, ptrs::board()],
        clobber: [eax, ecx, edx],
    }

    board_mouse_down_with_tool(x: i32, y: i32, click_count: i32, cursor_type: i32) {
        addr: 0x411060,
        regs: { eax = [0x6a9ec0, 0x768], edx = x, ecx = y },
        stack: [cursor_type, click_count],
        clobber: [eax, ecx, edx],
    }

    board_pick_zombie_waves() {
        addr: 0x4092e0,
        this: edi = [0x6a9ec0, 0x768],
        clobber: [eax, ecx, edx],
    }

    board_load_background_images(board: *mut Board) {
        addr: 0x40a160,
        this: esi = board,
        clobber: [eax, ecx, edx],
    }

    board_process_delete_queue() {
        addr: 0x41bad0,
        this: esi = [0x6a9ec0, 0x768],
        clobber: [eax, ecx, edx],
    }

    board_remove_cutscene_zombies() {
        addr: 0x40df70,
        this: ebx = [0x6a9ec0, 0x768],
        clobber: [eax, ecx, edx],
    }

    // Objdump 0x416110: ECX=Board*, EAX=row, boolean in AL.
    board_row_can_have_zombies(row: i32) -> u8 {
        addr: 0x416110,
        this: ecx = [0x6a9ec0, 0x768],
        regs: { eax = row },
        clobber: [eax, edx],
        ret: al,
    }

    board_stage_has_grave_stones() -> u8 {
        addr: 0x41c040,
        this: edx = [0x6a9ec0, 0x768],
        clobber: [eax, ecx],
        ret: al,
    }

    board_stage_has_pool() -> u8 {
        addr: 0x41c0d0,
        this: eax = [0x6a9ec0, 0x768],
        clobber: [eax],
        ret: al,
    }

    board_stage_has_roof() -> u8 {
        addr: 0x41c0b0,
        this: eax = [0x6a9ec0, 0x768],
        clobber: [eax],
        ret: al,
    }

    board_stage_is_night() -> u8 {
        addr: 0x41c010,
        this: eax = [0x6a9ec0, 0x768],
        clobber: [eax],
        ret: al,
    }

    board_total_zombies_health_in_wave(board: *mut Board, wave: i32) -> i32 {
        addr: 0x412e30,
        this: ebx = board,
        stack: [wave],
        clobber: [eax, ecx, edx],
        ret: eax,
    }

    board_try_save_file() {
        addr: 0x408c30,
        stack: [ptrs::board()],
        clobber: [eax, ecx, edx],
    }

    // Objdump and decompiled source both identify Board::Update as void. EAX is caller-clobbered
    // residue and must never be exposed as a success/game-over boolean.
    board_update() {
        addr: 0x415d40,
        this: ecx = [0x6a9ec0, 0x768],
        clobber: [eax, ecx, edx],
    }

    board_update_mouse_position() {
        addr: 0x40eab0,
        this: eax = [0x6a9ec0, 0x768],
        clobber: [eax, ecx, edx],
    }

    // Objdump 0x41CBF0: Board* in EDI, explosion y in EBX, x at stack@4, `ret 4`.
    board_kill_all_plants_in_radius(board: *mut Board, x: i32, y: i32) {
        addr: 0x41cbf0,
        regs: { edi = board, ebx = y },
        stack: [x],
        clobber: [eax, ecx, edx],
    }

}

pub(crate) unsafe fn board_add_plant(row: i32, col: i32, plant_type: i32, imitator_type: i32) -> *mut Plant {
    // SAFETY: the caller upholds the raw wrapper contract; PvZ returns its 32-bit Plant pointer in EAX.
    unsafe { board_add_plant_raw(row, col, plant_type, imitator_type) as usize as *mut Plant }
}
