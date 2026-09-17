//! Early defensive threat predicates.
//!
//! These helpers are intentionally small compatibility placeholders, not a complete automatic
//! defense strategy.

use crate::model::ZombieKind;

/// Whether this zombie is an airborne/drop threat normally handled by Blover or Umbrella Leaf.
#[must_use]
pub const fn is_airborne_or_drop_threat(kind: ZombieKind) -> bool {
    matches!(kind, ZombieKind::Balloon | ZombieKind::Bungee)
}

/// Whether this zombie commonly becomes high priority near home.
#[must_use]
pub const fn is_home_pressure_threat(kind: ZombieKind) -> bool {
    matches!(
        kind,
        ZombieKind::Digger | ZombieKind::PoleVaulting | ZombieKind::Pogo | ZombieKind::DolphinRider
    )
}
