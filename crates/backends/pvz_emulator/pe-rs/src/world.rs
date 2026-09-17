#![allow(unsafe_code)]

use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ptr::NonNull;
use std::rc::Rc;

use crate::error::{Error, Result};
use crate::kinds::{
    KernelPultRule, MaidCheat, PlantDamageRule, PlantType, PlantWeapon, PlantingReason, SceneType, ZombieDanceCheat,
    ZombieType,
};
use crate::raw;

/// A pointer into PE storage whose lifetime is bounded by the current world borrow.
///
/// This type never creates a Rust reference to storage mutated by the emulator.
pub struct Borrowed<'a, T> {
    ptr: NonNull<T>,
    _borrow: PhantomData<&'a World>,
}

impl<T> Copy for Borrowed<'_, T> {}

impl<T> Clone for Borrowed<'_, T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> std::fmt::Debug for Borrowed<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Borrowed").field(&self.ptr).finish()
    }
}

impl<T> PartialEq for Borrowed<'_, T> {
    fn eq(&self, other: &Self) -> bool {
        self.ptr == other.ptr
    }
}

impl<T> Eq for Borrowed<'_, T> {}

impl<'a, T> Borrowed<'a, T> {
    fn new(ptr: NonNull<T>) -> Self {
        Self {
            ptr,
            _borrow: PhantomData,
        }
    }

    /// Rebinds a raw PE pointer to an externally enforced unique-world borrow.
    ///
    /// # Safety
    ///
    /// `ptr` must belong to the current live world, and `'a` must not outlive
    /// that world's backend borrow or cross an invalidating operation.
    #[doc(hidden)]
    pub unsafe fn from_non_null(ptr: NonNull<T>) -> Self {
        Self::new(ptr)
    }

    #[must_use]
    pub const fn as_ptr(self) -> *mut T {
        self.ptr.as_ptr()
    }

    #[must_use]
    pub const fn as_non_null(self) -> NonNull<T> {
        self.ptr
    }
}

pub type PlantRef<'a> = Borrowed<'a, raw::pe_rs_plant>;
pub type ZombieRef<'a> = Borrowed<'a, raw::pe_rs_zombie>;
pub type GridItemRef<'a> = Borrowed<'a, raw::pe_rs_griditem>;
pub type ProjectileRef<'a> = Borrowed<'a, raw::pe_rs_projectile>;
pub type CardRef<'a> = Borrowed<'a, raw::pe_rs_card>;
pub type GridPlantStatusRef<'a> = Borrowed<'a, raw::pe_rs_grid_plant_status>;
pub type SpawnDataRef<'a> = Borrowed<'a, raw::pe_rs_spawn_data>;
pub type SunDataRef<'a> = Borrowed<'a, raw::pe_rs_sun_data>;
pub type IcePathDataRef<'a> = Borrowed<'a, raw::pe_rs_ice_path_data>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolSlot {
    pub active: bool,
    pub id: u32,
    pub next_free: u32,
}

/// Owns one native PE world. Construction and destruction are the only heap
/// allocation performed by this facade.
///
/// Scene and object views statically block every operation that can invalidate
/// their native pointers:
///
/// ```compile_fail
/// use pe_rs::{SceneType, World};
/// let mut world = World::new_deterministic(SceneType::Pool, 1, 2, 3).unwrap();
/// let scene = world.scene();
/// world.update().unwrap();
/// drop(scene);
/// ```
///
/// ```compile_fail
/// use pe_rs::{PlantType, SceneType, World};
/// let mut world = World::new_deterministic(SceneType::Pool, 1, 2, 3).unwrap();
/// let plant = world
///     .new_plant(0, 0, PlantType::Sunflower, PlantType::None)
///     .unwrap()
///     .unwrap();
/// world.reset_deterministic(SceneType::Roof, 4, 5, 6).unwrap();
/// drop(plant);
/// ```
///
/// Views also inherit the world's worker-thread affinity:
///
/// ```compile_fail
/// use pe_rs::{SceneType, World};
/// let world = World::new_deterministic(SceneType::Pool, 1, 2, 3).unwrap();
/// let scene = world.scene();
/// std::thread::scope(|scope| {
///     scope.spawn(move || drop(scene));
/// });
/// ```
///
/// ```compile_fail
/// use pe_rs::{SceneType, World};
/// let world = World::new_deterministic(SceneType::Pool, 1, 2, 3).unwrap();
/// let scene = world.scene();
/// std::thread::spawn(move || drop(scene));
/// ```
pub struct World {
    raw: NonNull<raw::pe_rs_world>,
    scene: NonNull<raw::pe_rs_scene>,
    _thread_bound: PhantomData<Rc<()>>,
}

impl World {
    pub fn new_deterministic(scene: SceneType, battle_seed: u32, level_seed: u32, dancer_clock: u32) -> Result<Self> {
        let raw = out_required_ptr(|out| {
            // SAFETY: `out` points to writable storage and all arguments are validated scalars.
            unsafe { raw::pe_rs_world_new_deterministic(scene.to_raw(), battle_seed, level_seed, dancer_clock, out) }
        })?;
        Self::from_raw(raw)
    }

    pub fn set_scene_type(&mut self, scene: SceneType) -> Result<()> {
        // SAFETY: `self.raw` remains uniquely owned and the scalar enum was validated by Rust.
        Error::from_status(unsafe { raw::pe_rs_world_set_scene_type(self.raw.as_ptr(), scene.to_raw()) })
    }

