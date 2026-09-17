#[cfg(test)]
use std::path::Path;

use rsvz_backend_api::backend::{
    AdvancedPauseBackend, BattleEntryBackend, FastForwardBackend, GameSpeedHintBackend, GameUiBackend, MainMenuBackend,
    SeedChooserFastForwardBackend,
};
use rsvz_model::model::{BattleConfig, GameMode, GameUi, PositiveFiniteF32, SceneKind};
use rsvz_model::{
    AdvancedPauseOptions, FastForwardOptions, FastForwardStopReason, RandomStreamKind, SeedChooserFastForwardOptions,
};

use crate::error::{Pvz1051Error, Result};
use crate::raw::{abi as asm, layout as ptrs};
use crate::runtime::Pvz1051Backend;

impl Pvz1051Backend {
    pub(crate) fn enter_saved_battle(&mut self, config: BattleConfig) -> Result<()> {
        self.enter_saved_game(game_mode_for_battle_config(config)?)
    }

    fn enter_saved_game(&mut self, mode: GameMode) -> Result<()> {
        self.enter_game_impl(mode, true)
    }

    fn enter_game_impl(&mut self, mode: GameMode, look_for_saved_game: bool) -> Result<()> {
        let actual = self.game_ui()?;
        let look_for_saved_game = i32::from(look_for_saved_game);
        match actual {
            GameUi::Loading => {
                // SAFETY: the decoded UI is Loading, matching PvZ's loading-complete path before
                // the game selector is discarded.
                unsafe { crate::raw::abi::lawn_app_loading_completed() };
                // SAFETY: the decoded UI started in a menu-like startup state; removing the game
                // selector mirrors the prior enter-game recipe before opening the requested mode.
                unsafe { crate::raw::abi::lawn_app_kill_game_selector() };
            }
            GameUi::Menu => {
                // SAFETY: the decoded UI is Menu, so the selector exists on the normal menu path.
                unsafe { crate::raw::abi::lawn_app_kill_game_selector() };
            }
            GameUi::Challenge => {
                // SAFETY: the decoded UI is Challenge, so PvZ's challenge-screen cleanup is the
                // matching transition before opening the requested mode.
                unsafe { crate::raw::abi::lawn_app_kill_challenge_screen() };
            }
            _ => {
                return Err(Pvz1051Error::WrongGameUi {
                    expected: GameUi::Menu,
                    actual,
                });
            }
        }
        // SAFETY: active level UIs were rejected above; the saved-game flag is selected by
        // the script host's explicit start mode.
        unsafe { crate::raw::abi::lawn_app_pre_new_game(look_for_saved_game, mode.0) };
        Ok(())
    }
}

const SURVIVAL_ENDLESS_SCENES: &[(SceneKind, i32)] = &[
    (SceneKind::Day, 11),
    (SceneKind::Night, 12),
    (SceneKind::Pool, 13),
    (SceneKind::Fog, 14),
    (SceneKind::Roof, 15),
];

#[cfg(test)]
fn saved_survival_scenes_in_dir(userdata_dir: &Path, player_id: u32) -> Vec<SceneKind> {
    SURVIVAL_ENDLESS_SCENES
        .iter()
        .filter_map(|(scene, mode)| {
            let path = userdata_dir.join(format!("game{player_id}_{mode}.dat"));
            path.is_file().then_some(*scene)
        })
        .collect()
}

impl BattleEntryBackend for Pvz1051Backend {
    fn enter_game(&mut self, config: BattleConfig) -> Result<()> {
        self.enter_saved_battle(config)
    }

    fn start_battle(&mut self) -> Result<()> {
        self.ensure_level_intro_ready()?;
        let seed_chooser = self.seed_chooser()?;
        let seed_bank = self.seed_bank()?;
        // SAFETY: `seed_chooser` is non-null and points to the active level-intro seed chooser.
        let selected = unsafe { ptrs::SeedChooserScreen::seeds_in_bank(seed_chooser.as_ptr()) };
        // SAFETY: `seed_bank` is non-null and points to the active Board seed bank.
        let slots = unsafe { ptrs::SeedBank::packet_count(seed_bank.as_ptr()) };
        // SAFETY: `seed_chooser` is non-null and points to the active level-intro seed chooser.
        let flying = unsafe { ptrs::SeedChooserScreen::seeds_in_flight(seed_chooser.as_ptr()) };
        validate_seed_chooser_fill(
            selected,
            slots,
            flying,
            crate::patches::random_locked(RandomStreamKind::Battle),
        )?;

        // Reset keeps a fresh chooser at stage 0 while it is being prepared. Restore the requested
        // stage now so native SetPacketType applies the correct survival initial-cooldown policy.
        crate::impls::reset::restore_pending_world_stage(self)?;

        // SAFETY: The game UI and seed chooser readiness were checked as LevelIntro above.
        // PickRandomSeeds fills missing slots without replacement, lands any flying cards, and
        // closes the chooser. Locked partial selection was rejected above because its rejection
        // sampling loop may otherwise never progress.
        unsafe { asm::seed_chooser_pick_random_seeds() };
        crate::impls::reset::apply_pending_card_cooldowns(seed_bank);

        Ok(())
    }
}

