use rsvz_model::model::{ZombieReanimationFacts, ZombieTrackProfile};

use super::facts::{PVZ_REANIM_BASE_FPS, REANIM_LOOP, RawZombieMotionFacts, TRACK_CHILLED_FACTOR};
use super::vanilla_track_frames;

pub(super) fn reanimation_is_valid(reanimation: ZombieReanimationFacts) -> bool {
    reanimation.anim_time.is_finite()
        && reanimation.rate.is_finite()
        && reanimation.rate >= 0.0
        && reanimation.frame_count > 0
}

pub(super) fn sample_reanimation_progress(reanimation: ZombieReanimationFacts) -> Option<f32> {
    if reanimation.loop_type != REANIM_LOOP {
        return None;
    }
    if (0.0..1.0).contains(&reanimation.anim_time) {
        Some(reanimation.anim_time)
    } else {
        None
    }
}

pub(super) fn reanimation_time_is_bounded(raw_anim_time: f32) -> bool {
    (0.0..=1.0).contains(&raw_anim_time)
}

pub(super) fn track_profile_reanimation_state(
    profile: ZombieTrackProfile, facts: RawZombieMotionFacts,
) -> Option<(f32, f32)> {
    let progress = sample_reanimation_progress(facts.reanimation)?;
    let frames = vanilla_track_frames(profile);
    if frames.len() as i32 != facts.reanimation.frame_count {
        return None;
    }
    let playback_rate_fps = track_playback_rate_fps(frames, facts)?;
    Some((progress, playback_rate_fps))
}

fn track_playback_rate_fps(frames: &[f32], facts: RawZombieMotionFacts) -> Option<f32> {
    let raw_rate = facts.reanimation.rate;
    let playback_rate_fps = if raw_rate > 0.0 {
        if facts.chilled_countdown > 0 {
            raw_rate / TRACK_CHILLED_FACTOR
        } else {
            raw_rate
        }
    } else if facts.frozen_countdown > 0 || facts.buttered_countdown > 0 {
        recompute_track_playback_rate_fps(frames, facts)?
    } else {
        return None;
    };
    if playback_rate_fps.is_finite() && playback_rate_fps >= 0.0 {
        Some(playback_rate_fps)
    } else {
        None
    }
}

fn recompute_track_playback_rate_fps(frames: &[f32], facts: RawZombieMotionFacts) -> Option<f32> {
    let distance = frames.last()? - frames.first()?;
    if !distance.is_finite() || distance < 1e-6 {
        return None;
    }
    let frame_count = frames.len() as f32;
    let one_over_speed = frame_count / distance;
    let playback_rate_fps = facts.speed_x * one_over_speed * PVZ_REANIM_BASE_FPS / facts.scale;
    if playback_rate_fps.is_finite() && playback_rate_fps >= 0.0 {
        Some(playback_rate_fps)
    } else {
        None
    }
}