    fn from_raw(raw: NonNull<raw::pe_rs_world>) -> Result<Self> {
        let scene = match out_required_ptr(|out| {
            // SAFETY: the newly allocated world is live and `out` is writable.
            unsafe { raw::pe_rs_world_scene(raw.as_ptr(), out) }
        }) {
            Ok(scene) => scene,
            Err(error) => {
                // SAFETY: allocation succeeded and ownership has not escaped.
                let _ = unsafe { raw::pe_rs_world_free(raw.as_ptr()) };
                return Err(error);
            }
        };
        Ok(Self {
            raw,
            scene,
            _thread_bound: PhantomData,
        })
    }

    pub fn reset_deterministic(
        &mut self, scene: SceneType, battle_seed: u32, level_seed: u32, dancer_clock: u32,
    ) -> Result<()> {
        // SAFETY: `self.raw` remains uniquely owned and all arguments are validated scalars.
        Error::from_status(unsafe {
            raw::pe_rs_world_reset_deterministic(
                self.raw.as_ptr(),
                scene.to_raw(),
                battle_seed,
                level_seed,
                dancer_clock,
            )
        })
    }

    /// Advances one logical PE frame and returns the legacy terminal flag.
    pub fn update(&mut self) -> Result<bool> {
        out_value(|out| {
            // SAFETY: both pointers are valid for the duration of the call.
            unsafe { raw::pe_rs_world_update(self.raw.as_ptr(), out) }
        })
        .map(|value: u8| value != 0)
    }

    #[doc(hidden)]
    #[allow(clippy::too_many_arguments, reason = "mirrors the fixed native event sink ABI")]
    pub unsafe fn set_event_sink(
        &mut self, interest: u32, begin_plant_effect: raw::pe_rs_begin_plant_effect_fn,
        finish_plant_effect: raw::pe_rs_finish_plant_effect_fn, emit_home_entry: raw::pe_rs_emit_home_entry_fn,
        emit_gargantuar_spawned: raw::pe_rs_emit_gargantuar_spawned_fn, emit_imp_thrown: raw::pe_rs_emit_imp_thrown_fn,
        emit_gargantuar_ash_hit: raw::pe_rs_emit_gargantuar_ash_hit_fn,
    ) -> Result<()> {
        // SAFETY: callback lifetimes are owned by the caller and the native
        // scene stores only the supplied copy-only function pointers.
        Error::from_status(unsafe {
            raw::pe_rs_world_set_event_sink(
                self.raw.as_ptr(),
                interest,
                begin_plant_effect,
                finish_plant_effect,
                emit_home_entry,
                emit_gargantuar_spawned,
                emit_imp_thrown,
                emit_gargantuar_ash_hit,
            )
        })
    }

    #[doc(hidden)]
    pub fn clear_event_sink(&mut self) -> Result<()> {
        // SAFETY: `self.raw` remains owned and live for this call.
        Error::from_status(unsafe { raw::pe_rs_world_clear_event_sink(self.raw.as_ptr()) })
    }

    #[doc(hidden)]
    pub fn restore_dancer_clock(&self, clock: u32) -> Result<()> {
        // SAFETY: the world is live and PE writes one address-stable timing scalar.
        Error::from_status(unsafe { raw::pe_rs_world_restore_dancer_clock(self.raw.as_ptr(), clock) })
    }

