use rsvz_model::model::{
    UniformZombieMotion, UnsupportedZombieMotionReason, ZombieKind, ZombieMotionDirection, ZombieMovementModel,
    ZombiePhase, ZombieReanimationFacts, ZombieTrackProfile,
};

use super::classify::classify_vanilla_zombie_motion;
use super::facts::*;
use super::reanim::sample_reanimation_progress;
use super::vanilla_track_frames;

fn facts(kind: ZombieKind, phase: ZombiePhase, frame_start: i32, frame_count: i32) -> RawZombieMotionFacts {
    RawZombieMotionFacts {
        kind,
        phase,
        zombie_height: HEIGHT_ZOMBIE_NORMAL,
        speed_x: 1.0,
        scale: 1.0,
        reanimation: ZombieReanimationFacts {
            anim_time: 0.24,
            last_time: 0.23,
            rate: 47.0,
            frame_start,
            frame_count,
            loop_type: REANIM_LOOP,
        },
        is_eating: false,
        mind_controlled: false,
        blowing_away: false,
        flat_tires: false,
        has_head: true,
        has_object: false,
        in_pool: false,
        yucky_face: false,
        frozen_countdown: 0,
        chilled_countdown: 0,
        buttered_countdown: 0,
    }
}

fn track_count(profile: ZombieTrackProfile) -> i32 {
    i32::try_from(vanilla_track_frames(profile).len()).expect("vanilla track length fits i32")
}

fn reanimation(anim_time: f32, rate: f32, frame_count: i32, loop_type: i32) -> ZombieReanimationFacts {
    ZombieReanimationFacts {
        anim_time,
        last_time: anim_time,
        rate,
        frame_start: 0,
        frame_count,
        loop_type,
    }
}

fn assert_track(
    raw: RawZombieMotionFacts, expected_profile: ZombieTrackProfile, expected_direction: ZombieMotionDirection,
) {
    let model = classify_vanilla_zombie_motion(raw);
    assert!(
        matches!(
            model,
            ZombieMovementModel::Track {
                profile,
                direction,
                ..
            } if profile == expected_profile && direction == expected_direction
        ),
        "expected {expected_profile:?}, got {model:?}"
    );
}

fn assert_track_case(
    kind: ZombieKind, phase: ZombiePhase, frame_start: i32, profile: ZombieTrackProfile,
    direction: ZombieMotionDirection,
) {
    assert_track(
        facts(kind, phase, frame_start, track_count(profile)),
        profile,
        direction,
    );
}

fn assert_uniform(
    raw: RawZombieMotionFacts, expected_uniform: UniformZombieMotion, expected_direction: ZombieMotionDirection,
) {
    assert_eq!(
        classify_vanilla_zombie_motion(raw),
        ZombieMovementModel::Uniform {
            motion: expected_uniform,
            direction: expected_direction,
        }
    );
}

fn assert_unsupported(raw: RawZombieMotionFacts, expected_reason: UnsupportedZombieMotionReason) {
    assert_eq!(
        classify_vanilla_zombie_motion(raw),
        ZombieMovementModel::Unsupported(expected_reason)
    );
}

#[test]
fn zombie_motion_classification_requires_stable_phase() {
    let frame_count = track_count(ZombieTrackProfile::NormalWalkA);
    let model = classify_vanilla_zombie_motion(facts(ZombieKind::Normal, ZombiePhase::ZombieNormal, 44, frame_count));
    match model {
        ZombieMovementModel::Track {
            profile: ZombieTrackProfile::NormalWalkA,
            progress,
            playback_rate_fps,
            ..
        } => {
            assert!((progress - 0.24).abs() < f32::EPSILON);
            assert!((playback_rate_fps - 47.0).abs() < f32::EPSILON);
        }
        model => panic!("expected NormalWalkA, got {model:?}"),
    }

    let model = classify_vanilla_zombie_motion(facts(ZombieKind::Normal, ZombiePhase::ZombieDying, 44, frame_count));
    assert_eq!(
        model,
        ZombieMovementModel::Unsupported(UnsupportedZombieMotionReason::UnsupportedPhase)
    );
}

