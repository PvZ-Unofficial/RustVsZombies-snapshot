//! 1051 fast-forward runtimes.

use rsvz_model::model::GameUi;
use rsvz_model::{FastForwardOptions, FastForwardStopReason, SeedChooserFastForwardOptions};

use crate::error::{Pvz1051Error, Result};
use crate::patches;
use crate::raw::kind::game_ui_from_raw;
use crate::raw::{abi as asm, layout as ptrs};
use crate::runtime::backend::{
    current_app, current_board, current_level_intro_board_ready, current_raw_game_ui, ensure_current_game_ui,
};
use crate::runtime::hook::profiler;

use super::seed_chooser::game_is_paused_or_modal_current;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FastForwardLoopDecision {
    Idle,
    Continue,
    Stopped(FastForwardStopReason),
}

#[derive(Default)]
pub(crate) struct FastForwardRuntime {
    active: Option<ActiveFastForward>,
}

struct ActiveFastForward {
    performance: patches::SimulationPerformanceGuard,
    frames_advanced: u64,
}

impl FastForwardRuntime {
    pub(crate) const fn new() -> Self {
        Self { active: None }
    }

    pub(crate) fn is_active(&self) -> bool {
        self.active.is_some()
    }

    pub(crate) fn frames_advanced(&self) -> u64 {
        self.active.as_ref().map_or(0, |active| active.frames_advanced)
    }

    pub(crate) fn start(&mut self, options: FastForwardOptions) -> Result<bool> {
        if self.active.is_some() {
            return Ok(false);
        }
        let performance = patches::SimulationPerformanceGuard::enable(options.performance)?;
        self.active = Some(ActiveFastForward {
            performance,
            frames_advanced: 0,
        });
        Ok(true)
    }

    pub(crate) fn stop(&mut self, reason: FastForwardStopReason) -> Result<FastForwardLoopDecision> {
        let Some(active) = self.active.as_mut() else {
            return Ok(FastForwardLoopDecision::Idle);
        };
        active.performance.restore()?;
        self.active = None;
        Ok(FastForwardLoopDecision::Stopped(reason))
    }

    pub(crate) fn before_tick(&mut self) -> Result<FastForwardLoopDecision> {
        let Some(_active) = self.active.as_mut() else {
            return Ok(FastForwardLoopDecision::Idle);
        };

        if let Some(reason) = fast_forward_stop_reason() {
            return self.stop(reason);
        }

        if game_is_paused_or_modal_current()? {
            return self.stop(FastForwardStopReason::Paused);
        }

        Ok(FastForwardLoopDecision::Continue)
    }

    pub(crate) fn advance_logic_frame(&mut self) -> Result<FastForwardLoopDecision> {
        if self.active.is_none() {
            return Ok(FastForwardLoopDecision::Idle);
        }
        match advance_1051_logic_frame()? {
            AdvanceOutcome::Advanced => {
                if let Some(active) = self.active.as_mut() {
                    active.frames_advanced = active.frames_advanced.saturating_add(1);
                }
                Ok(FastForwardLoopDecision::Continue)
            }
            AdvanceOutcome::Stop(reason) => self.stop(reason),
        }
    }
}

enum AdvanceOutcome {
    Advanced,
    Stop(FastForwardStopReason),
}

fn advance_1051_logic_frame() -> Result<AdvanceOutcome> {
    if let Some(reason) = fast_forward_stop_reason() {
        return Ok(AdvanceOutcome::Stop(reason));
    }

    if game_is_paused_or_modal_current()? {
        return Ok(AdvanceOutcome::Stop(FastForwardStopReason::Paused));
    }

    let app = current_app()?;
    let _board = current_board()?;
    crate::runtime::imitator_morph::begin_update_batch();
    crate::impls::event::begin_logic_frame_direct();
    profiler::measure_backend_update(|| {
        // SAFETY: `app` is an active, non-null 1051 LawnApp object and the field pointer is
        // valid for the copied PvZ i32 app counter.
        unsafe { increment(ptrs::LawnApp::app_counter_mut(app.as_ptr())) };
        // SAFETY: The active board is available for the 1051 delete-queue drain routine.
        // Native LawnApp::UpdateFrames drains it before every Board::Update.
        unsafe { asm::board_process_delete_queue() };
        // SAFETY: The app/board are active, non-null 1051 objects. This mirrors AvZ's
        // skip-loop ordering without entering the window update path.
        unsafe { asm::board_update() };
        // SAFETY: The active app owns a live effect system for the 1051 delete-queue routine.
        unsafe { asm::effect_system_process_delete_queue() };
        // SAFETY: The active app/board state is valid for the 1051 game-end check routine.
        unsafe { asm::lawn_app_check_for_game_end() };
    });
    crate::impls::event::end_logic_frame_direct();
    Ok(AdvanceOutcome::Advanced)
}

fn fast_forward_stop_reason() -> Option<FastForwardStopReason> {
    let Ok(raw_ui) = current_raw_game_ui() else {
        return Some(FastForwardStopReason::BoardUnavailable);
    };
    if !matches!(game_ui_from_raw(raw_ui), Ok(GameUi::Playing)) {
        return Some(FastForwardStopReason::LeftFight);
    }
    current_board()
        .is_err()
        .then_some(FastForwardStopReason::BoardUnavailable)
}