    pub fn scene(&self) -> Scene<'_> {
        Scene {
            ptr: self.scene,
            _borrow: PhantomData,
        }
    }

    /// Replaces a bank using PE's native cooldown-preserving reselection.
    pub fn select_plants(&self, cards: &[crate::PlantType], imitater: crate::PlantType) -> Result<()> {
        // SAFETY: repr(i32) elements, live owner, synchronous read of the bounded slice.
        Error::from_status(unsafe {
            raw::pe_rs_world_select_plants(
                self.raw.as_ptr(),
                cards.as_ptr().cast(),
                cards.len() as u32,
                imitater.to_raw(),
            )
        })
    }

    /// Returns the exact successor created by the most recent native morph of
    /// this imitater placeholder. The relation expires when PE reclaims the
    /// dead placeholder on its next update.
    pub fn imitater_morph_successor(&self, placeholder_id: u32) -> Option<u32> {
        // SAFETY: this world is live; native lookup checks the scalar generation ID.
        let successor = unsafe { raw::pe_rs_world_imitater_morph_successor(self.raw.as_ptr(), placeholder_id) };
        (successor != 0).then_some(successor)
    }

    pub fn reset_sun(&self) -> Result<()> {
        // SAFETY: the world is live for the call.
        Error::from_status(unsafe { raw::pe_rs_world_reset_sun(self.raw.as_ptr()) })
    }

    pub fn reset_spawn(&self) -> Result<()> {
        // SAFETY: the world is live for the call.
        Error::from_status(unsafe { raw::pe_rs_world_reset_spawn(self.raw.as_ptr()) })
    }

    pub fn total_zombies_health_in_wave(&self, wave: u32) -> i32 {
        // SAFETY: the owner-bound borrow is live; native query handles the scalar inputs.
        unsafe { raw::pe_rs_spawn_total_zombies_health_in_wave(self.raw.as_ptr(), wave) }
    }

    pub fn current_spawn_health(&self) -> u32 {
        // SAFETY: the owner-bound borrow is live; native query handles the scalar inputs.
        unsafe { raw::pe_rs_spawn_current_health(self.raw.as_ptr()) }
    }

    pub fn pick_spawn_list(&self) -> Result<bool> {
        out_value(|out| {
            // SAFETY: the world is live and `out` is writable.
            unsafe { raw::pe_rs_spawn_pick_list(self.raw.as_ptr(), out) }
        })
        .map(|value: u8| value != 0)
    }

    pub fn current_plant_cost(&self, seed: PlantType, imitater: PlantType) -> Result<u32> {
        let cost: i32 = out_value(|out| {
            // SAFETY: the world is live and enum scalars/out storage are valid.
            unsafe { raw::pe_rs_current_plant_cost(self.raw.as_ptr(), seed.to_raw(), imitater.to_raw(), out) }
        })?;
        u32::try_from(cost).map_err(|_| Error::InvalidArgument { detail: cost })
    }

    pub fn can_take_sun_money(&self, amount: i32) -> Result<bool> {
        out_value(|out| {
            // SAFETY: the world is live and `out` is writable.
            unsafe { raw::pe_rs_can_take_sun_money(self.raw.as_ptr(), amount, out) }
        })
        .map(|value: u8| value != 0)
    }

    pub fn take_sun_money(&self, amount: i32) -> Result<bool> {
        out_value(|out| {
            // SAFETY: the world is live and PE performs an address-stable scalar mutation.
            unsafe { raw::pe_rs_take_sun_money(self.raw.as_ptr(), amount, out) }
        })
        .map(|value: u8| value != 0)
    }

    pub fn seed_can_pick_up(&self, card: CardRef<'_>) -> Result<bool> {
        out_value(|out| {
            // SAFETY: `card` is a borrow-bound PE pointer; the bridge verifies bank membership.
            unsafe { raw::pe_rs_seed_can_pick_up(self.raw.as_ptr(), card.as_ptr(), out) }
        })
        .map(|value: u8| value != 0)
    }

    pub fn seed_was_planted(&self, card: CardRef<'_>) -> Result<()> {
        // SAFETY: `card` is borrow-bound and the bridge verifies bank membership.
        Error::from_status(unsafe { raw::pe_rs_seed_was_planted(self.raw.as_ptr(), card.as_ptr()) })
    }

    pub fn can_plant_at(&self, grid_x: i32, grid_y: i32, planting: PlantType) -> Result<PlantingReason> {
        let reason = out_value(|out| {
            // SAFETY: the world is live and scalar arguments/out storage are valid.
            unsafe { raw::pe_rs_can_plant_at(self.raw.as_ptr(), grid_x, grid_y, planting.to_raw(), out) }
        })?;
        PlantingReason::from_raw(reason).ok_or(Error::InvalidArgument { detail: reason })
    }

    pub fn new_plant(
        &self, grid_x: i32, grid_y: i32, seed: PlantType, imitater: PlantType,
    ) -> Result<Option<PlantRef<'_>>> {
        out_optional_ptr(|out| {
            // SAFETY: the world is live and arguments/out storage are valid.
            unsafe { raw::pe_rs_new_plant(self.raw.as_ptr(), grid_x, grid_y, seed.to_raw(), imitater.to_raw(), out) }
        })
        .map(|ptr| ptr.map(Borrowed::new))
    }

    pub fn add_plant(
        &self, grid_x: i32, grid_y: i32, seed: PlantType, imitater: PlantType,
    ) -> Result<Option<PlantRef<'_>>> {
        out_optional_ptr(|out| {
            // SAFETY: the world is live and arguments/out storage are valid.
            unsafe { raw::pe_rs_add_plant(self.raw.as_ptr(), grid_x, grid_y, seed.to_raw(), imitater.to_raw(), out) }
        })
        .map(|ptr| ptr.map(Borrowed::new))
    }

    pub fn plant_die(&self, plant: PlantRef<'_>) -> Result<()> {
        // SAFETY: `plant` is borrow-bound; the bridge revalidates it against this pool.
        Error::from_status(unsafe { raw::pe_rs_plant_die(self.raw.as_ptr(), plant.as_ptr()) })
    }

    pub fn plant_imitater_morph(&self, plant: PlantRef<'_>) -> Result<PlantRef<'_>> {
        let mut successor = std::ptr::null_mut();
        // SAFETY: the bridge validates pool membership and type. The pool never
        // relocates, so the returned successor is bounded by this world borrow.
        Error::from_status(unsafe {
            raw::pe_rs_plant_imitater_morph(self.raw.as_ptr(), plant.as_ptr(), &raw mut successor)
        })?;
        Ok(Borrowed::new(
            NonNull::new(successor).expect("successful morph returns its successor"),
        ))
    }

    pub fn plant_set_sleep(&self, plant: PlantRef<'_>, asleep: bool) -> Result<()> {
        // SAFETY: the handle is borrow-bound and this action does not relocate pool storage.
        Error::from_status(unsafe { raw::pe_rs_plant_set_sleep(plant.as_ptr(), u8::from(asleep)) })
    }

    pub fn plant_play_idle(&self, plant: PlantRef<'_>, fps: f32) -> Result<()> {
        // SAFETY: the handle is borrow-bound and this action writes only its object/reanim state.
        Error::from_status(unsafe { raw::pe_rs_plant_play_idle(plant.as_ptr(), fps) })
    }

    pub fn fire_cob(&self, plant: PlantRef<'_>, target_x: i32, target_y: i32) -> Result<bool> {
        out_value(|out| {
            // SAFETY: the handle is borrow-bound and the bridge verifies pool membership.
            unsafe { raw::pe_rs_plant_fire_cob(self.raw.as_ptr(), plant.as_ptr(), target_x, target_y, out) }
        })
        .map(|value: u8| value != 0)
    }

    pub fn plant_hit_box(&self, plant: PlantRef<'_>) -> raw::pe_rs_rect {
        unsafe {
            // SAFETY: the live entity and validated weapon satisfy the native query,
            // which initializes all four output fields in a single call.
            out_native_value(|out| raw::pe_rs_plant_hit_box(plant.as_ptr(), out))
        }
    }

    pub fn plant_attack_rect(&self, plant: PlantRef<'_>, weapon: PlantWeapon) -> raw::pe_rs_rect {
        unsafe {
            // SAFETY: the live entity and validated weapon satisfy the native query,
            // which initializes all four output fields in a single call.
            out_native_value(|out| raw::pe_rs_plant_attack_rect(plant.as_ptr(), weapon as i32, out))
        }
    }

    pub fn plant_damage_range_flags(&self, plant: PlantRef<'_>, weapon: PlantWeapon) -> i32 {
        // SAFETY: the plant is borrowed and PlantWeapon has only the two native variants.
        unsafe { raw::pe_rs_plant_damage_range_flags(plant.as_ptr(), weapon as i32) }
    }

    pub fn add_zombie_in_row(&self, zombie: ZombieType, row: i32, from_wave: i32) -> Result<Option<ZombieRef<'_>>> {
        out_optional_ptr(|out| {
            // SAFETY: the world is live and scalar arguments/out storage are valid.
            unsafe { raw::pe_rs_add_zombie_in_row(self.raw.as_ptr(), zombie.to_raw(), row, from_wave, out) }
        })
        .map(|ptr| ptr.map(Borrowed::new))
    }

    pub fn place_zombie(&self, zombie: ZombieType, grid_x: i32, grid_y: i32) -> Result<Option<ZombieRef<'_>>> {
        out_optional_ptr(|out| {
            // SAFETY: the world is live and arguments/out storage are valid.
            unsafe { raw::pe_rs_place_zombie(self.raw.as_ptr(), zombie.to_raw(), grid_x, grid_y, out) }
        })
        .map(|ptr| ptr.map(Borrowed::new))
    }

    pub fn zombie_die_no_loot(&self, zombie: ZombieRef<'_>) -> Result<()> {
        // SAFETY: the handle is borrow-bound and the bridge verifies pool membership.
        Error::from_status(unsafe { raw::pe_rs_zombie_die_no_loot(self.raw.as_ptr(), zombie.as_ptr()) })
    }

    pub fn zombie_can_be_attacked(&self, zombie: ZombieRef<'_>, flags: u8) -> Result<bool> {
        out_value(|out| {
            // SAFETY: the handle is borrow-bound and `out` is writable.
            unsafe { raw::pe_rs_zombie_can_be_attacked(self.raw.as_ptr(), zombie.as_ptr(), flags, out) }
        })
        .map(|value: u8| value != 0)
    }

    pub fn zombie_can_attack_plant(
        &self, zombie: ZombieRef<'_>, plant: PlantRef<'_>, attack_type: i32,
    ) -> Result<bool> {
        out_value(|out| {
            // SAFETY: both handles are borrow-bound and `out` is writable.
            unsafe {
                raw::pe_rs_zombie_can_attack_plant(self.raw.as_ptr(), zombie.as_ptr(), plant.as_ptr(), attack_type, out)
            }
        })
        .map(|value: u8| value != 0)
    }

    pub fn zombie_hit_box(&self, zombie: ZombieRef<'_>) -> raw::pe_rs_rect {
        unsafe {
            // SAFETY: the live entity and validated weapon satisfy the native query,
            // which initializes all four output fields in a single call.
            out_native_value(|out| raw::pe_rs_zombie_hit_box(zombie.as_ptr(), out))
        }
    }

    pub fn projectile_attack_box(&self, projectile: ProjectileRef<'_>) -> raw::pe_rs_rect {
        unsafe {
            // SAFETY: the live entity and validated weapon satisfy the native query,
            // which initializes all four output fields in a single call.
            out_native_value(|out| raw::pe_rs_projectile_attack_box(projectile.as_ptr(), out))
        }
    }

    pub fn add_ladder(&self, grid_x: i32, grid_y: i32) -> Result<Option<GridItemRef<'_>>> {
        self.add_griditem(grid_x, grid_y, raw::pe_rs_add_ladder)
    }

    pub fn add_crater(&self, grid_x: i32, grid_y: i32) -> Result<Option<GridItemRef<'_>>> {
        self.add_griditem(grid_x, grid_y, raw::pe_rs_add_crater)
    }

    pub fn add_gravestone(&self, grid_x: i32, grid_y: i32) -> Result<Option<GridItemRef<'_>>> {
        self.add_griditem(grid_x, grid_y, raw::pe_rs_add_gravestone)
    }

    fn add_griditem<'a>(
        &'a self, grid_x: i32, grid_y: i32,
        add: unsafe extern "C" fn(*mut raw::pe_rs_world, i32, i32, *mut *mut raw::pe_rs_griditem) -> raw::pe_rs_status,
    ) -> Result<Option<GridItemRef<'a>>> {
        out_optional_ptr(|out| {
            // SAFETY: the world is live and the selected bridge function has this exact ABI.
            unsafe { add(self.raw.as_ptr(), grid_x, grid_y, out) }
        })
        .map(|ptr| ptr.map(Borrowed::new))
    }

    pub fn griditem_die(&self, item: GridItemRef<'_>) -> Result<()> {
        // SAFETY: the handle is borrow-bound and the bridge verifies pool membership.
        Error::from_status(unsafe { raw::pe_rs_griditem_die(self.raw.as_ptr(), item.as_ptr()) })
    }
}

