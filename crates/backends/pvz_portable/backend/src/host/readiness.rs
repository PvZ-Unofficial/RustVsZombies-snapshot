use std::cell::Cell;

#[cfg(not(test))]
use rsvz_backend_api::backend::{BattleEntryBackend, GameUiBackend};
use rsvz_backend_api::opening::SeedChooserOpeningAction;
#[cfg(test)]
use rsvz_backend_api::opening::SeedChooserReadiness;
#[cfg(test)]
use rsvz_model::GameUi;
#[cfg(not(test))]
use rsvz_model::{BattleConfig, EndlessBattleConfig, GameUi, SceneKind};

use crate::{PortableBackend, PortableBackendError};

#[cfg(not(test))]
const CONTINUE_DIALOG_DELAY: u8 = 3;

thread_local! {
    static LAST_UI: Cell<Option<GameUi>> = const { Cell::new(None) };
    static AUTO_ENTRY_REQUESTED: Cell<bool> = const { Cell::new(false) };
    static CONTINUE_DIALOG_COUNTDOWN: Cell<Option<u8>> = const { Cell::new(None) };
    #[cfg(test)]
    static TEST_SEED_CHOOSER_READINESS: Cell<SeedChooserReadiness> = const { Cell::new(SeedChooserReadiness::Ready) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BeforeDispatch {
    Dispatch { opening_ready: bool },
    AdvanceNative,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AfterDispatch {
    KeepSkip,
    AdvanceNative,
}

#[cfg(not(test))]
pub(super) fn before_dispatch(
    backend: &PortableBackend, stop_requested: bool,
) -> Result<BeforeDispatch, PortableBackendError> {
    if stop_requested {
        return Ok(BeforeDispatch::Dispatch { opening_ready: true });
    }
    let ui = backend.game_ui()?;
    let previous = LAST_UI.replace(Some(ui));
    if returned_to_menu(previous, ui) {
        pvzp_rs::restore_game_speed()?;
    }
    if advance_continue_dialog(ui)? {
        return Ok(BeforeDispatch::AdvanceNative);
    }
    if should_advance_native_in_menu(previous, ui, AUTO_ENTRY_REQUESTED.get()) {
        return Ok(BeforeDispatch::AdvanceNative);
    }
    if ui == GameUi::Playing {
        let world = backend.world()?;
        if pending_seed_selection(world.level_complete(), world.next_survival_stage_counter()) {
            return Ok(BeforeDispatch::AdvanceNative);
        }
    }
    if ui != GameUi::LevelIntro {
        return Ok(BeforeDispatch::Dispatch { opening_ready: true });
    }
    let readiness = backend.seed_chooser_readiness()?;
    match readiness.opening_action() {
        SeedChooserOpeningAction::DispatchReady => Ok(BeforeDispatch::Dispatch { opening_ready: true }),
        SeedChooserOpeningAction::DispatchPrepare => Ok(BeforeDispatch::Dispatch { opening_ready: false }),
        SeedChooserOpeningAction::AdvanceNative => {
            if readiness.should_cancel_view_lawn() {
                pvzp_rs::seed_chooser_cancel_view_lawn()?;
            }
            Ok(BeforeDispatch::AdvanceNative)
        }
    }
}

#[cfg(test)]
pub(super) fn before_dispatch(
    _backend: &PortableBackend, stop_requested: bool,
) -> Result<BeforeDispatch, PortableBackendError> {
    let readiness = if stop_requested {
        SeedChooserReadiness::Ready
    } else {
        TEST_SEED_CHOOSER_READINESS.get()
    };
    Ok(match readiness.opening_action() {
        SeedChooserOpeningAction::DispatchReady => BeforeDispatch::Dispatch { opening_ready: true },
        SeedChooserOpeningAction::DispatchPrepare => BeforeDispatch::Dispatch { opening_ready: false },
        SeedChooserOpeningAction::AdvanceNative => BeforeDispatch::AdvanceNative,
    })
}

#[cfg(not(test))]
pub(super) fn after_dispatch(backend: &mut PortableBackend) -> Result<AfterDispatch, PortableBackendError> {
    let ui = backend.game_ui()?;
    if !should_attempt_auto_entry(ui) {
        return Ok(AfterDispatch::KeepSkip);
    }
    match backend.enter_game(BattleConfig::Endless(EndlessBattleConfig::new(SceneKind::Pool))) {
        Ok(()) => {
            AUTO_ENTRY_REQUESTED.set(true);
            CONTINUE_DIALOG_COUNTDOWN.set(Some(CONTINUE_DIALOG_DELAY));
            Ok(AfterDispatch::KeepSkip)
        }
        Err(error) if error.is_board_unavailable() => Ok(AfterDispatch::AdvanceNative),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
pub(super) fn after_dispatch(_backend: &mut PortableBackend) -> Result<AfterDispatch, PortableBackendError> {
    Ok(AfterDispatch::KeepSkip)
}

fn should_attempt_auto_entry(ui: GameUi) -> bool {
    is_menu(ui) && !AUTO_ENTRY_REQUESTED.get()
}

#[cfg(not(test))]
fn advance_continue_dialog(ui: GameUi) -> Result<bool, PortableBackendError> {
    let Some(remaining) = CONTINUE_DIALOG_COUNTDOWN.get() else {
        return Ok(false);
    };
    let remaining = remaining.saturating_sub(1);
    if remaining > 0 {
        CONTINUE_DIALOG_COUNTDOWN.set(Some(remaining));
        return Ok(true);
    }
    let clicked = pvzp_rs::click_continue_dialog_if_present()?;
    if clicked || is_menu(ui) {
        CONTINUE_DIALOG_COUNTDOWN.set(Some(1));
        return Ok(true);
    }
    CONTINUE_DIALOG_COUNTDOWN.set(None);
    Ok(false)
}

fn returned_to_menu(previous: Option<GameUi>, current: GameUi) -> bool {
    is_menu(current) && previous.is_some_and(|previous| !is_menu(previous))
}

fn should_advance_native_in_menu(previous: Option<GameUi>, current: GameUi, auto_entry_requested: bool) -> bool {
    is_menu(current) && !returned_to_menu(previous, current) && auto_entry_requested
}

fn is_menu(ui: GameUi) -> bool {
    matches!(ui, GameUi::Loading | GameUi::Menu | GameUi::Challenge)
}

fn pending_seed_selection(level_complete: bool, next_survival_stage_counter: i32) -> bool {
    level_complete || next_survival_stage_counter > 0
}

pub(super) fn clear() {
    LAST_UI.set(None);
    AUTO_ENTRY_REQUESTED.set(false);
    CONTINUE_DIALOG_COUNTDOWN.set(None);
    #[cfg(test)]
    TEST_SEED_CHOOSER_READINESS.set(SeedChooserReadiness::Ready);
}

#[cfg(test)]
pub(super) fn set_seed_chooser_readiness_for_test(readiness: SeedChooserReadiness) {
    TEST_SEED_CHOOSER_READINESS.set(readiness);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_entry_is_initial_only_and_only_for_main_ui() {
        assert_ne!(AfterDispatch::KeepSkip, AfterDispatch::AdvanceNative);
        clear();
        assert!(!should_attempt_auto_entry(GameUi::Playing));
        assert!(should_attempt_auto_entry(GameUi::Loading));
        assert!(should_attempt_auto_entry(GameUi::Menu));
        AUTO_ENTRY_REQUESTED.set(true);
        assert!(!should_attempt_auto_entry(GameUi::Loading));
        assert!(!should_attempt_auto_entry(GameUi::Challenge));
    }

    #[test]
    fn stable_menu_advances_after_the_return_boundary_is_observed() {
        assert_ne!(
            BeforeDispatch::Dispatch { opening_ready: true },
            BeforeDispatch::AdvanceNative
        );
        assert!(!should_advance_native_in_menu(
            Some(GameUi::Playing),
            GameUi::Menu,
            true
        ));
        assert!(should_advance_native_in_menu(Some(GameUi::Menu), GameUi::Menu, true));
        assert!(!should_advance_native_in_menu(Some(GameUi::Menu), GameUi::Menu, false));
    }

    #[test]
    fn survival_stage_transition_advances_until_seed_selection() {
        assert!(pending_seed_selection(false, 1));
        assert!(pending_seed_selection(true, 0));
        assert!(!pending_seed_selection(false, 0));
    }

    #[test]
    fn return_to_menu_detects_only_non_menu_transition() {
        assert!(returned_to_menu(Some(GameUi::Playing), GameUi::Menu));
        assert!(!returned_to_menu(Some(GameUi::Menu), GameUi::Challenge));
        assert!(!returned_to_menu(None, GameUi::Menu));
    }
}
