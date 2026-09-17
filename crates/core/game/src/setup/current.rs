//! setup operations for the selected backend.

use crate::lineup::{Lineup, LineupApplyBackend, LineupApplyOptions, LineupReloadPolicy, PendingLineup};
use crate::logic::SeedChooserFastForwardOptions;
use crate::logic::cards::validate_card_selection;
use crate::logic::fast_forward::{FastForwardOptions, FastForwardWindow};
use crate::logic::zombies::{
    DesiredSpawnWaveError, ZombieSpawnRequest, ZombieTypeSelection, average_spawn_list, set_spawn_wave,
};
use crate::runtime::{RuntimeError, RuntimeResult};
use crate::setup::{OpeningState, ScriptSetup};
use rsvz_model::{CardSelection, DEFAULT_SPAWN_WAVES, PositiveFiniteF32, ReloadMode, ZombieKind, ZombieSpawnMode};
use rsvz_schedule::timeline::IntoRelativeTime;
use std::cell::{Cell, RefCell};

thread_local! {
    static SCRIPT_SETUP: RefCell<ScriptSetup> = RefCell::new(ScriptSetup::default());
    pub(crate) static OPENING: RefCell<OpeningState> = RefCell::new(OpeningState::default());
    pub(crate) static AUTO_ENTER: Cell<bool> = const { Cell::new(true) };
}

pub fn with_script_setup<R>(f: impl FnOnce(&mut ScriptSetup) -> R) -> R {
    SCRIPT_SETUP.with_borrow_mut(f)
}

pub fn reset_script_setup() {
    SCRIPT_SETUP.with_borrow_mut(|setup| *setup = ScriptSetup::default());
    crate::measure::clear_refresh_rule_applier();
}

pub fn set_auto_enter(enabled: bool) {
    AUTO_ENTER.set(enabled);
}

#[doc(hidden)]
#[must_use]
pub fn auto_enter_enabled() -> bool {
    AUTO_ENTER.get()
}

pub fn prepare_current_opening(seed: u64) -> RuntimeResult<()>
where
    rsvz_current::CurrentBackend: crate::LineupApplyBackend
        + rsvz_backend_api::backend::SpawnScheduleBackend
        + rsvz_backend_api::backend::GameSpeedHintBackend
        + rsvz_backend_api::backend::SeedChooserFastForwardBackend,
{
    SCRIPT_SETUP.with_borrow(|setup| {
        OPENING.with_borrow_mut(|opening| {
            crate::setup::prepare_script_opening(setup, seed, opening)
                .map_err(|error| RuntimeError::new(error.to_string()))
        })
    })?;
    // Release setup's borrow before entering user code. Opening dispatch calls preparation once.
    let choose = SCRIPT_SETUP.with_borrow(|setup| setup.dynamic_cards);
    if let Some(choose) = choose {
        let cards = choose()?;
        validate_card_selection(&cards).map_err(|error| RuntimeError::new(error.to_string()))?;
        with_script_setup(|setup| setup.desired_cards = Some(cards));
    }
    Ok(())
}

pub fn finish_current_opening() -> RuntimeResult<()>
where
    rsvz_current::CurrentBackend:
        rsvz_backend_api::backend::CardSelectionReadBackend + rsvz_backend_api::backend::CardAppendSelectionBackend,
{
    SCRIPT_SETUP.with_borrow(|setup| {
        crate::setup::finish_script_opening(setup).map_err(|error| RuntimeError::new(error.to_string()))?;
        crate::measure::apply_current_refresh_rules(setup.measurement.refresh())
    })
}

pub fn reload(mode: ReloadMode) {
    with_script_setup(|setup| setup.reload_mode = mode);
}

pub fn select_cards(cards: Vec<CardSelection>) {
    match validate_card_selection(&cards) {
        Ok(()) => with_script_setup(|setup| {
            setup.desired_cards = Some(cards);
            setup.dynamic_cards = None;
        }),
        Err(error) => crate::registration::record_error(error),
    }
}

/// Chooses cards after the current opening's spawn types are initialized.
/// The callback is rerun for each new opening, not for each chooser UI update.
pub fn select_cards_with(choose: fn() -> RuntimeResult<Vec<CardSelection>>) {
    with_script_setup(|setup| {
        setup.desired_cards = None;
        setup.dynamic_cards = Some(choose);
    });
}

pub fn set_zombies(selection: ZombieTypeSelection, mode: ZombieSpawnMode) {
    match ZombieSpawnRequest::new(selection, mode) {
        Ok(request) => {
            with_script_setup(|setup| {
                setup.spawn_list = None;
                setup.zombie_spawn_request = Some(request);
            });
        }
        Err(error) => crate::registration::record_error(error),
    }
}

