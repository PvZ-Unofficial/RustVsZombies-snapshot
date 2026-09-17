#![allow(
    unsafe_code,
    reason = "this module binds borrowed native Portable objects to world lifetimes"
)]

use std::marker::PhantomData;
use std::ptr::NonNull;

use crate::{Error, Result, raw};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolSlot {
    pub active: bool,
    pub id: u32,
    pub next_free: u32,
}

pub struct Borrowed<'a, T> {
    ptr: NonNull<T>,
    _borrow: PhantomData<&'a World<'a>>,
}

impl<T> Copy for Borrowed<'_, T> {}

impl<T> Clone for Borrowed<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Borrowed<'_, T> {
    fn new(ptr: NonNull<T>) -> Self {
        Self {
            ptr,
            _borrow: PhantomData,
        }
    }

    #[must_use]
    pub const fn as_ptr(self) -> *mut T {
        self.ptr.as_ptr()
    }

    #[must_use]
    pub const fn as_non_null(self) -> NonNull<T> {
        self.ptr
    }

    /// Rebinds a pointer already validated against the current native world.
    ///
    /// # Safety
    ///
    /// `ptr` must remain valid for `'a` and belong to the same world borrow.
    #[doc(hidden)]
    pub unsafe fn from_non_null<'a>(ptr: NonNull<T>) -> Borrowed<'a, T> {
        Borrowed::new(ptr)
    }
}

pub type PlantRef<'a> = Borrowed<'a, raw::pvzp_rs_plant>;
pub type ZombieRef<'a> = Borrowed<'a, raw::pvzp_rs_zombie>;
pub type ProjectileRef<'a> = Borrowed<'a, raw::pvzp_rs_projectile>;
pub type GridItemRef<'a> = Borrowed<'a, raw::pvzp_rs_grid_item>;
pub type ItemRef<'a> = Borrowed<'a, raw::pvzp_rs_item>;
pub type SeedRef<'a> = Borrowed<'a, raw::pvzp_rs_seed>;
pub type ReanimationRef<'a> = Borrowed<'a, raw::pvzp_rs_reanimation>;

pub fn game_ui() -> Result<i32> {
    out_value(|out| {
        // SAFETY: `out` points to writable scalar storage.
        unsafe { raw::pvzp_rs_game_ui(out) }
    })
}

pub fn seed_chooser_mouse_visible() -> Result<bool> {
    out_value(|out| {
        // SAFETY: `out` points to writable scalar storage.
        unsafe { raw::pvzp_rs_seed_chooser_mouse_visible(out) }
    })
    .map(|value: u8| value != 0)
}

pub fn seed_chooser_parent_present() -> Result<bool> {
    out_value(|out| {
        // SAFETY: `out` points to writable scalar storage.
        unsafe { raw::pvzp_rs_seed_chooser_parent_present(out) }
    })
    .map(|value: u8| value != 0)
}

pub fn seed_chooser_widget_manager_present() -> Result<bool> {
    out_value(|out| {
        // SAFETY: `out` points to writable scalar storage.
        unsafe { raw::pvzp_rs_seed_chooser_widget_manager_present(out) }
    })
    .map(|value: u8| value != 0)
}

pub fn seed_chooser_modal_present() -> Result<bool> {
    out_value(|out| {
        // SAFETY: `out` points to writable scalar storage.
        unsafe { raw::pvzp_rs_seed_chooser_modal_present(out) }
    })
    .map(|value: u8| value != 0)
}

pub fn seed_chooser_choose_state() -> Result<i32> {
    out_value(|out| {
        // SAFETY: `out` points to writable scalar storage.
        unsafe { raw::pvzp_rs_seed_chooser_choose_state(out) }
    })
}

pub fn seed_chooser_view_lawn_time() -> Result<i32> {
    out_value(|out| {
        // SAFETY: `out` points to writable scalar storage.
        unsafe { raw::pvzp_rs_seed_chooser_view_lawn_time(out) }
    })
}

pub fn seed_chooser_seeds_in_flight() -> Result<i32> {
    out_value(|out| {
        // SAFETY: `out` points to writable scalar storage.
        unsafe { raw::pvzp_rs_seed_chooser_seeds_in_flight(out) }
    })
}

pub fn seed_chooser_cancel_view_lawn() -> Result<()> {
    // SAFETY: the bridge validates the current chooser structural state.
    Error::from_status(unsafe { raw::pvzp_rs_seed_chooser_cancel_view_lawn() })
}

pub fn click_continue_dialog_if_present() -> Result<bool> {
    out_value(|out| {
        // SAFETY: `out` points to writable scalar storage.
        unsafe { raw::pvzp_rs_click_continue_dialog_if_present(out) }
    })
    .map(|clicked: u8| clicked != 0)
}

pub fn input_focused() -> Result<bool> {
    out_value(|out| {
        // SAFETY: `out` points to writable scalar storage.
        unsafe { raw::pvzp_rs_input_focused(out) }
    })
    .map(|focused: u8| focused != 0)
}

pub fn app_counter() -> Result<u32> {
    out_value(|out| {
        // SAFETY: `out` points to writable scalar storage.
        unsafe { raw::pvzp_rs_app_counter(out) }
    })
}

pub fn set_app_counter(counter: u32) -> Result<()> {
    // SAFETY: the bridge validates that the application exists before writing the scalar.
    Error::from_status(unsafe { raw::pvzp_rs_set_app_counter(counter) })
}

pub fn selected_card_count() -> Result<u32> {
    out_value(|out| {
        // SAFETY: `out` points to writable scalar storage.
        unsafe { raw::pvzp_rs_selected_card_count(out) }
    })
}

pub fn selected_card(index: u32) -> Result<(i32, i32)> {
    let mut packet = 0;
    let mut imitater = 0;
    // SAFETY: both output pointers refer to writable local scalar storage.
    Error::from_status(unsafe { raw::pvzp_rs_selected_card(index, &raw mut packet, &raw mut imitater) })?;
    Ok((packet, imitater))
}

