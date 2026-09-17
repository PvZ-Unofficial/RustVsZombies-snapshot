mod output;
mod readiness;
pub mod wire;

use std::cell::Cell;
use std::cell::RefCell;
#[cfg(test)]
use std::fs;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::panic::{self, AssertUnwindSafe};
use std::time::Instant;

use rsvz_backend_api::artifact::SessionArtifact;
#[cfg(test)]
use windows_sys::Win32::Foundation::TRUE;
use windows_sys::Win32::Foundation::{FALSE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
#[cfg(test)]
use windows_sys::Win32::System::Threading::{CreateEventW, SetEvent};
use windows_sys::Win32::System::Threading::{EVENT_ALL_ACCESS, OpenEventW, WaitForSingleObject};

use crate::{DispatchEntry, DispatchInput, DispatchResult, PortableBackend};

struct HostState {
    config: wire::PortableRunConfig,
    dispatch: DispatchEntry,
    backend: PortableBackend,
    started: Instant,
    frames: u64,
    completed_rounds: u64,
    stop_event: Option<OwnedHandle>,
    active: bool,
    dispatch_started: bool,
    cleanup_complete: bool,
}

thread_local! {
    static HOST: RefCell<Option<HostState>> = const { RefCell::new(None) };
    static NATIVE_EVENT_FAILED: Cell<bool> = const { Cell::new(false) };
    #[cfg(test)]
    static TEST_LAYOUT_VALID: Cell<bool> = const { Cell::new(true) };
}

pub fn initialize(config_json: &[u8], dispatch: DispatchEntry) -> Result<(), String> {
    let config: wire::PortableRunConfig =
        serde_json::from_slice(config_json).map_err(|error| format!("parse Portable config failed: {error}"))?;
    config.validate()?;
    if let Err(error) = verify_native_layout() {
        return Err(publish_initialization_failure(&config, error));
    }
    let stop_event = open_stop_event(config.stop_event_name.as_deref())
        .map_err(|error| publish_initialization_failure(&config, error))?;
    HOST.with_borrow_mut(|slot| {
        if slot.is_some() {
            return Err("Portable runtime is already initialized".to_owned());
        }
        *slot = Some(HostState {
            config,
            dispatch,
            backend: PortableBackend::new(),
            started: Instant::now(),
            frames: 0,
            completed_rounds: 0,
            stop_event,
            active: true,
            dispatch_started: false,
            cleanup_complete: false,
        });
        NATIVE_EVENT_FAILED.set(false);
        Ok(())
    })
}

fn open_stop_event(name: Option<&str>) -> Result<Option<OwnedHandle>, String> {
    let Some(name) = name else {
        return Ok(None);
    };
    let name: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `name` is NUL-terminated and remains alive for the synchronous call.
    let handle = unsafe { OpenEventW(EVENT_ALL_ACCESS, FALSE, name.as_ptr()) };
    if handle.is_null() {
        return Err(format!(
            "open Portable stop event failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    // SAFETY: `OpenEventW` returned a new owned handle.
    Ok(Some(unsafe { OwnedHandle::from_raw_handle(handle) }))
}

fn stop_event_signaled(event: Option<&OwnedHandle>) -> Result<bool, String> {
    let Some(event) = event else {
        return Ok(false);
    };
    // SAFETY: `event` owns a live waitable event handle for the duration of this call.
    match unsafe { WaitForSingleObject(event.as_raw_handle(), 0) } {
        WAIT_OBJECT_0 => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        WAIT_FAILED => Err(format!(
            "wait for Portable stop event failed: {}",
            std::io::Error::last_os_error()
        )),
        status => Err(format!("wait for Portable stop event returned {status:#x}")),
    }
}

fn publish_initialization_failure(config: &wire::PortableRunConfig, error: String) -> String {
    let result = wire::PortableRunResult {
        schema_version: wire::SCHEMA_VERSION,
        run_id: config.run_id.clone(),
        backend: wire::BackendKind::PvzPortable {},
        terminal: wire::Terminal::Failure { message: error.clone() },
        wall_ns: 0,
        frames: 0,
        completed_rounds: 0,
    };
    if let Err(publish_error) = output::write_control(&config.report_path, &config.run_id, &result) {
        return format!("{error}; failed to publish initialization failure: {publish_error}");
    }
    error
}

#[cfg(not(test))]
fn verify_native_layout() -> Result<(), String> {
    pvzp_rs::verify_layout().map_err(|error| format!("verify Portable ABI failed: {error}"))
}

#[cfg(test)]
fn verify_native_layout() -> Result<(), String> {
    TEST_LAYOUT_VALID.with(|valid| {
        valid
            .get()
            .then_some(())
            .ok_or_else(|| "verify Portable ABI failed: test layout mismatch".to_owned())
    })
}

/// Native ingress contract: LawnApp stays alive on this game thread until
/// dispatch returns, before native update, Board replacement, or plugin unload.
pub fn dispatch(world_replaced: bool, completed_rounds: u64) -> i32 {
    HOST.with(|host| {
        let Ok(mut slot) = host.try_borrow_mut() else {
            return 0;
        };
        let Some(state) = slot.as_mut() else {
            return 2;
        };
        if !state.active {
            return 2;
        }
        if NATIVE_EVENT_FAILED.replace(false) {
            finish(
                state,
                wire::Terminal::Failure {
                    message: "native event callback panicked".to_owned(),
                },
                None,
            );
            return 2;
        }

        state.completed_rounds = state.completed_rounds.saturating_add(completed_rounds);
        let stop_requested = match stop_event_signaled(state.stop_event.as_ref()) {
            Ok(requested) => requested,
            Err(error) => {
                finish(state, wire::Terminal::Failure { message: error }, None);
                return 2;
            }
        };
        let opening_ready = match readiness::before_dispatch(&state.backend, stop_requested) {
            Ok(readiness::BeforeDispatch::Dispatch { opening_ready }) => opening_ready,
            Ok(readiness::BeforeDispatch::AdvanceNative) => {
                state.frames += 1;
                return 0;
            }
            Err(error) => {
                finish(
                    state,
                    wire::Terminal::Failure {
                        message: error.to_string(),
                    },
                    None,
                );
                return 2;
            }
        };
        state.dispatch_started = true;
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            (state.dispatch)(
                &mut state.backend,
                DispatchInput {
                    stop_requested,
                    world_replaced,
                    completed_rounds,
                    opening_ready,
                },
            )
        }));
        match result {
            Ok(DispatchResult::Continue) => {
                state.frames += 1;
                0
            }
            Ok(DispatchResult::SkipUpdate) => match readiness::after_dispatch(&mut state.backend) {
                Ok(readiness::AfterDispatch::KeepSkip) => 1,
                Ok(readiness::AfterDispatch::AdvanceNative) => {
                    state.frames += 1;
                    0
                }
                Err(error) => {
                    finish(
                        state,
                        wire::Terminal::Failure {
                            message: error.to_string(),
                        },
                        None,
                    );
                    2
                }
            },
            Ok(DispatchResult::Stop { artifact, error }) => {
                state.cleanup_complete = true;
                let terminal = error.map_or(wire::Terminal::Success {}, |error| wire::Terminal::Failure {
                    message: error.to_string(),
                });
                finish(state, terminal, artifact);
                2
            }
            Err(_) => {
                finish(
                    state,
                    wire::Terminal::Failure {
                        message: "user dispatch panicked".to_owned(),
                    },
                    None,
                );
                2
            }
        }
    })
}

pub fn shutdown() -> i32 {
    HOST.with(|host| {
        let Ok(mut slot) = host.try_borrow_mut() else {
            return 1;
        };
        if let Some(state) = slot.as_mut()
            && state.dispatch_started
            && !state.cleanup_complete
        {
            let result = panic::catch_unwind(AssertUnwindSafe(|| {
                (state.dispatch)(
                    &mut state.backend,
                    DispatchInput {
                        stop_requested: true,
                        world_replaced: false,
                        completed_rounds: 0,
                        opening_ready: false,
                    },
                )
            }));
            match result {
                Ok(DispatchResult::Stop { .. }) => state.cleanup_complete = true,
                _ => return 1, // Preserve the token and DLL if cleanup did not complete.
            }
        }
        if let Some(state) = slot.as_mut()
            && state.active
        {
            finish(
                state,
                wire::Terminal::Failure {
                    message: "game shut down before the script completed".to_owned(),
                },
                None,
            );
        }
        slot.take();
        0
    })
}

pub fn native_event_panicked() {
    NATIVE_EVENT_FAILED.set(true);
}

fn finish(state: &mut HostState, terminal: wire::Terminal, artifact: Option<SessionArtifact>) {
    state.active = false;
    readiness::clear();
    crate::event::clear_native_event_sink();
    restore_all();

    let mut terminal = terminal;
    if let (wire::Terminal::Success {}, Some(path), Some(artifact)) =
        (&terminal, state.config.output_path.as_deref(), artifact)
        && let Err(error) = output::write_artifact(path, &state.config.run_id, &artifact)
    {
        terminal = wire::Terminal::Failure { message: error };
    }
    let result = wire::PortableRunResult {
        schema_version: wire::SCHEMA_VERSION,
        run_id: state.config.run_id.clone(),
        backend: wire::BackendKind::PvzPortable {},
        terminal,
        wall_ns: state.started.elapsed().as_nanos(),
        frames: state.frames,
        completed_rounds: state.completed_rounds,
    };
    if let Err(error) = output::write_control(&state.config.report_path, &state.config.run_id, &result) {
        crate::default_log_output(&format!("failed to publish Portable result: {error}"));
    }
}

fn restore_all() {
    unsafe extern "C" {
        fn rsvz_pvzp_restore_all();
    }
    // SAFETY: restoration is idempotent and runs on the Portable game thread.
    unsafe { rsvz_pvzp_restore_all() };
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    use rsvz_backend_api::opening::SeedChooserReadiness;

    use super::*;

    static NEXT_DIR: AtomicU64 = AtomicU64::new(0);
    static SEQUENCE: AtomicUsize = AtomicUsize::new(0);
    static OPENING_READY_SEEN: AtomicUsize = AtomicUsize::new(0);

    #[unsafe(no_mangle)]
    extern "C" fn rsvz_pvzp_restore_all() {}

    #[unsafe(no_mangle)]
    extern "C" fn rsvz_pvzp_log(_message: *const std::ffi::c_char, _length: usize) {}

    struct TestRun {
        dir: PathBuf,
        config: wire::PortableRunConfig,
        stop_event: Option<OwnedHandle>,
    }

    impl TestRun {
        fn new(label: &str) -> Self {
            Self::with_stop_event(label, false)
        }

        fn with_stop_event(label: &str, enabled: bool) -> Self {
            let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("rsvz-pvzp-host-{label}-{}-{id}", std::process::id()));
            fs::create_dir_all(&dir).expect("create test directory");
            let stop_event_name =
                enabled.then(|| format!("Local\\RustVsZombies.PvZPortable.Test.{}.{}", std::process::id(), id));
            let stop_event = stop_event_name.as_deref().map(create_stop_event);
            Self {
                config: wire::PortableRunConfig {
                    schema_version: wire::SCHEMA_VERSION,
                    run_id: format!("{label}-{id}"),
                    backend: wire::BackendKind::PvzPortable {},
                    output_path: None,
                    report_path: dir.join("report.json"),
                    stop_event_name,
                },
                dir,
                stop_event,
            }
        }

        fn initialize(&self, entry: DispatchEntry) -> Result<(), String> {
            initialize(&serde_json::to_vec(&self.config).expect("serialize config"), entry)
        }

        fn result(&self) -> wire::PortableRunResult {
            serde_json::from_slice(&fs::read(&self.config.report_path).expect("read report")).expect("parse report")
        }

        fn signal_stop(&self) {
            let event = self.stop_event.as_ref().expect("test run has a stop event");
            // SAFETY: `event` owns a live event handle.
            assert_ne!(unsafe { SetEvent(event.as_raw_handle()) }, 0);
        }
    }

    impl Drop for TestRun {
        fn drop(&mut self) {
            HOST.with_borrow_mut(|host| *host = None);
            TEST_LAYOUT_VALID.set(true);
            let _cleanup = fs::remove_dir_all(&self.dir);
        }
    }

    fn create_stop_event(name: &str) -> OwnedHandle {
        let name: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: default security is requested and `name` is NUL-terminated for this call.
        let handle = unsafe { CreateEventW(std::ptr::null(), TRUE, FALSE, name.as_ptr()) };
        assert!(
            !handle.is_null(),
            "create test stop event: {}",
            std::io::Error::last_os_error()
        );
        // SAFETY: `CreateEventW` returned a new owned handle.
        unsafe { OwnedHandle::from_raw_handle(handle) }
    }

    fn sequence_entry(_backend: &mut PortableBackend, _input: DispatchInput) -> DispatchResult {
        match SEQUENCE.fetch_add(1, Ordering::Relaxed) {
            0 => DispatchResult::Continue,
            1 => DispatchResult::SkipUpdate,
            _ => DispatchResult::Stop {
                artifact: None,
                error: None,
            },
        }
    }

    fn panic_entry(_backend: &mut PortableBackend, _input: DispatchInput) -> DispatchResult {
        panic!("test dispatch panic")
    }

    fn opening_ready_entry(_backend: &mut PortableBackend, input: DispatchInput) -> DispatchResult {
        OPENING_READY_SEEN.store(usize::from(input.opening_ready()), Ordering::Relaxed);
        DispatchResult::Stop {
            artifact: None,
            error: None,
        }
    }

    fn stop_entry(_backend: &mut PortableBackend, input: DispatchInput) -> DispatchResult {
        if input.stop_requested {
            DispatchResult::Stop {
                artifact: None,
                error: None,
            }
        } else {
            DispatchResult::Continue
        }
    }

    #[test]
    fn dispatch_distinguishes_continue_skip_and_stop() {
        let run = TestRun::new("sequence");
        SEQUENCE.store(0, Ordering::Relaxed);
        run.initialize(sequence_entry).expect("initialize");
        assert_eq!(dispatch(false, 1), 0);
        assert_eq!(dispatch(false, 2), 1);
        assert_eq!(dispatch(true, 3), 2);
        let result = run.result();
        assert_eq!(result.frames, 1);
        assert_eq!(result.completed_rounds, 6);
        assert_eq!(result.terminal, wire::Terminal::Success {});
    }

    #[test]
    fn dispatch_panic_is_reported_and_disables_runtime() {
        let run = TestRun::new("panic");
        run.initialize(panic_entry).expect("initialize");
        assert_eq!(dispatch(false, 0), 2);
        assert_eq!(dispatch(false, 0), 2);
        assert_eq!(
            run.result().terminal,
            wire::Terminal::Failure {
                message: "user dispatch panicked".to_owned()
            }
        );
    }

    #[test]
    fn named_event_requests_cooperative_stop() {
        let run = TestRun::with_stop_event("stop-event", true);
        run.initialize(stop_entry).expect("initialize");
        run.signal_stop();
        assert_eq!(dispatch(false, 0), 2);
        assert_eq!(run.result().terminal, wire::Terminal::Success {});
    }

    #[test]
    fn dispatch_passes_native_seed_chooser_readiness_to_runtime() {
        let run = TestRun::new("opening-ready");
        OPENING_READY_SEEN.store(1, Ordering::Relaxed);
        readiness::set_seed_chooser_readiness_for_test(SeedChooserReadiness::NotSeedChoosing);
        run.initialize(opening_ready_entry).expect("initialize");
        assert_eq!(dispatch(false, 0), 2);
        assert_eq!(OPENING_READY_SEEN.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn native_event_panic_and_shutdown_are_terminal_failures() {
        let event_run = TestRun::new("event-panic");
        event_run.initialize(sequence_entry).expect("initialize");
        native_event_panicked();
        assert!(!event_run.config.report_path.exists());
        assert_eq!(dispatch(false, 0), 2);
        assert_eq!(
            event_run.result().terminal,
            wire::Terminal::Failure {
                message: "native event callback panicked".to_owned()
            }
        );
        drop(event_run);

        let shutdown_run = TestRun::new("shutdown");
        shutdown_run.initialize(sequence_entry).expect("initialize");
        shutdown();
        assert_eq!(
            shutdown_run.result().terminal,
            wire::Terminal::Failure {
                message: "game shut down before the script completed".to_owned()
            }
        );
    }

    #[test]
    fn abi_mismatch_never_installs_host_state() {
        let run = TestRun::new("layout");
        TEST_LAYOUT_VALID.set(false);
        assert!(
            run.initialize(sequence_entry)
                .expect_err("layout must fail")
                .contains("ABI")
        );
        assert_eq!(dispatch(false, 0), 2);
        assert_eq!(
            run.result().terminal,
            wire::Terminal::Failure {
                message: "verify Portable ABI failed: test layout mismatch".to_owned()
            }
        );
    }

    #[test]
    fn nested_dispatch_and_shutdown_cannot_borrow_a_second_token() {
        let run = TestRun::new("nested");
        run.initialize(sequence_entry).expect("initialize");
        HOST.with_borrow_mut(|slot| {
            assert!(slot.is_some());
            assert_eq!(dispatch(false, 0), 0);
            assert_eq!(shutdown(), 1);
            native_event_panicked();
            assert!(slot.as_ref().unwrap().active);
        });
        assert_eq!(dispatch(false, 0), 2);
        assert_eq!(shutdown(), 0);
        assert_eq!(shutdown(), 0);
    }

    #[test]
    fn report_failure_does_not_turn_safe_cleanup_into_an_unload_failure() {
        fn stop(_: &mut PortableBackend, _: DispatchInput) -> DispatchResult {
            DispatchResult::Stop {
                artifact: None,
                error: None,
            }
        }
        let mut run = TestRun::new("report-failure");
        run.config.report_path = run.dir.clone(); // A directory cannot be replaced by the report file.
        run.initialize(stop).unwrap();
        assert_eq!(dispatch(false, 0), 2);
        assert_eq!(shutdown(), 0);
        HOST.with_borrow(|slot| assert!(slot.is_none()));
    }

    #[test]
    fn incomplete_script_cleanup_retains_the_host_resources() {
        fn keep_running(_: &mut PortableBackend, _: DispatchInput) -> DispatchResult {
            DispatchResult::Continue
        }
        let run = TestRun::new("incomplete-cleanup");
        run.initialize(keep_running).unwrap();
        assert_eq!(dispatch(false, 0), 0);
        assert_eq!(shutdown(), 1);
        HOST.with_borrow(|slot| assert!(slot.is_some()));
    }
}
