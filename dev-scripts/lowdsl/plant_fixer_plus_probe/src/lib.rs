use std::cell::RefCell;
use std::fmt::Display;
use std::rc::Rc;

use rsvz::core::logic::cards::{find_seed_slot, new_plant, seed_slot_current_cost};
use rsvz::core::logic::cleanup::kill_all_zombies;
use rsvz::core::logic::plant_fixer::{PlantFixer, PlantFixerBackend};
use rsvz::core::model::{CardSelection, SceneKind};
use rsvz::core::runtime::{RuntimeError, RuntimeResult};
use rsvz::prelude::ZombieReadBackend;
use rsvz::prelude::{
    Grid, GridItemEditBackend, ObjectEditOutcome, PlantCostBackend, PlantCreateBackend, PlantHealthWriteBackend,
    PlantId, PlantKind, PlantReadBackend, PlantRemoveBackend, SceneEditBackend, SeedBankReadBackend,
    SeedRuleEditBackend, SunCostRuleEditBackend, SunWriteBackend, ZombieKillBackend, ZombieRuleEditBackend,
};

const STOP_SPAWN_TIME: i32 = -599;
const FIRST_CASE_TIME: i32 = 100;
const CASE_SPACING: i32 = 120;
const FINAL_PADDING: i32 = 80;
const THRESHOLD_HP: i32 = 200;
const PLENTY_SUN: u32 = 9999;

const A: Grid = Grid { row: 0, col: 0 };
const B: Grid = Grid { row: 0, col: 1 };
const C: Grid = Grid { row: 1, col: 0 };

#[derive(Clone, Copy, Debug)]
struct PlantState {
    id: PlantId,
    hp: i32,
}

#[derive(Clone, Copy, Debug)]
struct ProbeCase {
    label: &'static str,
    kind: ProbeCaseKind,
}

#[derive(Clone, Copy, Debug)]
enum ProbeCaseKind {
    LowerHpBeatsGridOrder,
    EqualHpPreservesConfiguredOrder,
    MissingBeatsLowHp,
    AboveThresholdIgnored,
    NoSunRepairsNothing,
    OneCardBudgetRepairsOnlyLowestHp,
    RechargeIgnoredRepairsOneTarget,
    ThresholdEqualIsRepaired,
    AllAboveThresholdNoop,
    MissingPeashooterCreatesNewPlant,
}

const CASES: &[ProbeCase] = &[
    ProbeCase {
        label: "lower-hp-beats-grid-order",
        kind: ProbeCaseKind::LowerHpBeatsGridOrder,
    },
    ProbeCase {
        label: "equal-hp-preserves-configured-order",
        kind: ProbeCaseKind::EqualHpPreservesConfiguredOrder,
    },
    ProbeCase {
        label: "missing-beats-low-hp",
        kind: ProbeCaseKind::MissingBeatsLowHp,
    },
    ProbeCase {
        label: "above-threshold-ignored",
        kind: ProbeCaseKind::AboveThresholdIgnored,
    },
    ProbeCase {
        label: "no-sun-repairs-nothing",
        kind: ProbeCaseKind::NoSunRepairsNothing,
    },
    ProbeCase {
        label: "one-card-budget-repairs-only-lowest-hp",
        kind: ProbeCaseKind::OneCardBudgetRepairsOnlyLowestHp,
    },
    ProbeCase {
        label: "recharge-ignored-repairs-one-target",
        kind: ProbeCaseKind::RechargeIgnoredRepairsOneTarget,
    },
    ProbeCase {
        label: "threshold-equal-is-repaired",
        kind: ProbeCaseKind::ThresholdEqualIsRepaired,
    },
    ProbeCase {
        label: "all-above-threshold-noop",
        kind: ProbeCaseKind::AllAboveThresholdNoop,
    },
    ProbeCase {
        label: "missing-peashooter-creates-new-plant",
        kind: ProbeCaseKind::MissingPeashooterCreatesNewPlant,
    },
];