pub fn select_card(packet_type: i32, imitater_type: i32) -> Result<()> {
    // SAFETY: both enum values are copied and validated by the bridge.
    Error::from_status(unsafe { raw::pvzp_rs_select_card(packet_type, imitater_type) })
}

pub fn start_battle() -> Result<()> {
    // SAFETY: the bridge validates the current chooser lifecycle state.
    Error::from_status(unsafe { raw::pvzp_rs_start_battle() })
}

pub fn enter_endless(scene: i32) -> Result<()> {
    // SAFETY: the bridge validates the scene and current UI transition.
    Error::from_status(unsafe { raw::pvzp_rs_enter_endless(scene) })
}

pub fn back_to_main_menu() -> Result<()> {
    // SAFETY: the bridge validates the current live-playing state.
    Error::from_status(unsafe { raw::pvzp_rs_back_to_main_menu() })
}

pub fn set_game_speed(speed: f32) -> Result<()> {
    // SAFETY: the copied scalar is validated by the bridge.
    Error::from_status(unsafe { raw::pvzp_rs_set_game_speed(speed) })
}

pub fn restore_game_speed() -> Result<()> {
    // SAFETY: the bridge only restores previously captured native timing scalars.
    Error::from_status(unsafe { raw::pvzp_rs_restore_game_speed() })
}

pub fn set_fast_forward(enabled: bool, performance: i32, suppress_window: bool) -> Result<()> {
    // SAFETY: all arguments are copied scalars validated by the bridge.
    Error::from_status(unsafe {
        raw::pvzp_rs_set_fast_forward(u8::from(enabled), performance, u8::from(suppress_window))
    })
}

pub fn fast_forward_active() -> bool {
    // SAFETY: this only reads native scalar state; any stream selector is a validated bool.
    unsafe { raw::pvzp_rs_fast_forward_active() != 0 }
}

pub fn request_seed_chooser_fast_forward(max_frames: u32) -> Result<()> {
    // SAFETY: the copied frame cap is validated by the bridge.
    Error::from_status(unsafe { raw::pvzp_rs_request_seed_chooser_fast_forward(max_frames) })
}

pub fn set_advanced_pause(
    enabled: bool, draw_mask: bool, rgba: u32, play_sound: bool, refresh_cursor_preview: bool,
) -> Result<()> {
    // SAFETY: all arguments are copied scalars.
    Error::from_status(unsafe {
        raw::pvzp_rs_set_advanced_pause(
            u8::from(enabled),
            u8::from(draw_mask),
            rgba,
            u8::from(play_sound),
            u8::from(refresh_cursor_preview),
        )
    })
}

pub fn advanced_pause_active() -> bool {
    // SAFETY: this query reads only bridge-owned scalar state and takes no pointers.
    unsafe { raw::pvzp_rs_advanced_pause_active() != 0 }
}

pub fn set_random_mode(mode: i32, value: u32) -> Result<()> {
    // SAFETY: the bridge validates the copied mode discriminant.
    Error::from_status(unsafe { raw::pvzp_rs_set_random_mode(mode, value) })
}

pub fn set_wave_spawn_random_seed(base_seed: Option<u32>) -> Result<()> {
    let (enabled, seed) = base_seed.map_or((0, 0), |seed| (1, seed));
    // SAFETY: the bridge copies the seed and changes only the inactive wave-spawn override.
    Error::from_status(unsafe { raw::pvzp_rs_set_wave_spawn_random_seed(enabled, seed) })
}

pub fn random_seed(stream: i32) -> Result<u32> {
    out_value(|out| {
        // SAFETY: output is writable scalar storage; the bridge checks the stream index.
        unsafe { raw::pvzp_rs_random_seed(stream, out) }
    })
}
pub fn random_locked(level_stream: bool) -> bool {
    // SAFETY: this only reads native scalar state; any stream selector is a validated bool.
    unsafe { raw::pvzp_rs_random_locked(level_stream) != 0 }
}
pub fn random_fixed(level_stream: bool) -> u32 {
    // SAFETY: this only reads native scalar state; any stream selector is a validated bool.
    unsafe { raw::pvzp_rs_random_fixed(level_stream) }
}

pub fn set_sun_production_mode(mode: i32) -> Result<()> {
    // SAFETY: the bridge validates the copied mode discriminant.
    Error::from_status(unsafe { raw::pvzp_rs_set_sun_production_mode(mode) })
}

pub struct World<'a> {
    raw: NonNull<raw::pvzp_rs_world>,
    _borrow: PhantomData<&'a ()>,
}

impl<'a> World<'a> {
    /// Reuses a Board validated by the caller's shared backend scope.
    ///
    /// # Safety
    /// The Board must remain alive without native update or replacement for `'a`.
    #[doc(hidden)]
    pub unsafe fn from_non_null(raw: NonNull<raw::pvzp_rs_world>) -> Self {
        Self {
            raw,
            _borrow: PhantomData,
        }
    }

    /// Borrows the current native Board for the caller-controlled backend scope.
    ///
    /// # Safety
    ///
    /// The caller must keep the Board alive and prevent native updates for `'a`.
    pub unsafe fn current() -> Result<Self> {
        let mut raw = std::ptr::null_mut();
        // SAFETY: `raw` points to writable pointer storage.
        Error::from_status(unsafe { raw::pvzp_rs_current_world(&raw mut raw) })?;
        Ok(Self {
            raw: NonNull::new(raw).ok_or(Error::NullPointer)?,
            _borrow: PhantomData,
        })
    }

    #[must_use]
    pub const fn as_ptr(&self) -> *mut raw::pvzp_rs_world {
        self.raw.as_ptr()
    }

    #[doc(hidden)]
    pub const fn as_non_null(&self) -> NonNull<raw::pvzp_rs_world> {
        self.raw
    }

