//! Backend-free fast-forward and advanced-pause request vocabulary.

use crate::model::{RelativeTime, WaveClockState};

/// Backend optimization level used while a backend is fast-forwarding simulation frames.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FastForwardPerformance {
    /// Do not enable backend-specific simulation optimizations.
    Off,
    /// Enable conservative backend simulation optimizations.
    #[default]
    Basic,
    /// Enable all known safe simulation-speed optimizations for this backend/version.
    Aggressive,
}

/// Safe options for a fast-forward request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FastForwardOptions {
    /// Backend-specific simulation optimization level.
    pub performance: FastForwardPerformance,
    /// Whether the backend should suppress normal window/render updates while advancing logic.
    pub suppress_window_update: bool,
}

impl FastForwardOptions {
    /// Conservative performance options.
    #[must_use]
    pub const fn basic() -> Self {
        Self {
            performance: FastForwardPerformance::Basic,
            suppress_window_update: true,
        }
    }

    /// Aggressive performance options.
    #[must_use]
    pub const fn aggressive() -> Self {
        Self {
            performance: FastForwardPerformance::Aggressive,
            suppress_window_update: true,
        }
    }

    /// No backend simulation optimizations, still using the fast-forward runtime boundary.
    #[must_use]
    pub const fn without_backend_optimization() -> Self {
        Self {
            performance: FastForwardPerformance::Off,
            suppress_window_update: true,
        }
    }
}

impl Default for FastForwardOptions {
    fn default() -> Self {
        Self::basic()
    }
}

/// Safe options for fast-forwarding the level-intro seed chooser UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SeedChooserFastForwardOptions {
    /// Temporary safety cap for the first 1051 implementation.
    pub max_frames: u32,
}

impl SeedChooserFastForwardOptions {
    /// Default temporary frame cap used to avoid hanging inside the injected hook.
    pub const DEFAULT_MAX_FRAMES: u32 = 5000;

    /// Creates options with an explicit extra widget-frame cap.
    #[must_use]
    pub const fn new(max_frames: u32) -> Self {
        Self { max_frames }
    }
}

impl Default for SeedChooserFastForwardOptions {
    fn default() -> Self {
        Self::new(Self::DEFAULT_MAX_FRAMES)
    }
}

/// Why a fast-forward request stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FastForwardStopReason {
    /// The request condition became false at the boundary frame.
    ConditionMet,
    /// The game left the battle board.
    LeftFight,
    /// The game is paused or a top-level modal/window is open.
    Paused,
    /// The battle board became unavailable.
    BoardUnavailable,
    /// The backend reported an error while fast-forwarding.
    BackendError,
    /// The host runtime is exiting, faulted, panicking, or unloading.
    RuntimeExit,
}

/// Wave-relative fast-forward stop target and timing state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FastForwardUntil {
    target: RelativeTime,
    wave_clocks: WaveClockState,
}

impl FastForwardUntil {
    /// Creates a target with the caller's known wave-clock assumptions/observations.
    #[must_use]
    pub const fn new(target: RelativeTime, wave_clocks: WaveClockState) -> Self {
        Self { target, wave_clocks }
    }

    /// Target wave-relative time.
    #[must_use]
    pub const fn target(&self) -> RelativeTime {
        self.target
    }

    /// Known/assumed wave clocks used to resolve the target.
    #[must_use]
    pub const fn wave_clocks(&self) -> &WaveClockState {
        &self.wave_clocks
    }

    /// Mutable access to the known/assumed wave clocks.
    pub fn wave_clocks_mut(&mut self) -> &mut WaveClockState {
        &mut self.wave_clocks
    }
}

/// A concrete fast-forward request.
pub struct FastForwardRequest {
    until: FastForwardUntil,
    options: FastForwardOptions,
}

impl FastForwardRequest {
    /// Creates a wave-relative request.
    #[must_use]
    pub const fn until(target: RelativeTime, options: FastForwardOptions, wave_clocks: WaveClockState) -> Self {
        Self {
            until: FastForwardUntil::new(target, wave_clocks),
            options,
        }
    }

    /// Request options.
    #[must_use]
    pub const fn options(&self) -> FastForwardOptions {
        self.options
    }

    /// Mutable access to the wave-relative stop target.
    pub fn until_mut(&mut self) -> &mut FastForwardUntil {
        &mut self.until
    }
}

/// Declarative script fast-forward window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FastForwardWindow {
    /// Optional start time. `None` means start once the battle timeline is ready.
    pub start: Option<RelativeTime>,
    /// End time checked before dispatching the stop frame.
    pub end: RelativeTime,
    /// Options used while the window is active.
    pub options: FastForwardOptions,
}

impl FastForwardWindow {
    /// Creates a window that begins as soon as the fight timeline is ready.
    #[must_use]
    pub const fn until(end: RelativeTime, options: FastForwardOptions) -> Self {
        Self {
            start: None,
            end,
            options,
        }
    }

    /// Creates a bounded fast-forward window.
    #[must_use]
    pub const fn between(start: RelativeTime, end: RelativeTime, options: FastForwardOptions) -> Self {
        Self {
            start: Some(start),
            end,
            options,
        }
    }
}

/// RGBA mask color used by advanced pause backends that support masking.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AdvancedPauseMaskColor {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl AdvancedPauseMaskColor {
    /// Creates an RGBA color.
    #[must_use]
    pub const fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }
}

impl Default for AdvancedPauseMaskColor {
    fn default() -> Self {
        Self::rgba(0, 0, 0, 96)
    }
}

/// Safe options for AvZ-style advanced pause.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AdvancedPauseOptions {
    /// Whether the backend should draw a translucent overlay mask when supported.
    pub draw_mask: bool,
    /// Overlay mask color.
    pub mask_color: AdvancedPauseMaskColor,
    /// Whether the backend should play an effect sound when toggled.
    pub play_sound: bool,
    /// Whether cursor object and preview refresh should continue while paused.
    pub refresh_cursor_preview: bool,
}

impl Default for AdvancedPauseOptions {
    fn default() -> Self {
        Self {
            draw_mask: false,
            mask_color: AdvancedPauseMaskColor::default(),
            play_sound: false,
            refresh_cursor_preview: true,
        }
    }
}
