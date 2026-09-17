use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::measure::MeasurementWindowEndReport;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EventMeasureReport {
    DamageNarrow(DamageNarrowReport),
    BroadPass(BroadPassReport),
    Smash(SmashReport),
    Pogo(PogoReport),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommonCounts {
    pub mode: String,
    pub requested_trials: Option<u64>,
    pub attempted_trials: u64,
    pub valid_trials: u64,
    pub invalid_trials: u64,
    pub aborted_unrun_trials: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_end: Option<MeasurementWindowEndReport>,
}

impl CommonCounts {
    pub fn validate(&self) -> Result<(), EventReportValidationError> {
        if self.valid_trials.checked_add(self.invalid_trials) != Some(self.attempted_trials) {
            return Err(EventReportValidationError::AttemptedBreakdown);
        }
        match self.requested_trials {
            Some(requested) if self.attempted_trials.checked_add(self.aborted_unrun_trials) != Some(requested) => {
                Err(EventReportValidationError::RequestedBreakdown)
            }
            None if self.aborted_unrun_trials != 0 => Err(EventReportValidationError::DurationHasUnrun),
            Some(_) | None => Ok(()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EventReportValidationError {
    #[error("valid and invalid trials do not equal attempted trials")]
    AttemptedBreakdown,
    #[error("attempted and aborted-unrun trials do not equal requested trials")]
    RequestedBreakdown,
    #[error("duration-limited reports cannot contain aborted-unrun trials")]
    DurationHasUnrun,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PassStats {
    pub success: u64,
    pub failure: u64,
    pub rate: f64,
}

impl PassStats {
    pub(super) fn new(success: u64, failure: u64) -> Self {
        Self {
            success,
            failure,
            rate: ratio_or_zero(success, success.saturating_add(failure)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DamageNarrowReport {
    #[serde(flatten)]
    pub common: CommonCounts,
    pub narrow_pass: PassStats,
    pub damage: DamageStats,
    pub failures: NarrowFailures,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imp_leak: Option<ImpLeakReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vehicle_crush_sample: Option<PlantLossSampleReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imitator_ice_loss_sample: Option<PlantLossSampleReport>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlantLossSampleReport {
    pub trial_sequence: u64,
    pub seed: u32,
    pub world_epoch: u64,
    pub main_counter: i32,
    pub wave: Option<i32>,
    pub time: Option<i32>,
    pub plant_id: u32,
    pub raw_kind: String,
    pub effective_kind: String,
    pub row: i32,
    pub col: i32,
    pub source: String,
    pub actor_id: u32,
    pub outcome: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DamageStats {
    pub total: i64,
    pub by_source: BTreeMap<String, i64>,
    pub by_plant: BTreeMap<String, i64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NarrowFailures {
    pub garg_smash: u64,
    pub home_total: u64,
    pub pogo_home: u64,
    pub refresh: u64,
    #[serde(default)]
    pub imp_leak: u64,
    #[serde(default)]
    pub vehicle_crush: u64,
    #[serde(default)]
    pub imitator_ice_loss: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImpLeakReport {
    pub enabled: bool,
    pub threshold_cs: u32,
    pub timing_error_bound_cs: u32,
    pub failure_trials: u64,
    pub cohorts: Vec<ImpLeakCohortReport>,
    pub samples: Vec<ImpLeakFailureSampleReport>,
    pub trace: ImpLeakTraceStatusReport,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImpLeakCohortReport {
    pub parent_kind: String,
    pub parent_from_wave: i32,
    pub throw_wave: Option<i32>,
    pub row: i32,
    pub throw_context: Vec<String>,
    pub exposed_trials: Option<u64>,
    pub first_failure_trials: u64,
    pub first_failure_rate: Option<f64>,
    pub thrown_imps: u64,
    pub safe_deaths: u64,
    pub censored_at_stop: u64,
    pub throw_time_min: Option<i32>,
    pub throw_time_max: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImpLeakFailureSampleReport {
    pub trial_sequence: u64,
    pub seed: u32,
    pub world_epoch: u64,
    pub parent_id: u32,
    pub imp_id: u32,
    pub imp_pool_order: String,
    pub parent_kind: String,
    pub parent_from_wave: i32,
    pub row: i32,
    pub throw_context: Vec<String>,
    pub thrown_at: ImpLeakTimeReport,
    /// First sampled pending eligibility (object, HP and x), even while smashing or immobilized.
    #[serde(default)]
    pub first_eligible_at: Option<ImpLeakTimeReport>,
    /// First sampled throwing phase; `thrown_at` is the actual child creation event.
    #[serde(default)]
    pub first_throwing_at: Option<ImpLeakTimeReport>,
    pub failed_at: ImpLeakTimeReport,
    pub first_bite_at: Option<ImpLeakTimeReport>,
    pub effective_chew_cs: u32,
    pub imp_x_at_failure: f32,
    pub target: ImpLeakTargetReport,
    pub throw_snapshot: ImpLeakThrowSnapshotReport,
    pub motion: Vec<ImpLeakMotionSegmentReport>,
    pub ash_hits: Vec<ImpLeakAshHitReport>,
    pub bite_episodes: Vec<ImpLeakBiteEpisodeReport>,
    pub coincident_imp_ids: Vec<u32>,
    pub trace_complete: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpLeakTimeReport {
    pub main_counter: i32,
    pub wave: Option<i32>,
    pub time: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpLeakTargetReport {
    pub plant_id: u32,
    pub raw_kind: String,
    pub effective_kind: String,
    pub row: i32,
    pub col: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImpLeakThrowSnapshotReport {
    pub parent_x: f32,
    pub parent_hp: i32,
    pub parent_max_hp: i32,
    pub parent_phase: i32,
    pub parent_speed_x: f32,
    pub parent_frozen: i32,
    pub parent_chilled: i32,
    pub parent_buttered: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImpLeakMotionSegmentReport {
    pub start: i32,
    pub end: i32,
    /// Signed offsets from the actual imp release, including across wave boundaries.
    #[serde(default)]
    pub start_relative_to_throw_cs: Option<i32>,
    #[serde(default)]
    pub end_relative_to_throw_cs: Option<i32>,
    pub start_x: f32,
    pub end_x: f32,
    pub raw_speed_x: f32,
    pub average_speed_x: f32,
    pub phase: i32,
    #[serde(default)]
    pub phase_name: Option<String>,
    /// Observed stopping conditions; multiple simultaneous conditions are retained.
    #[serde(default)]
    pub stop_reasons: Vec<String>,
    pub row: i32,
    pub moving: bool,
    pub frozen: bool,
    pub chilled: bool,
    pub buttered: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ImpLeakAshHitReport {
    pub at: i32,
    pub x: f32,
    pub hp_before: i32,
    pub phase: i32,
    pub terminal_below_1800: bool,
    pub frozen: bool,
    pub chilled: bool,
    pub buttered: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpLeakBiteEpisodeReport {
    pub plant_id: u32,
    pub raw_kind: String,
    pub effective_kind: String,
    pub row: i32,
    pub col: i32,
    pub start: i32,
    pub last_bite: i32,
    pub effective_cs: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpLeakTraceStatusReport {
    pub complete: bool,
    pub active_family_overflows: u64,
    pub cohort_overflows: u64,
    pub dropped_motion_segments: u64,
    pub dropped_ash_hits: u64,
    pub dropped_bite_episodes: u64,
    pub dropped_samples: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BroadPassReport {
    #[serde(flatten)]
    pub common: CommonCounts,
    pub broad_pass: PassStats,
    pub home: HomeStats,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomeStats {
    pub entries: u64,
    pub by_row: BTreeMap<String, u64>,
    pub first_entry_time: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SmashReport {
    #[serde(flatten)]
    pub common: CommonCounts,
    pub smash: EventDistribution,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EventDistribution {
    pub trials: u64,
    pub events: u64,
    pub probability: f64,
    pub by_grid: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PogoReport {
    #[serde(flatten)]
    pub common: CommonCounts,
    pub pogo_home: PogoStats,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PogoStats {
    pub trials: u64,
    pub entries: u64,
    pub probability: f64,
    pub by_row: BTreeMap<String, u64>,
    pub first_entry_time: Option<u64>,
}

pub(super) fn ratio_or_zero(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}
