//! Backend-neutral contact geometry value types.

use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign};

/// Native plant weapon selector used by `GetPlantAttackRect` and `GetDamageRangeFlags`.
#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlantWeapon {
    Primary = 0,
    Secondary = 1,
}

use crate::model::{CobTarget, Grid, PixelPos, PlantId, PlantKind, ZombieId, ZombieKind, ZombiePhase};

/// Backend-neutral zombie geometry input for predicted horizontal attack bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZombieGeometryInput {
    pub kind: ZombieKind,
    pub row: i32,
    pub x: f32,
    pub profile: ZombieContactProfile,
    pub orientation: ZombieGeometryOrientation,
    pub body_width: i32,
}

impl ZombieGeometryInput {
    #[must_use]
    pub const fn with_orientation(mut self, orientation: ZombieGeometryOrientation) -> Self {
        self.orientation = orientation;
        self
    }

    #[must_use]
    pub const fn mirrored(self) -> Self {
        self.with_orientation(ZombieGeometryOrientation::Mirrored)
    }

    pub fn with_body_width(mut self, width: i32) -> Result<Self, ZombieGeometryError> {
        if width <= 0 {
            return Err(ZombieGeometryError::InvalidBodyWidth);
        }
        self.body_width = width;
        Ok(self)
    }
}

/// Horizontal orientation used by PvZ zombie contact rectangles.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ZombieGeometryOrientation {
    Normal,
    Mirrored,
}

/// Backend-neutral zombie base contact profile.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ZombieContactProfile {
    CommonBody,
    NarrowBiteBody,
    Football,
    Digger,
    Snorkel,
    Ladder,
    Vehicle,
    Gargantuar,
    PoleBeforeVault,
    PoleVaulting,
    PogoMounted,
    PogoOnFoot,
    Balloon,
    DolphinRiding,
    DolphinJumping,
    DolphinOnFoot,
}

/// Zombie geometry calculation failure.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZombieGeometryError {
    InvalidCoordinate,
    InvalidBodyWidth,
    IncompatibleKindProfile,
    UnsupportedProfile,
    MissingBodyReanimationTime,
    InvalidRelativeRect,
}

/// PvZ native contact rectangle relative to a zombie's `mX`/`mY` anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RelativeContactRect {
    pub x_offset: i32,
    pub y_offset: i32,
    pub width: i32,
    pub height: i32,
}

/// Owned body-reanimation sample assembled by core motion logic and diagnostics.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZombieReanimationFacts {
    pub anim_time: f32,
    pub last_time: f32,
    pub rate: f32,
    pub frame_start: i32,
    pub frame_count: i32,
    pub loop_type: i32,
}

impl RelativeContactRect {
    #[must_use]
    pub const fn new(x_offset: i32, y_offset: i32, width: i32, height: i32) -> Self {
        Self {
            x_offset,
            y_offset,
            width,
            height,
        }
    }

    #[must_use]
    pub const fn checked_new(x_offset: i32, y_offset: i32, width: i32, height: i32) -> Option<Self> {
        let rect = Self::new(x_offset, y_offset, width, height);
        if rect.is_valid() { Some(rect) } else { None }
    }

    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.width > 0
            && self.height > 0
            && self.x_offset.checked_add(self.width).is_some()
            && self.y_offset.checked_add(self.height).is_some()
    }

    #[must_use]
    pub fn horizontal_range_at(self, x: i32) -> Option<AbsoluteContactRange> {
        let left = i64::from(x) + i64::from(self.x_offset);
        let right = left + i64::from(self.width);
        Some(AbsoluteContactRange::new(
            i32::try_from(left).ok()?,
            i32::try_from(right).ok()?,
        ))
    }

    #[must_use]
    pub fn to_absolute_rect(self, x: i32, y: i32) -> Option<ContactRect> {
        if !self.is_valid() {
            return None;
        }
        let left = i64::from(x) + i64::from(self.x_offset);
        let top = i64::from(y) + i64::from(self.y_offset);
        ContactRect::from_pos_size(
            i32::try_from(left).ok()?,
            i32::try_from(top).ok()?,
            self.width,
            self.height,
        )
    }
}