impl Drop for World {
    fn drop(&mut self) {
        // SAFETY: this is the unique owned native handle and Drop runs once.
        let _ = unsafe { raw::pe_rs_world_free(self.raw.as_ptr()) };
    }
}

#[derive(Clone, Copy)]
pub struct Scene<'a> {
    ptr: NonNull<raw::pe_rs_scene>,
    _borrow: PhantomData<&'a World>,
}

impl<'a> Scene<'a> {
    /// Rebinds a scene to an externally enforced backend borrow.
    ///
    /// # Safety
    /// `ptr` must remain the live scene throughout `'a`, without update or reset.
    #[doc(hidden)]
    pub unsafe fn from_non_null(ptr: NonNull<raw::pe_rs_scene>) -> Self {
        Self {
            ptr,
            _borrow: PhantomData,
        }
    }

    #[doc(hidden)]
    pub const fn as_non_null(self) -> NonNull<raw::pe_rs_scene> {
        self.ptr
    }

    pub fn seed_rngs(self, battle_seed: u32, level_seed: u32) -> Result<()> {
        // SAFETY: this scene is borrowed from a live world.
        Error::from_status(unsafe { raw::pe_rs_scene_seed_rngs(self.ptr.as_ptr(), battle_seed, level_seed) })
    }

    pub fn lock_rngs(self, value: u32) -> Result<()> {
        // SAFETY: this scene is borrowed from a live world.
        Error::from_status(unsafe { raw::pe_rs_scene_lock_rngs(self.ptr.as_ptr(), value) })
    }