    pub fn plants(&self) -> PlantPool<'a> {
        // SAFETY: the bridge returns an embedded pool of this live Board.
        unsafe { PlantPool::new(NonNull::new_unchecked(raw::pvzp_rs_world_plant_pool(self.raw.as_ptr()))) }
    }

    pub fn zombies(&self) -> ZombiePool<'a> {
        // SAFETY: the bridge returns an embedded pool of this live Board.
        unsafe {
            ZombiePool::new(NonNull::new_unchecked(raw::pvzp_rs_world_zombie_pool(
                self.raw.as_ptr(),
            )))
        }
    }

    pub fn projectiles(&self) -> ProjectilePool<'a> {
        // SAFETY: the bridge returns an embedded pool of this live Board.
        unsafe {
            ProjectilePool::new(NonNull::new_unchecked(raw::pvzp_rs_world_projectile_pool(
                self.raw.as_ptr(),
            )))
        }
    }

    pub fn grid_items(&self) -> GridItemPool<'a> {
        // SAFETY: the bridge returns an embedded pool of this live Board.
        unsafe {
            GridItemPool::new(NonNull::new_unchecked(raw::pvzp_rs_world_grid_item_pool(
                self.raw.as_ptr(),
            )))
        }
    }

    pub fn items(&self) -> ItemPool<'a> {
        // SAFETY: the bridge returns an embedded pool of this live Board.
        unsafe { ItemPool::new(NonNull::new_unchecked(raw::pvzp_rs_world_item_pool(self.raw.as_ptr()))) }
    }

    pub fn scene(&self) -> i32 {
        // SAFETY: this live borrowed Board owns the copied scalar field.
        unsafe { raw::pvzp_rs_world_scene(self.raw.as_ptr()) }
    }

    pub fn paused(&self) -> bool {
        // SAFETY: this live borrowed Board owns the copied scalar field.
        unsafe { raw::pvzp_rs_world_paused(self.raw.as_ptr()) != 0 }
    }

    pub fn seed_choosing(&self) -> Result<bool> {
        self.scalar(raw::pvzp_rs_world_seed_choosing)
            .map(|value: u8| value != 0)
    }

    pub fn main_counter(&self) -> u32 {
        // SAFETY: this live borrowed Board owns the copied scalar field.
        unsafe { raw::pvzp_rs_world_main_counter(self.raw.as_ptr()) }
    }

    pub fn imitater_successor(&self, placeholder_id: u32) -> Option<u32> {
        let mut successor = 0;
        // SAFETY: this world is live and output points to writable scalar storage.
        let found =
            unsafe { raw::pvzp_rs_world_imitater_successor(self.raw.as_ptr(), placeholder_id, &raw mut successor) };
        (found != 0).then_some(successor)
    }

    pub fn reset(&mut self, completed_rounds: u32, seed: u32, initial_sun: u32, ready_cooldowns: bool) -> Result<()> {
        // SAFETY: the bridge validates that this is the current Board before replacing it.
        Error::from_status(unsafe {
            raw::pvzp_rs_world_reset(
                self.raw.as_ptr(),
                completed_rounds,
                seed,
                initial_sun,
                u8::from(ready_cooldowns),
            )
        })
    }

    pub fn cursor_type(&self) -> Result<i32> {
        self.scalar(raw::pvzp_rs_world_cursor_type)
    }

    pub fn current_wave(&self) -> i32 {
        // SAFETY: this live borrowed Board owns the copied scalar field.
        unsafe { raw::pvzp_rs_world_current_wave(self.raw.as_ptr()) }
    }

    pub fn sun(&self) -> i32 {
        // SAFETY: this live borrowed Board owns the copied scalar field.
        unsafe { raw::pvzp_rs_world_sun(self.raw.as_ptr()) }
    }

    pub fn level_complete(&self) -> bool {
        // SAFETY: this live borrowed Board owns the copied scalar field.
        unsafe { raw::pvzp_rs_world_level_complete(self.raw.as_ptr()) != 0 }
    }

    pub fn next_survival_stage_counter(&self) -> i32 {
        // SAFETY: this live borrowed Board owns the copied scalar field.
        unsafe { raw::pvzp_rs_world_next_survival_stage_counter(self.raw.as_ptr()) }
    }

    pub fn set_sun(&self, sun: u32) -> Result<()> {
        // SAFETY: this world is live and the scalar is copied.
        Error::from_status(unsafe { raw::pvzp_rs_world_set_sun(self.raw.as_ptr(), sun) })
    }

    pub fn natural_sun_generated(&self) -> i32 {
        // SAFETY: this world remains alive throughout its protected borrow.
        unsafe { raw::pvzp_rs_world_natural_sun_generated(self.raw.as_ptr()) }
    }

    pub fn natural_sun_countdown(&self) -> i32 {
        // SAFETY: this world remains alive throughout its protected borrow.
        unsafe { raw::pvzp_rs_world_natural_sun_countdown(self.raw.as_ptr()) }
    }

    pub fn set_natural_sun_generated(&self, count: i32) -> Result<()> {
        // SAFETY: this world is live and the scalar is copied.
        Error::from_status(unsafe { raw::pvzp_rs_world_set_natural_sun_generated(self.raw.as_ptr(), count) })
    }

    pub fn set_natural_sun_countdown(&self, countdown: i32) -> Result<()> {
        // SAFETY: this world is live and the scalar is copied.
        Error::from_status(unsafe { raw::pvzp_rs_world_set_natural_sun_countdown(self.raw.as_ptr(), countdown) })
    }

    pub fn ice_path_x(&self, row: u32) -> Result<i32> {
        out_value(|out| {
            // SAFETY: this world is live, output is writable, and native code checks row bounds.
            unsafe { raw::pvzp_rs_world_ice_path_x(self.raw.as_ptr(), row, out) }
        })
    }
    pub fn ice_path_countdown(&self, row: u32) -> Result<u32> {
        out_value(|out| {
            // SAFETY: this world is live, output is writable, and native code checks row bounds.
            unsafe { raw::pvzp_rs_world_ice_path_countdown(self.raw.as_ptr(), row, out) }
        })
    }
    pub fn row_pick_weight_bits(&self, row: u32) -> Result<u32> {
        out_value(|out| {
            // SAFETY: this world is live, output is writable, and native code checks row bounds.
            unsafe { raw::pvzp_rs_world_row_pick_weight_bits(self.raw.as_ptr(), row, out) }
        })
    }
    pub fn row_pick_last_picked_bits(&self, row: u32) -> Result<u32> {
        out_value(|out| {
            // SAFETY: this world is live, output is writable, and native code checks row bounds.
            unsafe { raw::pvzp_rs_world_row_pick_last_picked_bits(self.raw.as_ptr(), row, out) }
        })
    }
    pub fn row_pick_second_last_picked_bits(&self, row: u32) -> Result<u32> {
        out_value(|out| {
            // SAFETY: this world is live, output is writable, and native code checks row bounds.
            unsafe { raw::pvzp_rs_world_row_pick_second_last_picked_bits(self.raw.as_ptr(), row, out) }
        })
    }
    pub fn spawn_allowed(&self, zombie_type: u32) -> Result<bool> {
        self.bool_call(|out| {
            // SAFETY: this world is live and `out` points to writable storage.
            unsafe { raw::pvzp_rs_world_spawn_allowed(self.raw.as_ptr(), zombie_type, out) }
        })
    }

    pub fn spawn_entry(&self, wave: u32, slot: u32) -> Result<i32> {
        out_value(|out| {
            // SAFETY: this world is live and `out` points to writable storage.
            unsafe { raw::pvzp_rs_world_spawn_entry(self.raw.as_ptr(), wave, slot, out) }
        })
    }

    pub fn total_waves(&self) -> i32 {
        // SAFETY: this live borrowed Board owns the copied scalar field.
        unsafe { raw::pvzp_rs_world_total_waves(self.raw.as_ptr()) }
    }

    pub fn refresh_countdown(&self) -> i32 {
        // SAFETY: this live borrowed Board owns the copied scalar field.
        unsafe { raw::pvzp_rs_world_refresh_countdown(self.raw.as_ptr()) }
    }

    pub fn initial_countdown(&self) -> i32 {
        // SAFETY: this live borrowed Board owns the copied scalar field.
        unsafe { raw::pvzp_rs_world_initial_countdown(self.raw.as_ptr()) }
    }

    pub fn huge_wave_countdown(&self) -> i32 {
        // SAFETY: this live borrowed Board owns the copied scalar field.
        unsafe { raw::pvzp_rs_world_huge_wave_countdown(self.raw.as_ptr()) }
    }

    pub fn level_end_countdown(&self) -> i32 {
        // SAFETY: this live borrowed Board owns the copied scalar field.
        unsafe { raw::pvzp_rs_world_level_end_countdown(self.raw.as_ptr()) }
    }

    pub fn commit_wave_refresh(&self, expected_wave: i32, initial_countdown: i32) -> Result<()> {
        // SAFETY: this world is live and the bridge validates both preconditions before writing.
        Error::from_status(unsafe {
            raw::pvzp_rs_world_commit_wave_refresh(self.raw.as_ptr(), expected_wave, initial_countdown)
        })
    }

    pub fn zombie_health_wave_start(&self) -> Result<i32> {
        self.scalar(raw::pvzp_rs_world_zombie_health_wave_start)
    }

    pub fn total_zombie_health_in_wave(&self, wave: i32) -> Result<i32> {
        out_value(|out| {
            // SAFETY: this world is live and `out` points to writable storage.
            unsafe { raw::pvzp_rs_world_total_zombie_health_in_wave(self.raw.as_ptr(), wave, out) }
        })
    }

    pub fn set_scene(&self, scene: i32) -> Result<()> {
        // SAFETY: this world is live and the bridge validates the enum code.
        Error::from_status(unsafe { raw::pvzp_rs_world_set_scene(self.raw.as_ptr(), scene) })
    }

    pub fn clear_lawn_mowers(&self) -> Result<()> {
        // SAFETY: this world is live; the bridge removes native mower objects through their normal death path.
        Error::from_status(unsafe { raw::pvzp_rs_world_clear_lawn_mowers(self.raw.as_ptr()) })
    }

    pub fn set_spawn_allowed(&self, zombie_type: i32, allowed: bool) -> Result<()> {
        // SAFETY: this world is live and the bridge validates the enum code.
        Error::from_status(unsafe {
            raw::pvzp_rs_world_set_spawn_allowed(self.raw.as_ptr(), zombie_type, u8::from(allowed))
        })
    }

    pub fn set_spawn_entry(&self, wave: u32, slot: u32, zombie_type: i32) -> Result<()> {
        // SAFETY: this world is live and the bridge validates all indices and the enum code.
        Error::from_status(unsafe { raw::pvzp_rs_world_set_spawn_entry(self.raw.as_ptr(), wave, slot, zombie_type) })
    }

    pub fn pick_spawn_list(&self) -> Result<()> {
        // SAFETY: this world is live and the native action owns its internal updates.
        Error::from_status(unsafe { raw::pvzp_rs_world_pick_spawn_list(self.raw.as_ptr()) })
    }

    pub fn grid_to_pixel_x(&self, grid_x: i32, grid_y: i32) -> Result<i32> {
        out_value(|out| {
            // SAFETY: this world is live and `out` points to writable storage.
            unsafe { raw::pvzp_rs_world_grid_to_pixel_x(self.raw.as_ptr(), grid_x, grid_y, out) }
        })
    }

    pub fn grid_to_pixel_y(&self, grid_x: i32, grid_y: i32) -> Result<i32> {
        out_value(|out| {
            // SAFETY: this world is live and `out` points to writable storage.
            unsafe { raw::pvzp_rs_world_grid_to_pixel_y(self.raw.as_ptr(), grid_x, grid_y, out) }
        })
    }

    pub fn pos_y_based_on_row(&self, pos_x: f32, row: i32) -> Result<f32> {
        out_value(|out| {
            // SAFETY: this world is live and `out` points to writable storage.
            unsafe { raw::pvzp_rs_world_pos_y_based_on_row(self.raw.as_ptr(), pos_x, row, out) }
        })
    }

    pub fn is_pool_square(&self, grid_x: i32, grid_y: i32) -> Result<bool> {
        self.bool_call(|out| {
            // SAFETY: this world is live and `out` points to writable storage.
            unsafe { raw::pvzp_rs_world_is_pool_square(self.raw.as_ptr(), grid_x, grid_y, out) }
        })
    }

    pub fn row_can_have_zombies(&self, row: i32) -> Result<bool> {
        self.bool_call(|out| {
            // SAFETY: this world is live and `out` points to writable storage.
            unsafe { raw::pvzp_rs_world_row_can_have_zombies(self.raw.as_ptr(), row, out) }
        })
    }

    pub fn seed_count(&self) -> Result<u32> {
        self.scalar(raw::pvzp_rs_world_seed_count)
    }

    pub fn seed_at(&self, index: u32) -> Result<SeedRef<'a>> {
        out_required_ptr(|out| {
            // SAFETY: this world is live and `out` points to writable pointer storage.
            unsafe { raw::pvzp_rs_world_seed_at(self.raw.as_ptr(), index, out) }
        })
        .map(Borrowed::new)
    }

    pub fn seed_can_pick_up(&self, seed: SeedRef<'a>) -> Result<bool> {
        self.bool_call(|out| {
            // SAFETY: `seed` is borrowed from this world's fixed seed bank.
            unsafe { raw::pvzp_rs_seed_can_pick_up(seed.as_ptr(), out) }
        })
    }

    pub fn seed_was_planted(&self, seed: SeedRef<'a>) -> Result<()> {
        // SAFETY: `seed` is borrowed from this world's fixed seed bank.
        Error::from_status(unsafe { raw::pvzp_rs_seed_was_planted(seed.as_ptr()) })
    }

    pub fn current_plant_cost(&self, packet_type: i32, imitater_type: i32) -> Result<i32> {
        out_value(|out| {
            // SAFETY: this world is live and `out` points to writable storage.
            unsafe { raw::pvzp_rs_world_current_plant_cost(self.raw.as_ptr(), packet_type, imitater_type, out) }
        })
    }

    pub fn can_take_sun(&self, amount: i32) -> Result<bool> {
        self.bool_call(|out| {
            // SAFETY: this world is live and `out` points to writable storage.
            unsafe { raw::pvzp_rs_world_can_take_sun(self.raw.as_ptr(), amount, out) }
        })
    }

    pub fn take_sun(&self, amount: i32) -> Result<bool> {
        self.bool_call(|out| {
            // SAFETY: this world is live and `out` points to writable storage.
            unsafe { raw::pvzp_rs_world_take_sun(self.raw.as_ptr(), amount, out) }
        })
    }

    pub fn can_plant_at(&self, packet_type: i32, grid_x: i32, grid_y: i32) -> Result<i32> {
        out_value(|out| {
            // SAFETY: this world is live and `out` points to writable storage.
            unsafe { raw::pvzp_rs_world_can_plant_at(self.raw.as_ptr(), packet_type, grid_x, grid_y, out) }
        })
    }

    pub fn new_plant(
        &self, grid_x: i32, grid_y: i32, packet_type: i32, imitater_type: i32,
    ) -> Result<Option<PlantRef<'a>>> {
        out_optional_ptr(|out| {
            // SAFETY: this world is live and `out` points to writable pointer storage.
            unsafe { raw::pvzp_rs_world_new_plant(self.raw.as_ptr(), grid_x, grid_y, packet_type, imitater_type, out) }
        })
        .map(|ptr| ptr.map(Borrowed::new))
    }

    pub fn add_plant(
        &self, grid_x: i32, grid_y: i32, packet_type: i32, imitater_type: i32,
    ) -> Result<Option<PlantRef<'a>>> {
        out_optional_ptr(|out| {
            // SAFETY: this world is live and `out` points to writable pointer storage.
            unsafe { raw::pvzp_rs_world_add_plant(self.raw.as_ptr(), grid_x, grid_y, packet_type, imitater_type, out) }
        })
        .map(|ptr| ptr.map(Borrowed::new))
    }

    pub fn plant_die(&self, plant: PlantRef<'a>) -> Result<()> {
        // SAFETY: `plant` is borrow-bound to this world.
        Error::from_status(unsafe { raw::pvzp_rs_plant_die(plant.as_ptr()) })
    }

    pub fn plant_imitater_morph(&self, plant: PlantRef<'a>) -> Result<Option<PlantRef<'a>>> {
        out_optional_ptr(|out| {
            // SAFETY: plant belongs to this borrowed Board. Native morph does
            // not advance or replace the world and returns a new fixed-pool slot.
            unsafe { raw::pvzp_rs_plant_imitater_morph(plant.as_ptr(), out) }
        })
        .map(|ptr| ptr.map(Borrowed::new))
    }

    pub fn plant_set_sleeping(&self, plant: PlantRef<'a>, asleep: bool) -> Result<()> {
        // SAFETY: `plant` is borrow-bound and the boolean is copied.
        Error::from_status(unsafe { raw::pvzp_rs_plant_set_sleeping(plant.as_ptr(), u8::from(asleep)) })
    }

    pub fn plant_play_idle(&self, plant: PlantRef<'a>, fps: f32) -> Result<()> {
        // SAFETY: `plant` is borrow-bound and `fps` is copied.
        Error::from_status(unsafe { raw::pvzp_rs_plant_play_idle(plant.as_ptr(), fps) })
    }

    pub fn plant_update_reanim_color(&self, plant: PlantRef<'a>) -> Result<()> {
        // SAFETY: `plant` is borrow-bound to this world.
        Error::from_status(unsafe { raw::pvzp_rs_plant_update_reanim_color(plant.as_ptr()) })
    }

    pub fn plant_fire_cob(&self, plant: PlantRef<'a>, target_x: i32, target_y: i32) -> Result<()> {
        // SAFETY: `plant` is borrow-bound and target coordinates are copied.
        Error::from_status(unsafe { raw::pvzp_rs_plant_fire_cob(plant.as_ptr(), target_x, target_y) })
    }

    pub fn plant_hit_box(&self, plant: PlantRef<'a>) -> raw::pvzp_rs_rect_i32 {
        unsafe {
            // SAFETY: the live entity and validated weapon satisfy the native query,
            // which initializes all four output fields in a single call.
            out_native_value(|out| raw::pvzp_rs_plant_hit_box(plant.as_ptr(), out))
        }
    }

    pub fn plant_attack_rect(&self, plant: PlantRef<'a>, weapon: i32) -> Result<raw::pvzp_rs_rect_i32> {
        if !(0..=1).contains(&weapon) {
            return Err(Error::InvalidArgument { detail: weapon });
        }
        Ok(unsafe {
            // SAFETY: the live entity and validated weapon satisfy the native query,
            // which initializes all four output fields in a single call.
            out_native_value(|out| raw::pvzp_rs_plant_attack_rect(plant.as_ptr(), weapon, out))
        })
    }

    pub fn plant_damage_range_flags(&self, plant: PlantRef<'a>, weapon: i32) -> Result<u32> {
        if !(0..=1).contains(&weapon) {
            return Err(Error::InvalidArgument { detail: weapon });
        }
        // SAFETY: the plant is borrowed and the copied weapon discriminant was validated.
        Ok(unsafe { raw::pvzp_rs_plant_damage_range_flags(plant.as_ptr(), weapon) })
    }

    pub fn plant_reanimation(&self, plant: PlantRef<'a>) -> Result<Option<ReanimationRef<'a>>> {
        out_optional_ptr(|out| {
            // SAFETY: `plant` is borrow-bound and `out` is writable.
            unsafe { raw::pvzp_rs_plant_reanimation(plant.as_ptr(), out) }
        })
        .map(|ptr| ptr.map(Borrowed::new))
    }

    pub fn zombie_pos_y_based_on_row(&self, zombie: ZombieRef<'a>, row: i32) -> Result<f32> {
        out_value(|out| {
            // SAFETY: `zombie` is borrow-bound and `out` is writable.
            unsafe { raw::pvzp_rs_zombie_pos_y_based_on_row(zombie.as_ptr(), row, out) }
        })
    }

    pub fn zombie_hit_box(&self, zombie: ZombieRef<'a>) -> Result<raw::pvzp_rs_rect_i32> {
        out_value(|out| {
            // SAFETY: `zombie` is borrow-bound and `out` is writable.
            unsafe { raw::pvzp_rs_zombie_hit_box(zombie.as_ptr(), out) }
        })
    }

    pub fn zombie_effected_by_damage(&self, zombie: ZombieRef<'a>, flags: u32) -> Result<bool> {
        self.bool_call(|out| {
            // SAFETY: `zombie` is borrow-bound and `out` is writable.
            unsafe { raw::pvzp_rs_zombie_effected_by_damage(zombie.as_ptr(), flags, out) }
        })
    }

    pub fn zombie_can_target_plant(
        &self, zombie: ZombieRef<'a>, plant: PlantRef<'a>, attack_type: i32,
    ) -> Result<bool> {
        self.bool_call(|out| {
            // SAFETY: both objects are borrow-bound and `out` is writable.
            unsafe { raw::pvzp_rs_zombie_can_target_plant(zombie.as_ptr(), plant.as_ptr(), attack_type, out) }
        })
    }

    pub fn zombie_reanimation(&self, zombie: ZombieRef<'a>) -> Result<Option<ReanimationRef<'a>>> {
        zombie.reanimation()
    }

    pub fn add_zombie_in_row(&self, zombie_type: i32, row: i32, from_wave: i32) -> Result<Option<ZombieRef<'a>>> {
        out_optional_ptr(|out| {
            // SAFETY: this world is live and `out` points to writable pointer storage.
            unsafe { raw::pvzp_rs_world_add_zombie_in_row(self.raw.as_ptr(), zombie_type, row, from_wave, out) }
        })
        .map(|ptr| ptr.map(Borrowed::new))
    }

    pub fn place_zombie(&self, zombie_type: i32, grid_x: i32, grid_y: i32) -> Result<ZombieRef<'a>> {
        out_required_ptr(|out| {
            // SAFETY: this world is live, arguments are copied, and `out` is writable.
            unsafe { raw::pvzp_rs_world_place_zombie(self.raw.as_ptr(), zombie_type, grid_x, grid_y, out) }
        })
        .map(Borrowed::new)
    }

    pub fn zombie_die_no_loot(&self, zombie: ZombieRef<'a>) -> Result<()> {
        // SAFETY: `zombie` is borrow-bound to this world.
        Error::from_status(unsafe { raw::pvzp_rs_zombie_die_no_loot(zombie.as_ptr()) })
    }

    pub fn zombie_die_with_loot(&self, zombie: ZombieRef<'a>) -> Result<()> {
        zombie.die_with_loot()
    }

    pub fn projectile_hit_box(&self, projectile: ProjectileRef<'a>) -> raw::pvzp_rs_rect_i32 {
        unsafe {
            // SAFETY: the live entity and validated weapon satisfy the native query,
            // which initializes all four output fields in a single call.
            out_native_value(|out| raw::pvzp_rs_projectile_hit_box(projectile.as_ptr(), out))
        }
    }

    pub fn add_ladder(&self, grid_x: i32, grid_y: i32) -> Result<Option<GridItemRef<'a>>> {
        self.add_grid_item(raw::pvzp_rs_world_add_ladder, grid_x, grid_y)
    }

    pub fn add_crater(&self, grid_x: i32, grid_y: i32) -> Result<Option<GridItemRef<'a>>> {
        self.add_grid_item(raw::pvzp_rs_world_add_crater, grid_x, grid_y)
    }

    pub fn add_gravestone(&self, grid_x: i32, grid_y: i32) -> Result<Option<GridItemRef<'a>>> {
        self.add_grid_item(raw::pvzp_rs_world_add_gravestone, grid_x, grid_y)
    }

    pub fn grid_item_die(&self, item: GridItemRef<'a>) -> Result<()> {
        // SAFETY: `item` is borrow-bound to this world.
        Error::from_status(unsafe { raw::pvzp_rs_grid_item_die(item.as_ptr()) })
    }

    pub fn item_collect(&self, item: ItemRef<'a>, play_sound: bool) -> Result<()> {
        // SAFETY: `item` is borrow-bound and the boolean is copied.
        Error::from_status(unsafe { raw::pvzp_rs_item_collect(item.as_ptr(), u8::from(play_sound)) })
    }

    fn add_grid_item(
        &self,
        add: unsafe extern "C" fn(
            *mut raw::pvzp_rs_world,
            i32,
            i32,
            *mut *mut raw::pvzp_rs_grid_item,
        ) -> raw::pvzp_rs_status,
        grid_x: i32, grid_y: i32,
    ) -> Result<Option<GridItemRef<'a>>> {
        out_optional_ptr(|out| {
            // SAFETY: this world is live and `out` points to writable pointer storage.
            unsafe { add(self.raw.as_ptr(), grid_x, grid_y, out) }
        })
        .map(|ptr| ptr.map(Borrowed::new))
    }

    fn scalar<T>(
        &self, get: unsafe extern "C" fn(*const raw::pvzp_rs_world, *mut T) -> raw::pvzp_rs_status,
    ) -> Result<T> {
        out_value(|out| {
            // SAFETY: this world is live and `out` points to writable storage.
            unsafe { get(self.raw.as_ptr(), out) }
        })
    }

    fn bool_call(&self, call: impl FnOnce(*mut u8) -> raw::pvzp_rs_status) -> Result<bool> {
        out_value(call).map(|value: u8| value != 0)
    }
}

