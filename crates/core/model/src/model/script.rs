//! Backend-neutral script lifecycle model.

/// Controls when a host may tear down and re-register a script.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ReloadMode {
    /// Do not automatically reload after initial registration.
    #[default]
    None,
    /// Reload when the game returns to a main-menu-like UI.
    ///
    /// This does not imply that a backend should automatically enter a saved game again.
    MainUi,
    /// Reload on main-menu return or a new fight board.
    ///
    /// This does not imply that a backend should automatically enter a saved game again.
    MainUiOrFightUi,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReloadBoundary {
    MainUi,
    FightUi,
}

/// Default starting sun for framework-controlled fresh worlds.
pub const DEFAULT_RESET_INITIAL_SUN: u32 = 8_000;

/// Controls the seed-packet cooldown state after a framework-controlled reset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ResetCardCooldowns {
    /// Make every selected card immediately usable.
    #[default]
    Ready,
    /// Keep the backend's native reset behavior.
    ///
    /// Backends that destroy the previous card bank during reset may reject this policy.
    PreserveNative,
}

/// Backend-neutral inputs for rebuilding a fresh trial world.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorldResetConfig {
    pub completed_rounds: u32,
    pub seed: u32,
    pub initial_sun: u32,
    pub card_cooldowns: ResetCardCooldowns,
}

impl Default for WorldResetConfig {
    fn default() -> Self {
        Self {
            completed_rounds: 0,
            seed: 0,
            initial_sun: DEFAULT_RESET_INITIAL_SUN,
            card_cooldowns: ResetCardCooldowns::Ready,
        }
    }
}

/// Immutable identity of one worker within a script session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SessionShard {
    pub index: u32,
    pub count: u32,
    pub seed_base: u32,
}

impl Default for SessionShard {
    fn default() -> Self {
        Self {
            index: 0,
            count: 1,
            seed_base: 0,
        }
    }
}

impl ReloadMode {
    #[must_use]
    pub const fn reloads_at(self, boundary: ReloadBoundary) -> bool {
        match (self, boundary) {
            (Self::None, _) | (Self::MainUi, ReloadBoundary::FightUi) => false,
            (Self::MainUi | Self::MainUiOrFightUi, ReloadBoundary::MainUi)
            | (Self::MainUiOrFightUi, ReloadBoundary::FightUi) => true,
        }
    }
}

/// High-level script lifecycle phase exposed to host adapters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ScriptPhase {
    /// Script is registering timeline callbacks and desired setup state.
    #[default]
    Registering,
    /// Level-intro setup phase.
    LevelIntro,
    /// Fight board is ready.
    Playing,
    /// Script has finished or was torn down.
    Finished,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reload_modes_cover_both_physical_boundaries() {
        assert!(!ReloadMode::None.reloads_at(ReloadBoundary::MainUi));
        assert!(!ReloadMode::None.reloads_at(ReloadBoundary::FightUi));
        assert!(ReloadMode::MainUi.reloads_at(ReloadBoundary::MainUi));
        assert!(!ReloadMode::MainUi.reloads_at(ReloadBoundary::FightUi));
        assert!(ReloadMode::MainUiOrFightUi.reloads_at(ReloadBoundary::MainUi));
        assert!(ReloadMode::MainUiOrFightUi.reloads_at(ReloadBoundary::FightUi));
    }

    #[test]
    fn controlled_worlds_default_to_eight_thousand_sun() {
        let reset = WorldResetConfig::default();
        assert_eq!(reset.initial_sun, 8_000);
        assert_eq!(reset.card_cooldowns, ResetCardCooldowns::Ready);
    }
}