#[test]
fn zombie_motion_track_playback_rate_reports_base_rate() {
    let mut raw = facts(
        ZombieKind::Normal,
        ZombiePhase::ZombieNormal,
        44,
        track_count(ZombieTrackProfile::NormalWalkA),
    );
    raw.reanimation.rate = 23.5;
    raw.chilled_countdown = 2;

    let model = classify_vanilla_zombie_motion(raw);
    match model {
        ZombieMovementModel::Track { playback_rate_fps, .. } => {
            assert!((playback_rate_fps - 47.0).abs() < f32::EPSILON)
        }
        model => panic!("expected track model, got {model:?}"),
    }
}

#[test]
fn zombie_motion_classification_rejects_unstable_flags() {
    let mut raw = facts(ZombieKind::Zomboni, ZombiePhase::ZombieNormal, 0, 1);
    raw.flat_tires = true;
    assert_eq!(
        classify_vanilla_zombie_motion(raw),
        ZombieMovementModel::Unsupported(UnsupportedZombieMotionReason::FlatTires)
    );

    raw.flat_tires = false;
    raw.mind_controlled = true;
    assert_eq!(
        classify_vanilla_zombie_motion(raw),
        ZombieMovementModel::Unsupported(UnsupportedZombieMotionReason::MindControlled)
    );

    raw.mind_controlled = false;
    raw.is_eating = true;
    assert_eq!(
        classify_vanilla_zombie_motion(raw),
        ZombieMovementModel::Unsupported(UnsupportedZombieMotionReason::Eating)
    );
}

#[test]
fn zombie_motion_classification_rejects_wrong_reanimation() {
    let raw = facts(ZombieKind::Normal, ZombiePhase::ZombieNormal, 44, 99);
    assert_eq!(
        classify_vanilla_zombie_motion(raw),
        ZombieMovementModel::Unsupported(UnsupportedZombieMotionReason::InvalidReanimation)
    );

    let frame_count = track_count(ZombieTrackProfile::NormalWalkA);
    let mut raw = facts(ZombieKind::Normal, ZombiePhase::ZombieNormal, 44, frame_count);
    raw.reanimation.loop_type = 2;
    assert_eq!(
        classify_vanilla_zombie_motion(raw),
        ZombieMovementModel::Unsupported(UnsupportedZombieMotionReason::InvalidReanimation)
    );

    let mut raw = facts(ZombieKind::Normal, ZombiePhase::ZombieNormal, 44, frame_count);
    raw.reanimation.rate = -47.0;
    assert_eq!(
        classify_vanilla_zombie_motion(raw),
        ZombieMovementModel::Unsupported(UnsupportedZombieMotionReason::InvalidReanimation)
    );

    let wrong_zomboni = facts(ZombieKind::Zomboni, ZombiePhase::ZombieNormal, 0, 1);
    assert_eq!(
        classify_vanilla_zombie_motion(wrong_zomboni),
        ZombieMovementModel::Unsupported(UnsupportedZombieMotionReason::InvalidReanimation)
    );
}

#[test]
fn zombie_motion_classification_distinguishes_invalid_motion_state() {
    let frame_count = track_count(ZombieTrackProfile::NormalWalkA);
    let mut raw = facts(ZombieKind::Normal, ZombiePhase::ZombieNormal, 44, frame_count);
    raw.speed_x = f32::NAN;
    assert_eq!(
        classify_vanilla_zombie_motion(raw),
        ZombieMovementModel::Unsupported(UnsupportedZombieMotionReason::InvalidMotionState)
    );

    raw = facts(ZombieKind::Normal, ZombiePhase::ZombieNormal, 44, frame_count);
    raw.scale = 0.0;
    assert_eq!(
        classify_vanilla_zombie_motion(raw),
        ZombieMovementModel::Unsupported(UnsupportedZombieMotionReason::InvalidMotionState)
    );

    raw = facts(ZombieKind::Normal, ZombiePhase::ZombieNormal, 44, frame_count);
    raw.reanimation.anim_time = f32::NAN;
    assert_eq!(
        classify_vanilla_zombie_motion(raw),
        ZombieMovementModel::Unsupported(UnsupportedZombieMotionReason::InvalidReanimation)
    );
}

