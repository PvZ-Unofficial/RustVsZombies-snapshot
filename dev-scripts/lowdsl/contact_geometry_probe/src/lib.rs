use std::cell::RefCell;
use std::fmt::Display;
use std::rc::Rc;

use rsvz::core::model::{PixelPos, SceneKind};
use rsvz::core::runtime::{RuntimeError, RuntimeResult};
use rsvz::prelude::{
    CobTarget, ContactCircle, ContactGeometryBackend, DamageRangeFlags, Grid, GridExplosionKind, GridGeometryBackend,
    GridItemEditBackend, ObjectEditOutcome, PlantCreateBackend, PlantDefenseKind, PlantEffectRuleEditBackend, PlantId,
    PlantKind, PlantReadBackend, PlantRemoveBackend, PlantThreatKind, PlantThreatShape, SceneEditBackend,
    ZombieBodyHealthWriteBackend, ZombieCreateBackend, ZombieId, ZombieKillBackend, ZombieKind, ZombiePlantThreatKind,
    ZombieReadBackend, ZombieRuleEditBackend, ZombieXWriteBackend,
};

const STOP_SPAWN_TIME: i32 = -599;
const SETUP_TIME: i32 = -590;
const PROBE_TIME: i32 = -580;
const LIVE_DAMAGE_FIRST_TIME: i32 = -540;
const LIVE_DAMAGE_CASE_SPACING: i32 = 30;
const LIVE_DAMAGE_CHECK_DELAY: i32 = 5;
const LIVE_DAMAGE_FINAL_PADDING: i32 = 16;
const PEA_GRID: Grid = Grid { row: 0, col: 0 };
const LIVE_DAMAGE_COL: i32 = 4;
const LIVE_DAMAGE_SPAWN_COL: i32 = 6;
const SCAN_MIN_X: i32 = -120;
const SCAN_MAX_X: i32 = 900;
const WIDE_SCAN_MIN_X: i32 = -1200;
const WIDE_SCAN_MAX_X: i32 = 1800;
const CIRCLE_RADIUS: i32 = 115;
const LIVE_DAMAGE_INITIAL_HP: i32 = 1000;

#[derive(Clone, Copy)]
struct ProbeIds {
    pea: PlantId,
    normal: ZombieId,
    wrong_row_normal: ZombieId,
    zomboni: ZombieId,
}

#[derive(Clone, Copy, Debug)]
struct XTransition {
    miss_x: i32,
    hit_x: i32,
}

#[derive(Clone, Copy, Debug)]
struct HitSpan {
    leftmost_hit_x: i32,
    rightmost_hit_x: i32,
}

#[derive(Clone, Copy, Debug)]
enum BoundarySide {
    LeftEntering,
    RightExiting,
}

#[derive(Clone, Debug)]
struct LiveDamageProbe {
    label: &'static str,
    explosion_kind: GridExplosionKind,
    side: BoundarySide,
    targets: Vec<ExpectedZombie>,
}

#[derive(Clone, Debug)]
struct ExpectedZombie {
    id: ZombieId,
    kind: ZombieKind,
    row: i32,
    x: f32,
    truncated_x: i32,
    initial_hp: i32,
    expected_hit: bool,
    label: String,
}

#[derive(Clone, Copy, Debug)]
struct LiveDamageCase {
    label: &'static str,
    explosion_kind: GridExplosionKind,
    plant_kind: PlantKind,
    zombie_kind: ZombieKind,
    scene: SceneKind,
    grid: Grid,
    side: BoundarySide,
    expected_hit: bool,
}

