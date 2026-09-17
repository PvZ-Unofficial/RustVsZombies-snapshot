use std::ptr::NonNull;

use rsvz_backend_api::opening::{SeedChooserReadiness, classify_seed_chooser_readiness};
use rsvz_model::model::{CardSelection, CheckedCardSelection, GameUi, PlantKind};

use crate::error::{Pvz1051Error, Result};
use crate::raw::kind::{PvzPlantType, plant_kind_from_raw};
use crate::raw::layout as ptrs;

use super::Pvz1051Backend;
use super::backend::{current_app, current_board};

const SEED_FLYING_TO_BANK: i32 = 0;
const SEED_IN_BANK: i32 = 1;

impl Pvz1051Backend {
    #[doc(hidden)]
    pub fn seed_chooser_readiness(&self) -> Result<SeedChooserReadiness> {
        self.ensure_game_ui(GameUi::LevelIntro)?;
        let app = self.app();

        // SAFETY: `app` is non-null and points to LawnApp; level intro construction may leave the
        // board absent, so the pointer is checked before dereferencing.
        let board = unsafe { ptrs::LawnApp::board(app.as_ptr()) };
        let Some(board) = NonNull::new(board) else {
            return Ok(SeedChooserReadiness::MissingBoard);
        };

        // SAFETY: `board` is non-null; cutscene is created during level-intro setup and may be null.
        let cut_scene = unsafe { ptrs::Board::cut_scene(board.as_ptr()) };
        let Some(cut_scene) = NonNull::new(cut_scene) else {
            return Ok(SeedChooserReadiness::MissingCutScene);
        };

        // SAFETY: `app` is non-null; seed chooser may be absent during construction/teardown.
        let seed_chooser = unsafe { ptrs::LawnApp::seed_chooser(app.as_ptr()) };
        let Some(seed_chooser) = NonNull::new(seed_chooser) else {
            return Ok(SeedChooserReadiness::MissingSeedChooser);
        };

        let paused_or_modal = game_is_paused_or_modal_for(app, board);
        // SAFETY: all pointers used below are non-null active level-intro objects. Each accessor
        // reads only copied scalar fields or parent/widget-manager pointers.
        let (
            seed_choosing,
            mouse_visible,
            parent_present,
            widget_manager_present,
            choose_state,
            view_lawn_time,
            seeds_in_flight,
        ) = unsafe {
            (
                ptrs::CutScene::seed_choosing(cut_scene.as_ptr()),
                ptrs::SeedChooserScreen::mouse_visible(seed_chooser.as_ptr()),
                !ptrs::SeedChooserScreen::parent(seed_chooser.as_ptr()).is_null(),
                !ptrs::SeedChooserScreen::widget_manager(seed_chooser.as_ptr()).is_null(),
                ptrs::SeedChooserScreen::choose_state(seed_chooser.as_ptr()),
                ptrs::SeedChooserScreen::view_lawn_time(seed_chooser.as_ptr()),
                ptrs::SeedChooserScreen::seeds_in_flight(seed_chooser.as_ptr()),
            )
        };
        Ok(classify_seed_chooser_readiness(
            seed_choosing,
            paused_or_modal,
            mouse_visible,
            parent_present,
            widget_manager_present,
            choose_state,
            view_lawn_time,
            seeds_in_flight,
        ))
    }

    pub(crate) fn ensure_seed_chooser_ready_for_auto_selection(&self) -> Result<()> {
        let readiness = self.seed_chooser_readiness()?;
        // Native ClickedSeedInChooser accepts multiple simultaneous SEED_FLYING_TO_BANK cards,
        // and PickRandomSeeds lands all of them before closing the chooser.
        if readiness.allows_card_actions() {
            Ok(())
        } else {
            Err(Pvz1051Error::AbiPreconditionFailed(readiness.not_ready_message()))
        }
    }

    #[doc(hidden)]
    pub fn seed_chooser_selected_count(&self) -> Result<usize> {
        self.ensure_game_ui(GameUi::LevelIntro)?;
        let seed_chooser = self.seed_chooser()?;
        // SAFETY: the active LevelIntro seed chooser is non-null; this is one copied scalar.
        let selected = unsafe { ptrs::SeedChooserScreen::seeds_in_bank(seed_chooser.as_ptr()) };
        usize::try_from(selected)
            .map_err(|_| Pvz1051Error::InvariantViolated("seed chooser selected count is negative"))
    }

