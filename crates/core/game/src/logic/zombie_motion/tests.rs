use super::*;
use rsvz_model::{UnsupportedZombieMotionReason, ZombieId, ZombieKind, ZombieMotionCounters};

#[derive(Clone, Copy)]
struct TestRules {
    frames: &'static [f32],
    uniform_factor: f32,
    track_factor: f32,
}

impl ZombieMotionRules for TestRules {
    fn track_frames(&self, _profile: ZombieTrackProfile) -> Option<&[f32]> {
        Some(self.frames)
    }

    fn uniform_chilled_factor(&self) -> f32 {
        self.uniform_factor
    }

    fn track_chilled_factor(&self) -> f32 {
        self.track_factor
    }
}

const TEST_RULES: TestRules = TestRules {
    frames: &[0.0, 10.0, 20.0],
    uniform_factor: 0.4,
    track_factor: 0.5,
};

fn uniform_state(motion: UniformZombieMotion) -> ZombieMotionState {
    ZombieMotionState {
        id: ZombieId::from_raw(1),
        kind: ZombieKind::Normal,
        row: 0,
        x: 100.0,
        speed_x: 2.0,
        scale: 1.0,
        counters: ZombieMotionCounters {
            frozen: 0,
            chilled: 0,
            buttered: 0,
        },
        model: ZombieMovementModel::Uniform {
            motion,
            direction: ZombieMotionDirection::Left,
        },
    }
}

fn track_state(progress: f32) -> ZombieMotionState {
    ZombieMotionState {
        id: ZombieId::from_raw(1),
        kind: ZombieKind::Normal,
        row: 0,
        x: 100.0,
        speed_x: 2.0,
        scale: 1.0,
        counters: ZombieMotionCounters {
            frozen: 0,
            chilled: 0,
            buttered: 0,
        },
        model: ZombieMovementModel::Track {
            profile: ZombieTrackProfile::NormalWalkA,
            progress,
            playback_rate_fps: 47.0,
            direction: ZombieMotionDirection::Left,
        },
    }
}

fn set_track_playback_rate(state: &mut ZombieMotionState, playback_rate_fps: f32) {
    let ZombieMovementModel::Track {
        playback_rate_fps: rate,
        ..
    } = &mut state.model
    else {
        panic!("expected track state");
    };
    *rate = playback_rate_fps;
}

fn set_motion_direction(state: &mut ZombieMotionState, value: ZombieMotionDirection) {
    match &mut state.model {
        ZombieMovementModel::Track { direction, .. } | ZombieMovementModel::Uniform { direction, .. } => {
            *direction = value
        }
        ZombieMovementModel::Unsupported(_) => panic!("expected supported motion state"),
    }
}

#[test]
fn zombie_motion_rejects_large_horizon_before_allocating() {
    let err = predict_stable_zombie_x_trace(
        &uniform_state(UniformZombieMotion::DiggerTunneling),
        MAX_ZOMBIE_MOTION_HORIZON + 1,
    )
    .expect_err("err");
    assert_eq!(err, ZombieMotionError::InvalidHorizon);
}

#[test]
fn zombie_motion_counter_order_matches_vanilla() {
    let mut state = uniform_state(UniformZombieMotion::DiggerTunneling);
    state.counters.frozen = 1;
    let frozen_one = predict_stable_zombie_x_trace(&state, 1).expect("trace");
    assert_eq!(frozen_one, vec![100.0, 98.0]);

    state.counters.frozen = 2;
    let frozen_two = predict_stable_zombie_x_trace(&state, 2).expect("trace");
    assert_eq!(frozen_two, vec![100.0, 100.0, 98.0]);

    state.counters.frozen = 0;
    state.counters.buttered = 1;
    let buttered_one = predict_stable_zombie_x_trace(&state, 1).expect("trace");
    assert_eq!(buttered_one, vec![100.0, 98.0]);

    state.counters.buttered = 2;
    let buttered_two = predict_stable_zombie_x_trace(&state, 2).expect("trace");
    assert_eq!(buttered_two, vec![100.0, 100.0, 98.0]);
}

