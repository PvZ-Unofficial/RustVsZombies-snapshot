//! Lightweight profiling primitives shared by backend runtimes.

use std::cell::RefCell;
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const FRAME_PROFILE_STAGES: [FrameProfileStage; 4] = [
    FrameProfileStage::LoopTotal,
    FrameProfileStage::RunTotal,
    FrameProfileStage::BackendUpdate,
    FrameProfileStage::Other,
];

static ACTIVE_DETAILED_PROFILES: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    static DETAILED_PROFILE: RefCell<Option<DetailedProfileStats>> = const { RefCell::new(None) };
}

/// Coarse frame stages that are stable across backend runtimes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum FrameProfileStage {
    /// Full profiled loop iteration.
    LoopTotal = 0,
    /// Script/runtime dispatch work inside the loop.
    RunTotal,
    /// Backend simulation update work inside the loop.
    BackendUpdate,
    /// Work not accounted for by `run_total` or `backend_update`.
    Other,
}

impl FrameProfileStage {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::LoopTotal => "loop_total",
            Self::RunTotal => "run_total",
            Self::BackendUpdate => "backend_update",
            Self::Other => "other",
        }
    }
}

/// Named detailed runtime stage used only by opt-in profilers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DetailedProfileStage {
    name: &'static str,
}

impl DetailedProfileStage {
    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self { name }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        self.name
    }
}

/// Named detailed runtime counter used only by opt-in profilers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DetailedProfileCounter {
    name: &'static str,
}

impl DetailedProfileCounter {
    #[must_use]
    pub const fn new(name: &'static str) -> Self {
        Self { name }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        self.name
    }
}

/// Aggregate stats for one profiled stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfileStageStats {
    count: u64,
    total_ns: u128,
    min_ns: u128,
    max_ns: u128,
}

impl ProfileStageStats {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            count: 0,
            total_ns: 0,
            min_ns: u128::MAX,
            max_ns: 0,
        }
    }

    pub fn record_duration(&mut self, elapsed: Duration) {
        self.record_ns(elapsed.as_nanos());
    }

    pub fn record_ns(&mut self, ns: u128) {
        self.count = self.count.saturating_add(1);
        self.total_ns = self.total_ns.saturating_add(ns);
        self.min_ns = self.min_ns.min(ns);
        self.max_ns = self.max_ns.max(ns);
    }

    pub fn merge_from(&mut self, other: Self) {
        self.count = self.count.saturating_add(other.count);
        self.total_ns = self.total_ns.saturating_add(other.total_ns);
        if other.count > 0 {
            self.min_ns = self.min_ns.min(other.min_ns);
            self.max_ns = self.max_ns.max(other.max_ns);
        }
    }

    #[must_use]
    pub const fn count(self) -> u64 {
        self.count
    }

    #[must_use]
    pub const fn total_ns(self) -> u128 {
        self.total_ns
    }

    #[must_use]
    pub fn avg_ns(self) -> u128 {
        if self.count == 0 {
            0
        } else {
            self.total_ns / u128::from(self.count)
        }
    }

    #[must_use]
    pub fn min_ns(self) -> u128 {
        if self.count == 0 { 0 } else { self.min_ns }
    }

    #[must_use]
    pub const fn max_ns(self) -> u128 {
        self.max_ns
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.count == 0
    }
}