/// Backend-neutral phase categories needed by exact zombie defense geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ZombieDefensePhase {
    Other,
    ZombieDying,
    ZombieBurned,
    ZombieMowered,
    RisingFromGrave,
    DancerRising,
    DiggerRising,
    DiggerRiseWithoutAxe,
    DiggerStunned,
    DiggerWalking,
    DolphinWalking,
    DolphinIntoPool,
    DolphinRiding,
    DolphinInJump,
    DolphinWalkingInPool,
    DolphinWalkingWithoutDolphin,
    SnorkelIntoPool,
}

impl ZombieDefensePhase {
    /// Projects the exact native action phase onto the subset that changes
    /// zombie defense geometry.
    #[must_use]
    pub const fn from_zombie_phase(phase: ZombiePhase) -> Self {
        match phase {
            ZombiePhase::ZombieDying => Self::ZombieDying,
            ZombiePhase::ZombieBurned => Self::ZombieBurned,
            ZombiePhase::ZombieMowered => Self::ZombieMowered,
            ZombiePhase::RisingFromGrave => Self::RisingFromGrave,
            ZombiePhase::DiggerRising => Self::DiggerRising,
            ZombiePhase::DiggerRiseWithoutAxe => Self::DiggerRiseWithoutAxe,
            ZombiePhase::DiggerStunned => Self::DiggerStunned,
            ZombiePhase::DiggerWalking => Self::DiggerWalking,
            ZombiePhase::DolphinWalking => Self::DolphinWalking,
            ZombiePhase::DolphinIntoPool => Self::DolphinIntoPool,
            ZombiePhase::DolphinRiding => Self::DolphinRiding,
            ZombiePhase::DolphinInJump => Self::DolphinInJump,
            ZombiePhase::DolphinWalkingInPool => Self::DolphinWalkingInPool,
            ZombiePhase::DolphinWalkingWithoutDolphin => Self::DolphinWalkingWithoutDolphin,
            ZombiePhase::SnorkelIntoPool => Self::SnorkelIntoPool,
            ZombiePhase::DancerRising => Self::DancerRising,
            _ => Self::Other,
        }
    }
}

/// Backend-neutral zombie height categories used by exact defense geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ZombieHeightState {
    Normal,
    InToPool,
    OutOfPool,
    DraggedUnder,
    UpToHighGround,
    DownOffHighGround,
    UpLadder,
    Falling,
    InToChimney,
    GettingBungeeDropped,
    Zombiquarium,
    Unknown(i32),
}

/// Row-to-y policy for pure zombie defense geometry convenience calculations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ZombieDefenseRowY {
    /// Pool/fog six-row layout: `row * 85 + 50`.
    PoolLikeSixRows,
    /// Ground five-row layout: `row * 100 + 50`.
    GroundFiveRows,
    /// Explicit native zombie `mY` anchor.
    Explicit(f32),
}

impl ZombieDefenseRowY {
    #[must_use]
    pub fn resolve(self, row: i32) -> f32 {
        match self {
            Self::PoolLikeSixRows => row as f32 * 85.0 + 50.0,
            Self::GroundFiveRows => row as f32 * 100.0 + 50.0,
            Self::Explicit(y) => y,
        }
    }
}

/// Convenience input for pure zombie defense geometry calculation.
///
/// `new` defaults to the pool/fog six-row row-y layout. Use [`Self::y`] when
/// exact native `mY` is known.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZombieDefenseSpec {
    pub kind: ZombieKind,
    pub row: i32,
    pub x: f32,
    pub profile: Option<ZombieContactProfile>,
    pub orientation: ZombieGeometryOrientation,
    pub body_width: Option<i32>,
    pub row_y: ZombieDefenseRowY,
    pub phase: ZombieDefensePhase,
    pub in_pool: bool,
    pub on_high_ground: bool,
    pub is_eating: bool,
    pub zombie_height: ZombieHeightState,
    pub altitude: f32,
    pub phase_counter: i32,
    pub scale_zombie: f32,
    pub vel_z: f32,
    pub body_reanim_anim_time: Option<f32>,
    pub mind_controlled: bool,
    pub has_object: Option<bool>,
}

