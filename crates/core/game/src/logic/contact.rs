//! Reusable contact geometry logic.

mod profile;

use rsvz_backend_api::{PlantReadBackend, ZombieReadBackend};

use rsvz_backend_api::ZombieRawFactsBackend;
use rsvz_backend_api::backend::{GridGeometryBackend, PlantContactBackend, ZombieContactBackend};
use rsvz_model::ZombieKind;
use rsvz_model::{
    AbsoluteContactRange, CHERRY_BOMB_RADIUS, ContactCircle, ContactRect, DOOM_SHROOM_RADIUS, DamageRangeFlags, Grid,
    GridExplosionKind, PixelPos, PlantContactRect, PlantDefenseBounds, PlantThreatKind, PlantThreatShape,
    ZombieAttackBounds, ZombieDefenseBounds, ZombieGeometryError, ZombiePlantThreatShape, ZombieThreatCandidateFacts,
    ZombieThreatCandidateShape,
};
use rsvz_model::{
    CobTarget, PlantDefenseKind, ZombieDefensePhase, ZombieDefenseState, ZombieGeometryInput, ZombiePhase, ZombieState,
};
use rsvz_model::{PlantId, ZombieId, ZombiePlantThreatKind};

use self::profile::{
    RawZombieContactFacts, active_contact_profile_from_facts, zombie_geometry_input_from_facts, zombie_height_from_raw,
};
use crate::logic::zombie_geometry::predicted_zombie_attack_bounds;
use crate::logic::zombie_geometry::{zombie_defense_geometry_from_state, zombie_defense_state_from_profile};

/// Minimum native horizontal overlap treated as contact by plant-threat geometry.
pub const PLANT_THREAT_HORIZONTAL_OVERLAP: i64 = 20;

#[must_use]
pub const fn grid_explosion_shape_from_pixel(kind: GridExplosionKind, grid: Grid, pixel: PixelPos) -> PlantThreatShape {
    let (threat_kind, row_range, radius) = match kind {
        GridExplosionKind::CherryBomb => (PlantThreatKind::CherryBomb, 1, CHERRY_BOMB_RADIUS),
        GridExplosionKind::DoomShroom => (PlantThreatKind::DoomShroom, 3, DOOM_SHROOM_RADIUS),
    };
    PlantThreatShape::new(
        threat_kind,
        grid.row,
        row_range,
        DamageRangeFlags::ALL_NON_MIND_CONTROLLED,
        ContactCircle::new(pixel.x.saturating_add(40), pixel.y.saturating_add(40), radius),
    )
}

#[must_use]
pub fn native_horizontal_rect_overlap(a: AbsoluteContactRange, b: AbsoluteContactRange) -> i64 {
    if !a.is_valid() || !b.is_valid() {
        return 0;
    }
    let left = i64::from(a.left.max(b.left));
    let right = i64::from(a.right.min(b.right));
    right - left
}

#[must_use]
pub fn attack_reaches_defense(attack: AbsoluteContactRange, defense: AbsoluteContactRange) -> bool {
    attack.is_valid()
        && defense.is_valid()
        && i64::from(attack.left) <= i64::from(defense.right)
        && i64::from(attack.right) >= i64::from(defense.left)
}

/// Test circle/rectangle overlap using PvZ `GetCircleRectOverlap` semantics.
///
/// PvZ stores rectangle endpoints as `x + width` / `y + height`, but this native helper treats
/// those right/bottom endpoints as inside the rectangle when the circle center is exactly on them.
#[must_use]
pub fn circle_hits_rect(circle: ContactCircle, rect: ContactRect) -> bool {
    if !circle.is_valid() || !rect.is_valid() {
        return false;
    }

    let closest_x = circle.x.clamp(rect.left, rect.right);
    let closest_y = circle.y.clamp(rect.top, rect.bottom);
    let dx = i128::from(circle.x) - i128::from(closest_x);
    let dy = i128::from(circle.y) - i128::from(closest_y);
    let radius = i128::from(circle.radius);
    dx * dx + dy * dy <= radius * radius
}

