pub(crate) mod lifecycle;

mod output;
mod readiness;
pub mod wire;

use std::ffi::c_void;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use rsvz_backend_api::error::RuntimeError;

use crate::runtime::hook;
use crate::{DispatchEntry, DispatchInput, DispatchResult};

const CONFIG_FILE_PREFIX: &str = "rsvz-1051-script-";

static CONFIG: OnceLock<wire::RunConfig> = OnceLock::new();
static FINISHED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeUpdate {
    Continue,
    Skip,
}

pub fn initialize(_context: *mut c_void, dispatch: DispatchEntry) -> u32 {
    match panic::catch_unwind(AssertUnwindSafe(|| initialize_inner(dispatch))) {
        Ok(Ok(())) => 1,
        Ok(Err(_)) | Err(_) => {
            hook::mark_unowned_initialization_failure();
            0
        }
    }
}

fn initialize_inner(dispatch: DispatchEntry) -> Result<(), String> {
    let config = load_config()?;
    CONFIG
        .set(config)
        .map_err(|_config| "1051 host config is already installed".to_owned())?;
    hook::install_dispatch_entry(dispatch).map_err(|error| error.to_string())?;
    hook::initialize().map_err(|error| error.to_string())
}

pub fn request_unload(_context: *mut c_void) -> u32 {
    u32::from(panic::catch_unwind(hook::request_unload_and_wait).unwrap_or(false))
}

pub(crate) fn dispatch_frame() -> NativeUpdate {
    panic::catch_unwind(AssertUnwindSafe(dispatch_frame_inner)).unwrap_or_else(|_panic| {
        finish(
            None,
            Some(RuntimeError::new("1051 backend host dispatch panicked")),
            false,
        )
    })
}

fn dispatch_frame_inner() -> NativeUpdate {
    let stop_requested = hook::unload_requested();
    let result = crate::runtime::hook::dispatch_entry::with_installed(|backend, dispatch| {
        let opening_ready = match readiness::before_dispatch(backend, stop_requested) {
            Ok(readiness::BeforeDispatch::AdvanceNative) => return DispatchResult::Continue,
            Ok(readiness::BeforeDispatch::Dispatch { opening_ready }) => opening_ready,
            Err(error) => {
                return DispatchResult::Stop {
                    artifact: None,
                    error: Some(RuntimeError::new(error.to_string())),
                };
            }
        };
        let result = dispatch(
            backend,
            DispatchInput {
                stop_requested,
                completed_rounds: readiness::take_completed_rounds(),
                opening_ready,
            },
        );
        if matches!(result, DispatchResult::SkipUpdate)
            && let Err(error) = readiness::after_dispatch(backend)
        {
            return DispatchResult::Stop {
                artifact: None,
                error: Some(RuntimeError::new(error.to_string())),
            };
        }
        result
    })
    .unwrap_or(DispatchResult::Continue);

    match result {
        DispatchResult::Continue => NativeUpdate::Continue,
        DispatchResult::SkipUpdate => NativeUpdate::Skip,
        DispatchResult::Stop { artifact, error } => finish(artifact, error, stop_requested),
    }
}

fn finish(
    artifact: Option<rsvz_backend_api::artifact::SessionArtifact>, error: Option<RuntimeError>,
    externally_stopped: bool,
) -> NativeUpdate {
    if FINISHED.swap(true, Ordering::AcqRel) {
        return NativeUpdate::Skip;
    }
    let config = config();
    let mut failure = error.map(|error| error.to_string());
    if externally_stopped && failure.is_none() {
        failure = Some("runner unloaded before script completion".to_owned());
    }
    if let Err(error) = crate::impls::reset::finish_session()
        && failure.is_none()
    {
        failure = Some(format!("restore 1051 reset session failed: {error}"));
    }
    readiness::clear();
    if let Err(error) = hook::prepare_runner_cleanup()
        && failure.is_none()
    {
        failure = Some(format!("prepare 1051 cleanup failed: {error}"));
    }
    if failure.is_none()
        && let (Some(path), Some(artifact)) = (config.output_path.as_deref(), artifact.as_ref())
        && let Err(error) = output::write_artifact(path, &config.run_id, artifact)
    {
        failure = Some(format!("write 1051 artifact failed: {error}"));
    }
    let result = wire::RunResult {
        schema_version: wire::SCHEMA_VERSION,
        run_id: config.run_id.clone(),
        terminal: failure
            .as_ref()
            .map_or(wire::Terminal::Success {}, |message| wire::Terminal::Failure {
                message: message.clone(),
            }),
    };
    if let Some(message) = failure.as_deref() {
        hook::report_runtime_error(message);
    }
    if let Err(error) = result
        .validate(Some(&config.run_id))
        .and_then(|()| output::write_control(&config.control_result_path, &config.run_id, &result))
    {
        hook::report_runtime_error(&format!("write 1051 control result failed: {error}"));
    }

    hook::mark_runner_cleanup_complete();
    hook::request_unload();
    NativeUpdate::Skip
}

fn load_config() -> Result<wire::RunConfig, String> {
    let path = std::env::temp_dir().join(format!("{CONFIG_FILE_PREFIX}{}.json", std::process::id()));
    let text =
        fs::read_to_string(&path).map_err(|error| format!("read host config {} failed: {error}", path.display()))?;
    let config: wire::RunConfig =
        serde_json::from_str(&text).map_err(|error| format!("parse 1051 host config failed: {error}"))?;
    config.validate(None)?;
    fs::remove_file(&path).map_err(|error| format!("remove consumed config {} failed: {error}", path.display()))?;
    Ok(config)
}

fn config() -> &'static wire::RunConfig {
    CONFIG
        .get()
        .expect("1051 host config must be installed before dispatch")
}
