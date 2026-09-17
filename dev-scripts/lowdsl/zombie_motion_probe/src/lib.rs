use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use rsvz::core::logic::cleanup::{clear_plants, kill_all_zombies};
use rsvz::core::logic::predict_stable_zombie_x_trace;
use rsvz::core::model::{
    Grid, PlantId, PlantKind, UniformZombieMotion, ZombieId, ZombieKind, ZombieMotionDirection, ZombieMotionState,
    ZombieMovementModel, ZombieTrackProfile,
};
use rsvz::core::runtime::{RuntimeError, RuntimeResult};
use rsvz::prelude::{
    PlantCreateBackend, PlantRemoveBackend, ZombieCreateBackend, ZombieRawFactsBackend, ZombieRemoveBackend,
};

const DEFAULT_TRACE_HORIZON: usize = 400;
const POGO_BOUNCE_TRACE_HORIZON: usize = 60;
const DEFAULT_REPREDICT_FRAME: usize = 200;
const DRIVE_START_FRAME: i32 = -580;
const REPORT_FRAME: i32 = 4625;
const MAX_OBSERVATIONS: usize = 40;

#[derive(Clone, Copy)]
struct ProbeDescriptor {
    label: &'static str,
    kind: ZombieKind,
    row: i32,
    col: i32,
    expected: ExpectedMotion,
    expected_direction: ZombieMotionDirection,
    speed_range: Option<SpeedRange>,
    setup_frame: i32,
    search_start_frame: i32,
    search_end_frame: i32,
    horizon: usize,
    plants: &'static [PlantSpec],
    coverage_key: Option<&'static str>,
    note: Option<&'static str>,
}

#[derive(Clone, Copy)]
enum ExpectedMotion {
    NormalWalk,
    Track(ZombieTrackProfile),
    Uniform(UniformZombieMotion),
}

#[derive(Clone, Copy)]
struct PlantSpec {
    label: &'static str,
    kind: PlantKind,
    row: i32,
    col: i32,
}

#[derive(Clone, Copy)]
struct SpeedRange {
    min: f32,
    max: f32,
}

struct ActiveProbe {
    descriptor: ProbeDescriptor,
    id: Option<ZombieId>,
    plant_ids: Vec<PlantId>,
    prediction: Option<PredictionTrace>,
    observations: Vec<String>,
    last_observation: Option<String>,
    failure: Option<String>,
    completed: bool,
}

struct PredictionTrace {
    id: ZombieId,
    label: &'static str,
    start_frame: i32,
    horizon: usize,
    repredict_frame: usize,
    expected_x: Vec<f32>,
    actual_x: Vec<f32>,
    max_delta_frame: usize,
    max_delta: f32,
    first_bit_mismatch: Option<BitMismatch>,
    repredicted: Option<Vec<f32>>,
    repredict_max_delta: Option<(usize, f32)>,
    repredict_first_bit_mismatch: Option<BitMismatch>,
    failure: Option<String>,
    coverage_key: Option<&'static str>,
}

#[derive(Clone, Copy, Debug)]
struct BitMismatch {
    frame: usize,
    expected: f32,
    expected_bits: u32,
    actual: f32,
    actual_bits: u32,
    delta: f32,
}

impl BitMismatch {
    fn new(frame: usize, expected: f32, actual: f32) -> Self {
        Self {
            frame,
            expected,
            expected_bits: expected.to_bits(),
            actual,
            actual_bits: actual.to_bits(),
            delta: (actual - expected).abs(),
        }
    }
}

const NORMAL_CANDIDATES_SETUP: i32 = -590;
const DIRECT_A_SETUP: i32 = -100;
const DIRECT_B_SETUP: i32 = 350;
const DIRECT_C_SETUP: i32 = 800;
const INDUCED_A_SETUP: i32 = 1320;
const INDUCED_B_SETUP: i32 = 2580;

const NORMAL_CANDIDATE_PROBES: &[ProbeDescriptor] = &[
    normal_candidate("normal_candidate_normal_0", ZombieKind::Normal, 0, 8),
    normal_candidate("normal_candidate_flag", ZombieKind::Flag, 1, 8),
    normal_candidate("normal_candidate_conehead_0", ZombieKind::Conehead, 4, 8),
    normal_candidate("normal_candidate_buckethead_0", ZombieKind::Buckethead, 0, 7),
    normal_candidate("normal_candidate_screen_door_0", ZombieKind::ScreenDoor, 1, 7),
    normal_candidate("normal_candidate_normal_1", ZombieKind::Normal, 4, 7),
    normal_candidate("normal_candidate_conehead_1", ZombieKind::Conehead, 0, 6),
    normal_candidate("normal_candidate_buckethead_1", ZombieKind::Buckethead, 1, 6),
    normal_candidate("normal_candidate_screen_door_1", ZombieKind::ScreenDoor, 4, 6),
    normal_candidate("normal_candidate_normal_2", ZombieKind::Normal, 0, 5),
    normal_candidate("normal_candidate_conehead_2", ZombieKind::Conehead, 1, 5),
    normal_candidate("normal_candidate_buckethead_2", ZombieKind::Buckethead, 4, 5),
    normal_candidate("normal_candidate_screen_door_2", ZombieKind::ScreenDoor, 0, 4),
    normal_candidate("normal_candidate_normal_3", ZombieKind::Normal, 1, 4),
    normal_candidate("normal_candidate_conehead_3", ZombieKind::Conehead, 4, 4),
    normal_candidate("normal_candidate_buckethead_3", ZombieKind::Buckethead, 0, 3),
    normal_candidate("normal_candidate_screen_door_3", ZombieKind::ScreenDoor, 1, 3),
    normal_candidate("normal_candidate_normal_4", ZombieKind::Normal, 4, 8),
    normal_candidate("normal_candidate_conehead_4", ZombieKind::Conehead, 0, 7),
    normal_candidate("normal_candidate_buckethead_4", ZombieKind::Buckethead, 1, 7),
    normal_candidate("normal_candidate_screen_door_4", ZombieKind::ScreenDoor, 4, 7),
    normal_candidate("normal_candidate_normal_5", ZombieKind::Normal, 0, 6),
    normal_candidate("normal_candidate_conehead_5", ZombieKind::Conehead, 1, 6),
    normal_candidate("normal_candidate_buckethead_5", ZombieKind::Buckethead, 4, 6),
    normal_candidate("normal_candidate_screen_door_5", ZombieKind::ScreenDoor, 0, 5),
    normal_candidate("normal_candidate_normal_6", ZombieKind::Normal, 1, 5),
    normal_candidate("normal_candidate_conehead_6", ZombieKind::Conehead, 4, 5),
    normal_candidate("normal_candidate_buckethead_6", ZombieKind::Buckethead, 0, 4),
    normal_candidate("normal_candidate_screen_door_6", ZombieKind::ScreenDoor, 1, 4),
    normal_candidate("normal_candidate_normal_7", ZombieKind::Normal, 4, 4),
    normal_candidate("normal_candidate_conehead_7", ZombieKind::Conehead, 0, 3),
    normal_candidate("normal_candidate_buckethead_7", ZombieKind::Buckethead, 1, 3),
];

