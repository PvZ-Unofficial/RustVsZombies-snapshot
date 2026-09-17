//! Pure backend-neutral zombie contact geometry.

use rsvz_model::{
    ContactRect, PredictedZombieAttackBounds, RelativeContactRect, ZombieContactProfile, ZombieDefenseDrawPose,
    ZombieDefenseGeometry, ZombieDefensePhase, ZombieDefenseSpec, ZombieDefenseState, ZombieGeometryError,
    ZombieGeometryInput, ZombieGeometryOrientation, ZombieHeightState, ZombieKind,
};

const CLIP_HEIGHT_LIMIT: f32 = -100.0;
const CLIP_HEIGHT_OFF: f32 = -200.0;
const HIGH_GROUND_HEIGHT: f32 = 30.0;

pub const fn is_profile_compatible(kind: ZombieKind, profile: ZombieContactProfile) -> bool {
    match profile {
        ZombieContactProfile::CommonBody => matches!(
            kind,
            ZombieKind::Normal
                | ZombieKind::Flag
                | ZombieKind::Conehead
                | ZombieKind::Buckethead
                | ZombieKind::Newspaper
                | ZombieKind::ScreenDoor
                | ZombieKind::DuckyTube
                | ZombieKind::JackInTheBox
                | ZombieKind::Yeti
                | ZombieKind::PeaHead
                | ZombieKind::WallNutHead
                | ZombieKind::JalapenoHead
                | ZombieKind::GatlingHead
                | ZombieKind::SquashHead
                | ZombieKind::TallNutHead
        ),
        ZombieContactProfile::NarrowBiteBody => matches!(
            kind,
            ZombieKind::Normal | ZombieKind::PoleVaulting | ZombieKind::DolphinRider
        ),
        ZombieContactProfile::Football => matches!(kind, ZombieKind::Football),
        ZombieContactProfile::Digger => matches!(kind, ZombieKind::Digger),
        ZombieContactProfile::Snorkel => matches!(kind, ZombieKind::Snorkel),
        ZombieContactProfile::Ladder => matches!(kind, ZombieKind::Ladder),
        ZombieContactProfile::Vehicle => matches!(kind, ZombieKind::Zomboni | ZombieKind::Catapult),
        ZombieContactProfile::Gargantuar => matches!(kind, ZombieKind::Gargantuar | ZombieKind::GigaGargantuar),
        ZombieContactProfile::PoleBeforeVault | ZombieContactProfile::PoleVaulting => {
            matches!(kind, ZombieKind::PoleVaulting)
        }
        ZombieContactProfile::PogoMounted | ZombieContactProfile::PogoOnFoot => matches!(kind, ZombieKind::Pogo),
        ZombieContactProfile::Balloon => matches!(kind, ZombieKind::Balloon),
        ZombieContactProfile::DolphinRiding
        | ZombieContactProfile::DolphinJumping
        | ZombieContactProfile::DolphinOnFoot => matches!(kind, ZombieKind::DolphinRider),
        _ => false,
    }
}

pub const fn default_body_width(kind: ZombieKind, profile: ZombieContactProfile) -> Result<i32, ZombieGeometryError> {
    if !is_profile_compatible(kind, profile) {
        return Err(ZombieGeometryError::IncompatibleKindProfile);
    }
    Ok(match profile {
        ZombieContactProfile::Gargantuar => 180,
        ZombieContactProfile::Vehicle => 153,
        _ => 80,
    })
}

pub fn zombie_geometry_input_from_profile(
    kind: ZombieKind, row: i32, x: f32, profile: ZombieContactProfile,
) -> Result<ZombieGeometryInput, ZombieGeometryError> {
    Ok(ZombieGeometryInput {
        kind,
        row,
        x,
        profile,
        orientation: ZombieGeometryOrientation::Normal,
        body_width: default_body_width(kind, profile)?,
    })
}

pub(crate) fn zombie_defense_state_from_profile(
    kind: ZombieKind, row: i32, x: i32, y: i32, profile: ZombieContactProfile,
) -> Result<ZombieDefenseState, ZombieGeometryError> {
    ZombieDefenseState::from_base_rect(
        kind,
        row,
        x,
        y,
        default_body_width(kind, profile)?,
        base_zombie_rect(profile)?,
    )
}

pub fn predicted_zombie_attack_bounds(
    input: ZombieGeometryInput,
) -> Result<PredictedZombieAttackBounds, ZombieGeometryError> {
    validate_input(input)?;
    let x = pvz_trunc_f32_to_i32(input.x)?;
    let rect = apply_horizontal_mirror(
        zombie_profile_attack_rect(input.profile)?,
        input.body_width,
        input.orientation,
    )?;
    Ok(PredictedZombieAttackBounds {
        kind: input.kind,
        row: input.row,
        x,
        range: rect
            .horizontal_range_at(x)
            .ok_or(ZombieGeometryError::InvalidCoordinate)?,
    })
}

pub fn zombie_defense_geometry(spec: ZombieDefenseSpec) -> Result<ZombieDefenseGeometry, ZombieGeometryError> {
    let profile = spec.profile.unwrap_or_else(|| default_defense_profile(spec));
    if !is_profile_compatible(spec.kind, profile) {
        return Err(ZombieGeometryError::IncompatibleKindProfile);
    }
    let body_width = spec.body_width.unwrap_or(default_body_width(spec.kind, profile)?);
    if body_width <= 0 {
        return Err(ZombieGeometryError::InvalidBodyWidth);
    }
    let x = pvz_trunc_f32_to_i32(spec.x)?;
    let y = pvz_trunc_f32_to_i32(spec.row_y.resolve(spec.row))?;
    let base_rect = apply_horizontal_mirror(base_zombie_rect(profile)?, body_width, spec.orientation)?;
    let mut state = ZombieDefenseState::from_base_rect(spec.kind, spec.row, x, y, body_width, base_rect)?;
    state.phase = spec.phase;
    state.mind_controlled = spec.mind_controlled;
    state.has_object = spec.has_object.unwrap_or(true);
    state.in_pool = spec.in_pool;
    state.on_high_ground = spec.on_high_ground;
    state.is_eating = spec.is_eating;
    state.zombie_height = spec.zombie_height;
    state.altitude = spec.altitude;
    state.phase_counter = spec.phase_counter;
    state.scale_zombie = spec.scale_zombie;
    state.vel_z = spec.vel_z;
    state.body_reanim_anim_time = spec.body_reanim_anim_time;
    zombie_defense_geometry_from_state(state)
}