#[test]
fn zombie_motion_samples_current_circulation_rate() {
    let progress = sample_reanimation_progress(reanimation(0.0, 47.0, 47, REANIM_LOOP)).expect("progress");
    assert!(progress.abs() < f32::EPSILON);
    let progress = sample_reanimation_progress(reanimation(0.99, 47.0, 47, REANIM_LOOP)).expect("progress");
    assert!((progress - 0.99).abs() < f32::EPSILON);
    assert_eq!(
        sample_reanimation_progress(reanimation(-0.02, 47.0, 47, REANIM_LOOP)),
        None
    );
    assert_eq!(
        sample_reanimation_progress(reanimation(-0.1, 0.0, 47, REANIM_LOOP)),
        None
    );
    assert_eq!(
        sample_reanimation_progress(reanimation(1.2, 0.0, 47, REANIM_LOOP)),
        None
    );
    assert_eq!(sample_reanimation_progress(reanimation(0.0, 47.0, 47, 2)), None);
}

#[test]
fn zombie_motion_classification_uses_yeti_object_for_backwards() {
    // UpdateYeti enters ZombiePhase::YetiRunning only after clearing has_object; that is the
    // rightward escape state, not the AvZ leftward walk table.
    let frame_count = track_count(ZombieTrackProfile::YetiWalk);
    let mut raw = facts(ZombieKind::Yeti, ZombiePhase::YetiRunning, 15, frame_count);
    raw.has_object = false;
    assert_unsupported(raw, UnsupportedZombieMotionReason::WalkingBackwards);

    raw.has_object = true;
    assert_unsupported(raw, UnsupportedZombieMotionReason::UnsupportedPhase);

    let mut raw = facts(ZombieKind::Yeti, ZombiePhase::ZombieNormal, 15, frame_count);
    raw.has_object = false;
    assert_unsupported(raw, UnsupportedZombieMotionReason::WalkingBackwards);
}

#[test]
fn zombie_motion_classification_maps_avz_branch_key_uniforms() {
    assert_uniform(
        facts(
            ZombieKind::Zomboni,
            ZombiePhase::ZombieNormal,
            0,
            ZOMBONI_DRIVE_FRAME_COUNT,
        ),
        UniformZombieMotion::Zomboni,
        ZombieMotionDirection::Left,
    );
    assert_uniform(
        facts(ZombieKind::Balloon, ZombiePhase::BalloonFlying, 0, 1),
        UniformZombieMotion::BalloonFlying,
        ZombieMotionDirection::Left,
    );
    let mut digger = facts(ZombieKind::Digger, ZombiePhase::DiggerTunneling, 128, 1);
    digger.reanimation.loop_type = REANIM_LOOP_FULL_LAST_FRAME;
    assert_uniform(
        digger,
        UniformZombieMotion::DiggerTunneling,
        ZombieMotionDirection::Left,
    );
    let mut digger_runtime_count = digger;
    digger_runtime_count.reanimation.frame_count = 42;
    digger_runtime_count.reanimation.anim_time = 1.0;
    assert_uniform(
        digger_runtime_count,
        UniformZombieMotion::DiggerTunneling,
        ZombieMotionDirection::Left,
    );

    let mut wrong_digger = facts(ZombieKind::Digger, ZombiePhase::DiggerTunneling, 128, 1);
    wrong_digger.reanimation.loop_type = REANIM_LOOP;
    assert_unsupported(wrong_digger, UnsupportedZombieMotionReason::InvalidReanimation);
    let mut wrong_digger_time = digger;
    wrong_digger_time.reanimation.anim_time = 1.1;
    assert_unsupported(wrong_digger_time, UnsupportedZombieMotionReason::InvalidReanimation);

    let mut pogo = facts(ZombieKind::Pogo, ZombiePhase::PogoBouncing, 155, 1);
    pogo.reanimation.loop_type = REANIM_PLAY_ONCE_AND_HOLD;
    pogo.reanimation.anim_time = 1.0;
    assert_uniform(pogo, UniformZombieMotion::PogoBouncing, ZombieMotionDirection::Left);
    let mut pogo_runtime_count = pogo;
    pogo_runtime_count.reanimation.frame_count = 40;
    pogo_runtime_count.reanimation.anim_time = 0.5;
    assert_uniform(
        pogo_runtime_count,
        UniformZombieMotion::PogoBouncing,
        ZombieMotionDirection::Left,
    );

    let mut wrong_pogo = facts(ZombieKind::Pogo, ZombiePhase::PogoBouncing, 155, 1);
    wrong_pogo.reanimation.loop_type = REANIM_PLAY_ONCE_AND_HOLD;
    wrong_pogo.reanimation.anim_time = 1.1;
    assert_unsupported(wrong_pogo, UnsupportedZombieMotionReason::InvalidReanimation);
    let mut wrong_pogo_loop = pogo;
    wrong_pogo_loop.reanimation.loop_type = REANIM_LOOP;
    assert_unsupported(wrong_pogo_loop, UnsupportedZombieMotionReason::InvalidReanimation);
    assert_uniform(
        facts(ZombieKind::Catapult, ZombiePhase::ZombieNormal, 0, 1),
        UniformZombieMotion::CatapultDriving,
        ZombieMotionDirection::Left,
    );
}