const DIRECT_A_PROBES: &[ProbeDescriptor] = &[
    direct_probe(
        "football",
        ZombieKind::Football,
        0,
        8,
        ExpectedMotion::Track(ZombieTrackProfile::FootballWalk),
        "football_walk",
        DIRECT_A_SETUP,
    ),
    direct_probe(
        "zomboni",
        ZombieKind::Zomboni,
        1,
        8,
        ExpectedMotion::Uniform(UniformZombieMotion::Zomboni),
        "zomboni_drive",
        DIRECT_A_SETUP,
    ),
    direct_probe(
        "jackbox",
        ZombieKind::JackInTheBox,
        2,
        8,
        ExpectedMotion::Track(ZombieTrackProfile::JackBoxWalk),
        "jackbox_walk",
        DIRECT_A_SETUP,
    ),
    direct_probe(
        "balloon_flying",
        ZombieKind::Balloon,
        3,
        8,
        ExpectedMotion::Uniform(UniformZombieMotion::BalloonFlying),
        "balloon_flying",
        DIRECT_A_SETUP,
    ),
    direct_probe(
        "catapult",
        ZombieKind::Catapult,
        4,
        8,
        ExpectedMotion::Uniform(UniformZombieMotion::CatapultDriving),
        "catapult_driving",
        DIRECT_A_SETUP,
    ),
];

const DIRECT_B_PROBES: &[ProbeDescriptor] = &[
    direct_probe(
        "digger_tunneling",
        ZombieKind::Digger,
        0,
        8,
        ExpectedMotion::Uniform(UniformZombieMotion::DiggerTunneling),
        "digger_tunneling",
        DIRECT_B_SETUP,
    ),
    direct_probe(
        "gargantuar",
        ZombieKind::Gargantuar,
        1,
        6,
        ExpectedMotion::Track(ZombieTrackProfile::GargantuarWalk),
        "gargantuar_walk",
        DIRECT_B_SETUP,
    ),
    direct_probe(
        "giga_gargantuar",
        ZombieKind::GigaGargantuar,
        2,
        6,
        ExpectedMotion::Track(ZombieTrackProfile::GargantuarWalk),
        "giga_gargantuar_walk",
        DIRECT_B_SETUP,
    ),
    direct_probe(
        "ladder_carrying",
        ZombieKind::Ladder,
        3,
        6,
        ExpectedMotion::Track(ZombieTrackProfile::LadderWalk),
        "ladder_carrying",
        DIRECT_B_SETUP,
    ),
    ProbeDescriptor {
        label: "yeti",
        kind: ZombieKind::Yeti,
        row: 4,
        col: 6,
        expected: ExpectedMotion::Track(ZombieTrackProfile::YetiWalk),
        expected_direction: ZombieMotionDirection::Left,
        speed_range: None,
        setup_frame: DIRECT_B_SETUP,
        search_start_frame: DIRECT_B_SETUP + 10,
        search_end_frame: DIRECT_B_SETUP + 40,
        horizon: DEFAULT_TRACE_HORIZON,
        plants: &[],
        coverage_key: Some("yeti_walk"),
        note: Some(
            "direct-spawn Yeti phase counter starts at 1500..=2000, so this 400-frame trace stays in normal-with-object walk; later escape is unsupported",
        ),
    },
];

const DIRECT_C_PROBES: &[ProbeDescriptor] = &[
    direct_probe(
        "pole_before_jump",
        ZombieKind::PoleVaulting,
        0,
        8,
        ExpectedMotion::Track(ZombieTrackProfile::PoleVaultBeforeJump),
        "pole_before_jump",
        DIRECT_C_SETUP,
    ),
    direct_probe(
        "newspaper_reading",
        ZombieKind::Newspaper,
        1,
        8,
        ExpectedMotion::Track(ZombieTrackProfile::NewspaperWalk),
        "newspaper_reading",
        DIRECT_C_SETUP,
    ),
    ProbeDescriptor {
        label: "pogo_bouncing",
        kind: ZombieKind::Pogo,
        row: 2,
        col: 8,
        expected: ExpectedMotion::Uniform(UniformZombieMotion::PogoBouncing),
        expected_direction: ZombieMotionDirection::Left,
        speed_range: None,
        setup_frame: DIRECT_C_SETUP,
        search_start_frame: DIRECT_C_SETUP + 10,
        search_end_frame: DIRECT_C_SETUP + 90,
        horizon: POGO_BOUNCE_TRACE_HORIZON,
        plants: &[],
        coverage_key: Some("pogo_bouncing_short_window"),
        note: Some(
            "short-window validation only: pogo refreshes speed every bounce, so 400-frame constant-speed validation is intentionally not claimed",
        ),
    },
];

