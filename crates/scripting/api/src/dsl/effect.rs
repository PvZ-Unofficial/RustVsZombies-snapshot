//! Effect-time card shorthand.

use rsvz_current::CurrentBackend;

#[cfg(all(test, feature = "pvz-emulator"))]
use crate::runtime::RuntimeResult;
use rsvz_backend_api::backend::{ImitatorMorphBackend, PlantEffectCountdownWriteBackend, SceneBackend};
use rsvz_game::logic::card_timing::{EffectTimeError, mushroom_effect_timing, simple_card_effect_timing};
use rsvz_game::logic::cards::CardContext;
use rsvz_model::model::{CardSelection, Grid, PlantKind};

use rsvz_game::cards::effect::{CoffeeIceEffect, ExplicitCoffeeIceEffect, MushroomEffect, SimpleEffect};

use super::card::Retention;
use super::expr::{Expr, Leaf, PrepareContext};

#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct SimpleEffectCard {
    kind: PlantKind,
}

impl SimpleEffectCard {
    const fn new(kind: PlantKind) -> Self {
        Self { kind }
    }
}

crate::callable::impl_callable! {
    impl<> SimpleEffectCard
    where {
        CurrentBackend: CardContext + 'static,
    }
    call_as(card; row: i32, col: i32) -> Expr {
        simple_effect_expr(card.kind, None, row, col)
    }
}

crate::callable::impl_callable! {
    impl<> SimpleEffectCard
    where {
        CurrentBackend: CardContext + 'static,
    }
    call_as(card; retention: Retention, row: i32, col: i32) -> Expr {
        simple_effect_expr(card.kind, Some(retention), row, col)
    }
}

#[allow(
    non_upper_case_globals,
    reason = "script DSL aliases intentionally use lowercase names"
)]
pub const a: SimpleEffectCard = SimpleEffectCard::new(PlantKind::CherryBomb);
#[allow(
    non_upper_case_globals,
    reason = "script DSL aliases intentionally use lowercase names"
)]
pub const j: SimpleEffectCard = SimpleEffectCard::new(PlantKind::Jalapeno);
#[allow(
    non_upper_case_globals,
    reason = "script DSL aliases intentionally use lowercase names"
)]
pub const w: SimpleEffectCard = SimpleEffectCard::new(PlantKind::Squash);

fn simple_effect_expr(kind: PlantKind, retention: Option<Retention>, row: i32, col: i32) -> Expr
where
    CurrentBackend: CardContext + 'static,
{
    if Grid::from_one_based(row, col).is_err() {
        return Expr::error(format!("invalid effect card grid ({row}, {col})"));
    }
    let Some(timing) = simple_card_effect_timing(kind) else {
        return Expr::error(format!("unsupported simple effect card {kind:?}"));
    };
    Expr::from_leaf(SimpleEffectLeaf(SimpleEffect {
        selection: timing.selection,
        placement_lead: timing.placement_lead,
        retention,
        row,
        col,
    }))
}

struct SimpleEffectLeaf(SimpleEffect);

impl Leaf for SimpleEffectLeaf
where
    CurrentBackend: CardContext + 'static,
{
    fn prepare(&self, context: &mut PrepareContext<'_>) {
        let result = self
            .0
            .prepare(context.semantic_time(), |time, callback| context.push(time, callback));
        report_time_errors(context, result);
    }
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct Mushroom<const IMITATOR: bool> {
    selection: CardSelection,
}

impl<const IMITATOR: bool> Mushroom<IMITATOR> {
    const fn new(selection: CardSelection) -> Self {
        Self { selection }
    }
}

crate::callable::impl_callable! {
    impl<> Mushroom<false>
    where {
        CurrentBackend: CardContext + SceneBackend + PlantEffectCountdownWriteBackend + 'static,
    }
    call_as(card; row: i32, col: i32) -> Expr {
        plain_mushroom_expr(card.selection, None, row, col)
    }
}

crate::callable::impl_callable! {
    impl<> Mushroom<false>
    where {
        CurrentBackend: CardContext + SceneBackend + PlantEffectCountdownWriteBackend + 'static,
    }
    call_as(card; retention: Retention, row: i32, col: i32) -> Expr {
        plain_mushroom_expr(card.selection, Some(retention), row, col)
    }
}

crate::callable::impl_callable! {
    impl<> Mushroom<true>
    where {
        CurrentBackend:
            CardContext + ImitatorMorphBackend + SceneBackend + PlantEffectCountdownWriteBackend + 'static,
    }
    call_as(card; row: i32, col: i32) -> Expr {
        mushroom_expr(card.selection, None, row, col)
    }
}

crate::callable::impl_callable! {
    impl<> Mushroom<true>
    where {
        CurrentBackend:
            CardContext + ImitatorMorphBackend + SceneBackend + PlantEffectCountdownWriteBackend + 'static,
    }
    call_as(card; retention: Retention, row: i32, col: i32) -> Expr {
        mushroom_expr(card.selection, Some(retention), row, col)
    }
}

#[allow(
    non_upper_case_globals,
    reason = "script DSL aliases intentionally use lowercase names"
)]
pub const i: Mushroom<false> = Mushroom::new(CardSelection::Plant(PlantKind::IceShroom));
#[allow(
    non_upper_case_globals,
    reason = "script DSL aliases intentionally use lowercase names"
)]
pub const mi: Mushroom<true> = Mushroom::new(CardSelection::Imitator(PlantKind::IceShroom));
#[allow(
    non_upper_case_globals,
    reason = "script DSL aliases intentionally use lowercase names"
)]
pub const n: Mushroom<false> = Mushroom::new(CardSelection::Plant(PlantKind::DoomShroom));