impl ZombieDefenseSpec {
    #[must_use]
    pub const fn new(kind: ZombieKind, row: i32, x: f32) -> Self {
        Self {
            kind,
            row,
            x,
            profile: None,
            orientation: ZombieGeometryOrientation::Normal,
            body_width: None,
            row_y: ZombieDefenseRowY::PoolLikeSixRows,
            phase: ZombieDefensePhase::Other,
            in_pool: false,
            on_high_ground: false,
            is_eating: false,
            zombie_height: ZombieHeightState::Normal,
            altitude: 0.0,
            phase_counter: 0,
            scale_zombie: 1.0,
            vel_z: 0.0,
            body_reanim_anim_time: None,
            mind_controlled: false,
            has_object: None,
        }
    }

    #[must_use]
    pub const fn profile(mut self, profile: ZombieContactProfile) -> Self {
        self.profile = Some(profile);
        self
    }

    #[must_use]
    pub const fn orientation(mut self, orientation: ZombieGeometryOrientation) -> Self {
        self.orientation = orientation;
        self
    }

    #[must_use]
    pub const fn mirrored(self) -> Self {
        self.orientation(ZombieGeometryOrientation::Mirrored)
    }

    #[must_use]
    pub const fn body_width(mut self, width: i32) -> Self {
        self.body_width = Some(width);
        self
    }

    #[must_use]
    pub const fn y(mut self, y: f32) -> Self {
        self.row_y = ZombieDefenseRowY::Explicit(y);
        self
    }

    #[must_use]
    pub const fn row_y_layout(mut self, row_y: ZombieDefenseRowY) -> Self {
        self.row_y = row_y;
        self
    }

    #[must_use]
    pub const fn phase(mut self, phase: ZombieDefensePhase) -> Self {
        self.phase = phase;
        self
    }

    #[must_use]
    pub const fn in_pool(mut self, in_pool: bool) -> Self {
        self.in_pool = in_pool;
        self
    }

    #[must_use]
    pub const fn on_high_ground(mut self, on_high_ground: bool) -> Self {
        self.on_high_ground = on_high_ground;
        self
    }

    #[must_use]
    pub const fn is_eating(mut self, is_eating: bool) -> Self {
        self.is_eating = is_eating;
        self
    }

    #[must_use]
    pub const fn zombie_height(mut self, zombie_height: ZombieHeightState) -> Self {
        self.zombie_height = zombie_height;
        self
    }

    #[must_use]
    pub const fn altitude(mut self, altitude: f32) -> Self {
        self.altitude = altitude;
        self
    }

    #[must_use]
    pub const fn phase_counter(mut self, phase_counter: i32) -> Self {
        self.phase_counter = phase_counter;
        self
    }

    #[must_use]
    pub const fn scale_zombie(mut self, scale_zombie: f32) -> Self {
        self.scale_zombie = scale_zombie;
        self
    }

    #[must_use]
    pub const fn vel_z(mut self, vel_z: f32) -> Self {
        self.vel_z = vel_z;
        self
    }

    #[must_use]
    pub const fn body_reanim_anim_time(mut self, anim_time: f32) -> Self {
        self.body_reanim_anim_time = Some(anim_time);
        self
    }

    #[must_use]
    pub const fn mind_controlled(mut self, mind_controlled: bool) -> Self {
        self.mind_controlled = mind_controlled;
        self
    }

    #[must_use]
    pub const fn has_object(mut self, has_object: bool) -> Self {
        self.has_object = Some(has_object);
        self
    }
}

