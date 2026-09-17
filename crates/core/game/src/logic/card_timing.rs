//! Registration-time plans for card primitives expressed by semantic effect time.

use crate::model::{CardSelection, PlantKind};

pub const EFFECT_COUNTDOWN_TARGET: i32 = 10;
/// Native delay from planting an imitator placeholder to creating its target plant.
pub const IMITATOR_MORPH_DELAY: i32 = 320;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RetentionKind {
    Keep(i32),
    To(i32),
}

/// Cleanup timing attached to one planted-card operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Retention(RetentionKind);

impl Retention {
    #[must_use]
    pub const fn keep(frames: i32) -> Self {
        Self(RetentionKind::Keep(frames))
    }

    #[must_use]
    pub const fn to(time: i32) -> Self {
        Self(RetentionKind::To(time))
    }

    pub fn requested_cleanup_time(self, semantic_time: i128) -> Result<i128, String> {
        match self.0 {
            RetentionKind::Keep(frames) => semantic_time
                .checked_add(i128::from(frames))
                .ok_or_else(|| "retention cleanup time overflowed i128".to_owned()),
            RetentionKind::To(time) => Ok(i128::from(time)),
        }
    }

    pub fn cleanup_time(self, semantic_time: i128, placement_time: i128) -> Result<i128, String> {
        let cleanup_time = self.requested_cleanup_time(semantic_time)?;
        if cleanup_time < placement_time {
            return Err(format!(
                "retention cleanup time {cleanup_time} is earlier than placement time {placement_time}"
            ));
        }
        Ok(cleanup_time)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimpleCardEffectTiming {
    pub selection: CardSelection,
    pub placement_lead: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MushroomEffectTiming {
    pub selection: CardSelection,
    pub kind: PlantKind,
    pub day_placement_lead: i32,
    pub day_coffee_lead: i32,
    pub night_placement_lead: i32,
    pub normalize_lead: i32,
}

/// Returns the AvZ-compatible placement lead for a simple effect-time card.
#[must_use]
pub const fn simple_card_effect_timing(kind: PlantKind) -> Option<SimpleCardEffectTiming> {
    let placement_lead = match kind {
        PlantKind::CherryBomb | PlantKind::Jalapeno => 100,
        PlantKind::Squash => 182,
        _ => return None,
    };
    Some(SimpleCardEffectTiming {
        selection: CardSelection::Plant(kind),
        placement_lead,
    })
}

/// Returns the fixed day/night callback plan for ice- and doom-shroom shorthand.
#[must_use]
pub const fn mushroom_effect_timing(selection: CardSelection) -> Option<MushroomEffectTiming> {
    let (kind, day_placement_lead, day_coffee_lead, night_placement_lead) = match selection {
        CardSelection::Plant(PlantKind::IceShroom) => (PlantKind::IceShroom, 299, 299, 100),
        CardSelection::Imitator(PlantKind::IceShroom) => (PlantKind::IceShroom, 619, 299, 420),
        CardSelection::Plant(PlantKind::DoomShroom) => (PlantKind::DoomShroom, 299, 299, 100),
        _ => return None,
    };
    Some(MushroomEffectTiming {
        selection,
        kind,
        day_placement_lead,
        day_coffee_lead,
        night_placement_lead,
        normalize_lead: EFFECT_COUNTDOWN_TARGET,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_effect_card_leads_match_avz_shorthand() {
        assert_eq!(
            simple_card_effect_timing(PlantKind::CherryBomb)
                .expect("cherry")
                .placement_lead,
            100
        );
        assert_eq!(
            simple_card_effect_timing(PlantKind::Jalapeno)
                .expect("jalapeno")
                .placement_lead,
            100
        );
        assert_eq!(
            simple_card_effect_timing(PlantKind::Squash)
                .expect("squash")
                .placement_lead,
            182
        );
    }

    #[test]
    fn mushroom_plans_distinguish_ordinary_and_imitator_ice() {
        let ordinary = mushroom_effect_timing(CardSelection::Plant(PlantKind::IceShroom)).expect("ordinary ice");
        let imitator = mushroom_effect_timing(CardSelection::Imitator(PlantKind::IceShroom)).expect("imitator ice");
        assert_eq!(ordinary.day_placement_lead, 299);
        assert_eq!(ordinary.night_placement_lead, 100);
        assert_eq!(imitator.day_placement_lead, 619);
        assert_eq!(imitator.day_coffee_lead, 299);
        assert_eq!(imitator.night_placement_lead, 420);
        assert_eq!(imitator.normalize_lead, 10);
    }

    #[test]
    fn daytime_doom_uses_avz_shorthand_effect_lead() {
        let doom = mushroom_effect_timing(CardSelection::Plant(PlantKind::DoomShroom)).expect("doom");
        assert_eq!(doom.day_placement_lead, 299);
        assert_eq!(doom.day_coffee_lead, 299);
    }
}

pub mod receipt;

/// Error without frontend wave context; retained in operation order.
#[derive(Debug, PartialEq, Eq)]
pub enum EffectTimeError {
    OutsideRange(&'static str),
    Overflow(&'static str),
    Retention(String),
}

pub fn mushroom_times(
    semantic_time: i128, timing: MushroomEffectTiming,
) -> Result<(i128, i128, i128, i128), Vec<EffectTimeError>> {
    let mut errors = Vec::new();
    let day_placement = checked_subtract(
        semantic_time,
        timing.day_placement_lead,
        "day mushroom placement",
        &mut errors,
    );
    let day_coffee = checked_subtract(
        semantic_time,
        timing.day_coffee_lead,
        "day mushroom coffee",
        &mut errors,
    );
    let night_placement = checked_subtract(
        semantic_time,
        timing.night_placement_lead,
        "night mushroom placement",
        &mut errors,
    );
    let normalize_time = checked_subtract(
        semantic_time,
        timing.normalize_lead,
        "mushroom effect normalization",
        &mut errors,
    );
    if errors.is_empty() {
        Ok((
            day_placement.expect("validated day placement"),
            day_coffee.expect("validated day coffee"),
            night_placement.expect("validated night placement"),
            normalize_time.expect("validated normalization"),
        ))
    } else {
        Err(errors)
    }
}

pub fn coffee_ice_times(semantic_time: i128) -> Result<(i32, i128, i128), Vec<EffectTimeError>> {
    let mut errors = Vec::new();
    let effect_time = match i32::try_from(semantic_time) {
        Ok(time) => Some(time),
        Err(_) => {
            errors.push(EffectTimeError::OutsideRange("coffee-ice effect"));
            None
        }
    };
    let wake_time = checked_subtract(
        semantic_time,
        crate::logic::ice_filler::COFFEE_ICE_LEAD,
        "coffee-ice wake",
        &mut errors,
    );
    let normalize_time = checked_subtract(
        semantic_time,
        EFFECT_COUNTDOWN_TARGET,
        "coffee-ice normalization",
        &mut errors,
    );
    if errors.is_empty() {
        Ok((
            effect_time.expect("validated effect time"),
            wake_time.expect("validated wake time"),
            normalize_time.expect("validated normalization time"),
        ))
    } else {
        Err(errors)
    }
}

pub(crate) fn checked_subtract(
    time: i128, lead: i32, label: &'static str, errors: &mut Vec<EffectTimeError>,
) -> Option<i128> {
    let time = time.checked_sub(i128::from(lead));
    match time.filter(|time| i32::try_from(*time).is_ok()) {
        Some(time) => Some(time),
        None => {
            errors.push(EffectTimeError::OutsideRange(label));
            None
        }
    }
}