#[rsvz::script]
fn script() {
    reload(MainUiOrFightUi);
    rsvz::setup::set_game_speed(1.0);
    set_zombies("普");
    select_cards([
        pumpkin, tallnut, wallnut, umbrella, garlic, pea, sunflower, pot, lily, spike,
    ]);

    let errors = Rc::new(RefCell::new(Vec::<String>::new()));

    let errors_for_setup = Rc::clone(&errors);
    physical_setup_at(STOP_SPAWN_TIME, move || {
        let result = (|| -> RuntimeResult<()> {
            rsvz::__private::with_board_access(|access| {
                let backend = access.backend();
                backend.set_zombie_spawn_stopped(true).map_err(runtime_error)?;
                clear_editable_board(backend)?;
                for zombie in backend.zombies() {
                    backend.kill_zombie(zombie).map_err(runtime_error)?;
                }
                Ok(())
            }).and_then(|result| result)?;
            rsvz::with_backend(|backend| backend.set_scene(SceneKind::Day).map_err(runtime_error))?;
            rsvz::__private::with_board_access(|access| {
                access.backend().set_sun(PLENTY_SUN).map_err(runtime_error)
            }).and_then(|result| result)
        })();
        if let Err(error) = result {
            push_error(&errors_for_setup, format!("initial setup failed: {error}"));
        }
        Ok(())
    })?;

    for (index, case) in CASES.iter().copied().enumerate() {
        let errors_for_case = Rc::clone(&errors);
        let time = FIRST_CASE_TIME + (index as i32) * CASE_SPACING;
        rsvz::try_at(1, time, move || {
            rsvz::__private::with_board_access(|access| {
                let backend = access.backend();
                if let Err(error) = run_probe_case(backend, case.kind) {
                    push_error(&errors_for_case, format!("{} failed: {error}", case.label));
                    if let Err(cleanup_error) = prepare_visible_case_board(backend) {
                        push_error(
                            &errors_for_case,
                            format!("{} cleanup after failure failed: {cleanup_error}", case.label),
                        );
                    }
                }
                Ok(())
            })
            .and_then(|result| result)
        })?;
    }

    let final_report_time = FIRST_CASE_TIME + (CASES.len() as i32) * CASE_SPACING + FINAL_PADDING;
    let errors_for_report = Rc::clone(&errors);
    rsvz::try_at(1, final_report_time, move || {
        rsvz::__private::with_board_access(|access| {
            let backend = access.backend();
            if let Err(error) = prepare_visible_case_board(backend) {
                push_error(&errors_for_report, format!("final cleanup failed: {error}"));
            }
            let errors = errors_for_report.borrow();
            if errors.is_empty() {
                Ok(())
            } else {
                Err(RuntimeError::new(format!(
                    "plant_fixer_plus_probe failed: {}",
                    errors.join(" | ")
                )))
            }
        })
        .and_then(|result| result)
    })?;
}

fn run_probe_case<B>(backend: &B, case: ProbeCaseKind) -> RuntimeResult<()>
where
    B: PlantFixerProbeBackend,
    B::Error: Display,
{
    match case {
        ProbeCaseKind::LowerHpBeatsGridOrder => verify_lower_hp_beats_grid_order(backend),
        ProbeCaseKind::EqualHpPreservesConfiguredOrder => verify_equal_hp_preserves_configured_order(backend),
        ProbeCaseKind::MissingBeatsLowHp => verify_missing_target_beats_low_hp_target(backend),
        ProbeCaseKind::AboveThresholdIgnored => verify_above_threshold_target_is_ignored(backend),
        ProbeCaseKind::NoSunRepairsNothing => verify_no_sun_repairs_nothing(backend),
        ProbeCaseKind::OneCardBudgetRepairsOnlyLowestHp => verify_one_card_budget_repairs_only_lowest_hp(backend),
        ProbeCaseKind::RechargeIgnoredRepairsOneTarget => verify_recharge_ignored_repairs_one_target(backend),
        ProbeCaseKind::ThresholdEqualIsRepaired => verify_threshold_equal_is_repaired(backend),
        ProbeCaseKind::AllAboveThresholdNoop => verify_all_above_threshold_noop(backend),
        ProbeCaseKind::MissingPeashooterCreatesNewPlant => verify_missing_peashooter_creates_new_plant(backend),
    }
}

fn verify_lower_hp_beats_grid_order<B>(backend: &B) -> RuntimeResult<()>
where
    B: PlantFixerProbeBackend,
    B::Error: Display,
{
    prepare_visible_case_board(backend)?;
    let kind = PlantKind::Garlic;
    let high = create_plant_with_hp(kind, A, 150)?;
    let low = create_plant_with_hp(kind, B, 50)?;

    run_repair_tick_with_card_budget(backend, kind, [A, B], 1, true)?;

    ensure_replaced(backend, kind, B, low, "lower-HP target")?;
    ensure_unchanged(backend, kind, A, high, 150, "higher-HP target")
}