#[test]
fn zombie_motion_classification_maps_avz_branch_key_tracks() {
    for kind in [
        ZombieKind::Normal,
        ZombieKind::Flag,
        ZombieKind::Conehead,
        ZombieKind::Buckethead,
        ZombieKind::ScreenDoor,
    ] {
        assert_track_case(
            kind,
            ZombiePhase::ZombieNormal,
            44,
            ZombieTrackProfile::NormalWalkA,
            ZombieMotionDirection::Left,
        );
        assert_track_case(
            kind,
            ZombiePhase::ZombieNormal,
            91,
            ZombieTrackProfile::NormalWalkB,
            ZombieMotionDirection::Left,
        );
        let mut swim = facts(
            kind,
            ZombiePhase::ZombieNormal,
            250,
            track_count(ZombieTrackProfile::NormalSwim),
        );
        swim.in_pool = true;
        assert_track(swim, ZombieTrackProfile::NormalSwim, ZombieMotionDirection::Left);
    }

    for kind in [ZombieKind::Normal, ZombieKind::Conehead, ZombieKind::Buckethead] {
        assert_track_case(
            kind,
            ZombiePhase::ZombieNormal,
            454,
            ZombieTrackProfile::NormalDance,
            ZombieMotionDirection::Left,
        );
    }

    let cases = [
        (
            ZombieKind::PoleVaulting,
            ZombiePhase::PolevaulterPreVault,
            13,
            ZombieTrackProfile::PoleVaultBeforeJump,
            ZombieMotionDirection::Left,
        ),
        (
            ZombieKind::PoleVaulting,
            ZombiePhase::PolevaulterPostVault,
            93,
            ZombieTrackProfile::PoleVaultAfterJump,
            ZombieMotionDirection::Left,
        ),
        (
            ZombieKind::Football,
            ZombiePhase::ZombieNormal,
            21,
            ZombieTrackProfile::FootballWalk,
            ZombieMotionDirection::Left,
        ),
        (
            ZombieKind::JackInTheBox,
            ZombiePhase::JackInTheBoxRunning,
            30,
            ZombieTrackProfile::JackBoxWalk,
            ZombieMotionDirection::Left,
        ),
        (
            ZombieKind::Balloon,
            ZombiePhase::BalloonWalking,
            84,
            ZombieTrackProfile::BalloonWalk,
            ZombieMotionDirection::Left,
        ),
        (
            ZombieKind::Digger,
            ZombiePhase::DiggerWalking,
            18,
            ZombieTrackProfile::DiggerWalk,
            ZombieMotionDirection::Right,
        ),
        (
            ZombieKind::Digger,
            ZombiePhase::DiggerWalkingWithoutAxe,
            18,
            ZombieTrackProfile::DiggerWalk,
            ZombieMotionDirection::Left,
        ),
        (
            ZombieKind::Pogo,
            ZombiePhase::ZombieNormal,
            29,
            ZombieTrackProfile::PogoWalk,
            ZombieMotionDirection::Left,
        ),
        (
            ZombieKind::Ladder,
            ZombiePhase::LadderCarrying,
            25,
            ZombieTrackProfile::LadderWalk,
            ZombieMotionDirection::Left,
        ),
        (
            ZombieKind::Ladder,
            ZombiePhase::ZombieNormal,
            132,
            ZombieTrackProfile::LadderWalk,
            ZombieMotionDirection::Left,
        ),
    ];
    for (kind, phase, frame_start, profile, direction) in cases {
        assert_track_case(kind, phase, frame_start, profile, direction);
    }

    for (phase, frame_start) in [(ZombiePhase::NewspaperReading, 25), (ZombiePhase::NewspaperMad, 145)] {
        assert_track_case(
            ZombieKind::Newspaper,
            phase,
            frame_start,
            ZombieTrackProfile::NewspaperWalk,
            ZombieMotionDirection::Left,
        );
    }
    let mut yeti = facts(
        ZombieKind::Yeti,
        ZombiePhase::ZombieNormal,
        15,
        track_count(ZombieTrackProfile::YetiWalk),
    );
    yeti.has_object = true;
    assert_track(yeti, ZombieTrackProfile::YetiWalk, ZombieMotionDirection::Left);
    for kind in [ZombieKind::Gargantuar, ZombieKind::GigaGargantuar] {
        assert_track_case(
            kind,
            ZombiePhase::ZombieNormal,
            22,
            ZombieTrackProfile::GargantuarWalk,
            ZombieMotionDirection::Left,
        );
    }
}

