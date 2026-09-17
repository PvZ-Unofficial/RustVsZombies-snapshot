use crate::backend::WaveRefreshControlBackend;
use crate::model::{
    AssumedWavelength, Wave, WaveClockState, WaveTimingError, WaveTimingSnapshot, WavelengthCheck,
    WavelengthDeclaration, WavelengthMode,
};
use rsvz_backend_api::error::RuntimeError;
use rsvz_current::CurrentBackend;

use super::{
    IntoAssumedWavelength, Timeline, TimelineAssumptionError, TimelineAssumptionMismatch, TimelineAssumptionWarning,
    TimelineControlTimingError, TimelineRefreshDelay, TimelineTimingViolation,
};

struct PreparedWavelengths {
    wave_clocks: WaveClockState,
    forced_controls: Vec<AssumedWavelength>,
    assumption_warning: Option<TimelineAssumptionWarning>,
}

/// Returns the PvZ/AvZ recommended wavelength bounds for a wave.
pub fn recommended_wavelength_bounds(wave: Wave, total_waves: Option<i32>) -> Result<(i32, i32), WaveTimingError> {
    if wave.0 < 0 {
        return Err(WaveTimingError::InvalidWave(wave));
    }
    if let Some(total_waves) = total_waves {
        if wave.0 > total_waves {
            return Err(WaveTimingError::WaveOutOfRange { wave, total_waves });
        }
        if wave.0 == total_waves {
            return Ok((500, 5999));
        }
    }
    Ok(if wave.0 % 10 == 9 { (1346, 5245) } else { (601, 3100) })
}

/// Returns the best available countdown to the next wave refresh.
#[must_use]
pub fn next_wave_countdown(snapshot: WaveTimingSnapshot) -> Option<i32> {
    if snapshot.total_waves == Some(snapshot.current_wave.0) {
        return snapshot
            .level_end_countdown
            .and_then(|countdown| (countdown > 0).then_some(countdown));
    }

    let countdown = snapshot.refresh_countdown?;
    if snapshot.current_wave.0 == 0 {
        return Some(countdown);
    }
    if snapshot.current_wave.0 % 10 == 9 {
        if countdown <= 5 {
            return snapshot.huge_wave_countdown;
        }
        return (countdown <= 200).then_some(countdown + 745);
    }
    (countdown <= 200).then_some(countdown)
}

/// Returns the inferred refresh clock for the current wave.
#[must_use]
pub fn current_wave_refresh_clock(snapshot: WaveTimingSnapshot) -> Option<i32> {
    match (snapshot.refresh_countdown, snapshot.initial_countdown) {
        (Some(refresh_countdown), Some(initial_countdown))
            if refresh_countdown <= 200 && initial_countdown - refresh_countdown > 400 =>
        {
            None
        }
        (Some(refresh_countdown), Some(initial_countdown)) => {
            Some(snapshot.clock + refresh_countdown - initial_countdown)
        }
        _ => Some(snapshot.clock),
    }
}

fn timer_only_initial_countdown(forced: AssumedWavelength) -> i32 {
    if forced.wave.0 % 10 == 9 {
        forced.length - 745
    } else {
        forced.length
    }
}

impl Timeline {
    /// Registers one assumed wavelength.
    pub fn assume_wavelength(&mut self, wave: Wave, length: i32) -> Result<(), TimelineAssumptionError> {
        let prepared = self.prepare_wavelength_declarations([WavelengthDeclaration::assumed(wave, length)])?;
        self.commit_prepared_wavelengths(prepared);
        Ok(())
    }

    /// Registers assumed wavelengths from `(wave, length)` pairs or explicit values.
    pub fn assume_wavelengths<I, T>(&mut self, items: I) -> Result<(), TimelineAssumptionError>
    where
        I: IntoIterator<Item = T>,
        T: IntoAssumedWavelength,
    {
        let declarations = items.into_iter().map(|item| {
            let assumed = item.into_assumed_wavelength();
            WavelengthDeclaration::assumed(assumed.wave, assumed.length)
        });
        let prepared = self.prepare_wavelength_declarations(declarations)?;
        self.commit_prepared_wavelengths(prepared);
        Ok(())
    }

    /// Registers one forced wavelength and returns declarations requiring backend writes.
    pub fn set_wavelength(&mut self, wave: Wave, length: i32) -> Result<Vec<AssumedWavelength>, TimelineAssumptionError>
    where
        CurrentBackend: WaveRefreshControlBackend,
    {
        self.apply_forced_wavelength_declarations([WavelengthDeclaration::forced(wave, length)], refresh_control)
    }

    /// Registers forced wavelengths from `(wave, length)` pairs or explicit values.
    pub fn set_wavelengths<I, T>(&mut self, items: I) -> Result<Vec<AssumedWavelength>, TimelineAssumptionError>
    where
        I: IntoIterator<Item = T>,
        T: IntoAssumedWavelength,
        CurrentBackend: WaveRefreshControlBackend,
    {
        let declarations = items.into_iter().map(|item| {
            let assumed = item.into_assumed_wavelength();
            WavelengthDeclaration::forced(assumed.wave, assumed.length)
        });
        self.apply_forced_wavelength_declarations(declarations, refresh_control)
    }

