//! Plant entity capabilities.
//!
//! Entity reads and actions use handles valid within the current backend borrow.
//! Store IDs across frames and resolve them again before accessing an entity.

use crate::backend::Backend;
use rsvz_model::model::{
    CheckedCardSelection, ContactRect, DamageRangeFlags, Grid, NonNegativeI32, ObjectEditOutcome, PixelPos, PlantId,
    PlantKind, PlantWeapon, Plantability, PositiveFiniteF32, PositiveHp, SunAmount,
};

/// Live plant read capability.
pub trait PlantReadBackend: Backend {
    /// Plant handle valid for the current read lifetime.
    type PlantHandle<'a>: Copy
    where
        Self: 'a;
    /// Live plant handle iterator.
    type PlantIter<'a>: Iterator<Item = Self::PlantHandle<'a>>
    where
        Self: 'a;

    /// Current readable live plant view.
    fn plants(&self) -> Result<Self::PlantIter<'_>, Self::Error>;
    /// Re-resolves a cross-frame plant ID. Missing Board is an error; missing ID is `Ok(None)`.
    fn plant(&self, id: PlantId) -> Result<Option<Self::PlantHandle<'_>>, Self::Error>;

    /// Reads plant ID from a validated handle.
    fn plant_id<'a>(&'a self, handle: Self::PlantHandle<'a>) -> PlantId;
    /// Reads plant kind from a validated handle.
    fn plant_kind<'a>(&'a self, handle: Self::PlantHandle<'a>) -> Result<PlantKind, Self::Error>;
    /// Reads the packet kind stored on the plant before imitater interpretation.
    fn plant_raw_kind<'a>(&'a self, handle: Self::PlantHandle<'a>) -> Result<PlantKind, Self::Error>;
    /// Reads the native integer plant x coordinate.
    fn plant_x<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32;
    /// Reads the native plant state scalar.
    fn plant_state<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32;
    /// Reads whether the plant is currently squished.
    fn plant_is_squished<'a>(&'a self, handle: Self::PlantHandle<'a>) -> bool;
    /// Reads the native bungee ownership state when the backend has one.
    fn plant_on_bungee_state<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32;
    /// Reads the current sleeping flag.
    fn plant_is_sleeping<'a>(&'a self, handle: Self::PlantHandle<'a>) -> bool;
    /// Reads the native wake-up countdown.
    fn plant_wake_up_counter<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32;
    /// Reads the native state countdown.
    fn plant_state_countdown<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32;
    /// Reads the native special-effect countdown.
    fn plant_effect_countdown<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32;
    /// Reads the native disappearance countdown used by blover.
    fn plant_disappear_countdown<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32;
    /// Reads the native eating-flash countdown.
    fn plant_eating_flash_counter<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32;
    /// Reads the body reanimation circulation/progress when present.
    fn plant_reanim_circulation<'a>(&'a self, handle: Self::PlantHandle<'a>) -> Result<Option<f32>, Self::Error>;
    /// Reads plant HP from a validated handle.
    fn plant_hp<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32;
    /// Reads the native plant row and column independently.
    fn plant_row<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32;
    fn plant_col<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32;
    /// Re-checks whether a validated handle is still live.
    fn plant_is_alive<'a>(&'a self, handle: Self::PlantHandle<'a>) -> bool;

    /// Visits borrowed live plants whose anchor equals `grid`.
    fn for_each_plant_at_anchor_grid<'a, F>(&'a self, grid: Grid, mut visit: F) -> Result<(), Self::Error>
    where
        F: FnMut(Self::PlantHandle<'a>) -> Result<(), Self::Error>,
    {
        let plants: Self::PlantIter<'a> = self.plants()?;
        for plant in plants {
            if self.plant_row(plant) == grid.row && self.plant_col(plant) == grid.col {
                visit(plant)?;
            }
        }
        Ok(())
    }

    /// Visits borrowed live plants occupying `grid`, once per object.
    fn for_each_plant_at_grid<'a, F>(&'a self, grid: Grid, mut visit: F) -> Result<(), Self::Error>
    where
        F: FnMut(Self::PlantHandle<'a>) -> Result<(), Self::Error>,
    {
        let plants: Self::PlantIter<'a> = self.plants()?;
        for plant in plants {
            let anchor = Grid {
                row: self.plant_row(plant),
                col: self.plant_col(plant),
            };
            if rsvz_model::plant_occupies_grid(anchor, self.plant_kind(plant)?, grid) {
                visit(plant)?;
            }
        }
        Ok(())
    }
}

