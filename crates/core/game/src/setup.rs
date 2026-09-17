//! Backend-neutral opening setup orchestration and registered setup state.
use crate::{
    lineup::{LineupApplyBackend, apply_lineup},
    logic::zombies::{apply_spawn_list, apply_zombie_spawn_request},
};

use crate::backend::{
    CardAppendSelectionBackend, CardSelectionReadBackend, GameSpeedHintBackend, SeedBankReadBackend,
    SeedChooserFastForwardBackend, SpawnScheduleBackend,
};
use crate::lineup::{ApplyLineupError, Lineup, LineupApplyOptions};
use crate::logic::SeedChooserFastForwardOptions;
use crate::logic::fast_forward::FastForwardWindow;
use crate::logic::zombies::{ApplySpawnListError, ApplyZombieSpawnRequestError, ZombieSpawnRequest};
use crate::model::{CardSelection, GameUi, MeasurementSetup, PositiveFiniteF32, ReloadBoundary, ReloadMode, SpawnList};

pub use rsvz_backend_api::opening::{
    SeedChooserOpeningAction, SeedChooserReadiness, classify_seed_chooser_readiness, mix_dancer_clock,
};

/// Controls whether a registered lineup is applied again after script reload.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineupReloadPolicy {
    /// Apply once per world; script reloads keep it consumed, fresh worlds reset it.
    #[default]
    InitialOnly,
    /// Re-apply this lineup every time the script is reloaded for a new board.
    Reapply,
}

/// Pending lineup setup registered by a script.
#[derive(Clone)]
pub struct PendingLineup {
    lineup: Lineup,
    options: LineupApplyOptions,
    policy: LineupReloadPolicy,
    source: String,
}

impl PendingLineup {
    #[must_use]
    pub fn new(
        lineup: Lineup, options: LineupApplyOptions, policy: LineupReloadPolicy, source: impl Into<String>,
    ) -> Self {
        Self {
            lineup,
            options,
            policy,
            source: source.into(),
        }
    }

    #[must_use]
    pub const fn lineup(&self) -> &Lineup {
        &self.lineup
    }

    #[must_use]
    pub const fn options(&self) -> LineupApplyOptions {
        self.options
    }

    #[must_use]
    pub const fn policy(&self) -> LineupReloadPolicy {
        self.policy
    }

    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }
}

/// Script-generation setup consumed by core Opening and ordinary session jobs.
#[derive(Clone, Default)]
pub struct ScriptSetup {
    pub reload_mode: ReloadMode,
    pub game_speed: Option<PositiveFiniteF32>,
    pub spawn_list: Option<SpawnList>,
    pub lineup: Option<PendingLineup>,
    pub zombie_spawn_request: Option<ZombieSpawnRequest>,
    pub seed_chooser_fast_forward: Option<SeedChooserFastForwardOptions>,
    pub fast_forward_windows: Vec<FastForwardWindow>,
    pub desired_cards: Option<Vec<CardSelection>>,
    /// Evaluated once after this opening's scene and spawn setup, before selecting cards.
    pub dynamic_cards: Option<fn() -> crate::runtime::RuntimeResult<Vec<CardSelection>>>,
    pub measurement: MeasurementSetup,
}

/// Opening history for the current world, retained across script reloads.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OpeningState {
    initial_lineup_consumed: bool,
}

impl OpeningState {
    fn apply_lineup(&mut self, lineup: Option<&PendingLineup>) -> Result<bool, ApplyLineupError>
    where
        rsvz_current::CurrentBackend: LineupApplyBackend,
    {
        let Some(lineup) =
            lineup.filter(|lineup| lineup.policy() == LineupReloadPolicy::Reapply || !self.initial_lineup_consumed)
        else {
            return Ok(false);
        };
        apply_lineup(lineup.lineup(), lineup.options())?;
        if lineup.policy() == LineupReloadPolicy::InitialOnly {
            self.initial_lineup_consumed = true;
        }
        Ok(true)
    }