const LIVE_DAMAGE_CASES: &[LiveDamageCase] = &[
    live_case(
        "cherry-r1-left-hit",
        GridExplosionKind::CherryBomb,
        PlantKind::CherryBomb,
        0,
        BoundarySide::LeftEntering,
        true,
    ),
    live_case(
        "cherry-r1-left-miss",
        GridExplosionKind::CherryBomb,
        PlantKind::CherryBomb,
        0,
        BoundarySide::LeftEntering,
        false,
    ),
    live_case(
        "cherry-r1-right-hit",
        GridExplosionKind::CherryBomb,
        PlantKind::CherryBomb,
        0,
        BoundarySide::RightExiting,
        true,
    ),
    live_case(
        "cherry-r1-right-miss",
        GridExplosionKind::CherryBomb,
        PlantKind::CherryBomb,
        0,
        BoundarySide::RightExiting,
        false,
    ),
    live_case(
        "cherry-r2-left-hit",
        GridExplosionKind::CherryBomb,
        PlantKind::CherryBomb,
        1,
        BoundarySide::LeftEntering,
        true,
    ),
    live_case(
        "cherry-r2-left-miss",
        GridExplosionKind::CherryBomb,
        PlantKind::CherryBomb,
        1,
        BoundarySide::LeftEntering,
        false,
    ),
    live_case(
        "cherry-r2-right-hit",
        GridExplosionKind::CherryBomb,
        PlantKind::CherryBomb,
        1,
        BoundarySide::RightExiting,
        true,
    ),
    live_case(
        "cherry-r2-right-miss",
        GridExplosionKind::CherryBomb,
        PlantKind::CherryBomb,
        1,
        BoundarySide::RightExiting,
        false,
    ),
    live_case(
        "cherry-r5-left-hit",
        GridExplosionKind::CherryBomb,
        PlantKind::CherryBomb,
        4,
        BoundarySide::LeftEntering,
        true,
    ),
    live_case(
        "cherry-r5-left-miss",
        GridExplosionKind::CherryBomb,
        PlantKind::CherryBomb,
        4,
        BoundarySide::LeftEntering,
        false,
    ),
    live_case(
        "cherry-r5-right-hit",
        GridExplosionKind::CherryBomb,
        PlantKind::CherryBomb,
        4,
        BoundarySide::RightExiting,
        true,
    ),
    live_case(
        "cherry-r5-right-miss",
        GridExplosionKind::CherryBomb,
        PlantKind::CherryBomb,
        4,
        BoundarySide::RightExiting,
        false,
    ),
    live_case(
        "doom-r1-left-hit",
        GridExplosionKind::DoomShroom,
        PlantKind::DoomShroom,
        0,
        BoundarySide::LeftEntering,
        true,
    ),
    live_case(
        "doom-r1-left-miss",
        GridExplosionKind::DoomShroom,
        PlantKind::DoomShroom,
        0,
        BoundarySide::LeftEntering,
        false,
    ),
    live_case(
        "doom-r1-right-hit",
        GridExplosionKind::DoomShroom,
        PlantKind::DoomShroom,
        0,
        BoundarySide::RightExiting,
        true,
    ),
    live_case(
        "doom-r1-right-miss",
        GridExplosionKind::DoomShroom,
        PlantKind::DoomShroom,
        0,
        BoundarySide::RightExiting,
        false,
    ),
    live_case(
        "doom-r2-left-hit",
        GridExplosionKind::DoomShroom,
        PlantKind::DoomShroom,
        1,
        BoundarySide::LeftEntering,
        true,
    ),
    live_case(
        "doom-r2-left-miss",
        GridExplosionKind::DoomShroom,
        PlantKind::DoomShroom,
        1,
        BoundarySide::LeftEntering,
        false,
    ),
    live_case(
        "doom-r2-right-hit",
        GridExplosionKind::DoomShroom,
        PlantKind::DoomShroom,
        1,
        BoundarySide::RightExiting,
        true,
    ),
    live_case(
        "doom-r2-right-miss",
        GridExplosionKind::DoomShroom,
        PlantKind::DoomShroom,
        1,
        BoundarySide::RightExiting,
        false,
    ),
    live_case(
        "doom-r3-left-hit",
        GridExplosionKind::DoomShroom,
        PlantKind::DoomShroom,
        2,
        BoundarySide::LeftEntering,
        true,
    ),
    live_case(
        "doom-r3-left-miss",
        GridExplosionKind::DoomShroom,
        PlantKind::DoomShroom,
        2,
        BoundarySide::LeftEntering,
        false,
    ),
    live_case(
        "doom-r3-right-hit",
        GridExplosionKind::DoomShroom,
        PlantKind::DoomShroom,
        2,
        BoundarySide::RightExiting,
        true,
    ),
    live_case(
        "doom-r3-right-miss",
        GridExplosionKind::DoomShroom,
        PlantKind::DoomShroom,
        2,
        BoundarySide::RightExiting,
        false,
    ),
    live_case(
        "cherry-repeat-r1-left-hit",
        GridExplosionKind::CherryBomb,
        PlantKind::CherryBomb,
        0,
        BoundarySide::LeftEntering,
        true,
    ),
    live_case(
        "cherry-repeat-r1-right-miss",
        GridExplosionKind::CherryBomb,
        PlantKind::CherryBomb,
        0,
        BoundarySide::RightExiting,
        false,
    ),
    live_case(
        "doom-repeat-r3-left-hit",
        GridExplosionKind::DoomShroom,
        PlantKind::DoomShroom,
        2,
        BoundarySide::LeftEntering,
        true,
    ),
    live_case(
        "doom-repeat-r3-right-miss",
        GridExplosionKind::DoomShroom,
        PlantKind::DoomShroom,
        2,
        BoundarySide::RightExiting,
        false,
    ),
    live_case_full(
        "day-cherry-cone-left-hit",
        GridExplosionKind::CherryBomb,
        PlantKind::CherryBomb,
        ZombieKind::Conehead,
        SceneKind::Day,
        0,
        BoundarySide::LeftEntering,
        true,
    ),
    live_case_full(
        "day-cherry-bucket-right-miss",
        GridExplosionKind::CherryBomb,
        PlantKind::CherryBomb,
        ZombieKind::Buckethead,
        SceneKind::Day,
        1,
        BoundarySide::RightExiting,
        false,
    ),
    live_case_full(
        "day-doom-football-left-hit",
        GridExplosionKind::DoomShroom,
        PlantKind::DoomShroom,
        ZombieKind::Football,
        SceneKind::Day,
        1,
        BoundarySide::LeftEntering,
        true,
    ),
    live_case_full(
        "day-doom-ladder-right-miss",
        GridExplosionKind::DoomShroom,
        PlantKind::DoomShroom,
        ZombieKind::Ladder,
        SceneKind::Day,
        1,
        BoundarySide::RightExiting,
        false,
    ),
    live_case_full(
        "roof-cherry-normal-left-hit",
        GridExplosionKind::CherryBomb,
        PlantKind::CherryBomb,
        ZombieKind::Normal,
        SceneKind::Roof,
        0,
        BoundarySide::LeftEntering,
        true,
    ),
    live_case_full(
        "roof-cherry-screen-right-miss",
        GridExplosionKind::CherryBomb,
        PlantKind::CherryBomb,
        ZombieKind::ScreenDoor,
        SceneKind::Roof,
        1,
        BoundarySide::RightExiting,
        false,
    ),
    live_case_full(
        "roof-doom-zomboni-left-hit",
        GridExplosionKind::DoomShroom,
        PlantKind::DoomShroom,
        ZombieKind::Zomboni,
        SceneKind::Roof,
        4,
        BoundarySide::LeftEntering,
        true,
    ),
    live_case_full(
        "roof-doom-garg-right-miss",
        GridExplosionKind::DoomShroom,
        PlantKind::DoomShroom,
        ZombieKind::Gargantuar,
        SceneKind::Roof,
        4,
        BoundarySide::RightExiting,
        false,
    ),
];

const fn live_case(
    label: &'static str, explosion_kind: GridExplosionKind, plant_kind: PlantKind, row: i32, side: BoundarySide,
    expected_hit: bool,
) -> LiveDamageCase {
    live_case_full(
        label,
        explosion_kind,
        plant_kind,
        ZombieKind::Normal,
        SceneKind::Day,
        row,
        side,
        expected_hit,
    )
}

const fn live_case_full(
    label: &'static str, explosion_kind: GridExplosionKind, plant_kind: PlantKind, zombie_kind: ZombieKind,
    scene: SceneKind, row: i32, side: BoundarySide, expected_hit: bool,
) -> LiveDamageCase {
    LiveDamageCase {
        label,
        explosion_kind,
        plant_kind,
        zombie_kind,
        scene,
        grid: Grid {
            row,
            col: LIVE_DAMAGE_COL,
        },
        side,
        expected_hit,
    }
}

