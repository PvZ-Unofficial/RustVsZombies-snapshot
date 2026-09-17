//! Refresh measurement aggregation.
//!
//! Measurement is deliberately not a second frame protocol. A runner starts a
//! trial, samples ordinary timing and wave-health facts around each real
//! update, and finishes the trial immediately when it observes a terminal.

use std::collections::BTreeMap;
use std::io::{self, Write};

use crate::SessionArtifact;
use crate::backend::{
    BattleStatusBackend, CobImpactDelayBackend, CommonZombieDanceBackend, WaveHealthBackend, WaveTimingBackend,
};
use crate::runtime::{RuntimeError, RuntimeFrameDispatch, RuntimeResult};
use rsvz_model::Wave;
use rsvz_model::WaveTimingSnapshot;
use rsvz_model::model::{
    BattleStatus, MeasureLimit, MeasureMode, MeasureTrialOutcome, MeasurementEnd, MeasurementTrialCounts,
    RefreshMeasureConfig, RefreshSample, RefreshTimingFact, RelativeTime, SessionShard, WaveClockState,
    WorldResetConfig,
};
mod report;
pub use report::*;

const REFRESH_WAVES: usize = 20;

/// Backend-neutral action derived from one script-frame dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeasurementDispatchDecision {
    /// No measurement trial is active; the runner keeps its ordinary policy.
    Inactive,
    /// The physical update may run and its post-update facts must be observed.
    ObserveAfterUpdate,
    /// The active trial must finish without treating the next update as a sample.
    Finish(MeasureTrialOutcome),
}

/// Classifies every dispatch variant consistently for both physical runners.
#[must_use]
pub const fn classify_measurement_dispatch(
    dispatch: &RuntimeFrameDispatch, measurement_active: bool, recoverable_error_reported: bool,
) -> MeasurementDispatchDecision {
    if !measurement_active {
        return MeasurementDispatchDecision::Inactive;
    }
    match dispatch {
        RuntimeFrameDispatch::Continue if !recoverable_error_reported => {
            MeasurementDispatchDecision::ObserveAfterUpdate
        }
        RuntimeFrameDispatch::TimingViolation(_) | RuntimeFrameDispatch::TimingControlViolation(_) => {
            MeasurementDispatchDecision::Finish(MeasureTrialOutcome::RefreshFailure)
        }
        RuntimeFrameDispatch::Continue
        | RuntimeFrameDispatch::OperationError(_)
        | RuntimeFrameDispatch::OperationPanic
        | RuntimeFrameDispatch::TimingBackendError(_)
        | RuntimeFrameDispatch::TimingControlError(_)
        | RuntimeFrameDispatch::FrameCallbackError(_)
        | RuntimeFrameDispatch::FrameCallbackPanic => MeasurementDispatchDecision::Finish(MeasureTrialOutcome::Invalid),
    }
}

/// Backend capabilities that affect portable Refresh setup policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RefreshMeasureCapabilities {
    pub cob_delay: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RefreshSetupError {
    #[error("refresh cob-delay setup is unsupported by this backend")]
    CobDelayUnsupported,
}

/// Validates the remaining Refresh-specific setup flags.
pub fn validate_refresh_setup(
    refresh: &RefreshMeasureConfig, capabilities: RefreshMeasureCapabilities,
) -> Result<(), RefreshSetupError> {
    if refresh.cob_delay() && !capabilities.cob_delay {
        return Err(RefreshSetupError::CobDelayUnsupported);
    }
    Ok(())
}

/// Returns `None` for a whole-level measurement and whether a configured
/// endpoint has been reached otherwise. Unknown clocks remain pending.
#[must_use]
pub fn measurement_window_reached(
    window_end: Option<RelativeTime>, current_clock: Option<i32>, clocks: &WaveClockState,
) -> Option<bool> {
    window_end.map(|window_end| {
        let Some(current_clock) = current_clock else {
            return false;
        };
        clocks
            .refresh_clock(window_end.wave)
            .is_some_and(|refresh_clock| current_clock >= refresh_clock.saturating_add(window_end.time))
    })
}

#[derive(Clone, Copy)]
struct ObservedRefreshFrame {
    sample: Option<RefreshSample>,
    battle_status: BattleStatus,
}

