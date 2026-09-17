use std::io::{self, Write};

use rsvz::SessionArtifact;
use rsvz::core::backend::BackendIdentityBackend;
use rsvz::core::logic::{ContactGeometryBackend, circle_hits_rect};
use rsvz::core::model::{ContactCircle, ContactRect, Grid, PlantId, PlantKind, ZombieId, ZombieKind, ZombiePhase};
use rsvz::core::runtime::{RuntimeError, RuntimeResult};
use rsvz::prelude::{
    PlantCreateBackend, PlantEffectCountdownWriteBackend, PlantEffectRuleEditBackend, PlantHealthWriteBackend,
    PlantReadBackend, PlantRemoveBackend, PlantStateWriteBackend, WorldResetBackend, ZombieCreateBackend,
    ZombiePhaseCountdownWriteBackend, ZombiePositionWriteBackend, ZombieRawFactsBackend, ZombieRemoveBackend,
    ZombieRuleEditBackend, ZombieXWriteBackend,
};
use rsvz::tick::{TickControl, TickLifetime, TickOptions};
use rsvz::{SessionJobKey, SessionShard, WorldResetConfig};

const FORMAT_VERSION: u32 = 1;
const RSVZ_SOURCE_COMMIT: &str = match option_env!("RSVZ_SOURCE_COMMIT") {
    Some(value) => value,
    None => "unrecorded",
};
const PE_SOURCE_COMMIT: &str = match option_env!("PVZ_EMULATOR_SOURCE_COMMIT") {
    Some(value) => value,
    None => "unrecorded",
};
const FORMAL_TRIALS_PER_CASE: u64 = 50_000;
const VALIDATION_SMOKE: bool = option_env!("SMART_FODDER_VALIDATION_SMOKE").is_some();
const VALIDATE_ONLY: bool = option_env!("SMART_FODDER_VALIDATE_ONLY").is_some();
const NORMAL_WAVE_VALIDATION: bool = option_env!("SMART_FODDER_VALIDATION_NORMAL_WAVE").is_some();
const FULL_WINDOW_VALIDATION: bool = option_env!("SMART_FODDER_VALIDATION_FULL_WINDOW").is_some();
const VALIDATION_CANDIDATE_COUNT: usize = if FULL_WINDOW_VALIDATION { 1 } else { 2 };
const PAIRED_VALIDATION_TRIALS: u64 = if VALIDATION_SMOKE { 8 } else { 50_000 };
const H_MAX: usize = 1_142;
const H_COUNT: usize = H_MAX + 1;
const X_MIN: f32 = 620.0;
const X_STEP: f32 = 3.0;
const X_COUNT: usize = 35;
const RELEASE_KIND_COUNT: usize = 3;
const JACK_GEOMETRY_COUNT: usize = 5;
const CASE_COUNT: usize = RELEASE_KIND_COUNT * X_COUNT + 1;
const POLE_CASE: usize = CASE_COUNT - 1;
const MEASURE_ROW: i32 = 0;
const CANNON_GRID: Grid = Grid {
    row: MEASURE_ROW,
    col: 6,
};
const FODDER_GRID: Grid = Grid {
    row: MEASURE_ROW,
    col: 8,
};
const ICE_GRID: Grid = Grid {
    row: MEASURE_ROW,
    col: 0,
};
const SAFETY_GRID: Grid = Grid {
    row: MEASURE_ROW,
    col: 1,
};
const SETUP_X: f32 = 720.0;
const JACK_EXPLOSION_RADIUS: i32 = 90;
const JACK_CENTER_OFFSET: i32 = 60;
const VALIDATION_OBSERVED_AT: usize = 650;
const VALIDATION_PLANT_AT: [i32; 2] = if FULL_WINDOW_VALIDATION {
    [700, 1_100]
} else if NORMAL_WAVE_VALIDATION {
    if option_env!("SMART_FODDER_VALIDATION_HIGH_F").is_some() {
        [850, 900]
    } else {
        [750, 800]
    }
} else {
    [1_150, 1_200]
};
const VALIDATION_ACTIVATION_AT: usize =
    if NORMAL_WAVE_VALIDATION && option_env!("SMART_FODDER_VALIDATION_A1500").is_some() {
        1_500
    } else if NORMAL_WAVE_VALIDATION {
        1_400
    } else {
        1_800
    };
const VALIDATION_START_X: f32 = 840.0;
const VALIDATION_SEED_OFFSET: u32 = 100_000_000;
const VALIDATION_JACK_COUNT: u8 = if NORMAL_WAVE_VALIDATION {
    1
} else if option_env!("SMART_FODDER_VALIDATION_ZERO_JACKS").is_some() {
    0
} else if option_env!("SMART_FODDER_VALIDATION_ONE_JACK").is_some() {
    1
} else {
    5
};
const VALIDATION_LADDER_COUNT: usize = if NORMAL_WAVE_VALIDATION { 2 } else { 1 };
const VALIDATION_FOOTBALL_COUNT: usize = if NORMAL_WAVE_VALIDATION { 2 } else { 0 };
const VALIDATION_ZOMBIE_COUNT: usize =
    VALIDATION_LADDER_COUNT + VALIDATION_JACK_COUNT as usize + VALIDATION_FOOTBALL_COUNT;
const VALIDATION_EVENT_STRATA: [&str; 4] = [
    "no_post_f_explosion",
    "fodder_died_with_post_f_explosion",
    "fodder_died_before_first_post_f_explosion",
    "other_post_f_explosion",
];
const VALIDATION_PAIR_STRATA: [&str; 2] = ["no_explosion_between_plant_times", "explosion_between_plant_times"];

static SMART_FODDER_MEASURE_JOB: u8 = 0;

#[derive(Clone, Copy, Debug, Default)]
struct Stats {
    n: u64,
    sum: f64,
    sumsq: f64,
}

impl Stats {
    fn add(&mut self, value: f64) {
        self.n = self.n.saturating_add(1);
        self.sum += value;
        self.sumsq += value * value;
    }

    fn merge(&mut self, other: Self) {
        self.n = self.n.saturating_add(other.n);
        self.sum += other.sum;
        self.sumsq += other.sumsq;
    }

    fn mean(self) -> f64 {
        if self.n == 0 { 0.0 } else { self.sum / self.n as f64 }
    }

    fn ci95_half_width(self) -> f64 {
        if self.n < 2 {
            return 0.0;
        }
        let n = self.n as f64;
        let variance = ((self.sumsq - self.sum * self.sum / n) / (n - 1.0)).max(0.0);
        1.96 * (variance / n).sqrt()
    }
}

struct SmartFodderArtifact {
    backend_name: String,
    backend_version: String,
    trials_per_case: u64,
    release: Vec<Stats>,
    jack: Vec<Stats>,
    pole: Vec<Stats>,
    invalid_attempts: u64,
    paired: [PairedStats; 2],
    paired_between_plant_times: [ComparisonStats; VALIDATION_PAIR_STRATA.len()],
    paired_ranking_disagreements: u64,
    paired_invalid_attempts: u64,
    seed_shards: Vec<SeedShard>,
}

#[derive(Clone, Copy, Debug, Default)]
struct ComparisonStats {
    predicted: Stats,
    actual: Stats,
    difference: Stats,
}

impl ComparisonStats {
    fn add(&mut self, predicted: f64, actual: f64) {
        self.predicted.add(predicted);
        self.actual.add(actual);
        self.difference.add(actual - predicted);
    }