#[must_use]
pub fn zombie_threat_hits_plant_geometry(threat: ZombiePlantThreatShape, plant: PlantContactRect) -> bool {
    if !plant.rect.is_valid() {
        return false;
    }
    match threat {
        ZombiePlantThreatShape::SameRowRange { row, range } => {
            row == plant.grid.row
                && native_horizontal_rect_overlap(range, plant.rect.horizontal_range())
                    >= PLANT_THREAT_HORIZONTAL_OVERLAP
        }
        ZombiePlantThreatShape::Circle(circle) => circle_hits_rect(circle, plant.rect),
        _ => false,
    }
}

#[must_use]
pub fn plant_threat_hits_zombie_geometry(threat: PlantThreatShape, zombie: ZombieDefenseBounds) -> bool {
    if threat.row_range < 0 || !threat.circle.is_valid() || !zombie.rect.is_valid() {
        return false;
    }
    let row_delta = i64::from(zombie.row) - i64::from(threat.center_row);
    row_delta.abs() <= i64::from(threat.row_range) && circle_hits_rect(threat.circle, zombie.rect)
}

#[must_use]
pub fn same_row_and_attack_reaches_plant_geometry(attack: ZombieAttackBounds, defense: PlantDefenseBounds) -> bool {
    attack.row == defense.grid.row && attack_reaches_defense(attack.range, defense.range)
}

#[derive(Debug, thiserror::Error)]
pub enum ContactGeometryQueryError {
    #[error("contact geometry interpretation failed: {0:?}")]
    Geometry(ZombieGeometryError),
}

impl From<ZombieGeometryError> for ContactGeometryQueryError {
    fn from(error: ZombieGeometryError) -> Self {
        Self::Geometry(error)
    }
}

pub fn zombie_threat_candidate_shape(
    facts: ZombieThreatCandidateFacts,
) -> Result<Option<ZombieThreatCandidateShape>, ZombieGeometryError> {
    match facts {
        ZombieThreatCandidateFacts::SameRowAttack {
            id,
            kind,
            threat_kind,
            geometry,
        } => Ok(Some(ZombieThreatCandidateShape {
            id,
            kind,
            threat_kind,
            shape: ZombiePlantThreatShape::SameRowRange {
                row: geometry.row,
                range: predicted_zombie_attack_bounds(geometry)?.range,
            },
        })),
        ZombieThreatCandidateFacts::Circle {
            id,
            kind,
            threat_kind,
            circle,
        } => Ok(Some(ZombieThreatCandidateShape {
            id,
            kind,
            threat_kind,
            shape: ZombiePlantThreatShape::Circle(circle),
        })),
        _ => Ok(None),
    }
}

/// Core-only capability bundle. Backends implement only the raw P/Z/X atoms.
pub trait ContactGeometryBackend: PlantContactBackend + ZombieContactBackend + GridGeometryBackend {}

impl<T> ContactGeometryBackend for T where T: PlantContactBackend + ZombieContactBackend + GridGeometryBackend {}

/// Composes the public zombie state value from borrowed raw fields.
pub fn zombie_state(zombie: ZombieId) -> Result<Option<ZombieState>, rsvz_current::CurrentBackendError>
where
    rsvz_current::CurrentBackend: ZombieRawFactsBackend,
{
    crate::access::with_backend(|backend| {
        let Some(handle) = crate::live_value::read_or_abort(backend.zombie(zombie), "zombie") else {
            return Ok(None);
        };
        Ok(Some(crate::zombie::state_from_handle(backend, handle)))
    })
}

pub fn plant_contact_rect(plant: PlantId) -> Result<Option<PlantContactRect>, ContactGeometryQueryError>
where
    rsvz_current::CurrentBackend: PlantContactBackend,
{
    crate::access::with_backend(|backend| {
        let Some(handle) = crate::live_value::read_or_abort(backend.plant(plant), "plant") else {
            return Ok(None);
        };
        let rect = backend
            .plant_hit_box(handle)
            .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
        if !rect.is_valid() {
            return Ok(None);
        }
        Ok(Some(PlantContactRect {
            id: plant,
            kind: backend
                .plant_kind(handle)
                .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into())),
            grid: crate::plant::grid_from_handle(backend, handle),
            rect,
        }))
    })
}

