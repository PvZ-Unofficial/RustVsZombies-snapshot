use std::collections::BTreeMap;

use rsvz_model::{MeasureMode, RelativeTime};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshReport {
    pub mode: String,
    pub requested_trials: Option<u64>,
    pub attempted_trials: u64,
    pub valid_trials: u64,
    pub invalid_trials: u64,
    pub aborted_unrun_trials: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window_end: Option<MeasurementWindowEndReport>,
    pub config: RefreshConfigReport,
    pub refresh: RefreshStats,
    pub samples: RefreshSampleStats,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeasurementWindowEndReport {
    pub wave: i32,
    pub time: i32,
}

impl From<RelativeTime> for MeasurementWindowEndReport {
    fn from(value: RelativeTime) -> Self {
        Self {
            wave: value.wave.0,
            time: value.time,
        }
    }
}

impl RefreshReport {
    pub fn validate(&self) -> Result<(), RefreshReportValidationError> {
        if self.mode != MeasureMode::Refresh.as_str() {
            return Err(RefreshReportValidationError::WrongMode(self.mode.clone()));
        }
        if self.valid_trials.checked_add(self.invalid_trials) != Some(self.attempted_trials) {
            return Err(RefreshReportValidationError::AttemptedBreakdown {
                attempted_trials: self.attempted_trials,
                valid_trials: self.valid_trials,
                invalid_trials: self.invalid_trials,
            });
        }
        if self.refresh.satisfied.checked_add(self.refresh.failed) != Some(self.valid_trials) {
            return Err(RefreshReportValidationError::ValidBreakdown {
                valid_trials: self.valid_trials,
                satisfied_trials: self.refresh.satisfied,
                failed_trials: self.refresh.failed,
            });
        }
        match self.requested_trials {
            Some(requested_trials)
                if self.attempted_trials.checked_add(self.aborted_unrun_trials) != Some(requested_trials) =>
            {
                Err(RefreshReportValidationError::RequestedBreakdown {
                    requested_trials,
                    attempted_trials: self.attempted_trials,
                    aborted_unrun_trials: self.aborted_unrun_trials,
                })
            }
            None if self.aborted_unrun_trials != 0 => Err(RefreshReportValidationError::DurationHasUnrun {
                aborted_unrun_trials: self.aborted_unrun_trials,
            }),
            Some(_) | None => Ok(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RefreshReportValidationError {
    #[error("measurement report mode {0:?} is not refresh")]
    WrongMode(String),
    #[error(
        "measurement report attempted count {attempted_trials} does not equal valid {valid_trials} plus invalid {invalid_trials}"
    )]
    AttemptedBreakdown {
        attempted_trials: u64,
        valid_trials: u64,
        invalid_trials: u64,
    },
    #[error(
        "measurement report valid count {valid_trials} does not equal satisfied {satisfied_trials} plus failed {failed_trials}"
    )]
    ValidBreakdown {
        valid_trials: u64,
        satisfied_trials: u64,
        failed_trials: u64,
    },
    #[error(
        "measurement report requested count {requested_trials} does not equal attempted {attempted_trials} plus aborted-unrun {aborted_unrun_trials}"
    )]
    RequestedBreakdown {
        requested_trials: u64,
        attempted_trials: u64,
        aborted_unrun_trials: u64,
    },
    #[error("duration measurement report cannot contain {aborted_unrun_trials} aborted-unrun trials")]
    DurationHasUnrun { aborted_unrun_trials: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshConfigReport {
    pub activate: bool,
    pub dance: String,
    pub cob_delay: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshStats {
    pub satisfied: u64,
    pub failed: u64,
    pub rate: f64,
    pub reasons: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshSampleStats {
    pub count: u64,
    pub average_hp_ratio: f64,
    pub average_refresh_probability: f64,
    pub average_accident_rate: f64,
    pub by_wave: BTreeMap<String, RefreshWaveStats>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RefreshWaveStats {
    pub count: u64,
    pub average_hp_ratio: f64,
    pub average_refresh_probability: f64,
    pub average_accident_rate: f64,
}