impl Default for ProfileStageStats {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DetailedStageEntry {
    stage: DetailedProfileStage,
    stats: ProfileStageStats,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DetailedCounterEntry {
    counter: DetailedProfileCounter,
    value: u128,
}

/// Aggregate detailed runtime profile stats.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DetailedProfileStats {
    stages: Vec<DetailedStageEntry>,
    counters: Vec<DetailedCounterEntry>,
}

#[expect(
    clippy::indexing_slicing,
    reason = "profile entries are accessed by indices returned from Vec::position"
)]
impl DetailedProfileStats {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            stages: Vec::new(),
            counters: Vec::new(),
        }
    }

    pub fn record_duration(&mut self, stage: DetailedProfileStage, elapsed: Duration) {
        self.record_ns(stage, elapsed.as_nanos());
    }

    pub fn record_ns(&mut self, stage: DetailedProfileStage, ns: u128) {
        self.stage_mut(stage).record_ns(ns);
    }

    pub fn increment(&mut self, counter: DetailedProfileCounter, value: u64) {
        let slot = self.counter_mut(counter);
        *slot = slot.saturating_add(u128::from(value));
    }

    pub fn merge_from(&mut self, other: &Self) {
        for (stage, stats) in other.stages() {
            self.stage_mut(stage).merge_from(stats);
        }
        for (counter, value) in other.counters() {
            let slot = self.counter_mut(counter);
            *slot = slot.saturating_add(value);
        }
    }

    #[must_use]
    pub fn stage(&self, stage: DetailedProfileStage) -> ProfileStageStats {
        self.stages
            .iter()
            .find(|entry| entry.stage == stage)
            .map_or_else(ProfileStageStats::new, |entry| entry.stats)
    }

    #[must_use]
    pub fn counter(&self, counter: DetailedProfileCounter) -> u128 {
        self.counters
            .iter()
            .find(|entry| entry.counter == counter)
            .map_or(0, |entry| entry.value)
    }

    pub fn stages(&self) -> impl Iterator<Item = (DetailedProfileStage, ProfileStageStats)> + '_ {
        self.stages.iter().map(|entry| (entry.stage, entry.stats))
    }

    pub fn counters(&self) -> impl Iterator<Item = (DetailedProfileCounter, u128)> + '_ {
        self.counters.iter().map(|entry| (entry.counter, entry.value))
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.stages.iter().all(|entry| entry.stats.is_empty()) && self.counters.iter().all(|entry| entry.value == 0)
    }

    #[must_use]
    pub fn render_summary(&self, label: &str, total_stage: DetailedProfileStage) -> String {
        let mut output = String::new();
        let total = self.stage(total_stage);
        let total_ns = total.total_ns();
        output.push_str(&format!(
            "{label} summary: profiled_dispatches={} total_ns={total_ns} avg_ns={}",
            total.count(),
            total.avg_ns()
        ));
        output.push('\n');

        for (stage, stats) in self.stages() {
            if stats.is_empty() {
                continue;
            }
            let pct = if total_ns > 0 {
                (stats.total_ns() as f64 * 100.0) / total_ns as f64
            } else {
                0.0
            };
            output.push_str(&format!(
                "{label} stage: name={} count={} total_ns={} avg_ns={} min_ns={} max_ns={} pct_of_dispatch_total={pct:.2}",
                stage.name(),
                stats.count(),
                stats.total_ns(),
                stats.avg_ns(),
                stats.min_ns(),
                stats.max_ns()
            ));
            output.push('\n');
        }

        for (counter, value) in self.counters() {
            if value == 0 {
                continue;
            }
            output.push_str(&format!("{label} counter: name={} value={value}", counter.name()));
            output.push('\n');
        }

        output
    }

    fn stage_mut(&mut self, stage: DetailedProfileStage) -> &mut ProfileStageStats {
        if let Some(index) = self.stages.iter().position(|entry| entry.stage == stage) {
            return &mut self.stages[index].stats;
        }
        self.stages.push(DetailedStageEntry {
            stage,
            stats: ProfileStageStats::new(),
        });
        let index = self.stages.len() - 1;
        &mut self.stages[index].stats
    }

    fn counter_mut(&mut self, counter: DetailedProfileCounter) -> &mut u128 {
        if let Some(index) = self.counters.iter().position(|entry| entry.counter == counter) {
            return &mut self.counters[index].value;
        }
        self.counters.push(DetailedCounterEntry { counter, value: 0 });
        let index = self.counters.len() - 1;
        &mut self.counters[index].value
    }
}

