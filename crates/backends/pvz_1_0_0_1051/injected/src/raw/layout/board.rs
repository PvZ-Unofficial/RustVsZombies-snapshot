use super::{
    Challenge, CursorPreview, DataArray, GameButton, Plant, Projectile, SeedBank, Zombie, addr_at_mut,
    read_unaligned_at, write_unaligned_at,
};

#[repr(C)]
pub(crate) struct Board {
    _data: [u8; 0x57b0],
}

impl Board {
    #[inline(always)]
    pub(crate) unsafe fn draw_count(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x5570) }
    }
    #[inline(always)]
    pub(crate) unsafe fn lawn_mower_count(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x110) }
    }
    #[inline(always)]
    pub(crate) unsafe fn challenge(this: *const Self) -> *mut Challenge {
        unsafe { read_unaligned_at(this, 0x160) }
    }
    #[inline(always)]
    pub(crate) unsafe fn cursor_object(this: *const Self) -> *mut CursorObject {
        unsafe { read_unaligned_at(this, 0x138) }
    }
    #[inline(always)]
    pub(crate) unsafe fn cursor_preview(this: *const Self) -> *mut CursorPreview {
        unsafe { read_unaligned_at(this, 0x13c) }
    }
    #[inline(always)]
    pub(crate) unsafe fn menu_button(this: *const Self) -> *mut GameButton {
        unsafe { read_unaligned_at(this, 0x148) }
    }
    #[inline(always)]
    pub(crate) unsafe fn plant_array(this: *const Self) -> *mut Plant {
        unsafe { read_unaligned_at(this, 0xac) }
    }
    #[inline(always)]
    pub(crate) unsafe fn plant_next(this: *mut Self) -> *mut i32 {
        unsafe { addr_at_mut(this, 0xb8) }
    }
    #[inline(always)]
    pub(crate) unsafe fn plants(this: *mut Self) -> *mut DataArray<Plant> {
        unsafe { addr_at_mut(this, 0xac) }
    }
    #[inline(always)]
    pub(crate) unsafe fn projectiles(this: *mut Self) -> *mut DataArray<Projectile> {
        unsafe { addr_at_mut(this, 0xc8) }
    }
    #[inline(always)]
    pub(crate) unsafe fn coins(this: *mut Self) -> *mut DataArray<Coin> {
        unsafe { addr_at_mut(this, 0xe4) }
    }
    #[inline(always)]
    pub(crate) unsafe fn lawn_mowers(this: *mut Self) -> *mut DataArray<LawnMower> {
        unsafe { addr_at_mut(this, 0x100) }
    }
    #[inline(always)]
    pub(crate) unsafe fn zombie_list(this: *mut Self) -> *mut u32 {
        unsafe { addr_at_mut(this, 0x6b4) }
    }
    #[inline(always)]
    pub(crate) unsafe fn zombie_array(this: *const Self) -> *mut Zombie {
        unsafe { read_unaligned_at(this, 0x90) }
    }
    #[inline(always)]
    pub(crate) unsafe fn zombie_next(this: *mut Self) -> *mut i32 {
        unsafe { addr_at_mut(this, 0x9c) }
    }
    #[inline(always)]
    pub(crate) unsafe fn zombies(this: *mut Self) -> *mut DataArray<Zombie> {
        unsafe { addr_at_mut(this, 0x90) }
    }
    #[inline(always)]
    pub(crate) unsafe fn zombie_type_list(this: *mut Self) -> *mut u8 {
        unsafe { this.cast::<u8>().add(0x54d4).cast::<u8>() }
    }
    #[inline(always)]
    pub(crate) unsafe fn grid_items(this: *mut Self) -> *mut DataArray<GridItem> {
        unsafe { addr_at_mut(this, 0x11c) }
    }
    #[inline(always)]
    pub(crate) unsafe fn seed_bank(this: *const Self) -> *mut SeedBank {
        unsafe { read_unaligned_at(this, 0x144) }
    }
    #[inline(always)]
    pub(crate) unsafe fn cut_scene(this: *const Self) -> *mut CutScene {
        unsafe { read_unaligned_at(this, 0x15c) }
    }
    #[inline(always)]
    pub(crate) unsafe fn paused(this: *const Self) -> bool {
        unsafe { read_unaligned_at::<u8>(this, 0x164) != 0 }
    }
    #[inline(always)]
    pub(crate) unsafe fn block_types(this: *mut Self) -> *mut i32 {
        unsafe { addr_at_mut(this, 0x168) }
    }
    #[inline(always)]
    pub(crate) unsafe fn row_types(this: *mut Self) -> *mut i32 {
        unsafe { addr_at_mut(this, 0x5d8) }
    }
    #[inline(always)]
    pub(crate) unsafe fn ice_min_x(this: *const Self, row: usize) -> i32 {
        unsafe { read_unaligned_at(this, 0x60c + row * 4) }
    }
    #[inline(always)]
    pub(crate) unsafe fn ice_timer(this: *const Self, row: usize) -> i32 {
        unsafe { read_unaligned_at(this, 0x624 + row * 4) }
    }
    #[inline(always)]
    pub(crate) unsafe fn row_pick_weight_bits(this: *const Self, row: usize) -> u32 {
        unsafe { read_unaligned_at::<f32>(this, 0x654 + row * 16 + 4).to_bits() }
    }
    #[inline(always)]
    pub(crate) unsafe fn row_pick_last_picked_bits(this: *const Self, row: usize) -> u32 {
        unsafe { read_unaligned_at::<f32>(this, 0x654 + row * 16 + 8).to_bits() }
    }
    #[inline(always)]
    pub(crate) unsafe fn row_pick_second_last_picked_bits(this: *const Self, row: usize) -> u32 {
        unsafe { read_unaligned_at::<f32>(this, 0x654 + row * 16 + 12).to_bits() }
    }

    #[inline(always)]
    pub(crate) unsafe fn zombie_in_wave(this: *const Self, wave: usize, slot: usize) -> i32 {
        unsafe { read_unaligned_at(this, 0x6b4 + (wave * 50 + slot) * 4) }
    }
    #[inline(always)]
    pub(crate) unsafe fn zombie_allowed(this: *const Self, raw_kind: usize) -> bool {
        unsafe { read_unaligned_at::<u8>(this, 0x54d4 + raw_kind) != 0 }
    }
    #[inline(always)]
    pub(crate) unsafe fn bonus_lawn_mowers_remaining(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x608) }
    }
    #[inline(always)]
    pub(crate) unsafe fn bonus_lawn_mowers_remaining_mut(this: *mut Self) -> *mut i32 {
        unsafe { addr_at_mut(this, 0x608) }
    }
    #[inline(always)]
    pub(crate) unsafe fn scene(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x554c) }
    }
    #[inline(always)]
    pub(crate) unsafe fn scene_mut(this: *mut Self) -> *mut i32 {
        unsafe { addr_at_mut(this, 0x554c) }
    }
    #[inline(always)]
    pub(crate) unsafe fn level(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x5550) }
    }
    // Decomp `Board::mNumSunsFallen` (+0x553c) feeds natural sun drop countdown aging.
    #[inline(always)]
    pub(crate) unsafe fn num_suns_fallen(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x553c) }
    }
    #[inline(always)]
    pub(crate) unsafe fn sun_countdown(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x5538) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_sun_countdown(this: *mut Self, value: i32) {
        unsafe { write_unaligned_at(this, 0x5538, value) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_num_suns_fallen(this: *mut Self, value: i32) {
        unsafe { write_unaligned_at(this, 0x553c, value) }
    }
    #[inline(always)]
    pub(crate) unsafe fn sun(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x5560) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_sun(this: *mut Self, value: i32) {
        unsafe { write_unaligned_at(this, 0x5560, value) }
    }
    #[inline(always)]
    pub(crate) unsafe fn num_waves(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x5564) }
    }
    #[inline(always)]
    pub(crate) unsafe fn clock(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x5568) }
    }
    #[inline(always)]
    pub(crate) unsafe fn clock_mut(this: *mut Self) -> *mut i32 {
        unsafe { addr_at_mut(this, 0x5568) }
    }
    #[inline(always)]
    pub(crate) unsafe fn effect_counter(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x556c) }
    }
    #[inline(always)]
    pub(crate) unsafe fn effect_counter_mut(this: *mut Self) -> *mut i32 {
        unsafe { addr_at_mut(this, 0x556c) }
    }
    #[inline(always)]
    pub(crate) unsafe fn current_wave(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x557c) }
    }
    #[inline(always)]
    pub(crate) unsafe fn tutorial_state(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x5584) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_tutorial_state(this: *mut Self, value: i32) {
        unsafe { write_unaligned_at(this, 0x5584, value) }
    }
    #[inline(always)]
    pub(crate) unsafe fn time_stop_counter(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x5748) }
    }
    #[inline(always)]
    pub(crate) unsafe fn zombie_refresh_hp(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x5594) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_zombie_refresh_hp(this: *mut Self, value: i32) {
        unsafe { write_unaligned_at(this, 0x5594, value) }
    }
    #[inline(always)]
    pub(crate) unsafe fn zombie_health_wave_start(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x5598) }
    }
    #[inline(always)]
    pub(crate) unsafe fn refresh_countdown(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x559c) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_refresh_countdown(this: *mut Self, value: i32) {
        unsafe { write_unaligned_at(this, 0x559c, value) }
    }
    #[inline(always)]
    pub(crate) unsafe fn initial_countdown(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x55a0) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_initial_countdown(this: *mut Self, value: i32) {
        unsafe { write_unaligned_at(this, 0x55a0, value) }
    }
    #[inline(always)]
    pub(crate) unsafe fn huge_wave_countdown(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x55a4) }
    }
    #[inline(always)]
    pub(crate) unsafe fn level_complete(this: *const Self) -> bool {
        unsafe { read_unaligned_at::<u8>(this, 0x55fc) != 0 }
    }
    #[inline(always)]
    pub(crate) unsafe fn level_end_countdown(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x5604) }
    }
    #[inline(always)]
    pub(crate) unsafe fn next_survival_stage_counter(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x5604) }
    }
    #[inline(always)]
    pub(crate) unsafe fn board_rand_seed(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x561c) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_board_rand_seed(this: *mut Self, value: i32) {
        unsafe { write_unaligned_at(this, 0x561c, value) }
    }
    #[inline(always)]
    pub(crate) unsafe fn pool_particle_system_id(this: *const Self) -> u32 {
        unsafe { read_unaligned_at(this, 0x5620) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_pool_particle_system_id(this: *mut Self, value: u32) {
        unsafe { write_unaligned_at(this, 0x5620, value) }
    }
    // Decomp `Board::mPlantsEaten`; `Board::KillAllPlantsInRadius` increments this
    // before killing each plant hit by a Jack-in-the-box explosion.
    #[inline(always)]
    pub(crate) unsafe fn plants_eaten(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x5798) }
    }
    #[inline(always)]
    pub(crate) unsafe fn plants_eaten_mut(this: *mut Self) -> *mut i32 {
        unsafe { addr_at_mut(this, 0x5798) }
    }
    #[inline(always)]
    pub(crate) unsafe fn plants_shoveled(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x579c) }
    }
    #[inline(always)]
    pub(crate) unsafe fn plants_shoveled_mut(this: *mut Self) -> *mut i32 {
        unsafe { addr_at_mut(this, 0x579c) }
    }
}

