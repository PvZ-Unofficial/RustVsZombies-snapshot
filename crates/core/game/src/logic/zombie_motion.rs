//! Stable-motion conditional zombie x-coordinate prediction.
//!
//! These APIs predict vanilla movement only while the caller knows the current motion profile,
//! direction, and speed rules remain stable. They do not prove that the requested horizon is safe
//! from future phase/action/field interactions.

mod classify;
mod facts;
mod reanim;

#[cfg(test)]
mod classify_tests;

use crate::backend::{ZombieRawFactsBackend, ZombieReadBackend};
use rsvz_model::{
    UniformChillPolicy, UniformZombieMotion, ZombieMotionDirection, ZombieMotionError, ZombieMotionState,
    ZombieMovementModel, ZombieTrackProfile,
};

use self::classify::classify_vanilla_zombie_motion;
use self::facts::RawZombieMotionFacts;
use rsvz_model::{UnsupportedZombieMotionReason, ZombieId, ZombieMotionCallError, ZombieMotionCounters};

pub const MAX_ZOMBIE_MOTION_HORIZON: u32 = 100_000;
const SECONDS_PER_UPDATE: f32 = 0.01;

/// Composes a motion state from current borrowed Z01-Z08 scalar facts.
pub fn zombie_motion_state(id: ZombieId) -> Option<ZombieMotionState>
where
    rsvz_current::CurrentBackend: ZombieRawFactsBackend,
{
    crate::live_value::read_or_abort(
        rsvz_current::with_backend_shared(|access| -> crate::runtime::RuntimeResult<_> {
            let Some(zombie) = crate::live_value::read_or_abort(access.zombie(id), "zombie") else {
                return Ok(None);
            };
            Ok(zombie_motion_state_from_handle(&access, id, zombie)?)
        })
        .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))
        .and_then(|result| result),
        "failed to read zombie motion",
    )
}

/// Samples the supplied entity borrow without resolving its ID a second time.
pub(crate) fn zombie_motion_state_from_handle<'a>(
    access: &'a rsvz_current::CurrentBackend, id: ZombieId,
    zombie: <rsvz_current::CurrentBackend as rsvz_backend_api::ZombieReadBackend>::ZombieHandle<'a>,
) -> Result<Option<ZombieMotionState>, rsvz_current::CurrentBackendError>
where
    rsvz_current::CurrentBackend: ZombieRawFactsBackend,
{
    let backend = access;
    if !backend.zombie_is_alive(zombie) {
        return Ok(None);
    }
    let kind = crate::live_value::read_or_abort(backend.zombie_kind(zombie), "zombie_kind");
    let phase = crate::live_value::read_or_abort(backend.zombie_phase(zombie), "zombie_phase");
    compose_zombie_motion_state(access, id, zombie, kind, phase).map(Some)
}

/// Composes stable motion for a currently running, unopened jack without
/// reading its hidden pre-pop phase counter.
///
/// Returns `None` when the ID is absent, dead, not a jack, or has already
/// left `JackInTheBoxRunning`. Callers must re-handle the latter as a visible
/// state change rather than falling back to [`zombie_motion_state`].
pub fn unopened_jack_motion_state(id: ZombieId) -> Option<ZombieMotionState>
where
    rsvz_current::CurrentBackend: ZombieRawFactsBackend,
{
    crate::live_value::read_or_abort(
        rsvz_current::with_backend_shared(|access| -> crate::runtime::RuntimeResult<_> {
            let backend = access;

            let Some(zombie) = crate::live_value::read_or_abort(access.zombie(id), "zombie") else {
                return Ok(None);
            };
            if !backend.zombie_is_alive(zombie) {
                return Ok(None);
            }
            let kind = crate::live_value::read_or_abort(backend.zombie_kind(zombie), "zombie_kind");
            let phase = crate::live_value::read_or_abort(backend.zombie_phase(zombie), "zombie_phase");
            if kind != rsvz_model::ZombieKind::JackInTheBox || phase != rsvz_model::ZombiePhase::JackInTheBoxRunning {
                return Ok(None);
            }
            Ok(compose_zombie_motion_state(&access, id, zombie, kind, phase).map(Some)?)
        })
        .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))
        .and_then(|result| result),
        "failed to read zombie motion",
    )
}

