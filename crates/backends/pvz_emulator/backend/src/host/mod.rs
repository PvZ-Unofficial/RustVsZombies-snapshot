mod output;
mod profile;
pub mod wire;
mod worker;
mod world;

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use rsvz_backend_api::artifact::SessionArtifact;

use crate::DispatchEntry;

pub fn run(dispatch: DispatchEntry) -> ExitCode {
    match run_inner(dispatch) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run_inner(dispatch: DispatchEntry) -> Result<(), String> {
    let started = Instant::now();
    let path = env::var_os(wire::CONFIG_PATH_ENV)
        .map(PathBuf::from)
        .ok_or_else(|| format!("{} is not configured", wire::CONFIG_PATH_ENV))?;
    let raw = fs::read_to_string(&path).map_err(|error| format!("read {} failed: {error}", path.display()))?;
    let config: wire::PeRunConfig =
        serde_json::from_str(&raw).map_err(|error| format!("parse PE config failed: {error}"))?;
    config.validate(None)?;
    fs::remove_file(&path).map_err(|error| format!("remove consumed {} failed: {error}", path.display()))?;

    let outputs = worker::run_all(&config, dispatch);
    let mut workers = Vec::with_capacity(outputs.len());
    let mut errors = Vec::new();
    let mut artifacts = Vec::new();
    for output in outputs {
        workers.push(output.result);
        if let Some(error) = output.error {
            errors.push(error);
        }
        if let Some(artifact) = output.artifact {
            artifacts.push(artifact);
        }
    }

    if errors.is_empty()
        && let Some(path) = config.output_path.as_deref()
        && let Err(error) = merge_artifacts(artifacts).and_then(|artifact| {
            if let Some(artifact) = artifact {
                output::write_artifact(path, &config.run_id, &artifact)?;
            }
            Ok(())
        })
    {
        errors.push(error);
    }
    let terminal = if errors.is_empty() {
        wire::Terminal::Success {}
    } else {
        wire::Terminal::Failure {
            message: errors.join("; "),
        }
    };
    let result = wire::PeRunResult {
        schema_version: wire::SCHEMA_VERSION,
        run_id: config.run_id.clone(),
        terminal,
        parent_wall_ns: started.elapsed().as_nanos(),
        workers,
    };
    result.validate_for(&config)?;
    output::write_control(&config.raw_result_path, &config.run_id, &result)?;

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn merge_artifacts(artifacts: impl IntoIterator<Item = SessionArtifact>) -> Result<Option<SessionArtifact>, String> {
    let mut artifacts = artifacts.into_iter();
    let Some(mut merged) = artifacts.next() else {
        return Ok(None);
    };
    for artifact in artifacts {
        merged.merge_from(artifact).map_err(|error| error.to_string())?;
    }
    Ok(Some(merged))
}