/// Complete backend-neutral input for exact zombie defense rectangle calculation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZombieDefenseState {
    pub kind: ZombieKind,
    pub row: i32,
    pub phase: ZombieDefensePhase,
    pub x: i32,
    pub y: i32,
    pub body_width: i32,
    pub base_rect: RelativeContactRect,
    pub mind_controlled: bool,
    pub has_object: bool,
    pub in_pool: bool,
    pub on_high_ground: bool,
    pub is_eating: bool,
    pub zombie_height: ZombieHeightState,
    pub altitude: f32,
    pub phase_counter: i32,
    pub scale_zombie: f32,
    pub vel_z: f32,
    pub body_reanim_anim_time: Option<f32>,
}

impl ZombieDefenseState {
    pub fn from_base_rect(
        kind: ZombieKind, row: i32, x: i32, y: i32, body_width: i32, base_rect: RelativeContactRect,
    ) -> Result<Self, ZombieGeometryError> {
        if body_width <= 0 {
            return Err(ZombieGeometryError::InvalidBodyWidth);
        }
        if !base_rect.is_valid() {
            return Err(ZombieGeometryError::InvalidRelativeRect);
        }
        Ok(Self {
            kind,
            row,
            phase: ZombieDefensePhase::Other,
            x,
            y,
            body_width,
            base_rect,
            mind_controlled: false,
            has_object: false,
            in_pool: false,
            on_high_ground: false,
            is_eating: false,
            zombie_height: ZombieHeightState::Normal,
            altitude: 0.0,
            phase_counter: 0,
            scale_zombie: 1.0,
            vel_z: 0.0,
            body_reanim_anim_time: None,
        })
    }

    #[must_use]
    pub const fn with_phase(mut self, phase: ZombieDefensePhase) -> Self {
        self.phase = phase;
        self
    }

    #[must_use]
    pub const fn with_pool_state(mut self, in_pool: bool) -> Self {
        self.in_pool = in_pool;
        self
    }

    #[must_use]
    pub const fn with_high_ground(mut self, on_high_ground: bool) -> Self {
        self.on_high_ground = on_high_ground;
        self
    }

    #[must_use]
    pub const fn with_altitude(mut self, altitude: f32) -> Self {
        self.altitude = altitude;
        self
    }

    #[must_use]
    pub const fn with_body_reanim_anim_time(mut self, anim_time: f32) -> Self {
        self.body_reanim_anim_time = Some(anim_time);
        self
    }
}

/// Defense-relevant subset of PvZ `ZombieDrawPosition`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZombieDefenseDrawPose {
    pub body_y: f32,
    pub clip_height: f32,
}

/// Pure predicted zombie horizontal attack geometry without a live object id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PredictedZombieAttackBounds {
    pub kind: ZombieKind,
    pub row: i32,
    pub x: i32,
    pub range: AbsoluteContactRange,
}

impl PredictedZombieAttackBounds {
    #[must_use]
    pub const fn with_id(self, id: ZombieId) -> ZombieAttackBounds {
        ZombieAttackBounds {
            id,
            kind: self.kind,
            row: self.row,
            x: self.x,
            range: self.range,
        }
    }
}

/// Pure zombie defense geometry without a live object id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ZombieDefenseGeometry {
    pub kind: ZombieKind,
    pub row: i32,
    pub x: i32,
    pub rect: ContactRect,
}

impl ZombieDefenseGeometry {
    #[must_use]
    pub const fn with_id(self, id: ZombieId) -> ZombieDefenseBounds {
        ZombieDefenseBounds {
            id,
            kind: self.kind,
            row: self.row,
            x: self.x,
            rect: self.rect,
        }
    }
}

/// Horizontal contact interval relative to an object's native `x` coordinate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RelativeContactRange {
    pub left_offset: i32,
    pub right_offset: i32,
}