const POLE_POST_PLANTS: &[PlantSpec] = &[PlantSpec {
    label: "vault_wallnut",
    kind: PlantKind::WallNut,
    row: 0,
    col: 5,
}];

const LADDER_MAGNET_PLANTS: &[PlantSpec] = &[
    PlantSpec {
        label: "ladder_magnet",
        kind: PlantKind::MagnetShroom,
        row: 1,
        col: 6,
    },
    PlantSpec {
        label: "ladder_magnet_coffee",
        kind: PlantKind::CoffeeBean,
        row: 1,
        col: 6,
    },
];

const POGO_MAGNET_PLANTS: &[PlantSpec] = &[
    PlantSpec {
        label: "pogo_magnet",
        kind: PlantKind::MagnetShroom,
        row: 2,
        col: 6,
    },
    PlantSpec {
        label: "pogo_magnet_coffee",
        kind: PlantKind::CoffeeBean,
        row: 2,
        col: 6,
    },
];

const BALLOON_WALK_PLANTS: &[PlantSpec] = &[PlantSpec {
    label: "balloon_cactus",
    kind: PlantKind::Cactus,
    row: 4,
    col: 0,
}];

const INDUCED_A_PROBES: &[ProbeDescriptor] = &[
    ProbeDescriptor {
        label: "pole_after_jump",
        kind: ZombieKind::PoleVaulting,
        row: 0,
        col: 8,
        expected: ExpectedMotion::Track(ZombieTrackProfile::PoleVaultAfterJump),
        expected_direction: ZombieMotionDirection::Left,
        speed_range: None,
        setup_frame: INDUCED_A_SETUP,
        search_start_frame: INDUCED_A_SETUP + 10,
        search_end_frame: INDUCED_A_SETUP + 500,
        horizon: DEFAULT_TRACE_HORIZON,
        plants: POLE_POST_PLANTS,
        coverage_key: Some("pole_after_jump"),
        note: Some("induced by wall-nut vault; plant is removed as soon as post-vault walk is sampled"),
    },
    ProbeDescriptor {
        label: "ladder_no_ladder",
        kind: ZombieKind::Ladder,
        row: 1,
        col: 8,
        expected: ExpectedMotion::Track(ZombieTrackProfile::LadderWalk),
        expected_direction: ZombieMotionDirection::Left,
        speed_range: Some(SpeedRange { min: 0.0, max: 0.4 }),
        setup_frame: INDUCED_A_SETUP,
        search_start_frame: INDUCED_A_SETUP + 10,
        search_end_frame: INDUCED_A_SETUP + 500,
        horizon: DEFAULT_TRACE_HORIZON,
        plants: LADDER_MAGNET_PLANTS,
        coverage_key: Some("ladder_no_ladder"),
        note: Some("induced by magnet-shroom stealing the carried ladder"),
    },
    ProbeDescriptor {
        label: "pogo_walking",
        kind: ZombieKind::Pogo,
        row: 2,
        col: 8,
        expected: ExpectedMotion::Track(ZombieTrackProfile::PogoWalk),
        expected_direction: ZombieMotionDirection::Left,
        speed_range: None,
        setup_frame: INDUCED_A_SETUP,
        search_start_frame: INDUCED_A_SETUP + 10,
        search_end_frame: INDUCED_A_SETUP + 500,
        horizon: DEFAULT_TRACE_HORIZON,
        plants: POGO_MAGNET_PLANTS,
        coverage_key: Some("pogo_walking"),
        note: Some("induced by magnet-shroom removing the pogo stick"),
    },
    ProbeDescriptor {
        label: "balloon_walking",
        kind: ZombieKind::Balloon,
        row: 4,
        col: 8,
        expected: ExpectedMotion::Track(ZombieTrackProfile::BalloonWalk),
        expected_direction: ZombieMotionDirection::Left,
        speed_range: None,
        setup_frame: INDUCED_A_SETUP,
        search_start_frame: INDUCED_A_SETUP + 10,
        search_end_frame: INDUCED_A_SETUP + 700,
        horizon: DEFAULT_TRACE_HORIZON,
        plants: BALLOON_WALK_PLANTS,
        coverage_key: Some("balloon_walking"),
        note: Some("induced by cactus popping the balloon"),
    },
];

const NEWSPAPER_ANGRY_PLANTS: &[PlantSpec] = &[
    PlantSpec {
        label: "newspaper_peashooter_a",
        kind: PlantKind::Peashooter,
        row: 0,
        col: 0,
    },
    PlantSpec {
        label: "newspaper_peashooter_b",
        kind: PlantKind::Peashooter,
        row: 0,
        col: 1,
    },
];

const DIGGER_WITHOUT_AXE_PLANTS: &[PlantSpec] = &[
    PlantSpec {
        label: "digger_magnet",
        kind: PlantKind::MagnetShroom,
        row: 2,
        col: 5,
    },
    PlantSpec {
        label: "digger_magnet_coffee",
        kind: PlantKind::CoffeeBean,
        row: 2,
        col: 5,
    },
];