    /// Registers unchecked wavelength bases for timeline conversion only.
    pub fn use_wavelength_basis<I, T>(&mut self, items: I) -> Result<(), TimelineAssumptionError>
    where
        I: IntoIterator<Item = T>,
        T: IntoAssumedWavelength,
    {
        let declarations = items.into_iter().map(|item| {
            let assumed = item.into_assumed_wavelength();
            WavelengthDeclaration::basis(assumed.wave, assumed.length)
        });
        let prepared = self.prepare_wavelength_declarations(declarations)?;
        self.commit_prepared_wavelengths(prepared);
        Ok(())
    }

    fn prepare_wavelength_declarations<I>(
        &self, declarations: I,
    ) -> Result<PreparedWavelengths, TimelineAssumptionError>
    where
        I: IntoIterator<Item = WavelengthDeclaration>,
    {
        let mut wave_clocks = self.wave_clocks.clone();
        let mut forced_writes = Vec::new();
        let mut assumption_warning = None;
        for declaration in declarations {
            assumption_warning = self
                .validate_wavelength_declaration_for_total_waves(declaration)?
                .or(assumption_warning);
            let outcome = wave_clocks.declare_wavelength_unchecked_bounds(declaration)?;
            if outcome.force_required {
                forced_writes.push(declaration.as_assumed_wavelength());
            }
        }
        if let Some(mismatch) = Self::registration_mismatch_for_all_declarations(&wave_clocks) {
            return Err(TimelineAssumptionError::AssumptionMismatch(mismatch));
        }
        Ok(PreparedWavelengths {
            wave_clocks,
            forced_controls: forced_writes,
            assumption_warning,
        })
    }

    fn commit_prepared_wavelengths(&mut self, prepared: PreparedWavelengths) {
        debug_assert!(prepared.forced_controls.is_empty());
        self.wave_clocks = prepared.wave_clocks;
        if prepared.assumption_warning.is_some() {
            self.assumption_warning = prepared.assumption_warning;
        }
        self.convert_pending_after_from_known_basis();
    }

    pub(super) fn apply_forced_wavelength_declarations<I, F, C>(
        &mut self, declarations: I, mut make_control: F,
    ) -> Result<Vec<AssumedWavelength>, TimelineAssumptionError>
    where
        I: IntoIterator<Item = WavelengthDeclaration>,
        F: FnMut(Wave, i32) -> C,
        C: FnMut() -> Result<(), RuntimeError> + 'static,
    {
        let PreparedWavelengths {
            wave_clocks,
            forced_controls,
            assumption_warning,
        } = self.prepare_wavelength_declarations(declarations)?;
        for forced in &forced_controls {
            self.validate_registration_with_wave_clocks(crate::model::RelativeTime::new(forced.wave, 1), &wave_clocks)?;
        }

        self.wave_clocks = wave_clocks;
        if assumption_warning.is_some() {
            self.assumption_warning = assumption_warning;
        }
        for forced in &forced_controls {
            let expected_current_wave = forced.wave;
            let initial_countdown = timer_only_initial_countdown(*forced);
            self.commit_internal_control(
                crate::model::RelativeTime::new(forced.wave, 1),
                make_control(expected_current_wave, initial_countdown),
            );
        }
        self.convert_pending_after_from_known_basis();
        Ok(forced_controls)
    }

    fn validate_wavelength_declaration_for_total_waves(
        &self, declaration: WavelengthDeclaration,
    ) -> Result<Option<TimelineAssumptionWarning>, TimelineAssumptionError> {
        if declaration.mode == WavelengthMode::Forced {
            let Some(total_waves) = self.total_waves else {
                return Err(TimelineAssumptionError::Timing(WaveTimingError::TotalWavesUnknown {
                    wave: declaration.wave,
                }));
            };
            if declaration.wave.0 == total_waves {
                return Err(TimelineAssumptionError::Timing(WaveTimingError::FinalWaveForced {
                    wave: declaration.wave,
                    total_waves,
                }));
            }
        }
        let (min, max) = recommended_wavelength_bounds(declaration.wave, self.total_waves)?;
        if declaration.length < min {
            return Err(TimelineAssumptionError::Timing(WaveTimingError::InvalidWavelength {
                wave: declaration.wave,
                length: declaration.length,
                min,
                max,
            }));
        }
        Ok(
            (declaration.length > max).then_some(TimelineAssumptionWarning::WavelengthAboveRecommended {
                wave: declaration.wave,
                length: declaration.length,
                min,
                max,
            }),
        )
    }

    /// Checks one assumption against observed wave refresh clocks.
    #[must_use]
    pub fn check_assumed_wavelength(&self, wave: Wave) -> WavelengthCheck {
        self.wave_clocks.check_assumed_wavelength(wave)
    }

