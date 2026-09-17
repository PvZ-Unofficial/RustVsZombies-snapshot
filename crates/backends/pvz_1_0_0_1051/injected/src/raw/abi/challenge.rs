use rsvz_abi_macros::pvz_abi_fn;

pvz_abi_fn! {
    // challenge_*

    // Objdump 0x42a0f0: ECX=Challenge*, EAX=grid_y, stack@4=type, stack@8=grid_x,
    // source/assembly return is void (`ret 8`).
    challenge_izombie_place_zombie(row: i32, col: i32, zombie_type: i32) {
        addr: 0x42a0f0,
        this: ecx = [0x6a9ec0, 0x768, 0x160],
        regs: { eax = row },
        stack: [col, zombie_type],
        clobber: [eax, ecx, edx],
    }

    challenge_puzzle_next_stage_clear() {
        addr: 0x429e50,
        this: edi = [0x6a9ec0, 0x768, 0x160],
        clobber: [eax, ecx, edx],
    }

}