pub fn zombie_defense_draw_pose(state: ZombieDefenseState) -> Result<ZombieDefenseDrawPose, ZombieGeometryError> {
    let body_y = -state.altitude;
    let mut clip_height = CLIP_HEIGHT_OFF;

    if state.phase == ZombieDefensePhase::RisingFromGrave {
        clip_height = if state.in_pool {
            body_y
        } else {
            body_y + (state.phase_counter as f32).min(40.0)
        };
        return Ok(ZombieDefenseDrawPose {
            body_y: high_ground_adjusted_body_y(body_y, state.on_high_ground),
            clip_height,
        });
    }

    if state.kind == ZombieKind::DolphinRider {
        if state.phase == ZombieDefensePhase::DolphinIntoPool {
            let anim = required_body_reanim_anim_time(state)?;
            if (0.56..=0.65).contains(&anim) {
                clip_height = 0.0;
            } else if anim >= 0.75 {
                clip_height = -state.altitude - 10.0;
            }
        } else if state.phase == ZombieDefensePhase::DolphinRiding {
            clip_height = if state.zombie_height == ZombieHeightState::DraggedUnder {
                -state.altitude - 15.0
            } else {
                -state.altitude - 10.0
            };
        } else if state.phase == ZombieDefensePhase::DolphinInJump {
            let anim = required_body_reanim_anim_time(state)?;
            if anim <= 0.06 {
                clip_height = -state.altitude - 10.0;
            } else if (0.5..=0.76).contains(&anim) {
                clip_height = -13.0;
            }
        } else if matches!(
            state.phase,
            ZombieDefensePhase::DolphinWalkingInPool | ZombieDefensePhase::ZombieDying
        ) {
            if state.phase == ZombieDefensePhase::ZombieDying {
                clip_height = -state.altitude + 44.0;
            } else if state.zombie_height == ZombieHeightState::DraggedUnder {
                clip_height = -state.altitude + 36.0;
            }
        } else if matches!(
            state.phase,
            ZombieDefensePhase::DolphinWalking | ZombieDefensePhase::DolphinWalkingWithoutDolphin
        ) && state.zombie_height == ZombieHeightState::OutOfPool
        {
            clip_height = -state.altitude;
        }
        return Ok(ZombieDefenseDrawPose { body_y, clip_height });
    }

    if state.kind == ZombieKind::Snorkel {
        if state.phase == ZombieDefensePhase::SnorkelIntoPool {
            let anim = required_body_reanim_anim_time(state)?;
            if anim >= 0.8 {
                clip_height = -10.0;
            }
        } else if state.in_pool {
            clip_height = -state.altitude - 5.0;
            clip_height += 20.0 - 20.0 * state.scale_zombie;
        }
        return Ok(ZombieDefenseDrawPose { body_y, clip_height });
    }

    if state.in_pool {
        clip_height = -state.altitude - 7.0;
        clip_height += 10.0 - 10.0 * state.scale_zombie;
        if state.is_eating {
            clip_height += 7.0;
        }
        return Ok(ZombieDefenseDrawPose { body_y, clip_height });
    }

    if state.phase == ZombieDefensePhase::DancerRising {
        return Ok(ZombieDefenseDrawPose {
            body_y: high_ground_adjusted_body_y(body_y, state.on_high_ground),
            clip_height: -state.altitude,
        });
    }

    if matches!(
        state.phase,
        ZombieDefensePhase::DiggerRising | ZombieDefensePhase::DiggerRiseWithoutAxe
    ) {
        return Ok(ZombieDefenseDrawPose {
            body_y,
            clip_height: if state.phase_counter > 20 {
                -state.altitude
            } else {
                CLIP_HEIGHT_OFF
            },
        });
    }

    if state.kind == ZombieKind::Bungee {
        return Ok(ZombieDefenseDrawPose {
            body_y: high_ground_adjusted_body_y(body_y, state.on_high_ground),
            clip_height,
        });
    }

    Ok(ZombieDefenseDrawPose { body_y, clip_height })
}

pub fn zombie_defense_geometry_from_state(
    state: ZombieDefenseState,
) -> Result<ZombieDefenseGeometry, ZombieGeometryError> {
    if state.body_width <= 0 {
        return Err(ZombieGeometryError::InvalidBodyWidth);
    }
    if !state.base_rect.is_valid() {
        return Err(ZombieGeometryError::InvalidRelativeRect);
    }
    let draw = zombie_defense_draw_pose(state)?;
    let rect = if is_walking_backwards(state) {
        let x_offset = checked_sub(
            checked_sub(state.body_width, state.base_rect.x_offset)?,
            state.base_rect.width,
        )?;
        RelativeContactRect {
            x_offset,
            ..state.base_rect
        }
    } else {
        state.base_rect
    };
    let absolute_x = state
        .x
        .checked_add(rect.x_offset)
        .ok_or(ZombieGeometryError::InvalidCoordinate)?;
    let y_with_body = pvz_trunc_f32_to_i32(state.y as f32 + draw.body_y)?;
    let absolute_y = y_with_body
        .checked_add(rect.y_offset)
        .ok_or(ZombieGeometryError::InvalidCoordinate)?;
    let height = if draw.clip_height > CLIP_HEIGHT_LIMIT {
        pvz_trunc_f32_to_i32(rect.height as f32 - draw.clip_height)?
    } else {
        rect.height
    };
    let rect = ContactRect::from_pos_size(absolute_x, absolute_y, rect.width, height)
        .ok_or(ZombieGeometryError::InvalidCoordinate)?;
    if !rect.is_valid() {
        return Err(ZombieGeometryError::InvalidRelativeRect);
    }
    Ok(ZombieDefenseGeometry {
        kind: state.kind,
        row: state.row,
        x: state.x,
        rect,
    })
}