fn verify_equal_hp_preserves_configured_order<B>(backend: &B) -> RuntimeResult<()>
where
    B: PlantFixerProbeBackend,
    B::Error: Display,
{
    prepare_visible_case_board(backend)?;
    let kind = PlantKind::TallNut;
    let first_in_list = create_plant_with_hp(kind, A, 100)?;
    let second_in_list = create_plant_with_hp(kind, B, 100)?;

    run_repair_tick_with_card_budget(backend, kind, [B, A], 1, true)?;

    ensure_replaced(backend, kind, B, second_in_list, "first configured equal-HP target")?;
    ensure_unchanged(
        backend,
        kind,
        A,
        first_in_list,
        100,
        "second configured equal-HP target",
    )
}

fn verify_missing_target_beats_low_hp_target<B>(backend: &B) -> RuntimeResult<()>
where
    B: PlantFixerProbeBackend,
    B::Error: Display,
{
    prepare_visible_case_board(backend)?;
    let kind = PlantKind::WallNut;
    let low = create_plant_with_hp(kind, A, 50)?;

    run_repair_tick_with_card_budget(backend, kind, [A, B], 1, true)?;

    ensure_new_plant(backend, kind, B, "missing target")?;
    ensure_unchanged(backend, kind, A, low, 50, "low-HP present target")
}

fn verify_above_threshold_target_is_ignored<B>(backend: &B) -> RuntimeResult<()>
where
    B: PlantFixerProbeBackend,
    B::Error: Display,
{
    prepare_visible_case_board(backend)?;
    let kind = PlantKind::UmbrellaLeaf;
    let above_threshold = create_plant_with_hp(kind, A, THRESHOLD_HP + 50)?;
    let below_threshold = create_plant_with_hp(kind, B, 50)?;

    run_repair_tick_with_card_budget(backend, kind, [A, B], 1, true)?;

    ensure_replaced(backend, kind, B, below_threshold, "below-threshold target")?;
    ensure_unchanged(
        backend,
        kind,
        A,
        above_threshold,
        THRESHOLD_HP + 50,
        "above-threshold target",
    )
}

fn verify_no_sun_repairs_nothing<B>(backend: &B) -> RuntimeResult<()>
where
    B: PlantFixerProbeBackend,
    B::Error: Display,
{
    prepare_visible_case_board(backend)?;
    let kind = PlantKind::Spikeweed;
    let first = create_plant_with_hp(kind, A, 20)?;
    let second = create_plant_with_hp(kind, B, 40)?;

    run_repair_tick_with_sun(backend, kind, [A, B], 0, true)?;

    ensure_unchanged(backend, kind, A, first, 20, "no-sun first target")?;
    ensure_unchanged(backend, kind, B, second, 40, "no-sun second target")
}

fn verify_one_card_budget_repairs_only_lowest_hp<B>(backend: &B) -> RuntimeResult<()>
where
    B: PlantFixerProbeBackend,
    B::Error: Display,
{
    prepare_visible_case_board(backend)?;
    let kind = PlantKind::Pumpkin;
    let low = create_plant_with_hp(kind, A, 40)?;
    let mid = create_plant_with_hp(kind, B, 80)?;
    let high = create_plant_with_hp(kind, C, 120)?;

    run_repair_tick_with_card_budget(backend, kind, [C, B, A], 1, true)?;

    ensure_replaced(backend, kind, A, low, "lowest-HP budget target")?;
    ensure_unchanged(backend, kind, B, mid, 80, "mid-HP budget target")?;
    ensure_unchanged(backend, kind, C, high, 120, "high-HP budget target")
}

fn verify_recharge_ignored_repairs_one_target<B>(backend: &B) -> RuntimeResult<()>
where
    B: PlantFixerProbeBackend,
    B::Error: Display,
{
    prepare_visible_case_board(backend)?;
    let kind = PlantKind::Sunflower;
    let first = create_plant_with_hp(kind, A, 70)?;
    let second = create_plant_with_hp(kind, B, 70)?;

    run_repair_tick_with_card_budget(backend, kind, [A, B], 2, true)?;

    ensure_replaced(backend, kind, A, first, "first recharge-ignored target")?;
    ensure_unchanged(backend, kind, B, second, 70, "second recharge-ignored target")
}

fn verify_threshold_equal_is_repaired<B>(backend: &B) -> RuntimeResult<()>
where
    B: PlantFixerProbeBackend,
    B::Error: Display,
{
    prepare_visible_case_board(backend)?;
    let kind = PlantKind::Garlic;
    let target = create_plant_with_hp(kind, A, THRESHOLD_HP)?;

    run_repair_tick_with_card_budget(backend, kind, [A], 1, true)?;

    ensure_replaced(backend, kind, A, target, "threshold-equal target")
}

