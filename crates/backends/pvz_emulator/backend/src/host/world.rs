use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use rsvz_backend_api::WaveTimingBackend;
use rsvz_backend_api::artifact::SessionArtifact;
use rsvz_backend_api::error::RuntimeError;
use rsvz_model::SessionShard;

use super::{profile, wire};
use crate::runtime::PeWorldOwner;
use crate::{DispatchEntry, DispatchInput, DispatchResult, PeUpdateOutcome, PeWorldConfig};

pub(super) struct WorkerOutput {
    pub result: wire::WorkerResult,
    pub artifact: Option<SessionArtifact>,
    pub error: Option<String>,
}

pub(super) fn run_worker(
    worker_index: usize, worker_count: usize, seed: u32, config: &wire::PeRunConfig, cancelled: &AtomicBool,
    dispatch: DispatchEntry,
) -> WorkerOutput {
    let output = match run_worker_inner(worker_index, worker_count, seed, config, cancelled, dispatch) {
        Ok(output) => output,
        Err(error) => failed_worker(worker_index, seed, error),
    };
    if output.error.is_some() {
        cancelled.store(true, Ordering::Release);
    }
    output
}

pub(super) fn failed_worker(worker_index: usize, seed: u32, error: String) -> WorkerOutput {
    WorkerOutput {
        result: worker_result(
            worker_index,
            seed,
            wire::StopReason::Failed {},
            Duration::ZERO,
            0,
            0,
            0,
            None,
            None,
        ),
        artifact: None,
        error: Some(error),
    }
}