#[rsvz::script]
fn script() {
    reload(MainUiOrFightUi);
    rsvz::setup::set_game_speed(1.0);
    set_zombies("普障桶门橄车梯白红");
    select_cards("IINAWPCCCC");

    let errors = Rc::new(RefCell::new(Vec::<String>::new()));

    let errors_for_rules = Rc::clone(&errors);
    rsvz::try_at(1, STOP_SPAWN_TIME, move || {
        rsvz::__private::with_board_access(|access| {
            let backend = access.backend();
            if let Err(error) = backend
                .set_zombie_spawn_stopped(true)
                .map_err(runtime_error)
                .and_then(|()| backend.set_instant_ice_and_ash_effects(true).map_err(runtime_error))
                .and_then(|()| backend.set_mushrooms_awake(true).map_err(runtime_error))
            {
                push_error(&errors_for_rules, format!("setup rule patches failed: {error}"));
            }
            Ok(())
        })
        .and_then(|result| result)
    })?;

    let ids = Rc::new(RefCell::new(std::option::Option::None::<ProbeIds>));

    let setup_ids = Rc::clone(&ids);
    let errors_for_geometry_setup = Rc::clone(&errors);
    physical_setup_at(SETUP_TIME, move || {
        rsvz::with_backend(|backend| {
            match setup_geometry_fixture(backend) {
                Ok(ids) => *setup_ids.borrow_mut() = Some(ids),
                Err(error) => {
                    push_error(
                        &errors_for_geometry_setup,
                        format!("geometry fixture setup failed: {error}"),
                    );
                    if let Err(cleanup_error) = reset_board(backend) {
                        push_error(
                            &errors_for_geometry_setup,
                            format!("geometry fixture cleanup after setup failure failed: {cleanup_error}"),
                        );
                    }
                }
            }
            Ok(())
        })
    })?;

    let probe_ids = Rc::clone(&ids);
    let errors_for_geometry_probe = Rc::clone(&errors);
    rsvz::try_at(1, PROBE_TIME, move || {
        rsvz::__private::with_board_access(|access| {
            let backend = access.backend();
            if let Err(error) = require_ids(&probe_ids).and_then(|ids| run_contact_geometry_probe(backend, ids)) {
                push_error(
                    &errors_for_geometry_probe,
                    format!("geometry query probe failed: {error}"),
                );
            }
            if let Err(cleanup_error) = reset_board(backend) {
                push_error(
                    &errors_for_geometry_probe,
                    format!("geometry query probe cleanup failed: {cleanup_error}"),
                );
            }
            Ok(())
        })
        .and_then(|result| result)
    })?;

    for (index, case) in LIVE_DAMAGE_CASES.iter().copied().enumerate() {
        let start_time = LIVE_DAMAGE_FIRST_TIME + (index as i32) * LIVE_DAMAGE_CASE_SPACING;
        register_live_damage_case(Rc::clone(&errors), start_time, case)?;
    }

    let final_report_time = LIVE_DAMAGE_FIRST_TIME
        + (LIVE_DAMAGE_CASES.len() as i32) * LIVE_DAMAGE_CASE_SPACING
        + LIVE_DAMAGE_FINAL_PADDING;
    let errors_for_report = Rc::clone(&errors);
    rsvz::try_at(1, final_report_time, move || {
        rsvz::__private::with_board_access(|access| {
            let backend = access.backend();
            if let Err(error) = reset_board(backend) {
                push_error(
                    &errors_for_report,
                    format!("final contact probe cleanup failed: {error}"),
                );
            }
            if let Err(error) = reset_probe_modifiers(backend) {
                push_error(
                    &errors_for_report,
                    format!("final contact probe modifier cleanup failed: {error}"),
                );
            }

            let errors = errors_for_report.borrow();
            if errors.is_empty() {
                Ok(())
            } else {
                Err(RuntimeError::new(format!(
                    "contact_geometry_probe failed: {}",
                    errors.join(" | ")
                )))
            }
        })
        .and_then(|result| result)
    })?;
}

fn setup_geometry_fixture<B>(backend: &mut B) -> RuntimeResult<ProbeIds>
where
    B: PlantCreateBackend
        + PlantRemoveBackend
        + ZombieCreateBackend
        + ZombieKillBackend
        + SceneEditBackend
        + GridItemEditBackend
        + PlantReadBackend
        + ZombieReadBackend,
    B::Error: Display,
{
    reset_board(backend)?;
    backend.set_scene(SceneKind::Day).map_err(runtime_error)?;
    let pea_id = {
        let plant = backend
            .new_plant(
                rsvz::core::model::CardSelection::Plant(PlantKind::Peashooter)
                    .checked()
                    .map_err(runtime_error)?,
                PEA_GRID,
            )
            .map_err(runtime_error)?;
        backend.plant_id(plant)
    };
    let normal_id = {
        let zombie = backend
            .place_zombie(
                ZombieKind::Normal,
                Grid {
                    row: PEA_GRID.row,
                    col: 6,
                },
            )
            .map_err(runtime_error)?;
        backend.zombie_id(zombie)
    };
    let wrong_row_normal_id = {
        let zombie = backend
            .place_zombie(
                ZombieKind::Normal,
                Grid {
                    row: PEA_GRID.row + 1,
                    col: 6,
                },
            )
            .map_err(runtime_error)?;
        backend.zombie_id(zombie)
    };
    let zomboni_id = {
        let zombie = backend
            .place_zombie(
                ZombieKind::Zomboni,
                Grid {
                    row: PEA_GRID.row,
                    col: 8,
                },
            )
            .map_err(runtime_error)?;
        backend.zombie_id(zombie)
    };
    Ok(ProbeIds {
        pea: pea_id,
        normal: normal_id,
        wrong_row_normal: wrong_row_normal_id,
        zomboni: zomboni_id,
    })
}

fn run_contact_geometry_probe<B>(backend: &B, ids: ProbeIds) -> RuntimeResult<()>
where
    B: ContactGeometryBackend + GridGeometryBackend + ZombieXWriteBackend,
    B::Error: Display,
{
    verify_basic_shapes(backend, ids)?;
    verify_zombie_attack_boundary(ids)?;
    verify_zombie_threat_boundary(ids)?;
    verify_plant_threat_boundary(ids.normal)?;
    Ok(())
}

