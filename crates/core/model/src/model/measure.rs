//! Backend-neutral measurement vocabulary.

use std::num::NonZeroU64;
use std::str::FromStr;
use std::time::Duration;

use crate::model::{Grid, PlantKind, RelativeTime};

/// Backend-neutral measurement modes accepted by script and tooling vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MeasureMode {
    DamageNarrow,
    BroadPass,
    Smash,
    Pogo,
    Refresh,
}

impl MeasureMode {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DamageNarrow => "damage-narrow",
            Self::BroadPass => "broad-pass",
            Self::Smash => "smash",
            Self::Pogo => "pogo",
            Self::Refresh => "refresh",
        }
    }
}

impl FromStr for MeasureMode {
    type Err = MeasureModeParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "damage-narrow" => Ok(Self::DamageNarrow),
            "broad-pass" => Ok(Self::BroadPass),
            "smash" => Ok(Self::Smash),
            "pogo" => Ok(Self::Pogo),
            "refresh" => Ok(Self::Refresh),
            _ => Err(MeasureModeParseError {
                value: value.to_owned(),
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid measurement mode {value:?}; expected refresh, damage-narrow, broad-pass, smash, or pogo")]
pub struct MeasureModeParseError {
    value: String,
}

/// A measurement limit that is valid by construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MeasureLimit(MeasureLimitKind);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum MeasureLimitKind {
    Trials(NonZeroU64),
    Duration(Duration),
}

impl MeasureLimit {
    pub fn trials(value: u64) -> Result<Self, MeasureLimitError> {
        NonZeroU64::new(value)
            .map(|value| Self(MeasureLimitKind::Trials(value)))
            .ok_or(MeasureLimitError::ZeroTrials)
    }

    pub fn duration(value: Duration) -> Result<Self, MeasureLimitError> {
        if value.is_zero() {
            Err(MeasureLimitError::ZeroDuration)
        } else {
            Ok(Self(MeasureLimitKind::Duration(value)))
        }
    }

    #[must_use]
    pub const fn trial_target(self) -> Option<u64> {
        match self.0 {
            MeasureLimitKind::Trials(value) => Some(value.get()),
            MeasureLimitKind::Duration(_) => None,
        }
    }

    #[must_use]
    pub const fn duration_target(self) -> Option<Duration> {
        match self.0 {
            MeasureLimitKind::Trials(_) => None,
            MeasureLimitKind::Duration(value) => Some(value),
        }
    }

    /// Checks the limit at a physical, fully sealed trial boundary.
    #[must_use]
    pub fn reached_at_trial_boundary(self, completed_trials: u64, elapsed: Duration) -> bool {
        match self.0 {
            MeasureLimitKind::Trials(target) => completed_trials >= target.get(),
            MeasureLimitKind::Duration(target) => elapsed >= target,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MeasureLimitError {
    #[error("measurement trial limit must be greater than zero")]
    ZeroTrials,
    #[error("measurement duration limit must be greater than zero")]
    ZeroDuration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MeasurementEnd {
    Completed,
    Aborted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MeasurementTrialCounts {
    pub requested_trials: Option<u64>,
    pub attempted_trials: u64,
    pub invalid_trials: u64,
    pub aborted_unrun_trials: u64,
}

/// Plant selector used by measurement protection policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProtectTarget {
    /// Protect the key plant layer at this grid.
    Grid(Grid),
    /// Protect only a plant of the given kind at this grid.
    Plant { grid: Grid, kind: PlantKind },
}

impl ProtectTarget {
    #[must_use]
    pub const fn grid(grid: Grid) -> Self {
        Self::Grid(grid)
    }

    #[must_use]
    pub const fn plant(grid: Grid, kind: PlantKind) -> Self {
        Self::Plant { grid, kind }
    }
}

/// Registration-time operation applied to the measurement protection policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtectionEdit {
    Add(Vec<ProtectTarget>),
    Remove(Vec<ProtectTarget>),
    Only(Vec<ProtectTarget>),
}

/// Common-zombie dance gait used by refresh measurements.
///
/// It applies only to normal, conehead, and buckethead zombies and is unrelated
/// to [`ZombieKind::Dancing`](crate::model::ZombieKind::Dancing).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RefreshDance {
    /// Compatibility spelling for disabled dance.
    #[default]
    Unchanged,
    /// Disable common-zombie dance during refresh trials.
    None,
    /// Use the activation-side fast maid/dance cheat.
    Fast,
    /// Use the separation-side slow maid/dance cheat.
    Slow,
}

impl RefreshDance {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unchanged => "unchanged",
            Self::None => "none",
            Self::Fast => "fast",
            Self::Slow => "slow",
        }
    }
}

/// Script-declared configuration for seml-style refresh measurements.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RefreshMeasureConfig {
    enabled: bool,
    assume_activate: bool,
    dance: bool,
    /// SEML's cobDelay flag.
    cob_delay: bool,
}

impl RefreshMeasureConfig {
    #[must_use]
    pub const fn assume_activate(&self) -> bool {
        self.assume_activate
    }