const fn default_defense_profile(spec: ZombieDefenseSpec) -> ZombieContactProfile {
    match spec.kind {
        ZombieKind::Football => ZombieContactProfile::Football,
        ZombieKind::Digger => ZombieContactProfile::Digger,
        ZombieKind::Snorkel => ZombieContactProfile::Snorkel,
        ZombieKind::Ladder => ZombieContactProfile::Ladder,
        ZombieKind::Zomboni | ZombieKind::Catapult => ZombieContactProfile::Vehicle,
        ZombieKind::Gargantuar | ZombieKind::GigaGargantuar => ZombieContactProfile::Gargantuar,
        ZombieKind::PoleVaulting => ZombieContactProfile::PoleBeforeVault,
        ZombieKind::Pogo if matches!(spec.has_object, Some(false)) => ZombieContactProfile::PogoOnFoot,
        ZombieKind::Pogo => ZombieContactProfile::PogoMounted,
        ZombieKind::Balloon => ZombieContactProfile::Balloon,
        ZombieKind::DolphinRider => match spec.phase {
            ZombieDefensePhase::DolphinWalking | ZombieDefensePhase::DolphinIntoPool => {
                ZombieContactProfile::NarrowBiteBody
            }
            ZombieDefensePhase::DolphinInJump => ZombieContactProfile::DolphinJumping,
            ZombieDefensePhase::DolphinWalkingInPool | ZombieDefensePhase::DolphinWalkingWithoutDolphin => {
                ZombieContactProfile::DolphinOnFoot
            }
            _ => ZombieContactProfile::DolphinRiding,
        },
        _ => ZombieContactProfile::CommonBody,
    }
}

pub fn pvz_trunc_f32_to_i32(value: f32) -> Result<i32, ZombieGeometryError> {
    const I32_MIN_F32: f32 = -2_147_483_648.0;
    const I32_MAX_EXCLUSIVE_F32: f32 = 2_147_483_648.0;

    if !value.is_finite() || !(I32_MIN_F32..I32_MAX_EXCLUSIVE_F32).contains(&value) {
        return Err(ZombieGeometryError::InvalidCoordinate);
    }
    Ok(value as i32)
}

fn validate_input(input: ZombieGeometryInput) -> Result<(), ZombieGeometryError> {
    if !is_profile_compatible(input.kind, input.profile) {
        return Err(ZombieGeometryError::IncompatibleKindProfile);
    }
    if input.body_width <= 0 {
        return Err(ZombieGeometryError::InvalidBodyWidth);
    }
    let _ = pvz_trunc_f32_to_i32(input.x)?;
    Ok(())
}

fn base_zombie_rect(profile: ZombieContactProfile) -> Result<RelativeContactRect, ZombieGeometryError> {
    Ok(match profile {
        ZombieContactProfile::CommonBody
        | ZombieContactProfile::NarrowBiteBody
        | ZombieContactProfile::PoleBeforeVault
        | ZombieContactProfile::PoleVaulting
        | ZombieContactProfile::PogoMounted => RelativeContactRect::new(36, 0, 42, 115),
        ZombieContactProfile::Football => RelativeContactRect::new(50, 0, 57, 115),
        ZombieContactProfile::Digger => RelativeContactRect::new(50, 0, 28, 115),
        ZombieContactProfile::Snorkel => RelativeContactRect::new(12, 0, 62, 115),
        ZombieContactProfile::Ladder => RelativeContactRect::new(36, 0, 42, 115),
        ZombieContactProfile::Vehicle => RelativeContactRect::new(0, -13, 153, 140),
        ZombieContactProfile::Gargantuar => RelativeContactRect::new(-17, -38, 125, 154),
        ZombieContactProfile::PogoOnFoot => RelativeContactRect::new(36, 17, 42, 115),
        ZombieContactProfile::Balloon => RelativeContactRect::new(36, 30, 42, 115),
        ZombieContactProfile::DolphinRiding | ZombieContactProfile::DolphinJumping => {
            RelativeContactRect::new(36, 0, 42, 115)
        }
        ZombieContactProfile::DolphinOnFoot => RelativeContactRect::new(20, 0, 42, 115),
        _ => return Err(ZombieGeometryError::UnsupportedProfile),
    })
}