fn verify_all_above_threshold_noop<B>(backend: &B) -> RuntimeResult<()>
where
    B: PlantFixerProbeBackend,
    B::Error: Display,
{
    prepare_visible_case_board(backend)?;
    let kind = PlantKind::Pumpkin;
    let first = create_plant_with_hp(kind, A, THRESHOLD_HP + 1)?;
    let second = create_plant_with_hp(kind, B, THRESHOLD_HP + 120)?;

    run_repair_tick_with_card_budget(backend, kind, [A, B], 2, true)?;

    ensure_unchanged(
        backend,
        kind,
        A,
        first,
        THRESHOLD_HP + 1,
        "above-threshold first noop target",
    )?;
    ensure_unchanged(
        backend,
        kind,
        B,
        second,
        THRESHOLD_HP + 120,
        "above-threshold second noop target",
    )
}

fn verify_missing_peashooter_creates_new_plant<B>(backend: &B) -> RuntimeResult<()>
where
    B: PlantFixerProbeBackend,
    B::Error: Display,
{
    prepare_visible_case_board(backend)?;
    let kind = PlantKind::Peashooter;

    run_repair_tick_with_card_budget(backend, kind, [A], 1, true)?;

    ensure_new_plant(backend, kind, A, "missing peashooter target")
}

fn run_repair_tick_with_card_budget<B, I>(
    backend: &B, kind: PlantKind, grids: I, card_count: u32, recharge_ignored: bool,
) -> RuntimeResult<()>
where
    B: PlantFixerProbeBackend,
    B::Error: Display,
    I: IntoIterator<Item = Grid>,
{
    let cost = seed_cost_for_kind(kind)?;
    let sun = cost
        .checked_mul(card_count)
        .ok_or_else(|| RuntimeError::new("plant fixer probe sun budget overflow"))?;
    run_repair_tick_with_sun(backend, kind, grids, sun, recharge_ignored)
}

fn run_repair_tick_with_sun<B, I>(
    backend: &B, kind: PlantKind, grids: I, sun: u32, recharge_ignored: bool,
) -> RuntimeResult<()>
where
    B: PlantFixerProbeBackend,
    B::Error: Display,
    I: IntoIterator<Item = Grid>,
{
    backend
        .set_seed_recharge_ignored(recharge_ignored)
        .map_err(runtime_error)?;
    backend.set_sun_cost_ignored(false).map_err(runtime_error)?;
    backend.set_sun(sun).map_err(runtime_error)?;

    let mut fixer = PlantFixer::new();
    fixer.set_check_cards(false);
    fixer
        .start_with(kind, grids, THRESHOLD_HP as f32, false)
        .map_err(runtime_error)?;
    fixer.tick().map_err(runtime_error)
}

fn seed_cost_for_kind(kind: PlantKind) -> RuntimeResult<u32>
where
    rsvz::__private::CurrentBackend: SeedBankReadBackend + PlantCostBackend,
{
    let selection = CardSelection::Plant(kind);
    let slot = find_seed_slot(selection)
        .ok_or_else(|| RuntimeError::new(format!("card {selection:?} is not selected")))?;
    let cost = seed_slot_current_cost(slot)
        .ok_or_else(|| RuntimeError::new(format!("card {selection:?} disappeared before its cost was read")))?;
    ensure(cost > 0, format!("card {selection:?} has zero cost"))?;
    Ok(cost)
}

fn create_plant_with_hp(kind: PlantKind, grid: Grid, hp: i32) -> RuntimeResult<PlantId>
where
    rsvz::__private::CurrentBackend: PlantCreateBackend + PlantHealthWriteBackend,
{
    let id = new_plant(kind, grid).map_err(runtime_error)?;
    match rsvz::core::modifier::set_plant_hp(id, hp).map_err(runtime_error)? {
        ObjectEditOutcome::Applied => Ok(id),
        ObjectEditOutcome::Missing => Err(RuntimeError::new(format!(
            "created {kind:?} at {grid:?} disappeared before HP setup"
        ))),
    }
}

fn ensure_replaced<B>(
    backend: &B, kind: PlantKind, grid: Grid, previous_id: PlantId, label: &'static str,
) -> RuntimeResult<()>
where
    B: PlantReadBackend,
    B::Error: Display,
{
    let state = require_plant(backend, kind, grid, label)?;
    ensure(
        state.id != previous_id,
        format!("{label} was not replaced; id stayed {previous_id:?}"),
    )?;
    ensure(
        state.hp > THRESHOLD_HP,
        format!("{label} replacement HP {} did not exceed threshold", state.hp),
    )
}