fn compose_zombie_motion_state<'a>(
    access: &'a rsvz_current::CurrentBackend, id: ZombieId,
    zombie: <rsvz_current::CurrentBackend as rsvz_backend_api::ZombieReadBackend>::ZombieHandle<'a>,
    kind: rsvz_model::ZombieKind, phase: rsvz_model::ZombiePhase,
) -> Result<ZombieMotionState, rsvz_current::CurrentBackendError>
where
    rsvz_current::CurrentBackend: ZombieRawFactsBackend,
{
    let backend = access;
    let reanimation = crate::zombie::reanimation_from_handle(access, zombie)?;
    let speed_x = backend.zombie_speed_x(zombie);
    let scale = backend.zombie_scale(zombie);
    let frozen_countdown = backend.zombie_frozen_countdown(zombie);
    let chilled_countdown = backend.zombie_chilled_countdown(zombie);
    let buttered_countdown = backend.zombie_buttered_countdown(zombie);
    let model = reanimation.map_or(
        ZombieMovementModel::Unsupported(UnsupportedZombieMotionReason::MissingReanimation),
        |reanimation| {
            classify_vanilla_zombie_motion(RawZombieMotionFacts {
                kind,
                phase,
                zombie_height: backend.zombie_height_state(zombie),
                speed_x,
                scale,
                reanimation,
                is_eating: backend.zombie_is_eating(zombie),
                mind_controlled: backend.zombie_is_mind_controlled(zombie),
                blowing_away: backend.zombie_is_blown_away(zombie),
                flat_tires: backend.zombie_has_flat_tires(zombie),
                has_head: backend.zombie_has_head(zombie),
                has_object: backend.zombie_has_object(zombie),
                in_pool: backend.zombie_is_in_pool(zombie),
                yucky_face: backend.zombie_has_yucky_face(zombie),
                frozen_countdown,
                chilled_countdown,
                buttered_countdown,
            })
        },
    );
    Ok(ZombieMotionState {
        id,
        kind,
        row: backend.zombie_row(zombie),
        x: backend.zombie_pos_x(zombie),
        speed_x,
        scale,
        counters: ZombieMotionCounters {
            frozen: frozen_countdown,
            chilled: chilled_countdown,
            buttered: buttered_countdown,
        },
        model,
    })
}

pub trait ZombieMotionRules {
    fn track_frames(&self, profile: ZombieTrackProfile) -> Option<&[f32]>;

    fn uniform_chilled_factor(&self) -> f32 {
        0.4
    }

    fn track_chilled_factor(&self) -> f32 {
        0.5
    }