#[test]
fn zombie_motion_chilled_counter_ticks_before_move() {
    let mut state = uniform_state(UniformZombieMotion::BalloonFlying);
    state.counters.chilled = 1;
    let chilled_one = predict_stable_zombie_x_trace(&state, 1).expect("trace");
    assert_eq!(chilled_one, vec![100.0, 98.0]);

    state.counters.chilled = 2;
    let chilled_two = predict_stable_zombie_x_trace(&state, 2).expect("trace");
    assert_eq!(chilled_two, vec![100.0, 99.2, 97.2]);
}

#[test]
fn zombie_motion_immobilized_still_ticks_chill() {
    let mut state = uniform_state(UniformZombieMotion::BalloonFlying);
    state.counters.frozen = 1;
    state.counters.chilled = 2;
    let trace = predict_stable_zombie_x_trace(&state, 1).expect("trace");
    assert_eq!(trace, vec![100.0, 99.2]);

    state.counters.frozen = 2;
    state.counters.chilled = 2;
    let trace = predict_stable_zombie_x_trace(&state, 2).expect("trace");
    assert_eq!(trace, vec![100.0, 100.0, 98.0]);
}

#[test]
fn zombie_motion_direction_right_increases_x() {
    let mut state = uniform_state(UniformZombieMotion::DiggerTunneling);
    set_motion_direction(&mut state, ZombieMotionDirection::Right);
    let trace = predict_stable_zombie_x_trace(&state, 2).expect("trace");
    assert_eq!(trace, vec![100.0, 102.0, 104.0]);
}

#[test]
fn zombie_motion_uniform_chill_policy_is_per_variant() {
    let mut digger = uniform_state(UniformZombieMotion::DiggerTunneling);
    digger.counters.chilled = 2;
    assert_eq!(predict_stable_zombie_x_at(&digger, 1).expect("x"), 98.0);

    for motion in [
        UniformZombieMotion::BalloonFlying,
        UniformZombieMotion::PogoBouncing,
        UniformZombieMotion::CatapultDriving,
    ] {
        let mut state = uniform_state(motion);
        state.counters.chilled = 2;
        assert_eq!(predict_stable_zombie_x_at(&state, 1).expect("x"), 99.2);
    }
}

#[test]
fn zombie_motion_pogo_walk_uses_vanilla_pogo_table() {
    assert_eq!(
        vanilla_track_frames(ZombieTrackProfile::PogoWalk),
        vanilla_track_frames(ZombieTrackProfile::NormalWalkA)
    );
    assert_ne!(
        vanilla_track_frames(ZombieTrackProfile::PogoWalk),
        vanilla_track_frames(ZombieTrackProfile::NormalWalkB)
    );
}

#[test]
fn zombie_motion_track_scale_affects_progress_not_dx() {
    let mut state = track_state(0.0);
    state.speed_x = 30.0;
    set_track_playback_rate(&mut state, 47.0);
    let uneven_rules = TestRules {
        frames: &[0.0, 10.0, 30.0],
        uniform_factor: 0.4,
        track_factor: 0.5,
    };
    let normal = predict_stable_zombie_x_trace_with_rules(&state, 2, &uneven_rules).expect("trace");
    let mut scaled = state;
    scaled.scale = 0.25;
    set_track_playback_rate(&mut scaled, 188.0);
    let fast_progress = predict_stable_zombie_x_trace_with_rules(&scaled, 2, &uneven_rules).expect("trace");
    assert_eq!(normal.get(1), fast_progress.get(1));
    assert_ne!(normal.get(2), fast_progress.get(2));
}