pub(crate) fn zombie_profile_attack_rect(
    profile: ZombieContactProfile,
) -> Result<RelativeContactRect, ZombieGeometryError> {
    Ok(match profile {
        ZombieContactProfile::CommonBody => RelativeContactRect::new(20, 0, 50, 115),
        ZombieContactProfile::NarrowBiteBody | ZombieContactProfile::Football => {
            RelativeContactRect::new(50, 0, 20, 115)
        }
        ZombieContactProfile::Digger => RelativeContactRect::new(50, 0, 20, 115),
        ZombieContactProfile::Snorkel => RelativeContactRect::new(-5, 0, 55, 115),
        ZombieContactProfile::Ladder => RelativeContactRect::new(10, 0, 50, 115),
        ZombieContactProfile::Vehicle => RelativeContactRect::new(10, -13, 133, 140),
        ZombieContactProfile::Gargantuar => RelativeContactRect::new(-30, -38, 89, 154),
        ZombieContactProfile::PoleBeforeVault => RelativeContactRect::new(-29, 0, 70, 115),
        ZombieContactProfile::PoleVaulting | ZombieContactProfile::DolphinJumping => {
            RelativeContactRect::new(-40, 0, 100, 115)
        }
        ZombieContactProfile::PogoMounted => RelativeContactRect::new(10, 0, 30, 115),
        ZombieContactProfile::PogoOnFoot => RelativeContactRect::new(20, 17, 50, 115),
        ZombieContactProfile::Balloon => RelativeContactRect::new(20, 30, 50, 115),
        ZombieContactProfile::DolphinRiding => RelativeContactRect::new(-29, 0, 70, 115),
        ZombieContactProfile::DolphinOnFoot => RelativeContactRect::new(30, 0, 30, 115),
        _ => return Err(ZombieGeometryError::UnsupportedProfile),
    })
}

fn apply_horizontal_mirror(
    rect: RelativeContactRect, body_width: i32, orientation: ZombieGeometryOrientation,
) -> Result<RelativeContactRect, ZombieGeometryError> {
    if body_width <= 0 {
        return Err(ZombieGeometryError::InvalidBodyWidth);
    }
    match orientation {
        ZombieGeometryOrientation::Normal => Ok(rect),
        ZombieGeometryOrientation::Mirrored => {
            let x_offset = checked_sub(checked_sub(body_width, rect.x_offset)?, rect.width)?;
            Ok(RelativeContactRect { x_offset, ..rect })
        }
        _ => Err(ZombieGeometryError::UnsupportedProfile),
    }
}

fn required_body_reanim_anim_time(state: ZombieDefenseState) -> Result<f32, ZombieGeometryError> {
    state
        .body_reanim_anim_time
        .filter(|anim_time| anim_time.is_finite())
        .ok_or(ZombieGeometryError::MissingBodyReanimationTime)
}

const fn high_ground_adjusted_body_y(body_y: f32, on_high_ground: bool) -> f32 {
    if on_high_ground {
        body_y - HIGH_GROUND_HEIGHT
    } else {
        body_y
    }
}

const fn is_walking_backwards(state: ZombieDefenseState) -> bool {
    if state.mind_controlled {
        return true;
    }
    if matches!(state.zombie_height, ZombieHeightState::Zombiquarium) {
        return state.vel_z < 1.570_796_4 || state.vel_z > 4.712_389;
    }
    if matches!(state.kind, ZombieKind::Digger) {
        return match state.phase {
            ZombieDefensePhase::DiggerRising
            | ZombieDefensePhase::DiggerStunned
            | ZombieDefensePhase::DiggerWalking => true,
            ZombieDefensePhase::ZombieDying | ZombieDefensePhase::ZombieBurned | ZombieDefensePhase::ZombieMowered => {
                state.has_object
            }
            ZombieDefensePhase::Other
            | ZombieDefensePhase::RisingFromGrave
            | ZombieDefensePhase::DancerRising
            | ZombieDefensePhase::DiggerRiseWithoutAxe
            | ZombieDefensePhase::DolphinWalking
            | ZombieDefensePhase::DolphinIntoPool
            | ZombieDefensePhase::DolphinRiding
            | ZombieDefensePhase::DolphinInJump
            | ZombieDefensePhase::DolphinWalkingInPool
            | ZombieDefensePhase::DolphinWalkingWithoutDolphin
            | ZombieDefensePhase::SnorkelIntoPool => false,
        };
    }
    matches!(state.kind, ZombieKind::Yeti) && !state.has_object
}