fn run_worker_inner(
    worker_index: usize, worker_count: usize, seed: u32, config: &wire::PeRunConfig, cancelled: &AtomicBool,
    dispatch: DispatchEntry,
) -> Result<WorkerOutput, String> {
    let started = Instant::now();
    let world_config = PeWorldConfig {
        battle_seed: seed,
        level_seed: seed,
        ..PeWorldConfig::default()
    };
    let owner = PeWorldOwner::new_reset_deferred_spawn(world_config).map_err(|error| error.to_string())?;
    let mut world = owner.install_current().map_err(|error| error.to_string())?;
    let shard = SessionShard {
        index: u32::try_from(worker_index).map_err(|_error| "worker index does not fit u32")?,
        count: u32::try_from(worker_count).map_err(|_error| "worker count does not fit u32")?,
        seed_base: match config.seed_mode {
            wire::SeedMode::Fixed { seed } | wire::SeedMode::Base { seed } => seed,
        },
    };
    let mut frames = 0u64;
    let mut partial_frames = 0u64;
    let mut completed_levels = 0u64;
    let mut completed_rounds = 0;
    let mut profile = profile::WorkerProfile::new(config.profile, config.profile_detail);
    let mut performance = profile::PerformanceWindow::new(config.performance_window_ms);

    loop {
        performance.poll(worker_index, started, frames, completed_levels);
        let limit = limit_reason(config, cancelled, started, frames, completed_levels);
        let mut frame_profile = profile.begin_frame();
        let output = profile.dispatch(&mut frame_profile, || {
            world.with_backend(|backend| {
                // Native hosts advance Survival repick countdowns without script dispatch.
                // A host limit must still reach the script's normal teardown path.
                if limit.is_none() {
                    match backend.level_end_countdown() {
                        Ok(countdown) if countdown > 0 => return DispatchResult::Continue,
                        Ok(_) => {}
                        Err(error) => {
                            return DispatchResult::Stop {
                                artifact: None,
                                error: Some(RuntimeError::new(error.to_string())),
                            };
                        }
                    }
                }
                dispatch(
                    backend,
                    DispatchInput {
                        shard,
                        stop_requested: limit.is_some(),
                        completed_rounds,
                    },
                )
            })
        });
        completed_rounds = 0;
        match output {
            DispatchResult::Continue => {}
            DispatchResult::SkipUpdate => {
                profile.finish_frame(frame_profile);
                continue;
            }
            DispatchResult::Stop { artifact, error } => {
                profile.finish_frame(frame_profile);
                performance.poll(worker_index, started, frames, completed_levels);
                let error = error.map(|error| error.to_string());
                let stop_reason = if error.is_some() {
                    wire::StopReason::Failed {}
                } else {
                    limit.unwrap_or(wire::StopReason::Script {})
                };
                return Ok(WorkerOutput {
                    result: worker_result(
                        worker_index,
                        seed,
                        stop_reason,
                        started.elapsed(),
                        frames,
                        completed_levels,
                        partial_frames,
                        profile.snapshot(),
                        performance.captured,
                    ),
                    artifact,
                    error,
                });
            }
        }

        let update = profile.update(&mut frame_profile, || world.update_world());
        profile.finish_frame(frame_profile);
        match update {
            Ok(PeUpdateOutcome::Normal) => {
                frames = frames.saturating_add(1);
                partial_frames = partial_frames.saturating_add(1);
            }
            Ok(PeUpdateOutcome::ObjectiveReached) => {
                frames = frames.saturating_add(1);
                completed_levels = completed_levels.saturating_add(1);
                completed_rounds = 1;
                partial_frames = 0;
            }
            Ok(PeUpdateOutcome::GameOver) => {
                frames = frames.saturating_add(1);
                partial_frames = partial_frames.saturating_add(1);
                let mut terminal_profile = profile.begin_frame();
                let output = profile.dispatch(&mut terminal_profile, || {
                    world.with_backend(|backend| {
                        dispatch(
                            backend,
                            DispatchInput {
                                shard,
                                stop_requested: false,
                                completed_rounds: 0,
                            },
                        )
                    })
                });
                profile.finish_frame(terminal_profile);
                performance.poll(worker_index, started, frames, completed_levels);
                return Ok(match output {
                    DispatchResult::Stop { artifact, error } => WorkerOutput {
                        result: worker_result(
                            worker_index,
                            seed,
                            if error.is_some() {
                                wire::StopReason::Failed {}
                            } else {
                                wire::StopReason::GameOver {}
                            },
                            started.elapsed(),
                            frames,
                            completed_levels,
                            partial_frames,
                            profile.snapshot(),
                            performance.captured,
                        ),
                        artifact,
                        error: error.map(|error| error.to_string()),
                    },
                    DispatchResult::Continue | DispatchResult::SkipUpdate => {
                        // The terminal dispatch can reset a measurement world for its next trial.
                        // Only the post-dispatch state can decide whether GameOver still applies.
                        if !world
                            .with_backend(|backend| backend.game_over())
                            .map_err(|error| error.to_string())?
                        {
                            continue;
                        }
                        // Preserve the protocol error, but let the normal stop boundary release
                        // session callbacks before the worker's TLS starts being destroyed.
                        let _ = world.with_backend(|backend| {
                            dispatch(
                                backend,
                                DispatchInput {
                                    shard,
                                    stop_requested: true,
                                    completed_rounds: 0,
                                },
                            )
                        });
                        WorkerOutput {
                            result: worker_result(
                                worker_index,
                                seed,
                                wire::StopReason::Failed {},
                                started.elapsed(),
                                frames,
                                completed_levels,
                                partial_frames,
                                profile.snapshot(),
                                performance.captured,
                            ),
                            artifact: None,
                            error: Some("dispatch did not stop after GameOver".to_owned()),
                        }
                    }
                });
            }
            Err(error) => {
                return Ok(WorkerOutput {
                    result: worker_result(
                        worker_index,
                        seed,
                        wire::StopReason::Failed {},
                        started.elapsed(),
                        frames,
                        completed_levels,
                        partial_frames,
                        profile.snapshot(),
                        performance.captured,
                    ),
                    artifact: None,
                    error: Some(error.to_string()),
                });
            }
        }
    }
}

fn limit_reason(
    config: &wire::PeRunConfig, cancelled: &AtomicBool, started: Instant, frames: u64, levels: u64,
) -> Option<wire::StopReason> {
    if cancelled.load(Ordering::Acquire) {
        return Some(wire::StopReason::Cancelled {});
    }
    if let Some(limit) = config.limits.max_wall_ms
        && started.elapsed() >= Duration::from_millis(limit)
    {
        return Some(wire::StopReason::MaxWall {});
    }
    if config.limits.max_sim_frames.is_some_and(|limit| frames >= limit) {
        return Some(wire::StopReason::MaxSimFrames {});
    }
    if config.limits.max_levels.is_some_and(|limit| levels >= limit) {
        return Some(wire::StopReason::MaxLevels {});
    }
    None
}

fn worker_result(
    worker_index: usize, seed: u32, stop_reason: wire::StopReason, wall: Duration, frames: u64, completed_levels: u64,
    partial_frames: u64, profile: Option<wire::RawProfile>, performance_window: Option<wire::PerformanceWindow>,
) -> wire::WorkerResult {
    wire::WorkerResult {
        worker_index,
        seed,
        stop_reason,
        wall_ns: wall.as_nanos(),
        frames,
        completed_levels,
        partial_frames,
        profile,
        performance_window,
    }
}