macro_rules! pool {
    (
        $name:ident, $raw_pool:ty, $raw_object:ty, $object_ref:ident,
        $max:path, $active:path, $capacity:path, $free:path, $next_key:path, $slot:path,
        $get:path, $try_get:path, $get_id:path
    ) => {
        #[derive(Clone, Copy)]
        pub struct $name<'a> {
            ptr: NonNull<$raw_pool>,
            _borrow: PhantomData<&'a World<'a>>,
        }

        impl<'a> $name<'a> {
            fn new(ptr: NonNull<$raw_pool>) -> Self {
                Self {
                    ptr,
                    _borrow: PhantomData,
                }
            }

            /// Rebinds a pool whose owning Board is protected by the caller.
            /// # Safety
            /// The pool must remain alive without update/reclamation for `'a`.
            #[doc(hidden)]
            pub unsafe fn from_non_null(ptr: NonNull<$raw_pool>) -> Self {
                Self::new(ptr)
            }
            #[doc(hidden)]
            pub const fn as_non_null(self) -> NonNull<$raw_pool> {
                self.ptr
            }

            pub fn max_used_count(self) -> u32 {
                // SAFETY: this non-null pool remains borrowed from its live Board.
                unsafe { $max(self.ptr.as_ptr()) }
            }

            pub fn active_count(self) -> u32 {
                // SAFETY: this non-null pool remains borrowed from its live Board.
                unsafe { $active(self.ptr.as_ptr()) }
            }

            pub fn capacity(self) -> u32 {
                // SAFETY: this non-null pool remains borrowed from its live Board.
                unsafe { $capacity(self.ptr.as_ptr()) }
            }

            pub fn free_list_head(self) -> u32 {
                // SAFETY: this non-null pool remains borrowed from its live Board.
                unsafe { $free(self.ptr.as_ptr()) }
            }

            pub fn next_id_key(self) -> u32 {
                // SAFETY: this non-null pool remains borrowed from its live Board.
                unsafe { $next_key(self.ptr.as_ptr()) }
            }

            pub fn slot(self, index: u32) -> Result<PoolSlot> {
                let mut active = 0;
                let mut id = 0;
                let mut next_free = 0;
                // SAFETY: this pool is live and all output pointers are writable local storage.
                Error::from_status(unsafe {
                    $slot(
                        self.ptr.as_ptr(),
                        index,
                        &raw mut active,
                        &raw mut id,
                        &raw mut next_free,
                    )
                })?;
                Ok(PoolSlot {
                    active: active != 0,
                    id,
                    next_free,
                })
            }

            pub fn get(self, index: i32) -> Result<Option<$object_ref<'a>>> {
                out_optional_ptr(|out| {
                    // SAFETY: this pool is live and `out` is writable.
                    unsafe { $get(self.ptr.as_ptr(), index, out) }
                })
                .map(|ptr| ptr.map(Borrowed::new))
            }

            pub fn try_to_get(self, id: u32) -> Option<$object_ref<'a>> {
                if id >> 16 == 0 {
                    return None;
                }
                // SAFETY: this pool is live; DataArrayTryToGet checks capacity and the complete ID before indexing.
                NonNull::new(unsafe { $try_get(self.ptr.as_ptr(), id) }).map(Borrowed::new)
            }

            pub fn get_id(self, object: $object_ref<'a>) -> Result<u32> {
                out_value(|out| {
                    // SAFETY: the pool and object share the same world borrow.
                    unsafe { $get_id(self.ptr.as_ptr(), object.as_ptr(), out) }
                })
            }
        }
    };
}