/// Runtime-neutral state for one session-lifetime Refresh task.
pub struct RefreshTask {
    state: MeasurementState,
    run: TrialRun,
    refresh: RefreshMeasureConfig,
    world_epoch: u64,
    rounds_seen: u64,
    observed: Option<ObservedRefreshFrame>,
    window_end: Option<RelativeTime>,
}

/// Cold-path command emitted by [`RefreshTask`] for the runtime adapter.
pub enum RefreshTaskControl {
    Continue,
    Reset(WorldResetConfig),
    Complete(SessionArtifact),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RefreshTaskError {
    #[error("completed-round counter moved backwards")]
    CompletedRoundsMovedBackwards,
    #[error("completed-round boundary does not match one active Refresh trial")]
    CompletedRoundBoundaryMismatch,
    #[error("Refresh session reached a trial boundary without an active trial")]
    MissingActiveTrial,
    #[error("level ended before the configured measurement window endpoint was reached")]
    WindowEndUnreached,
    #[error(transparent)]
    Sample(#[from] RefreshSampleError),
    #[error(transparent)]
    TrialCount(#[from] MeasurementTrialCountError),
}

impl RefreshTask {
    #[must_use]
    pub fn new(
        limit: MeasureLimit, refresh: RefreshMeasureConfig, completed_rounds: u32, shard: SessionShard,
        rounds_seen: u64,
    ) -> Self {
        Self::new_with_window(limit, refresh, completed_rounds, shard, rounds_seen, None)
    }

    #[must_use]
    pub fn new_with_window(
        limit: MeasureLimit, refresh: RefreshMeasureConfig, completed_rounds: u32, shard: SessionShard,
        rounds_seen: u64, window_end: Option<RelativeTime>,
    ) -> Self {
        Self {
            state: MeasurementState::new_refresh(),
            run: TrialRun::new(limit, completed_rounds, shard),
            refresh,
            world_epoch: u64::MAX,
            rounds_seen,
            observed: None,
            window_end,
        }
    }

    pub fn set_artifact_limit(&mut self, limit: MeasureLimit) {
        self.run.artifact_limit = limit;
    }

    #[must_use]
    pub fn next_reset_config(&self) -> WorldResetConfig {
        self.run.next_reset_config()
    }

    /// Captures the previous physical update before Timeline and user tasks run.
    pub fn observe_after_update(&mut self, clocks: &WaveClockState) -> Result<(), ObserveRefreshError>
    where
        rsvz_current::CurrentBackend: BattleStatusBackend + WaveHealthBackend + WaveTimingBackend,
    {
        if !self.state.trial_active() || self.observed.is_some() {
            return Ok(());
        }
        rsvz_current::with_backend_shared(|backend| {
            let battle_status = backend
                .battle_status()
                .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
            let sample = if matches!(battle_status, BattleStatus::Lost | BattleStatus::Ended) {
                None
            } else {
                let snapshot =
                    crate::timing::wave_timing().map_err(|error| ObserveRefreshError::Backend(error.into()))?;
                capture_refresh_after_update(&mut self.state, snapshot, clocks)?
            };
            self.observed = Some(ObservedRefreshFrame { sample, battle_status });
            Ok(())
        })
        .map_err(|error| ObserveRefreshError::Backend(RuntimeError::new(error.to_string())))?
    }

    pub fn tick(
        &mut self, world_epoch: u64, completed_rounds: u64, interrupted: Option<MeasureTrialOutcome>,
    ) -> Result<RefreshTaskControl, RefreshTaskError> {
        self.tick_at(world_epoch, None, &WaveClockState::new(), completed_rounds, interrupted)
    }

    pub fn tick_at(
        &mut self, world_epoch: u64, current_clock: Option<i32>, clocks: &WaveClockState, completed_rounds: u64,
        interrupted: Option<MeasureTrialOutcome>,
    ) -> Result<RefreshTaskControl, RefreshTaskError> {
        let completed_round_boundary = if completed_rounds != self.rounds_seen {
            let Some(delta) = completed_rounds.checked_sub(self.rounds_seen) else {
                return Err(RefreshTaskError::CompletedRoundsMovedBackwards);
            };
            self.rounds_seen = completed_rounds;
            if delta != 1 || !self.state.trial_active() {
                return Err(RefreshTaskError::CompletedRoundBoundaryMismatch);
            }
            true
        } else {
            false
        };
        if world_epoch != self.world_epoch {
            self.world_epoch = world_epoch;
            self.observed = None;
            self.state.start_trial();
        }
        if let Some(outcome) = interrupted {
            self.observed = None;
            return self.finish(outcome);
        }
        let observed = self.observed.take();
        if let Some(sample) = observed.and_then(|observed| observed.sample) {
            self.state
                .record_refresh_sample(sample, self.refresh.assume_activate())?;
        }
        let window_reached = measurement_window_reached(self.window_end, current_clock, clocks);
        if let Some(observed) = observed {
            let terminal = match observed.battle_status {
                BattleStatus::ObjectiveReached => Some((MeasureTrialOutcome::ObjectiveReached, true)),
                BattleStatus::Lost => Some((MeasureTrialOutcome::Invalid, false)),
                BattleStatus::Ended => Some((MeasureTrialOutcome::Invalid, true)),
                BattleStatus::Running | BattleStatus::Unknown => None,
            };
            if let Some((outcome, requires_endpoint)) = terminal {
                if requires_endpoint && window_reached == Some(false) {
                    return Err(RefreshTaskError::WindowEndUnreached);
                }
                return self.finish(outcome);
            }
        }
        if window_reached == Some(true) {
            return self.finish(MeasureTrialOutcome::ObjectiveReached);
        }
        if completed_round_boundary {
            if window_reached == Some(false) {
                return Err(RefreshTaskError::WindowEndUnreached);
            }
            return self.finish(MeasureTrialOutcome::ObjectiveReached);
        }
        Ok(RefreshTaskControl::Continue)
    }

    fn finish(&mut self, outcome: MeasureTrialOutcome) -> Result<RefreshTaskControl, RefreshTaskError> {
        if !self.state.trial_active() {
            return Err(RefreshTaskError::MissingActiveTrial);
        }
        self.state.finish_trial(outcome);
        if self.run.finish_trial(self.state.partial().attempted_trials()) {
            return Ok(RefreshTaskControl::Complete(self.state.artifact_with_run(
                self.run.limit,
                MeasurementEnd::Completed,
                self.refresh.clone(),
                self.run.artifact_config(),
                self.window_end,
            )?));
        }
        Ok(RefreshTaskControl::Reset(self.next_reset_config()))
    }
}

/// Applies PUC/SEML refresh-only rules to the current world.
pub fn apply_refresh_rules(refresh: &RefreshMeasureConfig) -> RuntimeResult<()>
where
    rsvz_current::CurrentBackend: CommonZombieDanceBackend + CobImpactDelayBackend,
{
    if !refresh.enabled() {
        return Ok(());
    }
    crate::access::with_backend(|backend| {
        backend
            .set_common_zombie_dance(refresh.dance())
            .map_err(crate::access::rejected_error)?;
        backend
            .set_cob_impact_delay(refresh.cob_delay())
            .map_err(crate::access::rejected_error)?;
        Ok(())
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct RefreshAccumulator {
    count: u64,
    hp_ratio_sum: f64,
    probability_sum: f64,
    accident_rate_sum: f64,
}

impl RefreshAccumulator {
    fn record(&mut self, hp_ratio: f64, probability: f64, accident_rate: f64) {
        self.count = self.count.saturating_add(1);
        self.hp_ratio_sum += hp_ratio;
        self.probability_sum += probability;
        self.accident_rate_sum += accident_rate;
    }

    fn merge(&mut self, other: Self) {
        self.count = self.count.saturating_add(other.count);
        self.hp_ratio_sum += other.hp_ratio_sum;
        self.probability_sum += other.probability_sum;
        self.accident_rate_sum += other.accident_rate_sum;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeasurementPartial {
    satisfied: u64,
    failed: u64,
    invalid: u64,
    refresh_count: u64,
    refresh_hp_ratio_sum: f64,
    refresh_probability_sum: f64,
    refresh_accident_rate_sum: f64,
    refresh_by_wave: [RefreshAccumulator; REFRESH_WAVES],
}

impl Default for MeasurementPartial {
    fn default() -> Self {
        Self {
            satisfied: 0,
            failed: 0,
            invalid: 0,
            refresh_count: 0,
            refresh_hp_ratio_sum: 0.0,
            refresh_probability_sum: 0.0,
            refresh_accident_rate_sum: 0.0,
            refresh_by_wave: [RefreshAccumulator::default(); REFRESH_WAVES],
        }
    }
}

impl MeasurementPartial {
    pub fn merge_from(&mut self, other: &Self) {
        self.satisfied = self.satisfied.saturating_add(other.satisfied);
        self.failed = self.failed.saturating_add(other.failed);
        self.invalid = self.invalid.saturating_add(other.invalid);
        self.refresh_count = self.refresh_count.saturating_add(other.refresh_count);
        self.refresh_hp_ratio_sum += other.refresh_hp_ratio_sum;
        self.refresh_probability_sum += other.refresh_probability_sum;
        self.refresh_accident_rate_sum += other.refresh_accident_rate_sum;
        for (target, source) in self.refresh_by_wave.iter_mut().zip(other.refresh_by_wave) {
            target.merge(source);
        }
    }

    #[must_use]
    pub const fn trial_count(&self, outcome: MeasureTrialOutcome) -> u64 {
        match outcome {
            MeasureTrialOutcome::ObjectiveReached => self.satisfied,
            MeasureTrialOutcome::RefreshFailure => self.failed,
            MeasureTrialOutcome::Invalid => self.invalid,
        }
    }

    #[must_use]
    pub const fn attempted_trials(&self) -> u64 {
        self.satisfied.saturating_add(self.failed).saturating_add(self.invalid)
    }

    #[must_use]
    pub const fn invalid_trials(&self) -> u64 {
        self.invalid
    }

    #[must_use]
    pub fn report(&self, counts: MeasurementTrialCounts, refresh: &RefreshMeasureConfig) -> RefreshReport {
        self.report_with_window(counts, refresh, None)
    }

    pub fn report_with_window(
        &self, counts: MeasurementTrialCounts, refresh: &RefreshMeasureConfig, window_end: Option<RelativeTime>,
    ) -> RefreshReport {
        let valid_trials = self.satisfied.saturating_add(self.failed);
        let mut reasons = BTreeMap::new();
        if self.failed != 0 {
            reasons.insert("refresh_failure".to_owned(), self.failed);
        }
        RefreshReport {
            mode: MeasureMode::Refresh.as_str().to_owned(),
            requested_trials: counts.requested_trials,
            attempted_trials: counts.attempted_trials,
            valid_trials,
            invalid_trials: counts.invalid_trials,
            aborted_unrun_trials: counts.aborted_unrun_trials,
            window_end: window_end.map(MeasurementWindowEndReport::from),
            config: RefreshConfigReport::from_config(refresh),
            refresh: RefreshStats {
                satisfied: self.satisfied,
                failed: self.failed,
                rate: ratio_or_zero(self.satisfied, valid_trials),
                reasons,
            },
            samples: RefreshSampleStats::from_partial(self),
        }
    }

    fn record_outcome(&mut self, outcome: MeasureTrialOutcome) {
        match outcome {
            MeasureTrialOutcome::ObjectiveReached => self.satisfied = self.satisfied.saturating_add(1),
            MeasureTrialOutcome::RefreshFailure => self.failed = self.failed.saturating_add(1),
            MeasureTrialOutcome::Invalid => self.invalid = self.invalid.saturating_add(1),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct TrialSamples {
    data: MeasurementPartial,
    sampled_waves: u32,
    due_wave: Option<i32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum TrialStatus {
    #[default]
    Idle,
    Active,
    Complete(MeasureTrialOutcome),
}

pub struct MeasurementState {
    status: TrialStatus,
    trial: TrialSamples,
    partial: MeasurementPartial,
}

impl Default for MeasurementState {
    fn default() -> Self {
        Self::new_refresh()
    }
}

impl MeasurementState {
    #[must_use]
    pub fn new_refresh() -> Self {
        Self {
            status: TrialStatus::Idle,
            trial: TrialSamples::default(),
            partial: MeasurementPartial::default(),
        }
    }

    /// Starts a trial immediately. An unfinished preceding trial is invalid.
    pub fn start_trial(&mut self) {
        if self.status == TrialStatus::Active {
            self.commit_trial(MeasureTrialOutcome::Invalid);
        }
        self.status = TrialStatus::Active;
        self.trial = TrialSamples::default();
    }

    #[must_use]
    pub const fn completed_outcome(&self) -> Option<MeasureTrialOutcome> {
        match self.status {
            TrialStatus::Complete(outcome) => Some(outcome),
            TrialStatus::Idle | TrialStatus::Active => None,
        }
    }

    #[must_use]
    pub const fn trial_active(&self) -> bool {
        matches!(self.status, TrialStatus::Active)
    }

    #[must_use]
    pub const fn partial(&self) -> &MeasurementPartial {
        &self.partial
    }

    /// Finishes an active trial immediately. An uncaptured due sample makes
    /// the trial invalid; a missing trial does not fabricate an attempt.
    pub fn finish_trial(&mut self, outcome: MeasureTrialOutcome) -> MeasureTrialOutcome {
        match self.status {
            TrialStatus::Complete(outcome) => return outcome,
            TrialStatus::Idle => return MeasureTrialOutcome::Invalid,
            TrialStatus::Active => {}
        }
        let outcome = if self.trial.due_wave.is_none() {
            outcome
        } else {
            MeasureTrialOutcome::Invalid
        };
        self.commit_trial(outcome);
        outcome
    }

    /// Seals an active trial as invalid exactly once.
    pub fn seal_active_invalid(&mut self) -> bool {
        if !self.trial_active() {
            return false;
        }
        self.commit_trial(MeasureTrialOutcome::Invalid);
        true
    }

    pub fn trial_counts(
        &self, limit: MeasureLimit, end: MeasurementEnd,
    ) -> Result<MeasurementTrialCounts, MeasurementTrialCountError> {
        if self.trial_active() {
            return Err(MeasurementTrialCountError::ActiveTrial);
        }
        let attempted_trials = self.partial.attempted_trials();
        let invalid_trials = self.partial.invalid_trials();
        if invalid_trials > attempted_trials {
            return Err(MeasurementTrialCountError::InvalidExceedsAttempted {
                invalid_trials,
                attempted_trials,
            });
        }

        let requested_trials = limit.trial_target();
        let aborted_unrun_trials = match (requested_trials, end) {
            (Some(requested), MeasurementEnd::Completed) if attempted_trials != requested => {
                return Err(MeasurementTrialCountError::CompletedTrialCountMismatch {
                    requested_trials: requested,
                    attempted_trials,
                });
            }
            (Some(requested), MeasurementEnd::Aborted) if attempted_trials > requested => {
                return Err(MeasurementTrialCountError::AttemptedExceedsRequested {
                    requested_trials: requested,
                    attempted_trials,
                });
            }
            (Some(requested), MeasurementEnd::Aborted) => requested - attempted_trials,
            (Some(_), MeasurementEnd::Completed) | (None, _) => 0,
        };
        Ok(MeasurementTrialCounts {
            requested_trials,
            attempted_trials,
            invalid_trials,
            aborted_unrun_trials,
        })
    }

    /// Marks the current wave for one post-update sample when its expected
    /// refresh has entered the 200 cs observation window.
    pub fn refresh_sample_due(&mut self, fact: RefreshTimingFact) -> bool {
        if self.status != TrialStatus::Active || !(1..=REFRESH_WAVES as i32).contains(&fact.wave) {
            return false;
        }
        let bit = 1u32 << (fact.wave - 1);
        if self.trial.sampled_waves & bit != 0 || self.trial.due_wave.is_some() {
            return false;
        }
        let due = fact.expected_next_refresh.is_some_and(|expected| {
            fact.observed_next_refresh.is_none()
                && fact.wavelength_declared
                && fact.clock >= expected.saturating_sub(200)
        });
        if due {
            self.trial.due_wave = Some(fact.wave);
        }
        due
    }

    pub fn record_refresh_sample(
        &mut self, sample: RefreshSample, assume_activate: bool,
    ) -> Result<(), RefreshSampleError> {
        if self.status != TrialStatus::Active {
            return Err(RefreshSampleError::NoActiveTrial);
        }
        if self.trial.due_wave != Some(sample.wave) {
            return Err(RefreshSampleError::UnexpectedWave {
                expected: self.trial.due_wave,
                actual: sample.wave,
            });
        }
        let wave_index = usize::try_from(sample.wave - 1)
            .ok()
            .filter(|wave| *wave < REFRESH_WAVES)
            .ok_or(RefreshSampleError::InvalidWave(sample.wave))?;
        let hp_ratio = if sample.initial_hp == 0 {
            0.0
        } else {
            sample.current_hp as f64 / sample.initial_hp as f64
        };
        let probability = (0.65 - hp_ratio.clamp(0.5, 0.65)) / 0.15;
        let accident_rate = if assume_activate {
            1.0 - probability
        } else {
            probability
        };

        let data = &mut self.trial.data;
        data.refresh_count = data.refresh_count.saturating_add(1);
        data.refresh_hp_ratio_sum += hp_ratio;
        data.refresh_probability_sum += probability;
        data.refresh_accident_rate_sum += accident_rate;
        data.refresh_by_wave[wave_index].record(hp_ratio, probability, accident_rate);
        self.trial.sampled_waves |= 1u32 << wave_index;
        self.trial.due_wave = None;
        Ok(())
    }

    fn commit_trial(&mut self, outcome: MeasureTrialOutcome) {
        self.partial.record_outcome(outcome);
        if outcome.is_valid_sample() {
            self.partial.merge_from(&self.trial.data);
        }
        self.status = TrialStatus::Complete(outcome);
        self.trial.due_wave = None;
    }

    pub fn artifact(
        &self, limit: MeasureLimit, end: MeasurementEnd, refresh: RefreshMeasureConfig,
    ) -> Result<SessionArtifact, MeasurementTrialCountError> {
        self.artifact_with_window(limit, end, refresh, None)
    }

    pub fn artifact_with_window(
        &self, limit: MeasureLimit, end: MeasurementEnd, refresh: RefreshMeasureConfig,
        window_end: Option<RelativeTime>,
    ) -> Result<SessionArtifact, MeasurementTrialCountError> {
        self.artifact_with_run(
            limit,
            end,
            refresh,
            MeasurementRunConfig::new(limit, 0, SessionShard::default()),
            window_end,
        )
    }

    fn artifact_with_run(
        &self, limit: MeasureLimit, end: MeasurementEnd, refresh: RefreshMeasureConfig, run: MeasurementRunConfig,
        window_end: Option<RelativeTime>,
    ) -> Result<SessionArtifact, MeasurementTrialCountError> {
        let counts = self.trial_counts(limit, end)?;
        Ok(self.artifact_with_counts(counts, refresh, run, window_end))
    }

    #[must_use]
    pub fn empty_artifact(requested_trials: Option<u64>, refresh: RefreshMeasureConfig) -> SessionArtifact {
        Self::empty_artifact_with_window(requested_trials, refresh, None)
    }

    #[must_use]
    pub fn empty_artifact_with_window(
        requested_trials: Option<u64>, refresh: RefreshMeasureConfig, window_end: Option<RelativeTime>,
    ) -> SessionArtifact {
        Self::new_refresh().artifact_with_counts(
            MeasurementTrialCounts {
                requested_trials,
                attempted_trials: 0,
                invalid_trials: 0,
                aborted_unrun_trials: requested_trials.unwrap_or(0),
            },
            refresh,
            MeasurementRunConfig::new(
                MeasureLimit::trials(requested_trials.unwrap_or(1).max(1))
                    .expect("the normalized empty-artifact trial count is nonzero"),
                0,
                SessionShard::default(),
            ),
            window_end,
        )
    }

    #[must_use]
    pub fn empty_artifact_with_run(
        requested_trials: Option<u64>, refresh: RefreshMeasureConfig, artifact_limit: MeasureLimit,
        completed_rounds: u32, shard: SessionShard, window_end: Option<RelativeTime>,
    ) -> SessionArtifact {
        Self::new_refresh().artifact_with_counts(
            MeasurementTrialCounts {
                requested_trials,
                attempted_trials: 0,
                invalid_trials: 0,
                aborted_unrun_trials: requested_trials.unwrap_or(0),
            },
            refresh,
            MeasurementRunConfig::new(artifact_limit, completed_rounds, shard),
            window_end,
        )
    }

    fn artifact_with_counts(
        &self, counts: MeasurementTrialCounts, refresh: RefreshMeasureConfig, run: MeasurementRunConfig,
        window_end: Option<RelativeTime>,
    ) -> SessionArtifact {
        RefreshArtifact {
            partial: self.partial.clone(),
            counts,
            refresh,
            run,
            window_end,
        }
        .into_session_artifact()
    }
}

struct RefreshArtifact {
    partial: MeasurementPartial,
    counts: MeasurementTrialCounts,
    refresh: RefreshMeasureConfig,
    run: MeasurementRunConfig,
    window_end: Option<RelativeTime>,
}

impl RefreshArtifact {
    fn into_session_artifact(self) -> SessionArtifact {
        SessionArtifact::new(self, merge_refresh_artifacts, write_refresh_artifact)
    }
}

fn merge_refresh_artifacts(target: &mut RefreshArtifact, source: RefreshArtifact) -> RuntimeResult<()> {
    if target.refresh != source.refresh || target.run != source.run || target.window_end != source.window_end {
        return Err(RuntimeError::new(
            "cannot merge Refresh artifacts with different configuration",
        ));
    }
    target.partial.merge_from(&source.partial);
    target.counts.requested_trials = match (target.counts.requested_trials, source.counts.requested_trials) {
        (Some(target), Some(source)) => Some(target.saturating_add(source)),
        (None, None) => None,
        _ => {
            return Err(RuntimeError::new(
                "cannot merge trial- and duration-limited Refresh artifacts",
            ));
        }
    };
    target.counts.attempted_trials = target
        .counts
        .attempted_trials
        .saturating_add(source.counts.attempted_trials);
    target.counts.invalid_trials = target
        .counts
        .invalid_trials
        .saturating_add(source.counts.invalid_trials);
    target.counts.aborted_unrun_trials = target
        .counts
        .aborted_unrun_trials
        .saturating_add(source.counts.aborted_unrun_trials);
    Ok(())
}

fn write_refresh_artifact(artifact: &RefreshArtifact, writer: &mut dyn Write) -> io::Result<()> {
    let report = artifact
        .partial
        .report_with_window(artifact.counts, &artifact.refresh, artifact.window_end);
    serde_json::to_writer(writer, &report).map_err(io::Error::other)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RefreshSampleError {
    #[error("refresh sample recorded without an active trial")]
    NoActiveTrial,
    #[error("refresh sample wave {actual} does not match pending wave {expected:?}")]
    UnexpectedWave { expected: Option<i32>, actual: i32 },
    #[error("refresh sample wave {0} is outside 1..=20")]
    InvalidWave(i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MeasurementTrialCountError {
    #[error("terminal measurement report cannot be built while a trial is active")]
    ActiveTrial,
    #[error("invalid trial count {invalid_trials} exceeds attempted trial count {attempted_trials}")]
    InvalidExceedsAttempted { invalid_trials: u64, attempted_trials: u64 },
    #[error("completed measurement requested {requested_trials} trials but attempted {attempted_trials}")]
    CompletedTrialCountMismatch {
        requested_trials: u64,
        attempted_trials: u64,
    },
    #[error("measurement attempted {attempted_trials} trials beyond requested {requested_trials}")]
    AttemptedExceedsRequested {
        requested_trials: u64,
        attempted_trials: u64,
    },
}

impl RefreshConfigReport {
    fn from_config(config: &RefreshMeasureConfig) -> Self {
        Self {
            activate: config.assume_activate(),
            dance: config.dance().as_str().to_owned(),
            cob_delay: config.cob_delay(),
        }
    }
}

impl RefreshSampleStats {
    fn from_partial(partial: &MeasurementPartial) -> Self {
        let count = partial.refresh_count as f64;
        let mut by_wave = BTreeMap::new();
        for (index, wave) in partial.refresh_by_wave.iter().copied().enumerate() {
            if wave.count == 0 {
                continue;
            }
            let wave_count = wave.count as f64;
            by_wave.insert(
                format!("w{}", index + 1),
                RefreshWaveStats {
                    count: wave.count,
                    average_hp_ratio: wave.hp_ratio_sum / wave_count,
                    average_refresh_probability: wave.probability_sum / wave_count,
                    average_accident_rate: wave.accident_rate_sum / wave_count,
                },
            );
        }
        Self {
            count: partial.refresh_count,
            average_hp_ratio: divide_or_zero(partial.refresh_hp_ratio_sum, count),
            average_refresh_probability: divide_or_zero(partial.refresh_probability_sum, count),
            average_accident_rate: divide_or_zero(partial.refresh_accident_rate_sum, count),
            by_wave,
        }
    }
}

fn ratio_or_zero(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn divide_or_zero(numerator: f64, denominator: f64) -> f64 {
    if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator
    }
}

/// Performs the single due Refresh observation after a real backend update.
///
/// The backend HP methods are not called until the timing state says a sample
/// is due, and a wave can be sampled at most once per trial.
pub fn observe_refresh_after_update(
    state: &mut MeasurementState, snapshot: WaveTimingSnapshot, clocks: &WaveClockState, assume_activate: bool,
) -> Result<bool, ObserveRefreshError>
where
    rsvz_current::CurrentBackend: WaveHealthBackend,
{
    let Some(sample) = capture_refresh_after_update(state, snapshot, clocks)? else {
        return Ok(false);
    };
    state
        .record_refresh_sample(sample, assume_activate)
        .map_err(ObserveRefreshError::Sample)?;
    Ok(true)
}

/// Captures one due sample without aggregating it. The ordinary Measure task
/// consumes the value after Timeline dispatch.
pub fn capture_refresh_after_update(
    state: &mut MeasurementState, snapshot: WaveTimingSnapshot, clocks: &WaveClockState,
) -> Result<Option<RefreshSample>, ObserveRefreshError>
where
    rsvz_current::CurrentBackend: WaveHealthBackend,
{
    let wave = snapshot.current_wave.0;
    if wave <= 0 {
        return Ok(None);
    }
    let next_wave = Wave(wave.saturating_add(1));
    if !state.refresh_sample_due(RefreshTimingFact {
        wave,
        clock: snapshot.clock,
        expected_next_refresh: clocks.assumed_refresh_clock(next_wave),
        observed_next_refresh: clocks.observed_refresh_clock(next_wave),
        wavelength_declared: clocks.wavelength_declaration(snapshot.current_wave).is_some(),
    }) {
        return Ok(None);
    }

    let (initial_hp, current_hp) = crate::access::with_backend(|backend| -> Result<_, ObserveRefreshError> {
        let initial_hp = backend
            .zombie_health_wave_start()
            .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
        let current_hp = backend
            .total_zombies_health_in_wave(wave - 1)
            .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
        Ok((initial_hp, current_hp))
    })?;
    Ok(Some(RefreshSample {
        wave,
        initial_hp: u32::try_from(initial_hp)
            .map_err(|_negative_hp| ObserveRefreshError::NegativeHealth("current wave initial HP"))?,
        current_hp: u32::try_from(current_hp)
            .map_err(|_negative_hp| ObserveRefreshError::NegativeHealth("current wave HP"))?,
    }))
}

#[derive(Debug, thiserror::Error)]
pub enum ObserveRefreshError {
    #[error("read Refresh wave health failed: {0}")]
    Backend(RuntimeError),
    #[error("{0} was negative")]
    NegativeHealth(&'static str),
    #[error(transparent)]
    Sample(RefreshSampleError),
}

#[cfg(test)]
mod tests;

mod current;
mod endless;
mod round_trace;
pub use current::{
    __current_setup, broad_pass_for, broad_pass_trials, completed_rounds, damage_narrow_for, damage_narrow_trials,
    end_at, imp_leak_detection, imp_leak_threshold, pogo_for, pogo_trials, protect_add, protect_only, protect_remove,
    protect_unrepairable_from_cards, refresh_activate, refresh_cob_delay, refresh_dance, refresh_for, refresh_trials,
    smash_for, smash_trials,
};
pub(crate) use current::{apply_current_refresh_rules, clear_refresh_rule_applier};
pub use endless::{
    ExpectedPassesEnd, expected_passes_for, expected_passes_for_with_end, expected_passes_trials, trace_plant_losses,
};
pub use round_trace::trace_round_end_sun;

mod run;
pub(crate) use run::{MeasurementRunConfig, TrialRun};
pub use run::{global_trial_sequence, local_trial_quota};
