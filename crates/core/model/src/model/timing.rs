//! Backend-neutral wave timing model.

use crate::model::Wave;

/// A time relative to a wave refresh point.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RelativeTime {
    /// Target wave.
    pub wave: Wave,
    /// Frame offset from the wave refresh point.
    pub time: i32,
}

impl RelativeTime {
    /// Creates a relative wave time.
    #[must_use]
    pub const fn new(wave: Wave, time: i32) -> Self {
        Self { wave, time }
    }
}

/// Assumed distance in frames from `wave` refresh to `wave + 1` refresh.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AssumedWavelength {
    pub wave: Wave,
    pub length: i32,
}

/// How a wavelength declaration affects the backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WavelengthMode {
    /// The wavelength is only a non-forcing timeline assumption.
    Assumed,
    /// The wavelength is a timeline basis and must be forced by the backend.
    Forced,
}

/// How observed timing should be checked against a wavelength declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WavelengthValidation {
    /// Let the host/runtime profile decide whether a mismatch is fatal.
    HostDefault,
    /// Use the declaration as a timing basis without mismatch reporting.
    Disabled,
}

impl WavelengthValidation {
    #[must_use]
    pub const fn reports_mismatch(self) -> bool {
        !matches!(self, Self::Disabled)
    }

    #[must_use]
    const fn merged_with(self, other: Self) -> Self {
        match (self, other) {
            (Self::HostDefault, _) | (_, Self::HostDefault) => Self::HostDefault,
            (Self::Disabled, Self::Disabled) => Self::Disabled,
        }
    }
}

/// A configured wave-length basis declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WavelengthDeclaration {
    pub wave: Wave,
    pub length: i32,
    pub mode: WavelengthMode,
    pub validation: WavelengthValidation,
}

impl WavelengthDeclaration {
    /// Creates a non-forcing declaration using host-default validation.
    #[must_use]
    pub const fn assumed(wave: Wave, length: i32) -> Self {
        Self {
            wave,
            length,
            mode: WavelengthMode::Assumed,
            validation: WavelengthValidation::HostDefault,
        }
    }

    /// Creates a forcing declaration whose runtime write is the source of truth.
    #[must_use]
    pub const fn forced(wave: Wave, length: i32) -> Self {
        Self {
            wave,
            length,
            mode: WavelengthMode::Forced,
            validation: WavelengthValidation::Disabled,
        }
    }

    /// Creates a non-forcing timing basis without mismatch validation.
    #[must_use]
    pub const fn basis(wave: Wave, length: i32) -> Self {
        Self {
            wave,
            length,
            mode: WavelengthMode::Assumed,
            validation: WavelengthValidation::Disabled,
        }
    }

    #[must_use]
    pub const fn as_assumed_wavelength(self) -> AssumedWavelength {
        AssumedWavelength {
            wave: self.wave,
            length: self.length,
        }
    }
}

/// Result of merging one wavelength declaration into [`WaveClockState`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WavelengthDeclarationOutcome {
    /// Whether this declaration newly requires a backend forced-refresh write.
    pub force_required: bool,
}

/// Result of comparing an assumption with two observed refresh clocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WavelengthCheck {
    Unknown,
    Matched,
    Mismatched { assumed: i32, actual: i32 },
}

/// Error in backend-neutral wave timing assumptions.
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum WaveTimingError {
    #[error("invalid wave for assumed wavelength: {0:?}")]
    InvalidWave(Wave),
    #[error("assumed wavelength wave {wave:?} exceeds backend total waves {total_waves}")]
    WaveOutOfRange { wave: Wave, total_waves: i32 },
    #[error("forced wavelength for {wave:?} requires known backend total waves")]
    TotalWavesUnknown { wave: Wave },
    #[error("forced wavelength is invalid for final wave {wave:?} with backend total waves {total_waves}")]
    FinalWaveForced { wave: Wave, total_waves: i32 },
    #[error("invalid assumed wavelength for {wave:?}: {length}, expected {min}..={max}")]
    InvalidWavelength {
        wave: Wave,
        length: i32,
        min: i32,
        max: i32,
    },
    #[error("conflicting assumed wavelength for {wave:?}: existing {existing}, requested {requested}")]
    ConflictingWavelength { wave: Wave, existing: i32, requested: i32 },
}

