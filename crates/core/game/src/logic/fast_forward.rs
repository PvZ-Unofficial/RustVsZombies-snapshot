//! Reusable fast-forward request helpers.

pub use rsvz_model::{
    AdvancedPauseMaskColor, AdvancedPauseOptions, FastForwardOptions, FastForwardPerformance, FastForwardRequest,
    FastForwardStopReason, FastForwardUntil, FastForwardWindow, SeedChooserFastForwardOptions,
};
use rsvz_model::{RelativeTime, Wave, WaveClockState, WaveTimingSnapshot};
use rsvz_schedule::timeline::{current_wave_refresh_clock, next_wave_countdown};

/// Evaluates backend fast-forward requests against per-frame snapshots.
pub trait FastForwardRequestExt {
    /// Returns whether the request should continue for the supplied snapshot.
    fn should_continue(&mut self, snapshot: Option<WaveTimingSnapshot>) -> bool;
}

impl FastForwardRequestExt for FastForwardRequest {
    fn should_continue(&mut self, snapshot: Option<WaveTimingSnapshot>) -> bool {
        snapshot.is_none_or(|snapshot| should_continue_fast_forward_until(self.until_mut(), snapshot))
    }
}

/// Evaluates declarative fast-forward windows against per-frame snapshots.
pub trait FastForwardWindowExt {
    /// Returns true after `start` and before `end` using the supplied timing state.
    fn should_start(&self, wave_clocks: &mut WaveClockState, snapshot: WaveTimingSnapshot) -> bool;
}

impl FastForwardWindowExt for FastForwardWindow {
    fn should_start(&self, wave_clocks: &mut WaveClockState, snapshot: WaveTimingSnapshot) -> bool {
        update_wave_clocks(wave_clocks, snapshot);
        let before_end = should_continue_until(self.end, wave_clocks, snapshot);
        if !before_end {
            return false;
        }
        self.start
            .is_none_or(|start| !should_continue_until(start, wave_clocks, snapshot))
    }
}

fn should_continue_fast_forward_until(until: &mut FastForwardUntil, snapshot: WaveTimingSnapshot) -> bool {
    let target = until.target();
    update_wave_clocks(until.wave_clocks_mut(), snapshot);
    should_continue_until(target, until.wave_clocks(), snapshot)
}

fn update_wave_clocks(wave_clocks: &mut WaveClockState, snapshot: WaveTimingSnapshot) {
    if let Some(refresh_clock) = current_wave_refresh_clock(snapshot) {
        wave_clocks.record_refresh_clock(snapshot.current_wave, refresh_clock);
    }
    if let Some(countdown) = next_wave_countdown(snapshot) {
        wave_clocks.record_refresh_clock(Wave(snapshot.current_wave.0 + 1), snapshot.clock + countdown);
    }
}

fn should_continue_until(target: RelativeTime, wave_clocks: &WaveClockState, snapshot: WaveTimingSnapshot) -> bool {
    if snapshot.current_wave.0 > target.wave.0 {
        return false;
    }

    let Some(target_refresh_clock) = wave_clocks.refresh_clock(target.wave) else {
        return true;
    };
    let target_clock = target_refresh_clock.saturating_add(target.time);
    snapshot.clock < target_clock
}

/// Returns the first declarative fast-forward window active at this update.
///
/// Storage and backend calls stay with the top-level current wrapper; this
/// function owns only the reusable timing policy.
#[must_use]
pub fn active_window_request(
    windows: &[FastForwardWindow], wave_clocks: &mut WaveClockState, snapshot: WaveTimingSnapshot,
) -> Option<(usize, FastForwardOptions)> {
    update_wave_clocks(wave_clocks, snapshot);
    windows.iter().enumerate().find_map(|(identity, window)| {
        let before_end = should_continue_until(window.end, wave_clocks, snapshot);
        let after_start = window
            .start
            .is_none_or(|start| !should_continue_until(start, wave_clocks, snapshot));
        (before_end && after_start).then_some((identity, window.options))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn until_continues_until_target_clock() {
        let mut clocks = WaveClockState::new();
        clocks.record_refresh_clock(Wave(1), 100);
        let mut request = FastForwardRequest::until(RelativeTime::new(Wave(1), 5), FastForwardOptions::basic(), clocks);

        assert!(request.should_continue(Some(WaveTimingSnapshot::minimal(104, Wave(1)))));
        assert!(!request.should_continue(Some(WaveTimingSnapshot::minimal(105, Wave(1)))));
    }

    #[test]
    fn until_continues_when_target_refresh_clock_is_unknown() {
        let mut request = FastForwardRequest::until(
            RelativeTime::new(Wave(3), 5),
            FastForwardOptions::basic(),
            WaveClockState::new(),
        );

        assert!(request.should_continue(Some(WaveTimingSnapshot::minimal(105, Wave(1)))));
    }

    #[test]
    fn until_continues_when_snapshot_has_no_timing() {
        let mut request = FastForwardRequest::until(
            RelativeTime::new(Wave(1), 5),
            FastForwardOptions::basic(),
            WaveClockState::new(),
        );

        assert!(request.should_continue(None));
    }

    #[test]
    fn window_starts_and_stops_at_bounds() {
        let mut clocks = WaveClockState::new();
        clocks.record_refresh_clock(Wave(13), 1000);
        clocks.record_refresh_clock(Wave(15), 2202);
        let window = FastForwardWindow::between(
            RelativeTime::new(Wave(13), 0),
            RelativeTime::new(Wave(15), 0),
            FastForwardOptions::aggressive(),
        );

        assert!(!window.should_start(&mut clocks.clone(), WaveTimingSnapshot::minimal(999, Wave(12))));
        assert!(window.should_start(&mut clocks.clone(), WaveTimingSnapshot::minimal(1000, Wave(13))));
        assert!(window.should_start(&mut clocks.clone(), WaveTimingSnapshot::minimal(2201, Wave(14))));
        assert!(!window.should_start(&mut clocks, WaveTimingSnapshot::minimal(2202, Wave(15))));
    }

    #[test]
    fn active_window_identity_changes_at_the_exact_pre_update_boundary() {
        let windows = [
            FastForwardWindow::between(
                RelativeTime::new(Wave(1), 0),
                RelativeTime::new(Wave(1), 5),
                FastForwardOptions::basic(),
            ),
            FastForwardWindow::between(
                RelativeTime::new(Wave(1), 5),
                RelativeTime::new(Wave(1), 10),
                FastForwardOptions::aggressive(),
            ),
        ];
        let mut clocks = WaveClockState::new();
        clocks.record_refresh_clock(Wave(1), 100);

        assert_eq!(
            active_window_request(&windows, &mut clocks, WaveTimingSnapshot::minimal(104, Wave(1)))
                .map(|(identity, _options)| identity),
            Some(0)
        );
        assert_eq!(
            active_window_request(&windows, &mut clocks, WaveTimingSnapshot::minimal(105, Wave(1)))
                .map(|(identity, _options)| identity),
            Some(1)
        );
        assert_eq!(
            active_window_request(&windows, &mut clocks, WaveTimingSnapshot::minimal(110, Wave(1))),
            None
        );
    }
}
