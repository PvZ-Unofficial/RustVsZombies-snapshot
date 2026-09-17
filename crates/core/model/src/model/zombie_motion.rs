//! Stable-motion zombie coordinate prediction model.

use crate::model::{ZombieId, ZombieKind};

/// Current zombie motion state sampled by a backend.
///
/// This is a stable-motion conditional prediction input. It does not prove that the current
/// profile remains valid for a future horizon; callers must only use it while phase/action/field
/// interactions are known to stay stable.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZombieMotionState {
    pub id: ZombieId,
    pub kind: ZombieKind,
    pub row: i32,
    pub x: f32,
    /// Non-negative horizontal speed magnitude. Supported model variants carry direction.
    pub speed_x: f32,
    pub scale: f32,
    pub counters: ZombieMotionCounters,
    pub model: ZombieMovementModel,
}

impl ZombieMotionState {
    /// Validates backend-independent invariants for stable-motion prediction.
    pub fn validate(&self) -> Result<(), ZombieMotionError> {
        if !self.x.is_finite()
            || !self.speed_x.is_finite()
            || self.speed_x < 0.0
            || !self.scale.is_finite()
            || self.scale <= 0.0
        {
            return Err(ZombieMotionError::InvalidMotionState);
        }
        self.model.validate()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZombieMotionDirection {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZombieMotionCounters {
    pub frozen: i32,
    pub chilled: i32,
    pub buttered: i32,
}

impl ZombieMotionCounters {
    pub fn tick_down(&mut self) {
        if self.frozen > 0 {
            self.frozen -= 1;
        }
        if self.chilled > 0 {
            self.chilled -= 1;
        }
        if self.buttered > 0 {
            self.buttered -= 1;
        }
    }

    #[must_use]
    pub const fn is_immobilized(&self) -> bool {
        self.frozen > 0 || self.buttered > 0
    }

    #[must_use]
    pub const fn is_chilled(&self) -> bool {
        self.chilled > 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ZombieMovementModel {
    Track {
        profile: ZombieTrackProfile,
        /// Track animation circulation rate used by stable-motion prediction.
        progress: f32,
        /// Base playback rate, in animation frames per second, for the current movement track.
        ///
        /// Temporary counters such as chill are applied by the predictor from [`ZombieMotionCounters`]
        /// so this should represent the unchilled stable movement rate for the sampled track.
        playback_rate_fps: f32,
        direction: ZombieMotionDirection,
    },
    Uniform {
        motion: UniformZombieMotion,
        direction: ZombieMotionDirection,
    },
    Unsupported(UnsupportedZombieMotionReason),
}

impl ZombieMovementModel {
    pub fn initial_track_progress(&self) -> Result<Option<f32>, ZombieMotionError> {
        match self {
            Self::Track { progress, .. } => Ok(Some(*progress)),
            Self::Uniform { .. } => Ok(None),
            Self::Unsupported(reason) => Err(ZombieMotionError::Unsupported(*reason)),
        }
    }

    fn validate(&self) -> Result<(), ZombieMotionError> {
        match self {
            Self::Track {
                progress,
                playback_rate_fps,
                ..
            } => {
                if !progress.is_finite() || !(0.0..1.0).contains(progress) {
                    return Err(ZombieMotionError::InvalidAnimationProgress);
                }
                if !playback_rate_fps.is_finite() || *playback_rate_fps < 0.0 {
                    return Err(ZombieMotionError::InvalidMotionState);
                }
                Ok(())
            }
            Self::Uniform { .. } => Ok(()),
            Self::Unsupported(reason) => Err(ZombieMotionError::Unsupported(*reason)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZombieTrackProfile {
    NormalWalkA,
    NormalWalkB,
    NormalSwim,
    NormalDance,
    PoleVaultBeforeJump,
    PoleVaultAfterJump,
    NewspaperWalk,
    FootballWalk,
    JackBoxWalk,
    BalloonWalk,
    DiggerWalk,
    PogoWalk,
    YetiWalk,
    LadderWalk,
    GargantuarWalk,
    ImpWalk,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UniformZombieMotion {
    Zomboni,
    BalloonFlying,
    DiggerTunneling,
    PogoBouncing,
    CatapultDriving,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UniformChillPolicy {
    AffectedByChill,
    IgnoreChill,
}

impl UniformZombieMotion {
    #[must_use]
    pub const fn chill_policy(self) -> UniformChillPolicy {
        match self {
            Self::BalloonFlying | Self::PogoBouncing | Self::CatapultDriving => UniformChillPolicy::AffectedByChill,
            Self::Zomboni | Self::DiggerTunneling => UniformChillPolicy::IgnoreChill,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedZombieMotionReason {
    UnsupportedKind,
    UnsupportedPhase,
    MissingReanimation,
    InvalidReanimation,
    InvalidMotionState,
    MindControlled,
    WalkingBackwards,
    Eating,
    BlowingAway,
    FlatTires,
    YuckyFaceFrozen,
}

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum ZombieMotionError {
    #[error("unsupported zombie motion: {0:?}")]
    Unsupported(UnsupportedZombieMotionReason),
    #[error("invalid prediction horizon")]
    InvalidHorizon,
    #[error("invalid zombie motion state")]
    InvalidMotionState,
    #[error("invalid zombie motion rules")]
    InvalidMotionRules,
    #[error("invalid zombie track profile")]
    InvalidTrackProfile,
    #[error("invalid zombie animation progress")]
    InvalidAnimationProgress,
}

#[derive(Debug, thiserror::Error)]
pub enum ZombieMotionCallError {
    #[error("object is unavailable")]
    ObjectUnavailable,
    #[error(transparent)]
    Motion(ZombieMotionError),
}