/// One-frame snapshot of backend wave timing state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaveTimingSnapshot {
    /// Current board clock.
    pub clock: i32,
    /// Current wave according to backend semantics.
    pub current_wave: Wave,
    /// Total wave count, if the backend can expose it safely.
    pub total_waves: Option<i32>,
    /// Countdown to the next normal refresh, if available.
    pub refresh_countdown: Option<i32>,
    /// Initial countdown before the first refresh, if available.
    pub initial_countdown: Option<i32>,
    /// Huge-wave countdown, if available.
    pub huge_wave_countdown: Option<i32>,
    /// Level-end countdown, if available.
    pub level_end_countdown: Option<i32>,
}

impl WaveTimingSnapshot {
    /// Creates a snapshot with only clock and current-wave data available.
    #[must_use]
    pub const fn minimal(clock: i32, current_wave: Wave) -> Self {
        Self {
            clock,
            current_wave,
            total_waves: None,
            refresh_countdown: None,
            initial_countdown: None,
            huge_wave_countdown: None,
            level_end_countdown: None,
        }
    }

    /// Whether the backend exposed any countdown data that may later resolve the next wave.
    #[must_use]
    pub const fn has_countdown_data(self) -> bool {
        self.refresh_countdown.is_some()
            || self.initial_countdown.is_some()
            || self.huge_wave_countdown.is_some()
            || self.level_end_countdown.is_some()
    }
}

/// Known wave refresh clocks observed or inferred by Timeline.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WaveClockState {
    refresh_clocks: Vec<(Wave, i32)>,
    assumed_wavelengths: Vec<AssumedWavelength>,
    wavelength_declarations: Vec<WavelengthDeclaration>,
    assumed_refresh_clocks: Vec<(Wave, i32)>,
}

