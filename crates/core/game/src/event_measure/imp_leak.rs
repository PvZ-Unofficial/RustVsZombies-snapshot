use rsvz_model::ZombiePhase;
use std::ops::Range;

#[cfg(test)]
use std::mem::size_of;

use rsvz_backend_api::backend::{PlantReadBackend, ZombieRawFactsBackend, ZombieReadBackend, ZombieStateBackend};
use rsvz_model::model::{
    GargantuarAshHitFact, GargantuarSpawnedFact, Grid, ImpThrownFact, PlantId, PlantKind, Wave, WaveClockState,
    ZombieId, ZombieKind,
};

use super::report::{
    ImpLeakAshHitReport, ImpLeakBiteEpisodeReport, ImpLeakCohortReport, ImpLeakFailureSampleReport,
    ImpLeakMotionSegmentReport, ImpLeakReport, ImpLeakTargetReport, ImpLeakThrowSnapshotReport, ImpLeakTimeReport,
    ImpLeakTraceStatusReport, ratio_or_zero,
};

#[cfg(test)]
pub(super) const MEMORY_BUDGET: usize = 512 * 1024;
const MAX_ACTIVE_FAMILIES: usize = 128;
const MAX_COHORTS: usize = 256;
const MAX_SAMPLES: usize = 64;
const MAX_TRIAL_MOTION: usize = 4_096;
const MAX_TRIAL_ASH: usize = 1_024;
const MAX_TRIAL_BITES: usize = 2_048;
const MAX_SAMPLE_MOTION: usize = 2_048;
const MAX_SAMPLE_ASH: usize = 512;
const MAX_SAMPLE_BITES: usize = 1_024;
const MAX_FAMILY_MOTION: usize = 32;
const MAX_FAMILY_ASH: usize = 8;
const MAX_FAMILY_BITES: usize = 16;
const TIMING_ERROR_BOUND_CS: u32 = 8;
const GARG_THROW_MIN_X: f32 = 400.0;