impl RelativeContactRange {
    #[must_use]
    pub const fn new(left_offset: i32, right_offset: i32) -> Self {
        Self {
            left_offset,
            right_offset,
        }
    }

    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.left_offset <= self.right_offset
    }

    #[must_use]
    pub fn to_absolute(self, x: i32) -> Option<AbsoluteContactRange> {
        let left = i64::from(x) + i64::from(self.left_offset);
        let right = i64::from(x) + i64::from(self.right_offset);
        Some(AbsoluteContactRange {
            left: i32::try_from(left).ok()?,
            right: i32::try_from(right).ok()?,
        })
    }
}

/// Horizontal contact interval in board pixel coordinates.
///
/// The `right` endpoint follows PvZ native rectangle semantics: `x + width`, not
/// `x + width - 1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AbsoluteContactRange {
    pub left: i32,
    pub right: i32,
}

impl AbsoluteContactRange {
    #[must_use]
    pub const fn new(left: i32, right: i32) -> Self {
        Self { left, right }
    }

    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.left <= self.right
    }
}

/// PvZ-style integer contact rectangle.
///
/// `right` and `bottom` are native endpoints (`position + size`), not inclusive
/// pixel indexes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContactRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl ContactRect {
    #[must_use]
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    /// Builds a non-empty native-endpoint rectangle.
    #[must_use]
    pub const fn checked_new(left: i32, top: i32, right: i32, bottom: i32) -> Option<Self> {
        if left < right && top < bottom {
            Some(Self::new(left, top, right, bottom))
        } else {
            None
        }
    }

    #[must_use]
    pub fn from_pos_size(x: i32, y: i32, width: i32, height: i32) -> Option<Self> {
        let right = i64::from(x) + i64::from(width);
        let bottom = i64::from(y) + i64::from(height);
        Some(Self {
            left: x,
            top: y,
            right: i32::try_from(right).ok()?,
            bottom: i32::try_from(bottom).ok()?,
        })
    }

    /// Builds a non-empty native-endpoint rectangle and rejects endpoint overflow.
    #[must_use]
    pub fn checked_from_pos_size(x: i32, y: i32, width: i32, height: i32) -> Option<Self> {
        if width <= 0 || height <= 0 {
            return None;
        }
        Self::from_pos_size(x, y, width, height)
    }

    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.left <= self.right && self.top <= self.bottom
    }

    #[must_use]
    pub const fn horizontal_range(self) -> AbsoluteContactRange {
        AbsoluteContactRange::new(self.left, self.right)
    }
}

/// Circular contact or explosion shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContactCircle {
    pub x: i32,
    pub y: i32,
    pub radius: i32,
}

impl ContactCircle {
    #[must_use]
    pub const fn new(x: i32, y: i32, radius: i32) -> Self {
        Self { x, y, radius }
    }

    #[must_use]
    pub const fn center(self) -> PixelPos {
        PixelPos { x: self.x, y: self.y }
    }

    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.radius >= 0
    }
}

/// Retained-bit native zombie damage eligibility flags.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DamageRangeFlags(u32);

impl DamageRangeFlags {
    pub const EMPTY: Self = Self(0);
    pub const GROUND: Self = Self(1 << 0);
    pub const FLYING: Self = Self(1 << 1);
    pub const SUBMERGED: Self = Self(1 << 2);
    pub const DOG: Self = Self(1 << 3);
    pub const OFF_GROUND: Self = Self(1 << 4);
    pub const DYING: Self = Self(1 << 5);
    pub const UNDERGROUND: Self = Self(1 << 6);
    pub const ONLY_MINDCONTROLLED: Self = Self(1 << 7);
    pub const ONLY_MIND_CONTROLLED: Self = Self::ONLY_MINDCONTROLLED;
    pub const ALL_NON_MIND_CONTROLLED: Self = Self(127);

    #[must_use]
    pub const fn empty() -> Self {
        Self::EMPTY
    }

    #[must_use]
    pub const fn from_bits_retain(bits: u32) -> Self {
        Self(bits)
    }

    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }
}