pub fn plant_defense_bounds(
    plant: PlantId, kind: PlantDefenseKind,
) -> Result<Option<PlantDefenseBounds>, ContactGeometryQueryError>
where
    rsvz_current::CurrentBackend: PlantContactBackend,
{
    if kind != PlantDefenseKind::ChewCrushSmash {
        return Ok(None);
    }
    crate::access::with_backend(|backend| {
        let Some(handle) = crate::live_value::read_or_abort(backend.plant(plant), "plant") else {
            return Ok(None);
        };
        plant_defense_bounds_from_handle(backend, handle, kind)
    })
}

pub(crate) fn plant_defense_bounds_from_handle<'a>(
    backend: &'a rsvz_current::CurrentBackend,
    handle: <rsvz_current::CurrentBackend as PlantReadBackend>::PlantHandle<'a>, kind: PlantDefenseKind,
) -> Result<Option<PlantDefenseBounds>, ContactGeometryQueryError>
where
    rsvz_current::CurrentBackend: PlantContactBackend,
{
    if kind != PlantDefenseKind::ChewCrushSmash {
        return Ok(None);
    }
    let rect = backend
        .plant_hit_box(handle)
        .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
    if !rect.is_valid() {
        return Ok(None);
    }
    let range = rect.horizontal_range();
    let Some(range) = range
        .left
        .checked_add(20)
        .zip(range.right.checked_sub(20))
        .map(|(left, right)| AbsoluteContactRange::new(left, right))
    else {
        return Ok(None);
    };
    if !range.is_valid() {
        return Ok(None);
    }
    Ok(Some(PlantDefenseBounds {
        id: backend.plant_id(handle),
        kind: backend
            .plant_kind(handle)
            .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into())),
        defense_kind: kind,
        grid: crate::plant::grid_from_handle(backend, handle),
        x: backend.plant_x(handle),
        range,
    }))
}

pub fn zombie_geometry_input(zombie: ZombieId) -> Result<Option<ZombieGeometryInput>, ContactGeometryQueryError>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::ZombieRawFactsBackend,
{
    crate::access::with_backend(|backend| {
        let Some(handle) = crate::live_value::read_or_abort(backend.zombie(zombie), "zombie") else {
            return Ok(None);
        };
        let facts =
            zombie_facts(backend, handle).unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
        zombie_geometry_input_from_facts(facts).map_err(ContactGeometryQueryError::Geometry)
    })
}

pub fn zombie_defense_state(zombie: ZombieId) -> Result<Option<ZombieDefenseState>, ContactGeometryQueryError>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::ZombieRawFactsBackend,
{
    crate::access::with_backend(|backend| {
        let Some(handle) = crate::live_value::read_or_abort(backend.zombie(zombie), "zombie") else {
            return Ok(None);
        };
        zombie_defense_state_from_handle(backend, handle)
    })
}

pub fn grid_explosion_shape(kind: GridExplosionKind, grid: Grid) -> Result<PlantThreatShape, ContactGeometryQueryError>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::GridGeometryBackend,
{
    crate::access::with_backend(|backend| {
        let pixel = PixelPos {
            x: backend
                .grid_to_pixel_x(grid.col, grid.row)
                .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into())),
            y: backend
                .grid_to_pixel_y(grid.col, grid.row)
                .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into())),
        };
        Ok(grid_explosion_shape_from_pixel(kind, grid, pixel))
    })
}

pub fn static_cob_impact_shape(target: CobTarget) -> Result<PlantThreatShape, ContactGeometryQueryError>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::GridGeometryBackend,
{
    let fire = crate::logic::cob::cob_target_to_pixel(target)
        .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
    Ok(PlantThreatShape::new(
        PlantThreatKind::StaticCobCannonImpact,
        target.row,
        1,
        DamageRangeFlags::ALL_NON_MIND_CONTROLLED,
        ContactCircle::new(fire.x.saturating_sub(7), fire.y, 115),
    ))
}

