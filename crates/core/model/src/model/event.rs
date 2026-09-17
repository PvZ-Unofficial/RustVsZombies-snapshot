//! Backend-neutral native event facts and resolved public events.

use crate::model::{BattleStatus, Grid, PlantId, PlantKind, ProjectileId, ZombieId, ZombieKind, ZombiePhase};

/// Cold-path interest mask frozen before a fight starts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct EventInterest(u32);

impl EventInterest {
    pub const JACK: Self = Self(1 << 0);
    pub const BITE: Self = Self(1 << 1);
    pub const GARGANTUAR: Self = Self(1 << 2);
    pub const BASKETBALL: Self = Self(1 << 3);
    pub const HOME_ENTRY: Self = Self(1 << 4);
    pub const IMP_DIAGNOSTIC: Self = Self(1 << 5);
    pub const VEHICLE_CRUSH: Self = Self(1 << 6);
    pub const BUNGEE: Self = Self(1 << 7);
    pub const PLANT_EFFECT: Self = Self(
        Self::JACK.0 | Self::BITE.0 | Self::GARGANTUAR.0 | Self::BASKETBALL.0 | Self::VEHICLE_CRUSH.0 | Self::BUNGEE.0,
    );

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[doc(hidden)]
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    #[doc(hidden)]
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }
}

/// Native actor requesting an effect on one plant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlantEffectSource {
    Jack(ZombieId),
    Bite(ZombieId),
    Gargantuar(ZombieId),
    Basketball(ProjectileId),
    ZomboniCrush(ZombieId),
    CatapultCrush(ZombieId),
    Bungee(ZombieId),
}

impl PlantEffectSource {
    #[must_use]
    pub const fn interest(self) -> EventInterest {
        match self {
            Self::Jack(_) => EventInterest::JACK,
            Self::Bite(_) => EventInterest::BITE,
            Self::Gargantuar(_) => EventInterest::GARGANTUAR,
            Self::Basketball(_) => EventInterest::BASKETBALL,
            Self::ZomboniCrush(_) | Self::CatapultCrush(_) => EventInterest::VEHICLE_CRUSH,
            Self::Bungee(_) => EventInterest::BUNGEE,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Jack(_) => "jack_explosion",
            Self::Bite(_) => "zombie_chew",
            Self::Gargantuar(_) => "gargantuar_smash",
            Self::Basketball(_) => "catapult_basket",
            Self::ZomboniCrush(_) => "zomboni_crush",
            Self::CatapultCrush(_) => "catapult_crush",
            Self::Bungee(_) => "bungee_steal",
        }
    }
}

/// Effect requested by native game logic before rule and measurement policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlantEffect {
    HpDamage { native_requested: i32 },
    InstantKill,
    Squish,
    Steal,
}

/// Copy-only fact captured immediately before a native plant effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlantEffectAttemptFact {
    pub source: PlantEffectSource,
    pub plant_id: PlantId,
    pub raw_kind: PlantKind,
    pub effective_kind: PlantKind,
    pub grid: Grid,
    pub hp_before: i32,
    pub max_hp: i32,
    pub effect: PlantEffect,
    pub main_counter: i32,
}

/// Copy key repeated internally while pairing an attempt and its outcome.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlantEffectKey {
    pub source: PlantEffectSource,
    pub plant_id: PlantId,
    pub raw_kind: PlantKind,
    pub effective_kind: PlantKind,
    pub grid: Grid,
    pub effect: PlantEffect,
}

impl PlantEffectAttemptFact {
    #[doc(hidden)]
    #[must_use]
    pub const fn key(self) -> PlantEffectKey {
        PlantEffectKey {
            source: self.source,
            plant_id: self.plant_id,
            raw_kind: self.raw_kind,
            effective_kind: self.effective_kind,
            grid: self.grid,
            effect: self.effect,
        }
    }
}

/// Synchronous decision returned before native logic applies an effect.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EventDecision {
    Apply,
    SuppressByMeasurement,
}

/// Source of the effect decision exposed on a resolved event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EventDecisionOrigin {
    Native,
    Measurement,
}

impl EventDecision {
    #[must_use]
    pub const fn origin(self) -> EventDecisionOrigin {
        match self {
            Self::Apply => EventDecisionOrigin::Native,
            Self::SuppressByMeasurement => EventDecisionOrigin::Measurement,
        }
    }
}

/// Frame-local token used only by the native bridge.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct EventToken(u32);