/// Scoped detailed profile session that clears thread-local state on drop.
/// The guard must stay on the thread whose profile it owns.
///
/// ```
/// use rsvz_profiling::{DetailedProfileGuard, detailed_profile_enabled};
/// let guard = DetailedProfileGuard::start();
/// assert!(detailed_profile_enabled());
/// assert!(guard.finish().is_some());
/// assert!(!detailed_profile_enabled());
/// ```
///
/// ```compile_fail
/// use rsvz_profiling::DetailedProfileGuard;
/// fn requires_send<T: Send>() {}
/// requires_send::<DetailedProfileGuard>();
/// ```
///
/// ```compile_fail
/// use rsvz_profiling::DetailedProfileGuard;
/// fn requires_sync<T: Sync>() {}
/// requires_sync::<DetailedProfileGuard>();
/// ```
#[derive(Debug)]
pub struct DetailedProfileGuard {
    active: bool,
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl DetailedProfileGuard {
    #[must_use]
    pub fn start() -> Self {
        start_detailed_profile();
        Self {
            active: true,
            _thread: std::marker::PhantomData,
        }
    }

    #[must_use]
    pub fn finish(mut self) -> Option<DetailedProfileStats> {
        self.take()
    }

    fn take(&mut self) -> Option<DetailedProfileStats> {
        if !self.active {
            return None;
        }
        let profile = take_detailed_profile();
        self.active = false;
        profile
    }
}

impl Drop for DetailedProfileGuard {
    fn drop(&mut self) {
        let _ = self.take();
    }
}

pub fn start_detailed_profile() {
    DETAILED_PROFILE.with(|profile| {
        let mut profile = profile.borrow_mut();
        if profile.is_none() {
            ACTIVE_DETAILED_PROFILES.fetch_add(1, Ordering::Relaxed);
        }
        *profile = Some(DetailedProfileStats::new());
    });
}

#[must_use]
pub fn take_detailed_profile() -> Option<DetailedProfileStats> {
    let profile = DETAILED_PROFILE.with(|profile| profile.borrow_mut().take());
    if profile.is_some() {
        ACTIVE_DETAILED_PROFILES.fetch_sub(1, Ordering::Relaxed);
    }
    profile
}

#[must_use]
pub fn detailed_profile_enabled() -> bool {
    if ACTIVE_DETAILED_PROFILES.load(Ordering::Relaxed) == 0 {
        return false;
    }
    DETAILED_PROFILE.with(|profile| profile.borrow().is_some())
}

pub fn record_detailed_profile_duration(stage: DetailedProfileStage, elapsed: Duration) {
    record_detailed_profile_ns(stage, elapsed.as_nanos());
}

pub fn record_detailed_profile_ns(stage: DetailedProfileStage, ns: u128) {
    if ACTIVE_DETAILED_PROFILES.load(Ordering::Relaxed) == 0 {
        return;
    }
    DETAILED_PROFILE.with(|profile| {
        if let Some(profile) = profile.borrow_mut().as_mut() {
            profile.record_ns(stage, ns);
        }
    });
}

pub fn increment_detailed_profile_counter(counter: DetailedProfileCounter, value: u64) {
    if ACTIVE_DETAILED_PROFILES.load(Ordering::Relaxed) == 0 {
        return;
    }
    DETAILED_PROFILE.with(|profile| {
        if let Some(profile) = profile.borrow_mut().as_mut() {
            profile.increment(counter, value);
        }
    });
}

pub fn measure_detailed_profile<R>(stage: DetailedProfileStage, f: impl FnOnce() -> R) -> R {
    if !detailed_profile_enabled() {
        return f();
    }
    let started = Instant::now();
    let result = f();
    record_detailed_profile_duration(stage, started.elapsed());
    result
}

/// Aggregate coarse frame profile stats.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameProfileStats {
    loop_total: ProfileStageStats,
    run_total: ProfileStageStats,
    backend_update: ProfileStageStats,
    other: ProfileStageStats,
}