fn plain_mushroom_expr(selection: CardSelection, retention: Option<Retention>, row: i32, col: i32) -> Expr
where
    CurrentBackend: CardContext + SceneBackend + PlantEffectCountdownWriteBackend + 'static,
{
    mushroom_expr_with(selection, retention, row, col, MushroomEffect::plain)
}

fn mushroom_expr(selection: CardSelection, retention: Option<Retention>, row: i32, col: i32) -> Expr
where
    CurrentBackend: CardContext + ImitatorMorphBackend + SceneBackend + PlantEffectCountdownWriteBackend + 'static,
{
    mushroom_expr_with(selection, retention, row, col, MushroomEffect::imitator)
}

fn mushroom_expr_with(
    selection: CardSelection, retention: Option<Retention>, row: i32, col: i32,
    make_effect: fn(rsvz_game::logic::card_timing::MushroomEffectTiming, Option<Retention>, i32, i32) -> MushroomEffect,
) -> Expr
where
    CurrentBackend: CardContext + SceneBackend + PlantEffectCountdownWriteBackend + 'static,
{
    if Grid::from_one_based(row, col).is_err() {
        return Expr::error(format!("invalid mushroom grid ({row}, {col})"));
    }
    let Some(timing) = mushroom_effect_timing(selection) else {
        return Expr::error(format!("unsupported mushroom selection {selection:?}"));
    };
    Expr::from_leaf(MushroomLeaf(make_effect(timing, retention, row, col)))
}

struct MushroomLeaf(MushroomEffect);

impl Leaf for MushroomLeaf
where
    CurrentBackend: CardContext + SceneBackend + PlantEffectCountdownWriteBackend + 'static,
{
    fn prepare(&self, context: &mut PrepareContext<'_>) {
        let result = self
            .0
            .prepare(context.semantic_time(), |time, callback| context.push(time, callback));
        report_time_errors(context, result);
    }
}

crate::callable::callable_api! {
    /// Constructs a stored-ice activation at its semantic effect time.
    ///
    /// `ci()` lets the current ice filler choose a stored ice. `ci(row, col)`
    /// activates the ice shroom at one explicit script-facing grid. Both forms
    /// schedule the coffee bean and normalize the native effect countdown.
    pub ci: CoffeeIce;

    where {
        CurrentBackend: CardContext + PlantEffectCountdownWriteBackend + 'static,
    }

    impl<>
    where {}
    call() -> Expr {
        Expr::from_leaf(CoffeeIceLeaf(CoffeeIceEffect))
    }

    impl<>
    where {}
    call(row: i32, col: i32) -> Expr {
        explicit_coffee_ice_expr(row, col)
    }
}

fn explicit_coffee_ice_expr(row: i32, col: i32) -> Expr
where
    CurrentBackend: CardContext + PlantEffectCountdownWriteBackend + 'static,
{
    if Grid::from_one_based(row, col).is_err() {
        return Expr::error(format!("invalid stored ice grid ({row}, {col})"));
    }
    Expr::from_leaf(ExplicitCoffeeIceLeaf(ExplicitCoffeeIceEffect { row, col }))
}

struct CoffeeIceLeaf(CoffeeIceEffect);

impl Leaf for CoffeeIceLeaf
where
    CurrentBackend: CardContext + PlantEffectCountdownWriteBackend + 'static,
{
    fn prepare(&self, context: &mut PrepareContext<'_>) {
        let result = self
            .0
            .prepare(context.semantic_time(), |time, callback| context.push(time, callback));
        report_time_errors(context, result);
    }
}

struct ExplicitCoffeeIceLeaf(ExplicitCoffeeIceEffect);

impl Leaf for ExplicitCoffeeIceLeaf
where
    CurrentBackend: CardContext + PlantEffectCountdownWriteBackend + 'static,
{
    fn prepare(&self, context: &mut PrepareContext<'_>) {
        let result = self
            .0
            .prepare(context.semantic_time(), |time, callback| context.push(time, callback));
        report_time_errors(context, result);
    }
}

pub(super) fn report_time_errors(context: &mut PrepareContext<'_>, result: Result<(), Vec<EffectTimeError>>) {
    if let Err(errors) = result {
        for error in errors {
            context.error(time_error(context.wave(), error));
        }
    }
}