    fn merge(&mut self, other: Self) {
        self.predicted.merge(other.predicted);
        self.actual.merge(other.actual);
        self.difference.merge(other.difference);
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct PairedStats {
    selected_plant_at: Stats,
    predicted: Stats,
    actual: Stats,
    difference: Stats,
    solver_elapsed_ms: Stats,
    cap_one_omitted: Stats,
    cap_one_excess: Stats,
    event_strata: [ComparisonStats; VALIDATION_EVENT_STRATA.len()],
}

#[derive(Clone, Copy, Debug)]
struct ValidationOutcome {
    predicted: f64,
    actual: f64,
    exploded_before_fodder: u8,
    exploded_at_end: u8,
    first_post_f_explosion_at: Option<i32>,
    fodder_death_at: Option<i32>,
    fodder_died_with_explosion: bool,
}

#[derive(Clone, Copy)]
struct SeedShard {
    index: u32,
    count: u32,
    seed_base: u32,
    attempts: u64,
}

fn merge_artifact(target: &mut SmartFodderArtifact, source: SmartFodderArtifact) -> RuntimeResult<()> {
    if target.backend_name != source.backend_name
        || target.backend_version != source.backend_version
        || target.trials_per_case != source.trials_per_case
        || target.release.len() != source.release.len()
        || target.jack.len() != source.jack.len()
        || target.pole.len() != source.pole.len()
    {
        return Err(RuntimeError::new("incompatible smart-fodder measurement artifacts"));
    }
    for (target, source) in target.release.iter_mut().zip(source.release) {
        target.merge(source);
    }
    for (target, source) in target.jack.iter_mut().zip(source.jack) {
        target.merge(source);
    }
    for (target, source) in target.pole.iter_mut().zip(source.pole) {
        target.merge(source);
    }
    target.invalid_attempts = target.invalid_attempts.saturating_add(source.invalid_attempts);
    for (target, source) in target.paired.iter_mut().zip(source.paired) {
        target.selected_plant_at.merge(source.selected_plant_at);
        target.predicted.merge(source.predicted);
        target.actual.merge(source.actual);
        target.difference.merge(source.difference);
        target.solver_elapsed_ms.merge(source.solver_elapsed_ms);
        target.cap_one_omitted.merge(source.cap_one_omitted);
        target.cap_one_excess.merge(source.cap_one_excess);
        for (target, source) in target.event_strata.iter_mut().zip(source.event_strata) {
            target.merge(source);
        }
    }
    for (target, source) in target
        .paired_between_plant_times
        .iter_mut()
        .zip(source.paired_between_plant_times)
    {
        target.merge(source);
    }
    target.paired_ranking_disagreements = target
        .paired_ranking_disagreements
        .saturating_add(source.paired_ranking_disagreements);
    target.paired_invalid_attempts = target
        .paired_invalid_attempts
        .saturating_add(source.paired_invalid_attempts);
    target.seed_shards.extend(source.seed_shards);
    Ok(())
}

fn write_artifact(value: &SmartFodderArtifact, writer: &mut dyn Write) -> io::Result<()> {
    write!(
        writer,
        "{{\"kind\":\"smart_fodder_tables\",\"format_version\":{FORMAT_VERSION},\"backend_name\":{:?},\"backend_version\":{:?},\"rsvz_source_commit\":{RSVZ_SOURCE_COMMIT:?},\"pvz_emulator_source_commit\":{PE_SOURCE_COMMIT:?},\"trials_per_case\":{},\"h_max\":{H_MAX},\"x_min\":{X_MIN},\"x_step\":{X_STEP},\"x_count\":{X_COUNT},\"invalid_attempts\":{},\"seed_formula\":\"seed_base + attempt_index * shard_count + shard_index (wrapping u32)\",\"control_timeline\":\"land ice; hold jack phase countdown through measurement; wait for thaw; real eat-stop transition or real C9 pole vault; sample h=0..1142 while slowed\",\"seed_shards\":[",
        value.backend_name, value.backend_version, value.trials_per_case, value.invalid_attempts
    )?;
    let mut first = true;
    for shard in &value.seed_shards {
        write_separator(writer, &mut first)?;
        write!(
            writer,
            "{{\"index\":{},\"count\":{},\"seed_base\":{},\"attempts\":{}}}",
            shard.index, shard.count, shard.seed_base, shard.attempts
        )?;
    }
    writer.write_all(b"],\"release_tail\":[")?;
    first = true;
    for kind in 0..RELEASE_KIND_COUNT {
        for x_index in 0..X_COUNT {
            for h in 0..H_COUNT {
                let stats = value.release[release_index(kind, x_index, h)];
                write_separator(writer, &mut first)?;
                write!(
                    writer,
                    "{{\"table_kind\":{kind},\"x_index\":{x_index},\"x_px\":{},\"h\":{h},\"n\":{},\"sum_damage\":{},\"sum_damage_sq\":{},\"mean\":{},\"ci95_half_width\":{}}}",
                    x_for_index(x_index),
                    stats.n,
                    stats.sum,
                    stats.sumsq,
                    stats.mean(),
                    stats.ci95_half_width()
                )?;
            }
        }
    }
    writer.write_all(b"],\"jack_position_tail\":[")?;
    first = true;
    for geometry in 0..JACK_GEOMETRY_COUNT {
        for x_index in 0..X_COUNT {
            for s in 0..H_COUNT {
                let stats = value.jack[jack_index(geometry, x_index, s)];
                write_separator(writer, &mut first)?;
                write!(
                    writer,
                    "{{\"geometry\":{geometry},\"x_index\":{x_index},\"x_px\":{},\"s\":{s},\"n\":{},\"danger_count\":{},\"mean\":{},\"ci95_half_width\":{}}}",
                    x_for_index(x_index),
                    stats.n,
                    stats.sum,
                    stats.mean(),
                    stats.ci95_half_width()
                )?;
            }
        }
    }
    writer.write_all(b"],\"pole_tail\":[")?;
    first = true;
    for h in 0..H_COUNT {
        let stats = value.pole[h];
        write_separator(writer, &mut first)?;
        write!(
            writer,
            "{{\"h\":{h},\"n\":{},\"sum_damage\":{},\"sum_damage_sq\":{},\"mean\":{},\"ci95_half_width\":{}}}",
            stats.n,
            stats.sum,
            stats.sumsq,
            stats.mean(),
            stats.ci95_half_width()
        )?;
    }
    writer.write_all(b"],\"paired_validation\":{")?;
    write!(
        writer,
        "\"trials\":{PAIRED_VALIDATION_TRIALS},\"normal_wave_spawn\":{NORMAL_WAVE_VALIDATION},\"full_window\":{FULL_WINDOW_VALIDATION},\"ladder_count\":{VALIDATION_LADDER_COUNT},\"jack_count\":{VALIDATION_JACK_COUNT},\"football_count\":{VALIDATION_FOOTBALL_COUNT},\"observed_at\":{VALIDATION_OBSERVED_AT},\"plant_at\":[{},{}],\"activation_at\":{VALIDATION_ACTIVATION_AT},\"ranking_disagreements\":{},\"invalid_attempts\":{},\"candidates\":[",
        VALIDATION_PLANT_AT[0],
        VALIDATION_PLANT_AT[1],
        value.paired_ranking_disagreements,
        value.paired_invalid_attempts,
    )?;
    for (index, stats) in value.paired.iter().take(VALIDATION_CANDIDATE_COUNT).enumerate() {
        if index != 0 {
            writer.write_all(b",")?;
        }
        write!(
            writer,
            "{{\"plant_at\":{},\"selected_plant_at_mean\":{},\"selected_plant_at_sum_sq\":{},\"n\":{},\"predicted_sum\":{},\"predicted_sum_sq\":{},\"predicted_mean\":{},\"actual_sum\":{},\"actual_sum_sq\":{},\"actual_mean\":{},\"difference_sum\":{},\"difference_sum_sq\":{},\"difference_mean\":{},\"difference_ci95_half_width\":{},\"solver_elapsed_ms_sum\":{},\"solver_elapsed_ms_mean\":{},\"solver_elapsed_ms_ci95_half_width\":{},\"cap_one_omitted_count\":{},\"cap_one_omitted_frequency\":{},\"cap_one_omitted_ci95_half_width\":{},\"cap_one_excess_sum\":{},\"cap_one_excess_mean\":{},\"cap_one_excess_ci95_half_width\":{},\"event_strata\":[",
            VALIDATION_PLANT_AT[index],
            stats.selected_plant_at.mean(),
            stats.selected_plant_at.sumsq,
            stats.difference.n,
            stats.predicted.sum,
            stats.predicted.sumsq,
            stats.predicted.mean(),
            stats.actual.sum,
            stats.actual.sumsq,
            stats.actual.mean(),
            stats.difference.sum,
            stats.difference.sumsq,
            stats.difference.mean(),
            stats.difference.ci95_half_width(),
            stats.solver_elapsed_ms.sum,
            stats.solver_elapsed_ms.mean(),
            stats.solver_elapsed_ms.ci95_half_width(),
            stats.cap_one_omitted.sum,
            stats.cap_one_omitted.mean(),
            stats.cap_one_omitted.ci95_half_width(),
            stats.cap_one_excess.sum,
            stats.cap_one_excess.mean(),
            stats.cap_one_excess.ci95_half_width(),
        )?;
        for (stratum_index, (name, stratum)) in VALIDATION_EVENT_STRATA.iter().zip(stats.event_strata).enumerate() {
            if stratum_index != 0 {
                writer.write_all(b",")?;
            }
            write!(
                writer,
                "{{\"name\":{name:?},\"n\":{},\"predicted_mean\":{},\"actual_mean\":{},\"difference_mean\":{},\"difference_ci95_half_width\":{},\"difference_contribution\":{}}}",
                stratum.difference.n,
                stratum.predicted.mean(),
                stratum.actual.mean(),
                stratum.difference.mean(),
                stratum.difference.ci95_half_width(),
                stratum.difference.sum / PAIRED_VALIDATION_TRIALS as f64,
            )?;
        }
        writer.write_all(b"]}")?;
    }
    writer.write_all(b"],\"paired_between_plant_times\":[")?;
    for (index, (name, stats)) in VALIDATION_PAIR_STRATA
        .iter()
        .zip(value.paired_between_plant_times)
        .enumerate()
    {
        if index != 0 {
            writer.write_all(b",")?;
        }
        write!(
            writer,
            "{{\"name\":{name:?},\"n\":{},\"predicted_difference_mean\":{},\"actual_difference_mean\":{},\"residual_mean\":{},\"residual_ci95_half_width\":{},\"residual_contribution\":{}}}",
            stats.difference.n,
            stats.predicted.mean(),
            stats.actual.mean(),
            stats.difference.mean(),
            stats.difference.ci95_half_width(),
            stats.difference.sum / PAIRED_VALIDATION_TRIALS as f64,
        )?;
    }
    writer.write_all(b"]}}")
}

fn write_separator(writer: &mut dyn Write, first: &mut bool) -> io::Result<()> {
    if *first {
        *first = false;
        Ok(())
    } else {
        writer.write_all(b",")
    }
}

fn artifact(value: SmartFodderArtifact) -> SessionArtifact {
    SessionArtifact::new(value, merge_artifact, write_artifact)
}

#[derive(Clone, Copy, Debug)]
enum TrialPhase {
    AwaitingReset,
    AwaitingValidationSpawn,
    Warmup {
        zombie: ZombieId,
        cannon: PlantId,
        lure: PlantId,
        saw_freeze: bool,
        lure_removed: bool,
    },
    Active {
        zombie: ZombieId,
        cannon: PlantId,
        cannon_rect: ContactRect,
        cannon_initial_hp: i32,
        h: usize,
    },
    PairedValidation {
        candidate: usize,
        cannon: PlantId,
        cannon_initial_hp: i32,
        cannon_last_hp: i32,
        actual_damage: f64,
        fodder: Option<PlantId>,
        selected_plant_at: Option<i32>,
        predicted: Option<f64>,
        elapsed: usize,
    },
}

struct MeasureRunner {
    shard: SessionShard,
    quota_per_case: u64,
    case_index: usize,
    valid_in_case: u64,
    attempt_index: u64,
    observed_epoch: u64,
    phase: TrialPhase,
    backend_name: String,
    backend_version: String,
    release: Vec<Stats>,
    jack: Vec<Stats>,
    pole: Vec<Stats>,
    damage_trace: Vec<f64>,
    danger_trace: Vec<[bool; JACK_GEOMETRY_COUNT]>,
    invalid_attempts: u64,
    validation_candidate: usize,
    validation_outcomes: [Option<ValidationOutcome>; 2],
    validation_exploded_before_fodder: Option<u8>,
    validation_last_exploded: Option<u8>,
    validation_first_post_f_explosion_at: Option<i32>,
    validation_fodder_death_at: Option<i32>,
    validation_fodder_died_with_explosion: bool,
    paired: [PairedStats; 2],
    paired_between_plant_times: [ComparisonStats; VALIDATION_PAIR_STRATA.len()],
    paired_ranking_disagreements: u64,
    paired_invalid_attempts: u64,
}

impl MeasureRunner {
    fn new(shard: SessionShard) -> Self {
        let count = u64::from(shard.count.max(1));
        let index = u64::from(shard.index);
        let trial_target = if VALIDATE_ONLY {
            PAIRED_VALIDATION_TRIALS
        } else {
            FORMAL_TRIALS_PER_CASE
        };
        let quotient = trial_target / count;
        let remainder = trial_target % count;
        Self {
            shard,
            quota_per_case: quotient + u64::from(index < remainder),
            case_index: if VALIDATE_ONLY { CASE_COUNT } else { 0 },
            valid_in_case: 0,
            attempt_index: 0,
            observed_epoch: u64::MAX,
            phase: TrialPhase::AwaitingReset,
            backend_name: String::new(),
            backend_version: String::new(),
            release: vec![Stats::default(); RELEASE_KIND_COUNT * X_COUNT * H_COUNT],
            jack: vec![Stats::default(); JACK_GEOMETRY_COUNT * X_COUNT * H_COUNT],
            pole: vec![Stats::default(); H_COUNT],
            damage_trace: Vec::with_capacity(H_COUNT),
            danger_trace: Vec::with_capacity(H_COUNT),
            invalid_attempts: 0,
            validation_candidate: 0,
            validation_outcomes: [None; 2],
            validation_exploded_before_fodder: None,
            validation_last_exploded: None,
            validation_first_post_f_explosion_at: None,
            validation_fodder_death_at: None,
            validation_fodder_died_with_explosion: false,
            paired: [PairedStats::default(); 2],
            paired_between_plant_times: [ComparisonStats::default(); VALIDATION_PAIR_STRATA.len()],
            paired_ranking_disagreements: 0,
            paired_invalid_attempts: 0,
        }
    }

    fn reset_config(&self) -> WorldResetConfig {
        let sequence = self
            .attempt_index
            .saturating_mul(u64::from(self.shard.count.max(1)))
            .saturating_add(u64::from(self.shard.index));
        let seed = if self.case_index < CASE_COUNT {
            self.shard.seed_base.wrapping_add(sequence as u32)
        } else {
            self.shard
                .seed_base
                .wrapping_add(VALIDATION_SEED_OFFSET)
                .wrapping_add(sequence as u32)
        };
        WorldResetConfig {
            completed_rounds: 500,
            seed,
            ..WorldResetConfig::default()
        }
    }

    fn tick<B>(&mut self, backend: &B) -> RuntimeResult<DriveAction>
    where
        B: BackendIdentityBackend
            + ContactGeometryBackend
            + PlantCreateBackend
            + PlantEffectCountdownWriteBackend
            + PlantEffectRuleEditBackend
            + PlantHealthWriteBackend
            + PlantRemoveBackend
            + PlantStateWriteBackend
            + WorldResetBackend
            + ZombieCreateBackend
            + ZombiePhaseCountdownWriteBackend
            + ZombiePositionWriteBackend
            + ZombieRawFactsBackend
            + ZombieRemoveBackend
            + ZombieRuleEditBackend
            + ZombieXWriteBackend,
    {
        let epoch = rsvz::session::world_epoch();
        if epoch != self.observed_epoch {
            self.observed_epoch = epoch;
            self.setup_trial(backend)?;
        }
        match self.phase {
            TrialPhase::AwaitingReset => Ok(DriveAction::Continue),
            TrialPhase::AwaitingValidationSpawn => self.await_validation_spawn(backend),
            TrialPhase::Warmup { .. } => self.drive_warmup(backend),
            TrialPhase::Active { .. } => self.drive_active(backend),
            TrialPhase::PairedValidation { .. } => self.drive_paired_validation(backend),
        }
    }

    fn setup_trial<B>(&mut self, backend: &B) -> RuntimeResult<()>
    where
        B: BackendIdentityBackend
            + ContactGeometryBackend
            + PlantCreateBackend
            + PlantEffectCountdownWriteBackend
            + PlantEffectRuleEditBackend
            + PlantHealthWriteBackend
            + PlantRemoveBackend
            + PlantStateWriteBackend
            + ZombieCreateBackend
            + ZombiePhaseCountdownWriteBackend
            + ZombiePositionWriteBackend
            + ZombieRawFactsBackend
            + ZombieRemoveBackend
            + ZombieRuleEditBackend
            + ZombieXWriteBackend,
    {
        if self.backend_name.is_empty() {
            self.backend_name = backend.backend_name().to_owned();
            self.backend_version = backend.backend_version().to_owned();
        }
        if self.case_index >= CASE_COUNT {
            return self.setup_paired_validation(backend);
        }
        rsvz::core::logic::cleanup::clear_plants().map_err(runtime_error)?;
        rsvz::core::logic::cleanup::clear_zombies().map_err(runtime_error)?;
        backend.set_zombie_spawn_stopped(true).map_err(runtime_error)?;
        backend.set_mushrooms_awake(true).map_err(runtime_error)?;

        let cannon = rsvz::core::logic::cards::new_plant(PlantKind::CobCannon, CANNON_GRID).map_err(runtime_error)?;
        let _safety = rsvz::core::logic::cards::new_plant(PlantKind::TallNut, SAFETY_GRID).map_err(runtime_error)?;
        let lure = rsvz::core::logic::cards::new_plant(PlantKind::Sunflower, FODDER_GRID).map_err(runtime_error)?;
        let ice = rsvz::core::logic::cards::new_plant(PlantKind::IceShroom, ICE_GRID).map_err(runtime_error)?;
        let handle = backend
            .plant(ice)
            .ok_or_else(|| RuntimeError::new("measurement ice vanished during setup"))?;
        backend.set_plant_state(handle, 2).map_err(runtime_error)?;
        rsvz::core::modifier::normalize_effect_countdown_by_id(ice, 1).map_err(runtime_error)?;

        let kind = case_zombie_kind(self.case_index);
        let zombie = rsvz::core::modifier::spawn_zombie(
            kind,
            Grid {
                row: MEASURE_ROW,
                col: 8,
            },
        )
        .map_err(runtime_error)?;
        rsvz::core::modifier::set_zombie_x(zombie, SETUP_X).map_err(runtime_error)?;
        if kind == ZombieKind::JackInTheBox {
            let handle = backend
                .zombie(zombie)
                .ok_or_else(|| RuntimeError::new("measurement jack vanished during setup"))?;
            backend
                .set_zombie_phase_countdown(
                    handle,
                    rsvz::core::model::NonNegativeI32::new(i32::MAX).expect("i32::MAX is non-negative"),
                )
                .map_err(runtime_error)?;
        }
        self.damage_trace.clear();
        self.danger_trace.clear();
        self.phase = TrialPhase::Warmup {
            zombie,
            cannon,
            lure,
            saw_freeze: false,
            lure_removed: false,
        };
        Ok(())
    }

    fn setup_paired_validation<B>(&mut self, backend: &B) -> RuntimeResult<()>
    where
        B: ContactGeometryBackend
            + PlantCreateBackend
            + PlantEffectCountdownWriteBackend
            + PlantEffectRuleEditBackend
            + PlantHealthWriteBackend
            + PlantRemoveBackend
            + PlantStateWriteBackend
            + ZombieCreateBackend
            + ZombiePhaseCountdownWriteBackend
            + ZombieRawFactsBackend
            + ZombieRemoveBackend
            + ZombieRuleEditBackend
            + ZombieXWriteBackend,
    {
        rsvz::core::logic::cleanup::clear_plants().map_err(runtime_error)?;
        rsvz::core::logic::cleanup::clear_zombies().map_err(runtime_error)?;
        backend
            .set_zombie_spawn_stopped(!NORMAL_WAVE_VALIDATION)
            .map_err(runtime_error)?;
        backend.set_mushrooms_awake(true).map_err(runtime_error)?;

        if NORMAL_WAVE_VALIDATION {
            self.phase = TrialPhase::AwaitingValidationSpawn;
            self.reset_validation_observations();
            return Ok(());
        }

        let (cannon, initial_hp) = create_validation_board(backend)?;

        for kind in std::iter::once(ZombieKind::Ladder).chain(std::iter::repeat_n(
            ZombieKind::JackInTheBox,
            usize::from(VALIDATION_JACK_COUNT),
        )) {
            let zombie = rsvz::core::modifier::spawn_zombie(
                kind,
                Grid {
                    row: MEASURE_ROW,
                    col: 8,
                },
            )
            .map_err(runtime_error)?;
            rsvz::core::modifier::set_zombie_x(zombie, VALIDATION_START_X).map_err(runtime_error)?;
        }
        self.phase = TrialPhase::PairedValidation {
            candidate: self.validation_candidate,
            cannon,
            cannon_initial_hp: initial_hp,
            cannon_last_hp: initial_hp,
            actual_damage: 0.0,
            fodder: None,
            selected_plant_at: None,
            predicted: None,
            elapsed: 0,
        };
        self.reset_validation_observations();
        Ok(())
    }

    fn await_validation_spawn<B>(&mut self, backend: &B) -> RuntimeResult<DriveAction>
    where
        B: PlantCreateBackend
            + PlantEffectCountdownWriteBackend
            + PlantHealthWriteBackend
            + PlantReadBackend
            + PlantStateWriteBackend
            + ZombiePositionWriteBackend
            + ZombieRawFactsBackend,
    {
        let mut ids = [None; VALIDATION_ZOMBIE_COUNT];
        let mut count = 0;
        let mut ladders = 0;
        let mut jacks = 0;
        let mut footballs = 0;
        for zombie in backend.zombies() {
            if !backend.zombie_is_alive(zombie)
                || backend.zombie_is_disappeared(zombie)
                || backend.zombie_from_wave(zombie) != 0
            {
                continue;
            }
            if count == ids.len() {
                return Err(RuntimeError::new(
                    "normal-wave validation spawned more than five W1 zombies",
                ));
            }
            match backend.zombie_kind(zombie).map_err(runtime_error)? {
                ZombieKind::Ladder => ladders += 1,
                ZombieKind::JackInTheBox => jacks += 1,
                ZombieKind::Football => footballs += 1,
                kind => {
                    return Err(RuntimeError::new(format!(
                        "normal-wave validation spawned unexpected zombie {kind:?}"
                    )));
                }
            }
            ids[count] = Some(backend.zombie_id(zombie));
            count += 1;
        }
        if count < ids.len() {
            return Ok(DriveAction::Continue);
        }
        if [ladders, jacks, footballs]
            != [
                VALIDATION_LADDER_COUNT,
                usize::from(VALIDATION_JACK_COUNT),
                VALIDATION_FOOTBALL_COUNT,
            ]
        {
            return Err(RuntimeError::new(format!(
                "normal-wave validation expected 2 ladders, 1 Jack and 2 footballs, got {ladders}/{jacks}/{footballs}"
            )));
        }
        for id in ids.into_iter().flatten() {
            if !rsvz::core::logic::zombies::move_zombie_to_row_by_id(id, MEASURE_ROW).map_err(runtime_error)? {
                return Err(RuntimeError::new(
                    "normal-wave validation zombie vanished while moving rows",
                ));
            }
            let zombie = backend
                .zombie(id)
                .ok_or_else(|| RuntimeError::new("normal-wave validation zombie vanished after moving rows"))?;
            if backend.zombie_row(zombie) != MEASURE_ROW {
                return Err(RuntimeError::new(
                    "normal-wave validation failed to move a zombie to the cannon row",
                ));
            }
        }

        let (cannon, initial_hp) = create_validation_board(backend)?;
        self.phase = TrialPhase::PairedValidation {
            candidate: self.validation_candidate,
            cannon,
            cannon_initial_hp: initial_hp,
            cannon_last_hp: initial_hp,
            actual_damage: 0.0,
            fodder: None,
            selected_plant_at: None,
            predicted: None,
            elapsed: 0,
        };
        Ok(DriveAction::Continue)
    }

    fn reset_validation_observations(&mut self) {
        self.validation_exploded_before_fodder = None;
        self.validation_last_exploded = None;
        self.validation_first_post_f_explosion_at = None;
        self.validation_fodder_death_at = None;
        self.validation_fodder_died_with_explosion = false;
    }

    fn drive_warmup<B>(&mut self, backend: &B) -> RuntimeResult<DriveAction>
    where
        B: ContactGeometryBackend + PlantRemoveBackend + ZombieRawFactsBackend + ZombieXWriteBackend,
    {
        let TrialPhase::Warmup {
            zombie,
            cannon,
            lure,
            mut saw_freeze,
            mut lure_removed,
        } = self.phase
        else {
            return Ok(DriveAction::Continue);
        };
        let Some(handle) = backend.zombie(zombie) else {
            return Ok(self.invalidate());
        };
        let phase = backend.zombie_phase(handle).map_err(runtime_error)?;
        if self.case_index / X_COUNT == 2 && phase != ZombiePhase::JackInTheBoxRunning {
            return Ok(self.invalidate());
        }
        if backend.zombie_frozen_countdown(handle) != 0 {
            saw_freeze = true;
            self.phase = TrialPhase::Warmup {
                zombie,
                cannon,
                lure,
                saw_freeze,
                lure_removed,
            };
            return Ok(DriveAction::Continue);
        }
        if !saw_freeze {
            return Ok(DriveAction::Continue);
        }
        let required_chill =
            i32::try_from(H_MAX).map_err(|_error| RuntimeError::new("measurement horizon does not fit i32"))?;
        if backend.zombie_chilled_countdown(handle) <= required_chill {
            return Err(RuntimeError::new(
                "ice warmup left too little chill for the full trajectory",
            ));
        }

        if self.case_index == POLE_CASE {
            if phase != ZombiePhase::PolevaulterPostVault {
                return Ok(DriveAction::Continue);
            }
            rsvz::core::modifier::remove_plant_by_id(lure).map_err(runtime_error)?;
            return self.start_active(backend, zombie, cannon);
        }

        if !lure_removed && backend.zombie_is_eating(handle) {
            rsvz::core::modifier::remove_plant_by_id(lure).map_err(runtime_error)?;
            lure_removed = true;
        } else if lure_removed && !backend.zombie_is_eating(handle) {
            let x_index = self.case_index % X_COUNT;
            rsvz::core::modifier::set_zombie_x(zombie, x_for_index(x_index)).map_err(runtime_error)?;
            return self.start_active(backend, zombie, cannon);
        }
        self.phase = TrialPhase::Warmup {
            zombie,
            cannon,
            lure,
            saw_freeze,
            lure_removed,
        };
        Ok(DriveAction::Continue)
    }

    fn start_active<B>(&mut self, backend: &B, zombie: ZombieId, cannon: PlantId) -> RuntimeResult<DriveAction>
    where
        B: ContactGeometryBackend,
    {
        let initial_hp = cannon_hp(backend, cannon).ok_or_else(|| RuntimeError::new("measurement cannon vanished"))?;
        let cannon_rect = rsvz::core::logic::contact::plant_contact_rect(cannon)
            .map_err(runtime_error)?
            .ok_or_else(|| RuntimeError::new("measurement cannon geometry vanished"))?
            .rect;
        self.phase = TrialPhase::Active {
            zombie,
            cannon,
            cannon_rect,
            cannon_initial_hp: initial_hp,
            h: 0,
        };
        self.record_active_frame(backend, zombie, cannon, cannon_rect, initial_hp)?;
        Ok(DriveAction::Continue)
    }

    fn drive_active<B>(&mut self, backend: &B) -> RuntimeResult<DriveAction>
    where
        B: ContactGeometryBackend + ZombieRawFactsBackend,
    {
        let TrialPhase::Active {
            zombie,
            cannon,
            cannon_rect,
            cannon_initial_hp,
            h,
        } = self.phase
        else {
            return Ok(DriveAction::Continue);
        };
        if self.case_index / X_COUNT == 2 {
            let Some(handle) = backend.zombie(zombie) else {
                return Ok(self.invalidate());
            };
            if backend.zombie_phase(handle).map_err(runtime_error)? != ZombiePhase::JackInTheBoxRunning {
                return Ok(self.invalidate());
            }
        }
        let next_h = h + 1;
        self.record_active_frame(backend, zombie, cannon, cannon_rect, cannon_initial_hp)?;
        if next_h < H_MAX {
            self.phase = TrialPhase::Active {
                zombie,
                cannon,
                cannon_rect,
                cannon_initial_hp,
                h: next_h,
            };
            return Ok(DriveAction::Continue);
        }
        self.commit_trace();
        self.valid_in_case += 1;
        if self.valid_in_case >= self.quota_per_case {
            self.case_index += 1;
            self.valid_in_case = 0;
        }
        if self.case_index >= CASE_COUNT {
            self.attempt_index = 0;
            let count = u64::from(self.shard.count.max(1));
            let index = u64::from(self.shard.index);
            self.quota_per_case =
                PAIRED_VALIDATION_TRIALS / count + u64::from(index < PAIRED_VALIDATION_TRIALS % count);
            self.validation_candidate = 0;
            self.validation_outcomes = [None; 2];
            self.phase = TrialPhase::AwaitingReset;
            return Ok(DriveAction::Reset(self.reset_config()));
        }
        self.attempt_index += 1;
        self.phase = TrialPhase::AwaitingReset;
        Ok(DriveAction::Reset(self.reset_config()))
    }

    fn drive_paired_validation<B>(&mut self, backend: &B) -> RuntimeResult<DriveAction>
    where
        B: ContactGeometryBackend + PlantCreateBackend + PlantReadBackend + ZombieRawFactsBackend,
    {
        let TrialPhase::PairedValidation {
            candidate,
            cannon,
            cannon_initial_hp,
            mut cannon_last_hp,
            mut actual_damage,
            mut fodder,
            selected_plant_at,
            predicted,
            elapsed,
        } = self.phase
        else {
            return Ok(DriveAction::Continue);
        };

        if FULL_WINDOW_VALIDATION && fodder.is_none() && selected_plant_at.is_some() {
            fodder = find_validation_fodder(backend)?;
            if fodder.is_some() {
                let exploded = exploded_jacks(backend)?;
                self.validation_exploded_before_fodder = Some(exploded);
                self.validation_last_exploded = Some(exploded);
            }
        }
        let new_explosion = if predicted.is_some() {
            self.observe_validation_events(backend, fodder, elapsed)?
        } else {
            false
        };
        if predicted.is_some() {
            match cannon_hp(backend, cannon) {
                Some(hp) => {
                    actual_damage += f64::from((cannon_last_hp - hp).max(0));
                    cannon_last_hp = hp;
                }
                None if cannon_last_hp > 0 && new_explosion => {
                    actual_damage += 300.0;
                    cannon_last_hp = 0;
                }
                None if cannon_last_hp > 0 => {
                    return Err(RuntimeError::new(
                        "paired-validation cannon vanished without a Jack explosion",
                    ));
                }
                None => {}
            }
        }
        if elapsed == VALIDATION_OBSERVED_AT && predicted.is_none() {
            let cannon_initial_hp = cannon_hp(backend, cannon)
                .ok_or_else(|| RuntimeError::new("paired-validation cannon vanished at observation time"))?;
            cannon_last_hp = cannon_initial_hp;
            self.validation_last_exploded = Some(exploded_jacks(backend)?);
            self.phase = TrialPhase::PairedValidation {
                candidate,
                cannon,
                cannon_initial_hp,
                cannon_last_hp,
                actual_damage: 0.0,
                fodder,
                selected_plant_at,
                predicted,
                elapsed: elapsed + 1,
            };
            return Ok(DriveAction::Predict(candidate));
        }
        if !FULL_WINDOW_VALIDATION
            && elapsed == usize::try_from(VALIDATION_PLANT_AT[candidate]).expect("positive validation plant time")
        {
            let exploded = exploded_jacks(backend)?;
            self.validation_exploded_before_fodder = Some(exploded);
            self.validation_last_exploded = Some(exploded);
            fodder =
                Some(rsvz::core::logic::cards::new_plant(PlantKind::Sunflower, FODDER_GRID).map_err(runtime_error)?);
        }
        if elapsed >= VALIDATION_ACTIVATION_AT {
            let predicted = predicted.ok_or_else(|| RuntimeError::new("paired validation missed prediction time"))?;
            let selected_plant_at =
                selected_plant_at.ok_or_else(|| RuntimeError::new("paired validation missed selected plant time"))?;
            if fodder.is_none() {
                return Err(RuntimeError::new("paired validation did not plant the selected fodder"));
            }
            self.finish_paired_candidate(candidate, selected_plant_at, predicted, actual_damage);
            if self.valid_in_case >= self.quota_per_case {
                return Ok(DriveAction::Complete(self.take_artifact()));
            }
            self.phase = TrialPhase::AwaitingReset;
            return Ok(DriveAction::Reset(self.reset_config()));
        }

        self.phase = TrialPhase::PairedValidation {
            candidate,
            cannon,
            cannon_initial_hp,
            cannon_last_hp,
            actual_damage,
            fodder,
            selected_plant_at,
            predicted,
            elapsed: elapsed + 1,
        };
        Ok(DriveAction::Continue)
    }

    fn accept_prediction(
        &mut self, candidate: usize, selected_plant_at: i32, predicted: f64, solver_elapsed_ms: f64,
    ) -> RuntimeResult<()> {
        let TrialPhase::PairedValidation {
            candidate: active_candidate,
            cannon,
            cannon_initial_hp,
            cannon_last_hp,
            actual_damage,
            fodder,
            selected_plant_at: current_plant_at,
            predicted: current,
            elapsed,
        } = self.phase
        else {
            return Err(RuntimeError::new("paired prediction arrived outside validation"));
        };
        if active_candidate != candidate || current.is_some() || current_plant_at.is_some() {
            return Err(RuntimeError::new("paired prediction does not match active candidate"));
        }
        self.paired[candidate].solver_elapsed_ms.add(solver_elapsed_ms);
        self.phase = TrialPhase::PairedValidation {
            candidate,
            cannon,
            cannon_initial_hp,
            cannon_last_hp,
            actual_damage,
            fodder,
            selected_plant_at: Some(selected_plant_at),
            predicted: Some(predicted),
            elapsed,
        };
        Ok(())
    }

    fn observe_validation_events<B>(
        &mut self, backend: &B, fodder: Option<PlantId>, elapsed: usize,
    ) -> RuntimeResult<bool>
    where
        B: PlantReadBackend + ZombieRawFactsBackend,
    {
        let exploded = exploded_jacks(backend)?;
        let new_explosion = self
            .validation_last_exploded
            .is_some_and(|previous| exploded > previous);
        if new_explosion && self.validation_first_post_f_explosion_at.is_none() {
            self.validation_first_post_f_explosion_at = i32::try_from(elapsed).ok();
        }
        self.validation_last_exploded = Some(exploded);
        if let Some(fodder) = fodder
            && self.validation_fodder_death_at.is_none()
            && !plant_is_alive(backend, fodder)
        {
            self.validation_fodder_death_at = i32::try_from(elapsed).ok();
            self.validation_fodder_died_with_explosion = new_explosion;
        }
        Ok(new_explosion)
    }

    fn finish_paired_candidate(&mut self, candidate: usize, selected_plant_at: i32, predicted: f64, actual: f64) {
        let exploded_before_fodder = self
            .validation_exploded_before_fodder
            .take()
            .expect("paired validation recorded the pre-fodder Jack count");
        let exploded_at_end = self
            .validation_last_exploded
            .take()
            .expect("paired validation recorded the final Jack count");
        let outcome = ValidationOutcome {
            predicted,
            actual,
            exploded_before_fodder,
            exploded_at_end,
            first_post_f_explosion_at: self.validation_first_post_f_explosion_at.take(),
            fodder_death_at: self.validation_fodder_death_at.take(),
            fodder_died_with_explosion: std::mem::take(&mut self.validation_fodder_died_with_explosion),
        };
        self.paired[candidate]
            .selected_plant_at
            .add(f64::from(selected_plant_at));
        self.paired[candidate].predicted.add(predicted);
        self.paired[candidate].actual.add(actual);
        self.paired[candidate].difference.add(actual - predicted);
        self.paired[candidate]
            .cap_one_omitted
            .add(if exploded_before_fodder >= 2 { 1.0 } else { 0.0 });
        self.paired[candidate]
            .cap_one_excess
            .add(f64::from(exploded_before_fodder.saturating_sub(1)));
        let stratum = validation_event_stratum(outcome);
        self.paired[candidate].event_strata[stratum].add(predicted, actual);
        self.validation_outcomes[candidate] = Some(outcome);
        if FULL_WINDOW_VALIDATION {
            self.validation_outcomes = [None; 2];
            self.validation_candidate = 0;
            self.valid_in_case = self.valid_in_case.saturating_add(1);
            self.attempt_index = self.attempt_index.saturating_add(1);
            return;
        }
        if candidate == 0 {
            self.validation_candidate = 1;
            return;
        }
        let [outcome_0, outcome_1] = self
            .validation_outcomes
            .map(|outcome| outcome.expect("both paired candidates completed"));
        let predicted_0 = outcome_0.predicted;
        let actual_0 = outcome_0.actual;
        let predicted_1 = outcome_1.predicted;
        let actual_1 = outcome_1.actual;
        let between_stratum = usize::from(outcome_1.exploded_before_fodder > outcome_0.exploded_before_fodder);
        self.paired_between_plant_times[between_stratum].add(predicted_1 - predicted_0, actual_1 - actual_0);
        let predicted_best = usize::from(predicted_1 <= predicted_0);
        let actual_best = usize::from(actual_1 <= actual_0);
        if predicted_best != actual_best {
            self.paired_ranking_disagreements = self.paired_ranking_disagreements.saturating_add(1);
        }
        self.validation_outcomes = [None; 2];
        self.validation_candidate = 0;
        self.valid_in_case = self.valid_in_case.saturating_add(1);
        self.attempt_index = self.attempt_index.saturating_add(1);
    }

    fn record_active_frame<B>(
        &mut self, backend: &B, zombie: ZombieId, cannon: PlantId, cannon_rect: ContactRect, cannon_initial_hp: i32,
    ) -> RuntimeResult<()>
    where
        B: ContactGeometryBackend + ZombieRawFactsBackend,
    {
        let remaining = cannon_hp(backend, cannon).unwrap_or(0).max(0);
        self.damage_trace
            .push(f64::from((cannon_initial_hp - remaining).max(0)));
        if self.case_index / X_COUNT == 2 {
            let Some(zombie) = backend.zombie(zombie) else {
                return Err(RuntimeError::new("jack vanished while recording its position tail"));
            };
            let center = ContactCircle::new(
                backend.zombie_int_x(zombie) + JACK_CENTER_OFFSET,
                backend.zombie_int_y(zombie) + JACK_CENTER_OFFSET,
                JACK_EXPLOSION_RADIUS,
            );
            self.danger_trace.push([
                circle_hits_rect(center, cannon_rect),
                circle_hits_rect(center, shifted_rect(cannon_rect, -100)),
                circle_hits_rect(center, shifted_rect(cannon_rect, 100)),
                circle_hits_rect(center, shifted_rect(cannon_rect, -85)),
                circle_hits_rect(center, shifted_rect(cannon_rect, 85)),
            ]);
        }
        Ok(())
    }

    fn commit_trace(&mut self) {
        debug_assert_eq!(self.damage_trace.len(), H_COUNT);
        if self.case_index == POLE_CASE {
            for (h, damage) in self.damage_trace.iter().copied().enumerate() {
                self.pole[h].add(damage);
            }
            return;
        }
        let kind = self.case_index / X_COUNT;
        let x_index = self.case_index % X_COUNT;
        for (h, damage) in self.damage_trace.iter().copied().enumerate() {
            self.release[release_index(kind, x_index, h)].add(damage);
        }
        if kind == 2 {
            debug_assert_eq!(self.danger_trace.len(), H_COUNT);
            for (s, danger) in self.danger_trace.iter().copied().enumerate() {
                for (geometry, hit) in danger.into_iter().enumerate() {
                    self.jack[jack_index(geometry, x_index, s)].add(if hit { 1.0 } else { 0.0 });
                }
            }
        }
    }

    fn invalidate(&mut self) -> DriveAction {
        self.invalid_attempts = self.invalid_attempts.saturating_add(1);
        self.attempt_index = self.attempt_index.saturating_add(1);
        self.phase = TrialPhase::AwaitingReset;
        self.damage_trace.clear();
        self.danger_trace.clear();
        DriveAction::Reset(self.reset_config())
    }

    fn take_artifact(&mut self) -> SessionArtifact {
        artifact(SmartFodderArtifact {
            backend_name: std::mem::take(&mut self.backend_name),
            backend_version: std::mem::take(&mut self.backend_version),
            trials_per_case: FORMAL_TRIALS_PER_CASE,
            release: std::mem::take(&mut self.release),
            jack: std::mem::take(&mut self.jack),
            pole: std::mem::take(&mut self.pole),
            invalid_attempts: self.invalid_attempts,
            paired: std::mem::take(&mut self.paired),
            paired_between_plant_times: std::mem::take(&mut self.paired_between_plant_times),
            paired_ranking_disagreements: self.paired_ranking_disagreements,
            paired_invalid_attempts: self.paired_invalid_attempts,
            seed_shards: vec![SeedShard {
                index: self.shard.index,
                count: self.shard.count,
                seed_base: self.shard.seed_base,
                attempts: self.attempt_index.saturating_add(1),
            }],
        })
    }
}

enum DriveAction {
    Continue,
    Predict(usize),
    Reset(WorldResetConfig),
    Complete(SessionArtifact),
}

fn create_validation_board<B>(backend: &B) -> RuntimeResult<(PlantId, i32)>
where
    B: PlantCreateBackend
        + PlantEffectCountdownWriteBackend
        + PlantHealthWriteBackend
        + PlantReadBackend
        + PlantStateWriteBackend,
{
    let cannon = rsvz::core::logic::cards::new_plant(PlantKind::CobCannon, CANNON_GRID).map_err(runtime_error)?;
    let cannon_handle = backend
        .plant(cannon)
        .ok_or_else(|| RuntimeError::new("paired-validation cannon vanished during setup"))?;
    backend
        .set_plant_hp(
            cannon_handle,
            rsvz::core::model::PositiveHp::new(1_000_000).expect("validation HP is positive"),
        )
        .map_err(runtime_error)?;
    let _safety = rsvz::core::logic::cards::new_plant(PlantKind::TallNut, SAFETY_GRID).map_err(runtime_error)?;
    let ice = rsvz::core::logic::cards::new_plant(PlantKind::IceShroom, ICE_GRID).map_err(runtime_error)?;
    let ice_handle = backend
        .plant(ice)
        .ok_or_else(|| RuntimeError::new("paired-validation ice vanished during setup"))?;
    backend.set_plant_state(ice_handle, 2).map_err(runtime_error)?;
    rsvz::core::modifier::normalize_effect_countdown_by_id(ice, 1).map_err(runtime_error)?;
    let initial_hp =
        cannon_hp(backend, cannon).ok_or_else(|| RuntimeError::new("paired-validation cannon vanished"))?;
    Ok((cannon, initial_hp))
}

fn cannon_hp<B: PlantReadBackend>(backend: &B, id: PlantId) -> Option<i32> {
    backend.plant(id).map(|plant| backend.plant_hp(plant))
}

fn find_validation_fodder<B>(backend: &B) -> RuntimeResult<Option<PlantId>>
where
    B: PlantReadBackend,
{
    for plant in backend.plants() {
        if backend.plant_is_alive(plant)
            && (backend.plant_row(plant) == FODDER_GRID.row && backend.plant_col(plant) == FODDER_GRID.col)
            && backend.plant_kind(plant).map_err(runtime_error)? == PlantKind::Sunflower
        {
            return Ok(Some(backend.plant_id(plant)));
        }
    }
    Ok(None)
}

fn exploded_jacks<B>(backend: &B) -> RuntimeResult<u8>
where
    B: ZombieRawFactsBackend,
{
    let mut live = 0_u8;
    for zombie in backend.zombies() {
        if backend.zombie_is_alive(zombie)
            && !backend.zombie_is_disappeared(zombie)
            && backend.zombie_kind(zombie).map_err(runtime_error)? == ZombieKind::JackInTheBox
        {
            live = live.saturating_add(1);
        }
    }
    Ok(VALIDATION_JACK_COUNT.saturating_sub(live))
}

fn plant_is_alive<B: PlantReadBackend>(backend: &B, id: PlantId) -> bool {
    backend.plant(id).is_some_and(|plant| backend.plant_is_alive(plant))
}

fn validation_event_stratum(outcome: ValidationOutcome) -> usize {
    if outcome.exploded_at_end == outcome.exploded_before_fodder {
        0
    } else if outcome.fodder_died_with_explosion {
        1
    } else if outcome
        .fodder_death_at
        .zip(outcome.first_post_f_explosion_at)
        .is_some_and(|(death, explosion)| death < explosion)
    {
        2
    } else {
        3
    }
}

fn shifted_rect(rect: ContactRect, dy: i32) -> ContactRect {
    ContactRect::new(
        rect.left,
        rect.top.saturating_add(dy),
        rect.right,
        rect.bottom.saturating_add(dy),
    )
}

fn case_zombie_kind(case_index: usize) -> ZombieKind {
    if case_index == POLE_CASE {
        ZombieKind::PoleVaulting
    } else {
        match case_index / X_COUNT {
            0 => ZombieKind::Ladder,
            1 => ZombieKind::Football,
            2 => ZombieKind::JackInTheBox,
            _ => unreachable!("validated measurement case index"),
        }
    }
}

const fn release_index(kind: usize, x_index: usize, h: usize) -> usize {
    (kind * X_COUNT + x_index) * H_COUNT + h
}

const fn jack_index(geometry: usize, x_index: usize, s: usize) -> usize {
    (geometry * X_COUNT + x_index) * H_COUNT + s
}

fn x_for_index(index: usize) -> f32 {
    X_MIN + X_STEP * index as f32
}

fn runtime_error(error: impl std::fmt::Display) -> RuntimeError {
    RuntimeError::new(error.to_string())
}

#[rsvz::script]
fn script() {
    if NORMAL_WAVE_VALIDATION {
        if FULL_WINDOW_VALIDATION {
            rsvz::setup::select_cards([PlantKind::Sunflower]);
        }
        rsvz::setup::set_zombies(
            [
                ZombieKind::Ladder,
                ZombieKind::Ladder,
                ZombieKind::JackInTheBox,
                ZombieKind::Football,
                ZombieKind::Football,
            ],
            rsvz::core::model::ZombieSpawnMode::Exact,
        );
    }
    if rsvz::claim_session_job(SessionJobKey::new(&SMART_FODDER_MEASURE_JOB))? {
        let shard = rsvz::session_shard();
        let mut runner = MeasureRunner::new(shard);
        let initial_reset = runner.reset_config();
        let _task = rsvz::tick::spawn(
            TickOptions::playing_frame().lifetime(TickLifetime::Session),
            move |_meta| {
                let mut action = rsvz::__private::with_board_access(|access| runner.tick(access.backend()))
                    .and_then(|result| result)?;
                if let DriveAction::Predict(candidate) = action {
                    let plant_at = VALIDATION_PLANT_AT[candidate];
                    let plant_window = if FULL_WINDOW_VALIDATION {
                        VALIDATION_PLANT_AT[0]..=VALIDATION_PLANT_AT[1]
                    } else {
                        plant_at..=plant_at
                    };
                    let spec = rsvz::smart_fodder::SmartFodderSpec {
                        card: rsvz::core::model::CardSelection::Plant(PlantKind::Sunflower),
                        row: MEASURE_ROW + 1,
                        plant_window,
                        remove_by: Option::<i32>::None,
                        activation_at: VALIDATION_ACTIVATION_AT as i32,
                    };
                    let solver_started = std::time::Instant::now();
                    let (selected_plant_at, remove_at, expected_damage) = if FULL_WINDOW_VALIDATION {
                        let prediction = rsvz::smart_fodder::try_smart_fodder(spec)?;
                        (prediction.plant_at, prediction.remove_at, prediction.expected_damage)
                    } else {
                        let prediction =
                            rsvz::smart_fodder::predict_smart_fodder_at(&spec, VALIDATION_OBSERVED_AT as i32)?;
                        (prediction.plant_at, prediction.remove_at, prediction.expected_damage)
                    };
                    let solver_elapsed_ms = solver_started.elapsed().as_secs_f64() * 1_000.0;
                    if (!FULL_WINDOW_VALIDATION && selected_plant_at != plant_at)
                        || !(VALIDATION_PLANT_AT[0]..=VALIDATION_PLANT_AT[1]).contains(&selected_plant_at)
                        || remove_at.is_some()
                    {
                        return Err(RuntimeError::new(
                            "try_smart_fodder returned an invalid validation choice",
                        ));
                    }
                    runner.accept_prediction(candidate, selected_plant_at, expected_damage, solver_elapsed_ms)?;
                    action = DriveAction::Continue;
                }
                match action {
                    DriveAction::Continue => Ok(TickControl::Continue),
                    DriveAction::Predict(_) => unreachable!("prediction action was handled above"),
                    DriveAction::Reset(config) => {
                        rsvz::request_world_reset(config)?;
                        Ok(TickControl::Continue)
                    }
                    DriveAction::Complete(artifact) => {
                        rsvz::publish_artifact(artifact)?;
                        rsvz::stop_script();
                        Ok(TickControl::Stop)
                    }
                }
            },
        );
        rsvz::request_world_reset(initial_reset)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_quotas_sum_to_exactly_fifty_thousand_per_case() {
        for workers in 1..=64 {
            let total = (0..workers)
                .map(|index| {
                    MeasureRunner::new(SessionShard {
                        index,
                        count: workers,
                        seed_base: 100,
                    })
                    .quota_per_case
                })
                .sum::<u64>();
            assert_eq!(total, FORMAL_TRIALS_PER_CASE);
        }
    }

    #[test]
    fn artifact_contains_raw_stats_and_confidence_intervals() {
        let value = SmartFodderArtifact {
            backend_name: "test".to_owned(),
            backend_version: "1".to_owned(),
            trials_per_case: FORMAL_TRIALS_PER_CASE,
            release: vec![Stats::default(); RELEASE_KIND_COUNT * X_COUNT * H_COUNT],
            jack: vec![Stats::default(); JACK_GEOMETRY_COUNT * X_COUNT * H_COUNT],
            pole: vec![Stats::default(); H_COUNT],
            invalid_attempts: 0,
            paired: [PairedStats::default(); 2],
            paired_between_plant_times: [ComparisonStats::default(); VALIDATION_PAIR_STRATA.len()],
            paired_ranking_disagreements: 0,
            paired_invalid_attempts: 0,
            seed_shards: vec![SeedShard {
                index: 0,
                count: 1,
                seed_base: 100,
                attempts: 1,
            }],
        };
        let mut output = Vec::new();
        write_artifact(&value, &mut output).expect("artifact JSON");
        let output = String::from_utf8(output).expect("UTF-8");
        assert!(output.contains("\"sum_damage_sq\""));
        assert!(output.contains("\"danger_count\""));
        assert!(output.contains("\"ci95_half_width\""));
        assert!(output.contains("\"cap_one_omitted_frequency\""));
        assert!(output.contains("\"cap_one_excess_mean\""));
        assert!(output.contains("\"event_strata\""));
        assert!(output.contains("\"paired_between_plant_times\""));
    }

    #[test]
    fn validation_event_strata_are_exclusive() {
        let outcome = |before, end, explosion, death, with_explosion| ValidationOutcome {
            predicted: 0.0,
            actual: 0.0,
            exploded_before_fodder: before,
            exploded_at_end: end,
            first_post_f_explosion_at: explosion,
            fodder_death_at: death,
            fodder_died_with_explosion: with_explosion,
        };
        assert_eq!(validation_event_stratum(outcome(0, 0, None, None, false)), 0);
        assert_eq!(validation_event_stratum(outcome(0, 1, Some(10), Some(10), true)), 1);
        assert_eq!(validation_event_stratum(outcome(0, 1, Some(20), Some(10), false)), 2);
        assert_eq!(validation_event_stratum(outcome(0, 1, Some(10), None, false)), 3);
    }
}