impl FrameProfileStats {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            loop_total: ProfileStageStats::new(),
            run_total: ProfileStageStats::new(),
            backend_update: ProfileStageStats::new(),
            other: ProfileStageStats::new(),
        }
    }

    pub fn record_duration(&mut self, stage: FrameProfileStage, elapsed: Duration) {
        self.record_ns(stage, elapsed.as_nanos());
    }

    pub fn record_ns(&mut self, stage: FrameProfileStage, ns: u128) {
        self.stage_mut(stage).record_ns(ns);
    }

    pub fn record_loop(&mut self, loop_ns: u128, run_total_ns: u128, backend_update_ns: u128) {
        self.record_ns(FrameProfileStage::LoopTotal, loop_ns);
        self.record_ns(FrameProfileStage::RunTotal, run_total_ns);
        self.record_ns(FrameProfileStage::BackendUpdate, backend_update_ns);
        self.record_ns(
            FrameProfileStage::Other,
            loop_ns.saturating_sub(run_total_ns.saturating_add(backend_update_ns)),
        );
    }

    pub fn merge_from(&mut self, other: Self) {
        for stage in FRAME_PROFILE_STAGES {
            self.stage_mut(stage).merge_from(other.stage(stage));
        }
    }

    #[must_use]
    pub const fn stage(&self, stage: FrameProfileStage) -> ProfileStageStats {
        match stage {
            FrameProfileStage::LoopTotal => self.loop_total,
            FrameProfileStage::RunTotal => self.run_total,
            FrameProfileStage::BackendUpdate => self.backend_update,
            FrameProfileStage::Other => self.other,
        }
    }

    fn stage_mut(&mut self, stage: FrameProfileStage) -> &mut ProfileStageStats {
        match stage {
            FrameProfileStage::LoopTotal => &mut self.loop_total,
            FrameProfileStage::RunTotal => &mut self.run_total,
            FrameProfileStage::BackendUpdate => &mut self.backend_update,
            FrameProfileStage::Other => &mut self.other,
        }
    }

    #[must_use]
    pub fn render_summary(
        &self, label: &str, reason: impl fmt::Debug, frames_advanced: u64, elapsed: Duration,
    ) -> String {
        let mut output = String::new();
        let elapsed_secs = elapsed.as_secs_f64();
        let approx_fps = if elapsed_secs > 0.0 {
            frames_advanced as f64 / elapsed_secs
        } else {
            0.0
        };
        let loop_total = self.stage(FrameProfileStage::LoopTotal);
        let loop_total_ns = loop_total.total_ns();
        output.push_str(&format!(
            "{label} summary: reason={reason:?} frames_advanced={frames_advanced} profiled_iterations={} elapsed_ms={:.3} approx_fps={approx_fps:.1}",
            loop_total.count(),
            elapsed_secs * 1000.0
        ));
        output.push('\n');
        for stage in FRAME_PROFILE_STAGES {
            let stats = self.stage(stage);
            if stats.is_empty() {
                continue;
            }
            let pct = if loop_total_ns > 0 {
                (stats.total_ns() as f64 * 100.0) / loop_total_ns as f64
            } else {
                0.0
            };
            output.push_str(&format!(
                "{label} stage: name={} count={} total_ns={} avg_ns={} min_ns={} max_ns={} pct_of_iteration_total={pct:.2}",
                stage.name(),
                stats.count(),
                stats.total_ns(),
                stats.avg_ns(),
                stats.min_ns(),
                stats.max_ns()
            ));
            output.push('\n');
        }
        output
    }
}

/// Zero-allocation timing accumulator for one physical frame iteration.
///
/// Disabled recorders avoid calling `Instant::now`; enabled recorders collect
/// run/update durations and commit them through `FrameProfileStats::record_loop`.
#[derive(Debug)]
pub struct FrameIterationRecorder {
    loop_started: Option<Instant>,
    run_total_ns: u128,
    backend_update_ns: u128,
}

impl FrameIterationRecorder {
    #[must_use]
    pub fn start(enabled: bool) -> Self {
        Self {
            loop_started: enabled.then(Instant::now),
            run_total_ns: 0,
            backend_update_ns: 0,
        }
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.loop_started.is_some()
    }

    pub fn record_run_duration(&mut self, elapsed: Duration) {
        self.record_run_ns(elapsed.as_nanos());
    }

    pub fn record_run_ns(&mut self, elapsed_ns: u128) {
        if self.enabled() {
            self.run_total_ns = self.run_total_ns.saturating_add(elapsed_ns);
        }
    }

    pub fn record_backend_update_duration(&mut self, elapsed: Duration) {
        self.record_backend_update_ns(elapsed.as_nanos());
    }

    pub fn record_backend_update_ns(&mut self, elapsed_ns: u128) {
        if self.enabled() {
            self.backend_update_ns = self.backend_update_ns.saturating_add(elapsed_ns);
        }
    }

    pub fn measure_run<R>(&mut self, f: impl FnOnce() -> R) -> R {
        let Some(_loop_started) = self.loop_started else {
            return f();
        };
        let started = Instant::now();
        let result = f();
        self.record_run_duration(started.elapsed());
        result
    }

    pub fn measure_backend_update<R>(&mut self, f: impl FnOnce() -> R) -> R {
        let Some(_loop_started) = self.loop_started else {
            return f();
        };
        let started = Instant::now();
        let result = f();
        self.record_backend_update_duration(started.elapsed());
        result
    }

