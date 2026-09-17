use rsvz_abi_macros::pvz_abi_fn;

use crate::raw::layout::ParticleSystem;

use crate::raw::layout as ptrs;

pvz_abi_fn! {
    // effect_*

    effect_system_process_delete_queue() {
        addr: 0x445680,
        stack: [ptrs::LawnApp::effect_system(ptrs::lawn_app())],
        clobber: [eax, ecx, edx],
    }

    particle_system_delete(particle_system: *mut ParticleSystem) {
        addr: 0x5160c0,
        stack: [particle_system],
        clobber: [eax, ecx, edx],
    }

}
