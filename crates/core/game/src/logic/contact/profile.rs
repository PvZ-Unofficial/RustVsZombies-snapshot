use rsvz_model::model::{ZombieContactProfile, ZombieGeometryOrientation, ZombieHeightState, ZombieKind, ZombiePhase};

use crate::logic::zombie_geometry::zombie_geometry_input_from_profile;
use rsvz_model::{ZombieGeometryError, ZombieGeometryInput};

const ZOMBIQUARIUM_MIRROR_MIN_ANGLE: f32 = 1.570_796_4;
const ZOMBIQUARIUM_MIRROR_MAX_ANGLE: f32 = 4.712_389;

pub(super) const fn zombie_height_from_raw(raw: i32) -> ZombieHeightState {
    match raw {
        0 => ZombieHeightState::Normal,
        1 => ZombieHeightState::InToPool,
        2 => ZombieHeightState::OutOfPool,
        3 => ZombieHeightState::DraggedUnder,
        4 => ZombieHeightState::UpToHighGround,
        5 => ZombieHeightState::DownOffHighGround,
        6 => ZombieHeightState::UpLadder,
        7 => ZombieHeightState::Falling,
        8 => ZombieHeightState::InToChimney,
        9 => ZombieHeightState::GettingBungeeDropped,
        10 => ZombieHeightState::Zombiquarium,
        _ => ZombieHeightState::Unknown(raw),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct RawZombieContactFacts {
    pub(super) kind: ZombieKind,
    pub(super) row: i32,
    pub(super) x: i32,
    pub(super) phase: ZombiePhase,
    pub(super) mind_controlled: bool,
    pub(super) has_object: bool,
    pub(super) zombie_height: ZombieHeightState,
    pub(super) vel_z: f32,
    pub(super) is_disappeared: bool,
}

pub(super) fn zombie_geometry_input_from_facts(
    facts: RawZombieContactFacts,
) -> Result<Option<ZombieGeometryInput>, ZombieGeometryError> {
    let Some(profile) = active_contact_profile_from_facts(facts) else {
        return Ok(None);
    };
    let orientation = orientation_from_facts(facts);
    Ok(Some(
        zombie_geometry_input_from_profile(facts.kind, facts.row, facts.x as f32, profile)?
            .with_orientation(orientation),
    ))
}

pub(super) fn active_contact_profile_from_facts(facts: RawZombieContactFacts) -> Option<ZombieContactProfile> {
    if facts.is_disappeared
        || matches!(
            facts.phase,
            ZombiePhase::ZombieDying | ZombiePhase::ZombieBurned | ZombiePhase::ZombieMowered
        )
    {
        return None;
    }
    contact_profile_from_facts(facts)
}

fn contact_profile_from_facts(facts: RawZombieContactFacts) -> Option<ZombieContactProfile> {
    match facts.kind {
        ZombieKind::Normal
        | ZombieKind::Flag
        | ZombieKind::Conehead
        | ZombieKind::Buckethead
        | ZombieKind::ScreenDoor
        | ZombieKind::DuckyTube
        | ZombieKind::PeaHead
        | ZombieKind::WallNutHead
        | ZombieKind::JalapenoHead
        | ZombieKind::GatlingHead
        | ZombieKind::SquashHead
        | ZombieKind::TallNutHead
        | ZombieKind::JackInTheBox => Some(ZombieContactProfile::CommonBody),
        ZombieKind::Yeti if matches!(facts.phase, ZombiePhase::ZombieNormal | ZombiePhase::YetiRunning) => {
            Some(ZombieContactProfile::CommonBody)
        }
        ZombieKind::Newspaper
            if matches!(
                facts.phase,
                ZombiePhase::NewspaperReading | ZombiePhase::NewspaperMaddening | ZombiePhase::NewspaperMad
            ) =>
        {
            Some(ZombieContactProfile::CommonBody)
        }
        ZombieKind::Football => Some(ZombieContactProfile::Football),
        ZombieKind::Digger
            if matches!(
                facts.phase,
                ZombiePhase::DiggerTunneling
                    | ZombiePhase::DiggerRising
                    | ZombiePhase::DiggerTunnelingPauseWithoutAxe
                    | ZombiePhase::DiggerRiseWithoutAxe
                    | ZombiePhase::DiggerStunned
                    | ZombiePhase::DiggerWalking
                    | ZombiePhase::DiggerWalkingWithoutAxe
                    | ZombiePhase::DiggerCutscene
            ) =>
        {
            Some(ZombieContactProfile::Digger)
        }
        ZombieKind::Snorkel
            if matches!(
                facts.phase,
                ZombiePhase::SnorkelWalking
                    | ZombiePhase::SnorkelIntoPool
                    | ZombiePhase::SnorkelWalkingInPool
                    | ZombiePhase::SnorkelUpToEat
                    | ZombiePhase::SnorkelEatingInPool
                    | ZombiePhase::SnorkelDownFromEat
            ) =>
        {
            Some(ZombieContactProfile::Snorkel)
        }
        ZombieKind::Ladder if matches!(facts.phase, ZombiePhase::LadderCarrying | ZombiePhase::LadderPlacing) => {
            Some(ZombieContactProfile::Ladder)
        }
        ZombieKind::Zomboni => Some(ZombieContactProfile::Vehicle),
        ZombieKind::Catapult
            if matches!(
                facts.phase,
                ZombiePhase::CatapultLaunching | ZombiePhase::CatapultReloading | ZombiePhase::ZombieNormal
            ) =>
        {
            Some(ZombieContactProfile::Vehicle)
        }
        ZombieKind::Gargantuar | ZombieKind::GigaGargantuar
            if matches!(
                facts.phase,
                ZombiePhase::ZombieNormal | ZombiePhase::GargantuarThrowing | ZombiePhase::GargantuarSmashing
            ) =>
        {
            Some(ZombieContactProfile::Gargantuar)
        }
        ZombieKind::PoleVaulting => match facts.phase {
            ZombiePhase::PolevaulterPreVault => Some(ZombieContactProfile::PoleBeforeVault),
            ZombiePhase::PolevaulterInVault => Some(ZombieContactProfile::PoleVaulting),
            ZombiePhase::PolevaulterPostVault => Some(ZombieContactProfile::NarrowBiteBody),
            _ => None,
        },
        ZombieKind::Pogo
            if matches!(
                facts.phase,
                ZombiePhase::PogoBouncing
                    | ZombiePhase::PogoHighBounce1
                    | ZombiePhase::PogoHighBounce2
                    | ZombiePhase::PogoHighBounce3
                    | ZombiePhase::PogoHighBounce4
                    | ZombiePhase::PogoHighBounce5
                    | ZombiePhase::PogoHighBounce6
                    | ZombiePhase::PogoForwardBounce2
                    | ZombiePhase::PogoForwardBounce7
            ) =>
        {
            Some(ZombieContactProfile::PogoMounted)
        }
        ZombieKind::Pogo if facts.phase == ZombiePhase::ZombieNormal && !facts.has_object => {
            Some(ZombieContactProfile::PogoOnFoot)
        }
        ZombieKind::Balloon
            if matches!(
                facts.phase,
                ZombiePhase::BalloonFlying | ZombiePhase::BalloonPopping | ZombiePhase::BalloonWalking
            ) =>
        {
            Some(ZombieContactProfile::Balloon)
        }
        ZombieKind::DolphinRider => match facts.phase {
            ZombiePhase::DolphinWalking | ZombiePhase::DolphinIntoPool => Some(ZombieContactProfile::NarrowBiteBody),
            ZombiePhase::DolphinRiding => Some(ZombieContactProfile::DolphinRiding),
            ZombiePhase::DolphinInJump => Some(ZombieContactProfile::DolphinJumping),
            ZombiePhase::DolphinWalkingInPool | ZombiePhase::DolphinWalkingWithoutDolphin => {
                Some(ZombieContactProfile::DolphinOnFoot)
            }
            _ => None,
        },
        _ => None,
    }
}

fn orientation_from_facts(facts: RawZombieContactFacts) -> ZombieGeometryOrientation {
    if facts.mind_controlled {
        return ZombieGeometryOrientation::Mirrored;
    }
    if facts.zombie_height == ZombieHeightState::Zombiquarium
        && (facts.vel_z < ZOMBIQUARIUM_MIRROR_MIN_ANGLE || facts.vel_z > ZOMBIQUARIUM_MIRROR_MAX_ANGLE)
    {
        return ZombieGeometryOrientation::Mirrored;
    }
    if matches!(
        (facts.kind, facts.phase),
        (
            ZombieKind::Digger,
            ZombiePhase::DiggerRising | ZombiePhase::DiggerStunned | ZombiePhase::DiggerWalking
        )
    ) || (facts.kind == ZombieKind::Yeti && !facts.has_object)
    {
        ZombieGeometryOrientation::Mirrored
    } else {
        ZombieGeometryOrientation::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(kind: ZombieKind, phase: ZombiePhase) -> RawZombieContactFacts {
        RawZombieContactFacts {
            kind,
            row: 0,
            x: 100,
            phase,
            mind_controlled: false,
            has_object: true,
            zombie_height: ZombieHeightState::Normal,
            vel_z: 0.0,
            is_disappeared: false,
        }
    }

    #[test]
    fn active_profiles_cover_phase_and_object_boundaries() {
        let cases = [
            (
                ZombieKind::Normal,
                ZombiePhase::ZombieNormal,
                ZombieContactProfile::CommonBody,
            ),
            (
                ZombieKind::JackInTheBox,
                ZombiePhase::JackInTheBoxRunning,
                ZombieContactProfile::CommonBody,
            ),
            (
                ZombieKind::Newspaper,
                ZombiePhase::NewspaperMaddening,
                ZombieContactProfile::CommonBody,
            ),
            (
                ZombieKind::Digger,
                ZombiePhase::DiggerWalking,
                ZombieContactProfile::Digger,
            ),
            (
                ZombieKind::Snorkel,
                ZombiePhase::SnorkelEatingInPool,
                ZombieContactProfile::Snorkel,
            ),
            (
                ZombieKind::Ladder,
                ZombiePhase::LadderPlacing,
                ZombieContactProfile::Ladder,
            ),
            (
                ZombieKind::Catapult,
                ZombiePhase::CatapultReloading,
                ZombieContactProfile::Vehicle,
            ),
            (
                ZombieKind::Gargantuar,
                ZombiePhase::GargantuarSmashing,
                ZombieContactProfile::Gargantuar,
            ),
            (
                ZombieKind::PoleVaulting,
                ZombiePhase::PolevaulterPreVault,
                ZombieContactProfile::PoleBeforeVault,
            ),
            (
                ZombieKind::PoleVaulting,
                ZombiePhase::PolevaulterInVault,
                ZombieContactProfile::PoleVaulting,
            ),
            (
                ZombieKind::PoleVaulting,
                ZombiePhase::PolevaulterPostVault,
                ZombieContactProfile::NarrowBiteBody,
            ),
            (
                ZombieKind::Pogo,
                ZombiePhase::PogoHighBounce3,
                ZombieContactProfile::PogoMounted,
            ),
            (
                ZombieKind::Balloon,
                ZombiePhase::BalloonWalking,
                ZombieContactProfile::Balloon,
            ),
            (
                ZombieKind::DolphinRider,
                ZombiePhase::DolphinInJump,
                ZombieContactProfile::DolphinJumping,
            ),
        ];
        for (kind, phase, expected) in cases {
            assert_eq!(active_contact_profile_from_facts(facts(kind, phase)), Some(expected));
        }

        let mut pogo_on_foot = facts(ZombieKind::Pogo, ZombiePhase::ZombieNormal);
        pogo_on_foot.has_object = false;
        assert_eq!(
            active_contact_profile_from_facts(pogo_on_foot),
            Some(ZombieContactProfile::PogoOnFoot)
        );
        assert_eq!(
            active_contact_profile_from_facts(facts(ZombieKind::Pogo, ZombiePhase::ZombieNormal)),
            None
        );
        assert_eq!(
            active_contact_profile_from_facts(facts(ZombieKind::Imp, ZombiePhase::ZombieNormal)),
            None
        );
    }

    #[test]
    fn inactive_zombies_have_no_contact_profile() {
        let mut disappeared = facts(ZombieKind::Normal, ZombiePhase::ZombieNormal);
        disappeared.is_disappeared = true;
        assert_eq!(active_contact_profile_from_facts(disappeared), None);
        for phase in [
            ZombiePhase::ZombieDying,
            ZombiePhase::ZombieBurned,
            ZombiePhase::ZombieMowered,
        ] {
            assert_eq!(
                active_contact_profile_from_facts(facts(ZombieKind::Normal, phase)),
                None
            );
        }
    }

    #[test]
    fn orientation_matches_native_backwards_rules_and_angle_boundaries() {
        let mut raw = facts(ZombieKind::Normal, ZombiePhase::ZombieNormal);
        raw.mind_controlled = true;
        assert_eq!(orientation_from_facts(raw), ZombieGeometryOrientation::Mirrored);

        raw = facts(ZombieKind::Digger, ZombiePhase::DiggerWalking);
        assert_eq!(orientation_from_facts(raw), ZombieGeometryOrientation::Mirrored);

        raw = facts(ZombieKind::Yeti, ZombiePhase::ZombieNormal);
        raw.has_object = false;
        assert_eq!(orientation_from_facts(raw), ZombieGeometryOrientation::Mirrored);

        raw = facts(ZombieKind::Normal, ZombiePhase::ZombiquariumDrift);
        raw.zombie_height = ZombieHeightState::Zombiquarium;
        for (angle, expected) in [
            (
                ZOMBIQUARIUM_MIRROR_MIN_ANGLE - 1.0e-6,
                ZombieGeometryOrientation::Mirrored,
            ),
            (ZOMBIQUARIUM_MIRROR_MIN_ANGLE, ZombieGeometryOrientation::Normal),
            (ZOMBIQUARIUM_MIRROR_MAX_ANGLE, ZombieGeometryOrientation::Normal),
            (
                ZOMBIQUARIUM_MIRROR_MAX_ANGLE + 1.0e-6,
                ZombieGeometryOrientation::Mirrored,
            ),
            (f32::NAN, ZombieGeometryOrientation::Normal),
        ] {
            raw.vel_z = angle;
            assert_eq!(orientation_from_facts(raw), expected, "angle {angle:?}");
        }
    }

    #[test]
    fn raw_height_codes_preserve_unknown_values() {
        let expected = [
            ZombieHeightState::Normal,
            ZombieHeightState::InToPool,
            ZombieHeightState::OutOfPool,
            ZombieHeightState::DraggedUnder,
            ZombieHeightState::UpToHighGround,
            ZombieHeightState::DownOffHighGround,
            ZombieHeightState::UpLadder,
            ZombieHeightState::Falling,
            ZombieHeightState::InToChimney,
            ZombieHeightState::GettingBungeeDropped,
            ZombieHeightState::Zombiquarium,
        ];
        for (raw, expected) in expected.into_iter().enumerate() {
            assert_eq!(zombie_height_from_raw(raw as i32), expected);
        }
        assert_eq!(zombie_height_from_raw(99), ZombieHeightState::Unknown(99));
    }
}
