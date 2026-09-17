#[cfg(feature = "fast-forward-profiler")]
mod imp {
    use std::cell::RefCell;
    use std::fs::{self, OpenOptions};
    use std::io::Write as _;
    use std::time::{Duration, Instant};

    use rsvz_model::FastForwardStopReason;
    use rsvz_profiling::{FrameIterationRecorder, FrameProfileStats};

    thread_local! {
        static PROFILER: RefCell<FastForwardProfiler> = const { RefCell::new(FastForwardProfiler::new()) };
    }

    struct FastForwardProfiler {
        active: bool,
        started: Option<Instant>,
        stats: FrameProfileStats,
        current_iteration: Option<FrameIterationRecorder>,
    }

    impl FastForwardProfiler {
        const fn new() -> Self {
            Self {
                active: false,
                started: None,
                stats: FrameProfileStats::new(),
                current_iteration: None,
            }
        }

        fn reset(&mut self) {
            self.active = true;
            self.started = Some(Instant::now());
            self.stats = FrameProfileStats::new();
            self.current_iteration = None;
        }

        fn begin_loop_iteration(&mut self) {
            if self.active {
                self.current_iteration = Some(FrameIterationRecorder::start(true));
            }
        }

        fn finish_loop_iteration(&mut self) {
            if !self.active {
                return;
            }
            if let Some(iteration) = self.current_iteration.take() {
                let _recorded = iteration.finish(&mut self.stats);
            }
        }

        fn record_run_total(&mut self, elapsed: Duration) {
            if let Some(iteration) = self.current_iteration.as_mut() {
                iteration.record_run_duration(elapsed);
            }
        }

        fn record_backend_update(&mut self, elapsed: Duration) {
            if let Some(iteration) = self.current_iteration.as_mut() {
                iteration.record_backend_update_duration(elapsed);
            }
        }

        fn finish_summary(&mut self, reason: FastForwardStopReason, frames_advanced: u64) -> Option<String> {
            if !self.active {
                return None;
            }
            self.active = false;
            let elapsed = self.started.map_or(Duration::ZERO, |started| started.elapsed());
            let summary = self
                .stats
                .render_summary("fast-forward profiler", reason, frames_advanced, elapsed);
            self.started = None;
            self.stats = FrameProfileStats::new();
            self.current_iteration = None;
            Some(summary)
        }
    }

    pub(crate) fn reset() {
        PROFILER.with(|profiler| profiler.borrow_mut().reset());
    }

    #[inline]
    pub(crate) fn measure_loop_iteration<T>(f: impl FnOnce() -> T) -> T {
        PROFILER.with(|profiler| profiler.borrow_mut().begin_loop_iteration());
        let result = f();
        PROFILER.with(|profiler| profiler.borrow_mut().finish_loop_iteration());
        result
    }

    #[inline]
    pub(crate) fn measure_run_total<T>(f: impl FnOnce() -> T) -> T {
        let started = Instant::now();
        let result = f();
        let elapsed = started.elapsed();
        PROFILER.with(|profiler| profiler.borrow_mut().record_run_total(elapsed));
        result
    }

    #[inline]
    pub(crate) fn measure_backend_update<T>(f: impl FnOnce() -> T) -> T {
        let started = Instant::now();
        let result = f();
        let elapsed = started.elapsed();
        PROFILER.with(|profiler| profiler.borrow_mut().record_backend_update(elapsed));
        result
    }

    pub(crate) fn finish_summary(reason: FastForwardStopReason, frames_advanced: u64) -> Option<String> {
        PROFILER.with(|profiler| profiler.borrow_mut().finish_summary(reason, frames_advanced))
    }

    pub(crate) fn write_summary_to_temp(summary: &str) {
        let path = std::env::temp_dir().join(format!("rsvz-1051-fast-forward-profile-{}.txt", std::process::id()));
        if let Some(parent) = path.parent()
            && fs::create_dir_all(parent).is_err()
        {
            return;
        }
        let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) else {
            return;
        };
        let _write_result = writeln!(file, "{summary}");
    }
}

#[cfg(not(feature = "fast-forward-profiler"))]
mod imp {
    use rsvz_model::FastForwardStopReason;

    pub(crate) fn reset() {}

    #[inline(always)]
    pub(crate) fn measure_loop_iteration<T>(f: impl FnOnce() -> T) -> T {
        f()
    }

    #[inline(always)]
    pub(crate) fn measure_run_total<T>(f: impl FnOnce() -> T) -> T {
        f()
    }

    #[inline(always)]
    pub(crate) fn measure_backend_update<T>(f: impl FnOnce() -> T) -> T {
        f()
    }

    pub(crate) fn finish_summary(_reason: FastForwardStopReason, _frames_advanced: u64) -> Option<String> {
        None
    }

    pub(crate) fn write_summary_to_temp(_summary: &str) {}
}

pub(crate) use imp::{
    finish_summary, measure_backend_update, measure_loop_iteration, measure_run_total, reset, write_summary_to_temp,
};