    pub fn set_wave_spawn_random_seed(self, base_seed: Option<u32>) -> Result<()> {
        // SAFETY: this scene is borrowed from a live world.
        Error::from_status(unsafe {
            raw::pe_rs_scene_set_wave_spawn_random_seed(
                self.ptr.as_ptr(),
                u8::from(base_seed.is_some()),
                base_seed.unwrap_or(0),
            )
        })
    }

    pub fn battle_randint(self, upper: u32) -> Result<u32> {
        out_value(|out| {
            // SAFETY: this scene is live and `out` is writable.
            unsafe { raw::pe_rs_scene_battle_randint(self.ptr.as_ptr(), upper, out) }
        })
    }

    pub fn rng_seed(self, level_stream: bool) -> u32 {
        // SAFETY: the scene is live and bool admits exactly the two native streams.
        unsafe { raw::pe_rs_scene_rng_seed(self.ptr.as_ptr(), level_stream) }
    }
    pub fn rng_locked(self, level_stream: bool) -> bool {
        // SAFETY: the scene is live and bool admits exactly the two native streams.
        unsafe { raw::pe_rs_scene_rng_locked(self.ptr.as_ptr(), level_stream) != 0 }
    }
    pub fn rng_fixed(self, level_stream: bool) -> u32 {
        // SAFETY: the scene is live and bool admits exactly the two native streams.
        unsafe { raw::pe_rs_scene_rng_fixed(self.ptr.as_ptr(), level_stream) }
    }

    pub fn main_counter(self) -> u32 {
        // SAFETY: Scene holds a live non-null scene for this borrow.
        unsafe { raw::pe_rs_scene_main_counter(self.ptr.as_ptr()) }
    }

    pub fn set_main_counter(self, counter: u32) -> Result<()> {
        // SAFETY: the scene is live for this address-stable scalar write.
        Error::from_status(unsafe { raw::pe_rs_scene_set_main_counter(self.ptr.as_ptr(), counter) })
    }

    pub fn game_over(self) -> bool {
        // SAFETY: this scene remains live for the current world borrow.
        unsafe { raw::pe_rs_scene_game_over(self.ptr.as_ptr()) != 0 }
    }

    pub fn dancer_clock(self) -> u32 {
        // SAFETY: Scene holds a live non-null scene for this borrow.
        unsafe { raw::pe_rs_scene_dancer_clock(self.ptr.as_ptr()) }
    }