    fn tick_counters_before_move(&self) -> bool {
        true
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct VanillaZombieMotionRules;

// Vanilla PvZ 1.0.0.1051 `_ground` track x samples rounded to one decimal place from the
// decompilation resource XML files under `SexyAppFramework/reanim/*.reanim`. These are the track
// key-frame x values consumed by `Reanimation::GetTrackVelocity`; AvZLib was used only as a
// non-authoritative comparison while keeping the reusable vanilla data in core.
const NORMAL_WALK_A: &[f32] = &[
    -9.8, -8.4, -7.0, -5.6, -4.1, -2.7, -1.3, 0.0, 1.4, 2.8, 4.2, 5.7, 7.1, 7.9, 8.8, 9.7, 10.5, 10.6, 10.8, 10.9,
    11.0, 11.0, 11.0, 11.0, 11.0, 13.4, 15.8, 18.1, 20.5, 22.8, 25.2, 27.6, 29.9, 31.1, 32.3, 33.5, 34.6, 35.9, 37.0,
    38.2, 39.4, 39.5, 39.6, 39.7, 39.8, 39.9, 40.0,
];
const NORMAL_WALK_B: &[f32] = &[
    -9.8, -8.5, -7.3, -6.0, -4.7, -3.4, -2.1, -0.9, 0.3, 1.6, 2.8, 4.1, 5.4, 6.7, 8.0, 9.2, 10.5, 10.6, 10.7, 10.7,
    10.8, 10.8, 10.9, 11.0, 12.8, 14.5, 16.3, 18.1, 19.9, 21.6, 23.4, 25.2, 27.0, 28.8, 30.5, 32.3, 34.0, 35.9, 37.6,
    39.4, 39.5, 39.5, 39.6, 39.8, 39.9, 39.9, 40.0,
];
const NORMAL_SWIM: &[f32] = &[
    -9.8, -8.4, -7.0, -5.6, -4.1, -2.7, -1.3, 0.0, 1.4, 2.8, 4.2, 5.7, 7.1, 7.9, 8.8, 9.7, 10.5, 10.6, 10.8, 10.9,
    11.0, 11.0, 11.0, 11.0, 11.0, 13.4, 15.8, 18.1, 20.5, 22.8, 25.2, 27.6, 29.9, 31.1, 32.3, 33.5, 34.6, 35.9, 37.0,
    38.2, 39.4, 40.0,
];
const NORMAL_DANCE: &[f32] = &[
    -9.8, -9.4, -8.9, -8.4, -7.9, -7.5, -7.0, -6.5, -6.1, -5.6, -5.1, -4.7, -4.2, -3.7, -3.3, -2.8, -2.3, -1.8, -1.4,
    -0.9, -0.4, 0.0, 0.3, 0.8, 1.3, 1.8, 2.2, 2.6, 3.1, 3.6, 4.1, 4.6, 5.0, 5.5, 6.0, 6.5, 6.9, 7.3, 7.8, 8.3, 8.8,
    9.3, 9.7, 10.2, 10.7, 11.1, 11.6, 12.1, 12.6, 13.0,
];
const POLE_VAULT_BEFORE_JUMP: &[f32] = &[
    -59.8, -59.0, -58.2, -57.3, -56.5, -55.7, -54.8, -54.0, -53.2, -52.3, -51.5, -50.7, -49.8, -49.0, -48.2, -47.3,
    -46.5, -45.7, -44.8, -44.0, -43.2, -42.3, -41.5, -40.7, -39.8, -39.0, -38.2, -37.3, -36.5, -35.7, -34.8, -34.0,
    -33.2, -32.3, -31.5, -30.7, -29.8,
];
const POLE_VAULT_AFTER_JUMP: &[f32] = &[
    -59.8, -59.1, -58.3, -57.6, -56.9, -55.0, -53.0, -51.1, -49.2, -47.2, -45.1, -43.1, -41.1, -39.0, -37.0, -35.0,
    -32.9, -30.9, -28.9, -27.0, -25.0, -23.0, -21.0, -19.0, -17.0, -14.7, -12.3, -10.0, -7.7, -5.8, -3.9, -2.0, -0.1,
    1.6, 3.5, 5.4, 7.3, 7.5, 7.8, 8.0, 8.2, 8.2, 8.1, 8.0, 8.0,
];
const NEWSPAPER_WALK: &[f32] = &[
    -59.8, -59.3, -58.8, -58.3, -57.8, -54.8, -51.8, -48.9, -45.9, -43.8, -41.6, -39.4, -37.3, -35.1, -33.8, -32.5,
    -31.2, -29.8, -28.5, -28.5, -28.5, -28.6, -28.6, -27.6, -26.6, -25.7, -24.7, -23.7, -21.6, -19.4, -17.2, -15.1,
    -12.9, -11.3, -9.7, -8.1, -6.5, -4.9, -4.2, -3.5, -2.7, -2.0, -2.0, -2.0, -2.0, -2.0, -2.0,
];
const FOOTBALL_WALK: &[f32] = &[
    -59.8, -57.4, -55.0, -52.6, -50.2, -47.8, -47.5, -47.3, -47.0, -46.7, -46.4, -46.1, -45.8, -44.3, -42.8, -41.3,
    -39.8, -38.3, -36.8, -35.3, -33.8, -33.5, -33.3, -33.0, -32.7, -32.4, -32.1, -31.8, -30.8, -29.8,
];
const JACK_BOX_WALK: &[f32] = &[
    -49.8, -49.8, -49.2, -48.6, -47.9, -44.3, -40.7, -39.7, -38.7, -38.7, -37.8, -36.8, -35.8, -34.0, -32.1, -32.1,
    -32.1, -30.5, -28.9,
];
const BALLOON_WALK: &[f32] = &[
    -9.8, -8.3, -6.8, -5.4, -3.9, -2.6, -1.4, -0.1, 1.0, 2.0, 3.2, 4.3, 5.4, 7.3, 9.2, 11.1, 13.0, 13.0, 13.1, 13.1,
    13.1, 14.8, 16.5, 18.2, 20.0, 21.7, 23.4, 25.1, 26.8, 27.7, 28.5, 29.3, 30.1, 32.6, 35.1, 37.5, 40.0, 40.0, 40.1,
    40.1, 40.1,
];
const DIGGER_WALK: &[f32] = &[
    -59.8, -58.1, -56.4, -54.6, -52.9, -50.0, -47.1, -44.2, -41.3, -39.8, -38.3, -36.7, -35.2, -32.6, -30.0, -27.5,
    -24.9, -21.8, -18.8, -15.7, -12.7, -10.7, -8.8, -6.8, -4.9, -3.8, -2.8, -1.7, -0.7, 0.9, 2.6, 4.4, 6.1, 7.8, 9.4,
    11.0, 12.7,
];
const YETI_WALK: &[f32] = &[
    -103.8, -97.5, -91.2, -86.5, -81.9, -77.2, -72.6, -69.1, -65.6, -62.2, -58.8, -55.2, -51.6, -48.0, -44.4, -40.8,
    -37.2, -35.8, -34.4, -32.5, -30.6, -28.7, -26.8, -26.8, -26.8, -22.5, -18.2, -13.9, -9.6, -5.3, -1.0, 3.1, 7.5,
    11.8,
];
const LADDER_WALK: &[f32] = &[
    -39.8, -39.0, -38.1, -37.3, -36.5, -33.6, -30.7, -27.9, -25.0, -21.1, -17.2, -13.2, -9.3, -5.4, -4.6, -3.9, -3.1,
    -2.4, -1.6, -1.6, -1.6, -1.6, -1.6, -0.6, 0.2, 1.2, 2.1, 3.0, 5.2, 7.3, 9.6, 11.7, 13.9, 15.9, 18.0, 20.1, 22.2,
    24.2, 24.7, 25.2, 25.6, 26.1, 26.1, 26.1, 26.1, 26.1, 26.1,
];
const GARGANTUAR_WALK: &[f32] = &[
    -79.8, -75.3, -70.8, -66.3, -61.9, -57.4, -54.5, -51.5, -48.5, -45.6, -42.6, -38.2, -33.8, -29.4, -25.0, -24.6,
    -24.1, -23.7, -23.2, -22.8, -21.1, -19.4, -17.7, -16.0, -14.3, -11.9, -9.5, -7.1, -4.7, -2.3, 3.0, 8.4, 13.8, 19.2,
    24.6, 29.1, 33.5, 38.0, 42.5, 42.5, 42.5, 42.5, 42.5, 42.5, 42.9, 43.3, 43.5, 43.9, 44.3,
];
const IMP_WALK: &[f32] = &[
    -59.8, -56.5, -53.2, -49.9, -47.8, -45.7, -43.5, -40.3, -37.1, -33.9, -33.0, -32.2, -31.3, -30.4, -28.7, -26.9,
    -25.2, -21.3, -17.4, -13.5, -12.3, -11.2, -10.0, -7.8, -5.5, -3.3, -2.2, -1.1, -0.1, 0.9, 1.6, 2.4, 3.1,
];

impl ZombieMotionRules for VanillaZombieMotionRules {
    fn track_frames(&self, profile: ZombieTrackProfile) -> Option<&[f32]> {
        Some(vanilla_track_frames(profile))
    }
}

#[must_use]
pub fn vanilla_track_frames(profile: ZombieTrackProfile) -> &'static [f32] {
    match profile {
        ZombieTrackProfile::NormalWalkA | ZombieTrackProfile::PogoWalk => NORMAL_WALK_A,
        ZombieTrackProfile::NormalWalkB => NORMAL_WALK_B,
        ZombieTrackProfile::NormalSwim => NORMAL_SWIM,
        ZombieTrackProfile::NormalDance => NORMAL_DANCE,
        ZombieTrackProfile::PoleVaultBeforeJump => POLE_VAULT_BEFORE_JUMP,
        ZombieTrackProfile::PoleVaultAfterJump => POLE_VAULT_AFTER_JUMP,
        ZombieTrackProfile::NewspaperWalk => NEWSPAPER_WALK,
        ZombieTrackProfile::FootballWalk => FOOTBALL_WALK,
        ZombieTrackProfile::JackBoxWalk => JACK_BOX_WALK,
        ZombieTrackProfile::BalloonWalk => BALLOON_WALK,
        ZombieTrackProfile::DiggerWalk => DIGGER_WALK,
        ZombieTrackProfile::YetiWalk => YETI_WALK,
        ZombieTrackProfile::LadderWalk => LADDER_WALK,
        ZombieTrackProfile::GargantuarWalk => GARGANTUAR_WALK,
        ZombieTrackProfile::ImpWalk => IMP_WALK,
    }
}

/// Predicts a stable-motion trace with vanilla rules.
///
/// `trace[0]` is the sampled x. `trace[1]` is the position after one vanilla zombie update.
/// This is conditional prediction and does not prove that future phase/action/field interactions
/// keep the zombie in the same motion profile.
pub fn predict_stable_zombie_x_trace(
    state: &ZombieMotionState, horizon_frames: u32,
) -> Result<Vec<f32>, ZombieMotionError> {
    predict_stable_zombie_x_trace_with_rules(state, horizon_frames, &VanillaZombieMotionRules)
}

/// Predicts a stable-motion trace with custom rules.
///
/// This is conditional prediction and does not prove that future phase/action/field interactions
/// keep the zombie in the same motion profile.
pub fn predict_stable_zombie_x_trace_with_rules<R: ZombieMotionRules>(
    state: &ZombieMotionState, horizon_frames: u32, rules: &R,
) -> Result<Vec<f32>, ZombieMotionError> {
    state.validate()?;
    let len = trace_len(horizon_frames)?;
    let mut xs = Vec::with_capacity(len);
    let _last = simulate_stable_motion(state, horizon_frames, rules, Some(&mut xs))?;
    Ok(xs)
}

/// Predicts one future x value with vanilla rules without allocating a trace.
///
/// This is conditional prediction and does not prove that future phase/action/field interactions
/// keep the zombie in the same motion profile.
pub fn predict_stable_zombie_x_at(state: &ZombieMotionState, after_frames: u32) -> Result<f32, ZombieMotionError> {
    predict_stable_zombie_x_at_with_rules(state, after_frames, &VanillaZombieMotionRules)
}

/// Predicts one future x value with custom rules without allocating a trace.
///
/// This is conditional prediction and does not prove that future phase/action/field interactions
/// keep the zombie in the same motion profile.
pub fn predict_stable_zombie_x_at_with_rules<R: ZombieMotionRules>(
    state: &ZombieMotionState, after_frames: u32, rules: &R,
) -> Result<f32, ZombieMotionError> {
    state.validate()?;
    let _len = trace_len(after_frames)?;
    simulate_stable_motion(state, after_frames, rules, None)
}

fn trace_len(horizon_frames: u32) -> Result<usize, ZombieMotionError> {
    if horizon_frames > MAX_ZOMBIE_MOTION_HORIZON {
        return Err(ZombieMotionError::InvalidHorizon);
    }
    Ok(horizon_frames as usize + 1)
}

fn simulate_stable_motion<R: ZombieMotionRules>(
    state: &ZombieMotionState, horizon_frames: u32, rules: &R, mut xs: Option<&mut Vec<f32>>,
) -> Result<f32, ZombieMotionError> {
    let mut x = state.x;
    let mut counters = state.counters;
    let mut progress = state.model.initial_track_progress()?;

    if let Some(trace) = xs.as_deref_mut() {
        trace.push(x);
    }

    for _ in 0..horizon_frames {
        let tick_counters_before_move = rules.tick_counters_before_move();
        if tick_counters_before_move {
            counters.tick_down();
        }

        if !counters.is_immobilized() {
            let (dx, direction) = step_motion(
                x,
                &mut progress,
                state.speed_x,
                state.scale,
                counters.is_chilled(),
                state.model,
                rules,
            )?;
            apply_direction(&mut x, dx, direction);
            if !x.is_finite() {
                return Err(ZombieMotionError::InvalidMotionState);
            }
        }

        if !tick_counters_before_move {
            counters.tick_down();
        }

        if let Some(trace) = xs.as_deref_mut() {
            trace.push(x);
        }
    }

    Ok(x)
}

fn step_motion<R: ZombieMotionRules>(
    current_x: f32, progress: &mut Option<f32>, speed_x: f32, scale: f32, chilled: bool, model: ZombieMovementModel,
    rules: &R,
) -> Result<(f32, ZombieMotionDirection), ZombieMotionError> {
    match model {
        ZombieMovementModel::Track {
            profile,
            playback_rate_fps,
            direction,
            ..
        } => Ok((
            step_track_motion(progress, playback_rate_fps, scale, chilled, profile, rules)?,
            direction,
        )),
        ZombieMovementModel::Uniform { motion, direction } => Ok((
            step_uniform_motion(current_x, speed_x, chilled, motion, rules)?,
            direction,
        )),
        ZombieMovementModel::Unsupported(reason) => Err(ZombieMotionError::Unsupported(reason)),
    }
}

fn step_track_motion<R: ZombieMotionRules>(
    progress: &mut Option<f32>, playback_rate_fps: f32, scale: f32, chilled: bool, profile: ZombieTrackProfile,
    rules: &R,
) -> Result<f32, ZombieMotionError> {
    let progress = progress.as_mut().ok_or(ZombieMotionError::InvalidAnimationProgress)?;
    let frames = rules
        .track_frames(profile)
        .ok_or(ZombieMotionError::InvalidTrackProfile)?;
    if frames.len() < 2 || !frames.iter().all(|value| value.is_finite()) {
        return Err(ZombieMotionError::InvalidTrackProfile);
    }
    let first = *frames.first().ok_or(ZombieMotionError::InvalidTrackProfile)?;
    let last = *frames.last().ok_or(ZombieMotionError::InvalidTrackProfile)?;
    let total = last - first;
    if !total.is_finite() || total <= 0.0 {
        return Err(ZombieMotionError::InvalidTrackProfile);
    }
    let frame_count = frames.len();
    let frame_count_f = frame_count as f32;
    let index = (*progress * (frame_count_f - 1.0) + 1.0) as usize;
    if index == 0 || index >= frame_count {
        return Err(ZombieMotionError::InvalidAnimationProgress);
    }
    let current = *frames.get(index).ok_or(ZombieMotionError::InvalidAnimationProgress)?;
    let previous = *frames
        .get(index - 1)
        .ok_or(ZombieMotionError::InvalidAnimationProgress)?;
    let segment_delta = current - previous;
    if !segment_delta.is_finite() {
        return Err(ZombieMotionError::InvalidTrackProfile);
    }

    let mut effective_playback_rate_fps = playback_rate_fps;
    if chilled {
        let factor = rules.track_chilled_factor();
        validate_factor(factor)?;
        effective_playback_rate_fps *= factor;
    }
    let dx = segment_delta * SECONDS_PER_UPDATE * effective_playback_rate_fps * scale;
    let progress_step = effective_playback_rate_fps * SECONDS_PER_UPDATE / frame_count_f;
    if !dx.is_finite() || !progress_step.is_finite() {
        return Err(ZombieMotionError::InvalidMotionState);
    }
    *progress = (*progress + progress_step).rem_euclid(1.0);
    if !progress.is_finite() || !(0.0..1.0).contains(progress) {
        return Err(ZombieMotionError::InvalidAnimationProgress);
    }
    Ok(dx)
}

fn step_uniform_motion<R: ZombieMotionRules>(
    current_x: f32, base_speed: f32, chilled: bool, uniform: UniformZombieMotion, rules: &R,
) -> Result<f32, ZombieMotionError> {
    let mut dx = match uniform {
        UniformZombieMotion::Zomboni => zomboni_speed_for_x(current_x)?,
        UniformZombieMotion::DiggerTunneling => base_speed,
        UniformZombieMotion::BalloonFlying
        | UniformZombieMotion::PogoBouncing
        | UniformZombieMotion::CatapultDriving => base_speed,
    };

    if chilled && uniform.chill_policy() == UniformChillPolicy::AffectedByChill {
        let factor = rules.uniform_chilled_factor();
        validate_factor(factor)?;
        dx *= factor;
    }
    if dx.is_finite() {
        Ok(dx)
    } else {
        Err(ZombieMotionError::InvalidMotionState)
    }
}

fn zomboni_speed_for_x(current_x: f32) -> Result<f32, ZombieMotionError> {
    if !current_x.is_finite() || current_x < i32::MIN as f32 || current_x > i32::MAX as f32 {
        return Err(ZombieMotionError::InvalidMotionState);
    }

    let x = current_x as i32;
    Ok(if x >= 700 {
        0.25
    } else if x >= 400 {
        0.25 - 0.0005 * (700 - x) as f32
    } else {
        0.1
    })
}

fn validate_factor(factor: f32) -> Result<(), ZombieMotionError> {
    if factor.is_finite() && factor >= 0.0 {
        Ok(())
    } else {
        Err(ZombieMotionError::InvalidMotionRules)
    }
}

fn apply_direction(x: &mut f32, dx: f32, direction: ZombieMotionDirection) {
    match direction {
        ZombieMotionDirection::Left => *x -= dx,
        ZombieMotionDirection::Right => *x += dx,
    }
}

/// Samples a current zombie and predicts a stable-motion trace with vanilla rules.
///
/// This is conditional prediction and does not prove that future phase/action/field interactions
/// keep the zombie in the same motion profile.
pub fn zombie_stable_x_trace(id: ZombieId, horizon_frames: u32) -> Result<Vec<f32>, ZombieMotionCallError>
where
    rsvz_current::CurrentBackend: ZombieRawFactsBackend,
{
    let Some(state) = zombie_motion_state(id) else {
        return Err(ZombieMotionCallError::ObjectUnavailable);
    };

    predict_stable_zombie_x_trace(&state, horizon_frames).map_err(ZombieMotionCallError::Motion)
}

#[cfg(test)]
mod tests;