fn time_error(wave: i32, error: EffectTimeError) -> String {
    match error {
        EffectTimeError::OutsideRange(label) => format!("wave {wave} {label} time is outside the i32 range"),
        EffectTimeError::Overflow(label) => format!("wave {wave} {label} time overflowed"),
        EffectTimeError::Retention(error) => error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "pvz-emulator")]
    fn reset() {
        rsvz_schedule::timeline::with_timeline(|timeline| timeline.clear_all());
    }
    #[cfg(feature = "pvz-emulator")]
    fn retained_card_expr(selection: CardSelection, retention: Retention) -> Expr {
        super::super::card::DslCard::default()(retention, selection, 2, 9)
    }
    #[test]
    fn effect_retention_can_be_negative_but_not_before_placement() {
        let retention = super::super::card::keep(-50);
        assert_eq!(retention.cleanup_time(1000, 900), Ok(950));
        assert!(super::super::card::keep(-101).cleanup_time(1000, 900).is_err());
    }
    #[test]
    fn mushroom_retention_must_follow_both_scene_placement_branches() {
        let timing = mushroom_effect_timing(CardSelection::Plant(PlantKind::IceShroom)).expect("ice timing");
        let semantic_time = 1_000_i128;
        let day = semantic_time - i128::from(timing.day_placement_lead);
        let night = semantic_time - i128::from(timing.night_placement_lead);
        let latest = day.max(night);

        assert_eq!(
            super::super::card::keep(-100).cleanup_time(semantic_time, latest),
            Ok(night)
        );
        assert!(
            super::super::card::keep(-101)
                .cleanup_time(semantic_time, latest)
                .is_err()
        );
    }
    #[test]
    fn mushroom_time_errors_are_aggregated_in_callback_order() {
        let timing =
            mushroom_effect_timing(CardSelection::Imitator(PlantKind::IceShroom)).expect("imitator ice timing");
        let errors = rsvz_game::logic::card_timing::mushroom_times(i128::from(i32::MIN), timing)
            .expect_err("all derived times overflow");

        assert_eq!(
            errors.into_iter().map(|error| time_error(3, error)).collect::<Vec<_>>(),
            [
                "wave 3 day mushroom placement time is outside the i32 range",
                "wave 3 day mushroom coffee time is outside the i32 range",
                "wave 3 night mushroom placement time is outside the i32 range",
                "wave 3 mushroom effect normalization time is outside the i32 range",
            ]
        );
    }
    #[test]
    fn coffee_ice_time_errors_are_aggregated() {
        let errors = rsvz_game::logic::card_timing::coffee_ice_times(i128::from(i32::MIN) - 1)
            .expect_err("all coffee-ice times overflow");

        assert_eq!(
            errors.into_iter().map(|error| time_error(4, error)).collect::<Vec<_>>(),
            [
                "wave 4 coffee-ice effect time is outside the i32 range",
                "wave 4 coffee-ice wake time is outside the i32 range",
                "wave 4 coffee-ice normalization time is outside the i32 range",
            ]
        );
    }
    #[test]
    #[cfg(feature = "pvz-emulator")]
    fn effect_time_and_retention_errors_are_reported_together() {
        reset();
        let simple = super::simple_effect_expr(PlantKind::CherryBomb, Some(super::super::card::keep(-1_000)), 2, 9);
        let simple_error = crate::registration::run_script(|| -> RuntimeResult<()> {
            (1, i32::MIN) << simple;
            Ok(())
        })
        .expect_err("simple effect has two independent time errors");
        assert!(
            simple_error
                .message()
                .contains("effect card placement time is outside the i32 range")
        );
        assert!(simple_error.message().contains("retention cleanup time"));
        rsvz_schedule::timeline::with_timeline(|timeline| assert_eq!(timeline.diagnostics().pending_count, 0));

        reset();
        let mushroom = super::mushroom_expr(
            CardSelection::Imitator(PlantKind::IceShroom),
            Some(super::super::card::keep(-1_000)),
            2,
            9,
        );
        let mushroom_error = crate::registration::run_script(|| -> RuntimeResult<()> {
            (1, i32::MIN) << mushroom;
            Ok(())
        })
        .expect_err("mushroom timing and retention errors must aggregate");
        assert_eq!(mushroom_error.message().split("; ").count(), 5);
        assert!(mushroom_error.message().contains("retention cleanup time"));
        rsvz_schedule::timeline::with_timeline(|timeline| assert_eq!(timeline.diagnostics().pending_count, 0));
    }
    #[test]
    #[cfg(feature = "pvz-emulator")]
    fn retained_card_cleanup_and_morph_overflow_errors_are_reported_together() {
        reset();
        let retained = retained_card_expr(
            CardSelection::Imitator(PlantKind::IceShroom),
            super::super::card::keep(-1),
        );
        let error = crate::registration::run_script(|| -> RuntimeResult<()> {
            (1, i32::MAX) << retained;
            Ok(())
        })
        .expect_err("cleanup and morph validation both fail");

        assert_eq!(error.message().split("; ").count(), 2);
        assert!(error.message().contains("retention cleanup time"));
        assert!(error.message().contains("imitator morph time is outside the i32 range"));
        rsvz_schedule::timeline::with_timeline(|timeline| assert_eq!(timeline.diagnostics().pending_count, 0));
    }
}