    #[must_use]
    pub const fn dance(&self) -> RefreshDance {
        if !self.dance {
            RefreshDance::None
        } else if self.assume_activate {
            RefreshDance::Fast
        } else {
            RefreshDance::Slow
        }
    }

    #[must_use]
    pub const fn cob_delay(&self) -> bool {
        self.cob_delay
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.enabled
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn enable(&mut self) {
        self.enabled = true;
    }

    pub fn set_assume_activate(&mut self, assume_activate: bool) {
        self.assume_activate = assume_activate;
    }

    pub fn set_dance(&mut self, dance: RefreshDance) {
        self.dance = matches!(dance, RefreshDance::Fast | RefreshDance::Slow);
    }

    pub fn set_cob_delay(&mut self, cob_delay: bool) {
        self.cob_delay = cob_delay;
    }
}

/// Script-declared protection policy for measurement modes that need it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProtectionPolicy {
    protect_unrepairable_from_cards: bool,
    edits: Vec<ProtectionEdit>,
}

/// DamageNarrow's optional long-chewing Imp failure rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImpLeakDiagnosticConfig {
    enabled: bool,
    threshold_cs: u32,
}

impl Default for ImpLeakDiagnosticConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold_cs: 200,
        }
    }
}

impl ImpLeakDiagnosticConfig {
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.enabled
    }

    #[must_use]
    pub const fn threshold_cs(self) -> u32 {
        self.threshold_cs
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    pub fn set_threshold_cs(&mut self, threshold_cs: u32) {
        self.threshold_cs = threshold_cs;
    }
}

impl ProtectionPolicy {
    #[must_use]
    pub const fn protect_unrepairable_from_cards_enabled(&self) -> bool {
        self.protect_unrepairable_from_cards
    }

    pub fn enable_protect_unrepairable_from_cards(&mut self) {
        self.protect_unrepairable_from_cards = true;
    }

    #[must_use]
    pub fn edits(&self) -> &[ProtectionEdit] {
        &self.edits
    }

    /// Mutable registration-time access used to resolve grid selectors against
    /// the applied lineup before hot-path matching begins.
    pub fn edits_mut(&mut self) -> &mut [ProtectionEdit] {
        &mut self.edits
    }

    pub fn add<I>(&mut self, targets: I)
    where
        I: IntoIterator<Item = ProtectTarget>,
    {
        self.edits.push(ProtectionEdit::Add(targets.into_iter().collect()));
    }

    pub fn remove<I>(&mut self, targets: I)
    where
        I: IntoIterator<Item = ProtectTarget>,
    {
        self.edits.push(ProtectionEdit::Remove(targets.into_iter().collect()));
    }

    pub fn only<I>(&mut self, targets: I)
    where
        I: IntoIterator<Item = ProtectTarget>,
    {
        self.edits.push(ProtectionEdit::Only(targets.into_iter().collect()));
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.protect_unrepairable_from_cards && self.edits.is_empty()
    }
}

/// Script setup relevant to measurement runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MeasurementSetup {
    protection: ProtectionPolicy,
    imp_leak: ImpLeakDiagnosticConfig,
    refresh: RefreshMeasureConfig,
    completed_rounds: u32,
    window_end: Option<RelativeTime>,
}

impl Default for MeasurementSetup {
    fn default() -> Self {
        Self {
            protection: ProtectionPolicy::default(),
            imp_leak: ImpLeakDiagnosticConfig::default(),
            refresh: RefreshMeasureConfig::default(),
            completed_rounds: 500,
            window_end: None,
        }
    }
}

impl MeasurementSetup {
    #[must_use]
    pub const fn protection(&self) -> &ProtectionPolicy {
        &self.protection
    }

    pub fn protection_mut(&mut self) -> &mut ProtectionPolicy {
        &mut self.protection
    }

    #[must_use]
    pub const fn imp_leak(&self) -> ImpLeakDiagnosticConfig {
        self.imp_leak
    }