const CONTEXT_ELIGIBLE_DURING_SMASH: u8 = 1 << 0;
const CONTEXT_FROZEN_WHILE_PENDING: u8 = 1 << 1;
const CONTEXT_BUTTERED_WHILE_PENDING: u8 = 1 << 2;
const CONTEXT_RETRIED_THROW: u8 = 1 << 3;
const CONTEXT_HISTORY_INCOMPLETE: u8 = 1 << 4;
const OTHER_COHORT: CohortKey = CohortKey {
    parent_kind: -1,
    parent_from_wave: 0,
    throw_wave: None,
    row: 0,
    throw_context: CONTEXT_HISTORY_INCOMPLETE,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SampleOutcome {
    Continue,
    Invalid,
    ImpLeak,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TrialEnd {
    ObjectiveReached,
    ImpLeak,
    OtherFailure,
    Invalid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CohortKey {
    parent_kind: i32,
    parent_from_wave: i32,
    throw_wave: Option<i32>,
    row: i32,
    throw_context: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CohortStats {
    key: CohortKey,
    exposed_trials: u64,
    first_failure_trials: u64,
    thrown_imps: u64,
    safe_deaths: u64,
    censored_at_stop: u64,
    throw_time_min: Option<i32>,
    throw_time_max: Option<i32>,
}

impl CohortStats {
    fn exposed(key: CohortKey, throw_time: Option<i32>) -> Self {
        Self {
            key,
            exposed_trials: 1,
            first_failure_trials: 0,
            thrown_imps: 1,
            safe_deaths: 0,
            censored_at_stop: 0,
            throw_time_min: throw_time,
            throw_time_max: throw_time,
        }
    }

    fn merge_from(&mut self, other: Self) {
        self.exposed_trials = self.exposed_trials.saturating_add(other.exposed_trials);
        self.first_failure_trials = self.first_failure_trials.saturating_add(other.first_failure_trials);
        self.thrown_imps = self.thrown_imps.saturating_add(other.thrown_imps);
        self.safe_deaths = self.safe_deaths.saturating_add(other.safe_deaths);
        self.censored_at_stop = self.censored_at_stop.saturating_add(other.censored_at_stop);
        self.throw_time_min = min_option_i32(self.throw_time_min, other.throw_time_min);
        self.throw_time_max = max_option_i32(self.throw_time_max, other.throw_time_max);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TraceDrops {
    active_family_overflows: u64,
    cohort_overflows: u64,
    dropped_motion_segments: u64,
    dropped_ash_hits: u64,
    dropped_bite_episodes: u64,
    dropped_samples: u64,
}

impl TraceDrops {
    fn merge_from(&mut self, other: Self) {
        self.active_family_overflows = self
            .active_family_overflows
            .saturating_add(other.active_family_overflows);
        self.cohort_overflows = self.cohort_overflows.max(other.cohort_overflows);
        self.dropped_motion_segments = self
            .dropped_motion_segments
            .saturating_add(other.dropped_motion_segments);
        self.dropped_ash_hits = self.dropped_ash_hits.saturating_add(other.dropped_ash_hits);
        self.dropped_bite_episodes = self.dropped_bite_episodes.saturating_add(other.dropped_bite_episodes);
        self.dropped_samples = self.dropped_samples.saturating_add(other.dropped_samples);
    }

    fn complete(self) -> bool {
        self == Self::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MotionSegment {
    family: u32,
    start: i32,
    end: i32,
    start_x: f32,
    end_x: f32,
    raw_speed_x: f32,
    phase: i32,
    row: i32,
    moving: bool,
    frozen: bool,
    chilled: bool,
    buttered: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct AshHit {
    family: u32,
    at: i32,
    x: f32,
    hp_before: i32,
    phase: i32,
    frozen: bool,
    chilled: bool,
    buttered: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct BiteTarget {
    plant_id: PlantId,
    raw_kind: PlantKind,
    effective_kind: PlantKind,
    grid: Grid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BiteEpisode {
    family: u32,
    target: BiteTarget,
    start: i32,
    last_bite: i32,
    effective_cs: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ThrowSnapshot {
    parent_x: f32,
    parent_hp: i32,
    parent_max_hp: i32,
    parent_phase: i32,
    parent_speed_x: f32,
    parent_frozen: i32,
    parent_chilled: i32,
    parent_buttered: i32,
}

impl From<ImpThrownFact> for ThrowSnapshot {
    fn from(fact: ImpThrownFact) -> Self {
        Self {
            parent_x: fact.parent_x,
            parent_hp: fact.parent_hp,
            parent_max_hp: fact.parent_max_hp,
            parent_phase: fact.parent_phase.code(),
            parent_speed_x: fact.parent_speed_x,
            parent_frozen: fact.parent_frozen,
            parent_chilled: fact.parent_chilled,
            parent_buttered: fact.parent_buttered,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ImpFrameState {
    is_eating: bool,
    frozen: bool,
    buttered: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct Family {
    serial: u32,
    parent_id: ZombieId,
    parent_kind: ZombieKind,
    parent_from_wave: i32,
    row: i32,
    child_id: Option<ZombieId>,
    throw_counter: Option<i32>,
    first_eligible_counter: Option<i32>,
    first_throwing_counter: Option<i32>,
    throw_snapshot: Option<ThrowSnapshot>,
    cohort: Option<CohortKey>,
    context: u8,
    saw_throwing: bool,
    left_throwing_without_child: bool,
    parent_sampled: bool,
    last_parent_x: f32,
    last_parent_counter: i32,
    last_parent_phase: i32,
    last_parent_speed: f32,
    last_parent_flags: u8,
    last_parent_moving: bool,
    motion_index: Option<usize>,
    bite_target: Option<BiteTarget>,
    bite_index: Option<usize>,
    first_bite: Option<i32>,
    imp_frame: Option<ImpFrameState>,
    effective_chew_cs: u32,
    crossed_at_failure: bool,
}

impl Family {
    fn spawned(serial: u32, fact: GargantuarSpawnedFact) -> Self {
        Self {
            serial,
            parent_id: fact.parent_id,
            parent_kind: fact.parent_kind,
            parent_from_wave: report_wave(fact.from_wave),
            row: fact.row + 1,
            child_id: None,
            throw_counter: None,
            first_eligible_counter: None,
            first_throwing_counter: None,
            throw_snapshot: None,
            cohort: None,
            context: 0,
            saw_throwing: false,
            left_throwing_without_child: false,
            parent_sampled: false,
            last_parent_x: 0.0,
            last_parent_counter: 0,
            last_parent_phase: 0,
            last_parent_speed: 0.0,
            last_parent_flags: 0,
            last_parent_moving: false,
            motion_index: None,
            bite_target: None,
            bite_index: None,
            first_bite: None,
            imp_frame: None,
            effective_chew_cs: 0,
            crossed_at_failure: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TimePoint {
    main_counter: i32,
    wave: Option<i32>,
    time: Option<i32>,
}

#[derive(Clone, Debug, PartialEq)]
struct SampleHeader {
    trial_sequence: u64,
    seed: u32,
    world_epoch: u64,
    key: CohortKey,
    parent_id: ZombieId,
    imp_id: ZombieId,
    thrown_at: TimePoint,
    first_eligible_at: Option<TimePoint>,
    first_throwing_at: Option<TimePoint>,
    failed_at: TimePoint,
    first_bite_at: Option<TimePoint>,
    effective_chew_cs: u32,
    imp_x: f32,
    target: BiteTarget,
    throw_snapshot: ThrowSnapshot,
    coincident_imp_ids: [u32; MAX_ACTIVE_FAMILIES],
    coincident_len: u8,
    motion: Range<usize>,
    ash: Range<usize>,
    bites: Range<usize>,
    trace_complete: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct ImpLeakAggregate {
    cohorts: Vec<CohortStats>,
    samples: Vec<SampleHeader>,
    sample_motion: Vec<MotionSegment>,
    sample_ash: Vec<AshHit>,
    sample_bites: Vec<BiteEpisode>,
    drops: TraceDrops,
}

impl ImpLeakAggregate {
    pub(super) fn reserve(&mut self, samples: bool) {
        self.cohorts.reserve_exact(MAX_COHORTS);
        if samples {
            self.samples.reserve_exact(MAX_SAMPLES);
            self.sample_motion.reserve_exact(MAX_SAMPLE_MOTION);
            self.sample_ash.reserve_exact(MAX_SAMPLE_ASH);
            self.sample_bites.reserve_exact(MAX_SAMPLE_BITES);
        }
    }

    pub(super) fn merge_from(&mut self, other: &Self) {
        for cohort in &other.cohorts {
            self.merge_cohort(*cohort);
        }
        self.cohorts.sort_unstable_by_key(|cohort| cohort.key);
        self.merge_samples(other);
        self.drops.merge_from(other.drops);
    }

    pub(super) fn merge_drops_from(&mut self, other: &Self) {
        self.drops.merge_from(other.drops);
    }

    pub(super) fn clear_trial(&mut self) {
        self.cohorts.clear();
        self.samples.clear();
        self.sample_motion.clear();
        self.sample_ash.clear();
        self.sample_bites.clear();
        self.drops = TraceDrops::default();
    }

    fn merge_samples(&mut self, other: &Self) {
        for (index, sample) in other.samples.iter().enumerate() {
            let rank = sample_rank(sample);
            if let Some(existing) = self.samples.iter().position(|entry| entry.key == sample.key) {
                if sample_rank(&self.samples[existing]) <= rank {
                    continue;
                }
                self.remove_sample(existing);
            }
            if self.samples.len() >= MAX_SAMPLES {
                let latest = self
                    .samples
                    .iter()
                    .enumerate()
                    .max_by_key(|(_, entry)| sample_rank(entry))
                    .map(|(index, entry)| (index, sample_rank(entry)));
                if let Some((latest_index, latest_rank)) = latest
                    && rank < latest_rank
                {
                    self.remove_sample(latest_index);
                    self.drops.dropped_samples = self.drops.dropped_samples.saturating_add(1);
                } else {
                    self.drops.dropped_samples = self.drops.dropped_samples.saturating_add(1);
                    continue;
                }
            }
            self.copy_sample_from(other, index);
        }
        self.samples.sort_unstable_by_key(|sample| sample.trial_sequence);
        while self.samples.len() > MAX_SAMPLES {
            self.remove_sample(self.samples.len() - 1);
            self.drops.dropped_samples = self.drops.dropped_samples.saturating_add(1);
        }
    }

    fn copy_sample_from(&mut self, source: &Self, index: usize) {
        let source_header = &source.samples[index];
        let motion_start = self.sample_motion.len();
        self.sample_motion
            .extend_from_slice(&source.sample_motion[source_header.motion.clone()]);
        let ash_start = self.sample_ash.len();
        self.sample_ash
            .extend_from_slice(&source.sample_ash[source_header.ash.clone()]);
        let bites_start = self.sample_bites.len();
        self.sample_bites
            .extend_from_slice(&source.sample_bites[source_header.bites.clone()]);
        let mut header = source_header.clone();
        header.motion = motion_start..self.sample_motion.len();
        header.ash = ash_start..self.sample_ash.len();
        header.bites = bites_start..self.sample_bites.len();
        self.samples.push(header);
    }

    fn remove_sample(&mut self, index: usize) {
        let header = self.samples.remove(index);
        remove_range(&mut self.sample_motion, header.motion.clone());
        remove_range(&mut self.sample_ash, header.ash.clone());
        remove_range(&mut self.sample_bites, header.bites.clone());
        for sample in &mut self.samples {
            adjust_range(&mut sample.motion, &header.motion);
            adjust_range(&mut sample.ash, &header.ash);
            adjust_range(&mut sample.bites, &header.bites);
        }
    }

    fn cohort_mut(&mut self, key: CohortKey) -> Option<&mut CohortStats> {
        self.cohorts.iter_mut().find(|cohort| cohort.key == key)
    }

    fn cohort_bucket_mut(&mut self, key: CohortKey) -> Option<&mut CohortStats> {
        let bucket = if self.cohorts.iter().any(|cohort| cohort.key == key) {
            key
        } else {
            OTHER_COHORT
        };
        self.cohort_mut(bucket)
    }

    fn expose(&mut self, key: CohortKey, throw_time: Option<i32>) {
        if let Some(cohort) = self.cohort_mut(key) {
            cohort.thrown_imps = cohort.thrown_imps.saturating_add(1);
            cohort.throw_time_min = min_option_i32(cohort.throw_time_min, throw_time);
            cohort.throw_time_max = max_option_i32(cohort.throw_time_max, throw_time);
            return;
        }
        if self.real_cohort_count() < MAX_COHORTS - 1 {
            self.cohorts.push(CohortStats::exposed(key, throw_time));
            return;
        }
        let (largest_index, largest_key) = self
            .cohorts
            .iter()
            .enumerate()
            .filter(|(_, cohort)| cohort.key != OTHER_COHORT)
            .max_by_key(|(_, cohort)| cohort.key)
            .map(|(index, cohort)| (index, cohort.key))
            .expect("a full cohort table has a real cohort");
        if key < largest_key {
            let evicted = self.cohorts.remove(largest_index);
            self.fold_into_other(evicted);
            self.cohorts.push(CohortStats::exposed(key, throw_time));
        } else {
            self.fold_exposure_into_other(throw_time);
        }
    }

    fn merge_cohort(&mut self, cohort: CohortStats) {
        if cohort.key == OTHER_COHORT {
            self.fold_into_other(cohort);
            return;
        }
        if let Some(existing) = self.cohort_mut(cohort.key) {
            existing.merge_from(cohort);
            return;
        }
        if self.real_cohort_count() < MAX_COHORTS - 1 {
            self.cohorts.push(cohort);
            return;
        }
        let (largest_index, largest_key) = self
            .cohorts
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.key != OTHER_COHORT)
            .max_by_key(|(_, entry)| entry.key)
            .map(|(index, entry)| (index, entry.key))
            .expect("a full cohort table has a real cohort");
        if cohort.key < largest_key {
            let evicted = self.cohorts.remove(largest_index);
            self.fold_into_other(evicted);
            self.cohorts.push(cohort);
        } else {
            self.fold_into_other(cohort);
        }
    }

    fn real_cohort_count(&self) -> usize {
        self.cohorts.iter().filter(|cohort| cohort.key != OTHER_COHORT).count()
    }

    fn fold_exposure_into_other(&mut self, throw_time: Option<i32>) {
        self.drops.cohort_overflows = 1;
        if let Some(other) = self.cohort_mut(OTHER_COHORT) {
            other.exposed_trials = other.exposed_trials.saturating_add(1);
            other.thrown_imps = other.thrown_imps.saturating_add(1);
            other.throw_time_min = min_option_i32(other.throw_time_min, throw_time);
            other.throw_time_max = max_option_i32(other.throw_time_max, throw_time);
        } else {
            self.cohorts.push(CohortStats::exposed(OTHER_COHORT, throw_time));
        }
    }

    fn fold_into_other(&mut self, mut cohort: CohortStats) {
        if cohort.key != OTHER_COHORT {
            self.drops.cohort_overflows = 1;
            cohort.key = OTHER_COHORT;
        }
        if let Some(other) = self.cohort_mut(OTHER_COHORT) {
            other.merge_from(cohort);
        } else {
            self.cohorts.push(cohort);
        }
    }

    pub(super) fn report(&self, enabled: bool, threshold_cs: u32, failure_trials: u64) -> ImpLeakReport {
        let cohorts = self
            .cohorts
            .iter()
            .map(|cohort| ImpLeakCohortReport {
                parent_kind: zombie_name(cohort.key.parent_kind),
                parent_from_wave: cohort.key.parent_from_wave,
                throw_wave: cohort.key.throw_wave,
                row: cohort.key.row,
                throw_context: context_names(cohort.key.throw_context),
                exposed_trials: (cohort.key != OTHER_COHORT).then_some(cohort.exposed_trials),
                first_failure_trials: cohort.first_failure_trials,
                first_failure_rate: (cohort.key != OTHER_COHORT)
                    .then(|| ratio_or_zero(cohort.first_failure_trials, cohort.exposed_trials)),
                thrown_imps: cohort.thrown_imps,
                safe_deaths: cohort.safe_deaths,
                censored_at_stop: cohort.censored_at_stop,
                throw_time_min: cohort.throw_time_min,
                throw_time_max: cohort.throw_time_max,
            })
            .collect();
        let samples = self.samples.iter().map(|sample| self.sample_report(sample)).collect();
        ImpLeakReport {
            enabled,
            threshold_cs,
            timing_error_bound_cs: TIMING_ERROR_BOUND_CS,
            failure_trials,
            cohorts,
            samples,
            trace: ImpLeakTraceStatusReport {
                complete: self.drops.complete(),
                active_family_overflows: self.drops.active_family_overflows,
                cohort_overflows: self
                    .cohorts
                    .iter()
                    .find(|cohort| cohort.key == OTHER_COHORT)
                    .map_or(self.drops.cohort_overflows, |cohort| cohort.thrown_imps),
                dropped_motion_segments: self.drops.dropped_motion_segments,
                dropped_ash_hits: self.drops.dropped_ash_hits,
                dropped_bite_episodes: self.drops.dropped_bite_episodes,
                dropped_samples: self.drops.dropped_samples,
            },
        }
    }

    fn sample_report(&self, sample: &SampleHeader) -> ImpLeakFailureSampleReport {
        let motion = self.sample_motion[sample.motion.clone()]
            .iter()
            .map(|segment| motion_report(segment, sample.thrown_at.main_counter))
            .collect();
        let ash_hits = self.sample_ash[sample.ash.clone()].iter().map(ash_report).collect();
        let bite_episodes = self.sample_bites[sample.bites.clone()]
            .iter()
            .map(bite_report)
            .collect();
        ImpLeakFailureSampleReport {
            trial_sequence: sample.trial_sequence,
            seed: sample.seed,
            world_epoch: sample.world_epoch,
            parent_id: sample.parent_id.raw(),
            imp_id: sample.imp_id.raw(),
            imp_pool_order: if sample.imp_id.index() < sample.parent_id.index() {
                "lower_than_parent".to_owned()
            } else {
                "higher_than_parent".to_owned()
            },
            parent_kind: zombie_name(sample.key.parent_kind),
            parent_from_wave: sample.key.parent_from_wave,
            row: sample.key.row,
            throw_context: context_names(sample.key.throw_context),
            thrown_at: time_report(sample.thrown_at),
            first_eligible_at: sample.first_eligible_at.map(time_report),
            first_throwing_at: sample.first_throwing_at.map(time_report),
            failed_at: time_report(sample.failed_at),
            first_bite_at: sample.first_bite_at.map(time_report),
            effective_chew_cs: sample.effective_chew_cs,
            imp_x_at_failure: sample.imp_x,
            target: target_report(sample.target),
            throw_snapshot: throw_report(sample.throw_snapshot),
            motion,
            ash_hits,
            bite_episodes,
            coincident_imp_ids: sample.coincident_imp_ids[..usize::from(sample.coincident_len)].to_vec(),
            trace_complete: sample.trace_complete,
        }
    }
}

pub(super) struct ImpLeakTracker {
    threshold_cs: u32,
    active: Vec<Family>,
    motion: Vec<MotionSegment>,
    ash: Vec<AshHit>,
    bites: Vec<BiteEpisode>,
    serial: u32,
    seeded: bool,
    critical_overflow: bool,
    drops: TraceDrops,
    trial_sequence: u64,
    seed: u32,
    world_epoch: u64,
    last_sample_counter: Option<i32>,
}

impl ImpLeakTracker {
    pub(super) fn new(_threshold_cs: u32) -> Self {
        Self {
            threshold_cs: _threshold_cs,
            active: Vec::with_capacity(MAX_ACTIVE_FAMILIES),
            motion: Vec::with_capacity(MAX_TRIAL_MOTION),
            ash: Vec::with_capacity(MAX_TRIAL_ASH),
            bites: Vec::with_capacity(MAX_TRIAL_BITES),
            serial: 0,
            seeded: false,
            critical_overflow: false,
            drops: TraceDrops::default(),
            trial_sequence: 0,
            seed: 0,
            world_epoch: 0,
            last_sample_counter: None,
        }
    }

    pub(super) fn start_trial(&mut self, trial_sequence: u64, seed: u32, world_epoch: u64) {
        self.active.clear();
        self.motion.clear();
        self.ash.clear();
        self.bites.clear();
        self.seeded = false;
        self.critical_overflow = false;
        self.drops = TraceDrops::default();
        self.trial_sequence = trial_sequence;
        self.seed = seed;
        self.world_epoch = world_epoch;
        self.last_sample_counter = None;
    }

    pub(super) fn note_gargantuar_spawned(&mut self, fact: GargantuarSpawnedFact) {
        if self.active.iter().any(|family| family.parent_id == fact.parent_id) {
            return;
        }
        let serial = self.next_serial();
        self.push_family(Family::spawned(serial, fact));
    }

    pub(super) fn note_imp_thrown(&mut self, fact: ImpThrownFact) {
        let index = self
            .active
            .iter()
            .position(|family| family.parent_id == fact.parent_id)
            .or_else(|| {
                let spawned = GargantuarSpawnedFact {
                    parent_id: fact.parent_id,
                    parent_kind: fact.parent_kind,
                    from_wave: fact.from_wave,
                    row: fact.row,
                    main_counter: fact.main_counter,
                };
                let serial = self.next_serial();
                self.push_family(Family::spawned(serial, spawned))
            });
        let Some(index) = index else { return };
        let family = &mut self.active[index];
        family.child_id = Some(fact.imp_id);
        family.throw_counter = Some(fact.main_counter);
        family.throw_snapshot = Some(fact.into());
        if fact.parent_frozen > 0 {
            family.context |= CONTEXT_FROZEN_WHILE_PENDING;
        }
        if fact.parent_buttered > 0 {
            family.context |= CONTEXT_BUTTERED_WHILE_PENDING;
        }
        if family.left_throwing_without_child {
            family.context |= CONTEXT_RETRIED_THROW;
        }
    }

    pub(super) fn note_ash_hit(&mut self, fact: GargantuarAshHitFact) {
        let Some(family) = self.active.iter().find(|family| family.parent_id == fact.parent_id) else {
            return;
        };
        if self.ash.len() == MAX_TRIAL_ASH
            || self.ash.iter().filter(|record| record.family == family.serial).count() == MAX_FAMILY_ASH
        {
            self.drops.dropped_ash_hits = self.drops.dropped_ash_hits.saturating_add(1);
            return;
        }
        self.ash.push(AshHit {
            family: family.serial,
            at: fact.main_counter,
            x: fact.x,
            hp_before: fact.hp_before,
            phase: fact.phase.code(),
            frozen: fact.frozen > 0,
            chilled: fact.chilled > 0,
            buttered: fact.buttered > 0,
        });
    }

    pub(super) fn note_bite(&mut self, zombie_id: ZombieId, target: BiteTarget, protected: bool, main_counter: i32) {
        let Some(family_index) = self.active.iter().position(|family| family.child_id == Some(zombie_id)) else {
            return;
        };
        if !protected {
            let family = &mut self.active[family_index];
            family.bite_target = None;
            family.bite_index = None;
            return;
        }
        let family = &mut self.active[family_index];
        family.first_bite.get_or_insert(main_counter);
        if family.bite_target == Some(target) {
            if let Some(index) = family.bite_index {
                self.bites[index].last_bite = main_counter;
            }
            return;
        }
        family.bite_target = Some(target);
        if self.bites.len() == MAX_TRIAL_BITES
            || self
                .bites
                .iter()
                .filter(|record| record.family == family.serial)
                .count()
                == MAX_FAMILY_BITES
        {
            family.bite_index = None;
            self.drops.dropped_bite_episodes = self.drops.dropped_bite_episodes.saturating_add(1);
            return;
        }
        let index = self.bites.len();
        self.bites.push(BiteEpisode {
            family: family.serial,
            target,
            start: main_counter,
            last_bite: main_counter,
            effective_cs: 0,
        });
        family.bite_index = Some(index);
    }

    pub(super) fn sample(
        &mut self, main_counter: i32, clocks: &WaveClockState, trial: &mut ImpLeakAggregate,
        samples: &mut ImpLeakAggregate,
    ) -> Result<SampleOutcome, crate::runtime::RuntimeError>
    where
        rsvz_current::CurrentBackend: PlantReadBackend + ZombieRawFactsBackend + ZombieStateBackend,
    {
        rsvz_current::with_backend_shared(|access| {
            let backend = access;
            if !self.begin_sample(main_counter) {
                return Ok(SampleOutcome::Continue);
            }
            if !self.seeded {
                self.seed_existing(main_counter)?;
                self.seeded = true;
            }
            if self.critical_overflow {
                return Ok(SampleOutcome::Invalid);
            }

            let mut primary: Option<(ZombieId, f32)> = None;
            let mut coincident = [0_u32; MAX_ACTIVE_FAMILIES];
            let mut coincident_len = 0_usize;
            let mut index = 0;
            while index < self.active.len() {
                self.resolve_cohort(index, clocks, trial);
                let parent_alive = self.sample_parent(index, main_counter)?;
                let child_id = self.active[index].child_id;
                if let Some(child_id) = child_id {
                    let Some(child) = crate::live_value::read_or_abort(backend.zombie(child_id), "zombie") else {
                        self.note_safe_death(index, trial);
                        self.remove_family(index);
                        continue;
                    };
                    if !backend.zombie_is_alive(child) {
                        self.note_safe_death(index, trial);
                        self.remove_family(index);
                        continue;
                    }
                    let plant_alive = match self.active[index].bite_target {
                        Some(target) => crate::live_value::read_or_abort(backend.plant(target.plant_id), "plant")
                            .is_some_and(|plant| backend.plant_is_alive(plant)),
                        None => false,
                    };
                    if effective_frame(self.active[index].imp_frame, plant_alive) {
                        let family = &mut self.active[index];
                        family.effective_chew_cs = family.effective_chew_cs.saturating_add(1);
                        if let Some(bite_index) = family.bite_index {
                            self.bites[bite_index].effective_cs = self.bites[bite_index].effective_cs.saturating_add(1);
                        }
                    }
                    let imp_x = backend.zombie_pos_x(child);
                    self.active[index].imp_frame = Some(ImpFrameState {
                        is_eating: backend.zombie_is_eating(child),
                        frozen: backend.zombie_frozen_countdown(child) > 0,
                        buttered: backend.zombie_buttered_countdown(child) > 0,
                    });
                    if crossed_threshold(self.active[index].effective_chew_cs, self.threshold_cs) {
                        self.active[index].crossed_at_failure = true;
                        if primary.is_none_or(|(id, _)| child_id < id) {
                            if let Some((old, _)) = primary
                                && coincident_len < coincident.len()
                            {
                                coincident[coincident_len] = old.raw();
                                coincident_len += 1;
                            }
                            primary = Some((child_id, imp_x));
                        } else if coincident_len < coincident.len() {
                            coincident[coincident_len] = child_id.raw();
                            coincident_len += 1;
                        }
                    }
                } else if !parent_alive {
                    self.remove_family(index);
                    continue;
                }
                index += 1;
            }

            let Some((imp_id, imp_x)) = primary else {
                return Ok(SampleOutcome::Continue);
            };
            self.capture_failure(
                imp_id,
                imp_x,
                main_counter,
                clocks,
                trial,
                samples,
                coincident,
                coincident_len,
            );
            Ok(SampleOutcome::ImpLeak)
        })
        .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))?
    }

    pub(super) fn finish_trial(&mut self, end: TrialEnd, trial: &mut ImpLeakAggregate) {
        if matches!(end, TrialEnd::ImpLeak | TrialEnd::OtherFailure) {
            for family in &self.active {
                if family.child_id.is_none() {
                    continue;
                }
                if end == TrialEnd::ImpLeak && family.crossed_at_failure {
                    continue;
                }
                if let Some(key) = family.cohort
                    && let Some(cohort) = trial.cohort_bucket_mut(key)
                {
                    cohort.censored_at_stop = cohort.censored_at_stop.saturating_add(1);
                }
            }
        }
        trial.drops.merge_from(self.drops);
    }

    pub(super) fn resolve_pending_cohorts(&mut self, clocks: &WaveClockState, trial: &mut ImpLeakAggregate) {
        for index in 0..self.active.len() {
            self.resolve_cohort(index, clocks, trial);
        }
    }

    pub(super) const fn has_critical_overflow(&self) -> bool {
        self.critical_overflow
    }

    #[cfg(test)]
    pub(super) fn reserved_bytes(&self, trial: &ImpLeakAggregate, partial: &ImpLeakAggregate) -> usize {
        self.active.capacity() * size_of::<Family>()
            + self.motion.capacity() * size_of::<MotionSegment>()
            + self.ash.capacity() * size_of::<AshHit>()
            + self.bites.capacity() * size_of::<BiteEpisode>()
            + aggregate_capacity_bytes(trial)
            + aggregate_capacity_bytes(partial)
    }

    fn next_serial(&mut self) -> u32 {
        let serial = self.serial;
        self.serial = self.serial.wrapping_add(1);
        serial
    }

    fn begin_sample(&mut self, main_counter: i32) -> bool {
        if self.last_sample_counter == Some(main_counter) {
            return false;
        }
        self.last_sample_counter = Some(main_counter);
        true
    }

    fn push_family(&mut self, family: Family) -> Option<usize> {
        if self.active.len() == MAX_ACTIVE_FAMILIES {
            self.critical_overflow = true;
            self.drops.active_family_overflows = 1;
            return None;
        }
        self.active.push(family);
        Some(self.active.len() - 1)
    }

    fn seed_existing(&mut self, main_counter: i32) -> Result<(), crate::runtime::RuntimeError>
    where
        rsvz_current::CurrentBackend: ZombieRawFactsBackend + ZombieStateBackend,
    {
        rsvz_current::with_backend_shared(|access| {
            let backend = access;
            for zombie in crate::live_value::read_or_abort(backend.zombies(), "zombies") {
                let kind = crate::live_value::read_or_abort(backend.zombie_kind(zombie), "zombie_kind");
                if !matches!(kind, ZombieKind::Gargantuar | ZombieKind::GigaGargantuar)
                    || !backend.zombie_has_object(zombie)
                {
                    continue;
                }
                let id = backend.zombie_id(zombie);
                if self.active.iter().any(|family| family.parent_id == id) {
                    continue;
                }
                let fact = GargantuarSpawnedFact {
                    parent_id: id,
                    parent_kind: kind,
                    from_wave: backend.zombie_from_wave(zombie),
                    row: backend.zombie_row(zombie),
                    main_counter,
                };
                let mut family = Family::spawned(self.next_serial(), fact);
                family.context |= CONTEXT_HISTORY_INCOMPLETE;
                self.push_family(family);
            }
            Ok(())
        })
        .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))?
    }

    fn sample_parent(&mut self, index: usize, main_counter: i32) -> Result<bool, crate::runtime::RuntimeError>
    where
        rsvz_current::CurrentBackend: ZombieRawFactsBackend + ZombieStateBackend,
    {
        rsvz_current::with_backend_shared(|access| {
            let backend = access;
            let parent_id = self.active[index].parent_id;
            let Some(parent) = crate::live_value::read_or_abort(backend.zombie(parent_id), "zombie") else {
                return Ok(false);
            };
            if !backend.zombie_is_alive(parent) {
                return Ok(false);
            }
            let phase = crate::live_value::read_or_abort(backend.zombie_phase(parent), "zombie_phase");
            let hp = backend.zombie_hp(parent);
            let max_hp = backend.zombie_max_hp(parent);
            let frozen = backend.zombie_frozen_countdown(parent) > 0;
            let chilled = backend.zombie_chilled_countdown(parent) > 0;
            let buttered = backend.zombie_buttered_countdown(parent) > 0;
            let has_object = backend.zombie_has_object(parent);
            let x = backend.zombie_pos_x(parent);
            let eligible = throw_pending_eligible(has_object, hp, max_hp, x);
            self.record_parent_controls(index, main_counter, phase, eligible, frozen, buttered, has_object);
            self.record_motion(
                index,
                main_counter,
                x,
                backend.zombie_speed_x(parent),
                phase,
                backend.zombie_row(parent),
                frozen,
                chilled,
                buttered,
            );
            Ok(true)
        })
        .map_err(|error| crate::runtime::RuntimeError::new(error.to_string()))?
    }

    fn record_parent_controls(
        &mut self, index: usize, main_counter: i32, phase: ZombiePhase, eligible: bool, frozen: bool, buttered: bool,
        has_object: bool,
    ) {
        let family = &mut self.active[index];
        if family.child_id.is_none() {
            if eligible {
                family.first_eligible_counter.get_or_insert(main_counter);
            }
            if phase == ZombiePhase::GargantuarThrowing {
                family.first_throwing_counter.get_or_insert(main_counter);
            }
        }
        if eligible && phase == ZombiePhase::GargantuarSmashing {
            family.context |= CONTEXT_ELIGIBLE_DURING_SMASH;
        }
        if eligible && frozen {
            family.context |= CONTEXT_FROZEN_WHILE_PENDING;
        }
        if eligible && buttered {
            family.context |= CONTEXT_BUTTERED_WHILE_PENDING;
        }
        if phase == ZombiePhase::GargantuarThrowing {
            family.saw_throwing = true;
        } else if family.saw_throwing && family.child_id.is_none() && has_object {
            family.left_throwing_without_child = true;
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "one copied parent motion sample has these native scalars"
    )]
    fn record_motion(
        &mut self, family_index: usize, counter: i32, x: f32, speed: f32, phase: ZombiePhase, row: i32, frozen: bool,
        chilled: bool, buttered: bool,
    ) {
        let family = &mut self.active[family_index];
        let moving = family.parent_sampled && (x - family.last_parent_x).abs() > 0.000_1;
        let flags = u8::from(frozen) | (u8::from(chilled) << 1) | (u8::from(buttered) << 2);
        let same = family.parent_sampled
            && family.last_parent_phase == phase.code()
            && family.last_parent_speed.to_bits() == speed.to_bits()
            && family.last_parent_flags == flags
            && family.last_parent_moving == moving;
        if same {
            if let Some(index) = family.motion_index {
                let segment = &mut self.motion[index];
                segment.end = counter;
                segment.end_x = x;
            }
        } else if self.motion.len() == MAX_TRIAL_MOTION
            || self
                .motion
                .iter()
                .filter(|record| record.family == family.serial)
                .count()
                == MAX_FAMILY_MOTION
        {
            family.motion_index = None;
            self.drops.dropped_motion_segments = self.drops.dropped_motion_segments.saturating_add(1);
        } else {
            let (start, start_x) = if family.parent_sampled {
                (family.last_parent_counter, family.last_parent_x)
            } else {
                (counter, x)
            };
            let index = self.motion.len();
            self.motion.push(MotionSegment {
                family: family.serial,
                start,
                end: counter,
                start_x,
                end_x: x,
                raw_speed_x: speed,
                phase: phase.code(),
                row: row + 1,
                moving,
                frozen,
                chilled,
                buttered,
            });
            family.motion_index = Some(index);
        }
        family.parent_sampled = true;
        family.last_parent_x = x;
        family.last_parent_counter = counter;
        family.last_parent_phase = phase.code();
        family.last_parent_speed = speed;
        family.last_parent_flags = flags;
        family.last_parent_moving = moving;
    }

    fn resolve_cohort(&mut self, index: usize, clocks: &WaveClockState, trial: &mut ImpLeakAggregate) {
        if self.active[index].cohort.is_some() {
            return;
        }
        let Some(counter) = self.active[index].throw_counter else {
            return;
        };
        let relative = relative_time(counter, clocks);
        let family = &mut self.active[index];
        if family.left_throwing_without_child {
            family.context |= CONTEXT_RETRIED_THROW;
        }
        let key = CohortKey {
            parent_kind: family.parent_kind.code(),
            parent_from_wave: family.parent_from_wave,
            throw_wave: relative.and_then(|time| time.wave),
            row: family.row,
            throw_context: family.context,
        };
        trial.expose(key, relative.and_then(|time| time.time));
        family.cohort = Some(key);
    }

    fn note_safe_death(&mut self, index: usize, trial: &mut ImpLeakAggregate) {
        if let Some(key) = self.active[index].cohort
            && let Some(cohort) = trial.cohort_bucket_mut(key)
        {
            cohort.safe_deaths = cohort.safe_deaths.saturating_add(1);
        }
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "failure capture receives one complete sealed sample"
    )]
    fn capture_failure(
        &mut self, imp_id: ZombieId, imp_x: f32, counter: i32, clocks: &WaveClockState, trial: &mut ImpLeakAggregate,
        samples: &mut ImpLeakAggregate, coincident: [u32; MAX_ACTIVE_FAMILIES], coincident_len: usize,
    ) {
        let Some(index) = self.active.iter().position(|family| family.child_id == Some(imp_id)) else {
            return;
        };
        self.resolve_cohort(index, clocks, trial);
        let family = &self.active[index];
        let Some(key) = family.cohort else { return };
        if let Some(cohort) = trial.cohort_bucket_mut(key) {
            cohort.first_failure_trials = cohort.first_failure_trials.saturating_add(1);
        }
        let (Some(throw_counter), Some(snapshot), Some(target)) =
            (family.throw_counter, family.throw_snapshot, family.bite_target)
        else {
            return;
        };
        let trace_complete = self.drops.complete() && trial.drops.complete();
        let rank = (u8::from(!trace_complete), self.trial_sequence);
        if let Some(existing) = samples.samples.iter().position(|sample| sample.key == key) {
            if sample_rank(&samples.samples[existing]) <= rank {
                return;
            }
            samples.remove_sample(existing);
        }
        if samples.samples.len() == MAX_SAMPLES {
            let Some((latest, latest_rank)) = samples
                .samples
                .iter()
                .enumerate()
                .max_by_key(|(_, sample)| sample_rank(sample))
                .map(|(index, sample)| (index, sample_rank(sample)))
            else {
                return;
            };
            if rank < latest_rank {
                samples.remove_sample(latest);
            } else {
                samples.drops.dropped_samples = samples.drops.dropped_samples.saturating_add(1);
                return;
            }
            samples.drops.dropped_samples = samples.drops.dropped_samples.saturating_add(1);
        }
        let motion_start = samples.sample_motion.len();
        copy_family_records(
            &self.motion,
            family.serial,
            &mut samples.sample_motion,
            MAX_SAMPLE_MOTION,
            &mut samples.drops.dropped_motion_segments,
        );
        let ash_start = samples.sample_ash.len();
        copy_family_records(
            &self.ash,
            family.serial,
            &mut samples.sample_ash,
            MAX_SAMPLE_ASH,
            &mut samples.drops.dropped_ash_hits,
        );
        let bite_start = samples.sample_bites.len();
        copy_family_records(
            &self.bites,
            family.serial,
            &mut samples.sample_bites,
            MAX_SAMPLE_BITES,
            &mut samples.drops.dropped_bite_episodes,
        );
        let first_bite_at = family.first_bite.map(|main_counter| {
            relative_time(main_counter, clocks).unwrap_or(TimePoint {
                main_counter,
                wave: None,
                time: None,
            })
        });
        samples.samples.push(SampleHeader {
            trial_sequence: self.trial_sequence,
            seed: self.seed,
            world_epoch: self.world_epoch,
            key,
            parent_id: family.parent_id,
            imp_id,
            thrown_at: relative_time(throw_counter, clocks).unwrap_or(TimePoint {
                main_counter: throw_counter,
                wave: None,
                time: None,
            }),
            first_eligible_at: family
                .first_eligible_counter
                .map(|counter| sampled_time(counter, clocks)),
            first_throwing_at: family
                .first_throwing_counter
                .map(|counter| sampled_time(counter, clocks)),
            failed_at: relative_time(counter, clocks).unwrap_or(TimePoint {
                main_counter: counter,
                wave: None,
                time: None,
            }),
            first_bite_at,
            effective_chew_cs: family.effective_chew_cs,
            imp_x,
            target,
            throw_snapshot: snapshot,
            coincident_imp_ids: coincident,
            coincident_len: u8::try_from(coincident_len).expect("active family bound fits u8"),
            motion: motion_start..samples.sample_motion.len(),
            ash: ash_start..samples.sample_ash.len(),
            bites: bite_start..samples.sample_bites.len(),
            trace_complete,
        });
    }

    fn remove_family(&mut self, index: usize) {
        let serial = self.active[index].serial;
        self.active.remove(index);
        self.motion.retain(|record| record.family != serial);
        self.ash.retain(|record| record.family != serial);
        self.bites.retain(|record| record.family != serial);
        for family in &mut self.active {
            family.motion_index = self.motion.iter().rposition(|record| record.family == family.serial);
            family.bite_index = self.bites.iter().rposition(|record| record.family == family.serial);
        }
    }
}

pub(super) fn bite_target(plant_id: PlantId, raw_kind: PlantKind, effective_kind: PlantKind, grid: Grid) -> BiteTarget {
    BiteTarget {
        plant_id,
        raw_kind,
        effective_kind,
        grid,
    }
}

#[cfg(test)]
fn aggregate_capacity_bytes(aggregate: &ImpLeakAggregate) -> usize {
    aggregate.cohorts.capacity() * size_of::<CohortStats>()
        + aggregate.samples.capacity() * size_of::<SampleHeader>()
        + aggregate.sample_motion.capacity() * size_of::<MotionSegment>()
        + aggregate.sample_ash.capacity() * size_of::<AshHit>()
        + aggregate.sample_bites.capacity() * size_of::<BiteEpisode>()
}

trait FamilyRecord {
    fn family(&self) -> u32;
}

impl FamilyRecord for MotionSegment {
    fn family(&self) -> u32 {
        self.family
    }
}

impl FamilyRecord for AshHit {
    fn family(&self) -> u32 {
        self.family
    }
}

impl FamilyRecord for BiteEpisode {
    fn family(&self) -> u32 {
        self.family
    }
}

fn copy_family_records<T: Copy + FamilyRecord>(
    source: &[T], family: u32, target: &mut Vec<T>, limit: usize, dropped: &mut u64,
) {
    for record in source.iter().filter(|record| record.family() == family) {
        if target.len() == limit {
            *dropped = dropped.saturating_add(1);
        } else {
            target.push(*record);
        }
    }
}

fn relative_time(main_counter: i32, clocks: &WaveClockState) -> Option<TimePoint> {
    let mut best = None;
    for raw_wave in 0..=64 {
        let wave = Wave(raw_wave);
        let Some(refresh) = clocks.refresh_clock(wave) else {
            continue;
        };
        if refresh <= main_counter && best.is_none_or(|(_, best_refresh)| refresh > best_refresh) {
            best = Some((raw_wave, refresh));
        }
    }
    best.map(|(wave, refresh)| TimePoint {
        main_counter,
        wave: Some(wave),
        time: Some(main_counter.saturating_sub(refresh)),
    })
}

fn sampled_time(main_counter: i32, clocks: &WaveClockState) -> TimePoint {
    relative_time(main_counter, clocks).unwrap_or(TimePoint {
        main_counter,
        wave: None,
        time: None,
    })
}

fn effective_frame(frame: Option<ImpFrameState>, target_alive: bool) -> bool {
    target_alive && frame.is_some_and(|frame| frame.is_eating && !frame.frozen && !frame.buttered)
}

fn throw_pending_eligible(has_object: bool, hp: i32, max_hp: i32, x: f32) -> bool {
    has_object && hp < max_hp / 2 && x > GARG_THROW_MIN_X
}

fn crossed_threshold(effective_cs: u32, threshold_cs: u32) -> bool {
    effective_cs > threshold_cs
}

fn report_wave(raw: i32) -> i32 {
    raw.saturating_add(1)
}

fn min_option_i32(left: Option<i32>, right: Option<i32>) -> Option<i32> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn max_option_i32(left: Option<i32>, right: Option<i32>) -> Option<i32> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn remove_range<T>(values: &mut Vec<T>, range: Range<usize>) {
    values.drain(range);
}

fn adjust_range(range: &mut Range<usize>, removed: &Range<usize>) {
    if range.start >= removed.end {
        let len = removed.end - removed.start;
        range.start -= len;
        range.end -= len;
    }
}

fn sample_rank(sample: &SampleHeader) -> (u8, u64) {
    (u8::from(!sample.trace_complete), sample.trial_sequence)
}

fn zombie_name(code: i32) -> String {
    if code == -1 {
        return "other".to_owned();
    }
    match ZombieKind::try_from_code(code) {
        Ok(ZombieKind::Gargantuar) => "gargantuar".to_owned(),
        Ok(ZombieKind::GigaGargantuar) => "giga_gargantuar".to_owned(),
        Ok(kind) => format!("{kind:?}"),
        Err(_) => format!("unknown_{code}"),
    }
}

fn context_names(context: u8) -> Vec<String> {
    let mut names = Vec::new();
    for (bit, name) in [
        (CONTEXT_ELIGIBLE_DURING_SMASH, "eligible_during_smash"),
        (CONTEXT_FROZEN_WHILE_PENDING, "frozen_while_pending"),
        (CONTEXT_BUTTERED_WHILE_PENDING, "buttered_while_pending"),
        (CONTEXT_RETRIED_THROW, "retry_after_interrupted_throw"),
        (CONTEXT_HISTORY_INCOMPLETE, "history_incomplete"),
    ] {
        if context & bit != 0 {
            names.push(name.to_owned());
        }
    }
    names
}

fn time_report(time: TimePoint) -> ImpLeakTimeReport {
    ImpLeakTimeReport {
        main_counter: time.main_counter,
        wave: time.wave,
        time: time.time,
    }
}

fn target_report(target: BiteTarget) -> ImpLeakTargetReport {
    ImpLeakTargetReport {
        plant_id: target.plant_id.raw(),
        raw_kind: target.raw_kind.report_name().to_owned(),
        effective_kind: target.effective_kind.report_name().to_owned(),
        row: target.grid.row + 1,
        col: target.grid.col + 1,
    }
}

fn throw_report(snapshot: ThrowSnapshot) -> ImpLeakThrowSnapshotReport {
    ImpLeakThrowSnapshotReport {
        parent_x: snapshot.parent_x,
        parent_hp: snapshot.parent_hp,
        parent_max_hp: snapshot.parent_max_hp,
        parent_phase: snapshot.parent_phase,
        parent_speed_x: snapshot.parent_speed_x,
        parent_frozen: snapshot.parent_frozen,
        parent_chilled: snapshot.parent_chilled,
        parent_buttered: snapshot.parent_buttered,
    }
}

fn motion_report(segment: &MotionSegment, throw_counter: i32) -> ImpLeakMotionSegmentReport {
    let frames = segment.end.saturating_sub(segment.start).max(1) as f32;
    let phase = ZombiePhase::try_from_code(segment.phase).ok();
    let mut stop_reasons = Vec::new();
    // The initial point has no preceding displacement sample and is not evidence of a stop.
    if !segment.moving && segment.end > segment.start {
        for (present, reason) in [
            (phase == Some(ZombiePhase::GargantuarSmashing), "smashing"),
            (phase == Some(ZombiePhase::GargantuarThrowing), "throwing"),
            (segment.frozen, "frozen"),
            (segment.buttered, "buttered"),
        ] {
            if present {
                stop_reasons.push(reason.to_owned());
            }
        }
        if stop_reasons.is_empty() {
            stop_reasons.push("unknown".to_owned());
        }
    }
    ImpLeakMotionSegmentReport {
        start: segment.start,
        end: segment.end,
        start_relative_to_throw_cs: Some(segment.start.saturating_sub(throw_counter)),
        end_relative_to_throw_cs: Some(segment.end.saturating_sub(throw_counter)),
        start_x: segment.start_x,
        end_x: segment.end_x,
        raw_speed_x: segment.raw_speed_x,
        average_speed_x: (segment.end_x - segment.start_x) / frames,
        phase: segment.phase,
        phase_name: phase.map(|phase| format!("{phase:?}")),
        stop_reasons,
        row: segment.row,
        moving: segment.moving,
        frozen: segment.frozen,
        chilled: segment.chilled,
        buttered: segment.buttered,
    }
}

fn ash_report(hit: &AshHit) -> ImpLeakAshHitReport {
    ImpLeakAshHitReport {
        at: hit.at,
        x: hit.x,
        hp_before: hit.hp_before,
        phase: hit.phase,
        terminal_below_1800: hit.hp_before < 1_800,
        frozen: hit.frozen,
        chilled: hit.chilled,
        buttered: hit.buttered,
    }
}

fn bite_report(episode: &BiteEpisode) -> ImpLeakBiteEpisodeReport {
    ImpLeakBiteEpisodeReport {
        plant_id: episode.target.plant_id.raw(),
        raw_kind: episode.target.raw_kind.report_name().to_owned(),
        effective_kind: episode.target.effective_kind.report_name().to_owned(),
        row: episode.target.grid.row + 1,
        col: episode.target.grid.col + 1,
        start: episode.start,
        last_bite: episode.last_bite,
        effective_cs: episode.effective_cs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thrown_fact(from_wave: i32) -> ImpThrownFact {
        ImpThrownFact {
            parent_id: ZombieId::from_raw(1),
            imp_id: ZombieId::from_raw(2),
            parent_kind: ZombieKind::Gargantuar,
            from_wave,
            row: 0,
            main_counter: 10,
            parent_x: 700.0,
            parent_hp: 1_800,
            parent_max_hp: 3_000,
            parent_phase: ZombiePhase::GargantuarThrowing,
            parent_speed_x: 0.0,
            parent_frozen: 0,
            parent_chilled: 0,
            parent_buttered: 0,
        }
    }

    fn sample_header(trial_sequence: u64) -> SampleHeader {
        let point = TimePoint {
            main_counter: 0,
            wave: None,
            time: None,
        };
        SampleHeader {
            trial_sequence,
            seed: 0,
            world_epoch: 0,
            key: CohortKey {
                parent_kind: ZombieKind::Gargantuar.code(),
                parent_from_wave: trial_sequence as i32,
                throw_wave: None,
                row: 1,
                throw_context: 0,
            },
            parent_id: ZombieId::from_raw(1),
            imp_id: ZombieId::from_raw(2),
            thrown_at: point,
            first_eligible_at: None,
            first_throwing_at: None,
            failed_at: point,
            first_bite_at: None,
            effective_chew_cs: 201,
            imp_x: 0.0,
            target: bite_target(
                PlantId::from_raw(1),
                PlantKind::WallNut,
                PlantKind::WallNut,
                Grid::new(0, 0).expect("grid"),
            ),
            throw_snapshot: ThrowSnapshot {
                parent_x: 0.0,
                parent_hp: 1_800,
                parent_max_hp: 3_000,
                parent_phase: ZombiePhase::GargantuarThrowing.code(),
                parent_speed_x: 0.0,
                parent_frozen: 0,
                parent_chilled: 0,
                parent_buttered: 0,
            },
            coincident_imp_ids: [0; MAX_ACTIVE_FAMILIES],
            coincident_len: 0,
            motion: 0..0,
            ash: 0..0,
            bites: 0..0,
            trace_complete: true,
        }
    }

    #[test]
    fn configured_storage_stays_within_approved_budget() {
        let tracker = ImpLeakTracker::new(200);
        let mut trial = ImpLeakAggregate::default();
        trial.reserve(false);
        let mut partial = ImpLeakAggregate::default();
        partial.reserve(true);
        let bytes = tracker.reserved_bytes(&trial, &partial);
        assert!(bytes <= MEMORY_BUDGET, "reserved {bytes} bytes");
    }

    #[test]
    fn context_names_are_stable_and_normal_is_empty() {
        assert!(context_names(0).is_empty());
        assert_eq!(
            context_names(CONTEXT_FROZEN_WHILE_PENDING | CONTEXT_RETRIED_THROW),
            ["frozen_while_pending", "retry_after_interrupted_throw"]
        );
    }

    #[test]
    fn effective_chewing_counts_slow_time_but_not_freeze_or_butter() {
        let active = |frozen, buttered| {
            Some(ImpFrameState {
                is_eating: true,
                frozen,
                buttered,
            })
        };
        assert!(effective_frame(active(false, false), true));
        assert!(!effective_frame(active(true, false), true));
        assert!(!effective_frame(active(false, true), true));
        assert!(!effective_frame(active(false, false), false));
        assert!(!effective_frame(None, true));
    }

    #[test]
    fn threshold_is_strictly_exceeded_and_zero_is_valid() {
        assert!(!crossed_threshold(200, 200));
        assert!(crossed_threshold(201, 200));
        assert!(!crossed_threshold(0, 0));
        assert!(crossed_threshold(1, 0));
    }

    #[test]
    fn pending_throw_eligibility_matches_the_native_x_boundary() {
        assert!(!throw_pending_eligible(true, 1_499, 3_000, 400.0));
        assert!(throw_pending_eligible(true, 1_499, 3_000, 400.000_03));
        assert!(!throw_pending_eligible(false, 1_499, 3_000, 500.0));
        assert!(!throw_pending_eligible(true, 1_500, 3_000, 500.0));
    }

    #[test]
    fn pending_context_records_smash_controls_and_interrupted_retry() {
        let mut tracker = ImpLeakTracker::new(200);
        tracker.start_trial(0, 0, 1);
        tracker.note_gargantuar_spawned(GargantuarSpawnedFact {
            parent_id: ZombieId::from_raw(1),
            parent_kind: ZombieKind::Gargantuar,
            from_wave: 0,
            row: 0,
            main_counter: 0,
        });
        tracker.record_parent_controls(0, 1, ZombiePhase::GargantuarSmashing, false, false, false, true);
        assert_eq!(tracker.active[0].context, 0);
        assert_eq!(tracker.active[0].first_eligible_counter, None);

        tracker.record_parent_controls(0, 2, ZombiePhase::GargantuarSmashing, true, true, true, true);
        assert_eq!(
            tracker.active[0].context,
            CONTEXT_ELIGIBLE_DURING_SMASH | CONTEXT_FROZEN_WHILE_PENDING | CONTEXT_BUTTERED_WHILE_PENDING
        );

        tracker.record_parent_controls(0, 3, ZombiePhase::GargantuarThrowing, true, true, true, true);
        tracker.record_parent_controls(0, 4, ZombiePhase::ZombieNormal, true, true, true, true);
        tracker.record_parent_controls(0, 5, ZombiePhase::GargantuarThrowing, true, false, false, true);
        tracker.note_imp_thrown(thrown_fact(0));
        assert_ne!(tracker.active[0].context & CONTEXT_RETRIED_THROW, 0);
        assert_eq!(tracker.active[0].first_eligible_counter, Some(2));
        assert_eq!(tracker.active[0].first_throwing_counter, Some(3));
    }

    #[test]
    fn motion_segments_keep_the_boundary_displacement() {
        let mut tracker = ImpLeakTracker::new(200);
        tracker.start_trial(0, 0, 1);
        tracker.note_gargantuar_spawned(GargantuarSpawnedFact {
            parent_id: ZombieId::from_raw(1),
            parent_kind: ZombieKind::Gargantuar,
            from_wave: 0,
            row: 0,
            main_counter: 0,
        });
        tracker.record_motion(0, 10, 700.0, 1.0, ZombiePhase::ZombieNormal, 0, false, false, false);
        tracker.record_motion(
            0,
            11,
            699.0,
            2.0,
            ZombiePhase::GargantuarThrowing,
            0,
            false,
            false,
            false,
        );
        let segment = tracker.motion.last().expect("second segment");
        assert_eq!((segment.start, segment.end), (10, 11));
        assert_eq!((segment.start_x, segment.end_x), (700.0, 699.0));
        assert_eq!(motion_report(segment, 20).average_speed_x, -1.0);
        assert!(motion_report(segment, 20).stop_reasons.is_empty());
    }

    #[test]
    fn failure_report_keeps_stops_before_eligibility_and_throw_timing_after_merge() {
        let mut tracker = ImpLeakTracker::new(200);
        tracker.start_trial(0, 100, 1);
        tracker.note_gargantuar_spawned(GargantuarSpawnedFact {
            parent_id: ZombieId::from_raw(1),
            parent_kind: ZombieKind::Gargantuar,
            from_wave: 0,
            row: 0,
            main_counter: 0,
        });
        for (counter, x, phase, eligible, frozen) in [
            (0, 700.0, ZombiePhase::ZombieNormal, false, false),
            (1, 700.0, ZombiePhase::GargantuarSmashing, false, false),
            (2, 700.0, ZombiePhase::GargantuarSmashing, true, true),
            (3, 699.0, ZombiePhase::ZombieNormal, true, false),
            (4, 699.0, ZombiePhase::GargantuarThrowing, true, false),
            (5, 699.0, ZombiePhase::GargantuarThrowing, true, false),
        ] {
            tracker.record_parent_controls(0, counter, phase, eligible, frozen, false, true);
            tracker.record_motion(0, counter, x, 0.3, phase, 0, frozen, false, false);
        }
        tracker.note_imp_thrown(thrown_fact(0));
        let target = bite_target(
            PlantId::from_raw(3),
            PlantKind::WallNut,
            PlantKind::WallNut,
            Grid::new(0, 0).unwrap(),
        );
        tracker.note_bite(ZombieId::from_raw(2), target, true, 20);
        let mut clocks = WaveClockState::new();
        clocks.record_refresh_clock(Wave(1), 0);
        clocks.record_refresh_clock(Wave(2), 3);
        let mut trial = ImpLeakAggregate::default();
        trial.reserve(false);
        let mut samples = ImpLeakAggregate::default();
        samples.reserve(true);
        tracker.capture_failure(
            ZombieId::from_raw(2),
            200.0,
            221,
            &clocks,
            &mut trial,
            &mut samples,
            [0; MAX_ACTIVE_FAMILIES],
            0,
        );
        let mut merged = ImpLeakAggregate::default();
        merged.reserve(true);
        merged.merge_from(&samples);
        let report = merged.report(true, 200, 1);
        let json = serde_json::to_vec(&report).unwrap();
        let decoded: ImpLeakReport = serde_json::from_slice(&json).unwrap();
        assert_eq!(decoded, report);
        let sample = &decoded.samples[0];
        assert_eq!(sample.first_eligible_at, Some(time_report(sampled_time(2, &clocks))));
        assert_eq!(sample.first_throwing_at, Some(time_report(sampled_time(4, &clocks))));
        assert_eq!(sample.first_eligible_at.unwrap().wave, Some(1));
        assert_eq!(sample.first_throwing_at.unwrap().wave, Some(2));
        assert_eq!(sample.thrown_at.main_counter, 10);
        assert!(sample.motion[0].stop_reasons.is_empty());
        let smash = &sample.motion[1];
        assert_eq!(smash.phase_name.as_deref(), Some("GargantuarSmashing"));
        assert_eq!(smash.stop_reasons, ["smashing"]);
        assert_eq!(
            (smash.start_relative_to_throw_cs, smash.end_relative_to_throw_cs),
            (Some(-10), Some(-9))
        );
        assert_eq!(sample.motion[2].stop_reasons, ["smashing", "frozen"]);
        assert!(sample.motion[3].moving);
        assert_eq!(sample.motion[4].stop_reasons, ["throwing"]);
        assert_eq!(sample.motion[4].end_relative_to_throw_cs, Some(-5));
        tracker.start_trial(1, 101, 2);
        assert!(tracker.active.is_empty());
    }

    #[test]
    fn omitted_motion_and_bite_segments_are_counted_once() {
        let mut tracker = ImpLeakTracker::new(200);
        tracker.start_trial(0, 0, 1);
        tracker.note_gargantuar_spawned(GargantuarSpawnedFact {
            parent_id: ZombieId::from_raw(1),
            parent_kind: ZombieKind::Gargantuar,
            from_wave: 0,
            row: 0,
            main_counter: 0,
        });
        for counter in 0..=MAX_FAMILY_MOTION as i32 {
            tracker.record_motion(
                0,
                counter,
                700.0,
                counter as f32,
                ZombiePhase::ZombieNormal,
                0,
                false,
                false,
                false,
            );
        }
        let dropped_motion = tracker.drops.dropped_motion_segments;
        tracker.record_motion(
            0,
            MAX_FAMILY_MOTION as i32 + 1,
            700.0,
            MAX_FAMILY_MOTION as f32,
            ZombiePhase::ZombieNormal,
            0,
            false,
            false,
            false,
        );
        assert_eq!((dropped_motion, tracker.drops.dropped_motion_segments), (1, 1));

        let imp = ZombieId::from_raw(2);
        tracker.active[0].child_id = Some(imp);
        for index in 0..=MAX_FAMILY_BITES {
            tracker.note_bite(
                imp,
                bite_target(
                    PlantId::from_raw(index as u32 + 1),
                    PlantKind::WallNut,
                    PlantKind::WallNut,
                    Grid::new(0, 0).expect("grid"),
                ),
                true,
                index as i32,
            );
        }
        let dropped_bites = tracker.drops.dropped_bite_episodes;
        let omitted_target = tracker.active[0].bite_target.expect("omitted target");
        tracker.note_bite(imp, omitted_target, true, 100);
        assert_eq!((dropped_bites, tracker.drops.dropped_bite_episodes), (1, 1));
    }

    #[test]
    fn every_same_frame_crossing_is_not_censored() {
        let mut tracker = ImpLeakTracker::new(200);
        tracker.start_trial(0, 0, 1);
        let mut second = thrown_fact(0);
        second.parent_id = ZombieId::from_raw(3);
        second.imp_id = ZombieId::from_raw(4);
        tracker.note_imp_thrown(thrown_fact(0));
        tracker.note_imp_thrown(second);
        let mut third = thrown_fact(0);
        third.parent_id = ZombieId::from_raw(5);
        third.imp_id = ZombieId::from_raw(6);
        tracker.note_imp_thrown(third);
        let mut trial = ImpLeakAggregate::default();
        trial.reserve(false);
        tracker.resolve_pending_cohorts(&WaveClockState::new(), &mut trial);
        tracker.active[0].crossed_at_failure = true;
        tracker.active[1].crossed_at_failure = true;
        tracker.finish_trial(TrialEnd::ImpLeak, &mut trial);
        assert_eq!(trial.cohorts[0].censored_at_stop, 1);
    }

    #[test]
    fn repeated_dispatch_at_one_clock_is_not_a_new_sample() {
        let mut tracker = ImpLeakTracker::new(200);
        tracker.start_trial(0, 0, 1);
        assert!(tracker.begin_sample(10));
        assert!(!tracker.begin_sample(10));
        assert!(tracker.begin_sample(11));
    }

    #[test]
    fn pending_throw_is_exposed_without_a_following_sample() {
        let mut tracker = ImpLeakTracker::new(200);
        tracker.start_trial(0, 0, 1);
        tracker.note_imp_thrown(thrown_fact(0));
        let mut trial = ImpLeakAggregate::default();
        trial.reserve(false);
        tracker.resolve_pending_cohorts(&WaveClockState::new(), &mut trial);
        assert_eq!(trial.cohorts.len(), 1);
        assert_eq!(trial.cohorts[0].thrown_imps, 1);
        assert_eq!(tracker.active[0].cohort.map(|key| key.parent_from_wave), Some(1));
    }

    #[test]
    fn overflow_keeps_the_family_original_cohort_key() {
        let mut trial = ImpLeakAggregate::default();
        trial.reserve(false);
        for wave in 0..255 {
            trial.expose(
                CohortKey {
                    parent_kind: ZombieKind::Gargantuar.code(),
                    parent_from_wave: wave,
                    throw_wave: None,
                    row: 1,
                    throw_context: 0,
                },
                None,
            );
        }
        let mut tracker = ImpLeakTracker::new(200);
        tracker.start_trial(0, 0, 1);
        tracker.note_imp_thrown(thrown_fact(998));
        tracker.resolve_pending_cohorts(&WaveClockState::new(), &mut trial);
        let key = tracker.active[0].cohort.expect("resolved cohort");
        assert_eq!(key.parent_from_wave, 999);
        assert!(trial.cohorts.iter().all(|cohort| cohort.key != key));
        assert!(trial.cohort_mut(OTHER_COHORT).is_some());
    }

    #[test]
    fn changing_protected_targets_does_not_reset_the_imp_total() {
        let mut tracker = ImpLeakTracker::new(200);
        tracker.start_trial(0, 0, 1);
        let parent = ZombieId::from_raw(1);
        let imp = ZombieId::from_raw(2);
        tracker.note_gargantuar_spawned(GargantuarSpawnedFact {
            parent_id: parent,
            parent_kind: ZombieKind::Gargantuar,
            from_wave: 0,
            row: 0,
            main_counter: 0,
        });
        tracker.active[0].child_id = Some(imp);
        tracker.active[0].effective_chew_cs = 137;
        tracker.note_bite(
            imp,
            bite_target(
                PlantId::from_raw(1),
                PlantKind::FlowerPot,
                PlantKind::FlowerPot,
                Grid { row: 0, col: 0 },
            ),
            true,
            10,
        );
        tracker.note_bite(
            imp,
            bite_target(
                PlantId::from_raw(2),
                PlantKind::CobCannon,
                PlantKind::CobCannon,
                Grid { row: 0, col: 1 },
            ),
            true,
            20,
        );
        assert_eq!(tracker.active[0].effective_chew_cs, 137);
        assert_eq!(tracker.bites.len(), 2);
    }

    #[test]
    fn cohort_capacity_keeps_an_explicit_other_bucket() {
        let mut aggregate = ImpLeakAggregate::default();
        aggregate.reserve(false);
        for wave in 0..300 {
            aggregate.expose(
                CohortKey {
                    parent_kind: ZombieKind::Gargantuar.code(),
                    parent_from_wave: wave,
                    throw_wave: Some(wave),
                    row: 1,
                    throw_context: 0,
                },
                Some(wave),
            );
        }
        assert_eq!(aggregate.cohorts.len(), MAX_COHORTS);
        assert!(aggregate.cohort_mut(OTHER_COHORT).is_some());
        assert_eq!(aggregate.drops.cohort_overflows, 1);
        let report = aggregate.report(true, 200, 0);
        assert_eq!(report.trace.cohort_overflows, 45);
        let other = report
            .cohorts
            .iter()
            .find(|cohort| cohort.parent_kind == "other")
            .expect("other cohort");
        assert_eq!((other.exposed_trials, other.first_failure_rate), (None, None));
        let json = serde_json::to_value(other).expect("serialize other cohort");
        assert!(json["exposed_trials"].is_null());
        assert!(json["first_failure_rate"].is_null());

        let mut reverse = ImpLeakAggregate::default();
        reverse.reserve(false);
        for wave in (0..300).rev() {
            reverse.expose(
                CohortKey {
                    parent_kind: ZombieKind::Gargantuar.code(),
                    parent_from_wave: wave,
                    throw_wave: Some(wave),
                    row: 1,
                    throw_context: 0,
                },
                Some(wave),
            );
        }
        aggregate.cohorts.sort_unstable_by_key(|cohort| cohort.key);
        reverse.cohorts.sort_unstable_by_key(|cohort| cohort.key);
        assert_eq!(aggregate, reverse);
    }

    #[test]
    fn capped_cohort_merge_is_order_and_tree_independent() {
        fn aggregate(range: std::ops::Range<i32>) -> ImpLeakAggregate {
            let mut aggregate = ImpLeakAggregate::default();
            aggregate.reserve(false);
            for wave in range {
                aggregate.expose(
                    CohortKey {
                        parent_kind: ZombieKind::Gargantuar.code(),
                        parent_from_wave: wave,
                        throw_wave: Some(wave),
                        row: 1,
                        throw_context: 0,
                    },
                    Some(wave),
                );
            }
            aggregate
        }

        let a = aggregate(100..300);
        let b = aggregate(0..200);
        let c = aggregate(200..350);

        let mut forward = ImpLeakAggregate::default();
        forward.reserve(false);
        forward.merge_from(&a);
        forward.merge_from(&b);

        let mut reverse = ImpLeakAggregate::default();
        reverse.reserve(false);
        reverse.merge_from(&b);
        reverse.merge_from(&a);
        assert_eq!(forward, reverse);

        let mut left = forward;
        left.merge_from(&c);
        let mut right_source = ImpLeakAggregate::default();
        right_source.reserve(false);
        right_source.merge_from(&b);
        right_source.merge_from(&c);
        let mut right = ImpLeakAggregate::default();
        right.reserve(false);
        right.merge_from(&a);
        right.merge_from(&right_source);
        assert_eq!(left, right);
        assert_eq!(left.cohorts.len(), MAX_COHORTS);
        assert_eq!(
            left.cohorts
                .iter()
                .filter(|cohort| cohort.key != OTHER_COHORT)
                .map(|cohort| cohort.key.parent_from_wave)
                .max(),
            Some(254)
        );
    }

    #[test]
    fn capped_sample_merge_records_replacement_commutatively() {
        let mut later = ImpLeakAggregate::default();
        later.reserve(true);
        later.samples.extend((100..164).map(sample_header));
        let mut earlier = ImpLeakAggregate::default();
        earlier.reserve(true);
        earlier.samples.push(sample_header(0));

        let mut forward = later.clone();
        forward.merge_from(&earlier);
        let mut reverse = earlier;
        reverse.merge_from(&later);
        assert_eq!(forward, reverse);
        assert_eq!(forward.drops.dropped_samples, 1);
        assert_eq!(forward.samples.first().map(|sample| sample.trial_sequence), Some(0));
        assert_eq!(forward.samples.last().map(|sample| sample.trial_sequence), Some(162));
    }

    #[test]
    fn complete_sample_replaces_an_earlier_incomplete_sample() {
        let mut incomplete = ImpLeakAggregate::default();
        incomplete.reserve(true);
        let mut first = sample_header(0);
        first.trace_complete = false;
        incomplete.samples.push(first);

        let mut complete = ImpLeakAggregate::default();
        complete.reserve(true);
        let mut later = sample_header(10);
        later.key = incomplete.samples[0].key;
        complete.samples.push(later);

        let mut forward = incomplete.clone();
        forward.merge_from(&complete);
        let mut reverse = complete;
        reverse.merge_from(&incomplete);
        assert_eq!(forward, reverse);
        assert_eq!(forward.samples.len(), 1);
        assert!(forward.samples[0].trace_complete);
        assert_eq!(forward.samples[0].trial_sequence, 10);
    }
}
