use rsvz_abi_macros::pvz_abi_fn;

use crate::raw::layout as ptrs;
use crate::raw::layout::{MouseWindow, SeedChooserScreen};

pvz_abi_fn! {
    // widget_container_* / widget_manager_* / zombie_*

    widget_container_remove_widget(
        container: *mut MouseWindow,
        widget: *mut SeedChooserScreen,
    ) {
        addr: 0x5371c0,
        this: ecx = container,
        stack: [widget],
        clobber: [eax, ecx, edx],
    }

    // Objdump 0x54c74b..0x54c75e pushes LawnApp+0x320, calls 0x538eb0,
    // reads the bool result from AL, and 0x538eb0 returns with `ret 4`.
    widget_manager_draw_screen() -> u8 {
        addr: 0x538eb0,
        stack: [ptrs::LawnApp::mouse_window(ptrs::lawn_app())],
        clobber: [eax, ecx, edx],
        ret: al,
    }

    widget_manager_update_frame() {
        addr: 0x539140,
        this: edi = [0x6a9ec0, 0x320],
        clobber: [eax, ecx, edx],
    }
}