fn verify_basic_shapes<B>(backend: &B, ids: ProbeIds) -> RuntimeResult<()>
where
    B: ContactGeometryBackend + GridGeometryBackend,
    B::Error: Display,
{
    let origin = rsvz::core::logic::grid::grid_to_pixel(PEA_GRID).map_err(runtime_error)?;
    let contact = rsvz::core::logic::contact::plant_contact_rect(ids.pea)
        .map_err(runtime_error)?
        .ok_or_else(|| RuntimeError::new("peashooter contact rect missing"))?;
    ensure(contact.grid == PEA_GRID, "peashooter contact grid mismatch")?;
    ensure(contact.rect.left == origin.x + 10, "peashooter contact left mismatch")?;

    let defense = rsvz::core::logic::contact::plant_defense_bounds(ids.pea, PlantDefenseKind::ChewCrushSmash)
        .map_err(runtime_error)?
        .ok_or_else(|| RuntimeError::new("peashooter defense bounds missing"))?;
    ensure(defense.grid == PEA_GRID, "peashooter defense grid mismatch")?;

    let attack = rsvz::core::logic::contact::zombie_attack_bounds(ids.normal)
        .map_err(runtime_error)?
        .ok_or_else(|| RuntimeError::new("normal zombie attack bounds missing"))?;
    ensure(attack.row == PEA_GRID.row, "normal attack row mismatch")?;

    ensure(
        !rsvz::core::logic::contact::zombie_attack_reaches_plant_geometry(ids.wrong_row_normal, ids.pea)
            .map_err(runtime_error)?,
        "wrong-row normal zombie should not reach peashooter defense geometry",
    )?;

    ensure(
        rsvz::core::logic::contact::zombie_threat_candidate(ids.normal, ZombiePlantThreatKind::DriveOver)
            .map_err(runtime_error)?
            .is_none(),
        "normal zombie should not produce a drive-over candidate",
    )?;
    let drive_over = rsvz::core::logic::contact::zombie_threat_candidate(ids.zomboni, ZombiePlantThreatKind::DriveOver)
        .map_err(runtime_error)?;
    ensure(drive_over.is_some(), "zomboni drive-over candidate missing")?;

    let cherry_threat = rsvz::core::logic::contact::grid_explosion_shape(GridExplosionKind::CherryBomb, PEA_GRID)
        .map_err(runtime_error)?;
    ensure(
        cherry_threat.kind == PlantThreatKind::CherryBomb,
        "cherry threat kind mismatch",
    )?;
    ensure(cherry_threat.circle.radius == CIRCLE_RADIUS, "cherry radius mismatch")?;

    let cob_threat = rsvz::core::logic::contact::static_cob_impact_shape(CobTarget {
        row: PEA_GRID.row,
        drop_col: 8.8,
    })
    .map_err(runtime_error)?;
    ensure(
        cob_threat.kind == PlantThreatKind::StaticCobCannonImpact,
        "static cob threat kind mismatch",
    )
}

fn verify_zombie_attack_boundary(ids: ProbeIds) -> RuntimeResult<()>
where
    rsvz::__private::CurrentBackend: ContactGeometryBackend + ZombieXWriteBackend,
{
    let transition = find_x_transition(
        ids.normal,
        SCAN_MIN_X,
        SCAN_MAX_X,
        |zombie| rsvz::core::logic::contact::zombie_attack_reaches_plant_geometry(zombie, ids.pea),
        "normal zombie attack/plant defense boundary",
    )?;
    verify_transition_is_stable(
        ids.normal,
        transition,
        |zombie| rsvz::core::logic::contact::zombie_attack_reaches_plant_geometry(zombie, ids.pea),
        "normal zombie attack/plant defense boundary",
    )
}

fn verify_zombie_threat_boundary(ids: ProbeIds) -> RuntimeResult<()>
where
    rsvz::__private::CurrentBackend: ContactGeometryBackend + ZombieXWriteBackend,
{
    let transition = find_x_transition(
        ids.zomboni,
        SCAN_MIN_X,
        SCAN_MAX_X,
        |zombie| {
            rsvz::core::logic::contact::zombie_threat_hits_plant(zombie, ids.pea, ZombiePlantThreatKind::DriveOver)
        },
        "zomboni drive-over/plant contact boundary",
    )?;
    verify_transition_is_stable(
        ids.zomboni,
        transition,
        |zombie| {
            rsvz::core::logic::contact::zombie_threat_hits_plant(zombie, ids.pea, ZombiePlantThreatKind::DriveOver)
        },
        "zomboni drive-over/plant contact boundary",
    )
}

fn verify_plant_threat_boundary(zombie: ZombieId) -> RuntimeResult<()>
where
    rsvz::__private::CurrentBackend: ContactGeometryBackend + ZombieXWriteBackend,
{
    const BASE_X: i32 = 360;
    set_zombie_x_checked(zombie, BASE_X)?;
    let base = rsvz::core::logic::contact::zombie_defense_bounds(zombie)
        .map_err(runtime_error)?
        .ok_or_else(|| RuntimeError::new("normal zombie defense bounds missing"))?;
    let center = PixelPos {
        x: base.rect.left - CIRCLE_RADIUS,
        y: base.rect.top + (base.rect.bottom - base.rect.top) / 2,
    };
    let threat = PlantThreatShape::new(
        PlantThreatKind::CherryBomb,
        PEA_GRID.row,
        0,
        DamageRangeFlags::ALL_NON_MIND_CONTROLLED,
        ContactCircle::new(center.x, center.y, CIRCLE_RADIUS),
    );
    ensure_geometry(zombie, threat, true, "circle tangent hit")?;

    let transition = find_x_transition(
        zombie,
        BASE_X - 8,
        BASE_X + 8,
        |zombie| rsvz::core::logic::contact::plant_threat_hits_zombie(threat, zombie),
        "core plant-threat/zombie defense circle boundary",
    )?;
    ensure_geometry_at(zombie, transition.miss_x, threat, false, "core circle boundary miss")?;
    ensure_geometry_at(zombie, transition.hit_x, threat, true, "core circle boundary hit")?;
    verify_positive_float_rounding_queries(zombie, threat, transition)?;
    verify_negative_float_truncation_query(zombie)?;

    let wrong_row_threat = PlantThreatShape::new(
        PlantThreatKind::CherryBomb,
        PEA_GRID.row + 1,
        0,
        DamageRangeFlags::ALL_NON_MIND_CONTROLLED,
        threat.circle,
    );
    ensure_geometry_at(
        zombie,
        transition.hit_x,
        wrong_row_threat,
        false,
        "same-circle wrong-row threat",
    )
}

fn verify_positive_float_rounding_queries(
    zombie: ZombieId, threat: PlantThreatShape, transition: XTransition,
) -> RuntimeResult<()>
where
    rsvz::__private::CurrentBackend: ContactGeometryBackend + ZombieXWriteBackend,
{
    for (index, x) in fractional_boundary_values(transition).into_iter().enumerate() {
        ensure_float_x_matches_truncated(zombie, threat, x, &format!("positive float X truncation query {index}"))?;
    }
    Ok(())
}

fn verify_negative_float_truncation_query(zombie: ZombieId) -> RuntimeResult<()>
where
    rsvz::__private::CurrentBackend: ContactGeometryBackend + ZombieXWriteBackend,
{
    const BASE_X: i32 = -24;
    set_zombie_x_checked(zombie, BASE_X)?;
    let base = rsvz::core::logic::contact::zombie_defense_bounds(zombie)
        .map_err(runtime_error)?
        .ok_or_else(|| RuntimeError::new("negative-X zombie defense bounds missing"))?;
    let center = PixelPos {
        x: base.rect.left - CIRCLE_RADIUS,
        y: base.rect.top + (base.rect.bottom - base.rect.top) / 2,
    };
    let threat = PlantThreatShape::new(
        PlantThreatKind::CherryBomb,
        PEA_GRID.row,
        0,
        DamageRangeFlags::ALL_NON_MIND_CONTROLLED,
        ContactCircle::new(center.x, center.y, CIRCLE_RADIUS),
    );
    let transition = find_x_transition(
        zombie,
        BASE_X - 8,
        BASE_X + 8,
        |zombie| rsvz::core::logic::contact::plant_threat_hits_zombie(threat, zombie),
        "negative float X truncation boundary",
    )?;
    let x = if transition.hit_x > transition.miss_x {
        transition.hit_x as f32 - 0.25
    } else {
        transition.miss_x as f32 - 0.25
    };
    ensure(
        x < 0.0,
        format!("negative float truncation probe did not remain negative: {x}"),
    )?;
    ensure_float_x_matches_truncated(zombie, threat, x, "negative float X truncates toward zero query")?;
    Ok(())
}

