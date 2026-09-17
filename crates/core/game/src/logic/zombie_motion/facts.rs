use rsvz_model::model::{ZombieKind, ZombiePhase, ZombieReanimationFacts};

pub(super) const REANIM_LOOP: i32 = 0;
pub(super) const REANIM_LOOP_FULL_LAST_FRAME: i32 = 1;
pub(super) const REANIM_PLAY_ONCE_AND_HOLD: i32 = 3;
pub(super) const HEIGHT_ZOMBIE_NORMAL: i32 = 0;
pub(super) const ZOMBONI_DRIVE_FRAME_START: i32 = 0;
// Confirmed with x86 cdb against PvZ 1.0.0.1051: Zomboni `anim_drive` reports 13
// runtime frames, even though the source reanim text contains more transform rows.
pub(super) const ZOMBONI_DRIVE_FRAME_COUNT: i32 = 13;
pub(super) const PVZ_REANIM_BASE_FPS: f32 = 47.0;
pub(super) const TRACK_CHILLED_FACTOR: f32 = 0.5;

#[derive(Clone, Copy, Debug)]
pub(super) struct RawZombieMotionFacts {
    pub(super) kind: ZombieKind,
    pub(super) phase: ZombiePhase,
    pub(super) zombie_height: i32,
    pub(super) speed_x: f32,
    pub(super) scale: f32,
    pub(super) reanimation: ZombieReanimationFacts,
    pub(super) is_eating: bool,
    pub(super) mind_controlled: bool,
    pub(super) blowing_away: bool,
    pub(super) flat_tires: bool,
    pub(super) has_head: bool,
    pub(super) has_object: bool,
    pub(super) in_pool: bool,
    pub(super) yucky_face: bool,
    pub(super) frozen_countdown: i32,
    pub(super) chilled_countdown: i32,
    pub(super) buttered_countdown: i32,
}