    /// Commits this iteration when enabled and reports whether a sample was recorded.
    pub fn finish(self, stats: &mut FrameProfileStats) -> bool {
        let Some(started) = self.loop_started else {
            return false;
        };
        self.finish_with_loop_duration(stats, started.elapsed());
        true
    }

    fn finish_with_loop_duration(self, stats: &mut FrameProfileStats, elapsed: Duration) {
        stats.record_loop(elapsed.as_nanos(), self.run_total_ns, self.backend_update_ns);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_STAGE: DetailedProfileStage = DetailedProfileStage::new("test_stage");
    const TEST_COUNTER: DetailedProfileCounter = DetailedProfileCounter::new("test_counter");

    #[test]
    fn record_loop_derives_other_work() {
        let mut stats = FrameProfileStats::new();

        stats.record_loop(100, 30, 50);

        assert_eq!(stats.stage(FrameProfileStage::LoopTotal).total_ns(), 100);
        assert_eq!(stats.stage(FrameProfileStage::RunTotal).total_ns(), 30);
        assert_eq!(stats.stage(FrameProfileStage::BackendUpdate).total_ns(), 50);
        assert_eq!(stats.stage(FrameProfileStage::Other).total_ns(), 20);

        let mut right = FrameProfileStats::new();
        right.record_loop(40, 10, 20);

        stats.merge_from(right);

        assert_eq!(stats.stage(FrameProfileStage::LoopTotal).count(), 2);
        assert_eq!(stats.stage(FrameProfileStage::LoopTotal).total_ns(), 140);
        assert_eq!(stats.stage(FrameProfileStage::Other).total_ns(), 30);
    }

    #[test]
    fn frame_iteration_recorder_classifies_enabled_and_disabled_paths() {
        let mut stats = FrameProfileStats::new();
        let mut enabled = FrameIterationRecorder::start(true);
        enabled.record_run_ns(30);
        enabled.record_backend_update_ns(50);
        assert!(enabled.finish(&mut stats));
        assert_eq!(stats.stage(FrameProfileStage::LoopTotal).count(), 1);
        assert_eq!(stats.stage(FrameProfileStage::RunTotal).total_ns(), 30);
        assert_eq!(stats.stage(FrameProfileStage::BackendUpdate).total_ns(), 50);

        let mut disabled = FrameIterationRecorder::start(false);
        disabled.record_run_ns(100);
        disabled.record_backend_update_ns(100);
        assert!(!disabled.finish(&mut stats));
        assert_eq!(stats.stage(FrameProfileStage::LoopTotal).count(), 1);
    }

    #[test]
    fn detailed_profile_records_stages_and_counters() {
        start_detailed_profile();

        record_detailed_profile_ns(TEST_STAGE, 100);
        increment_detailed_profile_counter(TEST_COUNTER, 2);

        let profile = take_detailed_profile().expect("profile should be active");
        assert_eq!(profile.stage(TEST_STAGE).total_ns(), 100);
        assert_eq!(profile.counter(TEST_COUNTER), 2);
        assert!(take_detailed_profile().is_none());
    }

    #[test]
    fn detailed_profile_guard_clears_state_on_unwind() {
        let _stale_profile = take_detailed_profile();

        let result = std::panic::catch_unwind(|| {
            let _profile = DetailedProfileGuard::start();
            record_detailed_profile_ns(TEST_STAGE, 100);
            panic!("forced detailed profile unwind");
        });

        assert!(result.is_err());
        assert!(!detailed_profile_enabled());
        assert!(take_detailed_profile().is_none());
    }

    #[test]
    fn detailed_profile_merge_keeps_dynamic_keys() {
        let mut left = DetailedProfileStats::new();
        left.record_ns(TEST_STAGE, 100);
        let mut right = DetailedProfileStats::new();
        right.record_ns(TEST_STAGE, 50);
        right.increment(TEST_COUNTER, 3);

        left.merge_from(&right);

        assert_eq!(left.stage(TEST_STAGE).count(), 2);
        assert_eq!(left.stage(TEST_STAGE).total_ns(), 150);
        assert_eq!(left.counter(TEST_COUNTER), 3);
    }
}