fn register_live_damage_case(
    errors: Rc<RefCell<Vec<String>>>, start_time: i32, case: LiveDamageCase,
) -> RuntimeResult<()> {
    let state = Rc::new(RefCell::new(std::option::Option::None::<LiveDamageProbe>));

    let setup_state = Rc::clone(&state);
    let setup_errors = Rc::clone(&errors);
    physical_setup_at(start_time, move || {
        match start_live_damage_case(case) {
            Ok(probe) => *setup_state.borrow_mut() = Some(probe),
            Err(error) => {
                push_error(&setup_errors, format!("{} setup failed: {error}", case.label));
                if let Err(cleanup_error) =
                    rsvz::__private::with_board_access(|access| reset_board(access.backend())).and_then(|result| result)
                {
                    push_error(
                        &setup_errors,
                        format!("{} cleanup after setup failure failed: {cleanup_error}", case.label),
                    );
                }
            }
        }
        Ok(())
    })?;

    for frame in (start_time + 1)..(start_time + LIVE_DAMAGE_CHECK_DELAY) {
        let pin_state = Rc::clone(&state);
        let pin_errors = Rc::clone(&errors);
        rsvz::try_at(1, frame, move || {
            rsvz::__private::with_board_access(|_access| {
                let probe = pin_state.borrow();
                if let Some(probe) = probe.as_ref() {
                    for target in &probe.targets {
                        if let Err(error) = set_zombie_x_float_if_present(target.id, target.x) {
                            push_error(
                                &pin_errors,
                                format!(
                                    "{} frame {frame} zombie pinning failed for {} at x={} (trunc {}): {error}",
                                    probe.label, target.label, target.x, target.truncated_x
                                ),
                            );
                        }
                    }
                }
                Ok(())
            })
            .and_then(|result| result)
        })?;
    }

    let verify_state = Rc::clone(&state);
    let verify_errors = Rc::clone(&errors);
    rsvz::try_at(1, start_time + LIVE_DAMAGE_CHECK_DELAY, move || {
        rsvz::__private::with_board_access(|access| {
            let backend = access.backend();
            let probe = verify_state.borrow_mut().take();
            if let Some(probe) = probe {
                if let Err(error) = verify_live_damage_case(backend, probe) {
                    push_error(&verify_errors, format!("{} verify failed: {error}", case.label));
                }
            }
            Ok(())
        })
        .and_then(|result| result)
    })?;
    Ok(())
}

fn start_live_damage_case(case: LiveDamageCase) -> RuntimeResult<LiveDamageProbe>
where
    rsvz::__private::CurrentBackend: LiveDamageProbeBackend,
{
    rsvz::with_backend(|backend| {
        reset_board(backend)?;
        backend.set_scene(case.scene).map_err(runtime_error)
    })?;
    rsvz::__private::with_board_access(|_access| {
        let zombie = rsvz::core::modifier::spawn_zombie(
            case.zombie_kind,
            Grid {
                row: case.grid.row,
                col: LIVE_DAMAGE_SPAWN_COL,
            },
        )
        .map_err(runtime_error)?;
        match rsvz::core::modifier::set_zombie_body_hp(zombie, LIVE_DAMAGE_INITIAL_HP).map_err(runtime_error)? {
            ObjectEditOutcome::Applied => {}
            ObjectEditOutcome::Missing => {
                return Err(RuntimeError::new(format!(
                    "{} zombie disappeared before HP setup",
                    case.label
                )));
            }
        }

        let threat =
            rsvz::core::logic::contact::grid_explosion_shape(case.explosion_kind, case.grid).map_err(runtime_error)?;
        let primary_span = find_hit_span(
            zombie,
            WIDE_SCAN_MIN_X,
            WIDE_SCAN_MAX_X,
            |zombie| rsvz::core::logic::contact::plant_threat_hits_zombie(threat, zombie),
            case.label,
        )?;
        let transition = edge_transition(primary_span, case.side, case.label)?;
        let boundary_x = if case.expected_hit {
            transition.hit_x
        } else {
            transition.miss_x
        };

        ensure_geometry_at(zombie, boundary_x, threat, case.expected_hit, case.label)?;

        let mut targets = vec![ExpectedZombie {
            id: zombie,
            kind: case.zombie_kind,
            row: case.grid.row,
            x: boundary_x as f32,
            truncated_x: boundary_x,
            initial_hp: LIVE_DAMAGE_INITIAL_HP,
            expected_hit: case.expected_hit,
            label: format!("primary row {} {:?}", case.grid.row + 1, case.zombie_kind),
        }];

        if should_add_float_live_targets(case) {
            add_float_live_targets(case, threat, transition, &mut targets)?;
        }

        add_strict_row_boundary_targets(case, threat, &mut targets)?;

        rsvz::core::logic::cards::new_plant(case.plant_kind, case.grid).map_err(runtime_error)?;

        Ok(LiveDamageProbe {
            label: case.label,
            explosion_kind: case.explosion_kind,
            side: case.side,
            targets,
        })
    })
    .and_then(|result| result)
}

fn add_float_live_targets(
    case: LiveDamageCase, threat: PlantThreatShape, transition: XTransition, targets: &mut Vec<ExpectedZombie>,
) -> RuntimeResult<()>
where
    rsvz::__private::CurrentBackend: LiveDamageProbeBackend,
{
    let values = fractional_boundary_values(transition);
    for (index, x) in values.into_iter().enumerate() {
        let truncated_x = pvz_trunc_f32_to_i32(x)?;
        let kind = match (case.explosion_kind, index) {
            (GridExplosionKind::CherryBomb, 0) => ZombieKind::Conehead,
            (GridExplosionKind::CherryBomb, _) => ZombieKind::Buckethead,
            (GridExplosionKind::DoomShroom, 0) => ZombieKind::Football,
            (GridExplosionKind::DoomShroom, _) => ZombieKind::Ladder,
        };
        let id = rsvz::core::modifier::spawn_zombie(
            kind,
            Grid {
                row: case.grid.row,
                col: LIVE_DAMAGE_SPAWN_COL,
            },
        )
        .map_err(runtime_error)?;
        match rsvz::core::modifier::set_zombie_body_hp(id, LIVE_DAMAGE_INITIAL_HP).map_err(runtime_error)? {
            ObjectEditOutcome::Applied => {}
            ObjectEditOutcome::Missing => {
                return Err(RuntimeError::new(format!(
                    "{} float zombie {index} disappeared before HP setup",
                    case.label
                )));
            }
        }
        let expected_hit =
            ensure_float_x_matches_truncated(id, threat, x, &format!("{} live float target {index}", case.label))?;
        targets.push(ExpectedZombie {
            id,
            kind,
            row: case.grid.row,
            x,
            truncated_x,
            initial_hp: LIVE_DAMAGE_INITIAL_HP,
            expected_hit,
            label: format!(
                "float row {} {:?} x={x} trunc={truncated_x} expected {}",
                case.grid.row + 1,
                kind,
                if expected_hit { "hit" } else { "miss" }
            ),
        });
    }
    Ok(())
}

