#![cfg(any(feature = "pvz-emulator", feature = "pvz-1-0-0-1051", feature = "pvz-portable"))]
use rsvz::cob::CobManager;
use rsvz::dsl::{CardAlias, DslCard, DslShovel, DslTryCard, Expr, Mushroom, P, Pp, RecoverP, fume, rp};
use rsvz::runtime::RuntimeResult;
use rsvz_model::{CardSelection, PlantKind, SeedSlot};
fn choose_cob(use_pair: bool) -> Expr {
    if use_pair {
        Pp::default()()
    } else {
        P::default()(15, 7.8)
    }
}

fn ordinary_alias_expression(alias: CardAlias) -> Expr {
    alias(2, 9)
}

fn ordinary_mushroom_expression(mushroom: Mushroom<false>) -> Expr {
    mushroom(2, 9)
}

#[test]
fn dsl_constructs_p_pp_and_borrowed_composition() {
    let chosen = choose_cob(true);
    let _expression = P::default()(15, 7.8) + &chosen + choose_cob(false);
}

#[test]
fn dsl_cob_overloads_cover_explicit_managers_recovery_raw_and_pp_column() {
    let manager = CobManager::new();
    let p = P::default();
    let recover_p = RecoverP::default();

    let _expression = p(&manager, [2, 4], 8.8)
        + p(&manager, [(2, 8.8), (4, 8.8)])
        + recover_p([2, 4], 8.8)
        + recover_p(&manager, [(2, 8.8), (4, 8.8)])
        + rp(1, 1, 2, 8.8)
        + Pp::default()(8.8);
}

#[test]
fn dsl_pp_rejects_a_non_finite_column_during_registration() {
    rsvz_schedule::timeline::with_timeline(|timeline| timeline.clear_all());
    let expression = Pp::default()(f32::NAN);

    let error = rsvz::__private::run_script(|| -> RuntimeResult<()> {
        (1, 100) << expression;
        Ok(())
    })
    .expect_err("invalid PP columns must be registration errors");

    assert_eq!(error.message().as_ref(), "invalid cob target (2, NaN)");
    rsvz_schedule::timeline::with_timeline(|timeline| assert_eq!(timeline.diagnostics().pending_count, 0));
}

#[test]
fn dsl_constructs_card_and_effect_alias_function_values() {
    let card = DslCard::default();
    let fume_expression = card(fume.selection(), 2, 9);
    let _expression = card(PlantKind::FumeShroom, 3, 9) + &fume_expression;

    let _ordinary_alias_compile_proof: fn(CardAlias) -> Expr = ordinary_alias_expression;
    let _ordinary_mushroom_compile_proof: fn(Mushroom<false>) -> Expr = ordinary_mushroom_expression;
}

#[test]
fn dsl_card_and_shovel_overloads_cover_slots_shared_candidates_and_exact_layers() {
    let card = DslCard::default();
    let try_card = DslTryCard::default();
    let shovel = DslShovel::default();
    let slot = SeedSlot::from_index_unchecked(0);

    let _expression = card(slot, 2, 9)
        + card([PlantKind::LilyPad, PlantKind::FumeShroom], 3, 8)
        + card([PlantKind::LilyPad, PlantKind::FumeShroom], [(3, 8), (3, 9)])
        + try_card(slot, [(2, 8), (2, 9)])
        + shovel(2, 9, PlantKind::Pumpkin)
        + shovel([(2, 9, PlantKind::Pumpkin)]);
}

#[test]
fn dsl_every_card_entry_rejects_invalid_imitator_selections_in_source_order() {
    rsvz_schedule::timeline::with_timeline(|timeline| timeline.clear_all());
    let bare = CardSelection::Plant(PlantKind::Imitator);
    let nested = CardSelection::Imitator(PlantKind::Imitator);
    let card = DslCard::default();
    let try_card = DslTryCard::default();

    let expression = card(bare, 2, 9)
        + card(nested, 2, 9)
        + card(bare, [(2, 9)])
        + card(nested, [(2, 9)])
        + card([(bare, 2, 9), (nested, 2, 9)])
        + try_card(bare, 2, 9)
        + try_card(nested, 2, 9)
        + try_card(bare, [(2, 9)])
        + try_card(nested, [(2, 9)]);

    let error = rsvz::__private::run_script(|| -> RuntimeResult<()> {
        (1, 100) << expression;
        Ok(())
    })
    .expect_err("all invalid selections must remain registration errors");

    let bare_error = "invalid card selection: Plant(Imitator)";
    let nested_error = "invalid card selection: Imitator(Imitator)";
    assert_eq!(
        error.message().as_ref(),
        [
            bare_error,
            nested_error,
            bare_error,
            nested_error,
            bare_error,
            nested_error,
            bare_error,
            nested_error,
            bare_error,
            nested_error,
        ]
        .join("; ")
    );
    assert_eq!(
        rsvz_schedule::timeline::with_timeline(|timeline| timeline.diagnostics().pending_count),
        0
    );
}

#[test]
fn dsl_candidate_card_collects_selection_empty_and_each_grid_error() {
    rsvz_schedule::timeline::with_timeline(|timeline| timeline.clear_all());
    let card = DslCard::default();
    let bare = CardSelection::Plant(PlantKind::Imitator);
    let expression = card(bare, Vec::<(i32, i32)>::new()) + card(PlantKind::FumeShroom, [(0, 0), (-1, 2)]);

    let error = rsvz::__private::run_script(|| -> RuntimeResult<()> {
        (1, 100) << expression;
        Ok(())
    })
    .expect_err("all independent candidate errors must be collected");

    assert_eq!(
        error.message().as_ref(),
        [
            "invalid card selection: Plant(Imitator)",
            "candidate card grid list cannot be empty",
            "invalid candidate card grid",
            "invalid candidate card grid",
        ]
        .join("; ")
    );
    rsvz_schedule::timeline::with_timeline(|timeline| assert_eq!(timeline.diagnostics().pending_count, 0));
}