fn validate_seed_chooser_fill(selected: i32, slots: i32, flying: i32, locked: bool) -> Result<()> {
    if selected > slots {
        return Err(Pvz1051Error::SeedChooserCardsNotReady {
            selected,
            slots,
            flying,
        });
    }
    if selected < slots && locked {
        return Err(Pvz1051Error::KindUnavailable(
            "partial card selection with locked randomness",
        ));
    }
    Ok(())
}

fn game_mode_for_battle_config(config: BattleConfig) -> Result<GameMode> {
    match config {
        BattleConfig::Endless(config) => survival_endless_game_mode(config.scene),
    }
}

fn survival_endless_game_mode(scene: SceneKind) -> Result<GameMode> {
    SURVIVAL_ENDLESS_SCENES
        .iter()
        .find_map(|(survival_scene, mode)| (*survival_scene == scene).then_some(GameMode(*mode)))
        .ok_or(Pvz1051Error::KindUnavailable(
            "survival endless is only available on adventure battle scenes",
        ))
}

impl MainMenuBackend for Pvz1051Backend {
    fn back_to_main_menu(&mut self) -> Result<()> {
        self.ensure_playing_ready()?;
        // SAFETY: The game UI and board readiness were checked as Playing above.
        unsafe { crate::raw::abi::lawn_app_do_back_to_main() };
        Ok(())
    }
}

impl GameSpeedHintBackend for Pvz1051Backend {
    fn set_game_speed(&self, speed: PositiveFiniteF32) -> Result<()> {
        let speed = speed.get();
        if !(0.05..=100.0).contains(&speed) {
            return Err(Pvz1051Error::InvalidGameSpeed(speed));
        }
        crate::runtime::hook::set_game_speed(speed)
    }
}

impl FastForwardBackend for Pvz1051Backend {
    fn start_fast_forward(&self, options: FastForwardOptions) -> Result<()> {
        crate::runtime::hook::start_fast_forward(options)
    }

    fn stop_fast_forward(&self, reason: FastForwardStopReason) -> Result<()> {
        crate::runtime::hook::stop_fast_forward(reason)
    }

    fn fast_forward_active(&self) -> bool {
        crate::runtime::hook::fast_forward_active()
    }
}

impl SeedChooserFastForwardBackend for Pvz1051Backend {
    fn request_seed_chooser_fast_forward(&self, options: SeedChooserFastForwardOptions) -> Result<()> {
        crate::runtime::hook::request_seed_chooser_fast_forward(options)
    }
}

impl AdvancedPauseBackend for Pvz1051Backend {
    fn set_advanced_pause_with_options(&self, enabled: bool, options: AdvancedPauseOptions) -> Result<()> {
        crate::runtime::hook::set_advanced_pause(self, enabled, options)
    }

    fn advanced_pause_active(&self) -> bool {
        crate::runtime::hook::advanced_pause_active()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn partial_seed_fill_rejects_locked_rng_but_accepts_seeded_rng() {
        assert!(validate_seed_chooser_fill(1, 10, 1, false).is_ok());
        assert!(matches!(
            validate_seed_chooser_fill(1, 10, 1, true),
            Err(Pvz1051Error::KindUnavailable(
                "partial card selection with locked randomness"
            ))
        ));
        assert!(validate_seed_chooser_fill(10, 10, 1, true).is_ok());
    }

    #[test]
    fn saved_survival_scenes_in_dir_uses_profile_and_survival_modes() {
        let dir = temp_userdata_dir("survival_scenes");
        fs::write(dir.join("game7_13.dat"), b"").unwrap();
        fs::write(dir.join("game7_15.dat"), b"").unwrap();
        fs::write(dir.join("game8_11.dat"), b"").unwrap();
        fs::write(dir.join("game7_10.dat"), b"").unwrap();

        let scenes = saved_survival_scenes_in_dir(&dir, 7);

        assert_eq!(scenes, vec![SceneKind::Pool, SceneKind::Roof]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn saved_survival_scenes_in_dir_reports_none_when_missing() {
        let dir = temp_userdata_dir("no_survival_scenes");

        assert!(saved_survival_scenes_in_dir(&dir, 7).is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    fn temp_userdata_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("rsvz_surface_{name}_{}_{}", std::process::id(), unique));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
