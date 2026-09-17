//! Ordinary session-lifetime benchmark task state and artifact.

use std::io::{self, Write};
use std::time::{Duration, Instant};

use rsvz_model::FastForwardOptions;
use rsvz_schedule::tick::TickMeta;

use crate::SessionArtifact;
use crate::runtime::{RuntimeError, RuntimeResult};

/// Battle fast-forward options used by the default Bench shorthand.
pub const DEFAULT_FAST_FORWARD_OPTIONS: FastForwardOptions = FastForwardOptions::aggressive();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BenchOptions {
    pub duration: Duration,
    pub precision: Duration,
    pub fast_forward: bool,
}

impl BenchOptions {
    /// Creates benchmark options with seed-chooser and aggressive battle fast-forward enabled.
    #[must_use]
    pub const fn new(duration: Duration) -> Self {
        Self {
            duration,
            precision: Duration::from_millis(10),
            fast_forward: true,
        }
    }

    #[must_use]
    pub const fn precision(mut self, precision: Duration) -> Self {
        self.precision = precision;
        self
    }

    #[must_use]
    pub const fn fast_forward(mut self, enabled: bool) -> Self {
        self.fast_forward = enabled;
        self
    }

    pub fn validate(self) -> RuntimeResult<Self> {
        if self.duration.is_zero() {
            return Err(RuntimeError::new("bench duration must be greater than zero"));
        }
        if self.precision.is_zero() {
            return Err(RuntimeError::new("bench precision must be greater than zero"));
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BenchReport {
    pub elapsed: Duration,
    pub frames: u64,
    pub completed_rounds: u64,
}

impl BenchReport {
    pub fn artifact(self) -> SessionArtifact {
        SessionArtifact::new(self, merge_reports, write_report_json)
    }
}

fn merge_reports(target: &mut BenchReport, source: BenchReport) -> RuntimeResult<()> {
    target.frames = target.frames.saturating_add(source.frames);
    target.completed_rounds = target.completed_rounds.saturating_add(source.completed_rounds);
    target.elapsed = target.elapsed.max(source.elapsed);
    Ok(())
}

fn write_report_json(report: &BenchReport, writer: &mut dyn Write) -> io::Result<()> {
    write!(
        writer,
        "{{\"kind\":\"bench\",\"elapsed_ns\":{},\"frames\":{},\"completed_rounds\":{}}}",
        report.elapsed.as_nanos(),
        report.frames,
        report.completed_rounds
    )
}

pub struct BenchTask {
    options: BenchOptions,
    started: Instant,
    last_check: Instant,
    frames: u64,
    completed_rounds: u64,
    frames_since_check: u64,
    frames_until_check: u64,
}

impl BenchTask {
    pub fn new(options: BenchOptions) -> RuntimeResult<Self> {
        let options = options.validate()?;
        let now = Instant::now();
        Ok(Self {
            options,
            started: now,
            last_check: now,
            frames: 0,
            completed_rounds: 0,
            frames_since_check: 0,
            frames_until_check: 1,
        })
    }

    /// Returns the final report only at the configured duration boundary.
    pub fn tick(&mut self, meta: TickMeta, completed_rounds: u64) -> Option<BenchReport> {
        self.completed_rounds = completed_rounds;
        if !meta.is_new_frame {
            return None;
        }
        self.frames = self.frames.saturating_add(1);
        self.frames_since_check += 1;
        if self.frames_since_check < self.frames_until_check {
            return None;
        }

        let now = Instant::now();
        let since_check = now.saturating_duration_since(self.last_check);
        if since_check.is_zero() {
            self.frames_until_check = self.frames_until_check.saturating_mul(2).min(1_000_000);
            return None;
        }
        let estimated = self
            .frames_since_check
            .saturating_mul(self.options.precision.as_nanos().try_into().unwrap_or(u64::MAX))
            / u64::try_from(since_check.as_nanos()).unwrap_or(u64::MAX).max(1);
        self.frames_until_check = estimated.clamp(1, 1_000_000);
        self.frames_since_check = 0;
        self.last_check = now;

        let elapsed = now.saturating_duration_since(self.started);
        (elapsed >= self.options.duration).then_some(BenchReport {
            elapsed,
            frames: self.frames,
            completed_rounds: self.completed_rounds,
        })
    }
}

#[cfg(test)]
mod tests {
    use rsvz_model::GameUi;
    use rsvz_schedule::tick::TickPhase;

    use super::*;

    fn tick_meta(is_new_frame: bool) -> TickMeta {
        TickMeta {
            phase: TickPhase::Playing,
            game_ui: Some(GameUi::Playing),
            clock: Some(1),
            is_new_frame,
        }
    }

    #[test]
    fn duration_shorthand_defaults_to_fast_forward() {
        assert!(BenchOptions::new(Duration::from_secs(1)).fast_forward);
        assert_eq!(DEFAULT_FAST_FORWARD_OPTIONS, FastForwardOptions::aggressive());
    }

    #[test]
    fn bench_counts_only_real_updates_and_uses_the_latest_round_total() {
        let mut task = BenchTask::new(BenchOptions::new(Duration::from_millis(1)).precision(Duration::from_nanos(1)))
            .expect("valid bench");
        assert!(task.tick(tick_meta(false), 2).is_none());
        std::thread::sleep(Duration::from_millis(2));

        let report = task
            .tick(tick_meta(true), 3)
            .expect("duration should finish at a real-frame boundary");
        assert_eq!(report.frames, 1);
        assert_eq!(report.completed_rounds, 3);
        assert!(report.elapsed >= Duration::from_millis(1));
    }

    #[test]
    fn bench_artifact_sums_counts_and_keeps_the_longest_elapsed_time() {
        let mut artifact = BenchReport {
            elapsed: Duration::from_millis(2),
            frames: 3,
            completed_rounds: 1,
        }
        .artifact();
        artifact
            .merge_from(
                BenchReport {
                    elapsed: Duration::from_millis(5),
                    frames: 7,
                    completed_rounds: 2,
                }
                .artifact(),
            )
            .expect("bench reports should merge");

        let mut json = Vec::new();
        artifact.write_json(&mut json).expect("bench report should encode");
        assert_eq!(
            String::from_utf8(json).expect("bench JSON should be UTF-8"),
            r#"{"kind":"bench","elapsed_ns":5000000,"frames":10,"completed_rounds":3}"#
        );
    }
}

mod current;
pub use current::{install_bench_session, start};
