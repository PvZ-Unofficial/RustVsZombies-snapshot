//! Internal effect-time card execution shared by script frontends.
//!
//! These operations emit into the caller's existing registration batch. They do
//! not bind a second timeline or own DSL wave/error context.

use crate::logic::card_timing::receipt::{RetentionState, cleanup_retained, resolve_imitator_successor};
use crate::logic::card_timing::{
    EFFECT_COUNTDOWN_TARGET, EffectTimeError, IMITATOR_MORPH_DELAY, MushroomEffectTiming, Retention, checked_subtract,
    coffee_ice_times, mushroom_times,
};
use crate::logic::cards::{self as core_cards, CardContext, PlantingComponent};
use crate::runtime::{RuntimeError, RuntimeResult};
use rsvz_backend_api::{ImitatorMorphBackend, PlantEffectCountdownWriteBackend, PlantReadBackend, SceneBackend};
use rsvz_current::CurrentBackend;
use rsvz_model::{CardSelection, Grid, PlantId, PlantKind};
use std::cell::Cell;
use std::rc::Rc;

type EffectCallback = Box<dyn FnMut() -> RuntimeResult<()> + 'static>;
type MorphResolver = fn(&Cell<RetentionState>) -> RuntimeResult<()>;

pub struct SimpleEffect {
    pub selection: CardSelection,
    pub placement_lead: i32,
    pub retention: Option<Retention>,
    pub row: i32,
    pub col: i32,
}

impl SimpleEffect
where
    CurrentBackend: CardContext + 'static,
{
    pub fn prepare(
        &self, semantic_time: i128, mut push: impl FnMut(i128, EffectCallback),
    ) -> Result<(), Vec<EffectTimeError>> {
        let mut errors = Vec::new();
        let raw_placement = semantic_time.checked_sub(i128::from(self.placement_lead));
        let placement_time = checked_subtract(semantic_time, self.placement_lead, "effect card placement", &mut errors);
        let cleanup_time = match self.retention {
            Some(retention) => match raw_placement.map_or_else(
                || retention.requested_cleanup_time(semantic_time),
                |placement| retention.cleanup_time(semantic_time, placement),
            ) {
                Ok(time) => Some(time),
                Err(error) => {
                    errors.push(EffectTimeError::Retention(error));
                    None
                }
            },
            None => None,
        };
        if !errors.is_empty() {
            return Err(errors);
        }
        let Some(placement_time) = placement_time else {
            return Ok(());
        };
        let selection = self.selection;
        let row = self.row;
        let col = self.col;
        if let Some(cleanup_time) = cleanup_time {
            let state = Rc::new(Cell::new(RetentionState::default()));
            let placement_state = Rc::clone(&state);
            push(
                placement_time,
                Box::new(move || {
                    {
                        core_cards::card_recording(selection, row, col, |component, id| {
                            record_component(&placement_state, component, id)
                        })
                    }
                    .map(|_receipt| ())
                    .map_err(Into::into)
                }),
            );
            push(cleanup_time, Box::new(move || cleanup_retained(&state)));
        } else {
            push(
                placement_time,
                Box::new(move || core_cards::card(selection, row, col).map(|_id| ()).map_err(Into::into)),
            );
        }
        Ok(())
    }
}

pub struct MushroomEffect {
    pub timing: crate::logic::card_timing::MushroomEffectTiming,
    pub retention: Option<Retention>,
    morph_resolver: Option<MorphResolver>,
    pub row: i32,
    pub col: i32,
}