impl EventToken {
    pub const INVALID: Self = Self(0);

    #[doc(hidden)]
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    #[doc(hidden)]
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.0 != 0
    }
}

/// Result of entering one instrumented native effect.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BeginPlantEffect {
    pub token: EventToken,
    pub decision: EventDecision,
}

impl BeginPlantEffect {
    pub const FAIL_OPEN: Self = Self {
        token: EventToken::INVALID,
        decision: EventDecision::Apply,
    };
}

/// Actual native result reported after an effect returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlantEffectOutcome {
    SuppressedByMeasurement,
    PreventedByRule,
    HpDelta { applied: i32 },
    Killed,
    Squished,
    Activated,
    NoEffect,
    Stolen,
}

/// Fully paired internal outcome fact.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EffectOutcomeFact {
    pub token: EventToken,
    pub key: PlantEffectKey,
    pub decision_origin: EventDecisionOrigin,
    pub outcome: PlantEffectOutcome,
}

/// Copy-only fact emitted before die-at-house/GameOver routing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HomeEntryFact {
    pub zombie_id: ZombieId,
    pub zombie_kind: ZombieKind,
    pub row: i32,
    pub main_counter: i32,
}

/// A Gargantuar entering the board while still carrying its Imp.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GargantuarSpawnedFact {
    pub parent_id: ZombieId,
    pub parent_kind: ZombieKind,
    pub from_wave: i32,
    pub row: i32,
    pub main_counter: i32,
}

/// The fully initialized Imp and its parent at the native throw point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImpThrownFact {
    pub parent_id: ZombieId,
    pub imp_id: ZombieId,
    pub parent_kind: ZombieKind,
    pub from_wave: i32,
    pub row: i32,
    pub main_counter: i32,
    pub parent_x: f32,
    pub parent_hp: i32,
    pub parent_max_hp: i32,
    pub parent_phase: ZombiePhase,
    pub parent_speed_x: f32,
    pub parent_frozen: i32,
    pub parent_chilled: i32,
    pub parent_buttered: i32,
}

/// A valid ash attack entering a Gargantuar before native damage/death routing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GargantuarAshHitFact {
    pub parent_id: ZombieId,
    pub main_counter: i32,
    pub x: f32,
    pub hp_before: i32,
    pub phase: ZombiePhase,
    pub frozen: i32,
    pub chilled: i32,
    pub buttered: i32,
}

/// A completed plant effect safe to observe from the next runtime dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlantEffectEvent {
    pub source: PlantEffectSource,
    pub plant_id: PlantId,
    pub raw_kind: PlantKind,
    pub effective_kind: PlantKind,
    pub grid: Grid,
    pub requested: PlantEffect,
    pub decision_origin: EventDecisionOrigin,
    pub outcome: PlantEffectOutcome,
    pub hp_before: i32,
    pub max_hp: i32,
    pub main_counter: i32,
}

/// A zombie crossing the house boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HomeEntryEvent {
    pub zombie_id: ZombieId,
    pub zombie_kind: ZombieKind,
    pub row: i32,
    pub main_counter: i32,
}

/// Public, resolved game event. Native pairing tokens are intentionally absent.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GameEvent {
    PlantEffect(PlantEffectEvent),
    HomeEntry(HomeEntryEvent),
}

impl From<HomeEntryFact> for HomeEntryEvent {
    fn from(fact: HomeEntryFact) -> Self {
        Self {
            zombie_id: fact.zombie_id,
            zombie_kind: fact.zombie_kind,
            row: fact.row,
            main_counter: fact.main_counter,
        }
    }
}

/// End-of-frame status passed through the native event bridge.
#[doc(hidden)]
pub type EventFrameStatus = BattleStatus;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interest_is_source_specific_and_composable() {
        let interest = EventInterest::JACK.union(EventInterest::HOME_ENTRY);
        assert!(interest.contains(EventInterest::JACK));
        assert!(interest.contains(EventInterest::HOME_ENTRY));
        assert!(!interest.contains(EventInterest::BITE));
        assert!(!interest.contains(EventInterest::IMP_DIAGNOSTIC));
        assert!(!EventInterest::default().contains(EventInterest::JACK));
    }

    #[test]
    fn resolved_event_does_not_need_a_bridge_token() {
        assert_eq!(std::mem::size_of::<GameEvent>(), 60);
        assert_eq!(EventToken::INVALID.raw(), 0);
    }
}