    pub(crate) fn seed_chooser_selected_card(&self, index: usize) -> Result<CheckedCardSelection> {
        self.ensure_game_ui(GameUi::LevelIntro)?;
        if index >= self.seed_chooser_selected_count()? {
            return Err(Pvz1051Error::InvalidSeedSlot);
        }
        let seed_chooser = self.seed_chooser()?;
        let index =
            i32::try_from(index).map_err(|_| Pvz1051Error::InvariantViolated("seed chooser bank index exceeds i32"))?;
        for raw in 0..=PvzPlantType::Imitator.raw() {
            // SAFETY: the active chooser owns one fixed ChosenSeed entry for every valid PvZ
            // plant type. These are copied scalar reads on the single native thread.
            let (state, bank_index, imitater_type) = unsafe {
                let chosen_seed = ptrs::SeedChooserScreen::chosen_seed(seed_chooser.as_ptr(), raw as usize);
                (
                    ptrs::ChosenSeed::seed_state(chosen_seed),
                    ptrs::ChosenSeed::seed_index_in_bank(chosen_seed),
                    ptrs::ChosenSeed::imitater_type(chosen_seed),
                )
            };
            if !selected_card_matches_bank_index(state, bank_index, index) {
                continue;
            }
            return card_selection_from_chosen_seed(raw, imitater_type);
        }
        Err(Pvz1051Error::InvariantViolated(
            "seed chooser bank index has no selected card",
        ))
    }

    #[doc(hidden)]
    pub fn request_cancel_view_lawn_if_possible(&self) -> Result<bool> {
        let readiness = self.seed_chooser_readiness()?;
        if !readiness.should_cancel_view_lawn() {
            return Ok(false);
        }
        let seed_chooser = self.seed_chooser()?;
        // SAFETY: readiness classified the active chooser as stable View Lawn. This mirrors native
        // CancelLawnView's state change and lets UpdateViewLawn perform the full cleanup.
        unsafe { ptrs::SeedChooserScreen::set_view_lawn_time(seed_chooser.as_ptr(), 251) };
        Ok(true)
    }
}

fn selected_card_matches_bank_index(state: i32, bank_index: i32, expected_index: i32) -> bool {
    matches!(state, SEED_FLYING_TO_BANK | SEED_IN_BANK) && bank_index == expected_index
}

fn card_selection_from_chosen_seed(raw: i32, imitater_type: i32) -> Result<CheckedCardSelection> {
    let packet_kind = PvzPlantType::from_raw(raw)?.to_core();
    let imitater_target = if packet_kind == PlantKind::Imitator {
        Some(plant_kind_from_raw(imitater_type)?)
    } else {
        None
    };
    CardSelection::from_packet_parts(packet_kind, imitater_target)
        .map_err(|_error| Pvz1051Error::InvariantViolated("invalid seed chooser card selection"))
}

pub(crate) fn game_is_paused_or_modal_current() -> Result<bool> {
    Ok(game_is_paused_or_modal_for(current_app()?, current_board()?))
}

fn game_is_paused_or_modal_for(app: NonNull<ptrs::LawnApp>, board: NonNull<ptrs::Board>) -> bool {
    // SAFETY: `board` is active and non-null; paused is a copied bool scalar.
    if unsafe { ptrs::Board::paused(board.as_ptr()) } {
        return true;
    }
    // SAFETY: `app` is active and non-null. The widget-manager and top-window pointers may be null.
    let mouse_window = unsafe { ptrs::LawnApp::mouse_window(app.as_ptr()) };
    if mouse_window.is_null() {
        return false;
    }
    // SAFETY: `mouse_window` was checked for null and points to PvZ's WidgetManager object.
    unsafe { !ptrs::MouseWindow::top_window(mouse_window).is_null() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_prefix_accepts_flying_or_landed_cards_at_their_bank_index() {
        assert!(selected_card_matches_bank_index(SEED_FLYING_TO_BANK, 1, 1));
        assert!(selected_card_matches_bank_index(SEED_IN_BANK, 2, 2));
        assert!(!selected_card_matches_bank_index(SEED_IN_BANK, 2, 1));
        assert!(!selected_card_matches_bank_index(3, 2, 2));
    }

    #[test]
    fn selected_card_preserves_normal_and_imitator_semantics() {
        assert_eq!(
            card_selection_from_chosen_seed(PvzPlantType::Peashooter.raw(), -1)
                .expect("normal selection")
                .selection(),
            CardSelection::Plant(PlantKind::Peashooter)
        );
        assert_eq!(
            card_selection_from_chosen_seed(PvzPlantType::Imitator.raw(), PvzPlantType::Pumpkin.raw())
                .expect("imitator selection")
                .selection(),
            CardSelection::Imitator(PlantKind::Pumpkin)
        );
        assert!(card_selection_from_chosen_seed(PvzPlantType::Imitator.raw(), -1).is_err());
    }
}