pub fn set_wave_zombies(wave: i32, zombies: Vec<ZombieKind>) {
    let Some(index) = wave
        .checked_sub(1)
        .and_then(|wave| usize::try_from(wave).ok())
        .filter(|wave| *wave < DEFAULT_SPAWN_WAVES)
    else {
        crate::registration::record_error(format!("wave must be in 1..=20, got {wave}"));
        return;
    };
    let result = with_script_setup(|setup| {
        if setup.spawn_list.is_none() {
            let Some(request) = setup.zombie_spawn_request.as_ref() else {
                return Err(DesiredSpawnWaveError::MissingBaseZombies);
            };
            let Some(base) = request.exact_average_types() else {
                return Err(DesiredSpawnWaveError::UnsupportedBaseZombies);
            };
            setup.spawn_list = Some(average_spawn_list(DEFAULT_SPAWN_WAVES, base.iter().copied()));
        }
        set_spawn_wave(
            setup.spawn_list.as_mut().expect("spawn list initialized above"),
            index,
            zombies,
        );
        setup.zombie_spawn_request = None;
        Ok(())
    });
    if let Err(error) = result {
        crate::registration::record_error(error);
    }
}

fn prepare_lineup_with_policy(source: &str, policy: LineupReloadPolicy) -> Option<Lineup> {
    let source = source.trim();
    match Lineup::parse_auto(source) {
        Ok(parsed) if crate::registration::is_active() => {
            with_script_setup(|setup| {
                setup.lineup = Some(PendingLineup::new(
                    parsed,
                    LineupApplyOptions::default(),
                    policy,
                    source,
                ))
            });
            None
        }
        Ok(parsed) => Some(parsed),
        Err(error) => {
            if crate::registration::is_active() {
                crate::registration::record_error(error);
            } else {
                crate::diagnostics::report_operation_error(RuntimeError::new(error.to_string()));
            }
            None
        }
    }
}

/// Registers an opening lineup, or applies it to the current board immediately.
pub fn lineup(source: &str, policy: LineupReloadPolicy)
where
    rsvz_current::CurrentBackend: LineupApplyBackend,
{
    let Some(parsed) = prepare_lineup_with_policy(source, policy) else {
        return;
    };
    if let Err(error) = crate::lineup::apply_lineup(&parsed, LineupApplyOptions::default()) {
        crate::diagnostics::report_operation_error(RuntimeError::new(error.to_string()));
    }
}

pub fn skip_until(time: impl IntoRelativeTime) {
    let window = FastForwardWindow::until(time.into_relative_time(), FastForwardOptions::aggressive());
    with_script_setup(|setup| setup.fast_forward_windows.push(window));
}

pub fn skip_between(start: impl IntoRelativeTime, end: impl IntoRelativeTime) {
    let window = FastForwardWindow::between(
        start.into_relative_time(),
        end.into_relative_time(),
        FastForwardOptions::aggressive(),
    );
    with_script_setup(|setup| setup.fast_forward_windows.push(window));
}

pub fn set_game_speed(speed: f32) {
    match PositiveFiniteF32::new(speed) {
        Ok(speed) => with_script_setup(|setup| setup.game_speed = Some(speed)),
        Err(error) if crate::registration::is_active() => crate::registration::record_error(error),
        Err(error) => crate::diagnostics::report_operation_error(RuntimeError::new(error.to_string())),
    }
}

pub fn skip_seed_chooser() {
    skip_seed_chooser_with_options(SeedChooserFastForwardOptions::default());
}

pub fn skip_seed_chooser_with_options(options: SeedChooserFastForwardOptions) {
    with_script_setup(|setup| setup.seed_chooser_fast_forward = Some(options));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lineup_defers_during_registration_and_returns_a_runtime_request_otherwise() {
        reset_script_setup();

        crate::registration::run_script(|| {
            assert!(prepare_lineup_with_policy("3,8 1 1 1 0 0", LineupReloadPolicy::InitialOnly).is_none());
            Ok(())
        })
        .expect("registration lineup should be valid");
        assert!(with_script_setup(|setup| setup.lineup.clone()).is_some());

        with_script_setup(|setup| setup.lineup = None);
        assert!(prepare_lineup_with_policy("3,8 1 1 1 0 0", LineupReloadPolicy::InitialOnly).is_some());
        assert!(with_script_setup(|setup| setup.lineup.clone()).is_none());
    }

    #[test]
    fn lineup_registration_accepts_reapply_policy() {
        reset_script_setup();

        crate::registration::run_script(|| {
            let _deferred = prepare_lineup_with_policy("3,8 1 1 1 0 0", LineupReloadPolicy::Reapply);
            Ok(())
        })
        .expect("reapply lineup should register");

        assert_eq!(
            with_script_setup(|setup| setup.lineup.clone())
                .expect("lineup should be set")
                .policy(),
            LineupReloadPolicy::Reapply
        );
    }
}
