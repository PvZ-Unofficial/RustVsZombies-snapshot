use rsvz_model::model::{
    UniformZombieMotion, UnsupportedZombieMotionReason, ZombieKind, ZombieMotionDirection, ZombieMovementModel,
    ZombiePhase, ZombieTrackProfile,
};

use super::facts::{
    HEIGHT_ZOMBIE_NORMAL, REANIM_LOOP_FULL_LAST_FRAME, REANIM_PLAY_ONCE_AND_HOLD, RawZombieMotionFacts,
    ZOMBONI_DRIVE_FRAME_COUNT, ZOMBONI_DRIVE_FRAME_START,
};
use super::reanim::{
    reanimation_is_valid, reanimation_time_is_bounded, sample_reanimation_progress, track_profile_reanimation_state,
};

pub(super) fn classify_vanilla_zombie_motion(facts: RawZombieMotionFacts) -> ZombieMovementModel {
    let unsupported = ZombieMovementModel::Unsupported;

    if facts.is_eating {
        return unsupported(UnsupportedZombieMotionReason::Eating);
    }
    if facts.blowing_away {
        return unsupported(UnsupportedZombieMotionReason::BlowingAway);
    }
    if facts.mind_controlled {
        return unsupported(UnsupportedZombieMotionReason::MindControlled);
    }
    if facts.yucky_face {
        return unsupported(UnsupportedZombieMotionReason::YuckyFaceFrozen);
    }
    if !facts.speed_x.is_finite() || facts.speed_x < 0.0 || !facts.scale.is_finite() || facts.scale <= 0.0 {
        return unsupported(UnsupportedZombieMotionReason::InvalidMotionState);
    }
    if !reanimation_is_valid(facts.reanimation) {
        return unsupported(UnsupportedZombieMotionReason::InvalidReanimation);
    }
    if facts.zombie_height != HEIGHT_ZOMBIE_NORMAL {
        return unsupported(UnsupportedZombieMotionReason::UnsupportedPhase);
    }
    if is_unsupported_backwards_motion(facts) {
        return unsupported(UnsupportedZombieMotionReason::WalkingBackwards);
    }
    if !facts.has_head && !matches!(facts.kind, ZombieKind::Zomboni) {
        return unsupported(UnsupportedZombieMotionReason::UnsupportedPhase);
    }

    if facts.kind == ZombieKind::Zomboni && facts.flat_tires {
        return unsupported(UnsupportedZombieMotionReason::FlatTires);
    }

    if let Some(uniform) = classify_uniform_motion(facts) {
        if !uniform_motion_matches_reanimation(uniform, facts) {
            return unsupported(UnsupportedZombieMotionReason::InvalidReanimation);
        }
        return ZombieMovementModel::Uniform {
            motion: uniform,
            direction: uniform_motion_direction(uniform),
        };
    }

    if let Some(profile) = classify_track_profile(facts) {
        let Some((progress, playback_rate_fps)) = track_profile_reanimation_state(profile, facts) else {
            return unsupported(UnsupportedZombieMotionReason::InvalidReanimation);
        };
        return ZombieMovementModel::Track {
            profile,
            progress,
            playback_rate_fps,
            direction: track_profile_direction(profile, facts),
        };
    }

    unsupported(classify_unknown_reason(facts))
}

fn classify_track_profile(facts: RawZombieMotionFacts) -> Option<ZombieTrackProfile> {
    match (facts.kind, facts.phase, facts.reanimation.frame_start) {
        (
            ZombieKind::Normal
            | ZombieKind::Flag
            | ZombieKind::Conehead
            | ZombieKind::Buckethead
            | ZombieKind::ScreenDoor,
            ZombiePhase::ZombieNormal,
            44,
        ) => Some(ZombieTrackProfile::NormalWalkA),
        (
            ZombieKind::Normal
            | ZombieKind::Flag
            | ZombieKind::Conehead
            | ZombieKind::Buckethead
            | ZombieKind::ScreenDoor,
            ZombiePhase::ZombieNormal,
            91,
        ) => Some(ZombieTrackProfile::NormalWalkB),
        (
            ZombieKind::Normal
            | ZombieKind::Flag
            | ZombieKind::Conehead
            | ZombieKind::Buckethead
            | ZombieKind::ScreenDoor,
            ZombiePhase::ZombieNormal,
            250,
        ) if facts.in_pool => Some(ZombieTrackProfile::NormalSwim),
        (ZombieKind::Normal | ZombieKind::Conehead | ZombieKind::Buckethead, ZombiePhase::ZombieNormal, 454) => {
            Some(ZombieTrackProfile::NormalDance)
        }
        (ZombieKind::PoleVaulting, ZombiePhase::PolevaulterPreVault, 13) => {
            Some(ZombieTrackProfile::PoleVaultBeforeJump)
        }
        (ZombieKind::PoleVaulting, ZombiePhase::PolevaulterPostVault, 93) => {
            Some(ZombieTrackProfile::PoleVaultAfterJump)
        }
        (ZombieKind::Newspaper, ZombiePhase::NewspaperReading, 25)
        | (ZombieKind::Newspaper, ZombiePhase::NewspaperMad, 145) => Some(ZombieTrackProfile::NewspaperWalk),
        (ZombieKind::Football, ZombiePhase::ZombieNormal, 21) => Some(ZombieTrackProfile::FootballWalk),
        (ZombieKind::JackInTheBox, ZombiePhase::JackInTheBoxRunning, 30) => Some(ZombieTrackProfile::JackBoxWalk),
        (ZombieKind::Balloon, ZombiePhase::BalloonWalking, 84) => Some(ZombieTrackProfile::BalloonWalk),
        (ZombieKind::Digger, ZombiePhase::DiggerWalking | ZombiePhase::DiggerWalkingWithoutAxe, 18) => {
            Some(ZombieTrackProfile::DiggerWalk)
        }
        (ZombieKind::Pogo, ZombiePhase::ZombieNormal, 29) => Some(ZombieTrackProfile::PogoWalk),
        (ZombieKind::Yeti, ZombiePhase::ZombieNormal, 15) if facts.has_object => Some(ZombieTrackProfile::YetiWalk),
        (ZombieKind::Ladder, ZombiePhase::LadderCarrying, 25) => Some(ZombieTrackProfile::LadderWalk),
        (ZombieKind::Ladder, ZombiePhase::ZombieNormal, 132) => Some(ZombieTrackProfile::LadderWalk),
        (ZombieKind::Gargantuar | ZombieKind::GigaGargantuar, ZombiePhase::ZombieNormal, 22) => {
            Some(ZombieTrackProfile::GargantuarWalk)
        }
        _ => None,
    }
}

