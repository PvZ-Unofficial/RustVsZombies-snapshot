//! Zombie entity capabilities.
//!
//! Entity reads and actions use handles valid within the current backend borrow.
//! Store IDs across frames and resolve them again before accessing an entity.

use crate::backend::{Backend, PlantReadBackend};
use rsvz_model::model::{
    ContactRect, DamageRangeFlags, Grid, I32RepresentableF32, NonNegativeI32, ObjectEditOutcome, PlantId, PositiveHp,
    ZombieId, ZombieKind, ZombiePhase,
};

/// Live zombie read capability.
pub trait ZombieReadBackend: Backend {
    /// Zombie handle valid for the current read lifetime.
    type ZombieHandle<'a>: Copy
    where
        Self: 'a;
    /// Live zombie handle iterator.
    type ZombieIter<'a>: Iterator<Item = Self::ZombieHandle<'a>>
    where
        Self: 'a;

    /// Current readable live zombie view in native object-pool update order.
    fn zombies(&self) -> Result<Self::ZombieIter<'_>, Self::Error>;
    /// Re-resolves a cross-frame zombie ID. Missing Board is an error; missing ID is `Ok(None)`.
    fn zombie(&self, id: ZombieId) -> Result<Option<Self::ZombieHandle<'_>>, Self::Error>;

    /// Reads zombie ID from a validated handle.
    fn zombie_id<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> ZombieId;
    /// Reads zombie kind from a validated handle.
    fn zombie_kind<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> Result<ZombieKind, Self::Error>;
    /// Reads zombie HP from a validated handle.
    fn zombie_hp<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> i32;
    /// Reads zombie row from a validated handle.
    fn zombie_row<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> i32;
    /// Reads frames elapsed since this zombie spawned.
    fn zombie_age<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> i32;
    /// Re-checks whether a validated handle is still live.
    fn zombie_is_alive<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> bool;
}

/// Raw vertical-position facts used by both row moves and zombie composition.
pub trait ZombieVerticalPositionBackend: ZombieReadBackend {
    fn zombie_pos_y<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> f32;
    fn zombie_pos_y_based_on_row<'a>(&'a self, zombie: Self::ZombieHandle<'a>, row: i32) -> Result<f32, Self::Error>;
}

/// Raw Z01-Z08 scalar facts used by backend-neutral state, contact and motion composition.
pub trait ZombieRawFactsBackend: ZombieVerticalPositionBackend {
    fn zombie_phase<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<ZombiePhase, Self::Error>;
    fn zombie_int_x<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32;
    fn zombie_int_y<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32;
    fn zombie_pos_x<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> f32;
    fn zombie_width<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32;
    fn zombie_height<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32;
    fn zombie_from_wave<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32;
    fn zombie_height_state<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32;
    fn zombie_phase_counter<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32;
    fn zombie_altitude<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> f32;
    fn zombie_speed_x<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> f32;
    fn zombie_scale<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> f32;
    fn zombie_speed_z<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> f32;
    fn zombie_is_eating<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool;
    fn zombie_is_disappeared<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool;
    fn zombie_is_mind_controlled<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool;
    fn zombie_is_blown_away<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool;
    fn zombie_has_flat_tires<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool;
    /// Whether a live zombie still has its head. False after logical death;
    /// decorative head state on retained remains is outside this capability.
    fn zombie_has_head<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool;
    fn zombie_has_object<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool;
    fn zombie_is_in_pool<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool;
    fn zombie_is_on_high_ground<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool;
    fn zombie_has_yucky_face<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool;
    fn zombie_yucky_face_counter<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32;
    fn zombie_frozen_countdown<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32;
    fn zombie_chilled_countdown<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32;
    fn zombie_buttered_countdown<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32;

    /// Each value is absent when the body reanimation is absent; it is resolved for this read.
    fn zombie_reanim_anim_time<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<Option<f32>, Self::Error>;
    fn zombie_reanim_last_time<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<Option<f32>, Self::Error>;
    fn zombie_reanim_rate<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<Option<f32>, Self::Error>;
    fn zombie_reanim_frame_start<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<Option<i32>, Self::Error>;
    fn zombie_reanim_frame_count<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<Option<i32>, Self::Error>;
    fn zombie_reanim_loop_type<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<Option<i32>, Self::Error>;
}