#[test]
fn zombie_motion_track_chill_uses_counter_after_tick() {
    let progress = 0.01;
    let mut unchilled = track_state(progress);
    unchilled.speed_x = 10.0;
    set_track_playback_rate(&mut unchilled, 70.5);
    let expected = predict_stable_zombie_x_at_with_rules(&unchilled, 1, &TEST_RULES).expect("unchilled x");

    let mut chilled_one = unchilled;
    chilled_one.counters.chilled = 1;
    assert_eq!(
        predict_stable_zombie_x_at_with_rules(&chilled_one, 1, &TEST_RULES).expect("chilled one x"),
        expected
    );

    let mut chilled_two = unchilled;
    chilled_two.counters.chilled = 2;
    let slowed = predict_stable_zombie_x_at_with_rules(&chilled_two, 1, &TEST_RULES).expect("chilled two x");
    assert!(slowed > expected);
}

#[test]
fn zombie_motion_invalid_track_inputs_are_errors() {
    let state = track_state(1.0);
    assert_eq!(
        predict_stable_zombie_x_trace_with_rules(&state, 1, &TEST_RULES).expect_err("err"),
        ZombieMotionError::InvalidAnimationProgress
    );

    let state = track_state(f32::NAN);
    assert_eq!(
        predict_stable_zombie_x_trace_with_rules(&state, 1, &TEST_RULES).expect_err("err"),
        ZombieMotionError::InvalidAnimationProgress
    );

    let state = track_state(-0.1);
    assert_eq!(
        predict_stable_zombie_x_trace_with_rules(&state, 1, &TEST_RULES).expect_err("err"),
        ZombieMotionError::InvalidAnimationProgress
    );

    let mut state = track_state(0.0);
    set_track_playback_rate(&mut state, f32::NAN);
    assert_eq!(
        predict_stable_zombie_x_trace_with_rules(&state, 1, &TEST_RULES).expect_err("err"),
        ZombieMotionError::InvalidMotionState
    );

    let mut state = track_state(0.0);
    set_track_playback_rate(&mut state, -1.0);
    assert_eq!(
        predict_stable_zombie_x_trace_with_rules(&state, 1, &TEST_RULES).expect_err("err"),
        ZombieMotionError::InvalidMotionState
    );

    let bad_rules = TestRules {
        frames: &[0.0],
        uniform_factor: 0.4,
        track_factor: 0.5,
    };
    assert_eq!(
        predict_stable_zombie_x_trace_with_rules(&track_state(0.0), 1, &bad_rules).expect_err("err"),
        ZombieMotionError::InvalidTrackProfile
    );

    for frames in [&[][..], &[1.0, 1.0][..], &[0.0, f32::NAN, 1.0][..]] {
        let bad_rules = TestRules {
            frames,
            uniform_factor: 0.4,
            track_factor: 0.5,
        };
        assert_eq!(
            predict_stable_zombie_x_trace_with_rules(&track_state(0.0), 1, &bad_rules).expect_err("err"),
            ZombieMotionError::InvalidTrackProfile
        );
    }
}

#[test]
fn zombie_motion_rejects_nonfinite_updated_x() {
    let mut state = uniform_state(UniformZombieMotion::DiggerTunneling);
    state.x = f32::MAX;
    state.speed_x = f32::MAX;
    set_motion_direction(&mut state, ZombieMotionDirection::Right);
    assert_eq!(
        predict_stable_zombie_x_trace(&state, 1).expect_err("err"),
        ZombieMotionError::InvalidMotionState
    );
}