fn classify_uniform_motion(facts: RawZombieMotionFacts) -> Option<UniformZombieMotion> {
    match (facts.kind, facts.phase, facts.reanimation.frame_start) {
        (ZombieKind::Zomboni, ZombiePhase::ZombieNormal, _) => Some(UniformZombieMotion::Zomboni),
        (ZombieKind::Balloon, ZombiePhase::BalloonFlying, 0) => Some(UniformZombieMotion::BalloonFlying),
        (ZombieKind::Digger, ZombiePhase::DiggerTunneling, 128) => Some(UniformZombieMotion::DiggerTunneling),
        (ZombieKind::Pogo, ZombiePhase::PogoBouncing, 155) => Some(UniformZombieMotion::PogoBouncing),
        (ZombieKind::Catapult, ZombiePhase::ZombieNormal, 0) => Some(UniformZombieMotion::CatapultDriving),
        _ => None,
    }
}

fn uniform_motion_matches_reanimation(uniform: UniformZombieMotion, facts: RawZombieMotionFacts) -> bool {
    match uniform {
        UniformZombieMotion::BalloonFlying | UniformZombieMotion::CatapultDriving => {
            sample_reanimation_progress(facts.reanimation).is_some()
        }
        UniformZombieMotion::DiggerTunneling => {
            facts.reanimation.loop_type == REANIM_LOOP_FULL_LAST_FRAME
                && reanimation_time_is_bounded(facts.reanimation.anim_time)
        }
        UniformZombieMotion::PogoBouncing => {
            facts.reanimation.loop_type == REANIM_PLAY_ONCE_AND_HOLD
                && reanimation_time_is_bounded(facts.reanimation.anim_time)
        }
        UniformZombieMotion::Zomboni => {
            facts.reanimation.frame_start == ZOMBONI_DRIVE_FRAME_START
                && facts.reanimation.frame_count == ZOMBONI_DRIVE_FRAME_COUNT
                && sample_reanimation_progress(facts.reanimation).is_some()
        }
    }
}

fn track_profile_direction(profile: ZombieTrackProfile, facts: RawZombieMotionFacts) -> ZombieMotionDirection {
    match (profile, facts.phase) {
        (ZombieTrackProfile::DiggerWalk, ZombiePhase::DiggerWalking) => ZombieMotionDirection::Right,
        _ => ZombieMotionDirection::Left,
    }
}

fn uniform_motion_direction(uniform: UniformZombieMotion) -> ZombieMotionDirection {
    match uniform {
        UniformZombieMotion::Zomboni
        | UniformZombieMotion::BalloonFlying
        | UniformZombieMotion::DiggerTunneling
        | UniformZombieMotion::PogoBouncing
        | UniformZombieMotion::CatapultDriving => ZombieMotionDirection::Left,
    }
}

fn is_unsupported_backwards_motion(facts: RawZombieMotionFacts) -> bool {
    facts.kind == ZombieKind::Yeti && !facts.has_object
}

fn classify_unknown_reason(facts: RawZombieMotionFacts) -> UnsupportedZombieMotionReason {
    match facts.kind {
        ZombieKind::Dancing
        | ZombieKind::BackupDancer
        | ZombieKind::DuckyTube
        | ZombieKind::Snorkel
        | ZombieKind::Bobsled
        | ZombieKind::DolphinRider
        | ZombieKind::Bungee
        | ZombieKind::Boss
        | ZombieKind::PeaHead
        | ZombieKind::WallNutHead
        | ZombieKind::JalapenoHead
        | ZombieKind::GatlingHead
        | ZombieKind::SquashHead
        | ZombieKind::TallNutHead
        | ZombieKind::Imp => UnsupportedZombieMotionReason::UnsupportedKind,
        _ => UnsupportedZombieMotionReason::UnsupportedPhase,
    }
}