fn checked_sub(lhs: i32, rhs: i32) -> Result<i32, ZombieGeometryError> {
    lhs.checked_sub(rhs).ok_or(ZombieGeometryError::InvalidBodyWidth)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsvz_model::AbsoluteContactRange;

    #[test]
    fn all_supported_profile_attack_ranges_match_decompiled_rects() {
        let cases = [
            (
                zombie_geometry_input_from_profile(ZombieKind::Normal, 0, 100.0, ZombieContactProfile::CommonBody)
                    .expect("common body"),
                AbsoluteContactRange::new(120, 170),
            ),
            (
                zombie_geometry_input_from_profile(ZombieKind::Normal, 0, 100.0, ZombieContactProfile::NarrowBiteBody)
                    .expect("narrow bite body"),
                AbsoluteContactRange::new(150, 170),
            ),
            (
                zombie_geometry_input_from_profile(ZombieKind::Football, 0, 100.0, ZombieContactProfile::Football)
                    .expect("football"),
                AbsoluteContactRange::new(150, 170),
            ),
            (
                zombie_geometry_input_from_profile(ZombieKind::Digger, 0, 100.0, ZombieContactProfile::Digger)
                    .expect("digger"),
                AbsoluteContactRange::new(150, 170),
            ),
            (
                zombie_geometry_input_from_profile(ZombieKind::Snorkel, 0, 100.0, ZombieContactProfile::Snorkel)
                    .expect("snorkel"),
                AbsoluteContactRange::new(95, 150),
            ),
            (
                zombie_geometry_input_from_profile(ZombieKind::Ladder, 0, 100.0, ZombieContactProfile::Ladder)
                    .expect("ladder"),
                AbsoluteContactRange::new(110, 160),
            ),
            (
                zombie_geometry_input_from_profile(ZombieKind::Zomboni, 0, 100.0, ZombieContactProfile::Vehicle)
                    .expect("zomboni"),
                AbsoluteContactRange::new(110, 243),
            ),
            (
                zombie_geometry_input_from_profile(ZombieKind::Gargantuar, 0, 100.0, ZombieContactProfile::Gargantuar)
                    .expect("gargantuar"),
                AbsoluteContactRange::new(70, 159),
            ),
            (
                zombie_geometry_input_from_profile(
                    ZombieKind::PoleVaulting,
                    0,
                    100.0,
                    ZombieContactProfile::PoleBeforeVault,
                )
                .expect("pole before vault"),
                AbsoluteContactRange::new(71, 141),
            ),
            (
                zombie_geometry_input_from_profile(
                    ZombieKind::PoleVaulting,
                    0,
                    100.0,
                    ZombieContactProfile::PoleVaulting,
                )
                .expect("pole vaulting"),
                AbsoluteContactRange::new(60, 160),
            ),
            (
                zombie_geometry_input_from_profile(ZombieKind::Pogo, 0, 100.0, ZombieContactProfile::PogoMounted)
                    .expect("pogo mounted"),
                AbsoluteContactRange::new(110, 140),
            ),
            (
                zombie_geometry_input_from_profile(ZombieKind::Pogo, 0, 100.0, ZombieContactProfile::PogoOnFoot)
                    .expect("pogo on foot"),
                AbsoluteContactRange::new(120, 170),
            ),
            (
                zombie_geometry_input_from_profile(ZombieKind::Balloon, 0, 100.0, ZombieContactProfile::Balloon)
                    .expect("balloon"),
                AbsoluteContactRange::new(120, 170),
            ),
            (
                zombie_geometry_input_from_profile(
                    ZombieKind::DolphinRider,
                    0,
                    100.0,
                    ZombieContactProfile::DolphinRiding,
                )
                .expect("dolphin riding"),
                AbsoluteContactRange::new(71, 141),
            ),
            (
                zombie_geometry_input_from_profile(
                    ZombieKind::DolphinRider,
                    0,
                    100.0,
                    ZombieContactProfile::DolphinJumping,
                )
                .expect("dolphin jumping"),
                AbsoluteContactRange::new(60, 160),
            ),
            (
                zombie_geometry_input_from_profile(
                    ZombieKind::DolphinRider,
                    0,
                    100.0,
                    ZombieContactProfile::DolphinOnFoot,
                )
                .expect("dolphin on foot"),
                AbsoluteContactRange::new(130, 160),
            ),
        ];

        for (input, expected) in cases {
            assert_eq!(
                predicted_zombie_attack_bounds(input).expect("prediction").range,
                expected
            );
        }
    }

    #[test]
    fn mirrored_orientation_uses_body_width() {
        let input = zombie_geometry_input_from_profile(
            ZombieKind::PoleVaulting,
            0,
            100.0,
            ZombieContactProfile::PoleBeforeVault,
        )
        .expect("pole")
        .with_body_width(80)
        .expect("width")
        .mirrored();
        assert_eq!(
            predicted_zombie_attack_bounds(input).expect("prediction").range,
            AbsoluteContactRange::new(139, 209)
        );
    }

    #[test]
    fn defense_spec_explicit_y_uses_endpoint_rect() {
        let input = ZombieDefenseSpec::new(ZombieKind::Gargantuar, 4, 100.9).y(200.9);
        let defense = zombie_defense_geometry(input).expect("defense");
        assert_eq!(defense.x, 100);
        assert_eq!(defense.rect, ContactRect::new(83, 162, 208, 316));
    }

    #[test]
    fn defense_spec_defaults_cover_common_zombies() {
        for (kind, expected) in [
            (ZombieKind::Normal, ContactRect::new(136, 220, 178, 335)),
            (ZombieKind::JackInTheBox, ContactRect::new(136, 220, 178, 335)),
            (ZombieKind::Football, ContactRect::new(150, 220, 207, 335)),
            (ZombieKind::Gargantuar, ContactRect::new(83, 182, 208, 336)),
        ] {
            let defense = zombie_defense_geometry(ZombieDefenseSpec::new(kind, 2, 100.0)).expect("defense geometry");
            assert_eq!(defense.rect, expected, "{kind:?}");
        }
    }

    #[test]
    fn defense_spec_keeps_explicit_orientation_for_yeti() {
        let normal = zombie_defense_geometry(ZombieDefenseSpec::new(ZombieKind::Yeti, 2, 100.0).y(200.0))
            .expect("normal yeti defense");
        assert_eq!(normal.rect, ContactRect::new(136, 200, 178, 315));

        let mirrored = zombie_defense_geometry(ZombieDefenseSpec::new(ZombieKind::Yeti, 2, 100.0).mirrored().y(200.0))
            .expect("mirrored yeti defense");
        assert_eq!(mirrored.rect, ContactRect::new(102, 200, 144, 315));
    }

    #[test]
    fn defense_spec_special_state_defaults_use_matching_profiles() {
        let dolphin_jump = zombie_defense_geometry(
            ZombieDefenseSpec::new(ZombieKind::DolphinRider, 2, 100.0)
                .phase(ZombieDefensePhase::DolphinInJump)
                .body_reanim_anim_time(0.5)
                .y(200.0),
        )
        .expect("dolphin jump defense");
        assert_eq!(dolphin_jump.rect, ContactRect::new(136, 200, 178, 328));

        let snorkel = zombie_defense_geometry(
            ZombieDefenseSpec::new(ZombieKind::Snorkel, 2, 100.0)
                .in_pool(true)
                .y(200.0),
        )
        .expect("snorkel defense");
        assert_eq!(snorkel.rect, ContactRect::new(112, 200, 174, 320));
    }

    #[test]
    fn truncation_matches_pvz_casts_and_rejects_invalid_values() {
        assert_eq!(pvz_trunc_f32_to_i32(100.1), Ok(100));
        assert_eq!(pvz_trunc_f32_to_i32(100.9), Ok(100));
        assert_eq!(pvz_trunc_f32_to_i32(-10.9), Ok(-10));
        assert_eq!(pvz_trunc_f32_to_i32(-2_147_483_648.0), Ok(i32::MIN));
        assert_eq!(pvz_trunc_f32_to_i32(2_147_483_520.0), Ok(2_147_483_520));
        assert_eq!(
            pvz_trunc_f32_to_i32(f32::NAN),
            Err(ZombieGeometryError::InvalidCoordinate)
        );
        assert_eq!(
            pvz_trunc_f32_to_i32(f32::INFINITY),
            Err(ZombieGeometryError::InvalidCoordinate)
        );
        assert_eq!(
            pvz_trunc_f32_to_i32(2_147_483_648.0),
            Err(ZombieGeometryError::InvalidCoordinate)
        );
        assert_eq!(
            pvz_trunc_f32_to_i32(-2_147_483_904.0),
            Err(ZombieGeometryError::InvalidCoordinate)
        );
    }

    #[test]
    fn compatibility_and_body_width_are_checked() {
        assert_eq!(
            zombie_geometry_input_from_profile(ZombieKind::Normal, 0, 100.0, ZombieContactProfile::Vehicle),
            Err(ZombieGeometryError::IncompatibleKindProfile)
        );
        assert_eq!(
            zombie_geometry_input_from_profile(ZombieKind::Normal, 0, 100.0, ZombieContactProfile::CommonBody)
                .expect("normal")
                .with_body_width(0),
            Err(ZombieGeometryError::InvalidBodyWidth)
        );
        assert_eq!(
            default_body_width(ZombieKind::GigaGargantuar, ZombieContactProfile::Gargantuar),
            Ok(180)
        );
        assert_eq!(
            zombie_geometry_input_from_profile(ZombieKind::Imp, 0, 100.0, ZombieContactProfile::NarrowBiteBody),
            Err(ZombieGeometryError::IncompatibleKindProfile)
        );
        assert_eq!(
            zombie_geometry_input_from_profile(ZombieKind::Dancing, 0, 100.0, ZombieContactProfile::NarrowBiteBody),
            Err(ZombieGeometryError::IncompatibleKindProfile)
        );
        assert_eq!(
            zombie_geometry_input_from_profile(
                ZombieKind::BackupDancer,
                0,
                100.0,
                ZombieContactProfile::NarrowBiteBody,
            ),
            Err(ZombieGeometryError::IncompatibleKindProfile)
        );
    }

    #[test]
    fn compatibility_table_matches_supported_profiles() {
        let valid_pairs = [
            (ZombieKind::Normal, ZombieContactProfile::CommonBody),
            (ZombieKind::Flag, ZombieContactProfile::CommonBody),
            (ZombieKind::Conehead, ZombieContactProfile::CommonBody),
            (ZombieKind::Buckethead, ZombieContactProfile::CommonBody),
            (ZombieKind::Newspaper, ZombieContactProfile::CommonBody),
            (ZombieKind::ScreenDoor, ZombieContactProfile::CommonBody),
            (ZombieKind::DuckyTube, ZombieContactProfile::CommonBody),
            (ZombieKind::JackInTheBox, ZombieContactProfile::CommonBody),
            (ZombieKind::Yeti, ZombieContactProfile::CommonBody),
            (ZombieKind::PeaHead, ZombieContactProfile::CommonBody),
            (ZombieKind::WallNutHead, ZombieContactProfile::CommonBody),
            (ZombieKind::JalapenoHead, ZombieContactProfile::CommonBody),
            (ZombieKind::GatlingHead, ZombieContactProfile::CommonBody),
            (ZombieKind::SquashHead, ZombieContactProfile::CommonBody),
            (ZombieKind::TallNutHead, ZombieContactProfile::CommonBody),
            (ZombieKind::Normal, ZombieContactProfile::NarrowBiteBody),
            (ZombieKind::PoleVaulting, ZombieContactProfile::NarrowBiteBody),
            (ZombieKind::DolphinRider, ZombieContactProfile::NarrowBiteBody),
            (ZombieKind::Football, ZombieContactProfile::Football),
            (ZombieKind::Digger, ZombieContactProfile::Digger),
            (ZombieKind::Snorkel, ZombieContactProfile::Snorkel),
            (ZombieKind::Ladder, ZombieContactProfile::Ladder),
            (ZombieKind::Zomboni, ZombieContactProfile::Vehicle),
            (ZombieKind::Catapult, ZombieContactProfile::Vehicle),
            (ZombieKind::Gargantuar, ZombieContactProfile::Gargantuar),
            (ZombieKind::GigaGargantuar, ZombieContactProfile::Gargantuar),
            (ZombieKind::PoleVaulting, ZombieContactProfile::PoleBeforeVault),
            (ZombieKind::PoleVaulting, ZombieContactProfile::PoleVaulting),
            (ZombieKind::Pogo, ZombieContactProfile::PogoMounted),
            (ZombieKind::Pogo, ZombieContactProfile::PogoOnFoot),
            (ZombieKind::Balloon, ZombieContactProfile::Balloon),
            (ZombieKind::DolphinRider, ZombieContactProfile::DolphinRiding),
            (ZombieKind::DolphinRider, ZombieContactProfile::DolphinJumping),
            (ZombieKind::DolphinRider, ZombieContactProfile::DolphinOnFoot),
        ];

        for (kind, profile) in valid_pairs {
            assert!(is_profile_compatible(kind, profile), "{kind:?}/{profile:?}");
        }

        for kind in [ZombieKind::Imp, ZombieKind::Dancing, ZombieKind::BackupDancer] {
            assert!(
                !is_profile_compatible(kind, ZombieContactProfile::NarrowBiteBody),
                "unvalidated {kind:?} must not be public-compatible yet"
            );
        }
    }

    fn exact_state(kind: ZombieKind) -> ZombieDefenseState {
        ZombieDefenseState::from_base_rect(kind, 2, 100, 200, 80, RelativeContactRect::new(10, 5, 20, 40))
            .expect("valid exact state")
    }

    fn draw(state: ZombieDefenseState) -> ZombieDefenseDrawPose {
        zombie_defense_draw_pose(state).expect("draw pose")
    }

    #[test]
    fn exact_default_altitude_and_clip_match_get_zombie_rect_order() {
        let state = exact_state(ZombieKind::Normal).with_altitude(12.8);
        assert_eq!(
            draw(state),
            ZombieDefenseDrawPose {
                body_y: -12.8,
                clip_height: CLIP_HEIGHT_OFF
            }
        );
        assert_eq!(
            zombie_defense_geometry_from_state(state).expect("bounds").rect,
            ContactRect::new(110, 192, 130, 232)
        );
    }

    #[test]
    fn exact_rising_from_grave_pool_non_pool_and_high_ground() {
        let mut state = exact_state(ZombieKind::Normal)
            .with_phase(ZombieDefensePhase::RisingFromGrave)
            .with_altitude(25.0);
        state.phase_counter = 30;
        assert_eq!(
            draw(state),
            ZombieDefenseDrawPose {
                body_y: -25.0,
                clip_height: 5.0
            }
        );
        assert_eq!(
            zombie_defense_geometry_from_state(state).expect("bounds").rect.bottom,
            215
        );

        let pooled = state.with_pool_state(true);
        assert_eq!(draw(pooled).clip_height, -25.0);

        let high = state.with_high_ground(true);
        assert_eq!(draw(high).body_y, -55.0);
    }

    #[test]
    fn exact_dolphin_draw_pose_branches() {
        let mut state = exact_state(ZombieKind::DolphinRider).with_altitude(20.0);
        state.phase = ZombieDefensePhase::DolphinIntoPool;
        assert_eq!(
            zombie_defense_draw_pose(state),
            Err(ZombieGeometryError::MissingBodyReanimationTime)
        );
        assert_eq!(
            draw(state.with_body_reanim_anim_time(0.55)).clip_height,
            CLIP_HEIGHT_OFF
        );
        assert_eq!(draw(state.with_body_reanim_anim_time(0.56)).clip_height, 0.0);
        assert_eq!(draw(state.with_body_reanim_anim_time(0.65)).clip_height, 0.0);
        assert_eq!(draw(state.with_body_reanim_anim_time(0.75)).clip_height, -30.0);

        state.phase = ZombieDefensePhase::DolphinRiding;
        assert_eq!(draw(state).clip_height, -30.0);
        state.zombie_height = ZombieHeightState::DraggedUnder;
        assert_eq!(draw(state).clip_height, -35.0);

        state.phase = ZombieDefensePhase::DolphinInJump;
        assert_eq!(draw(state.with_body_reanim_anim_time(0.06)).clip_height, -30.0);
        assert_eq!(draw(state.with_body_reanim_anim_time(0.5)).clip_height, -13.0);
        assert_eq!(
            draw(state.with_body_reanim_anim_time(0.77)).clip_height,
            CLIP_HEIGHT_OFF
        );

        state.phase = ZombieDefensePhase::DolphinWalkingInPool;
        assert_eq!(draw(state).clip_height, 16.0);
        state.phase = ZombieDefensePhase::ZombieDying;
        assert_eq!(draw(state).clip_height, 24.0);
        state.zombie_height = ZombieHeightState::OutOfPool;
        state.phase = ZombieDefensePhase::DolphinWalking;
        assert_eq!(draw(state).clip_height, -20.0);
        state.phase = ZombieDefensePhase::DolphinWalkingWithoutDolphin;
        assert_eq!(draw(state).clip_height, -20.0);
    }

    #[test]
    fn exact_snorkel_pool_and_general_pool_branches() {
        let mut snorkel = exact_state(ZombieKind::Snorkel).with_altitude(12.0);
        snorkel.phase = ZombieDefensePhase::SnorkelIntoPool;
        assert_eq!(
            draw(snorkel.with_body_reanim_anim_time(0.79)).clip_height,
            CLIP_HEIGHT_OFF
        );
        assert_eq!(draw(snorkel.with_body_reanim_anim_time(0.8)).clip_height, -10.0);
        snorkel.phase = ZombieDefensePhase::Other;
        snorkel.in_pool = true;
        snorkel.scale_zombie = 0.5;
        assert_eq!(draw(snorkel).clip_height, -7.0);

        let mut normal = exact_state(ZombieKind::Normal)
            .with_altitude(12.0)
            .with_pool_state(true);
        normal.scale_zombie = 0.5;
        assert_eq!(draw(normal).clip_height, -14.0);
        normal.is_eating = true;
        assert_eq!(draw(normal).clip_height, -7.0);
    }

    #[test]
    fn exact_dancer_digger_bungee_and_walking_backwards_branches() {
        let dancer = exact_state(ZombieKind::Dancing)
            .with_phase(ZombieDefensePhase::DancerRising)
            .with_altitude(10.0)
            .with_high_ground(true);
        assert_eq!(
            draw(dancer),
            ZombieDefenseDrawPose {
                body_y: -40.0,
                clip_height: -10.0
            }
        );

        let mut digger = exact_state(ZombieKind::Digger)
            .with_phase(ZombieDefensePhase::DiggerRising)
            .with_altitude(10.0);
        digger.phase_counter = 20;
        assert_eq!(draw(digger).clip_height, CLIP_HEIGHT_OFF);
        digger.phase_counter = 21;
        assert_eq!(draw(digger).clip_height, -10.0);
        digger.phase = ZombieDefensePhase::DiggerRiseWithoutAxe;
        assert_eq!(draw(digger).clip_height, -10.0);

        let bungee = exact_state(ZombieKind::Bungee)
            .with_altitude(10.0)
            .with_high_ground(true);
        assert_eq!(
            draw(bungee),
            ZombieDefenseDrawPose {
                body_y: -40.0,
                clip_height: CLIP_HEIGHT_OFF
            }
        );

        let mirrored =
            zombie_defense_geometry_from_state(exact_state(ZombieKind::Normal).with_phase(ZombieDefensePhase::Other))
                .expect("normal");
        assert_eq!(mirrored.rect.left, 110);

        let mut backwards = exact_state(ZombieKind::Normal);
        backwards.mind_controlled = true;
        assert_eq!(
            zombie_defense_geometry_from_state(backwards)
                .expect("mind controlled")
                .rect
                .left,
            150
        );

        let mut aquarium = exact_state(ZombieKind::Normal);
        aquarium.zombie_height = ZombieHeightState::Zombiquarium;
        aquarium.vel_z = 1.0;
        assert_eq!(
            zombie_defense_geometry_from_state(aquarium)
                .expect("zombiquarium")
                .rect
                .left,
            150
        );
        aquarium.vel_z = 3.0;
        assert_eq!(
            zombie_defense_geometry_from_state(aquarium)
                .expect("zombiquarium")
                .rect
                .left,
            110
        );
        aquarium.vel_z = 5.0;
        assert_eq!(
            zombie_defense_geometry_from_state(aquarium)
                .expect("zombiquarium")
                .rect
                .left,
            150
        );

        let mut digger_backwards = exact_state(ZombieKind::Digger).with_phase(ZombieDefensePhase::DiggerWalking);
        assert_eq!(
            zombie_defense_geometry_from_state(digger_backwards)
                .expect("digger walking")
                .rect
                .left,
            150
        );
        digger_backwards.phase = ZombieDefensePhase::DiggerStunned;
        assert_eq!(
            zombie_defense_geometry_from_state(digger_backwards)
                .expect("digger stunned")
                .rect
                .left,
            150
        );
        digger_backwards.phase = ZombieDefensePhase::ZombieBurned;
        digger_backwards.has_object = false;
        assert_eq!(
            zombie_defense_geometry_from_state(digger_backwards)
                .expect("burned no object")
                .rect
                .left,
            110
        );
        digger_backwards.has_object = true;
        assert_eq!(
            zombie_defense_geometry_from_state(digger_backwards)
                .expect("burned object")
                .rect
                .left,
            150
        );
        digger_backwards.phase = ZombieDefensePhase::ZombieMowered;
        assert_eq!(
            zombie_defense_geometry_from_state(digger_backwards)
                .expect("mowered object")
                .rect
                .left,
            150
        );

        let mut yeti = exact_state(ZombieKind::Yeti);
        assert_eq!(
            zombie_defense_geometry_from_state(yeti)
                .expect("yeti no object")
                .rect
                .left,
            150
        );
        yeti.has_object = true;
        assert_eq!(
            zombie_defense_geometry_from_state(yeti).expect("yeti object").rect.left,
            110
        );
    }

    #[test]
    fn exact_clip_boundaries_truncation_and_invalid_inputs() {
        let mut state = exact_state(ZombieKind::DolphinRider).with_altitude(0.0);
        state.phase = ZombieDefensePhase::DolphinInJump;
        assert_eq!(
            zombie_defense_geometry_from_state(state.with_body_reanim_anim_time(0.5))
                .expect("clip -13")
                .rect,
            ContactRect::new(110, 205, 130, 258)
        );

        let mut rising = exact_state(ZombieKind::Normal)
            .with_phase(ZombieDefensePhase::RisingFromGrave)
            .with_altitude(100.0);
        rising.phase_counter = 0;
        assert_eq!(draw(rising).clip_height, -100.0);
        assert_eq!(
            zombie_defense_geometry_from_state(rising)
                .expect("no boundary clip")
                .rect
                .bottom,
            145
        );

        let mut trunc = exact_state(ZombieKind::Normal)
            .with_altitude(-2.9)
            .with_pool_state(true);
        trunc.scale_zombie = 1.0;
        assert_eq!(
            zombie_defense_geometry_from_state(trunc)
                .expect("positive body trunc")
                .rect
                .top,
            207
        );
        trunc.altitude = 2.9;
        assert_eq!(
            zombie_defense_geometry_from_state(trunc)
                .expect("negative body trunc")
                .rect
                .top,
            202
        );

        assert_eq!(
            ZombieDefenseState::from_base_rect(ZombieKind::Normal, 0, 0, 0, 0, RelativeContactRect::new(0, 0, 1, 1),),
            Err(ZombieGeometryError::InvalidBodyWidth)
        );
        assert_eq!(
            ZombieDefenseState::from_base_rect(ZombieKind::Normal, 0, 0, 0, 80, RelativeContactRect::new(0, 0, 0, 1),),
            Err(ZombieGeometryError::InvalidRelativeRect)
        );
        assert_eq!(
            zombie_defense_geometry_from_state(exact_state(ZombieKind::Normal).with_altitude(f32::NAN)),
            Err(ZombieGeometryError::InvalidCoordinate)
        );
        assert_eq!(
            zombie_defense_geometry_from_state(exact_state(ZombieKind::Normal).with_altitude(f32::INFINITY)),
            Err(ZombieGeometryError::InvalidCoordinate)
        );
    }
}