pool!(
    PlantPool,
    raw::pvzp_rs_plant_pool,
    raw::pvzp_rs_plant,
    PlantRef,
    raw::pvzp_rs_plant_pool_max_used_count,
    raw::pvzp_rs_plant_pool_active_count,
    raw::pvzp_rs_plant_pool_capacity,
    raw::pvzp_rs_plant_pool_free_list_head,
    raw::pvzp_rs_plant_pool_next_id_key,
    raw::pvzp_rs_plant_pool_slot,
    raw::pvzp_rs_plant_pool_get,
    raw::pvzp_rs_plant_pool_try_to_get,
    raw::pvzp_rs_plant_pool_get_id
);
pool!(
    ZombiePool,
    raw::pvzp_rs_zombie_pool,
    raw::pvzp_rs_zombie,
    ZombieRef,
    raw::pvzp_rs_zombie_pool_max_used_count,
    raw::pvzp_rs_zombie_pool_active_count,
    raw::pvzp_rs_zombie_pool_capacity,
    raw::pvzp_rs_zombie_pool_free_list_head,
    raw::pvzp_rs_zombie_pool_next_id_key,
    raw::pvzp_rs_zombie_pool_slot,
    raw::pvzp_rs_zombie_pool_get,
    raw::pvzp_rs_zombie_pool_try_to_get,
    raw::pvzp_rs_zombie_pool_get_id
);
pool!(
    ProjectilePool,
    raw::pvzp_rs_projectile_pool,
    raw::pvzp_rs_projectile,
    ProjectileRef,
    raw::pvzp_rs_projectile_pool_max_used_count,
    raw::pvzp_rs_projectile_pool_active_count,
    raw::pvzp_rs_projectile_pool_capacity,
    raw::pvzp_rs_projectile_pool_free_list_head,
    raw::pvzp_rs_projectile_pool_next_id_key,
    raw::pvzp_rs_projectile_pool_slot,
    raw::pvzp_rs_projectile_pool_get,
    raw::pvzp_rs_projectile_pool_try_to_get,
    raw::pvzp_rs_projectile_pool_get_id
);
pool!(
    GridItemPool,
    raw::pvzp_rs_grid_item_pool,
    raw::pvzp_rs_grid_item,
    GridItemRef,
    raw::pvzp_rs_grid_item_pool_max_used_count,
    raw::pvzp_rs_grid_item_pool_active_count,
    raw::pvzp_rs_grid_item_pool_capacity,
    raw::pvzp_rs_grid_item_pool_free_list_head,
    raw::pvzp_rs_grid_item_pool_next_id_key,
    raw::pvzp_rs_grid_item_pool_slot,
    raw::pvzp_rs_grid_item_pool_get,
    raw::pvzp_rs_grid_item_pool_try_to_get,
    raw::pvzp_rs_grid_item_pool_get_id
);
pool!(
    ItemPool,
    raw::pvzp_rs_item_pool,
    raw::pvzp_rs_item,
    ItemRef,
    raw::pvzp_rs_item_pool_max_used_count,
    raw::pvzp_rs_item_pool_active_count,
    raw::pvzp_rs_item_pool_capacity,
    raw::pvzp_rs_item_pool_free_list_head,
    raw::pvzp_rs_item_pool_next_id_key,
    raw::pvzp_rs_item_pool_slot,
    raw::pvzp_rs_item_pool_get,
    raw::pvzp_rs_item_pool_try_to_get,
    raw::pvzp_rs_item_pool_get_id
);