/// Native P02/P08/P09/P14 plant facts over a current borrowed plant.
pub trait PlantContactBackend: PlantReadBackend {
    fn plant_hit_box<'a>(&'a self, plant: Self::PlantHandle<'a>) -> Result<ContactRect, Self::Error>;
    fn plant_attack_rect<'a>(
        &'a self, plant: Self::PlantHandle<'a>, weapon: PlantWeapon,
    ) -> Result<ContactRect, Self::Error>;
    fn plant_damage_range_flags<'a>(&'a self, plant: Self::PlantHandle<'a>, weapon: PlantWeapon) -> DamageRangeFlags;
}

pub trait PlantStateBackend: PlantReadBackend {
    fn plant_y<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32;
    fn plant_width<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32;
    fn plant_height<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32;
    fn plant_max_hp<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32;
    fn plant_target_x<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32;
    fn plant_target_y<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32;
    fn plant_target_object<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32;
    fn plant_direction<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32;
    fn plant_collision<'a>(&'a self, plant: Self::PlantHandle<'a>) -> Result<ContactRect, Self::Error>;
    fn plant_imitater_kind<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32;

    /// Gameplay recent-eating countdown, distinct from the cosmetic eating-flash counter.
    fn plant_recently_eaten_countdown<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32;
    fn plant_launch_counter<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32;
    fn plant_shooting_counter<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32;
}

/// Native current-cost query with the packet and imitater kinds kept distinct.
pub trait PlantCostBackend: Backend {
    fn current_plant_cost(&self, selection: CheckedCardSelection) -> Result<SunAmount, Self::Error>;
}

/// Native placement-only query. This must not include sun or cooldown checks.
pub trait PlantPlacementBackend: Backend {
    fn can_plant_at(&self, selection: CheckedCardSelection, grid: Grid) -> Result<Plantability, Self::Error>;
}

/// Native A02/A02a plant-pool allocation facts.
pub trait PlantPoolBackend: Backend {
    fn plant_pool_active_count(&self) -> Result<u32, Self::Error>;
    fn plant_pool_capacity(&self) -> Result<u32, Self::Error>;
}

/// Native plant creation actions. Selection interpretation and any conversion
/// to a cross-frame ID stay in `rsvz-game`.
///
/// The returned handle borrows the backend, so it cannot escape the current
/// world borrow:
///
/// ```compile_fail
/// use rsvz_backend_api::backend::PlantCreateBackend;
/// use rsvz_model::model::{Grid, PlantKind};
/// fn escape<B: PlantCreateBackend>(backend: &B) -> B::PlantHandle<'static> {
///     let selection = rsvz_model::CardSelection::Plant(PlantKind::Peashooter)
///         .checked()
///         .unwrap();
///     backend
///         .new_plant(selection, Grid { row: 0, col: 0 })
///         .unwrap()
/// }
/// ```
pub trait PlantCreateBackend: PlantReadBackend {
    /// Immediately performs native ImitaterMorph, consuming the live placeholder
    /// and returning its new, fully initialized target. No world frame is advanced.
    fn morph_imitator<'a>(&'a self, placeholder: Self::PlantHandle<'a>) -> Result<Self::PlantHandle<'a>, Self::Error>;
    fn new_plant<'a>(
        &'a self, selection: CheckedCardSelection, grid: Grid,
    ) -> Result<Self::PlantHandle<'a>, Self::Error>;
    fn add_plant<'a>(
        &'a self, selection: CheckedCardSelection, grid: Grid,
    ) -> Result<Self::PlantHandle<'a>, Self::Error>;
}