impl WaveClockState {
    /// Creates an empty state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            refresh_clocks: Vec::new(),
            assumed_wavelengths: Vec::new(),
            wavelength_declarations: Vec::new(),
            assumed_refresh_clocks: Vec::new(),
        }
    }

    /// Returns the known refresh clock for a wave.
    #[must_use]
    pub fn refresh_clock(&self, wave: Wave) -> Option<i32> {
        self.observed_refresh_clock(wave)
            .or_else(|| self.assumed_refresh_clock(wave))
    }

    /// Returns an observed refresh clock without consulting assumptions.
    #[must_use]
    pub fn observed_refresh_clock(&self, wave: Wave) -> Option<i32> {
        self.refresh_clocks
            .iter()
            .find_map(|(known_wave, clock)| (*known_wave == wave).then_some(*clock))
    }

    /// Returns a refresh clock derived from assumptions.
    #[must_use]
    pub fn assumed_refresh_clock(&self, wave: Wave) -> Option<i32> {
        self.assumed_refresh_clocks
            .iter()
            .find_map(|(known_wave, clock)| (*known_wave == wave).then_some(*clock))
    }

    /// Records a refresh clock if the wave is not known yet.
    pub fn record_refresh_clock(&mut self, wave: Wave, clock: i32) {
        if self.observed_refresh_clock(wave).is_none() {
            self.refresh_clocks.push((wave, clock));
            self.recompute_assumed_refresh_clocks();
        }
    }

    /// Records a declaration after the schedule layer has applied PvZ timing rules.
    pub fn declare_wavelength_unchecked_bounds(
        &mut self, declaration: WavelengthDeclaration,
    ) -> Result<WavelengthDeclarationOutcome, WaveTimingError> {
        let mut outcome = WavelengthDeclarationOutcome::default();
        if let Some(existing) = self
            .wavelength_declarations
            .iter_mut()
            .find(|existing| existing.wave == declaration.wave)
        {
            if existing.length != declaration.length {
                return Err(WaveTimingError::ConflictingWavelength {
                    wave: declaration.wave,
                    existing: existing.length,
                    requested: declaration.length,
                });
            }
            let was_forced = existing.mode == WavelengthMode::Forced;
            if declaration.mode == WavelengthMode::Forced {
                existing.mode = WavelengthMode::Forced;
            }
            existing.validation = existing.validation.merged_with(declaration.validation);
            outcome.force_required = !was_forced && existing.mode == WavelengthMode::Forced;
        } else {
            outcome.force_required = declaration.mode == WavelengthMode::Forced;
            self.wavelength_declarations.push(declaration);
            self.wavelength_declarations
                .sort_by_key(|declaration| declaration.wave.0);
        }
        self.sync_assumed_wavelengths();
        self.recompute_assumed_refresh_clocks();
        Ok(outcome)
    }

    /// Returns the configured assumed wavelength for `wave`.
    #[must_use]
    pub fn assumed_wavelength(&self, wave: Wave) -> Option<i32> {
        self.assumed_wavelengths
            .iter()
            .find_map(|assumed| (assumed.wave == wave).then_some(assumed.length))
    }

    /// Returns all configured wavelength assumptions.
    #[must_use]
    pub fn assumed_wavelengths(&self) -> &[AssumedWavelength] {
        &self.assumed_wavelengths
    }

    /// Returns the configured declaration for `wave`.
    #[must_use]
    pub fn wavelength_declaration(&self, wave: Wave) -> Option<WavelengthDeclaration> {
        self.wavelength_declarations
            .iter()
            .copied()
            .find(|declaration| declaration.wave == wave)
    }

    /// Returns all configured wavelength declarations.
    #[must_use]
    pub fn wavelength_declarations(&self) -> &[WavelengthDeclaration] {
        &self.wavelength_declarations
    }

    /// Derives assumed refresh clocks forward from an observed wave refresh.
    pub fn derive_assumed_refresh_clocks_from(&mut self, wave: Wave) {
        let Some(mut clock) = self.observed_refresh_clock(wave) else {
            return;
        };
        let mut current = wave;
        for _ in 0..64 {
            let Some(length) = self.assumed_wavelength(current) else {
                break;
            };
            let next = Wave(current.0 + 1);
            let next_clock = clock.saturating_add(length);
            if self.observed_refresh_clock(next).is_none() && self.assumed_refresh_clock(next).is_none() {
                self.assumed_refresh_clocks.push((next, next_clock));
            }
            clock = self.refresh_clock(next).unwrap_or(next_clock);
            current = next;
        }
    }

    /// Checks a configured wavelength against observed clocks when both are known.
    #[must_use]
    pub fn check_assumed_wavelength(&self, wave: Wave) -> WavelengthCheck {
        let Some(assumed) = self.assumed_wavelength(wave) else {
            return WavelengthCheck::Unknown;
        };
        let (Some(start), Some(end)) = (
            self.observed_refresh_clock(wave),
            self.observed_refresh_clock(Wave(wave.0 + 1)),
        ) else {
            return WavelengthCheck::Unknown;
        };
        let actual = end.saturating_sub(start);
        if actual == assumed {
            WavelengthCheck::Matched
        } else {
            WavelengthCheck::Mismatched { assumed, actual }
        }
    }

    /// Clears assumptions and clocks derived from assumptions.
    pub fn clear_assumptions(&mut self) {
        self.assumed_wavelengths.clear();
        self.wavelength_declarations.clear();
        self.assumed_refresh_clocks.clear();
    }

    /// Clears observed and derived refresh clocks while keeping configured assumptions.
    pub fn clear(&mut self) {
        self.refresh_clocks.clear();
        self.assumed_refresh_clocks.clear();
    }

    /// Clears observed clocks, derived clocks, and configured assumptions.
    pub fn clear_all(&mut self) {
        self.clear();
        self.clear_assumptions();
    }

    fn recompute_assumed_refresh_clocks(&mut self) {
        self.assumed_refresh_clocks.clear();
        let observed = self.refresh_clocks.clone();
        for (wave, _clock) in observed {
            self.derive_assumed_refresh_clocks_from(wave);
        }
    }

    fn sync_assumed_wavelengths(&mut self) {
        self.assumed_wavelengths = self
            .wavelength_declarations
            .iter()
            .copied()
            .map(WavelengthDeclaration::as_assumed_wavelength)
            .collect();
    }
}
