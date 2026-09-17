use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use super::wire;
use super::world::{WorkerOutput, failed_worker, run_worker};
use crate::DispatchEntry;

pub(super) fn run_all(config: &wire::PeRunConfig, dispatch: DispatchEntry) -> Vec<WorkerOutput> {
    let cancelled = AtomicBool::new(false);
    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(config.threads);
        for worker_index in 0..config.threads {
            let seed = match config.seed_mode {
                wire::SeedMode::Fixed { seed } => seed,
                wire::SeedMode::Base { seed } => seed.wrapping_add(u32::try_from(worker_index).unwrap_or(u32::MAX)),
            };
            let worker_cancelled = &cancelled;
            let handle = thread::Builder::new()
                .name(format!("rsvz-pe-worker-{worker_index}"))
                .spawn_scoped(scope, move || {
                    let output = catch_unwind(AssertUnwindSafe(|| {
                        run_worker(worker_index, config.threads, seed, config, worker_cancelled, dispatch)
                    }));
                    if output.is_err() {
                        worker_cancelled.store(true, Ordering::Release);
                    }
                    output
                });
            if handle.is_err() {
                cancelled.store(true, Ordering::Release);
            }
            handles.push((worker_index, seed, handle));
        }

        handles
            .into_iter()
            .map(|(worker_index, seed, handle)| match handle {
                Ok(handle) => match handle.join() {
                    Ok(Ok(output)) => output,
                    Ok(Err(_)) | Err(_) => {
                        failed_worker(worker_index, seed, format!("PE worker {worker_index} panicked"))
                    }
                },
                Err(error) => failed_worker(
                    worker_index,
                    seed,
                    format!("spawn PE worker {worker_index} failed: {error}"),
                ),
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::path::PathBuf;

    use rsvz_backend_api::{WorldResetBackend, ZombieCreateBackend, ZombieXWriteBackend};
    use rsvz_model::{I32RepresentableF32, WorldResetConfig, ZombieKind};

    use super::*;
    use crate::{DispatchInput, DispatchResult, PeBackend};

    thread_local! {
        static RESET_STAGE: Cell<u8> = const { Cell::new(0) };
    }
    static CLEANUP_REQUESTED: AtomicBool = AtomicBool::new(false);
    static TRANSPORT_DISPATCHES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    fn transport_fixture(backend: &mut PeBackend, input: DispatchInput) -> DispatchResult {
        let call = TRANSPORT_DISPATCHES.fetch_add(1, Ordering::SeqCst);
        if input.stop_requested {
            return DispatchResult::Stop {
                artifact: None,
                error: None,
            };
        }
        if call == 0 {
            backend
                .with_current_world_mut(|world| {
                    let spawn = crate::access::spawn_data(world).as_ptr();
                    // SAFETY: the fixture exclusively borrows this worker's world; no entities live.
                    unsafe { std::ptr::addr_of_mut!((*spawn).countdown.endgame).write(3) };
                })
                .unwrap();
        }
        DispatchResult::Continue
    }

    #[test]
    fn repick_advances_without_callbacks_and_still_honors_limits() {
        for (frames, calls) in [(4, 3), (1, 2)] {
            let mut config = test_config("repick-dispatch");
            config.threads = 1;
            config.limits.max_sim_frames = Some(frames);
            TRANSPORT_DISPATCHES.store(0, Ordering::SeqCst);
            let outputs = run_all(&config, transport_fixture);
            assert!(outputs[0].error.is_none(), "{:?}", outputs[0].error);
            assert_eq!(outputs[0].result.frames, frames);
            assert_eq!(TRANSPORT_DISPATCHES.load(Ordering::SeqCst), calls);
        }
    }

    fn spawn_home_entry(backend: &PeBackend) {
        let zombie = backend.add_zombie_in_row(ZombieKind::Normal, 0, 0).unwrap().unwrap();
        backend
            .set_zombie_x(zombie, I32RepresentableF32::new(-200.0).unwrap())
            .unwrap();
    }

    fn reset_after_game_over(backend: &mut PeBackend, input: DispatchInput) -> DispatchResult {
        assert!(!input.stop_requested, "a successful reset must keep the worker running");
        let stage = RESET_STAGE.get();
        RESET_STAGE.set(stage + 1);
        match stage {
            0 => {
                spawn_home_entry(backend);
                DispatchResult::Continue
            }
            1 => {
                assert!(backend.game_over().unwrap());
                backend
                    .reset_world(WorldResetConfig {
                        completed_rounds: 63,
                        ..WorldResetConfig::default()
                    })
                    .unwrap();
                DispatchResult::SkipUpdate
            }
            2 => {
                assert!(!backend.game_over().unwrap());
                DispatchResult::Continue
            }
            3 => DispatchResult::Stop {
                artifact: None,
                error: None,
            },
            _ => panic!("unexpected extra dispatch"),
        }
    }

    fn skip_without_reset(backend: &mut PeBackend, input: DispatchInput) -> DispatchResult {
        if input.stop_requested {
            CLEANUP_REQUESTED.store(true, Ordering::SeqCst);
            return DispatchResult::Stop {
                artifact: None,
                error: None,
            };
        }
        if backend.game_over().unwrap() {
            DispatchResult::SkipUpdate
        } else {
            spawn_home_entry(backend);
            DispatchResult::Continue
        }
    }

    #[test]
    fn game_over_reset_continues_to_the_next_world() {
        let mut config = test_config("game-over-reset");
        config.threads = 1;
        config.limits.max_wall_ms = Some(1_000);
        let outputs = run_all(&config, reset_after_game_over);
        assert!(outputs[0].error.is_none(), "{:?}", outputs[0].error);
        assert_eq!(outputs[0].result.stop_reason, wire::StopReason::Script {});
        assert_eq!(outputs[0].result.frames, 2);
        assert_eq!(outputs[0].result.completed_levels, 0);
    }

    #[test]
    fn game_over_without_reset_requests_cleanup_and_remains_an_error() {
        let mut config = test_config("game-over-no-reset");
        config.threads = 1;
        config.limits.max_wall_ms = Some(1_000);
        CLEANUP_REQUESTED.store(false, Ordering::SeqCst);
        let outputs = run_all(&config, skip_without_reset);
        assert_eq!(
            outputs[0].error.as_deref(),
            Some("dispatch did not stop after GameOver")
        );
        assert!(CLEANUP_REQUESTED.load(Ordering::SeqCst));
        assert_eq!(outputs[0].result.stop_reason, wire::StopReason::Failed {});
    }

    fn panic_on_second_worker(_: &mut PeBackend, input: DispatchInput) -> DispatchResult {
        assert!(input.shard.index != 1, "worker fixture panic");
        if input.stop_requested {
            DispatchResult::Stop {
                artifact: None,
                error: None,
            }
        } else {
            DispatchResult::Continue
        }
    }

    fn fail_second_worker(_: &mut PeBackend, input: DispatchInput) -> DispatchResult {
        if input.shard.index == 1 {
            DispatchResult::Stop {
                artifact: None,
                error: Some(rsvz_backend_api::error::RuntimeError::new("worker fixture failure")),
            }
        } else if input.stop_requested {
            DispatchResult::Stop {
                artifact: None,
                error: None,
            }
        } else {
            DispatchResult::Continue
        }
    }

    fn stop_at_limit(_: &mut PeBackend, input: DispatchInput) -> DispatchResult {
        if input.stop_requested {
            DispatchResult::Stop {
                artifact: None,
                error: None,
            }
        } else {
            DispatchResult::Continue
        }
    }

    fn test_config(run_id: &str) -> wire::PeRunConfig {
        wire::PeRunConfig {
            schema_version: wire::SCHEMA_VERSION,
            run_id: run_id.to_owned(),
            threads: 2,
            seed_mode: wire::SeedMode::Base { seed: 100 },
            limits: wire::RunLimits::default(),
            profile: false,
            profile_detail: false,
            performance_window_ms: None,
            output_path: None,
            raw_result_path: PathBuf::from("unused.json"),
        }
    }

    #[test]
    fn worker_panic_cancels_siblings_and_preserves_derived_seeds() {
        let outputs = run_all(&test_config("worker-panic"), panic_on_second_worker);

        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0].result.seed, 100);
        assert_eq!(outputs[1].result.seed, 101);
        assert!(matches!(outputs[0].result.stop_reason, wire::StopReason::Cancelled {}));
        assert!(matches!(outputs[1].result.stop_reason, wire::StopReason::Failed {}));
    }

    #[test]
    fn worker_error_cancels_siblings() {
        let outputs = run_all(&test_config("worker-error"), fail_second_worker);

        assert!(matches!(outputs[0].result.stop_reason, wire::StopReason::Cancelled {}));
        assert!(matches!(outputs[1].result.stop_reason, wire::StopReason::Failed {}));
    }

    #[test]
    fn mid_level_stop_reports_partial_frames() {
        let mut config = test_config("partial-frames");
        config.threads = 1;
        config.limits.max_sim_frames = Some(3);

        let outputs = run_all(&config, stop_at_limit);

        assert_eq!(outputs[0].result.frames, 3);
        assert_eq!(outputs[0].result.partial_frames, 3);
        assert_eq!(outputs[0].result.completed_levels, 0);
    }
}
