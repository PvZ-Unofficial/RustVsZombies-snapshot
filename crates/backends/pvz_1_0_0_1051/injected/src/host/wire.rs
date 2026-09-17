//! Versioned, backend-local 1051 host control schema.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 3;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunConfig {
    pub schema_version: u32,
    pub run_id: String,
    pub output_path: Option<PathBuf>,
    pub control_result_path: PathBuf,
}

impl RunConfig {
    pub fn validate(&self, expected_run_id: Option<&str>) -> Result<(), String> {
        validate_root(self.schema_version, &self.run_id, expected_run_id)?;
        if self.control_result_path.as_os_str().is_empty() {
            return Err("1051 control result path must not be empty".to_owned());
        }
        if self
            .output_path
            .as_ref()
            .is_some_and(|path| path.as_os_str().is_empty() || path == &self.control_result_path)
        {
            return Err("1051 artifact output path must be nonempty and distinct".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Terminal {
    Success {},
    Failure { message: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunResult {
    pub schema_version: u32,
    pub run_id: String,
    pub terminal: Terminal,
}

impl RunResult {
    pub fn validate(&self, expected_run_id: Option<&str>) -> Result<(), String> {
        validate_root(self.schema_version, &self.run_id, expected_run_id)
    }
}

fn validate_root(schema_version: u32, run_id: &str, expected_run_id: Option<&str>) -> Result<(), String> {
    if schema_version != SCHEMA_VERSION {
        return Err(format!(
            "unsupported 1051 wire schema version {schema_version}; expected {SCHEMA_VERSION}"
        ));
    }
    if run_id.is_empty() {
        return Err("1051 wire run_id must not be empty".to_owned());
    }
    if let Some(expected) = expected_run_id
        && run_id != expected
    {
        return Err(format!("stale 1051 wire run_id {run_id:?}; expected {expected:?}"));
    }
    Ok(())
}
