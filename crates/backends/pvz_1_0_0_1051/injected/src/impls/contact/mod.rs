use std::ptr::{self, NonNull};

use rsvz_backend_api::backend::{
    PlantContactBackend, PlantReadBackend, ZombieContactBackend, ZombieRawFactsBackend, ZombieReadBackend,
    ZombieVerticalPositionBackend,
};
use rsvz_model::model::{ContactRect, DamageRangeFlags, PlantWeapon, ZombiePhase};

use crate::error::{Pvz1051Error, Result};
use crate::raw::layout as ptrs;
use crate::raw::types::RawRect;
use crate::runtime::Pvz1051Backend;

macro_rules! zombie_scalar {
    ($handle:expr, $field:ident) => {{
        // SAFETY: the handle was generation-validated and remains bound to this backend borrow;
        // only one scalar is copied from the current slot.
        unsafe { ptrs::Zombie::$field($handle.ptr.as_ptr()) }
    }};
}

pub(super) fn native_rect(
    operation: &'static str, query: impl FnOnce(*mut RawRect) -> *mut RawRect,
) -> Result<ContactRect> {
    let mut raw = RawRect::new(0, 0, 0, 0);
    if !ptr::eq(query(&raw mut raw).cast_const(), &raw const raw) {
        return Err(Pvz1051Error::InvariantViolated(operation));
    }
    ContactRect::checked_from_pos_size(raw.x, raw.y, raw.width, raw.height)
        .ok_or(Pvz1051Error::InvariantViolated(operation))
}

impl PlantContactBackend for Pvz1051Backend {
    fn plant_hit_box<'a>(&'a self, plant: Self::PlantHandle<'a>) -> Result<ContactRect> {
        native_rect("Plant::GetPlantRect", |out| {
            // SAFETY: objdump verifies EAX=out/ECX=Plant; the handle and out buffer are current.
            unsafe { crate::raw::abi::plant_get_plant_rect(plant.ptr.as_ptr(), out) }
        })
    }

    fn plant_attack_rect<'a>(&'a self, plant: Self::PlantHandle<'a>, weapon: PlantWeapon) -> Result<ContactRect> {
        native_rect("Plant::GetPlantAttackRect", |out| {
            // SAFETY: objdump verifies EAX=out/ECX=Plant/stack weapon.
            unsafe { crate::raw::abi::plant_get_attack_rect(plant.ptr.as_ptr(), weapon as i32, out) }
        })
    }

    fn plant_damage_range_flags<'a>(&'a self, plant: Self::PlantHandle<'a>, weapon: PlantWeapon) -> DamageRangeFlags {
        // SAFETY: objdump verifies EAX=Plant/stack weapon and an integer EAX result.
        let flags = unsafe { crate::raw::abi::plant_get_damage_range_flags(plant.ptr.as_ptr(), weapon as i32) };
        DamageRangeFlags::from_bits_retain(flags as u32)
    }
}

impl ZombieVerticalPositionBackend for Pvz1051Backend {
    fn zombie_pos_y<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> f32 {
        zombie_scalar!(zombie, pos_y)
    }

    fn zombie_pos_y_based_on_row<'a>(&'a self, zombie: Self::ZombieHandle<'a>, row: i32) -> Result<f32> {
        self.validate_zombie_row(row)?;
        // SAFETY: objdump verifies EAX=Zombie, stack row and x87 ST0 result for Z18.
        Ok(unsafe { crate::raw::abi::zombie_get_pos_y_based_on_row(zombie.ptr.as_ptr(), row) })
    }
}