pub fn zombie_threat_candidate(
    zombie: ZombieId, threat_kind: ZombiePlantThreatKind,
) -> Result<Option<ZombieThreatCandidateShape>, ContactGeometryQueryError>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::ZombieRawFactsBackend,
{
    crate::access::with_backend(|backend| {
        let Some(handle) = crate::live_value::read_or_abort(backend.zombie(zombie), "zombie") else {
            return Ok(None);
        };
        let facts =
            zombie_facts(backend, handle).unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
        let Some(facts) = zombie_threat_candidate_from_facts(backend, zombie, handle, facts, threat_kind)? else {
            return Ok(None);
        };
        zombie_threat_candidate_shape(facts).map_err(ContactGeometryQueryError::Geometry)
    })
}

pub fn zombie_threat_hits_plant(
    zombie: ZombieId, plant: PlantId, threat_kind: ZombiePlantThreatKind,
) -> Result<bool, ContactGeometryQueryError>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::ZombieRawFactsBackend + PlantContactBackend,
{
    let Some(threat) = zombie_threat_candidate(zombie, threat_kind)? else {
        return Ok(false);
    };
    let Some(plant) = plant_contact_rect(plant)? else {
        return Ok(false);
    };
    Ok(zombie_threat_hits_plant_geometry(threat.shape, plant))
}

pub fn zombie_attack_bounds(zombie: ZombieId) -> Result<Option<ZombieAttackBounds>, ContactGeometryQueryError>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::ZombieRawFactsBackend,
{
    crate::access::with_backend(|backend| {
        let Some(handle) = crate::live_value::read_or_abort(backend.zombie(zombie), "zombie") else {
            return Ok(None);
        };
        zombie_attack_bounds_from_handle(backend, handle)
    })
}

pub(crate) fn zombie_attack_bounds_from_handle<'a>(
    backend: &'a rsvz_current::CurrentBackend,
    handle: <rsvz_current::CurrentBackend as ZombieReadBackend>::ZombieHandle<'a>,
) -> Result<Option<ZombieAttackBounds>, ContactGeometryQueryError>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::ZombieRawFactsBackend,
{
    let facts = zombie_facts(backend, handle).unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
    let Some(input) = zombie_geometry_input_from_facts(facts).map_err(ContactGeometryQueryError::Geometry)? else {
        return Ok(None);
    };
    Ok(Some(
        predicted_zombie_attack_bounds(input)?.with_id(backend.zombie_id(handle)),
    ))
}

pub fn zombie_defense_bounds(zombie: ZombieId) -> Result<Option<ZombieDefenseBounds>, ContactGeometryQueryError>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::ZombieRawFactsBackend,
{
    let Some(state) = zombie_defense_state(zombie)? else {
        return Ok(None);
    };
    Ok(Some(zombie_defense_geometry_from_state(state)?.with_id(zombie)))
}

pub fn zombie_attack_reaches_plant_geometry(zombie: ZombieId, plant: PlantId) -> Result<bool, ContactGeometryQueryError>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::ZombieRawFactsBackend + PlantContactBackend,
{
    let Some(attack) = zombie_attack_bounds(zombie)? else {
        return Ok(false);
    };
    let Some(defense) = plant_defense_bounds(plant, PlantDefenseKind::ChewCrushSmash)? else {
        return Ok(false);
    };
    Ok(same_row_and_attack_reaches_plant_geometry(attack, defense))
}

pub fn plant_threat_hits_zombie(threat: PlantThreatShape, zombie: ZombieId) -> Result<bool, ContactGeometryQueryError>
where
    rsvz_current::CurrentBackend: ZombieContactBackend,
{
    crate::access::with_backend(|backend| {
        let Some(handle) = crate::live_value::read_or_abort(backend.zombie(zombie), "zombie") else {
            return Ok(false);
        };
        if !backend
            .zombie_effected_by_damage(handle, threat.damage_flags)
            .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()))
        {
            return Ok(false);
        }
        let Some(state) = zombie_defense_state_from_handle(backend, handle)? else {
            return Ok(false);
        };
        let defense = zombie_defense_geometry_from_state(state)?.with_id(zombie);
        Ok(plant_threat_hits_zombie_geometry(threat, defense))
    })
}

