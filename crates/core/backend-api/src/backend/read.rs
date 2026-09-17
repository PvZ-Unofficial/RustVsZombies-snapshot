//! Basic read-only backend capability traits.

use crate::backend::Backend;
use rsvz_model::model::{FiniteF32, GameUi, SceneKind, Wave};

/// Host UI state read capability.
pub trait GameUiBackend: Backend {
    /// PvZ internal game scene / host UI state.
    fn game_ui(&self) -> Result<GameUi, Self::Error>;

    /// Whether a `game_ui` error only means the host app/root is currently absent.
    fn game_ui_unavailable(&self, error: &Self::Error) -> bool;
}

/// Board readiness read capability.
pub trait BoardReadinessBackend: Backend {
    /// Whether the level-intro board has the data needed by safe core logic.
    fn level_intro_board_ready(&self) -> Result<bool, Self::Error>;

    /// Whether the playing board has the data needed by safe core logic.
    fn playing_board_ready(&self) -> Result<bool, Self::Error>;
}

/// Scene kind read capability.
pub trait SceneBackend: Backend {
    /// Current terrain scene.
    fn scene(&self) -> Result<SceneKind, Self::Error>;
}

/// Main board clock read capability.
pub trait ClockBackend: Backend {
    /// Current board main timer.
    fn clock(&self) -> Result<i32, Self::Error>;
}

/// Cursor attribution/type read capability.
pub trait CursorQueryBackend: Backend {
    /// Current board cursor type. PvZ uses 0 for an idle cursor.
    fn cursor_type(&self) -> Result<i32, Self::Error>;
}

/// Current wave read capability.
pub trait CurrentWaveBackend: Backend {
    /// Current wave.
    fn current_wave(&self) -> Result<Wave, Self::Error>;
}

/// Sun amount read capability.
pub trait SunQueryBackend: Backend {
    /// Current sun amount.
    fn sun(&self) -> Result<u32, Self::Error>;
}

/// Grid-to-pixel geometry capability.
pub trait GridGeometryBackend: Backend {
    /// Native `GridToPixelX(grid_x, grid_y)` scalar fact.
    fn grid_to_pixel_x(&self, grid_x: i32, grid_y: i32) -> Result<i32, Self::Error>;
    /// Native `GridToPixelY(grid_x, grid_y)` scalar fact.
    fn grid_to_pixel_y(&self, grid_x: i32, grid_y: i32) -> Result<i32, Self::Error>;
    /// Native row baseline y for an x position.
    fn pos_y_based_on_row(&self, pos_x: FiniteF32, row: i32) -> Result<f32, Self::Error>;
}

/// Native board terrain admission facts.
pub trait GridTerrainBackend: Backend {
    fn is_pool_square(&self, grid_x: i32, grid_y: i32) -> Result<bool, Self::Error>;
    fn row_can_have_zombies(&self, row: i32) -> Result<bool, Self::Error>;
}