impl MushroomEffect
where
    CurrentBackend: CardContext + SceneBackend + PlantEffectCountdownWriteBackend + 'static,
{
    pub fn prepare(
        &self, semantic_time: i128, mut push: impl FnMut(i128, EffectCallback),
    ) -> Result<(), Vec<EffectTimeError>> {
        let (times, mut errors) = match mushroom_times(semantic_time, self.timing) {
            Ok(times) => (Some(times), Vec::new()),
            Err(errors) => (None, errors),
        };
        // The scene is only known when callbacks run.  Retention therefore has
        // to be valid for both branches, including the later placement.
        let raw_day_placement = semantic_time.checked_sub(i128::from(self.timing.day_placement_lead));
        let raw_night_placement = semantic_time.checked_sub(i128::from(self.timing.night_placement_lead));
        let latest_placement = raw_day_placement
            .zip(raw_night_placement)
            .map(|(day, night)| day.max(night));
        let cleanup_time = match self.retention {
            Some(retention) => match latest_placement.map_or_else(
                || retention.requested_cleanup_time(semantic_time),
                |placement| retention.cleanup_time(semantic_time, placement),
            ) {
                Ok(time) => Some(time),
                Err(error) => {
                    errors.push(EffectTimeError::Retention(error));
                    None
                }
            },
            None => None,
        };
        if !errors.is_empty() {
            return Err(errors);
        }
        let Some((day_placement, day_coffee, night_placement, normalize_time)) = times else {
            return Ok(());
        };

        let state = Rc::new(Cell::new(RetentionState::default()));
        let day_state = Rc::clone(&state);
        let selection = self.timing.selection;
        let morph_resolver = self.morph_resolver;
        let row = self.row;
        let col = self.col;
        push(
            day_placement,
            Box::new(move || {
                let scene = crate::access::with_backend(|backend| backend.scene()).map_err(runtime_error)?;
                if scene.is_night() {
                    return Ok(());
                }
                plant_recorded(&day_state, selection, row, col)
            }),
        );

        let coffee_state = Rc::clone(&state);
        push(
            day_coffee,
            Box::new(move || {
                let scene = crate::access::with_backend(|backend| backend.scene()).map_err(runtime_error)?;
                if scene.is_night() {
                    return Ok(());
                }
                if let Some(resolve_morph) = morph_resolver {
                    resolve_morph(&coffee_state)?;
                }
                let Some(main) = coffee_state.get().main else {
                    return Ok(());
                };
                crate::access::with_backend(|backend| {
                    if crate::live_value::read_or_abort(backend.plant(main), "plant").is_none() {
                        return Ok(());
                    }
                    core_cards::card(CardSelection::Plant(PlantKind::CoffeeBean), row, col)
                        .map(|_id| ())
                        .map_err(Into::into)
                })
            }),
        );

        let night_state = Rc::clone(&state);
        push(
            night_placement,
            Box::new(move || {
                let scene = crate::access::with_backend(|backend| backend.scene()).map_err(runtime_error)?;
                if !scene.is_night() {
                    return Ok(());
                }
                plant_recorded(&night_state, selection, row, col)
            }),
        );

        if let Some(resolve_morph) = morph_resolver {
            let Some(night_morph) = night_placement.checked_add(i128::from(IMITATOR_MORPH_DELAY)) else {
                return Err(vec![EffectTimeError::Overflow("imitator morph")]);
            };
            let night_morph_state = Rc::clone(&state);
            push(
                night_morph,
                Box::new(move || {
                    let scene = crate::access::with_backend(|backend| backend.scene()).map_err(runtime_error)?;
                    if !scene.is_night() {
                        return Ok(());
                    }
                    resolve_morph(&night_morph_state)
                }),
            );
        }

        let normalize_state = Rc::clone(&state);
        push(
            normalize_time,
            Box::new(move || {
                let Some(main) = normalize_state.get().main else {
                    return Ok(());
                };
                crate::modifier::normalize_effect_countdown_by_id(main, EFFECT_COUNTDOWN_TARGET)
                    .map(|_outcome| ())
                    .map_err(runtime_error)
            }),
        );

        if let Some(cleanup_time) = cleanup_time {
            push(cleanup_time, Box::new(move || cleanup_retained(&state)));
        }
        Ok(())
    }
}

pub struct CoffeeIceEffect;

impl CoffeeIceEffect
where
    CurrentBackend: CardContext + PlantEffectCountdownWriteBackend + 'static,
{
    pub fn prepare(
        &self, semantic_time: i128, mut push: impl FnMut(i128, EffectCallback),
    ) -> Result<(), Vec<EffectTimeError>> {
        let (effect_time, wake_time, normalize_time) = match coffee_ice_times(semantic_time) {
            Ok(times) => times,
            Err(errors) => {
                return Err(errors);
            }
        };
        let timing = crate::logic::ice_filler::coffee_ice_timing(effect_time);
        debug_assert_eq!(
            i128::from(timing.wake_time),
            wake_time,
            "coffee-ice timing must preserve the computed wake time"
        );
        let state = Rc::new(Cell::new(None::<PlantId>));
        let coffee_state = Rc::clone(&state);
        push(
            wake_time,
            Box::new(move || {
                let grid = crate::ice_filler::coffee_grid()?;
                let Some(grid) = grid else {
                    return Ok(());
                };
                let id = crate::logic::cob::find_plant_at_kind(grid, PlantKind::IceShroom);
                coffee_state.set(id);
                Ok(())
            }),
        );
        push(
            normalize_time,
            Box::new(move || {
                let Some(id) = state.get() else {
                    return Ok(());
                };
                crate::modifier::normalize_effect_countdown_by_id(id, EFFECT_COUNTDOWN_TARGET)
                    .map(|_outcome| ())
                    .map_err(runtime_error)
            }),
        );
        Ok(())
    }
}

pub struct ExplicitCoffeeIceEffect {
    pub row: i32,
    pub col: i32,
}