/// Exact native lineage produced when an imitator placeholder morphs.
///
/// The relation is an event fact, not a search by grid or plant kind. Backends
/// only need to retain events from the immediately preceding native update
/// batch because callers query at the native morph boundary.
pub trait ImitatorMorphBackend: PlantReadBackend {
    fn imitator_morph_successor(&self, placeholder: PlantId) -> Result<Option<PlantId>, Self::Error>;
}

/// Native `Plant::Die` action over a current borrowed handle.
pub trait PlantRemoveBackend: PlantReadBackend {
    fn remove_plant<'a>(&'a self, handle: Self::PlantHandle<'a>) -> Result<(), Self::Error>;
}

/// Native plant sleeping action over a current borrowed handle.
pub trait PlantSleepBackend: PlantReadBackend {
    fn set_plant_sleeping<'a>(&'a self, handle: Self::PlantHandle<'a>, asleep: bool) -> Result<(), Self::Error>;
    fn set_plant_wake_up_counter<'a>(&'a self, handle: Self::PlantHandle<'a>, counter: i32) -> Result<(), Self::Error>;
}

/// Native plant idle-animation action used by direct setup composition.
pub trait PlantIdleAnimationBackend: PlantReadBackend {
    fn play_plant_idle_animation<'a>(
        &'a self, handle: Self::PlantHandle<'a>, fps: PositiveFiniteF32,
    ) -> Result<(), Self::Error>;
}

/// Native P04 state-countdown write over a borrowed plant handle.
pub trait PlantStateCountdownWriteBackend: PlantReadBackend {
    fn set_plant_state_countdown<'a>(
        &'a self, handle: Self::PlantHandle<'a>, countdown: i32,
    ) -> Result<(), Self::Error>;
}

/// Native P01 state write over a borrowed plant handle.
pub trait PlantStateWriteBackend: PlantReadBackend {
    fn set_plant_state<'a>(&'a self, handle: Self::PlantHandle<'a>, state: i32) -> Result<(), Self::Error>;
}

/// Native cob fire action over a borrowed plant handle.
pub trait CobFireBackend: PlantReadBackend {
    fn fire_cob<'a>(&'a self, cob: Self::PlantHandle<'a>, target: PixelPos) -> Result<(), Self::Error>;
}

/// Direct plant health write capability.
///
/// `hp` must be positive. This edits current plant health only and does not change max HP or
/// kind-specific durability rules.
pub trait PlantHealthWriteBackend: PlantReadBackend {
    /// Sets current plant health through a current-frame plant handle.
    fn set_plant_hp<'a>(
        &'a self, handle: Self::PlantHandle<'a>, hp: PositiveHp,
    ) -> Result<ObjectEditOutcome, Self::Error>;
}

/// Direct plant special/effect countdown write capability.
pub trait PlantEffectCountdownWriteBackend: PlantReadBackend {
    /// Sets the native special/effect countdown for a current-frame plant handle.
    fn set_plant_effect_countdown<'a>(
        &'a self, handle: Self::PlantHandle<'a>, target_countdown: NonNegativeI32,
    ) -> Result<ObjectEditOutcome, Self::Error>;
}

/// Backend capability for cosmetic plant visual writes.
/// Headless backends may accept these writes as no-ops.
pub trait PlantVisualStateBackend: PlantReadBackend {
    /// Writes the native eating-flash countdown field.
    fn set_plant_eating_flash_counter<'a>(
        &'a self, plant: Self::PlantHandle<'a>, value: i32,
    ) -> Result<(), Self::Error>;

    /// Re-run the native plant reanimation color update for the target plant.
    fn update_plant_reanim_color<'a>(&'a self, plant: Self::PlantHandle<'a>) -> Result<(), Self::Error>;
}
