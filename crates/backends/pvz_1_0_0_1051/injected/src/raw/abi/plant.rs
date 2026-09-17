use rsvz_abi_macros::pvz_abi_fn;

use crate::raw::layout::Plant;
use crate::raw::types::RawRect;

pvz_abi_fn! {
    // plant_*
    // Objdump 0x466B80 and UpdateImitater callsite 0x466D47:
    // Plant* in ESI, no stack arguments, callee preserves EBX/EDI/ESI.
    plant_imitater_morph(plant: *mut Plant) {
        addr: 0x466b80,
        this: esi = plant,
        clobber: [eax, ecx, edx],
    }

    plant_cob_cannon_fire(plant: *mut Plant, x: i32, y: i32) {
        addr: 0x466d50,
        this: eax = plant,
        stack: [y, x],
        clobber: [eax, ecx, edx],
    }

    plant_play_idle_anim(plant: *mut Plant, rate: f32) {
        addr: 0x468280,
        this: edi = plant,
        stack: [rate],
        clobber: [eax, ecx, edx],
    }

    plant_die(plant: *mut Plant) {
        addr: 0x4679b0,
        stack: [plant],
        clobber: [eax, ecx, edx],
    }

    // Objdump 0x45EC00: Plant* in ESI, no stack arguments.
    plant_spikerock_take_damage(plant: *mut Plant) {
        addr: 0x45ec00,
        this: esi = plant,
        clobber: [eax, ecx, edx],
    }

    // Objdump 0x462B80: Plant* at stack@4, `ret 4`.
    plant_squish(plant: *mut Plant) {
        addr: 0x462b80,
        stack: [plant],
        clobber: [eax, ecx, edx],
    }

    plant_update_reanim_color(plant: *mut Plant) {
        addr: 0x4635c0,
        stack: [plant],
        clobber: [eax, ecx, edx],
    }

    plant_set_sleeping(plant: *mut Plant, asleep: i32) {
        addr: 0x45e860,
        this: eax = plant,
        stack: [asleep],
        clobber: [eax, ecx, edx],
    }
    // Objdump 0x467ef0: EAX=out Rect, ECX=Plant*, EAX returns the same out pointer.
    plant_get_plant_rect(plant: *mut Plant, out_rect: *mut RawRect) -> *mut RawRect {
        addr: 0x467ef0,
        this: ecx = plant,
        regs: { eax = out_rect },
        clobber: [ecx, edx],
        ret: eax,
    }

    // Objdump 0x467f90: EAX=out Rect, ECX=Plant*, stack@4=PlantWeapon, `ret 4`.
    plant_get_attack_rect(plant: *mut Plant, weapon: i32, out_rect: *mut RawRect) -> *mut RawRect {
        addr: 0x467f90,
        this: ecx = plant,
        regs: { eax = out_rect },
        stack: [weapon],
        clobber: [ecx, edx],
        ret: eax,
    }

    // Objdump 0x45eb10: EAX=Plant*, stack@4=PlantWeapon, EAX result, `ret 4`.
    plant_get_damage_range_flags(plant: *mut Plant, weapon: i32) -> i32 {
        addr: 0x45eb10,
        this: eax = plant,
        stack: [weapon],
        clobber: [ecx, edx],
        ret: eax,
    }
}