impl ExplicitCoffeeIceEffect
where
    CurrentBackend: CardContext + PlantEffectCountdownWriteBackend + 'static,
{
    pub fn prepare(
        &self, semantic_time: i128, mut push: impl FnMut(i128, EffectCallback),
    ) -> Result<(), Vec<EffectTimeError>> {
        let (effect_time, wake_time, normalize_time) = match coffee_ice_times(semantic_time) {
            Ok(times) => times,
            Err(errors) => {
                return Err(errors);
            }
        };
        let timing = crate::logic::ice_filler::coffee_ice_timing(effect_time);
        debug_assert_eq!(
            i128::from(timing.wake_time),
            wake_time,
            "coffee-ice timing must preserve the computed wake time"
        );

        let grid = Grid::from_one_based(self.row, self.col).expect("explicit coffee-ice grid was validated");
        let state = Rc::new(Cell::new(None::<PlantId>));
        let coffee_state = Rc::clone(&state);
        let row = self.row;
        let col = self.col;
        push(
            wake_time,
            Box::new(move || {
                let _coffee = core_cards::card(CardSelection::Plant(PlantKind::CoffeeBean), row, col)?;
                let id = crate::logic::cob::find_plant_at_kind(grid, PlantKind::IceShroom);
                coffee_state.set(id);
                Ok(())
            }),
        );
        push(
            normalize_time,
            Box::new(move || {
                let Some(id) = state.get() else {
                    return Ok(());
                };
                crate::modifier::normalize_effect_countdown_by_id(id, EFFECT_COUNTDOWN_TARGET)
                    .map(|_outcome| ())
                    .map_err(runtime_error)
            }),
        );
        Ok(())
    }
}

impl MushroomEffect {
    pub fn plain(timing: MushroomEffectTiming, retention: Option<Retention>, row: i32, col: i32) -> Self {
        Self {
            timing,
            retention,
            row,
            col,
            morph_resolver: None,
        }
    }

    pub fn imitator(timing: MushroomEffectTiming, retention: Option<Retention>, row: i32, col: i32) -> Self
    where
        CurrentBackend: ImitatorMorphBackend,
    {
        Self {
            morph_resolver: matches!(timing.selection, CardSelection::Imitator(_))
                .then_some(resolve_imitator_successor as MorphResolver),
            ..Self::plain(timing, retention, row, col)
        }
    }
}
fn plant_recorded(
    state: &Cell<RetentionState>, selection: CardSelection, row: i32, col: i32,
) -> Result<(), RuntimeError>
where
    CurrentBackend: CardContext,
{
    {
        core_cards::card_recording(selection, row, col, |component, id| {
            record_component(state, component, id);
        })
    }
    .map(|_receipt| ())
    .map_err(Into::into)
}

fn record_component(state: &Cell<RetentionState>, component: PlantingComponent, id: PlantId) {
    let mut current = state.get();
    match component {
        PlantingComponent::AutoContainer => current.container = Some(id),
        PlantingComponent::Main => current.main = Some(id),
    }
    state.set(current);
}

fn runtime_error(error: impl std::fmt::Display) -> RuntimeError {
    RuntimeError::new(error.to_string())
}

pub struct RetainedCardEffect {
    pub selection: CardSelection,
    pub row: i32,
    pub col: i32,
    pub retention: Retention,
}

impl RetainedCardEffect
where
    CurrentBackend: CardContext + ImitatorMorphBackend + 'static,
{
    pub fn prepare(
        &self, semantic_time: i128, mut push: impl FnMut(i128, EffectCallback),
    ) -> Result<(), Vec<EffectTimeError>> {
        let mut errors = Vec::new();
        let cleanup_time = match self.retention.cleanup_time(semantic_time, semantic_time) {
            Ok(time) => Some(time),
            Err(error) => {
                errors.push(EffectTimeError::Retention(error));
                None
            }
        };
        let resolution_time = if matches!(self.selection, CardSelection::Imitator(_)) {
            match semantic_time
                .checked_add(i128::from(IMITATOR_MORPH_DELAY))
                .filter(|time| i32::try_from(*time).is_ok())
            {
                Some(time) => Some(time),
                None => {
                    errors.push(EffectTimeError::OutsideRange("imitator morph"));
                    None
                }
            }
        } else {
            None
        };
        if !errors.is_empty() {
            return Err(errors);
        }
        let cleanup_time = cleanup_time.expect("validated cleanup time");
        let state = Rc::new(Cell::new(RetentionState::default()));
        let placement_state = Rc::clone(&state);
        let selection = self.selection;
        let row = self.row;
        let col = self.col;
        push(
            semantic_time,
            Box::new(move || {
                {
                    core_cards::card_recording(selection, row, col, |component, id| {
                        let mut current = placement_state.get();
                        match component {
                            PlantingComponent::AutoContainer => current.container = Some(id),
                            PlantingComponent::Main => current.main = Some(id),
                        }
                        placement_state.set(current);
                    })
                }
                .map(|_receipt| ())
                .map_err(Into::into)
            }),
        );
        if let Some(resolution_time) = resolution_time
            && cleanup_time >= resolution_time
        {
            let resolution_state = Rc::clone(&state);
            push(
                resolution_time,
                Box::new(move || resolve_imitator_successor(&resolution_state)),
            );
        }
        push(cleanup_time, Box::new(move || cleanup_retained(&state)));
        Ok(())
    }
}
