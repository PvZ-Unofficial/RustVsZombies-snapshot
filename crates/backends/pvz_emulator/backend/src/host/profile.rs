use rsvz_profiling::{
    DetailedProfileGuard, DetailedProfileStage, DetailedProfileStats, FrameIterationRecorder, FrameProfileStage,
    FrameProfileStats, ProfileStageStats, measure_detailed_profile,
};

use super::wire;
use std::time::{Duration, Instant};

pub(super) struct PerformanceWindow {
    requested_ms: Option<u64>,
    pub(super) captured: Option<wire::PerformanceWindow>,
}

impl PerformanceWindow {
    pub(super) fn new(requested_ms: Option<u64>) -> Self {
        Self {
            requested_ms,
            captured: None,
        }
    }

    pub(super) fn poll(&mut self, worker_index: usize, started: Instant, frames: u64, levels: u64) {
        if self.requested_ms.is_some() && self.captured.is_none() {
            self.observe(worker_index, started.elapsed(), frames, levels);
        }
    }

    pub(super) fn observe(&mut self, worker_index: usize, elapsed: Duration, frames: u64, completed_levels: u64) {
        if self.captured.is_none()
            && let Some(requested_ms) = self.requested_ms
            && elapsed >= Duration::from_millis(requested_ms)
        {
            let sample = wire::PerformanceWindow {
                worker_index,
                requested_ms,
                wall_ns: elapsed.as_nanos(),
                frames,
                completed_levels,
            };
            // A single checkpoint is available to tooling immediately, even when
            // a script continues for a long tail. This is independent of script policy.
            eprintln!(
                "PE_PERFORMANCE_WINDOW {}",
                serde_json::to_string(&sample).expect("performance counters serialize")
            );
            self.captured = Some(sample);
        }
    }
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    #[test]
    fn prefix_is_frozen_while_tail_counters_continue() {
        let mut window = PerformanceWindow::new(Some(10));
        window.observe(2, Duration::from_millis(9), 9, 0);
        assert!(window.captured.is_none());
        window.observe(2, Duration::from_millis(10), 10, 1);
        let sealed = window.captured;
        window.observe(2, Duration::from_secs(100), 999, 99);
        assert_eq!(window.captured, sealed);
        assert_eq!(window.captured.unwrap().frames, 10);
    }
}

const HOST_DISPATCH_TOTAL: DetailedProfileStage = DetailedProfileStage::new("host_dispatch_total");
const FRAME_STAGES: [FrameProfileStage; 4] = [
    FrameProfileStage::LoopTotal,
    FrameProfileStage::RunTotal,
    FrameProfileStage::BackendUpdate,
    FrameProfileStage::Other,
];

pub(super) struct WorkerProfile {
    frame: Option<FrameProfileStats>,
    detail: Option<DetailedProfileStats>,
}

impl WorkerProfile {
    pub(super) fn new(frame: bool, detail: bool) -> Self {
        Self {
            frame: frame.then(FrameProfileStats::new),
            detail: detail.then(DetailedProfileStats::new),
        }
    }

    pub(super) fn begin_frame(&self) -> FrameIterationRecorder {
        FrameIterationRecorder::start(self.frame.is_some())
    }

    pub(super) fn dispatch<R>(&mut self, frame: &mut FrameIterationRecorder, f: impl FnOnce() -> R) -> R {
        let detail = self.detail.as_ref().map(|_| DetailedProfileGuard::start());
        let result = frame.measure_run(|| measure_detailed_profile(HOST_DISPATCH_TOTAL, f));
        if let Some(sample) = detail.and_then(DetailedProfileGuard::finish)
            && let Some(total) = self.detail.as_mut()
        {
            total.merge_from(&sample);
        }
        result
    }

    pub(super) fn update<R>(&mut self, frame: &mut FrameIterationRecorder, f: impl FnOnce() -> R) -> R {
        frame.measure_backend_update(f)
    }

    pub(super) fn finish_frame(&mut self, frame: FrameIterationRecorder) {
        if let Some(total) = self.frame.as_mut() {
            let _recorded = frame.finish(total);
        }
    }

    pub(super) fn snapshot(&self) -> Option<wire::RawProfile> {
        if self.frame.is_none() && self.detail.is_none() {
            return None;
        }
        let frame_stages = self.frame.as_ref().map_or_else(Vec::new, |profile| {
            FRAME_STAGES
                .into_iter()
                .map(|stage| wire_stage(stage.name(), profile.stage(stage)))
                .collect()
        });
        let (detailed_stages, detailed_counters) = self.detail.as_ref().map_or_else(
            || (Vec::new(), Vec::new()),
            |profile| {
                (
                    profile
                        .stages()
                        .map(|(stage, stats)| wire_stage(stage.name(), stats))
                        .collect(),
                    profile
                        .counters()
                        .map(|(counter, value)| wire::ProfileCounter {
                            name: counter.name().to_owned(),
                            value,
                        })
                        .collect(),
                )
            },
        );
        Some(wire::RawProfile {
            frame_stages,
            detailed_stages,
            detailed_counters,
        })
    }
}

fn wire_stage(name: &str, stats: ProfileStageStats) -> wire::ProfileStage {
    wire::ProfileStage {
        name: name.to_owned(),
        count: stats.count(),
        total_ns: stats.total_ns(),
        min_ns: stats.min_ns(),
        max_ns: stats.max_ns(),
    }
}