    pub fn imp_leak_mut(&mut self) -> &mut ImpLeakDiagnosticConfig {
        &mut self.imp_leak
    }

    #[must_use]
    pub const fn refresh(&self) -> &RefreshMeasureConfig {
        &self.refresh
    }

    pub fn refresh_mut(&mut self) -> &mut RefreshMeasureConfig {
        &mut self.refresh
    }

    #[must_use]
    pub const fn completed_rounds(&self) -> u32 {
        self.completed_rounds
    }

    pub fn set_completed_rounds(&mut self, completed_rounds: u32) {
        self.completed_rounds = completed_rounds;
    }

    #[must_use]
    pub const fn window_end(&self) -> Option<RelativeTime> {
        self.window_end
    }

    pub fn set_window_end(&mut self, window_end: RelativeTime) -> Result<(), MeasurementWindowError> {
        if window_end.wave.0 < 0 {
            return Err(MeasurementWindowError::InvalidWave(window_end.wave.0));
        }
        self.window_end = Some(window_end);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MeasurementWindowError {
    #[error("measurement window end wave {0} must be non-negative")]
    InvalidWave(i32),
}

/// Ordinary timing facts used to decide whether a Refresh sample is due.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RefreshTimingFact {
    pub wave: i32,
    pub clock: i32,
    pub expected_next_refresh: Option<i32>,
    pub observed_next_refresh: Option<i32>,
    pub wavelength_declared: bool,
}

/// Ordinary wave-health facts sampled by Refresh measurement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RefreshSample {
    pub wave: i32,
    pub initial_hp: u32,
    pub current_hp: u32,
}

/// Core trial classification shared by every backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MeasureTrialOutcome {
    ObjectiveReached,
    RefreshFailure,
    Invalid,
}

impl MeasureTrialOutcome {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ObjectiveReached => "objective_reached",
            Self::RefreshFailure => "refresh_failure",
            Self::Invalid => "invalid",
        }
    }

    #[must_use]
    pub const fn is_valid_sample(self) -> bool {
        !matches!(self, Self::Invalid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measure_limit_rejects_zero_and_checks_boundaries() {
        assert_eq!(MeasureLimit::trials(0), Err(MeasureLimitError::ZeroTrials));
        assert_eq!(
            MeasureLimit::duration(Duration::ZERO),
            Err(MeasureLimitError::ZeroDuration)
        );

        let trials = MeasureLimit::trials(3).expect("non-zero trials");
        assert_eq!(trials.trial_target(), Some(3));
        assert!(!trials.reached_at_trial_boundary(2, Duration::from_secs(100)));
        assert!(trials.reached_at_trial_boundary(3, Duration::ZERO));

        let duration = MeasureLimit::duration(Duration::from_secs(5)).expect("non-zero duration");
        assert_eq!(duration.duration_target(), Some(Duration::from_secs(5)));
        assert!(!duration.reached_at_trial_boundary(100, Duration::from_millis(4_999)));
        assert!(duration.reached_at_trial_boundary(1, Duration::from_secs(5)));
    }

    #[test]
    fn measure_mode_has_one_typed_parser() {
        assert_eq!("refresh".parse(), Ok(MeasureMode::Refresh));
        assert!("unknown".parse::<MeasureMode>().is_err());
    }

    #[test]
    fn measurement_window_rejects_negative_waves_and_last_value_wins() {
        let mut setup = MeasurementSetup::default();
        assert_eq!(
            setup.set_window_end(RelativeTime::new(crate::model::Wave(-1), 0)),
            Err(MeasurementWindowError::InvalidWave(-1))
        );
        setup
            .set_window_end(RelativeTime::new(crate::model::Wave(2), -200))
            .expect("valid endpoint");
        setup
            .set_window_end(RelativeTime::new(crate::model::Wave(4), 0))
            .expect("replacement endpoint");
        assert_eq!(setup.window_end(), Some(RelativeTime::new(crate::model::Wave(4), 0)));
    }

    #[test]
    fn imp_leak_diagnostic_defaults_to_200_and_keeps_enable_independent() {
        let mut setup = MeasurementSetup::default();
        assert!(setup.imp_leak().enabled());
        assert_eq!(setup.imp_leak().threshold_cs(), 200);
        setup.imp_leak_mut().set_enabled(false);
        setup.imp_leak_mut().set_threshold_cs(0);
        assert!(!setup.imp_leak().enabled());
        assert_eq!(setup.imp_leak().threshold_cs(), 0);
    }
}