fn zombie_facts<'a>(
    backend: &'a rsvz_current::CurrentBackend,
    zombie: <rsvz_current::CurrentBackend as rsvz_backend_api::ZombieReadBackend>::ZombieHandle<'a>,
) -> Result<RawZombieContactFacts, rsvz_current::CurrentBackendError>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::ZombieRawFactsBackend,
{
    Ok(RawZombieContactFacts {
        kind: crate::live_value::read_or_abort(backend.zombie_kind(zombie), "zombie_kind"),
        row: backend.zombie_row(zombie),
        x: backend.zombie_int_x(zombie),
        phase: crate::live_value::read_or_abort(backend.zombie_phase(zombie), "zombie_phase"),
        mind_controlled: backend.zombie_is_mind_controlled(zombie),
        has_object: backend.zombie_has_object(zombie),
        zombie_height: zombie_height_from_raw(backend.zombie_height_state(zombie)),
        vel_z: backend.zombie_speed_z(zombie),
        is_disappeared: backend.zombie_is_disappeared(zombie),
    })
}

fn zombie_defense_state_from_handle<'a>(
    backend: &'a rsvz_current::CurrentBackend,
    zombie: <rsvz_current::CurrentBackend as rsvz_backend_api::ZombieReadBackend>::ZombieHandle<'a>,
) -> Result<Option<ZombieDefenseState>, ContactGeometryQueryError>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::ZombieRawFactsBackend,
{
    let facts = zombie_facts(backend, zombie).unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
    let Some(profile) = active_contact_profile_from_facts(facts) else {
        return Ok(None);
    };
    let mut state =
        zombie_defense_state_from_profile(facts.kind, facts.row, facts.x, backend.zombie_int_y(zombie), profile)?;
    state.phase = ZombieDefensePhase::from_zombie_phase(facts.phase);
    state.mind_controlled = facts.mind_controlled;
    state.has_object = facts.has_object;
    state.in_pool = backend.zombie_is_in_pool(zombie);
    state.on_high_ground = backend.zombie_is_on_high_ground(zombie);
    state.is_eating = backend.zombie_is_eating(zombie);
    state.zombie_height = facts.zombie_height;
    state.altitude = backend.zombie_altitude(zombie);
    state.phase_counter = backend.zombie_phase_counter(zombie);
    state.scale_zombie = backend.zombie_scale(zombie);
    state.vel_z = facts.vel_z;
    if defense_phase_needs_body_reanim_time(facts.kind, state.phase) {
        state.body_reanim_anim_time = backend
            .zombie_reanim_anim_time(zombie)
            .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
    }
    Ok(Some(state))
}

fn zombie_threat_candidate_from_facts<'a>(
    backend: &'a rsvz_current::CurrentBackend, id: ZombieId,
    zombie: <rsvz_current::CurrentBackend as rsvz_backend_api::ZombieReadBackend>::ZombieHandle<'a>,
    facts: RawZombieContactFacts, threat_kind: ZombiePlantThreatKind,
) -> Result<Option<ZombieThreatCandidateFacts>, ContactGeometryQueryError>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::ZombieRawFactsBackend,
{
    match threat_kind {
        ZombiePlantThreatKind::DriveOver
            if matches!(facts.kind, ZombieKind::Zomboni | ZombieKind::Catapult)
                && !backend.zombie_has_flat_tires(zombie) =>
        {
            let Some(geometry) = zombie_geometry_input_from_facts(facts)? else {
                return Ok(None);
            };
            Ok(Some(ZombieThreatCandidateFacts::SameRowAttack {
                id,
                kind: facts.kind,
                threat_kind,
                geometry,
            }))
        }
        ZombiePlantThreatKind::GargantuarSmash
            if matches!(facts.kind, ZombieKind::Gargantuar | ZombieKind::GigaGargantuar)
                && facts.phase == ZombiePhase::GargantuarSmashing =>
        {
            let Some(geometry) = zombie_geometry_input_from_facts(facts)? else {
                return Ok(None);
            };
            Ok(Some(ZombieThreatCandidateFacts::SameRowAttack {
                id,
                kind: facts.kind,
                threat_kind,
                geometry,
            }))
        }
        ZombiePlantThreatKind::JackExplosion
            if facts.kind == ZombieKind::JackInTheBox
                && facts.phase == ZombiePhase::JackInTheBoxPopping
                && !facts.mind_controlled
                && active_contact_profile_from_facts(facts).is_some()
                && backend.zombie_has_head(zombie) =>
        {
            Ok(Some(ZombieThreatCandidateFacts::Circle {
                id,
                kind: facts.kind,
                threat_kind,
                circle: ContactCircle::new(
                    facts.x.saturating_add(backend.zombie_width(zombie) / 2),
                    backend
                        .zombie_int_y(zombie)
                        .saturating_add(backend.zombie_height(zombie) / 2),
                    90,
                ),
            }))
        }
        _ => Ok(None),
    }
}