const INDUCED_B_PROBES: &[ProbeDescriptor] = &[
    ProbeDescriptor {
        label: "newspaper_angry",
        kind: ZombieKind::Newspaper,
        row: 0,
        col: 8,
        expected: ExpectedMotion::Track(ZombieTrackProfile::NewspaperWalk),
        expected_direction: ZombieMotionDirection::Left,
        speed_range: Some(SpeedRange { min: 0.8, max: 1.0 }),
        setup_frame: INDUCED_B_SETUP,
        search_start_frame: INDUCED_B_SETUP + 10,
        search_end_frame: INDUCED_B_SETUP + 1500,
        horizon: DEFAULT_TRACE_HORIZON,
        plants: NEWSPAPER_ANGRY_PLANTS,
        coverage_key: Some("newspaper_angry"),
        note: Some("induced by controlled peashooter damage; plants are removed once angry walk is sampled"),
    },
    ProbeDescriptor {
        label: "digger_walking_with_axe",
        kind: ZombieKind::Digger,
        row: 1,
        col: 0,
        expected: ExpectedMotion::Track(ZombieTrackProfile::DiggerWalk),
        expected_direction: ZombieMotionDirection::Right,
        speed_range: None,
        setup_frame: INDUCED_B_SETUP,
        search_start_frame: INDUCED_B_SETUP + 10,
        search_end_frame: INDUCED_B_SETUP + 1500,
        horizon: DEFAULT_TRACE_HORIZON,
        plants: &[],
        coverage_key: Some("digger_walking_with_axe"),
        note: Some("induced by spawning near the left edge and waiting for rise/stun/walk"),
    },
    ProbeDescriptor {
        label: "digger_walking_without_axe",
        kind: ZombieKind::Digger,
        row: 2,
        col: 8,
        expected: ExpectedMotion::Track(ZombieTrackProfile::DiggerWalk),
        expected_direction: ZombieMotionDirection::Left,
        speed_range: None,
        setup_frame: INDUCED_B_SETUP,
        search_start_frame: INDUCED_B_SETUP + 10,
        search_end_frame: INDUCED_B_SETUP + 1500,
        horizon: DEFAULT_TRACE_HORIZON,
        plants: DIGGER_WITHOUT_AXE_PLANTS,
        coverage_key: Some("digger_walking_without_axe"),
        note: Some("induced by magnet-shroom stealing the pickaxe during tunneling"),
    },
];

const PROBE_BATCHES: &[&[ProbeDescriptor]] = &[
    NORMAL_CANDIDATE_PROBES,
    DIRECT_A_PROBES,
    DIRECT_B_PROBES,
    DIRECT_C_PROBES,
    INDUCED_A_PROBES,
    INDUCED_B_PROBES,
];

const LIVE_UNAVAILABLE_PROBES: &[(&str, &str)] = &[
    (
        "normal_swim",
        "ordinary live reachability is unavailable because pool lanes use DuckyTube-specific kinds; covered by raw-facts backend tests",
    ),
    (
        "normal_dance",
        "no safe-facing runtime dance-mode toggle exists in this probe; covered by raw-facts backend tests",
    ),
];

const REQUIRED_COVERAGE: &[&str] = &[
    "normal_walk_a",
    "normal_walk_b",
    "football_walk",
    "zomboni_drive",
    "jackbox_walk",
    "balloon_flying",
    "catapult_driving",
    "digger_tunneling",
    "gargantuar_walk",
    "giga_gargantuar_walk",
    "ladder_carrying",
    "yeti_walk",
    "pole_before_jump",
    "newspaper_reading",
    "pogo_bouncing_short_window",
    "pole_after_jump",
    "ladder_no_ladder",
    "pogo_walking",
    "balloon_walking",
    "newspaper_angry",
    "digger_walking_with_axe",
    "digger_walking_without_axe",
];

const fn normal_candidate(label: &'static str, kind: ZombieKind, row: i32, col: i32) -> ProbeDescriptor {
    ProbeDescriptor {
        label,
        kind,
        row,
        col,
        expected: ExpectedMotion::NormalWalk,
        expected_direction: ZombieMotionDirection::Left,
        speed_range: None,
        setup_frame: NORMAL_CANDIDATES_SETUP,
        search_start_frame: NORMAL_CANDIDATES_SETUP + 10,
        search_end_frame: NORMAL_CANDIDATES_SETUP + 50,
        horizon: DEFAULT_TRACE_HORIZON,
        plants: &[],
        coverage_key: None,
        note: Some("normal-family random walk variant candidate; rows avoid pool lanes when present"),
    }
}

const fn direct_probe(
    label: &'static str, kind: ZombieKind, row: i32, col: i32, expected: ExpectedMotion, coverage_key: &'static str,
    setup_frame: i32,
) -> ProbeDescriptor {
    ProbeDescriptor {
        label,
        kind,
        row,
        col,
        expected,
        expected_direction: ZombieMotionDirection::Left,
        speed_range: None,
        setup_frame,
        search_start_frame: setup_frame + 10,
        search_end_frame: setup_frame + 40,
        horizon: DEFAULT_TRACE_HORIZON,
        plants: &[],
        coverage_key: Some(coverage_key),
        note: Some("direct-spawn branch-key probe"),
    }
}