impl BitOr for DamageRangeFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for DamageRangeFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for DamageRangeFlags {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl BitAndAssign for DamageRangeFlags {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

/// MVP plant defense interval kind.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlantDefenseKind {
    ChewCrushSmash,
}

/// Contact rectangle query result for a live plant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlantContactRect {
    pub id: PlantId,
    pub kind: PlantKind,
    pub grid: Grid,
    pub rect: ContactRect,
}

/// SmartRemove-style plant defense interval.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlantDefenseBounds {
    pub id: PlantId,
    pub kind: PlantKind,
    pub defense_kind: PlantDefenseKind,
    pub grid: Grid,
    pub x: i32,
    pub range: AbsoluteContactRange,
}

/// Alive-only current zombie horizontal attack geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ZombieAttackBounds {
    pub id: ZombieId,
    pub kind: ZombieKind,
    pub row: i32,
    pub x: i32,
    pub range: AbsoluteContactRange,
}

/// Alive-only zombie defense rectangle for geometry-only hits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ZombieDefenseBounds {
    pub id: ZombieId,
    pub kind: ZombieKind,
    pub row: i32,
    pub x: i32,
    pub rect: ContactRect,
}

/// MVP zombie plant-threat candidate kinds.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ZombiePlantThreatKind {
    DriveOver,
    GargantuarSmash,
    JackExplosion,
}

/// Candidate plant-threat geometry for a live zombie.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ZombieThreatCandidateShape {
    pub id: ZombieId,
    pub kind: ZombieKind,
    pub threat_kind: ZombiePlantThreatKind,
    pub shape: ZombiePlantThreatShape,
}

/// Backend-sampled facts for a zombie plant-threat candidate.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ZombieThreatCandidateFacts {
    SameRowAttack {
        id: ZombieId,
        kind: ZombieKind,
        threat_kind: ZombiePlantThreatKind,
        geometry: ZombieGeometryInput,
    },
    Circle {
        id: ZombieId,
        kind: ZombieKind,
        threat_kind: ZombiePlantThreatKind,
        circle: ContactCircle,
    },
}

/// Geometry shape used by zombie plant-threat candidates.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ZombiePlantThreatShape {
    SameRowRange { row: i32, range: AbsoluteContactRange },
    Circle(ContactCircle),
}

/// MVP grid explosion kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GridExplosionKind {
    CherryBomb,
    DoomShroom,
}

pub const CHERRY_BOMB_RADIUS: i32 = 115;
pub const DOOM_SHROOM_RADIUS: i32 = 250;

/// Plant-originated zombie-threat shape kinds.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlantThreatKind {
    CherryBomb,
    DoomShroom,
    StaticCobCannonImpact,
}

/// Circular plant threat used by cherry, doom, and static cob approximation queries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlantThreatShape {
    pub kind: PlantThreatKind,
    pub center_row: i32,
    pub row_range: i32,
    pub damage_flags: DamageRangeFlags,
    pub circle: ContactCircle,
}

impl PlantThreatShape {
    #[must_use]
    pub const fn new(
        kind: PlantThreatKind, center_row: i32, row_range: i32, damage_flags: DamageRangeFlags, circle: ContactCircle,
    ) -> Self {
        Self {
            kind,
            center_row,
            row_range,
            damage_flags,
            circle,
        }
    }
}