fn add_strict_row_boundary_targets(
    case: LiveDamageCase, threat: PlantThreatShape, targets: &mut Vec<ExpectedZombie>,
) -> RuntimeResult<()>
where
    rsvz::__private::CurrentBackend: LiveDamageProbeBackend,
{
    for row in scene_rows(case.scene) {
        let kind = off_row_zombie_kind(case, row);
        if row_can_be_damaged_by_threat(threat, row) {
            let scanner = spawn_zombie_with_hp(kind, row)?;
            let span = find_hit_span(
                scanner,
                WIDE_SCAN_MIN_X,
                WIDE_SCAN_MAX_X,
                |zombie| rsvz::core::logic::contact::plant_threat_hits_zombie(threat, zombie),
                case.label,
            )?;
            let left = edge_transition(span, BoundarySide::LeftEntering, case.label)?;
            let right = edge_transition(span, BoundarySide::RightExiting, case.label)?;

            push_existing_expected_target(
                targets,
                scanner,
                kind,
                row,
                left.miss_x,
                false,
                threat,
                format!("row {} left-edge 1px miss", row + 1),
            )?;
            spawn_expected_target(
                targets,
                kind,
                row,
                left.hit_x,
                true,
                threat,
                format!("row {} left-edge 1px hit", row + 1),
            )?;
            spawn_expected_target(
                targets,
                kind,
                row,
                right.hit_x,
                true,
                threat,
                format!("row {} right-edge 1px hit", row + 1),
            )?;
            spawn_expected_target(
                targets,
                kind,
                row,
                right.miss_x,
                false,
                threat,
                format!("row {} right-edge 1px miss", row + 1),
            )?;
        } else {
            spawn_expected_target(
                targets,
                kind,
                row,
                threat.circle.x - 36,
                false,
                threat,
                format!("row {} out-of-row-range miss", row + 1),
            )?;
        }
    }
    Ok(())
}

fn spawn_expected_target(
    targets: &mut Vec<ExpectedZombie>, kind: ZombieKind, row: i32, x: i32, expected_hit: bool,
    threat: PlantThreatShape, label: String,
) -> RuntimeResult<()>
where
    rsvz::__private::CurrentBackend: LiveDamageProbeBackend,
{
    let id = spawn_zombie_with_hp(kind, row)?;
    push_existing_expected_target(targets, id, kind, row, x, expected_hit, threat, label)
}

#[allow(clippy::too_many_arguments)]
fn push_existing_expected_target(
    targets: &mut Vec<ExpectedZombie>, id: ZombieId, kind: ZombieKind, row: i32, x: i32, expected_hit: bool,
    threat: PlantThreatShape, label: String,
) -> RuntimeResult<()>
where
    rsvz::__private::CurrentBackend: LiveDamageProbeBackend,
{
    ensure_geometry_at(id, x, threat, expected_hit, "strict row boundary")?;
    targets.push(ExpectedZombie {
        id,
        kind,
        row,
        x: x as f32,
        truncated_x: x,
        initial_hp: LIVE_DAMAGE_INITIAL_HP,
        expected_hit,
        label,
    });
    Ok(())
}

fn spawn_zombie_with_hp(kind: ZombieKind, row: i32) -> RuntimeResult<ZombieId>
where
    rsvz::__private::CurrentBackend: ZombieCreateBackend + ZombieBodyHealthWriteBackend + ZombieReadBackend,
{
    let id = rsvz::core::modifier::spawn_zombie(
        kind,
        Grid {
            row,
            col: LIVE_DAMAGE_SPAWN_COL,
        },
    )
    .map_err(runtime_error)?;
    match rsvz::core::modifier::set_zombie_body_hp(id, LIVE_DAMAGE_INITIAL_HP).map_err(runtime_error)? {
        ObjectEditOutcome::Applied => Ok(id),
        ObjectEditOutcome::Missing => Err(RuntimeError::new(format!(
            "spawned {kind:?} row {} disappeared before HP setup",
            row + 1
        ))),
    }
}

fn row_can_be_damaged_by_threat(threat: PlantThreatShape, row: i32) -> bool {
    (row - threat.center_row).abs() <= threat.row_range
}

fn should_add_float_live_targets(case: LiveDamageCase) -> bool {
    matches!(
        case.label,
        "cherry-r2-left-hit"
            | "cherry-r2-right-miss"
            | "doom-r2-left-hit"
            | "doom-r2-right-miss"
            | "roof-cherry-normal-left-hit"
            | "roof-doom-zomboni-left-hit"
    )
}

fn fractional_boundary_values(transition: XTransition) -> [f32; 2] {
    if transition.hit_x > transition.miss_x {
        [transition.miss_x as f32 + 0.75, transition.hit_x as f32 + 0.25]
    } else {
        [transition.hit_x as f32 + 0.75, transition.miss_x as f32 + 0.25]
    }
}

fn verify_live_damage_case<B>(backend: &B, probe: LiveDamageProbe) -> RuntimeResult<()>
where
    B: LiveDamageProbeBackend,
    B::Error: Display,
{
    let mut errors = Vec::new();
    if let Err(error) = verify_live_damage_result(backend, &probe) {
        errors.push(error.to_string());
    }
    if let Err(error) = reset_board(backend) {
        errors.push(format!("{} cleanup failed: {error}", probe.label));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(RuntimeError::new(errors.join(" | ")))
    }
}