#[rsvz::script]
fn script() {
    reload(MainUiOrFightUi);
    rsvz::setup::set_game_speed(1.0);
    set_zombies("普车橄气矿篮白红梯雪");
    select_cards("IIKAWPCCCC");

    let active_probes = Rc::new(RefCell::new(Vec::<ActiveProbe>::new()));
    let probe_errors = Rc::new(RefCell::new(Vec::<String>::new()));

    for batch in PROBE_BATCHES {
        let active_for_setup = Rc::clone(&active_probes);
        let errors_for_setup = Rc::clone(&probe_errors);
        let batch = *batch;
        let setup_frame = batch.first().expect("probe batch must not be empty").setup_frame;
        rsvz::try_at(1, setup_frame, move || {
            rsvz::__private::with_board_access(|_access| {
                if let Err(error) = clear_plants() {
                    errors_for_setup
                        .borrow_mut()
                        .push(format!("batch {setup_frame}: clear plants failed: {error}"));
                }
                if let Err(error) = kill_all_zombies() {
                    errors_for_setup
                        .borrow_mut()
                        .push(format!("batch {setup_frame}: initial zombie cleanup failed: {error}"));
                }
                for descriptor in batch {
                    active_for_setup.borrow_mut().push(setup_probe(*descriptor));
                }
                Ok(())
            })
            .and_then(|result| result)
        })?;
    }

    for frame in DRIVE_START_FRAME..=REPORT_FRAME {
        let active_for_drive = Rc::clone(&active_probes);
        let errors_for_drive = Rc::clone(&probe_errors);
        rsvz::try_at(1, frame, move || {
            rsvz::__private::with_board_access(|access| {
                let backend = access.backend();
                let mut active = active_for_drive.borrow_mut();
                for probe in active.iter_mut() {
                    if let Err(error) = drive_probe_frame(backend, probe, frame) {
                        errors_for_drive
                            .borrow_mut()
                            .push(format!("{} frame {frame}: {error}", probe.descriptor.label));
                    }
                }
                Ok(())
            })
            .and_then(|result| result)
        })?;
    }

    let active_for_report = Rc::clone(&active_probes);
    let errors_for_report = Rc::clone(&probe_errors);
    rsvz::try_at(1, REPORT_FRAME + 1, move || {
        rsvz::__private::with_board_access(|_access| {
            let active = active_for_report.borrow();
            let probe_errors = errors_for_report.borrow();
            let mut failures = validation_errors(&active, &probe_errors);
            if let Err(error) = clear_plants() {
                failures.push(format!("final plant cleanup failed: {error}"));
            }
            if let Err(error) = kill_all_zombies() {
                failures.push(format!("final zombie cleanup failed: {error}"));
            }
            if failures.is_empty() {
                Ok(())
            } else {
                let mut details = failures;
                details.extend(diagnostic_lines(&active, &probe_errors));
                Err(RuntimeError::new(format!(
                    "zombie_motion_probe failed: {}",
                    details.join(" | ")
                )))
            }
        })
        .and_then(|result| result)
    })?;
}

fn setup_probe(descriptor: ProbeDescriptor) -> ActiveProbe
where
    rsvz::__private::CurrentBackend: PlantCreateBackend + ZombieCreateBackend,
{
    let mut probe = ActiveProbe {
        descriptor,
        id: None,
        plant_ids: Vec::new(),
        prediction: None,
        observations: Vec::new(),
        last_observation: None,
        failure: None,
        completed: false,
    };

    for plant in descriptor.plants {
        match create_probe_plant(*plant) {
            Ok(id) => probe.plant_ids.push(id),
            Err(error) => {
                probe.failure = Some(format!("plant {} setup failed: {error}", plant.label));
                return probe;
            }
        }
    }

    match spawn_probe_zombie(descriptor.kind, descriptor.row, descriptor.col) {
        Ok(id) => probe.id = Some(id),
        Err(error) => probe.failure = Some(format!("spawn failed: {error}")),
    }
    probe
}

fn create_probe_plant(plant: PlantSpec) -> RuntimeResult<PlantId>
where
    rsvz::__private::CurrentBackend: PlantCreateBackend,
{
    rsvz::core::logic::cards::new_plant(
        plant.kind,
        Grid {
            row: plant.row,
            col: plant.col,
        },
    )
    .map_err(|error| RuntimeError::new(error.to_string()))
}

fn spawn_probe_zombie(kind: ZombieKind, row: i32, col: i32) -> RuntimeResult<ZombieId>
where
    rsvz::__private::CurrentBackend: ZombieCreateBackend,
{
    rsvz::core::modifier::spawn_zombie(kind, Grid { row, col }).map_err(|error| RuntimeError::new(error.to_string()))
}

fn drive_probe_frame<B>(backend: &B, probe: &mut ActiveProbe, frame: i32) -> RuntimeResult<()>
where
    B: PlantRemoveBackend + ZombieRawFactsBackend + ZombieRemoveBackend,
{
    if probe.completed || probe.failure.is_some() || frame < probe.descriptor.search_start_frame {
        return Ok(());
    }
    let Some(id) = probe.id else {
        return Ok(());
    };

    if probe.prediction.is_some() {
        drive_running_prediction(backend, probe, frame, id)
    } else if frame <= probe.descriptor.search_end_frame {
        search_for_target_state(backend, probe, frame, id)
    } else {
        probe.failure = Some(format!(
            "target state was not reached in search window {}..={}; observations=[{}]",
            probe.descriptor.search_start_frame,
            probe.descriptor.search_end_frame,
            probe.observations.join(" | ")
        ));
        cleanup_probe_objects(backend, probe);
        Ok(())
    }
}

fn search_for_target_state<B>(backend: &B, probe: &mut ActiveProbe, frame: i32, id: ZombieId) -> RuntimeResult<()>
where
    B: PlantRemoveBackend + ZombieRawFactsBackend + ZombieRemoveBackend,
{
    let Some(state) = rsvz::core::logic::zombie_motion_state(id) else {
        probe.failure = Some("motion state unavailable during target search".to_owned());
        cleanup_probe_objects(backend, probe);
        return Ok(());
    };
    record_observation(probe, frame, &state);
    if expect_motion(&state, probe.descriptor).is_err() {
        return Ok(());
    }

    match build_prediction_trace(&state, id, probe.descriptor, frame) {
        Ok(prediction) => {
            cleanup_probe_plants(backend, probe);
            probe.prediction = Some(prediction);
        }
        Err(error) => {
            probe.failure = Some(error);
            cleanup_probe_objects(backend, probe);
        }
    }
    Ok(())
}

