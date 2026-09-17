use rsvz_abi_macros::pvz_abi_fn;

use crate::raw::layout::{Plant, Zombie};
use crate::raw::types::RawRect;

pvz_abi_fn! {
    zombie_die_no_loot(zombie: *mut Zombie) {
        addr: 0x530510,
        this: ecx = zombie,
        clobber: [eax, ecx, edx],
    }

    zombie_die_with_loot(zombie: *mut Zombie) {
        addr: 0x5302f0,
        this: ecx = zombie,
        clobber: [eax, ecx, edx],
    }

    zombie_effected_by_damage(zombie: *mut Zombie, flags: u32) -> u8 {
        addr: 0x531a80,
        this: esi = zombie,
        stack: [flags],
        clobber: [eax, ecx, edx],
        ret: al,
    }

    zombie_get_zombie_rect(zombie: *mut Zombie, out_rect: *mut RawRect) -> *mut RawRect {
        addr: 0x5320b0,
        this: ebx = zombie,
        regs: { edi = out_rect },
        clobber: [eax, ecx, edx],
        ret: eax,
    }

    // Objdump 0x532140: EBX=Zombie*, EDI=out Rect, EAX returns the same out pointer.
    zombie_get_attack_rect(zombie: *mut Zombie, out_rect: *mut RawRect) -> *mut RawRect {
        addr: 0x532140,
        this: ebx = zombie,
        regs: { edi = out_rect },
        clobber: [eax, ecx, edx],
        ret: eax,
    }

    // Objdump 0x52e4c0: ECX=Zombie*, stack@4=Plant*, stack@8=attack type, `ret 8`.
    zombie_can_target_plant(zombie: *mut Zombie, plant: *mut Plant, attack_type: i32) -> u8 {
        addr: 0x52e4c0,
        this: ecx = zombie,
        stack: [attack_type, plant],
        clobber: [eax, ecx, edx],
        ret: al,
    }

    // Objdump 0x534700: EAX=Zombie*, AL boolean result.
    zombie_is_dead_or_dying(zombie: *mut Zombie) -> u8 {
        addr: 0x534700,
        this: eax = zombie,
        clobber: [eax, ecx, edx],
        ret: al,
    }

    // Objdump 0x531880: EAX=Zombie*, stack@4=row, x87 ST0 return, `ret 4`.
    zombie_get_pos_y_based_on_row(zombie: *mut Zombie, row: i32) -> f32 {
        addr: 0x531880,
        this: eax = zombie,
        stack: [row],
        clobber: [eax, ecx, edx],
        ret: st0,
    }

    zombie_play_zombie_appear_sound(zombie: *mut Zombie) {
        addr: 0x530640,
        this: ecx = zombie,
        clobber: [eax, ecx, edx],
    }
}
