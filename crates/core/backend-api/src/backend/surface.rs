//! GUI, audio, input, host hint, and injected-runtime capabilities.

use crate::backend::Backend;
use rsvz_model::model::{BattleConfig, Grid, MouseButton, PixelPos, PositiveFiniteF32, Rect, SoundId};

/// 鼠标键盘输入能力。
///
/// Input is synchronous and exclusive: it can replace Board or enter a native
/// modal loop. The host must defer ordinary dispatch and unload until it returns;
/// callers must resample phase/clock after the existing access epoch changes.
///
/// ```
/// use rsvz_backend_api::InputBackend;
/// use rsvz_model::model::{PixelPos, MouseButton};
/// fn receivers<B: InputBackend>() {
///     let _: fn(&mut B, PixelPos) -> Result<(), B::Error> = B::mouse_move;
///     let _: fn(&mut B, PixelPos, MouseButton) -> Result<(), B::Error> = B::mouse_down;
///     let _: fn(&mut B, PixelPos, MouseButton) -> Result<(), B::Error> = B::mouse_up;
///     let _: fn(&mut B) -> Result<(), B::Error> = B::release_mouse;
/// }
/// ```
pub trait InputBackend: Backend {
    /// 移动鼠标。
    fn mouse_move(&mut self, pos: PixelPos) -> Result<(), Self::Error>;
    /// 鼠标按下。
    fn mouse_down(&mut self, pos: PixelPos, button: MouseButton) -> Result<(), Self::Error>;
    /// 鼠标抬起。
    fn mouse_up(&mut self, pos: PixelPos, button: MouseButton) -> Result<(), Self::Error>;
    /// 释放当前鼠标持有物。
    fn release_mouse(&mut self) -> Result<(), Self::Error>;
}

/// 游戏内显示/overlay 能力。
///
/// This is host feedback. Headless or minimal hosts may implement these operations as best-effort
/// no-ops, but gameplay semantic operations must not report success without applying their effect.
pub trait DisplayBackend: Backend {
    /// 显示文字。
    fn show_text(&self, text: &str, pos: PixelPos) -> Result<(), Self::Error>;
    /// 绘制矩形。
    fn draw_rect(&self, rect: Rect) -> Result<(), Self::Error>;
    /// 切换 overlay。
    fn set_overlay_enabled(&self, enabled: bool) -> Result<(), Self::Error>;
}

/// 音频能力。
///
/// This is host feedback. A backend may safely no-op audio feedback when no audio host exists.
pub trait AudioBackend: Backend {
    /// 播放音效。
    fn play_sound(&self, sound: SoundId) -> Result<(), Self::Error>;
    /// 停止音效。
    fn stop_sound(&self, sound: SoundId) -> Result<(), Self::Error>;
}

/// Battle entry capability.
///
/// These are gameplay/flow semantic actions. Implementations must not report success if a requested
/// non-no-op transition could not be applied; a headless backend may still define `start_battle` as
/// a no-op when its battle is already started by construction.
pub trait BattleEntryBackend: Backend {
    /// 进入关卡。
    fn enter_game(&mut self, config: BattleConfig) -> Result<(), Self::Error>;
    /// 开始战斗；若卡槽尚未填满，按原生选卡器语义随机补满后再进入战斗。
    ///
    /// 后端无法安全完成随机补卡（例如随机流已锁定）时必须返回错误。
    fn start_battle(&mut self) -> Result<(), Self::Error>;
}

/// Main-menu navigation capability.
pub trait MainMenuBackend: Backend {
    /// 返回主界面。
    fn back_to_main_menu(&mut self) -> Result<(), Self::Error>;
}

/// Host speed hint capability.
///
/// This is a host hint rather than a guaranteed simulation semantic. A backend may no-op or record
/// the hint if it has no live speed control, but must not use this trait for injected loop restore.
pub trait GameSpeedHintBackend: Backend {
    /// 设置游戏速度倍率 hint。
    fn set_game_speed(&self, speed: PositiveFiniteF32) -> Result<(), Self::Error>;
}

/// 花园能力。
pub trait GardenBackend: Backend {
    /// 给指定格子浇水。
    fn water_plant(&self, grid: Grid) -> Result<(), Self::Error>;
}