impl ZombieRawFactsBackend for Pvz1051Backend {
    fn zombie_phase<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<ZombiePhase> {
        crate::raw::kind::zombie_phase_from_raw(zombie_scalar!(zombie, phase))
    }
    fn zombie_int_x<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        zombie_scalar!(zombie, x)
    }
    fn zombie_int_y<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        zombie_scalar!(zombie, y)
    }
    fn zombie_pos_x<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> f32 {
        zombie_scalar!(zombie, pos_x)
    }
    fn zombie_width<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        zombie_scalar!(zombie, width)
    }
    fn zombie_height<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        zombie_scalar!(zombie, height)
    }

    fn zombie_from_wave<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        zombie_scalar!(zombie, from_wave)
    }
    fn zombie_height_state<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        zombie_scalar!(zombie, zombie_height)
    }
    fn zombie_phase_counter<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        zombie_scalar!(zombie, phase_counter)
    }
    fn zombie_altitude<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> f32 {
        zombie_scalar!(zombie, altitude)
    }
    fn zombie_speed_x<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> f32 {
        zombie_scalar!(zombie, speed_x)
    }
    fn zombie_scale<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> f32 {
        zombie_scalar!(zombie, scale_zombie)
    }
    fn zombie_speed_z<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> f32 {
        zombie_scalar!(zombie, vel_z)
    }
    fn zombie_is_eating<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        zombie_scalar!(zombie, is_eating)
    }
    fn zombie_is_disappeared<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        zombie_scalar!(zombie, is_disappeared)
    }
    fn zombie_is_mind_controlled<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        zombie_scalar!(zombie, mind_controlled)
    }
    fn zombie_is_blown_away<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        zombie_scalar!(zombie, blowing_away)
    }
    fn zombie_has_flat_tires<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        zombie_scalar!(zombie, flat_tires)
    }
    fn zombie_has_head<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        self.zombie_is_alive(zombie) && zombie_scalar!(zombie, has_head)
    }
    fn zombie_has_object<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        zombie_scalar!(zombie, has_object)
    }
    fn zombie_is_in_pool<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        zombie_scalar!(zombie, in_pool)
    }
    fn zombie_is_on_high_ground<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        zombie_scalar!(zombie, on_high_ground)
    }
    fn zombie_has_yucky_face<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        zombie_scalar!(zombie, yucky_face)
    }
    fn zombie_yucky_face_counter<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        zombie_scalar!(zombie, yucky_face_counter)
    }
    fn zombie_frozen_countdown<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        zombie_scalar!(zombie, frozen_countdown)
    }
    fn zombie_chilled_countdown<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        zombie_scalar!(zombie, chilled_countdown)
    }
    fn zombie_buttered_countdown<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        zombie_scalar!(zombie, buttered_countdown)
    }

    fn zombie_reanim_anim_time<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<Option<f32>> {
        reanimation_value(self.app(), zombie.ptr, ptrs::Reanimation::anim_time)
    }
    fn zombie_reanim_last_time<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<Option<f32>> {
        reanimation_value(self.app(), zombie.ptr, ptrs::Reanimation::last_frame_time)
    }
    fn zombie_reanim_rate<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<Option<f32>> {
        reanimation_value(self.app(), zombie.ptr, ptrs::Reanimation::anim_rate)
    }
    fn zombie_reanim_frame_start<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<Option<i32>> {
        reanimation_value(self.app(), zombie.ptr, ptrs::Reanimation::frame_start)
    }
    fn zombie_reanim_frame_count<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<Option<i32>> {
        reanimation_value(self.app(), zombie.ptr, ptrs::Reanimation::frame_count)
    }
    fn zombie_reanim_loop_type<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<Option<i32>> {
        reanimation_value(self.app(), zombie.ptr, ptrs::Reanimation::loop_type)
    }
}

impl ZombieContactBackend for Pvz1051Backend {
    fn zombie_effected_by_damage<'a>(
        &'a self, zombie: Self::ZombieHandle<'a>, flags: DamageRangeFlags,
    ) -> Result<bool> {
        // SAFETY: objdump-verified Z15 ABI; handle is current and flags are scalar.
        Ok(unsafe { crate::raw::abi::zombie_effected_by_damage(zombie.ptr.as_ptr(), flags.bits()) } != 0)
    }

    fn zombie_can_attack_plant<'a>(
        &'a self, zombie: <Self as ZombieReadBackend>::ZombieHandle<'a>,
        plant: <Self as PlantReadBackend>::PlantHandle<'a>, attack_type: i32,
    ) -> Result<bool> {
        // SAFETY: objdump verifies ECX=Zombie and the two stack arguments; both handles are current.
        Ok(
            unsafe { crate::raw::abi::zombie_can_target_plant(zombie.ptr.as_ptr(), plant.ptr.as_ptr(), attack_type) }
                != 0,
        )
    }

    fn zombie_is_dead_or_dying<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        // SAFETY: objdump verifies EAX=Zombie and AL result for Z17.
        unsafe { crate::raw::abi::zombie_is_dead_or_dying(zombie.ptr.as_ptr()) != 0 }
    }
}

fn resolve_reanimation(
    app: NonNull<ptrs::LawnApp>, zombie: NonNull<ptrs::Zombie>,
) -> Result<Option<NonNull<ptrs::Reanimation>>> {
    // SAFETY: app is current; every nested pointer is checked before use.
    let Some(effect_system) = NonNull::new(unsafe { ptrs::LawnApp::effect_system(app.as_ptr()) }) else {
        return Err(Pvz1051Error::InvariantViolated("effect system"));
    };
    // SAFETY: effect_system was checked non-null.
    let Some(holder) = NonNull::new(unsafe { ptrs::EffectSystem::reanimation_holder(effect_system.as_ptr()) }) else {
        return Err(Pvz1051Error::InvariantViolated("reanimation holder"));
    };
    // SAFETY: both objects are current; DataArray::try_to_get validates the full generation ID.
    Ok(unsafe {
        let array = ptrs::ReanimationHolder::reanimations(holder.as_ptr());
        let id = ptrs::Zombie::body_reanim_id(zombie.as_ptr());
        ptrs::DataArray::try_to_get(array, id)
    })
}

fn reanimation_value<T>(
    app: NonNull<ptrs::LawnApp>, zombie: NonNull<ptrs::Zombie>, read: unsafe fn(*const ptrs::Reanimation) -> T,
) -> Result<Option<T>> {
    // Decorative remains are outside gameplay animation queries.
    if !unsafe { ptrs::Zombie::is_alive(zombie.as_ptr()) } {
        return Ok(None);
    }
    // Child existence and generation are checked for this immediate read only.
    // No reanimation pointer survives an intervening action.
    Ok(resolve_reanimation(app, zombie)?.map(|reanimation| unsafe { read(reanimation.as_ptr()) }))
}