    pub fn reset_world(&mut self) {
        self.initial_lineup_consumed = false;
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ApplyScriptOpeningError {
    #[error("apply opening board setup failed: {0}")]
    Board(#[from] ApplyLineupError),
    #[error("apply opening spawn list failed: {0}")]
    SpawnList(#[from] ApplySpawnListError),
    #[error("apply opening zombie request failed: {0}")]
    ZombieRequest(#[from] ApplyZombieSpawnRequestError),
    #[error("initialize default opening spawn failed: {0}")]
    DefaultSpawn(crate::runtime::RuntimeError),
    #[error("refresh opening spawn preview failed: {0}")]
    SpawnPreview(crate::runtime::RuntimeError),
    #[error("read opening selected cards failed: {0}")]
    ReadCards(crate::runtime::RuntimeError),
    #[error("opening selected card {index} is {actual:?}, expected {expected:?}")]
    CardMismatch {
        index: usize,
        expected: CardSelection,
        actual: CardSelection,
    },
    #[error("append opening card failed: {0}")]
    AppendCard(crate::runtime::RuntimeError),
    #[error("apply opening game-speed hint failed: {0}")]
    GameSpeed(crate::runtime::RuntimeError),
    #[error("apply opening measurement modifier failed: {0}")]
    MeasurementModifier(crate::runtime::RuntimeError),
    #[error("request opening seed-chooser fast-forward failed: {0}")]
    SeedChooserFastForward(crate::runtime::RuntimeError),
}

/// Applies the chooser-independent part of one backend-neutral opening.
pub fn prepare_script_opening(
    setup: &ScriptSetup, seed: u64, state: &mut OpeningState,
) -> Result<(), ApplyScriptOpeningError>
where
    rsvz_current::CurrentBackend:
        LineupApplyBackend + SpawnScheduleBackend + GameSpeedHintBackend + SeedChooserFastForwardBackend,
{
    state.apply_lineup(setup.lineup.as_ref())?;
    apply_game_speed_hint(setup.game_speed).map_err(|error| ApplyScriptOpeningError::GameSpeed(error.into()))?;

    crate::access::with_backend(|backend| {
        if let Some(spawn_list) = setup.spawn_list.as_ref() {
            apply_spawn_list(spawn_list)?;
        } else if let Some(request) = setup.zombie_spawn_request.as_ref() {
            apply_zombie_spawn_request(request, seed)?;
        } else {
            backend
                .pick_spawn_list()
                .map_err(|error| ApplyScriptOpeningError::DefaultSpawn(error.into()))?;
        }
        backend
            .refresh_spawn_preview()
            .map_err(|error| ApplyScriptOpeningError::SpawnPreview(error.into()))?;

        if let Some(options) = setup.seed_chooser_fast_forward {
            backend
                .request_seed_chooser_fast_forward(options)
                .map_err(|error| ApplyScriptOpeningError::SeedChooserFastForward(error.into()))?;
        }

        Ok(())
    })
}

/// Completes one prepared opening once card-selection actions are safe.
pub fn finish_script_opening(setup: &ScriptSetup) -> Result<(), ApplyScriptOpeningError>
where
    rsvz_current::CurrentBackend: CardSelectionReadBackend + CardAppendSelectionBackend,
{
    crate::access::with_backend(|backend| {
        let desired = setup.desired_cards.as_deref().unwrap_or(&[]);
        let selected = backend
            .selected_card_count()
            .map_err(|error| ApplyScriptOpeningError::ReadCards(error.into()))?;
        for (index, expected) in desired.iter().copied().take(selected).enumerate() {
            let actual = backend
                .selected_card(index)
                .map_err(|error| ApplyScriptOpeningError::ReadCards(error.into()))?
                .selection();
            if actual != expected {
                return Err(ApplyScriptOpeningError::CardMismatch {
                    index,
                    expected,
                    actual,
                });
            }
        }
        for selection in desired.iter().copied().skip(selected) {
            backend
                .select_card(
                    selection
                        .checked()
                        .expect("script card setup is validated during registration"),
                )
                .map_err(|error| ApplyScriptOpeningError::AppendCard(error.into()))?;
        }

        backend
            .finish_card_selection()
            .map_err(|error| ApplyScriptOpeningError::AppendCard(error.into()))?;

        Ok(())
    })
}

/// Applies an optional pre-validated game-speed hint.
pub fn apply_game_speed_hint(speed: Option<PositiveFiniteF32>) -> Result<(), crate::runtime::RuntimeError>
where
    rsvz_current::CurrentBackend: GameSpeedHintBackend,
{
    crate::access::with_backend(|backend| {
        if let Some(speed) = speed {
            backend.set_game_speed(speed).map_err(crate::access::rejected_error)?;
        }
        Ok(())
    })
}

#[must_use]
pub fn classify_reload_boundary(previous: Option<GameUi>, current: GameUi) -> Option<ReloadBoundary> {
    let current_is_main = matches!(current, GameUi::Loading | GameUi::Menu | GameUi::Challenge);
    let previous_was_main = matches!(previous, Some(GameUi::Loading | GameUi::Menu | GameUi::Challenge));
    if current_is_main && previous.is_some() && !previous_was_main {
        Some(ReloadBoundary::MainUi)
    } else if current == GameUi::LevelIntro && previous == Some(GameUi::Playing) {
        Some(ReloadBoundary::FightUi)
    } else {
        None
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VerifySelectedCardsError {
    #[error("read active card selection failed: {0}")]
    Backend(crate::runtime::RuntimeError),
    #[error("active board has fewer card slots than the script opening selection")]
    TooFew,
    #[error("active board has more card slots than the script opening selection")]
    TooMany,
    #[error("active board card {index} is {actual:?}, expected {expected:?}")]
    Mismatch {
        index: usize,
        expected: CardSelection,
        actual: CardSelection,
    },
}

pub fn verify_selected_cards(expected: &[CardSelection]) -> Result<(), VerifySelectedCardsError>
where
    rsvz_current::CurrentBackend: SeedBankReadBackend,
{
    crate::access::with_backend(|backend| {
        let mut actual = backend
            .seeds()
            .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
        for (index, expected) in expected.iter().copied().enumerate() {
            let seed = actual.next().ok_or(VerifySelectedCardsError::TooFew)?;
            let selected = backend
                .seed_selection(seed)
                .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
            if selected.selection() != expected {
                return Err(VerifySelectedCardsError::Mismatch {
                    index,
                    expected,
                    actual: selected.selection(),
                });
            }
        }
        if actual.next().is_some() {
            return Err(VerifySelectedCardsError::TooMany);
        }
        Ok(())
    })
}

/// Returns whether a registered lineup should be applied for this board.
#[must_use]
pub fn registered_board_setup_pending(lineup: Option<&PendingLineup>, initial_board_setup_applied: bool) -> bool {
    match lineup.map(PendingLineup::policy) {
        Some(LineupReloadPolicy::InitialOnly) => !initial_board_setup_applied,
        Some(LineupReloadPolicy::Reapply) => true,
        None => false,
    }
}

/// Applies a registered lineup, including its target scene, through core policy.
pub fn apply_registered_board_setup(
    lineup: Option<&PendingLineup>, initial_board_setup_applied: bool,
) -> Result<bool, ApplyLineupError>
where
    rsvz_current::CurrentBackend: LineupApplyBackend,
{
    OpeningState {
        initial_lineup_consumed: initial_board_setup_applied,
    }
    .apply_lineup(lineup)
}

mod current;
pub(crate) use current::{AUTO_ENTER, OPENING};
pub use current::{
    auto_enter_enabled, finish_current_opening, lineup, prepare_current_opening, reload, reset_script_setup,
    select_cards, select_cards_with, set_auto_enter, set_game_speed, set_wave_zombies, set_zombies, skip_between,
    skip_seed_chooser, skip_seed_chooser_with_options, skip_until, with_script_setup,
};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reload_boundary_classification_uses_ui_transitions() {
        assert_eq!(
            classify_reload_boundary(Some(GameUi::Playing), GameUi::Menu),
            Some(ReloadBoundary::MainUi)
        );
        assert_eq!(
            classify_reload_boundary(Some(GameUi::Playing), GameUi::LevelIntro),
            Some(ReloadBoundary::FightUi)
        );
        assert_eq!(classify_reload_boundary(Some(GameUi::Menu), GameUi::Menu), None);
    }
}
