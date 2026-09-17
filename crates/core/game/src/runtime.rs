//! Backend-neutral error used by user callbacks and shared game operations.

use rsvz_backend_api::backend::{BoardReadinessBackend, GameUiBackend, WaveTimingBackend};
use rsvz_model::{GameUi, RelativeTime, WaveTimingSnapshot};
use rsvz_schedule::tick::{TickMeta, TickPhase};

/// Host-facing result of one script-frame dispatch.
///
/// The enum lives in core because both physical runners classify the same
/// semantic outcomes, while each runner retains its own update/error policy.
#[derive(Debug, PartialEq, Eq)]
pub enum RuntimeFrameDispatch {
    Continue,
    OperationError(RuntimeError),
    OperationPanic,
    TimingBackendError(RuntimeError),
    TimingViolation(String),
    TimingControlError(RuntimeError),
    TimingControlViolation(String),
    FrameCallbackError(RuntimeError),
    FrameCallbackPanic,
}

pub use rsvz_backend_api::error::{RuntimeError, RuntimeResult};

/// Backend-neutral facts sampled once for a runtime frame.
#[derive(Clone, Copy, Debug)]
pub struct RuntimeFrameFacts {
    pub phase: TickPhase,
    pub game_ui: Option<GameUi>,
    pub clock: Option<i32>,
    pub snapshot: Option<WaveTimingSnapshot>,
}

/// One backend sample shared by Timeline and Tick dispatch for a frame.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct CurrentFrameSample {
    /// Physical access generation; absent for logical notifications and backend-free test input.
    pub access_epoch: Option<u64>,
    pub meta: TickMeta,
    pub snapshot: Option<WaveTimingSnapshot>,
    pub report_time: Option<RelativeTime>,
}

impl CurrentFrameSample {
    #[doc(hidden)]
    #[must_use]
    pub const fn game_ui(&self) -> Option<GameUi> {
        self.meta.game_ui
    }

    #[doc(hidden)]
    #[must_use]
    pub const fn snapshot(&self) -> Option<WaveTimingSnapshot> {
        self.snapshot
    }

    #[doc(hidden)]
    #[must_use]
    pub const fn phase(&self) -> TickPhase {
        self.meta.phase
    }
}

/// Tracks frame identity without reading the backend a second time.
pub struct RuntimeFrameState {
    last_playing_clock: Option<i32>,
    last_meta: TickMeta,
}

impl Default for RuntimeFrameState {
    fn default() -> Self {
        Self {
            last_playing_clock: None,
            last_meta: TickMeta::unavailable(),
        }
    }
}

impl RuntimeFrameState {
    #[must_use]
    pub fn input(&mut self, facts: RuntimeFrameFacts) -> (TickMeta, Option<WaveTimingSnapshot>) {
        let is_new_frame =
            facts.phase == TickPhase::Playing && facts.clock.is_some() && facts.clock != self.last_playing_clock;
        self.last_playing_clock = if facts.phase == TickPhase::Playing {
            facts.clock
        } else {
            None
        };

        let meta = TickMeta {
            phase: facts.phase,
            game_ui: facts.game_ui,
            clock: facts.clock,
            is_new_frame,
        };
        self.last_meta = meta;
        (meta, facts.snapshot)
    }

    #[must_use]
    pub const fn last_meta(&self) -> TickMeta {
        self.last_meta
    }
}

/// Samples current phase and timing without forwarding a backend through game.
pub fn runtime_frame_facts() -> RuntimeResult<RuntimeFrameFacts>
where
    rsvz_current::CurrentBackend: GameUiBackend + BoardReadinessBackend + WaveTimingBackend,
{
    rsvz_current::with_backend_shared(|backend| -> RuntimeResult<_> {
        let game_ui = match backend.game_ui() {
            Ok(game_ui) => game_ui,
            Err(error) if backend.game_ui_unavailable(&error) => {
                return Ok(RuntimeFrameFacts {
                    phase: TickPhase::Unavailable,
                    game_ui: None,
                    clock: None,
                    snapshot: None,
                });
            }
            Err(error) => return Err(error.into()),
        };

        match game_ui {
            GameUi::Loading | GameUi::Menu => Ok(RuntimeFrameFacts {
                phase: TickPhase::Menu,
                game_ui: Some(game_ui),
                clock: None,
                snapshot: None,
            }),
            GameUi::LevelIntro => Ok(RuntimeFrameFacts {
                phase: if backend.level_intro_board_ready()? {
                    TickPhase::LevelIntro
                } else {
                    TickPhase::NotReady
                },
                game_ui: Some(game_ui),
                clock: None,
                snapshot: None,
            }),
            GameUi::Playing if backend.playing_board_ready()? => {
                let snapshot = crate::timing::wave_timing()?;
                Ok(RuntimeFrameFacts {
                    phase: TickPhase::Playing,
                    game_ui: Some(game_ui),
                    clock: Some(snapshot.clock),
                    snapshot: Some(snapshot),
                })
            }
            GameUi::Playing => Ok(RuntimeFrameFacts {
                phase: TickPhase::NotReady,
                game_ui: Some(game_ui),
                clock: None,
                snapshot: None,
            }),
            GameUi::ZombiesWon | GameUi::Award | GameUi::Credit => Ok(RuntimeFrameFacts {
                phase: TickPhase::Finished,
                game_ui: Some(game_ui),
                clock: None,
                snapshot: None,
            }),
            GameUi::Challenge => Ok(RuntimeFrameFacts {
                phase: TickPhase::Other,
                game_ui: Some(game_ui),
                clock: None,
                snapshot: None,
            }),
        }
    })
    .map_err(|error| RuntimeError::new(error.to_string()))?
}
