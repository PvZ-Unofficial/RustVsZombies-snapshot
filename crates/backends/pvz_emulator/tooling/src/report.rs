//! Cold-path presentation of raw PE runner results.

use anyhow::{Context, Result, bail};

use crate::wire;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReportFormat {
    None,
    Json,
    Text { stats: bool },
}

pub(crate) fn render_result(result: &wire::PeRunResult, format: ReportFormat) -> Result<()> {
    if let wire::Terminal::Failure { message } = &result.terminal {
        bail!("generated PE runner failed: {message}");
    }
    match format {
        ReportFormat::None => return Ok(()),
        ReportFormat::Json => {
            println!(
                "{}",
                serde_json::to_string_pretty(result).context("serialize PE JSON report failed")?
            );
            return Ok(());
        }
        ReportFormat::Text { stats } => {
            println!("{}", text_summary(result));
            if stats {
                for worker in &result.workers {
                    println!("{}", worker_stats_line(worker));
                }
            }
        }
    }
    render_profiles(result);
    Ok(())
}

fn text_summary(result: &wire::PeRunResult) -> String {
    let total_frames = result.workers.iter().map(|worker| worker.frames).sum::<u64>();
    let completed_levels = result.workers.iter().map(|worker| worker.completed_levels).sum::<u64>();
    let partial_frames = result.workers.iter().map(|worker| worker.partial_frames).sum::<u64>();
    let seconds = result.parent_wall_ns as f64 / 1_000_000_000.0;
    let frames_per_second = if seconds > 0.0 {
        total_frames as f64 / seconds
    } else {
        0.0
    };
    let levels_per_second = if seconds > 0.0 {
        completed_levels as f64 / seconds
    } else {
        0.0
    };
    let average = if completed_levels == 0 {
        "n/a".to_owned()
    } else {
        format!(
            "{:.3}",
            total_frames.saturating_sub(partial_frames) as f64 / completed_levels as f64
        )
    };
    format!(
        "PE run completed\nwall_ms={}\ntotal_frames={total_frames}\ncompleted_levels={completed_levels}\npartial_frames={partial_frames}\nframes_per_wall_sec={frames_per_second:.3}\nlevels_per_wall_sec={levels_per_second:.3}\naverage_frames_per_level={average}",
        result.parent_wall_ns / 1_000_000
    )
}

fn worker_stats_line(worker: &wire::WorkerResult) -> String {
    format!(
        "worker index={} stop={} seed={} frames={} completed_levels={} partial_frames={} wall_ms={}",
        worker.worker_index,
        stop_reason_name(worker.stop_reason),
        worker.seed,
        worker.frames,
        worker.completed_levels,
        worker.partial_frames,
        worker.wall_ns / 1_000_000,
    )
}

const fn stop_reason_name(reason: wire::StopReason) -> &'static str {
    match reason {
        wire::StopReason::Failed {} => "failed",
        wire::StopReason::Script {} => "script",
        wire::StopReason::GameOver {} => "game_over",
        wire::StopReason::MaxWall {} => "max_wall",
        wire::StopReason::MaxSimFrames {} => "max_sim_frames",
        wire::StopReason::MaxLevels {} => "max_levels",
        wire::StopReason::Cancelled {} => "cancelled",
    }
}

fn render_profiles(result: &wire::PeRunResult) {
    for worker in &result.workers {
        let Some(profile) = &worker.profile else {
            continue;
        };
        for stage in &profile.frame_stages {
            println!(
                "profile worker={} section=frame stage={} count={} total_ns={} min_ns={} max_ns={}",
                worker.worker_index, stage.name, stage.count, stage.total_ns, stage.min_ns, stage.max_ns
            );
        }
        for stage in &profile.detailed_stages {
            println!(
                "profile worker={} section=detail stage={} count={} total_ns={} min_ns={} max_ns={}",
                worker.worker_index, stage.name, stage.count, stage.total_ns, stage.min_ns, stage.max_ns
            );
        }
        for counter in &profile.detailed_counters {
            println!(
                "profile worker={} counter={} value={}",
                worker.worker_index, counter.name, counter.value
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result() -> wire::PeRunResult {
        wire::PeRunResult {
            schema_version: wire::SCHEMA_VERSION,
            run_id: "run".to_owned(),
            terminal: wire::Terminal::Success {},
            parent_wall_ns: 2_000_000_000,
            workers: vec![wire::WorkerResult {
                worker_index: 0,
                seed: 7,
                stop_reason: wire::StopReason::MaxSimFrames {},
                wall_ns: 1_000_000,
                frames: 10,
                completed_levels: 2,
                partial_frames: 3,
                profile: None,
                performance_window: None,
            }],
        }
    }

    #[test]
    fn text_renderer_derives_rates_and_excludes_partial_frames_from_average() {
        let result = result();
        let summary = text_summary(&result);
        assert!(summary.contains("total_frames=10"));
        assert!(summary.contains("frames_per_wall_sec=5.000"));
        assert!(summary.contains("average_frames_per_level=3.500"));
        assert!(worker_stats_line(&result.workers[0]).contains("stop=max_sim_frames"));
    }
}