fn drive_running_prediction<B>(backend: &B, probe: &mut ActiveProbe, frame: i32, id: ZombieId) -> RuntimeResult<()>
where
    B: PlantRemoveBackend + ZombieRawFactsBackend + ZombieRemoveBackend,
{
    let Some(prediction) = probe.prediction.as_mut() else {
        return Ok(());
    };
    let Ok(offset) = usize::try_from(frame - prediction.start_frame) else {
        return Ok(());
    };
    if offset == 0 || offset > prediction.horizon {
        return Ok(());
    }

    let Some(state) = rsvz::core::logic::zombie_motion_state(id) else {
        prediction.failure = Some(format!("frame {offset}: motion state unavailable"));
        cleanup_probe_objects(backend, probe);
        return Ok(());
    };
    if let Err(error) = expect_motion(&state, probe.descriptor) {
        prediction.failure = Some(format!("frame {offset}: motion changed: {error}"));
        cleanup_probe_objects(backend, probe);
        return Ok(());
    }

    let expected_x = prediction.expected_x[offset];
    let delta = (state.x - expected_x).abs();
    prediction.actual_x.push(state.x);
    if delta > prediction.max_delta {
        prediction.max_delta = delta;
        prediction.max_delta_frame = offset;
    }
    if state.x.to_bits() != expected_x.to_bits() && prediction.first_bit_mismatch.is_none() {
        prediction.first_bit_mismatch = Some(BitMismatch::new(offset, expected_x, state.x));
    }

    if offset == prediction.repredict_frame {
        if let Err(error) = record_reprediction(&state, prediction) {
            prediction.failure = Some(error);
        }
    }
    if offset == prediction.horizon {
        cleanup_probe_objects(backend, probe);
        probe.completed = true;
    }
    Ok(())
}

fn build_prediction_trace(
    state: &ZombieMotionState, id: ZombieId, descriptor: ProbeDescriptor, start_frame: i32,
) -> Result<PredictionTrace, String> {
    let trace = predict_stable_zombie_x_trace(state, descriptor.horizon as u32).map_err(|error| {
        format!(
            "{}: {}-frame prediction failed: {error}; {}",
            descriptor.label,
            descriptor.horizon,
            describe_state(state)
        )
    })?;
    if trace.len() != descriptor.horizon + 1 {
        return Err(format!(
            "{}: prediction trace length mismatch: expected {}, got {}",
            descriptor.label,
            descriptor.horizon + 1,
            trace.len()
        ));
    }
    if trace[0].to_bits() != state.x.to_bits() {
        return Err(format!(
            "{}: prediction trace starts at {}, sampled x is {}; {}",
            descriptor.label,
            format_f32_bits(trace[0]),
            format_f32_bits(state.x),
            describe_state(state)
        ));
    }
    if let Some((frame, x)) = trace.iter().copied().enumerate().find(|(_, x)| !x.is_finite()) {
        return Err(format!(
            "{}: prediction produced non-finite x at frame {frame}: {x}",
            descriptor.label
        ));
    }

    Ok(PredictionTrace {
        id,
        label: descriptor.label,
        start_frame,
        horizon: descriptor.horizon,
        repredict_frame: repredict_frame_for(descriptor.horizon),
        expected_x: trace,
        actual_x: vec![state.x],
        max_delta_frame: 0,
        max_delta: 0.0,
        first_bit_mismatch: None,
        repredicted: None,
        repredict_max_delta: None,
        repredict_first_bit_mismatch: None,
        failure: None,
        coverage_key: coverage_key_for_state(descriptor, state),
    })
}

fn repredict_frame_for(horizon: usize) -> usize {
    if horizon >= DEFAULT_TRACE_HORIZON {
        DEFAULT_REPREDICT_FRAME
    } else {
        horizon / 2
    }
}

fn record_reprediction(state: &ZombieMotionState, prediction: &mut PredictionTrace) -> Result<(), String> {
    let remaining = prediction.horizon - prediction.repredict_frame;
    let fresh_trace = predict_stable_zombie_x_trace(state, remaining as u32).map_err(|error| {
        format!(
            "{} frame {} re-prediction failed: {error}; {}",
            prediction.label,
            prediction.repredict_frame,
            describe_state(state)
        )
    })?;
    let mut max_delta = 0.0;
    let mut max_delta_frame = prediction.repredict_frame;
    for (relative_frame, fresh_x) in fresh_trace.iter().copied().enumerate() {
        let original_frame = prediction.repredict_frame + relative_frame;
        let expected_x = prediction.expected_x[original_frame];
        let delta = (fresh_x - expected_x).abs();
        if delta > max_delta {
            max_delta = delta;
            max_delta_frame = original_frame;
        }
        if fresh_x.to_bits() != expected_x.to_bits() && prediction.repredict_first_bit_mismatch.is_none() {
            prediction.repredict_first_bit_mismatch = Some(BitMismatch::new(original_frame, expected_x, fresh_x));
        }
    }
    prediction.repredicted = Some(fresh_trace);
    prediction.repredict_max_delta = Some((max_delta_frame, max_delta));
    Ok(())
}