#[repr(C)]
pub(crate) struct LawnMower {
    _data: [u8; 0x48],
}

impl LawnMower {
    #[inline(always)]
    pub(crate) unsafe fn is_dead(this: *const Self) -> bool {
        unsafe { read_unaligned_at::<u8>(this, 0x30) != 0 }
    }
}

#[repr(C)]
pub(crate) struct Coin {
    _data: [u8; 0xd8],
}

impl Coin {
    #[inline(always)]
    pub(crate) unsafe fn pos_x(this: *const Self) -> f32 {
        unsafe { read_unaligned_at(this, 0x24) }
    }
    #[inline(always)]
    pub(crate) unsafe fn pos_y(this: *const Self) -> f32 {
        unsafe { read_unaligned_at(this, 0x28) }
    }
    #[inline(always)]
    pub(crate) unsafe fn is_dead(this: *const Self) -> bool {
        unsafe { read_unaligned_at::<u8>(this, 0x38) != 0 }
    }
    #[inline(always)]
    pub(crate) unsafe fn is_being_collected(this: *const Self) -> bool {
        unsafe { read_unaligned_at::<u8>(this, 0x50) != 0 }
    }
    #[inline(always)]
    pub(crate) unsafe fn coin_type(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x58) }
    }
}

#[repr(C)]
pub(crate) struct GridItem {
    _data: [u8; 0xec],
}