fn verify_live_damage_result<B>(backend: &B, probe: &LiveDamageProbe) -> RuntimeResult<()>
where
    B: LiveDamageProbeBackend,
    B::Error: Display,
{
    let mut failures = Vec::new();
    for target in &probe.targets {
        let zombie_state = backend
            .zombie(target.id)
            .map(|zombie| (backend.zombie_is_alive(zombie), backend.zombie_hp(zombie)));

        if target.expected_hit {
            let damaged = zombie_state
                .map(|(alive, hp)| !alive || hp < target.initial_hp)
                .unwrap_or(true);
            if !damaged {
                failures.push(format!(
                    "{} {} did not damage {:?} row {} at x={} trunc={} ({:?}, {:?}); zombie_state={zombie_state:?}",
                    probe.label,
                    target.label,
                    target.kind,
                    target.row + 1,
                    target.x,
                    target.truncated_x,
                    probe.explosion_kind,
                    probe.side
                ));
            }
        } else {
            let Some((alive, hp)) = zombie_state else {
                failures.push(format!(
                    "{} {} unexpectedly removed {:?} row {} at x={} trunc={} ({:?}, {:?})",
                    probe.label,
                    target.label,
                    target.kind,
                    target.row + 1,
                    target.x,
                    target.truncated_x,
                    probe.explosion_kind,
                    probe.side
                ));
                continue;
            };
            if !(alive && hp == target.initial_hp) {
                failures.push(format!(
                    "{} {} unexpectedly damaged {:?} row {} at x={} trunc={} ({:?}, {:?}); expected hp {}, got zombie_state={zombie_state:?}",
                    probe.label,
                    target.label,
                    target.kind,
                    target.row + 1,
                    target.x,
                    target.truncated_x,
                    probe.explosion_kind,
                    probe.side,
                    target.initial_hp
                ));
            }
        }
    }

    if failures.is_empty() {
        Ok(())
    } else {
        Err(RuntimeError::new(failures.join(" | ")))
    }
}

fn scene_rows(scene: SceneKind) -> std::ops::Range<i32> {
    0..(scene.row_count() as i32)
}

fn off_row_zombie_kind(case: LiveDamageCase, row: i32) -> ZombieKind {
    match (case.explosion_kind, row.rem_euclid(5)) {
        (GridExplosionKind::CherryBomb, 0) => ZombieKind::Normal,
        (GridExplosionKind::CherryBomb, 1) => ZombieKind::Conehead,
        (GridExplosionKind::CherryBomb, 2) => ZombieKind::Buckethead,
        (GridExplosionKind::CherryBomb, 3) => ZombieKind::ScreenDoor,
        (GridExplosionKind::CherryBomb, _) => ZombieKind::Football,
        (GridExplosionKind::DoomShroom, 0) => ZombieKind::Normal,
        (GridExplosionKind::DoomShroom, 1) => ZombieKind::Ladder,
        (GridExplosionKind::DoomShroom, 2) => ZombieKind::Zomboni,
        (GridExplosionKind::DoomShroom, 3) => ZombieKind::Football,
        (GridExplosionKind::DoomShroom, _) => ZombieKind::Gargantuar,
    }
}

fn edge_transition(span: HitSpan, side: BoundarySide, label: &'static str) -> RuntimeResult<XTransition> {
    match side {
        BoundarySide::LeftEntering => {
            let miss_x = span
                .leftmost_hit_x
                .checked_sub(1)
                .ok_or_else(|| RuntimeError::new(format!("{label} left boundary underflow at {span:?}")))?;
            Ok(XTransition {
                miss_x,
                hit_x: span.leftmost_hit_x,
            })
        }
        BoundarySide::RightExiting => {
            let miss_x = span
                .rightmost_hit_x
                .checked_add(1)
                .ok_or_else(|| RuntimeError::new(format!("{label} right boundary overflow at {span:?}")))?;
            Ok(XTransition {
                miss_x,
                hit_x: span.rightmost_hit_x,
            })
        }
    }
}

fn find_hit_span<F, E>(
    zombie: ZombieId, min_x: i32, max_x: i32, mut query: F, label: &'static str,
) -> RuntimeResult<HitSpan>
where
    rsvz::__private::CurrentBackend: ZombieReadBackend + ZombieXWriteBackend,
    F: FnMut(ZombieId) -> Result<bool, E>,
    E: Display,
{
    let mut leftmost_hit_x = None;
    let mut rightmost_hit_x = None;
    let mut left_endpoint = None;
    let mut right_endpoint = None;

    for x in min_x..=max_x {
        set_zombie_x_checked(zombie, x)?;
        let hit = query(zombie).map_err(runtime_error)?;
        if x == min_x {
            left_endpoint = Some(hit);
        }
        if x == max_x {
            right_endpoint = Some(hit);
        }
        if hit {
            if leftmost_hit_x.is_none() {
                leftmost_hit_x = Some(x);
            }
            rightmost_hit_x = Some(x);
        }
    }

    let Some(leftmost_hit_x) = leftmost_hit_x else {
        return Err(RuntimeError::new(format!(
            "{label} no hit span in {min_x}..={max_x}; endpoint hits: left={}, right={}",
            left_endpoint.unwrap_or(false),
            right_endpoint.unwrap_or(false)
        )));
    };
    let rightmost_hit_x = rightmost_hit_x.expect("rightmost hit exists when leftmost hit exists");
    ensure(
        leftmost_hit_x > min_x,
        format!("{label} hit span touches left scan boundary {min_x}; rightmost={rightmost_hit_x}, widen scan"),
    )?;
    ensure(
        rightmost_hit_x < max_x,
        format!("{label} hit span touches right scan boundary {max_x}; leftmost={leftmost_hit_x}, widen scan"),
    )?;

    Ok(HitSpan {
        leftmost_hit_x,
        rightmost_hit_x,
    })
}

fn find_x_transition<F, E>(
    zombie: ZombieId, min_x: i32, max_x: i32, mut query: F, label: &'static str,
) -> RuntimeResult<XTransition>
where
    rsvz::__private::CurrentBackend: ZombieReadBackend + ZombieXWriteBackend,
    F: FnMut(ZombieId) -> Result<bool, E>,
    E: Display,
{
    set_zombie_x_checked(zombie, min_x)?;
    let mut previous_x = min_x;
    let mut previous = query(zombie).map_err(runtime_error)?;
    for x in (min_x + 1)..=max_x {
        set_zombie_x_checked(zombie, x)?;
        let current = query(zombie).map_err(runtime_error)?;
        if previous != current {
            let transition = if current {
                XTransition {
                    miss_x: previous_x,
                    hit_x: x,
                }
            } else {
                XTransition {
                    miss_x: x,
                    hit_x: previous_x,
                }
            };
            ensure(
                (transition.miss_x - transition.hit_x).abs() == 1,
                "contact boundary transition was not adjacent",
            )?;
            return Ok(transition);
        }
        previous_x = x;
        previous = current;
    }
    Err(RuntimeError::new(format!(
        "{label} did not produce a one-pixel transition in {min_x}..={max_x}"
    )))
}

