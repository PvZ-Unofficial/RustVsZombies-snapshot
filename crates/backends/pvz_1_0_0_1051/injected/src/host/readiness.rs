use std::cell::Cell;

use rsvz_backend_api::backend::{BattleEntryBackend, GameUiBackend};
use rsvz_backend_api::opening::SeedChooserOpeningAction;
use rsvz_model::{BattleConfig, EndlessBattleConfig, GameUi, SceneKind};

use crate::error::Result;
use crate::runtime::Pvz1051Backend;

const CONTINUE_DIALOG_DELAY: u8 = 3;

thread_local! {
    static LAST_UI: Cell<Option<GameUi>> = const { Cell::new(None) };
    static AUTO_ENTRY_REQUESTED: Cell<bool> = const { Cell::new(false) };
    static CONTINUE_DIALOG_COUNTDOWN: Cell<Option<u8>> = const { Cell::new(None) };
    static COMPLETED_ROUNDS: Cell<u64> = const { Cell::new(0) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BeforeDispatch {
    Dispatch { opening_ready: bool },
    AdvanceNative,
}

pub(super) fn before_dispatch(backend: &Pvz1051Backend, stop_requested: bool) -> Result<BeforeDispatch> {
    if stop_requested {
        return Ok(BeforeDispatch::Dispatch { opening_ready: true });
    }
    let ui = backend.game_ui()?;
    let previous = LAST_UI.replace(Some(ui));
    if previous == Some(GameUi::Playing)
        && matches!(ui, GameUi::LevelIntro | GameUi::Award | GameUi::Credit)
        && !crate::impls::reset::world_reset_transition_pending()
    {
        COMPLETED_ROUNDS.set(COMPLETED_ROUNDS.get().saturating_add(1));
    }
    let returned_to_menu = returned_to_menu(previous, ui);
    if returned_to_menu {
        crate::runtime::hook::restore_game_speed()?;
        AUTO_ENTRY_REQUESTED.set(true);
    }

    if ui == GameUi::Playing {
        crate::impls::reset::normalize_pending_world_origin(backend)?;
        crate::impls::reset::restore_pending_world_stage(backend)?;
    }
    if advance_continue_dialog(backend, ui)? {
        return Ok(BeforeDispatch::AdvanceNative);
    }
    if should_advance_native_in_menu(previous, ui, AUTO_ENTRY_REQUESTED.get()) {
        return Ok(BeforeDispatch::AdvanceNative);
    }

    match ui {
        GameUi::LevelIntro => {
            let readiness = backend.seed_chooser_readiness()?;
            match readiness.opening_action() {
                SeedChooserOpeningAction::DispatchReady => Ok(BeforeDispatch::Dispatch { opening_ready: true }),
                SeedChooserOpeningAction::DispatchPrepare => Ok(BeforeDispatch::Dispatch { opening_ready: false }),
                SeedChooserOpeningAction::AdvanceNative => {
                    if readiness.should_cancel_view_lawn() {
                        let _requested = backend.request_cancel_view_lawn_if_possible()?;
                    }
                    Ok(BeforeDispatch::AdvanceNative)
                }
            }
        }
        GameUi::Playing if backend.playing_board_pending_seed_selection()?.is_some() => {
            Ok(BeforeDispatch::AdvanceNative)
        }
        _ => Ok(BeforeDispatch::Dispatch { opening_ready: true }),
    }
}

pub(super) fn after_dispatch(backend: &mut Pvz1051Backend) -> Result<()> {
    let ui = backend.game_ui()?;
    if !is_menu(ui) || AUTO_ENTRY_REQUESTED.replace(true) {
        return Ok(());
    }
    backend.enter_game(BattleConfig::Endless(EndlessBattleConfig::new(SceneKind::Pool)))?;
    CONTINUE_DIALOG_COUNTDOWN.set(Some(CONTINUE_DIALOG_DELAY));
    Ok(())
}

fn advance_continue_dialog(backend: &Pvz1051Backend, ui: GameUi) -> Result<bool> {
    let Some(remaining) = CONTINUE_DIALOG_COUNTDOWN.get() else {
        return Ok(false);
    };
    let remaining = remaining.saturating_sub(1);
    if remaining > 0 {
        CONTINUE_DIALOG_COUNTDOWN.set(Some(remaining));
        return Ok(true);
    }
    let clicked = backend.click_continue_dialog_if_present()?;
    if clicked || is_menu(ui) {
        CONTINUE_DIALOG_COUNTDOWN.set(Some(1));
        return Ok(true);
    }
    CONTINUE_DIALOG_COUNTDOWN.set(None);
    Ok(false)
}

fn is_menu(ui: GameUi) -> bool {
    matches!(ui, GameUi::Loading | GameUi::Menu | GameUi::Challenge)
}

fn returned_to_menu(previous: Option<GameUi>, current: GameUi) -> bool {
    is_menu(current) && previous.is_some_and(|previous| !is_menu(previous))
}

fn should_advance_native_in_menu(previous: Option<GameUi>, current: GameUi, auto_entry_requested: bool) -> bool {
    is_menu(current) && !returned_to_menu(previous, current) && auto_entry_requested
}

pub(super) fn clear() {
    LAST_UI.set(None);
    AUTO_ENTRY_REQUESTED.set(false);
    CONTINUE_DIALOG_COUNTDOWN.set(None);
    COMPLETED_ROUNDS.set(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_entry_is_initial_only_after_a_manual_menu_return() {
        assert!(!returned_to_menu(None, GameUi::Menu));
        assert!(!returned_to_menu(Some(GameUi::Menu), GameUi::Menu));
        assert!(returned_to_menu(Some(GameUi::Playing), GameUi::Menu));
        assert!(returned_to_menu(Some(GameUi::LevelIntro), GameUi::Challenge));
    }

    #[test]
    fn stable_menu_advances_natively_after_the_one_shot_entry_is_consumed() {
        assert!(!should_advance_native_in_menu(None, GameUi::Menu, false));
        assert!(!should_advance_native_in_menu(
            Some(GameUi::Playing),
            GameUi::Menu,
            true
        ));
        assert!(should_advance_native_in_menu(Some(GameUi::Menu), GameUi::Menu, true));
        assert!(!should_advance_native_in_menu(Some(GameUi::Menu), GameUi::Menu, false));
        assert!(!should_advance_native_in_menu(
            Some(GameUi::Menu),
            GameUi::LevelIntro,
            true
        ));
    }
}

pub(super) fn take_completed_rounds() -> u64 {
    COMPLETED_ROUNDS.replace(0)
}