    pub(super) fn update_time(&mut self, snapshot: WaveTimingSnapshot) -> [Option<Wave>; 2] {
        self.current_clock = Some(snapshot.clock);
        self.current_wave = Some(snapshot.current_wave);
        let mut new_observed_waves = [None; 2];
        if let Some(refresh_clock) = current_wave_refresh_clock(snapshot) {
            new_observed_waves[0] = self.record_refresh_clock_observation(snapshot.current_wave, refresh_clock);
        }
        if let Some(countdown) = next_wave_countdown(snapshot) {
            let next_wave = Wave(snapshot.current_wave.0 + 1);
            new_observed_waves[1] = self.record_refresh_clock_observation(next_wave, snapshot.clock + countdown);
        }
        new_observed_waves
    }

    pub(super) fn past_due_internal_control(&self, snapshot: WaveTimingSnapshot) -> Option<TimelineControlTimingError> {
        let control = self.internal_controls.front()?;
        let due_clock = self
            .wave_clocks
            .refresh_clock(control.target.wave)
            .map(|refresh_clock| refresh_clock.saturating_add(control.target.time));
        let missed = snapshot.current_wave.0 > control.target.wave.0
            || due_clock.is_some_and(|due_clock| snapshot.clock > due_clock);
        missed.then_some(TimelineControlTimingError::PastDue {
            target: control.target,
            due_clock,
            current_wave: snapshot.current_wave,
            current_clock: snapshot.clock,
        })
    }

    fn record_refresh_clock_observation(&mut self, wave: Wave, refresh_clock: i32) -> Option<Wave> {
        let is_new = self.wave_clocks.observed_refresh_clock(wave).is_none();
        self.wave_clocks.record_refresh_clock(wave, refresh_clock);
        is_new.then_some(wave)
    }

    pub(super) fn timing_violation_for_new_clocks(
        &self, new_observed_waves: &[Option<Wave>; 2],
    ) -> Option<TimelineTimingViolation> {
        let mut checked = [None; 4];
        let mut count = 0;
        for observed_wave in new_observed_waves.iter().copied().flatten() {
            let mut candidates = [None, Some(observed_wave)];
            if observed_wave.0 > 1 {
                candidates[0] = Some(Wave(observed_wave.0 - 1));
            }
            for wave in candidates.into_iter().flatten() {
                if wave.0 <= 0 || checked.contains(&Some(wave)) {
                    continue;
                }
                checked[count] = Some(wave);
                count += 1;
                if let Some(mismatch) = Self::assumption_mismatch_for_wave(&self.wave_clocks, wave) {
                    return Some(TimelineTimingViolation::WavelengthMismatch(mismatch));
                }
            }
        }
        None
    }

    pub(super) fn refresh_delay_for_snapshot(&self, snapshot: WaveTimingSnapshot) -> Option<TimelineTimingViolation> {
        if !snapshot.has_countdown_data() {
            return None;
        }
        let wave = snapshot.current_wave;
        let declaration = self.wave_clocks.wavelength_declaration(wave)?;
        if !declaration.validation.reports_mismatch() {
            return None;
        }
        let next_wave = Wave(wave.0 + 1);
        if self.wave_clocks.observed_refresh_clock(next_wave).is_some() {
            return None;
        }
        let expected_next_refresh = self.wave_clocks.assumed_refresh_clock(next_wave)?;
        let deadline_clock = expected_next_refresh.saturating_sub(200);
        if snapshot.clock < deadline_clock {
            return None;
        }
        Some(TimelineTimingViolation::RefreshDelay(TimelineRefreshDelay {
            wave,
            assumed: declaration.length,
            expected_next_refresh,
            deadline_clock,
            current_clock: snapshot.clock,
        }))
    }

    fn registration_mismatch_for_all_declarations(wave_clocks: &WaveClockState) -> Option<TimelineAssumptionMismatch> {
        wave_clocks
            .wavelength_declarations()
            .iter()
            .filter(|declaration| declaration.validation.reports_mismatch())
            .find_map(|declaration| Self::assumption_mismatch_for_wave(wave_clocks, declaration.wave))
    }

    fn assumption_mismatch_for_wave(wave_clocks: &WaveClockState, wave: Wave) -> Option<TimelineAssumptionMismatch> {
        let declaration = wave_clocks.wavelength_declaration(wave)?;
        if !declaration.validation.reports_mismatch() {
            return None;
        }
        match wave_clocks.check_assumed_wavelength(wave) {
            WavelengthCheck::Mismatched { assumed, actual } => {
                Some(TimelineAssumptionMismatch { wave, assumed, actual })
            }
            WavelengthCheck::Unknown | WavelengthCheck::Matched => None,
        }
    }

    pub(super) fn should_report_timing_violation(&self) -> bool {
        self.timing_violation_policy.reports_host_default()
    }
}

fn refresh_control(wave: Wave, initial_countdown: i32) -> impl FnMut() -> Result<(), RuntimeError>
where
    CurrentBackend: WaveRefreshControlBackend,
{
    move || {
        rsvz_current::with_backend_shared(|backend| {
            backend
                .commit_timer_only_wave_refresh(wave, initial_countdown)
                .map_err(RuntimeError::from)
        })
        .map_err(|error| RuntimeError::new(error.to_string()))?
    }
}