fn expect_motion(state: &ZombieMotionState, descriptor: ProbeDescriptor) -> Result<(), String> {
    if state.kind != descriptor.kind {
        return Err(format!(
            "expected {:?} kind; {}",
            descriptor.kind,
            describe_state(state)
        ));
    }
    if motion_direction(state.model) != Some(descriptor.expected_direction) {
        return Err(format!(
            "expected {:?} direction; {}",
            descriptor.expected_direction,
            describe_state(state)
        ));
    }
    if let Some(range) = descriptor.speed_range {
        if state.speed_x < range.min || state.speed_x > range.max {
            return Err(format!(
                "expected speed_x in [{:.3}, {:.3}], got {:.4}; {}",
                range.min,
                range.max,
                state.speed_x,
                describe_state(state)
            ));
        }
    }
    let matched = match (descriptor.expected, state.model) {
        (
            ExpectedMotion::NormalWalk,
            ZombieMovementModel::Track {
                profile: ZombieTrackProfile::NormalWalkA | ZombieTrackProfile::NormalWalkB,
                progress,
                ..
            },
        ) => progress.is_finite() && (0.0..1.0).contains(&progress),
        (ExpectedMotion::Track(expected_profile), ZombieMovementModel::Track { profile, progress, .. }) => {
            profile == expected_profile && progress.is_finite() && (0.0..1.0).contains(&progress)
        }
        (ExpectedMotion::Uniform(expected_uniform), ZombieMovementModel::Uniform { motion, .. }) => {
            motion == expected_uniform
        }
        _ => false,
    };
    if matched {
        Ok(())
    } else {
        Err(format!(
            "expected {}, got {:?}; {}",
            describe_expected_motion(descriptor.expected),
            state.model,
            describe_state(state)
        ))
    }
}

fn motion_direction(model: ZombieMovementModel) -> Option<ZombieMotionDirection> {
    match model {
        ZombieMovementModel::Track { direction, .. } | ZombieMovementModel::Uniform { direction, .. } => {
            Some(direction)
        }
        ZombieMovementModel::Unsupported(_) => None,
    }
}

fn cleanup_probe_objects<B>(backend: &B, probe: &mut ActiveProbe)
where
    B: PlantRemoveBackend + ZombieRemoveBackend,
{
    cleanup_probe_plants(backend, probe);
    if let Some(id) = probe.id.take() {
        if let Ok(Some(zombie)) = backend.zombie(id) {
            let _ = backend.remove_zombie(zombie);
        }
    }
}

fn cleanup_probe_plants<B>(backend: &B, probe: &mut ActiveProbe)
where
    B: PlantRemoveBackend,
{
    for plant_id in probe.plant_ids.drain(..) {
        if let Ok(Some(plant)) = backend.plant(plant_id) {
            let _ = backend.remove_plant(plant);
        }
    }
}

fn record_observation(probe: &mut ActiveProbe, frame: i32, state: &ZombieMotionState) {
    let observation = format!("frame={frame} {}", describe_state(state));
    if probe.last_observation.as_deref() == Some(observation.as_str()) {
        return;
    }
    probe.last_observation = Some(observation.clone());
    if probe.observations.len() < MAX_OBSERVATIONS {
        probe.observations.push(observation);
    }
}

fn coverage_key_for_state(descriptor: ProbeDescriptor, state: &ZombieMotionState) -> Option<&'static str> {
    if let Some(key) = descriptor.coverage_key {
        return Some(key);
    }
    match state.model {
        ZombieMovementModel::Track {
            profile: ZombieTrackProfile::NormalWalkA,
            ..
        } if is_normal_family(state.kind) => Some("normal_walk_a"),
        ZombieMovementModel::Track {
            profile: ZombieTrackProfile::NormalWalkB,
            ..
        } if is_normal_family(state.kind) => Some("normal_walk_b"),
        _ => None,
    }
}

fn is_normal_family(kind: ZombieKind) -> bool {
    matches!(
        kind,
        ZombieKind::Normal | ZombieKind::Flag | ZombieKind::Conehead | ZombieKind::Buckethead | ZombieKind::ScreenDoor
    )
}

fn validation_errors(probes: &[ActiveProbe], probe_errors: &[String]) -> Vec<String> {
    let mut errors = probe_errors.to_vec();
    if probes.is_empty() {
        errors.push("no probes were registered".to_owned());
    }
    for probe in probes {
        if let Some(failure) = probe_failure(probe) {
            errors.push(format!("{}: {failure}", probe.descriptor.label));
        }
    }
    let covered = coverage_set(probes);
    for required in REQUIRED_COVERAGE {
        if !covered.contains(required) {
            errors.push(format!("missing required live coverage: {required}"));
        }
    }
    errors
}

fn probe_failure(probe: &ActiveProbe) -> Option<String> {
    if let Some(failure) = probe.failure.as_ref() {
        return Some(failure.clone());
    }
    let Some(prediction) = probe.prediction.as_ref() else {
        return Some(format!(
            "prediction never started; observations=[{}]",
            probe.observations.join(" | ")
        ));
    };
    prediction_failure(prediction)
}

fn prediction_failure(prediction: &PredictionTrace) -> Option<String> {
    if let Some(failure) = prediction.failure.as_ref() {
        return Some(failure.clone());
    }
    if prediction.actual_x.len() != prediction.expected_x.len() {
        return Some(format!(
            "actual trace length mismatch: expected {}, got {}",
            prediction.expected_x.len(),
            prediction.actual_x.len()
        ));
    }
    if let Some(mismatch) = prediction.first_bit_mismatch {
        return Some(format!(
            "frame {} coordinate bits differ: {}",
            mismatch.frame,
            format_bit_mismatch(mismatch)
        ));
    }
    match (prediction.repredict_max_delta, prediction.repredict_first_bit_mismatch) {
        (Some(_), Some(mismatch)) => Some(format!(
            "reprediction coordinate bits differ at frame {}: {}",
            mismatch.frame,
            format_bit_mismatch(mismatch)
        )),
        (Some(_), None) => None,
        (None, _) => Some(format!(
            "missing frame-{} reprediction result",
            prediction.repredict_frame
        )),
    }
}