fn verify_transition_is_stable<F, E>(
    zombie: ZombieId, transition: XTransition, mut query: F, label: &'static str,
) -> RuntimeResult<()>
where
    rsvz::__private::CurrentBackend: ZombieReadBackend + ZombieXWriteBackend,
    F: FnMut(ZombieId) -> Result<bool, E>,
    E: Display,
{
    for (x, expected) in [
        (transition.miss_x, false),
        (transition.hit_x, true),
        (transition.miss_x, false),
        (transition.hit_x, true),
    ] {
        set_zombie_x_checked(zombie, x)?;
        let actual = query(zombie).map_err(runtime_error)?;
        ensure(
            actual == expected,
            format!("{label} unstable at x={x}: expected {expected}, got {actual}"),
        )?;
    }
    Ok(())
}

fn ensure_geometry_at(
    zombie: ZombieId, x: i32, threat: PlantThreatShape, expected: bool, label: &'static str,
) -> RuntimeResult<()>
where
    rsvz::__private::CurrentBackend: ContactGeometryBackend + ZombieXWriteBackend,
{
    set_zombie_x_checked(zombie, x)?;
    ensure_geometry(zombie, threat, expected, label)
}

fn ensure_geometry(zombie: ZombieId, threat: PlantThreatShape, expected: bool, label: &'static str) -> RuntimeResult<()>
where
    rsvz::__private::CurrentBackend: ContactGeometryBackend,
{
    let geometry = rsvz::core::logic::contact::plant_threat_hits_zombie(threat, zombie).map_err(runtime_error)?;
    ensure(
        geometry == expected,
        format!("{label} expected {expected}, got {geometry}"),
    )
}

fn ensure_float_x_matches_truncated(
    zombie: ZombieId, threat: PlantThreatShape, x: f32, label: &str,
) -> RuntimeResult<bool>
where
    rsvz::__private::CurrentBackend: ContactGeometryBackend + ZombieXWriteBackend,
{
    let truncated_x = pvz_trunc_f32_to_i32(x)?;
    set_zombie_x_checked(zombie, truncated_x)?;
    let expected_geometry =
        rsvz::core::logic::contact::plant_threat_hits_zombie(threat, zombie).map_err(runtime_error)?;
    set_zombie_x_float_checked(zombie, x)?;
    let geometry = rsvz::core::logic::contact::plant_threat_hits_zombie(threat, zombie).map_err(runtime_error)?;
    ensure(
        geometry == expected_geometry,
        format!(
            "{label} float geometry mismatch at x={x} trunc={truncated_x}: expected {expected_geometry}, got {geometry}"
        ),
    )?;
    Ok(geometry)
}

fn set_zombie_x_checked(zombie: ZombieId, x: i32) -> RuntimeResult<()>
where
    rsvz::__private::CurrentBackend: ZombieReadBackend + ZombieXWriteBackend,
{
    match rsvz::core::modifier::set_zombie_x(zombie, x as f32).map_err(runtime_error)? {
        ObjectEditOutcome::Applied => Ok(()),
        ObjectEditOutcome::Missing => Err(RuntimeError::new(format!(
            "zombie {zombie:?} disappeared while setting x={x}"
        ))),
    }
}

fn set_zombie_x_float_checked(zombie: ZombieId, x: f32) -> RuntimeResult<()>
where
    rsvz::__private::CurrentBackend: ZombieReadBackend + ZombieXWriteBackend,
{
    let _truncated_x = pvz_trunc_f32_to_i32(x)?;
    match rsvz::core::modifier::set_zombie_x(zombie, x).map_err(runtime_error)? {
        ObjectEditOutcome::Applied => Ok(()),
        ObjectEditOutcome::Missing => Err(RuntimeError::new(format!(
            "zombie {zombie:?} disappeared while setting float x={x}"
        ))),
    }
}

fn set_zombie_x_float_if_present(zombie: ZombieId, x: f32) -> RuntimeResult<()>
where
    rsvz::__private::CurrentBackend: ZombieReadBackend + ZombieXWriteBackend,
{
    let _truncated_x = pvz_trunc_f32_to_i32(x)?;
    match rsvz::core::modifier::set_zombie_x(zombie, x).map_err(runtime_error)? {
        ObjectEditOutcome::Applied | ObjectEditOutcome::Missing => Ok(()),
    }
}

fn pvz_trunc_f32_to_i32(x: f32) -> RuntimeResult<i32> {
    if !x.is_finite() || x < i32::MIN as f32 || x >= 2_147_483_648.0 {
        return Err(RuntimeError::new(format!(
            "float x={x} is not representable as PvZ i32 coordinate"
        )));
    }
    Ok(x as i32)
}

fn require_ids(ids: &RefCell<Option<ProbeIds>>) -> RuntimeResult<ProbeIds> {
    ids.borrow()
        .as_ref()
        .copied()
        .ok_or_else(|| RuntimeError::new("contact geometry probe ids were not recorded"))
}

fn reset_board<B>(backend: &B) -> RuntimeResult<()>
where
    B: PlantRemoveBackend + ZombieKillBackend + GridItemEditBackend + ZombieReadBackend,
    B::Error: Display,
{
    clear_editable_board(backend)?;
    for zombie in backend.zombies() {
        backend.kill_zombie(zombie).map_err(runtime_error)?;
    }
    Ok(())
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

fn reset_probe_modifiers<B>(backend: &B) -> RuntimeResult<()>
where
    B: PlantEffectRuleEditBackend + ZombieRuleEditBackend,
    B::Error: Display,
{
    backend.set_zombie_spawn_stopped(false).map_err(runtime_error)?;
    backend.set_instant_ice_and_ash_effects(false).map_err(runtime_error)?;
    backend.set_mushrooms_awake(false).map_err(runtime_error)
}

fn push_error(errors: &Rc<RefCell<Vec<String>>>, message: impl Into<String>) {
    errors.borrow_mut().push(message.into());
}

trait LiveDamageProbeBackend:
    ContactGeometryBackend
    + PlantCreateBackend
    + PlantRemoveBackend
    + ZombieCreateBackend
    + ZombieKillBackend
    + SceneEditBackend
    + GridItemEditBackend
    + PlantEffectRuleEditBackend
    + PlantReadBackend
    + ZombieBodyHealthWriteBackend
    + ZombieReadBackend
    + ZombieXWriteBackend
{
}

impl<T> LiveDamageProbeBackend for T where
    T: ContactGeometryBackend
        + PlantCreateBackend
        + PlantRemoveBackend
        + ZombieCreateBackend
        + ZombieKillBackend
        + SceneEditBackend
        + GridItemEditBackend
        + PlantEffectRuleEditBackend
        + PlantReadBackend
        + ZombieBodyHealthWriteBackend
        + ZombieReadBackend
        + ZombieXWriteBackend
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