/// Request data for a static cob impact approximation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StaticCobImpactRequest {
    pub target: CobTarget,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_range_flags_keep_native_bits() {
        assert_eq!(DamageRangeFlags::empty(), DamageRangeFlags::EMPTY);
        assert!(DamageRangeFlags::empty().is_empty());
        assert_eq!(DamageRangeFlags::ALL_NON_MIND_CONTROLLED.bits(), 127);
        assert_eq!(DamageRangeFlags::DOG.bits(), 1 << 3);
        assert_eq!(DamageRangeFlags::ONLY_MINDCONTROLLED.bits(), 1 << 7);
        assert!(DamageRangeFlags::ALL_NON_MIND_CONTROLLED.contains(DamageRangeFlags::DOG));
        assert!(!DamageRangeFlags::ALL_NON_MIND_CONTROLLED.intersects(DamageRangeFlags::ONLY_MIND_CONTROLLED));

        let custom = DamageRangeFlags::GROUND | DamageRangeFlags::ONLY_MIND_CONTROLLED;
        assert_eq!(custom.bits(), 129);
    }

    #[test]
    fn relative_range_conversion_uses_checked_wide_arithmetic() {
        let range = RelativeContactRange::new(-5, 10)
            .to_absolute(20)
            .expect("range fits i32");
        assert_eq!(range, AbsoluteContactRange::new(15, 30));

        assert!(RelativeContactRange::new(0, 1).to_absolute(i32::MAX).is_none());

        let rect = RelativeContactRect::new(-5, 10, 15, 20);
        assert_eq!(rect.horizontal_range_at(20), Some(AbsoluteContactRange::new(15, 30)));
        assert_eq!(rect.to_absolute_rect(20, 30), Some(ContactRect::new(15, 40, 30, 60)));
        assert!(
            RelativeContactRect::new(0, 0, 1, 1)
                .horizontal_range_at(i32::MAX)
                .is_none()
        );
        assert!(RelativeContactRect::new(0, 0, -1, 1).to_absolute_rect(0, 0).is_none());
    }

    #[test]
    fn contact_rect_from_pos_size_uses_native_endpoint_semantics() {
        assert!(RelativeContactRange::new(-10, 20).is_valid());
        assert!(!RelativeContactRange::new(20, -10).is_valid());
        assert!(RelativeContactRect::new(0, 0, 1, 1).is_valid());
        assert!(!RelativeContactRect::new(0, 0, 0, 1).is_valid());
        assert!(!RelativeContactRect::new(0, 0, 1, 0).is_valid());
        assert!(AbsoluteContactRange::new(1, 1).is_valid());
        assert!(!AbsoluteContactRange::new(2, 1).is_valid());
        assert!(ContactRect::new(0, 0, 10, 10).is_valid());
        assert!(!ContactRect::new(10, 0, 0, 10).is_valid());
        assert!(ContactCircle::new(0, 0, 0).is_valid());
        assert!(!ContactCircle::new(0, 0, -1).is_valid());
        assert_eq!(
            ContactRect::from_pos_size(10, 20, 30, 40),
            Some(ContactRect::new(10, 20, 40, 60))
        );
        assert!(ContactRect::from_pos_size(i32::MAX, 0, 1, 1).is_none());
        assert!(ContactRect::checked_from_pos_size(0, 0, -1, 1).is_none());
        assert!(ContactRect::checked_from_pos_size(0, 0, 0, 1).is_none());
        assert!(ContactRect::checked_new(0, 0, 1, 1).is_some());
        assert!(ContactRect::checked_new(0, 0, 0, 1).is_none());
        assert!(RelativeContactRect::checked_new(0, 0, 1, 1).is_some());
        assert!(RelativeContactRect::checked_new(0, 0, 0, 1).is_none());
        assert!(RelativeContactRect::checked_new(i32::MAX, 0, 1, 1).is_none());
    }

    #[test]
    fn defense_phase_is_an_explicit_lossy_geometry_projection() {
        assert_eq!(
            ZombieDefensePhase::from_zombie_phase(ZombiePhase::DolphinInJump),
            ZombieDefensePhase::DolphinInJump
        );
        assert_eq!(
            ZombieDefensePhase::from_zombie_phase(ZombiePhase::DiggerWalking),
            ZombieDefensePhase::DiggerWalking
        );
        for phase in [
            ZombiePhase::ZombieNormal,
            ZombiePhase::JackInTheBoxRunning,
            ZombiePhase::PogoBouncing,
            ZombiePhase::BossIdle,
        ] {
            assert_eq!(ZombieDefensePhase::from_zombie_phase(phase), ZombieDefensePhase::Other);
        }
    }
}