fn diagnostic_lines(probes: &[ActiveProbe], probe_errors: &[String]) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!(
        "zombie_motion_probe diagnostic default_horizon={DEFAULT_TRACE_HORIZON} pogo_bounce_horizon={POGO_BOUNCE_TRACE_HORIZON} exact_match=f32_to_bits report_frame={REPORT_FRAME}"
    ));
    for error in probe_errors {
        lines.push(format!("probe_error={error}"));
    }
    for (label, reason) in LIVE_UNAVAILABLE_PROBES {
        lines.push(format!("{label} status=live_unavailable reason={reason}"));
    }

    let covered = coverage_set(probes);
    for required in REQUIRED_COVERAGE {
        let status = if covered.contains(required) {
            "covered"
        } else {
            "missing"
        };
        lines.push(format!("coverage {required} status={status}"));
    }

    for probe in probes {
        let failure = probe_failure(probe);
        let status = if failure.is_some() { "fail" } else { "pass" };
        lines.push(format!(
            "{} status={} setup_frame={} search={}..={} horizon={} expected={} direction={:?} speed_range={} note={:?} failure={:?}",
            probe.descriptor.label,
            status,
            probe.descriptor.setup_frame,
            probe.descriptor.search_start_frame,
            probe.descriptor.search_end_frame,
            probe.descriptor.horizon,
            describe_expected_motion(probe.descriptor.expected),
            probe.descriptor.expected_direction,
            describe_speed_range(probe.descriptor.speed_range),
            probe.descriptor.note,
            failure,
        ));
        for observation in &probe.observations {
            lines.push(format!("{} observation {observation}", probe.descriptor.label));
        }
        if let Some(prediction) = probe.prediction.as_ref() {
            lines.extend(prediction_diagnostic_lines(prediction));
        }
    }
    lines
}

fn prediction_diagnostic_lines(prediction: &PredictionTrace) -> Vec<String> {
    let failure = prediction_failure(prediction);
    let status = if failure.is_some() { "fail" } else { "pass" };
    let mut lines = Vec::new();
    lines.push(format!(
        "{} trace_status={} id={:?} start_frame={} horizon={} expected_len={} actual_len={} max_delta_frame={} max_delta={:.9e} first_bit_mismatch={} repredict_frame={} repredict_max_delta={:?} repredict_first_bit_mismatch={} coverage_key={:?} failure={:?}",
        prediction.label,
        status,
        prediction.id,
        prediction.start_frame,
        prediction.horizon,
        prediction.expected_x.len(),
        prediction.actual_x.len(),
        prediction.max_delta_frame,
        prediction.max_delta,
        format_optional_bit_mismatch(prediction.first_bit_mismatch),
        prediction.repredict_frame,
        prediction.repredict_max_delta,
        format_optional_bit_mismatch(prediction.repredict_first_bit_mismatch),
        prediction.coverage_key,
        failure,
    ));
    lines.push(format!(
        "{} theoretical={}",
        prediction.label,
        format_array(&prediction.expected_x)
    ));
    lines.push(format!(
        "{} actual={}",
        prediction.label,
        format_array(&prediction.actual_x)
    ));
    if let Some(repredicted) = prediction.repredicted.as_deref() {
        lines.push(format!(
            "{} repredicted_from_{}={}",
            prediction.label,
            prediction.repredict_frame,
            format_array(repredicted)
        ));
    }
    lines
}

fn coverage_set(probes: &[ActiveProbe]) -> BTreeSet<&'static str> {
    probes
        .iter()
        .filter_map(|probe| probe.prediction.as_ref())
        .filter(|prediction| prediction_failure(prediction).is_none())
        .filter_map(|prediction| prediction.coverage_key)
        .collect()
}

fn describe_expected_motion(expected: ExpectedMotion) -> String {
    match expected {
        ExpectedMotion::NormalWalk => "NormalWalkA/B".to_owned(),
        ExpectedMotion::Track(profile) => format!("{profile:?}"),
        ExpectedMotion::Uniform(uniform) => format!("{uniform:?}"),
    }
}

fn describe_speed_range(range: Option<SpeedRange>) -> String {
    match range {
        Some(range) => format!("[{:.3}, {:.3}]", range.min, range.max),
        None => "any".to_owned(),
    }
}

fn format_array(values: &[f32]) -> String {
    let mut text = String::from("[");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            text.push_str(", ");
        }
        text.push_str(&format!("{value:.9e}"));
    }
    text.push(']');
    text
}

fn format_optional_bit_mismatch(mismatch: Option<BitMismatch>) -> String {
    mismatch.map_or_else(|| "None".to_owned(), format_bit_mismatch)
}

fn format_bit_mismatch(mismatch: BitMismatch) -> String {
    format!(
        "Some(frame={} expected={:.9e}/0x{:08X} actual={:.9e}/0x{:08X} delta={:.9e})",
        mismatch.frame,
        mismatch.expected,
        mismatch.expected_bits,
        mismatch.actual,
        mismatch.actual_bits,
        mismatch.delta
    )
}

fn format_f32_bits(value: f32) -> String {
    format!("{value:.9e}/0x{:08X}", value.to_bits())
}

fn describe_state(state: &ZombieMotionState) -> String {
    format!(
        "state id={:?} kind={:?} row={} x={:.4} speed_x={:.4} scale={:.4} direction={:?} counters(frozen={}, chilled={}, buttered={}) model={:?}",
        state.id,
        state.kind,
        state.row,
        state.x,
        state.speed_x,
        state.scale,
        motion_direction(state.model),
        state.counters.frozen,
        state.counters.chilled,
        state.counters.buttered,
        state.model
    )
}