fn ensure_new_plant<B>(backend: &B, kind: PlantKind, grid: Grid, label: &'static str) -> RuntimeResult<()>
where
    B: PlantReadBackend,
    B::Error: Display,
{
    let state = require_plant(backend, kind, grid, label)?;
    ensure(
        state.hp > THRESHOLD_HP,
        format!("{label} new plant HP {} did not exceed threshold", state.hp),
    )
}

fn ensure_unchanged<B>(
    backend: &B, kind: PlantKind, grid: Grid, expected_id: PlantId, expected_hp: i32, label: &'static str,
) -> RuntimeResult<()>
where
    B: PlantReadBackend,
    B::Error: Display,
{
    let state = require_plant(backend, kind, grid, label)?;
    ensure(
        state.id == expected_id,
        format!("{label} was unexpectedly replaced: {expected_id:?} -> {:?}", state.id),
    )?;
    ensure(
        state.hp == expected_hp,
        format!("{label} HP changed: expected {expected_hp}, got {}", state.hp),
    )
}

fn require_plant<B>(backend: &B, kind: PlantKind, grid: Grid, label: &'static str) -> RuntimeResult<PlantState>
where
    B: PlantReadBackend,
    B::Error: Display,
{
    plant_state_at(backend, kind, grid)?.ok_or_else(|| {
        RuntimeError::new(format!(
            "{label} plant {kind:?} at row {}, col {} is missing",
            grid.row, grid.col
        ))
    })
}

fn plant_state_at<B>(backend: &B, kind: PlantKind, grid: Grid) -> RuntimeResult<Option<PlantState>>
where
    B: PlantReadBackend,
    B::Error: Display,
{
    for plant in backend.plants() {
        if backend.plant_is_alive(plant)
            && backend.plant_kind(plant).map_err(runtime_error)? == kind
            && (backend.plant_row(plant) == grid.row && backend.plant_col(plant) == grid.col)
        {
            return Ok(Some(PlantState {
                id: backend.plant_id(plant),
                hp: backend.plant_hp(plant),
            }));
        }
    }
    Ok(None)
}

fn prepare_visible_case_board<B>(backend: &B) -> RuntimeResult<()>
where
    B: PlantRemoveBackend + ZombieKillBackend + SceneEditBackend + GridItemEditBackend + SunWriteBackend,
    B::Error: Display,
{
    clear_editable_board(backend)?;
    kill_all_zombies().map_err(runtime_error)?;
    backend.set_sun(PLENTY_SUN).map_err(runtime_error)
}

fn clear_editable_board<B>(backend: &B) -> RuntimeResult<()>
where
    B: PlantRemoveBackend + GridItemEditBackend,
    B::Error: Display,
{
    for plant in backend.plants() {
        backend.remove_plant(plant).map_err(runtime_error)?;
    }
    for item in backend.grid_items() {
        backend.remove_grid_item(item).map_err(runtime_error)?;
    }
    Ok(())
}

fn push_error(errors: &Rc<RefCell<Vec<String>>>, message: impl Into<String>) {
    errors.borrow_mut().push(message.into());
}

trait PlantFixerProbeBackend:
    PlantFixerBackend
    + PlantHealthWriteBackend
    + SunWriteBackend
    + SeedRuleEditBackend
    + SunCostRuleEditBackend
    + SceneEditBackend
    + GridItemEditBackend
    + ZombieKillBackend
{
}

impl<T> PlantFixerProbeBackend for T where
    T: PlantFixerBackend
        + PlantHealthWriteBackend
        + SunWriteBackend
        + SeedRuleEditBackend
        + SunCostRuleEditBackend
        + SceneEditBackend
        + GridItemEditBackend
        + ZombieKillBackend
{
}

fn ensure(condition: bool, message: impl Into<RuntimeError>) -> RuntimeResult<()> {
    if condition { Ok(()) } else { Err(message.into()) }
}

fn runtime_error(error: impl Display) -> RuntimeError {
    RuntimeError::new(error.to_string())
}

// Keep physical replacement in this native frame, after timeline borrows end
// and before the host updates the world. Verification starts on later frames.
fn physical_setup_at(time: i32, setup: impl FnOnce() -> RuntimeResult<()> + 'static) -> RuntimeResult<()> {
    let pending = Rc::new(std::cell::Cell::new(false));
    let due = Rc::clone(&pending);
    rsvz::try_at(1, time, move || due.set(true))?;
    let mut setup = Some(setup);
    rsvz::tick::spawn(rsvz::tick::TickOptions::any_dispatch(), move |_| {
        if !pending.get() { return Ok(rsvz::tick::TickControl::Continue); }
        if let Some(setup) = setup.take() { setup()?; }
        Ok(rsvz::tick::TickControl::Stop)
    });
    Ok(())
}