    pub fn spawn_data(self) -> SpawnDataRef<'a> {
        // SAFETY: the bridge returns an embedded member of this live scene.
        unsafe { SpawnDataRef::new(NonNull::new_unchecked(raw::pe_rs_scene_spawn_data(self.ptr.as_ptr()))) }
    }

    pub fn sun_data(self) -> SunDataRef<'a> {
        // SAFETY: the bridge returns an embedded member of this live scene.
        unsafe { SunDataRef::new(NonNull::new_unchecked(raw::pe_rs_scene_sun_data(self.ptr.as_ptr()))) }
    }

    pub fn ice_path_data(self) -> IcePathDataRef<'a> {
        // SAFETY: the bridge returns an embedded member of this live scene.
        unsafe {
            IcePathDataRef::new(NonNull::new_unchecked(raw::pe_rs_scene_ice_path_data(
                self.ptr.as_ptr(),
            )))
        }
    }

    pub fn projectiles(self) -> ProjectilePool<'a> {
        // SAFETY: the bridge returns an embedded member of this live scene.
        unsafe {
            ProjectilePool::new(NonNull::new_unchecked(raw::pe_rs_scene_projectile_pool(
                self.ptr.as_ptr(),
            )))
        }
    }

    pub fn card_at(self, slot: u32) -> Result<CardRef<'a>> {
        out_required_ptr(|out| {
            // SAFETY: the scene is live and `out` is writable.
            unsafe { raw::pe_rs_scene_card_at(self.ptr.as_ptr(), slot, out) }
        })
        .map(Borrowed::new)
    }

    pub fn grid_plant_status_at(self, grid_x: i32, grid_y: i32) -> Result<GridPlantStatusRef<'a>> {
        out_required_ptr(|out| {
            // SAFETY: the scene is live and `out` is writable.
            unsafe { raw::pe_rs_scene_grid_plant_status_at(self.ptr.as_ptr(), grid_x, grid_y, out) }
        })
        .map(Borrowed::new)
    }

    pub fn plants(self) -> PlantPool<'a> {
        // SAFETY: the bridge returns an embedded member of this live scene.
        unsafe { PlantPool::new(NonNull::new_unchecked(raw::pe_rs_scene_plant_pool(self.ptr.as_ptr()))) }
    }

    pub fn zombies(self) -> ZombiePool<'a> {
        // SAFETY: the bridge returns an embedded member of this live scene.
        unsafe { ZombiePool::new(NonNull::new_unchecked(raw::pe_rs_scene_zombie_pool(self.ptr.as_ptr()))) }
    }

    pub fn griditems(self) -> GridItemPool<'a> {
        // SAFETY: the bridge returns an embedded member of this live scene.
        unsafe {
            GridItemPool::new(NonNull::new_unchecked(raw::pe_rs_scene_griditem_pool(
                self.ptr.as_ptr(),
            )))
        }
    }

    pub fn is_pool_square(self, grid_x: i32, grid_y: i32) -> bool {
        // SAFETY: the owner-bound borrow is live; native query handles the scalar inputs.
        unsafe { raw::pe_rs_scene_is_pool_square(self.ptr.as_ptr(), grid_x, grid_y) != 0 }
    }

    pub fn row_can_have_zombies(self, row: i32) -> bool {
        // SAFETY: the owner-bound borrow is live; native query handles the scalar inputs.
        unsafe { raw::pe_rs_scene_row_can_have_zombies(self.ptr.as_ptr(), row) != 0 }
    }

    pub fn grid_to_pixel_x(self, grid_x: i32, grid_y: i32) -> Result<i32> {
        out_value(|out| {
            // SAFETY: the scene is live and `out` is writable.
            unsafe { raw::pe_rs_scene_grid_to_pixel_x(self.ptr.as_ptr(), grid_x, grid_y, out) }
        })
    }

    pub fn grid_to_pixel_y(self, grid_x: i32, grid_y: i32) -> Result<i32> {
        out_value(|out| {
            // SAFETY: the scene is live and `out` is writable.
            unsafe { raw::pe_rs_scene_grid_to_pixel_y(self.ptr.as_ptr(), grid_x, grid_y, out) }
        })
    }

    pub fn pos_y_based_on_row(self, pos_x: f32, row: i32) -> Result<f32> {
        out_value(|out| {
            // SAFETY: the scene is live and `out` is writable.
            unsafe { raw::pe_rs_scene_pos_y_based_on_row(self.ptr.as_ptr(), pos_x, row, out) }
        })
    }

    pub fn spawn_entry(self, wave: u32, slot: u32) -> Result<ZombieType> {
        let raw_type = out_value(|out| {
            // SAFETY: the scene is live and `out` is writable.
            unsafe { raw::pe_rs_spawn_entry(self.ptr.as_ptr(), wave, slot, out) }
        })?;
        ZombieType::from_raw(raw_type).ok_or(Error::InvalidArgument { detail: raw_type })
    }

    pub fn set_spawn_entry(self, wave: u32, slot: u32, zombie: ZombieType) -> Result<()> {
        // SAFETY: the scene is live and the remaining arguments are scalars.
        Error::from_status(unsafe { raw::pe_rs_spawn_set_entry(self.ptr.as_ptr(), wave, slot, zombie.to_raw()) })
    }

    pub fn spawn_flag(self, zombie_type: u32) -> Result<bool> {
        out_value(|out| {
            // SAFETY: the scene is live and `out` is writable.
            unsafe { raw::pe_rs_spawn_flag(self.ptr.as_ptr(), zombie_type, out) }
        })
        .map(|value: u8| value != 0)
    }

    pub fn set_spawn_flag(self, zombie_type: u32, enabled: bool) -> Result<()> {
        // SAFETY: the scene is live and the remaining arguments are scalars.
        Error::from_status(unsafe { raw::pe_rs_spawn_set_flag(self.ptr.as_ptr(), zombie_type, u8::from(enabled)) })
    }

    pub fn kernel_pult_rule(self) -> Result<KernelPultRule> {
        // SAFETY: the scene is live; enum validity is checked below.
        let rule = unsafe { raw::pe_rs_scene_kernel_pult_rule(self.ptr.as_ptr()) };
        match rule {
            0 => Ok(KernelPultRule::Normal),
            1 => Ok(KernelPultRule::AlwaysButter),
            2 => Ok(KernelPultRule::AlwaysKernel),
            _ => Err(Error::InvalidArgument { detail: rule }),
        }
    }

    pub fn set_kernel_pult_rule(self, rule: KernelPultRule) -> Result<()> {
        // SAFETY: the scene is live and the enum has a validated representation.
        Error::from_status(unsafe { raw::pe_rs_scene_set_kernel_pult_rule(self.ptr.as_ptr(), rule as i32) })
    }

    pub fn plant_damage_rule(self) -> Result<PlantDamageRule> {
        // SAFETY: the scene is live; enum validity is checked below.
        let rule = unsafe { raw::pe_rs_scene_plant_damage_rule(self.ptr.as_ptr()) };
        match rule {
            0 => Ok(PlantDamageRule::Normal),
            1 => Ok(PlantDamageRule::Invincible),
            2 => Ok(PlantDamageRule::Weak),
            _ => Err(Error::InvalidArgument { detail: rule }),
        }
    }

    pub fn set_plant_damage_rule(self, rule: PlantDamageRule) -> Result<()> {
        // SAFETY: the scene is live and the enum has a validated representation.
        Error::from_status(unsafe { raw::pe_rs_scene_set_plant_damage_rule(self.ptr.as_ptr(), rule as i32) })
    }

    pub fn maid_cheat(self) -> Result<MaidCheat> {
        // SAFETY: the scene is live; enum validity is checked below.
        let state = unsafe { raw::pe_rs_scene_maid_cheat(self.ptr.as_ptr()) };
        MaidCheat::from_raw(state).ok_or(Error::InvalidArgument { detail: state })
    }

    pub fn set_maid_cheat(self, state: MaidCheat) -> Result<()> {
        // SAFETY: the scene is live and the enum has a validated representation.
        Error::from_status(unsafe { raw::pe_rs_scene_set_maid_cheat(self.ptr.as_ptr(), state.to_raw()) })
    }

    pub fn set_common_zombie_dance(self, state: ZombieDanceCheat) -> Result<()> {
        // SAFETY: the scene is live and the enum has a validated representation.
        Error::from_status(unsafe { raw::pe_rs_scene_set_common_zombie_dance(self.ptr.as_ptr(), state.to_raw()) })
    }

    pub fn set_dance_mode(self, enabled: bool) -> Result<()> {
        // SAFETY: the scene is live and the native operation does not recycle objects.
        Error::from_status(unsafe { raw::pe_rs_scene_set_dance_mode(self.ptr.as_ptr(), u8::from(enabled)) })
    }
}

