use super::{
    Board, DataArray, ParticleSystem, Reanimation, SeedChooserScreen, TopMouseWindow, addr_at_mut, c_void, ptr,
    read_unaligned_at, write_unaligned_at,
};

#[repr(C)]
pub(crate) struct Challenge {
    _data: [u8; 0],
}

impl Challenge {
    #[inline(always)]
    pub(crate) unsafe fn endless_rounds(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x6c) }
    }
    #[inline(always)]
    pub(crate) unsafe fn endless_rounds_mut(this: *mut Self) -> *mut i32 {
        unsafe { addr_at_mut(this, 0x6c) }
    }
}

#[repr(C)]
pub(crate) struct CursorPreview {
    _data: [u8; 0],
}

#[repr(C)]
pub(crate) struct EffectSystem {
    _data: [u8; 0],
}

impl EffectSystem {
    #[inline(always)]
    pub(crate) unsafe fn particle_system_holder(this: *const Self) -> *mut ParticleSystemHolder {
        unsafe { read_unaligned_at(this, 0x0) }
    }
    #[inline(always)]
    pub(crate) unsafe fn reanimation_holder(this: *const Self) -> *mut ReanimationHolder {
        unsafe { read_unaligned_at(this, 0x8) }
    }
}

#[repr(C)]
pub(crate) struct ParticleSystemHolder {
    _data: [u8; 0],
}

impl ParticleSystemHolder {
    #[inline(always)]
    pub(crate) unsafe fn particle_systems(this: *const Self) -> *mut ParticleSystem {
        unsafe { read_unaligned_at(this, 0x0) }
    }
    #[inline(always)]
    pub(crate) unsafe fn particle_system_count_max(this: *const Self) -> u32 {
        unsafe { read_unaligned_at(this, 0x4) }
    }
}

#[repr(C)]
pub(crate) struct GameButton {
    _data: [u8; 0],
}

impl GameButton {
    #[inline(always)]
    pub(crate) unsafe fn btn_no_draw(this: *const Self) -> bool {
        unsafe { read_unaligned_at::<u8>(this, 0xf9) != 0 }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_btn_no_draw(this: *mut Self, value: bool) {
        unsafe { write_unaligned_at(this, 0xf9, u8::from(value)) }
    }
}

#[repr(C)]
pub(crate) struct LawnApp {
    _data: [u8; 0],
}

impl LawnApp {
    #[inline(always)]
    pub(crate) unsafe fn mouse_window(this: *const Self) -> *mut MouseWindow {
        unsafe { read_unaligned_at(this, 0x320) }
    }
    // 1051 evidence: SexyAppBase::mHWnd, AvZ2 APvzBase::Hwnd(), and AvZ1 address tables
    // all identify the main PvZ window handle at LawnApp + 0x350.
    #[inline(always)]
    pub(crate) unsafe fn hwnd(this: *const Self) -> *mut c_void {
        unsafe { read_unaligned_at(this, 0x350) }
    }
    #[inline(always)]
    pub(crate) unsafe fn tick_ms(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x454) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_tick_ms(this: *mut Self, value: i32) {
        unsafe { write_unaligned_at(this, 0x454, value) }
    }
    #[inline(always)]
    pub(crate) unsafe fn update_multiplier(this: *const Self) -> f64 {
        unsafe { read_unaligned_at(this, 0x490) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_update_multiplier(this: *mut Self, value: f64) {
        unsafe { write_unaligned_at(this, 0x490, value) }
    }
    #[inline(always)]
    pub(crate) unsafe fn board(this: *const Self) -> *mut Board {
        unsafe { read_unaligned_at(this, 0x768) }
    }
    #[inline(always)]
    pub(crate) unsafe fn seed_chooser(this: *const Self) -> *mut SeedChooserScreen {
        unsafe { read_unaligned_at(this, 0x774) }
    }
    #[inline(always)]
    pub(crate) unsafe fn clear_seed_chooser(this: *mut Self) {
        unsafe { write_unaligned_at(this, 0x774, ptr::null_mut::<SeedChooserScreen>()) }
    }
    #[inline(always)]
    pub(crate) unsafe fn game_mode(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x7f8) }
    }
    #[inline(always)]
    pub(crate) unsafe fn game_ui(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x7fc) }
    }
    // Objdump SeedPacket::CanPickUp/MouseDown: LawnApp+0x814 gates EasyPlantingCheat.
    #[inline(always)]
    pub(crate) unsafe fn is_easy_planting_cheat_enabled(this: *const Self) -> bool {
        unsafe { read_unaligned_at::<u8>(this, 0x814) != 0 }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_easy_planting_cheat(this: *mut Self, value: bool) {
        unsafe { write_unaligned_at(this, 0x814, u8::from(value)) }
    }
    #[inline(always)]
    pub(crate) unsafe fn effect_system(this: *const Self) -> *mut EffectSystem {
        unsafe { read_unaligned_at(this, 0x820) }
    }
    #[inline(always)]
    pub(crate) unsafe fn player_info(this: *const Self) -> *mut PlayerInfo {
        unsafe { read_unaligned_at(this, 0x82c) }
    }
    #[inline(always)]
    pub(crate) unsafe fn app_counter(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x838) }
    }
    #[inline(always)]
    pub(crate) unsafe fn app_counter_mut(this: *mut Self) -> *mut i32 {
        unsafe { addr_at_mut(this, 0x838) }
    }
    #[inline(always)]
    pub(crate) unsafe fn music(this: *const Self) -> *mut u8 {
        unsafe { read_unaligned_at(this, 0x83c) }
    }
    #[inline(always)]
    pub(crate) unsafe fn app_rand_seed(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x870) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_app_rand_seed(this: *mut Self, value: i32) {
        unsafe { write_unaligned_at(this, 0x870, value) }
    }
}

#[repr(C)]
pub(crate) struct MouseWindow {
    _data: [u8; 0],
}

impl MouseWindow {
    #[inline(always)]
    pub(crate) unsafe fn top_window(this: *const Self) -> *mut TopMouseWindow {
        unsafe { read_unaligned_at(this, 0x94) }
    }
    #[inline(always)]
    pub(crate) unsafe fn mouse_abscissa(this: *mut Self) -> *mut i32 {
        unsafe { addr_at_mut(this, 0xe0) }
    }
    #[inline(always)]
    pub(crate) unsafe fn mouse_ordinate(this: *mut Self) -> *mut i32 {
        unsafe { addr_at_mut(this, 0xe4) }
    }
}

#[repr(C)]
pub(crate) struct PlayerInfo {
    _data: [u8; 0],
}

impl PlayerInfo {
    #[inline(always)]
    pub(crate) unsafe fn id(this: *const Self) -> u32 {
        unsafe { read_unaligned_at(this, 0x20) }
    }
    #[inline(always)]
    pub(crate) unsafe fn level(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x24) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_level(this: *mut Self, value: i32) {
        unsafe { write_unaligned_at(this, 0x24, value) }
    }
}

#[repr(C)]
pub(crate) struct ReanimationHolder {
    _data: [u8; 0],
}

impl ReanimationHolder {
    #[inline(always)]
    pub(crate) unsafe fn reanimations(this: *mut Self) -> *mut DataArray<Reanimation> {
        unsafe { addr_at_mut(this, 0x0) }
    }
}

#[repr(C)]
pub(crate) struct WidgetContainer {
    _data: [u8; 0],
}