#[test]
fn zombie_motion_custom_rules_validate_factors_and_allow_negative_segments() {
    let negative_segment_rules = TestRules {
        frames: &[0.0, -1.0, 10.0],
        uniform_factor: 0.4,
        track_factor: 0.5,
    };
    let trace = predict_stable_zombie_x_trace_with_rules(&track_state(0.0), 1, &negative_segment_rules).expect("trace");
    assert!(trace.get(1).copied().expect("x") > 100.0);

    let bad_factor_rules = TestRules {
        frames: &[0.0, 10.0, 20.0],
        uniform_factor: f32::NAN,
        track_factor: -1.0,
    };
    let mut state = track_state(0.0);
    state.counters.chilled = 2;
    assert_eq!(
        predict_stable_zombie_x_trace_with_rules(&state, 1, &bad_factor_rules).expect_err("err"),
        ZombieMotionError::InvalidMotionRules
    );

    for uniform_factor in [f32::NAN, -1.0] {
        let bad_uniform_rules = TestRules {
            frames: &[0.0, 10.0, 20.0],
            uniform_factor,
            track_factor: 0.5,
        };
        let mut state = uniform_state(UniformZombieMotion::BalloonFlying);
        state.counters.chilled = 2;
        assert_eq!(
            predict_stable_zombie_x_trace_with_rules(&state, 1, &bad_uniform_rules).expect_err("err"),
            ZombieMotionError::InvalidMotionRules
        );
    }
}

#[test]
fn zombie_motion_unsupported_returns_error() {
    let mut state = uniform_state(UniformZombieMotion::DiggerTunneling);
    state.model = ZombieMovementModel::Unsupported(UnsupportedZombieMotionReason::UnsupportedKind);
    assert_eq!(
        predict_stable_zombie_x_trace(&state, 1).expect_err("err"),
        ZombieMotionError::Unsupported(UnsupportedZombieMotionReason::UnsupportedKind)
    );
}

#[test]
fn zombie_motion_trace_len_is_horizon_plus_one() {
    let zero = predict_stable_zombie_x_trace(&uniform_state(UniformZombieMotion::DiggerTunneling), 0).expect("trace");
    assert_eq!(zero, vec![100.0], "horizon 0 should return current x");

    let trace = predict_stable_zombie_x_trace(&uniform_state(UniformZombieMotion::DiggerTunneling), 3).expect("trace");
    assert_eq!(trace.len(), 4);
    assert_eq!(trace.first().copied(), Some(100.0));
}

#[test]
fn zombie_motion_zomboni_uses_current_x_for_each_speed_update() {
    let mut state = uniform_state(UniformZombieMotion::Zomboni);
    state.x = 401.0;
    state.speed_x = 0.2;
    let trace = predict_stable_zombie_x_trace(&state, 3).expect("trace");
    assert_eq!(trace, vec![401.0, 400.8995, 400.7995, 400.6995]);
    let first_delta = trace.first().copied().expect("x0") - trace.get(1).copied().expect("x1");
    let second_delta = trace.get(1).copied().expect("x1") - trace.get(2).copied().expect("x2");
    let third_delta = trace.get(2).copied().expect("x2") - trace.get(3).copied().expect("x3");
    assert!((first_delta - 0.1005).abs() < 0.000_1);
    assert!((second_delta - 0.1).abs() < 0.000_1);
    assert!((third_delta - 0.1).abs() < 0.000_1);
}

#[test]
fn zombie_motion_zomboni_boundaries_match_vanilla_int_x_policy() {
    fn one_frame_delta(x: f32) -> f32 {
        let mut state = uniform_state(UniformZombieMotion::Zomboni);
        state.x = x;
        state.speed_x = 0.2;
        let next = predict_stable_zombie_x_at(&state, 1).expect("x");
        x - next
    }

    assert!((one_frame_delta(700.5) - 0.25).abs() < 0.000_1);
    assert!((one_frame_delta(700.0) - 0.25).abs() < 0.000_1);
    assert!((one_frame_delta(699.9) - 0.2495).abs() < 0.000_1);
    assert!((one_frame_delta(400.9) - 0.1).abs() < 0.000_1);
    assert!((one_frame_delta(400.0) - 0.1).abs() < 0.000_1);
    assert!((one_frame_delta(399.9) - 0.1).abs() < 0.000_1);
}
