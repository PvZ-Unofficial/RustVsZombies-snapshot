//! Versioned PE host control schema. User artifacts use their own root JSON.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 2;
pub const CONFIG_PATH_ENV: &str = "RSVZ_PE_RUN_CONFIG_PATH";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SeedMode {
    Fixed { seed: u32 },
    Base { seed: u32 },
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunLimits {
    pub max_wall_ms: Option<u64>,
    pub max_sim_frames: Option<u64>,
    pub max_levels: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeRunConfig {
    pub schema_version: u32,
    pub run_id: String,
    pub threads: usize,
    pub seed_mode: SeedMode,
    pub limits: RunLimits,
    pub profile: bool,
    pub profile_detail: bool,
    /// Freeze coarse performance counters at this elapsed time, without stopping simulation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub performance_window_ms: Option<u64>,
    pub output_path: Option<PathBuf>,
    pub raw_result_path: PathBuf,
}

impl PeRunConfig {
    pub fn validate(&self, expected_run_id: Option<&str>) -> Result<(), String> {
        validate_root(self.schema_version, &self.run_id, expected_run_id)?;
        if self.threads == 0 {
            return Err("PE runner threads must be greater than zero".to_owned());
        }
        if self.performance_window_ms == Some(0) {
            return Err("PE performance window must be greater than zero".to_owned());
        }
        if self.raw_result_path.as_os_str().is_empty() {
            return Err("PE raw result path must not be empty".to_owned());
        }
        if self
            .output_path
            .as_ref()
            .is_some_and(|path| path.as_os_str().is_empty() || path == &self.raw_result_path)
        {
            return Err("PE artifact output path must be nonempty and distinct".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StopReason {
    Script {},
    Failed {},
    GameOver {},
    MaxWall {},
    MaxSimFrames {},
    MaxLevels {},
    Cancelled {},
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Terminal {
    Success {},
    Failure { message: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileStage {
    pub name: String,
    pub count: u64,
    pub total_ns: u128,
    pub min_ns: u128,
    pub max_ns: u128,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawProfile {
    pub frame_stages: Vec<ProfileStage>,
    pub detailed_stages: Vec<ProfileStage>,
    pub detailed_counters: Vec<ProfileCounter>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileCounter {
    pub name: String,
    pub value: u128,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkerResult {
    pub worker_index: usize,
    pub seed: u32,
    pub stop_reason: StopReason,
    pub wall_ns: u128,
    pub frames: u64,
    pub completed_levels: u64,
    pub partial_frames: u64,
    pub profile: Option<RawProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub performance_window: Option<PerformanceWindow>,
}

/// One immutable prefix of a worker run; tail simulation never changes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerformanceWindow {
    pub worker_index: usize,
    pub requested_ms: u64,
    pub wall_ns: u128,
    pub frames: u64,
    pub completed_levels: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PeRunResult {
    pub schema_version: u32,
    pub run_id: String,
    pub terminal: Terminal,
    pub parent_wall_ns: u128,
    pub workers: Vec<WorkerResult>,
}

impl PeRunResult {
    pub fn validate(&self, expected_run_id: Option<&str>) -> Result<(), String> {
        validate_root(self.schema_version, &self.run_id, expected_run_id)?;
        for worker in &self.workers {
            if let Some(window) = worker.performance_window
                && (window.worker_index != worker.worker_index
                    || window.requested_ms == 0
                    || window.wall_ns < u128::from(window.requested_ms) * 1_000_000
                    || window.wall_ns > worker.wall_ns
                    || window.frames > worker.frames
                    || window.completed_levels > worker.completed_levels)
            {
                return Err("PE performance window is not a valid worker prefix".to_owned());
            }
        }
        if matches!(self.terminal, Terminal::Success {}) && self.workers.is_empty() {
            return Err("successful PE result must contain at least one worker".to_owned());
        }
        if self
            .workers
            .windows(2)
            .any(|pair| pair[0].worker_index >= pair[1].worker_index)
        {
            return Err("PE worker results must be strictly ordered by worker index".to_owned());
        }
        Ok(())
    }

    pub fn validate_for(&self, config: &PeRunConfig) -> Result<(), String> {
        self.validate(Some(&config.run_id))?;
        if matches!(self.terminal, Terminal::Success {})
            && (self.workers.len() != config.threads
                || self
                    .workers
                    .iter()
                    .enumerate()
                    .any(|(index, worker)| worker.worker_index != index))
        {
            return Err(format!(
                "successful PE result must cover worker indices 0..{}, received {:?}",
                config.threads,
                self.workers
                    .iter()
                    .map(|worker| worker.worker_index)
                    .collect::<Vec<_>>()
            ));
        }
        Ok(())
    }
}

fn validate_root(schema_version: u32, run_id: &str, expected_run_id: Option<&str>) -> Result<(), String> {
    if schema_version != SCHEMA_VERSION {
        return Err(format!(
            "unsupported PE wire schema version {schema_version}; expected {SCHEMA_VERSION}"
        ));
    }
    if run_id.is_empty() {
        return Err("PE wire run_id must not be empty".to_owned());
    }
    if let Some(expected) = expected_run_id
        && run_id != expected
    {
        return Err(format!("stale PE wire run_id {run_id:?}; expected {expected:?}"));
    }
    Ok(())
}