/// The caller must ensure the call initializes the complete output before returning.
unsafe fn out_native_value<T>(call: impl FnOnce(*mut T)) -> T {
    let mut value = std::mem::MaybeUninit::uninit();
    call(value.as_mut_ptr());
    // SAFETY: required by the caller contract above.
    unsafe { value.assume_init() }
}

fn out_value<T>(call: impl FnOnce(*mut T) -> raw::pvzp_rs_status) -> Result<T> {
    let mut value = std::mem::MaybeUninit::uninit();
    Error::from_status(call(value.as_mut_ptr()))?;
    // SAFETY: a successful bridge call initializes its required out parameter.
    Ok(unsafe { value.assume_init() })
}

fn out_optional_ptr<T>(call: impl FnOnce(*mut *mut T) -> raw::pvzp_rs_status) -> Result<Option<NonNull<T>>> {
    let mut ptr = std::ptr::null_mut();
    Error::from_status(call(&raw mut ptr))?;
    Ok(NonNull::new(ptr))
}

fn out_required_ptr<T>(call: impl FnOnce(*mut *mut T) -> raw::pvzp_rs_status) -> Result<NonNull<T>> {
    out_optional_ptr(call)?.ok_or(Error::NullPointer)
}

impl<'a> Borrowed<'a, raw::pvzp_rs_zombie> {
    pub fn is_dead_or_dying(self) -> bool {
        // SAFETY: the zombie borrow remains address-valid and the query only reads its fields.
        unsafe { raw::pvzp_rs_zombie_is_dead_or_dying(self.as_ptr()) != 0 }
    }

    /// Applies native loot-producing death without looking up the current Board again.
    pub fn die_with_loot(self) -> Result<()> {
        // SAFETY: this occupied zombie slot remains valid for its World borrow.
        Error::from_status(unsafe { raw::pvzp_rs_zombie_die_with_loot(self.as_ptr()) })
    }
    pub fn reanimation(self) -> Result<Option<ReanimationRef<'a>>> {
        out_optional_ptr(|out| {
            // SAFETY: `zombie` is borrow-bound and `out` is writable.
            unsafe { raw::pvzp_rs_zombie_reanimation(self.as_ptr(), out) }
        })
        .map(|ptr| ptr.map(Borrowed::new))
    }
}