/// Native Z13-Z17 contact queries/actions over current borrowed objects.
pub trait ZombieContactBackend: ZombieRawFactsBackend + PlantReadBackend {
    fn zombie_effected_by_damage<'a>(
        &'a self, zombie: Self::ZombieHandle<'a>, flags: DamageRangeFlags,
    ) -> Result<bool, Self::Error>;
    fn zombie_can_attack_plant<'a>(
        &'a self, zombie: <Self as ZombieReadBackend>::ZombieHandle<'a>,
        plant: <Self as PlantReadBackend>::PlantHandle<'a>, attack_type: i32,
    ) -> Result<bool, Self::Error>;
    fn zombie_is_dead_or_dying<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool;
}

pub trait ZombieStateBackend: ZombieReadBackend {
    fn zombie_action<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32;
    fn zombie_collision<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<ContactRect, Self::Error>;
    fn zombie_max_hp<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32;
    fn zombie_accessory_1_hp<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32;
    fn zombie_accessory_1_max_hp<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32;
    fn zombie_accessory_2_hp<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32;
    fn zombie_accessory_2_max_hp<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32;
    /// Stored related ID with the native missing sentinel normalized to zero.
    fn zombie_related_id<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> u32;
    /// Reads one of four follower slots; an index outside 0..4 is invalid.
    fn zombie_follower_id<'a>(&'a self, zombie: Self::ZombieHandle<'a>, index: usize) -> Result<u32, Self::Error>;
    fn zombie_target_plant<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Option<PlantId>;
}

/// Native zombie creation atoms kept separate by their 1051 source semantics.
pub trait ZombieCreateBackend: ZombieReadBackend {
    fn add_zombie_in_row<'a>(
        &'a self, kind: ZombieKind, row: i32, from_wave: i32,
    ) -> Result<Option<Self::ZombieHandle<'a>>, Self::Error>;
    fn place_zombie<'a>(&'a self, kind: ZombieKind, grid: Grid) -> Result<Self::ZombieHandle<'a>, Self::Error>;
}

/// Native no-loot zombie removal action.
pub trait ZombieRemoveBackend: ZombieReadBackend {
    fn remove_zombie<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> Result<(), Self::Error>;
}

/// Native loot-producing zombie death action, unsupported on PE until loot exists.
pub trait ZombieKillBackend: ZombieReadBackend {
    fn kill_zombie<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> Result<(), Self::Error>;
}

/// Raw paired row/y write used by the backend-neutral row-move composition.
pub trait ZombiePositionWriteBackend: ZombieVerticalPositionBackend {
    fn set_zombie_row_and_y<'a>(
        &'a self, handle: Self::ZombieHandle<'a>, row: i32, y: I32RepresentableF32,
    ) -> Result<(), Self::Error>;
}

/// Direct zombie body health write capability.
///
/// `hp` must be positive. This edits main body health only and does not change shields,
/// accessories, heads, arms, or other layered health values.
pub trait ZombieBodyHealthWriteBackend: ZombieReadBackend {
    /// Sets main body health through a current-frame zombie handle.
    fn set_zombie_body_hp<'a>(
        &'a self, handle: Self::ZombieHandle<'a>, hp: PositiveHp,
    ) -> Result<ObjectEditOutcome, Self::Error>;
}

/// Direct zombie horizontal coordinate write capability.
///
/// `x` is the backend world horizontal coordinate used by live zombie state and contact/motion
/// logic. This must not imply row, y-coordinate, render-order, or lane migration semantics.
pub trait ZombieXWriteBackend: ZombieReadBackend {
    /// Sets the horizontal coordinate through a current-frame zombie handle.
    fn set_zombie_x<'a>(
        &'a self, handle: Self::ZombieHandle<'a>, x: I32RepresentableF32,
    ) -> Result<ObjectEditOutcome, Self::Error>;
}

/// Direct zombie native action/phase countdown write capability.
///
/// This is intended for controlled measurements that must hold an otherwise
/// independent state transition outside the sampled horizon. It does not
/// change the zombie's phase, animation, position, or movement speed.
pub trait ZombiePhaseCountdownWriteBackend: ZombieReadBackend {
    /// Sets the native action/phase countdown through a current-frame handle.
    fn set_zombie_phase_countdown<'a>(
        &'a self, handle: Self::ZombieHandle<'a>, countdown: NonNegativeI32,
    ) -> Result<ObjectEditOutcome, Self::Error>;
}