impl GridItem {
    #[inline(always)]
    pub(crate) unsafe fn grid_item_type(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x8) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_grid_item_type(this: *mut Self, value: i32) {
        unsafe { write_unaligned_at(this, 0x8, value) }
    }
    #[inline(always)]
    pub(crate) unsafe fn grid_item_state(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0xc) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_grid_item_state(this: *mut Self, value: i32) {
        unsafe { write_unaligned_at(this, 0xc, value) }
    }
    #[inline(always)]
    pub(crate) unsafe fn grid_x(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x10) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_grid_x(this: *mut Self, value: i32) {
        unsafe { write_unaligned_at(this, 0x10, value) }
    }
    #[inline(always)]
    pub(crate) unsafe fn grid_y(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x14) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_grid_y(this: *mut Self, value: i32) {
        unsafe { write_unaligned_at(this, 0x14, value) }
    }
    #[inline(always)]
    pub(crate) unsafe fn grid_item_counter(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x18) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_grid_item_counter(this: *mut Self, value: i32) {
        unsafe { write_unaligned_at(this, 0x18, value) }
    }
    #[inline(always)]
    pub(crate) unsafe fn render_order(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x1c) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_render_order(this: *mut Self, value: i32) {
        unsafe { write_unaligned_at(this, 0x1c, value) }
    }
    #[inline(always)]
    pub(crate) unsafe fn is_dead(this: *const Self) -> bool {
        unsafe { read_unaligned_at::<u8>(this, 0x20) != 0 }
    }
    #[inline(always)]
    pub(crate) unsafe fn pos_x(this: *const Self) -> f32 {
        unsafe { read_unaligned_at(this, 0x24) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_pos_x(this: *mut Self, value: f32) {
        unsafe { write_unaligned_at(this, 0x24, value) }
    }
    #[inline(always)]
    pub(crate) unsafe fn pos_y(this: *const Self) -> f32 {
        unsafe { read_unaligned_at(this, 0x28) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_pos_y(this: *mut Self, value: f32) {
        unsafe { write_unaligned_at(this, 0x28, value) }
    }
    #[inline(always)]
    pub(crate) unsafe fn grid_item_reanim_id(this: *const Self) -> u32 {
        unsafe { read_unaligned_at(this, 0x34) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_grid_item_reanim_id(this: *mut Self, value: u32) {
        unsafe { write_unaligned_at(this, 0x34, value) }
    }
}

#[repr(C)]
pub(crate) struct Reanimation {
    _data: [u8; 0xa0],
}

impl Reanimation {
    // Temporary same-offset alias for existing cob animation code.
    #[inline(always)]
    pub(crate) unsafe fn anim_time(this: *const Self) -> f32 {
        unsafe { read_unaligned_at(this, 0x4) }
    }
    #[inline(always)]
    pub(crate) unsafe fn circulation_rate(this: *const Self) -> f32 {
        unsafe { read_unaligned_at(this, 0x4) }
    }
    #[inline(always)]
    pub(crate) unsafe fn anim_rate(this: *const Self) -> f32 {
        unsafe { read_unaligned_at(this, 0x8) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_anim_rate(this: *mut Self, value: f32) {
        unsafe { write_unaligned_at(this, 0x8, value) }
    }
    #[inline(always)]
    pub(crate) unsafe fn loop_type(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x10) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_loop_type(this: *mut Self, value: i32) {
        unsafe { write_unaligned_at(this, 0x10, value) }
    }
    #[inline(always)]
    pub(crate) unsafe fn is_dead(this: *const Self) -> bool {
        unsafe { read_unaligned_at::<u8>(this, 0x14) != 0 }
    }
    #[inline(always)]
    pub(crate) unsafe fn frame_start(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x18) }
    }
    #[inline(always)]
    pub(crate) unsafe fn frame_count(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x1c) }
    }
    #[inline(always)]
    pub(crate) unsafe fn loop_count(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x5c) }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_loop_count(this: *mut Self, value: i32) {
        unsafe { write_unaligned_at(this, 0x5c, value) }
    }
    #[inline(always)]
    pub(crate) unsafe fn is_attachment(this: *const Self) -> bool {
        unsafe { read_unaligned_at::<u8>(this, 0x64) != 0 }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_is_attachment(this: *mut Self, value: bool) {
        unsafe { write_unaligned_at(this, 0x64, u8::from(value)) }
    }
    #[inline(always)]
    pub(crate) unsafe fn last_frame_time(this: *const Self) -> f32 {
        unsafe { read_unaligned_at(this, 0x94) }
    }
}

#[repr(C)]
pub(crate) struct ParticleSystem {
    _data: [u8; 0x2c],
}

impl ParticleSystem {
    #[inline(always)]
    pub(crate) unsafe fn particle_type(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x0) }
    }
    #[inline(always)]
    pub(crate) unsafe fn dead(this: *const Self) -> bool {
        unsafe { read_unaligned_at::<u8>(this, 0x1c) != 0 }
    }
}

#[repr(C)]
pub(crate) struct CutScene {
    _data: [u8; 0],
}

impl CutScene {
    #[inline(always)]
    pub(crate) unsafe fn seed_choosing(this: *const Self) -> bool {
        unsafe { read_unaligned_at::<u8>(this, 0x2c) != 0 }
    }
    #[inline(always)]
    pub(crate) unsafe fn zombie_preview_created(this: *const Self) -> bool {
        unsafe { read_unaligned_at::<u8>(this, 0x35) != 0 }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_zombie_preview_created(this: *mut Self, value: bool) {
        unsafe { write_unaligned_at(this, 0x35, u8::from(value)) }
    }
    #[inline(always)]
    pub(crate) unsafe fn lawn_items_placed(this: *const Self) -> bool {
        unsafe { read_unaligned_at::<u8>(this, 0x36) != 0 }
    }
    #[inline(always)]
    pub(crate) unsafe fn set_lawn_items_placed(this: *mut Self, value: bool) {
        unsafe { write_unaligned_at(this, 0x36, u8::from(value)) }
    }
}

#[repr(C)]
pub(crate) struct CursorObject {
    _data: [u8; 0x4c],
}

impl CursorObject {
    #[inline(always)]
    pub(crate) unsafe fn cursor_type(this: *const Self) -> i32 {
        unsafe { read_unaligned_at(this, 0x30) }
    }
    #[inline(always)]
    pub(crate) unsafe fn cob_cannon_plant_id(this: *const Self) -> u32 {
        unsafe { read_unaligned_at(this, 0x40) }
    }
}