#[test]
fn zombie_motion_classification_rejects_unverified_track_states() {
    const HEIGHT_UP_LADDER: i32 = 6;

    let walk_count = track_count(ZombieTrackProfile::NormalWalkA);
    let mut climbing = facts(ZombieKind::Normal, ZombiePhase::ZombieNormal, 44, walk_count);
    climbing.zombie_height = HEIGHT_UP_LADDER;
    assert_unsupported(climbing, UnsupportedZombieMotionReason::UnsupportedPhase);

    for kind in [ZombieKind::Flag, ZombieKind::ScreenDoor] {
        assert_unsupported(
            facts(
                kind,
                ZombiePhase::ZombieNormal,
                454,
                track_count(ZombieTrackProfile::NormalDance),
            ),
            UnsupportedZombieMotionReason::UnsupportedPhase,
        );
    }
}

#[test]
fn zombie_motion_classification_does_not_copy_avz_swim_fallthrough() {
    assert_unsupported(
        facts(
            ZombieKind::Normal,
            ZombiePhase::ZombieNormal,
            250,
            track_count(ZombieTrackProfile::NormalSwim),
        ),
        UnsupportedZombieMotionReason::UnsupportedPhase,
    );
}

#[test]
fn zombie_motion_classification_requires_phase_not_only_frame_start() {
    for raw in [
        facts(
            ZombieKind::Balloon,
            ZombiePhase::ZombieNormal,
            84,
            track_count(ZombieTrackProfile::BalloonWalk),
        ),
        facts(
            ZombieKind::Digger,
            ZombiePhase::ZombieNormal,
            18,
            track_count(ZombieTrackProfile::DiggerWalk),
        ),
        facts(
            ZombieKind::Pogo,
            ZombiePhase::PogoBouncing,
            29,
            track_count(ZombieTrackProfile::PogoWalk),
        ),
        facts(
            ZombieKind::Ladder,
            ZombiePhase::PogoBouncing,
            132,
            track_count(ZombieTrackProfile::LadderWalk),
        ),
    ] {
        assert_unsupported(raw, UnsupportedZombieMotionReason::UnsupportedPhase);
    }
}

#[test]
fn zombie_motion_classification_keeps_avz_empty_kinds_unsupported() {
    for kind in [
        ZombieKind::Dancing,
        ZombieKind::BackupDancer,
        ZombieKind::Snorkel,
        ZombieKind::DolphinRider,
        ZombieKind::Bobsled,
        ZombieKind::Bungee,
        ZombieKind::Boss,
        ZombieKind::PeaHead,
        ZombieKind::WallNutHead,
        ZombieKind::JalapenoHead,
        ZombieKind::GatlingHead,
        ZombieKind::SquashHead,
        ZombieKind::TallNutHead,
        ZombieKind::Imp,
        ZombieKind::DuckyTube,
    ] {
        assert_unsupported(
            facts(
                kind,
                ZombiePhase::ZombieNormal,
                44,
                track_count(ZombieTrackProfile::NormalWalkA),
            ),
            UnsupportedZombieMotionReason::UnsupportedKind,
        );
    }
}