unsafe fn increment(value: *mut i32) {
    if value.is_null() {
        return;
    }
    // SAFETY: Caller guarantees `value` points to a live copied PvZ i32 counter.
    unsafe { *value = (*value).saturating_add(1) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SeedChooserFastForwardLoopDecision {
    Idle,
    Continue,
    Warmup,
    Stopped(SeedChooserFastForwardStopReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SeedChooserFastForwardStopReason {
    LeftLevelIntro,
    MaxFrames,
    RuntimeExit,
}

#[derive(Default)]
pub(crate) struct SeedChooserFastForwardRuntime {
    active: Option<ActiveSeedChooserFastForward>,
}

struct ActiveSeedChooserFastForward {
    options: SeedChooserFastForwardOptions,
    frames_advanced: u32,
}

impl SeedChooserFastForwardRuntime {
    pub(crate) const fn new() -> Self {
        Self { active: None }
    }

    pub(crate) fn request(&mut self, options: SeedChooserFastForwardOptions) -> bool {
        if self.active.is_some() {
            return false;
        }
        self.active = Some(ActiveSeedChooserFastForward {
            options,
            frames_advanced: 0,
        });
        true
    }

    pub(crate) fn stop(&mut self, reason: SeedChooserFastForwardStopReason) -> SeedChooserFastForwardLoopDecision {
        if self.active.take().is_some() {
            SeedChooserFastForwardLoopDecision::Stopped(reason)
        } else {
            SeedChooserFastForwardLoopDecision::Idle
        }
    }

    pub(crate) fn before_tick(&mut self) -> Result<SeedChooserFastForwardLoopDecision> {
        let Some(active) = self.active.as_mut() else {
            return Ok(SeedChooserFastForwardLoopDecision::Idle);
        };

        if game_ui_from_raw(current_raw_game_ui()?)? != GameUi::LevelIntro {
            return Ok(SeedChooserFastForwardLoopDecision::Stopped(
                SeedChooserFastForwardStopReason::LeftLevelIntro,
            ));
        }

        let board = current_board()?;
        // Native CutScene::Update refuses to advance a fresh board before Board::Draw. Draw the
        // dirty widget tree into the back buffer here without SexyAppBase::Redraw's front-buffer
        // blit, so the chooser loop can continue without exposing an intermediate chooser frame.
        // SAFETY: `current_board` returned the active checked Board; mDrawCount is a copied scalar.
        if unsafe { ptrs::Board::draw_count(board.as_ptr()) } == 0 {
            let app = current_app()?;
            // SAFETY: `app` is active; the widget-manager pointer may still be absent during
            // construction and is checked before entering the verified native DrawScreen ABI.
            let widget_manager = unsafe { ptrs::LawnApp::mouse_window(app.as_ptr()) };
            if !widget_manager.is_null() {
                // SAFETY: objdump verifies that 0x538eb0 receives this checked pointer on the
                // stack, returns its bool in AL, and performs callee cleanup with `ret 4`.
                let _drew = unsafe { asm::widget_manager_draw_screen() };
            }
        }
        // A missing manager or an unexpectedly clean widget tree falls back to the original outer
        // draw path rather than bypassing the native CutScene gate.
        if unsafe { ptrs::Board::draw_count(board.as_ptr()) } == 0 {
            return Ok(SeedChooserFastForwardLoopDecision::Warmup);
        }

        if active.frames_advanced >= active.options.max_frames {
            return Ok(SeedChooserFastForwardLoopDecision::Stopped(
                SeedChooserFastForwardStopReason::MaxFrames,
            ));
        }

        Ok(SeedChooserFastForwardLoopDecision::Continue)
    }

    pub(crate) fn advance_widget_frame(&mut self) -> Result<SeedChooserFastForwardLoopDecision> {
        if self.active.is_none() {
            return Ok(SeedChooserFastForwardLoopDecision::Idle);
        }
        advance_seed_chooser_widget_frame()?;
        if let Some(active) = self.active.as_mut() {
            active.frames_advanced = active.frames_advanced.saturating_add(1);
        }
        Ok(SeedChooserFastForwardLoopDecision::Continue)
    }
}

fn advance_seed_chooser_widget_frame() -> Result<()> {
    ensure_current_game_ui(GameUi::LevelIntro)?;
    if !current_level_intro_board_ready() {
        return Err(Pvz1051Error::AbiPreconditionFailed(
            "1051 level-intro board is not ready for seed chooser fast-forward",
        ));
    }
    let app = current_app()?;
    // SAFETY: `app` is non-null and points to the active LawnApp. The widget manager pointer may be
    // absent during construction and is checked before calling the 1051 wrapper.
    let widget_manager = unsafe { ptrs::LawnApp::mouse_window(app.as_ptr()) };
    if widget_manager.is_null() {
        return Err(Pvz1051Error::AbiPreconditionFailed(
            "1051 widget manager is not ready for seed chooser fast-forward",
        ));
    }

    // SAFETY: The game is in LevelIntro, the board/cutscene and widget manager are present, and
    // this calls `WidgetManager::UpdateFrame` directly without using the patched main-loop slot.
    unsafe { asm::widget_manager_update_frame() };
    Ok(())
}