const fn defense_phase_needs_body_reanim_time(kind: ZombieKind, phase: rsvz_model::ZombieDefensePhase) -> bool {
    matches!(
        (kind, phase),
        (
            ZombieKind::DolphinRider,
            rsvz_model::ZombieDefensePhase::DolphinIntoPool
        ) | (ZombieKind::DolphinRider, rsvz_model::ZombieDefensePhase::DolphinInJump)
            | (ZombieKind::Snorkel, rsvz_model::ZombieDefensePhase::SnorkelIntoPool)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsvz_model::{
        DamageRangeFlags, Grid, PlantDefenseKind, PlantKind, PlantThreatKind, ZombieContactProfile,
        ZombieGeometryInput, ZombieKind,
    };

    fn plant(rect: ContactRect) -> PlantContactRect {
        PlantContactRect {
            id: PlantId::from_raw(1),
            kind: PlantKind::Peashooter,
            grid: Grid { row: 2, col: 3 },
            rect,
        }
    }

    fn zombie_defense(row: i32, rect: ContactRect) -> ZombieDefenseBounds {
        ZombieDefenseBounds {
            id: ZombieId::from_raw(1),
            kind: ZombieKind::Normal,
            row,
            x: rect.left,
            rect,
        }
    }

    #[test]
    fn native_endpoint_overlap_uses_right_endpoint_directly() {
        assert_eq!(
            native_horizontal_rect_overlap(AbsoluteContactRange::new(10, 30), AbsoluteContactRange::new(30, 50),),
            0
        );
        assert_eq!(
            native_horizontal_rect_overlap(AbsoluteContactRange::new(10, 31), AbsoluteContactRange::new(30, 50),),
            1
        );
        assert_eq!(
            native_horizontal_rect_overlap(AbsoluteContactRange::new(10, 20), AbsoluteContactRange::new(30, 50),),
            -10
        );
        assert_eq!(
            native_horizontal_rect_overlap(AbsoluteContactRange::new(50, 10), AbsoluteContactRange::new(30, 50),),
            0
        );
    }

    #[test]
    fn attack_reach_uses_closed_bounds() {
        assert!(attack_reaches_defense(
            AbsoluteContactRange::new(10, 20),
            AbsoluteContactRange::new(20, 30),
        ));
        assert!(!attack_reaches_defense(
            AbsoluteContactRange::new(10, 19),
            AbsoluteContactRange::new(20, 30),
        ));
        assert!(!attack_reaches_defense(
            AbsoluteContactRange::new(30, 20),
            AbsoluteContactRange::new(20, 30),
        ));
    }

    #[test]
    fn circle_hits_rect_handles_boundaries_and_invalid_inputs() {
        let rect = ContactRect::new(10, 10, 20, 20);
        assert!(circle_hits_rect(ContactCircle::new(0, 15, 10), rect));
        assert!(!circle_hits_rect(ContactCircle::new(0, 15, 9), rect));
        assert!(circle_hits_rect(ContactCircle::new(15, 15, 0), rect));
        assert!(circle_hits_rect(ContactCircle::new(20, 20, 0), rect));
        assert!(!circle_hits_rect(ContactCircle::new(15, 15, -1), rect));
        assert!(!circle_hits_rect(
            ContactCircle::new(15, 15, 1),
            ContactRect::new(20, 10, 10, 20),
        ));
    }

    #[test]
    fn circle_distance_uses_wide_arithmetic() {
        let rect = ContactRect::new(i32::MAX - 10, i32::MAX - 10, i32::MAX, i32::MAX);
        assert!(!circle_hits_rect(
            ContactCircle::new(i32::MIN, i32::MIN, i32::MAX),
            rect
        ));
    }

    #[test]
    fn zombie_horizontal_threat_requires_same_row_and_twenty_pixel_overlap() {
        let target = plant(ContactRect::new(100, 0, 180, 80));
        let near = ZombiePlantThreatShape::SameRowRange {
            row: 2,
            range: AbsoluteContactRange::new(80, 120),
        };
        let short = ZombiePlantThreatShape::SameRowRange {
            row: 2,
            range: AbsoluteContactRange::new(80, 119),
        };
        let wrong_row = ZombiePlantThreatShape::SameRowRange {
            row: 1,
            range: AbsoluteContactRange::new(80, 140),
        };
        assert!(zombie_threat_hits_plant_geometry(near, target));
        assert!(!zombie_threat_hits_plant_geometry(short, target));
        assert!(!zombie_threat_hits_plant_geometry(wrong_row, target));
    }

    #[test]
    fn zombie_circle_threat_does_not_require_same_row() {
        let target = plant(ContactRect::new(100, 100, 180, 180));
        assert!(zombie_threat_hits_plant_geometry(
            ZombiePlantThreatShape::Circle(ContactCircle::new(140, 140, 1)),
            target,
        ));
    }

    #[test]
    fn plant_threat_filters_rows_and_invalid_geometry() {
        let threat = PlantThreatShape::new(
            PlantThreatKind::CherryBomb,
            2,
            1,
            DamageRangeFlags::ALL_NON_MIND_CONTROLLED,
            ContactCircle::new(100, 100, 50),
        );
        assert!(plant_threat_hits_zombie_geometry(
            threat,
            zombie_defense(3, ContactRect::new(80, 80, 120, 120)),
        ));
        assert!(!plant_threat_hits_zombie_geometry(
            threat,
            zombie_defense(4, ContactRect::new(80, 80, 120, 120)),
        ));

        let invalid = PlantThreatShape::new(
            PlantThreatKind::CherryBomb,
            2,
            -1,
            DamageRangeFlags::ALL_NON_MIND_CONTROLLED,
            ContactCircle::new(100, 100, 50),
        );
        assert!(!plant_threat_hits_zombie_geometry(
            invalid,
            zombie_defense(2, ContactRect::new(80, 80, 120, 120)),
        ));
    }

    #[test]
    fn current_attack_reach_requires_same_row() {
        let attack = ZombieAttackBounds {
            id: ZombieId::from_raw(1),
            kind: ZombieKind::Normal,
            row: 2,
            x: 100,
            range: AbsoluteContactRange::new(80, 120),
        };
        let defense = PlantDefenseBounds {
            id: PlantId::from_raw(1),
            kind: PlantKind::Peashooter,
            defense_kind: PlantDefenseKind::ChewCrushSmash,
            grid: Grid { row: 2, col: 3 },
            x: 100,
            range: AbsoluteContactRange::new(120, 160),
        };
        assert!(same_row_and_attack_reaches_plant_geometry(attack, defense));

        let wrong_row = PlantDefenseBounds {
            grid: Grid { row: 1, col: 3 },
            ..defense
        };
        assert!(!same_row_and_attack_reaches_plant_geometry(attack, wrong_row));
    }

    #[test]
    fn candidate_shape_reports_invalid_geometry() {
        let facts = ZombieThreatCandidateFacts::SameRowAttack {
            id: ZombieId::from_raw(1),
            kind: ZombieKind::Normal,
            threat_kind: ZombiePlantThreatKind::DriveOver,
            geometry: ZombieGeometryInput {
                kind: ZombieKind::Normal,
                row: 0,
                x: 0.0,
                profile: ZombieContactProfile::CommonBody,
                orientation: rsvz_model::ZombieGeometryOrientation::Normal,
                body_width: 0,
            },
        };

        assert!(matches!(
            zombie_threat_candidate_shape(facts),
            Err(ZombieGeometryError::InvalidBodyWidth)
        ));
    }
}