macro_rules! bool_modifier {
    ($get:ident, $set:ident, $raw_get:ident, $raw_set:ident) => {
        impl Scene<'_> {
            pub fn $get(self) -> bool {
                // SAFETY: this scene remains live for its owner-bound borrow.
                unsafe { raw::$raw_get(self.ptr.as_ptr()) != 0 }
            }

            pub fn $set(self, enabled: bool) -> Result<()> {
                // SAFETY: the scene is live for this address-stable scalar write.
                Error::from_status(unsafe { raw::$raw_set(self.ptr.as_ptr(), u8::from(enabled)) })
            }
        }
    };
}

bool_modifier!(
    seed_recharge_ignored,
    set_seed_recharge_ignored,
    pe_rs_scene_seed_recharge_ignored,
    pe_rs_scene_set_seed_recharge_ignored
);
bool_modifier!(
    sun_cost_ignored,
    set_sun_cost_ignored,
    pe_rs_scene_sun_cost_ignored,
    pe_rs_scene_set_sun_cost_ignored
);
bool_modifier!(
    instant_special_effects,
    set_instant_special_effects,
    pe_rs_scene_instant_special_effects,
    pe_rs_scene_set_instant_special_effects
);
bool_modifier!(
    easy_planting_cheat,
    set_easy_planting_cheat,
    pe_rs_scene_easy_planting_cheat,
    pe_rs_scene_set_easy_planting_cheat
);
bool_modifier!(
    planting_restrictions_ignored,
    set_planting_restrictions_ignored,
    pe_rs_scene_planting_restrictions_ignored,
    pe_rs_scene_set_planting_restrictions_ignored
);
bool_modifier!(
    mushrooms_awake,
    set_mushrooms_awake,
    pe_rs_scene_mushrooms_awake,
    pe_rs_scene_set_mushrooms_awake
);
bool_modifier!(
    cob_delay_disabled,
    set_cob_delay_disabled,
    pe_rs_scene_cob_delay_disabled,
    pe_rs_scene_set_cob_delay_disabled
);
bool_modifier!(
    cob_fixed_delay,
    set_cob_fixed_delay,
    pe_rs_scene_cob_fixed_delay,
    pe_rs_scene_set_cob_fixed_delay
);
bool_modifier!(
    cob_recharge_shortened,
    set_cob_recharge_shortened,
    pe_rs_scene_cob_recharge_shortened,
    pe_rs_scene_set_cob_recharge_shortened
);
bool_modifier!(
    cob_drift_fixed,
    set_cob_drift_fixed,
    pe_rs_scene_cob_drift_fixed,
    pe_rs_scene_set_cob_drift_fixed
);
bool_modifier!(
    natural_sun_drop_disabled,
    set_natural_sun_drop_disabled,
    pe_rs_scene_natural_sun_drop_disabled,
    pe_rs_scene_set_natural_sun_drop_disabled
);
bool_modifier!(
    jack_explosions_disabled,
    set_jack_explosions_disabled,
    pe_rs_scene_jack_explosions_disabled,
    pe_rs_scene_set_jack_explosions_disabled
);
bool_modifier!(
    special_events_disabled,
    set_special_events_disabled,
    pe_rs_scene_special_events_disabled,
    pe_rs_scene_set_special_events_disabled
);
bool_modifier!(
    zombie_spawn_stopped,
    set_zombie_spawn_stopped,
    pe_rs_scene_zombie_spawn_stopped,
    pe_rs_scene_set_zombie_spawn_stopped
);
bool_modifier!(
    zombies_die_at_house,
    set_zombies_die_at_house,
    pe_rs_scene_zombies_die_at_house,
    pe_rs_scene_set_zombies_die_at_house
);

macro_rules! pool {
    (
        $name:ident, $raw_pool:ty, $raw_object:ty, $object_ref:ident,
        $max:path, $active:path, $capacity:path, $free:path, $next_key:path, $slot:path,
        $get:path, $try_get:path, $get_id:path
    ) => {
        #[derive(Clone, Copy)]
        pub struct $name<'a> {
            ptr: NonNull<$raw_pool>,
            _borrow: PhantomData<&'a World>,
        }

        impl<'a> $name<'a> {
            fn new(ptr: NonNull<$raw_pool>) -> Self {
                Self {
                    ptr,
                    _borrow: PhantomData,
                }
            }

            /// Rebinds pool storage to an externally enforced backend borrow.
            ///
            /// # Safety
            /// `ptr` must belong to the live world for all of `'a`; no world
            /// update, reset, reclamation or pool-origin reset may cross that borrow.
            #[doc(hidden)]
            pub unsafe fn from_non_null(ptr: NonNull<$raw_pool>) -> Self {
                Self::new(ptr)
            }

            #[doc(hidden)]
            pub const fn as_non_null(self) -> NonNull<$raw_pool> {
                self.ptr
            }

            pub fn max_used_count(self) -> u32 {
                // SAFETY: this non-null pool is borrowed from a live scene.
                unsafe { $max(self.ptr.as_ptr()) }
            }

            pub fn active_count(self) -> u32 {
                // SAFETY: this non-null pool is borrowed from a live scene.
                unsafe { $active(self.ptr.as_ptr()) }
            }

            pub fn capacity(self) -> u32 {
                // SAFETY: this non-null pool is borrowed from a live scene.
                unsafe { $capacity(self.ptr.as_ptr()) }
            }

            pub fn free_list_head(self) -> u32 {
                // SAFETY: this non-null pool is borrowed from a live scene.
                unsafe { $free(self.ptr.as_ptr()) }
            }

            pub fn next_id_key(self) -> u32 {
                // SAFETY: this non-null pool is borrowed from a live scene.
                unsafe { $next_key(self.ptr.as_ptr()) }
            }

            pub fn slot(self, index: u32) -> Result<PoolSlot> {
                let mut active = 0;
                let mut id = 0;
                let mut next_free = 0;
                // SAFETY: this pool is live and all output pointers refer to local storage.
                Error::from_status(unsafe { $slot(self.ptr.as_ptr(), index, &mut active, &mut id, &mut next_free) })?;
                Ok(PoolSlot {
                    active: active != 0,
                    id,
                    next_free,
                })
            }

            pub fn get(self, native_index: i32) -> Option<$object_ref<'a>> {
                // SAFETY: the pool is live; native get preserves bounds and occupancy checks.
                NonNull::new(unsafe { $get(self.ptr.as_ptr(), native_index) }).map(Borrowed::new)
            }

            /// Resolves an untrusted generation ID against the current pool.
            pub fn try_to_get(self, id: u32) -> Option<$object_ref<'a>> {
                // SAFETY: this pool is live; native lookup validates the supplied generation ID.
                NonNull::new(unsafe { $try_get(self.ptr.as_ptr(), id) }).map(Borrowed::new)
            }

            pub fn get_id(self, object: $object_ref<'a>) -> Option<u32> {
                // SAFETY: both borrows are live; native code validates pool membership before indexing.
                let id = unsafe { $get_id(self.ptr.as_ptr(), object.as_ptr()) };
                (id != 0).then_some(id)
            }
        }
    };
}

pool!(
    PlantPool,
    raw::pe_rs_plant_pool,
    raw::pe_rs_plant,
    PlantRef,
    raw::pe_rs_plant_pool_max_used_count,
    raw::pe_rs_plant_pool_active_count,
    raw::pe_rs_plant_pool_capacity,
    raw::pe_rs_plant_pool_free_list_head,
    raw::pe_rs_plant_pool_next_id_key,
    raw::pe_rs_plant_pool_slot,
    raw::pe_rs_plant_pool_get,
    raw::pe_rs_plant_pool_try_to_get,
    raw::pe_rs_plant_pool_get_id
);
pool!(
    ZombiePool,
    raw::pe_rs_zombie_pool,
    raw::pe_rs_zombie,
    ZombieRef,
    raw::pe_rs_zombie_pool_max_used_count,
    raw::pe_rs_zombie_pool_active_count,
    raw::pe_rs_zombie_pool_capacity,
    raw::pe_rs_zombie_pool_free_list_head,
    raw::pe_rs_zombie_pool_next_id_key,
    raw::pe_rs_zombie_pool_slot,
    raw::pe_rs_zombie_pool_get,
    raw::pe_rs_zombie_pool_try_to_get,
    raw::pe_rs_zombie_pool_get_id
);
pool!(
    GridItemPool,
    raw::pe_rs_griditem_pool,
    raw::pe_rs_griditem,
    GridItemRef,
    raw::pe_rs_griditem_pool_max_used_count,
    raw::pe_rs_griditem_pool_active_count,
    raw::pe_rs_griditem_pool_capacity,
    raw::pe_rs_griditem_pool_free_list_head,
    raw::pe_rs_griditem_pool_next_id_key,
    raw::pe_rs_griditem_pool_slot,
    raw::pe_rs_griditem_pool_get,
    raw::pe_rs_griditem_pool_try_to_get,
    raw::pe_rs_griditem_pool_get_id
);
pool!(
    ProjectilePool,
    raw::pe_rs_projectile_pool,
    raw::pe_rs_projectile,
    ProjectileRef,
    raw::pe_rs_projectile_pool_max_used_count,
    raw::pe_rs_projectile_pool_active_count,
    raw::pe_rs_projectile_pool_capacity,
    raw::pe_rs_projectile_pool_free_list_head,
    raw::pe_rs_projectile_pool_next_id_key,
    raw::pe_rs_projectile_pool_slot,
    raw::pe_rs_projectile_pool_get,
    raw::pe_rs_projectile_pool_try_to_get,
    raw::pe_rs_projectile_pool_get_id
);

/// The caller must ensure the call initializes the complete output before returning.
unsafe fn out_native_value<T>(call: impl FnOnce(*mut T)) -> T {
    let mut value = std::mem::MaybeUninit::uninit();
    call(value.as_mut_ptr());
    // SAFETY: required by the caller contract above.
    unsafe { value.assume_init() }
}

fn out_value<T>(f: impl FnOnce(*mut T) -> raw::pe_rs_status) -> Result<T> {
    let mut out = MaybeUninit::<T>::uninit();
    Error::from_status(f(out.as_mut_ptr()))?;
    // SAFETY: every successful bridge out-parameter call initializes its output.
    Ok(unsafe { out.assume_init() })
}

fn out_optional_ptr<T>(f: impl FnOnce(*mut *mut T) -> raw::pe_rs_status) -> Result<Option<NonNull<T>>> {
    let mut out = std::ptr::null_mut();
    Error::from_status(f(&mut out))?;
    Ok(NonNull::new(out))
}

fn out_required_ptr<T>(f: impl FnOnce(*mut *mut T) -> raw::pe_rs_status) -> Result<NonNull<T>> {
    out_optional_ptr(f)?.ok_or(Error::NullHandle)
}
