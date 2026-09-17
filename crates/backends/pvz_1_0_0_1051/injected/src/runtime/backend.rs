use std::marker::PhantomData;
use std::ptr::NonNull;
use std::rc::Rc;

use rsvz_backend_api::backend::SceneBackend;
use rsvz_model::model::{GameUi, Grid};

use crate::error::{Pvz1051Error, Result};
use crate::raw::kind::game_ui_from_raw;
use crate::raw::{abi as asm, layout as ptrs};

const CONTINUE_DIALOG_KIND: i32 = 37;

/// Linear capability for the active PvZ 1.0.0.1051 process state.
///
/// The private zero-sized marker binds the token to the PvZ main thread. The
/// lack of safe constructors, `Default`, `Clone`, and `Copy` prevents a second
/// safe capability from being fabricated while borrowed handles are alive.
///
/// Same-epoch edits only need a shared receiver:
///
/// ```no_run
/// use rsvz_backend_1051::Pvz1051Backend;
/// use rsvz_backend_api::backend::{PlantHealthWriteBackend, PlantReadBackend};
/// use rsvz_model::model::PositiveHp;
///
/// fn edit_first_plant(backend: &Pvz1051Backend) {
///     if let Some(plant) = backend.plants().unwrap().next() {
///         backend
///             .set_plant_hp(plant, PositiveHp::new(1).unwrap())
///             .unwrap();
///     }
/// }
/// ```
///
/// Epoch-changing operations cannot overlap a live handle:
///
/// ```compile_fail
/// use rsvz_backend_1051::Pvz1051Backend;
/// use rsvz_backend_api::backend::{PlantReadBackend, SceneEditBackend};
/// use rsvz_model::model::SceneKind;
///
/// fn replace_scene(backend: &mut Pvz1051Backend) {
///     let plant = backend.plants().unwrap().next().unwrap();
///     backend.set_scene(SceneKind::Day).unwrap();
///     let _ = backend.plant_id(plant);
/// }
/// ```
///
/// ```compile_fail
/// use rsvz_backend_1051::Pvz1051Backend;
/// let _backend = Pvz1051Backend::new();
/// ```
///
/// ```compile_fail
/// use rsvz_backend_1051::Pvz1051Backend;
/// let _backend = Pvz1051Backend { _thread_bound: std::marker::PhantomData };
/// ```
///
/// ```compile_fail
/// use rsvz_backend_1051::Pvz1051Backend;
/// fn require_default<T: Default>() {}
/// require_default::<Pvz1051Backend>();
/// ```
///
/// ```compile_fail
/// use rsvz_backend_1051::Pvz1051Backend;
/// fn require_clone<T: Clone>() {}
/// require_clone::<Pvz1051Backend>();
/// ```
///
///
/// ```compile_fail
/// use rsvz_backend_1051::Pvz1051Backend;
/// fn require_send<T: Send>() {}
/// require_send::<Pvz1051Backend>();
/// ```
///
/// ```compile_fail
/// use rsvz_backend_1051::Pvz1051Backend;
/// fn require_sync<T: Sync>() {}
/// require_sync::<Pvz1051Backend>();
/// ```
#[derive(Debug)]
pub struct Pvz1051Backend {
    _thread_bound: PhantomData<Rc<()>>,
}

impl Pvz1051Backend {
    /// Claims the process-global backend capability for the injected runtime owner.
    ///
    /// # Safety
    ///
    /// The caller must create this token at most once for the process lifetime and must keep it in
    /// the single injected-runtime owner slot. Safe code must never construct a second token.
    /// It may only lend the token while the native LawnApp is alive on the game
    /// thread. Every borrow must end before native update/reclamation, App teardown,
    /// or host unload; a live App does not imply a live Board.

    pub(crate) const unsafe fn claim_runtime_owner_token() -> Self {
        Self {
            _thread_bound: PhantomData,
        }
    }

    pub(crate) fn app(&self) -> NonNull<ptrs::LawnApp> {
        // SAFETY: the host only lends its unique token during a native ingress
        // with a live LawnApp. No App address is retained between operations.
        unsafe { NonNull::new_unchecked(ptrs::lawn_app()) }
    }

    pub(crate) fn board(&self) -> Result<NonNull<ptrs::Board>> {
        let app = self.app();
        // SAFETY: a token proves App lifetime, but a menu can have no Board.
        NonNull::new(unsafe { ptrs::LawnApp::board(app.as_ptr()) }).ok_or(Pvz1051Error::NullBoard)
    }

    pub(crate) fn raw_game_ui(&self) -> i32 {
        let app = self.app();
        // SAFETY: the unique token is lent only while LawnApp is alive.
        unsafe { ptrs::LawnApp::game_ui(app.as_ptr()) }
    }

    pub(crate) fn level_intro_board_ready(&self) -> Result<bool> {
        self.ensure_game_ui(GameUi::LevelIntro)?;
        Ok(Self::level_intro_board_ready_for_app(self.app()))
    }

    pub(crate) fn ensure_game_ui(&self, expected: GameUi) -> Result<()> {
        let actual = game_ui_from_raw(self.raw_game_ui())?;
        if actual != expected {
            return Err(Pvz1051Error::WrongGameUi { expected, actual });
        }
        Ok(())
    }

    pub(crate) fn ensure_playing_ready(&self) -> Result<()> {
        self.ensure_playing()
    }

    pub(crate) fn ensure_playing(&self) -> Result<()> {
        self.ensure_game_ui(GameUi::Playing)?;
        let _board = self.board()?;
        Ok(())
    }

    pub(crate) fn validate_grid(&self, grid: Grid) -> Result<()> {
        if !rsvz_model::FieldInfo::from_scene(self.scene()?).contains_grid(grid) {
            return Err(Pvz1051Error::InvalidGrid);
        }
        Ok(())
    }

    pub(crate) fn ensure_level_intro_ready(&self) -> Result<()> {
        self.ensure_game_ui(GameUi::LevelIntro)?;
        self.ensure_seed_chooser_ready_for_auto_selection()
    }

    #[doc(hidden)]
    pub fn playing_board_pending_seed_selection(&self) -> Result<Option<(bool, i32)>> {
        self.ensure_game_ui(GameUi::Playing)?;
        let board = self.board()?;
        // SAFETY: `board` is non-null and points to the current playing board; these are copied
        // scalar fields. In Survival, `mNextSurvivalStageCounter > 0` / `mLevelComplete` means the
        // saved board is transitioning into the next LevelIntro/seed-chooser rather than a fixed
        // in-progress fight, so card verification must be deferred.
        let level_complete = unsafe { ptrs::Board::level_complete(board.as_ptr()) };
        // SAFETY: same Board validity argument as above; this aliases PvZ's
        // `mNextSurvivalStageCounter` at 0x5604.
        let next_survival_stage_counter = unsafe { ptrs::Board::next_survival_stage_counter(board.as_ptr()) };
        Ok(classify_pending_seed_selection(
            level_complete,
            next_survival_stage_counter,
        ))
    }

    #[doc(hidden)]
    pub fn click_continue_dialog_if_present(&self) -> Result<bool> {
        // SAFETY: the token proves App lifetime; dialog 37 is PvZ's ContinueDialog. KillDialog is idempotent:
        // it returns false when the dialog is absent, and removing this modal keeps the loaded board
        // while unblocking the fight UI.
        Ok(unsafe { asm::lawn_app_kill_dialog(CONTINUE_DIALOG_KIND) } != 0)
    }
}

pub(crate) fn current_app() -> Result<NonNull<ptrs::LawnApp>> {
    // SAFETY: 0x6a9ec0 is the PvZ 1.0.0.1051 global LawnApp pointer. The null check below
    // converts the wrapper precondition into a recoverable backend error.
    let app = unsafe { ptrs::lawn_app() };
    NonNull::new(app).ok_or(Pvz1051Error::NullLawnApp)
}

pub(crate) fn current_board() -> Result<NonNull<ptrs::Board>> {
    let app = current_app()?;
    // SAFETY: `app` is a non-null LawnApp pointer read from the global root; board may be null
    // outside a running game and is checked immediately.
    let board = unsafe { ptrs::LawnApp::board(app.as_ptr()) };
    NonNull::new(board).ok_or(Pvz1051Error::NullBoard)
}

pub(crate) fn current_raw_game_ui() -> Result<i32> {
    let app = current_app()?;
    // SAFETY: `app` is non-null and points to the current LawnApp root.
    Ok(unsafe { ptrs::LawnApp::game_ui(app.as_ptr()) })
}

impl Pvz1051Backend {
    pub(crate) fn seed_chooser(&self) -> Result<NonNull<ptrs::SeedChooserScreen>> {
        let app = self.app();
        // SAFETY: `app` is a non-null LawnApp pointer read from the global root; seed chooser may
        // be null outside the level-intro UI and is checked immediately.
        let seed_chooser = unsafe { ptrs::LawnApp::seed_chooser(app.as_ptr()) };
        NonNull::new(seed_chooser).ok_or(Pvz1051Error::NullSeedChooser)
    }

    pub(crate) fn seed_bank(&self) -> Result<NonNull<ptrs::SeedBank>> {
        let board = self.board()?;
        // SAFETY: `board` is non-null and the seed bank field is checked for null before use.
        let bank = unsafe { ptrs::Board::seed_bank(board.as_ptr()) };
        NonNull::new(bank).ok_or(Pvz1051Error::NullSeedBank)
    }

    fn level_intro_board_ready_for_app(app: NonNull<ptrs::LawnApp>) -> bool {
        // SAFETY: `app` is non-null and points to LawnApp; board may still be absent during UI
        // construction and is checked before use.
        let board = unsafe { ptrs::LawnApp::board(app.as_ptr()) };
        if board.is_null() {
            return false;
        }

        // SAFETY: `board` is non-null and points to the active level-intro board. The cutscene
        // pointer is created asynchronously by PvZ and is checked immediately.
        let cut_scene = unsafe { ptrs::Board::cut_scene(board) };
        !cut_scene.is_null()
    }
}

pub(crate) fn current_level_intro_board_ready() -> bool {
    current_app().is_ok_and(Pvz1051Backend::level_intro_board_ready_for_app)
}

pub(crate) fn ensure_current_game_ui(expected: GameUi) -> Result<()> {
    let actual = game_ui_from_raw(current_raw_game_ui()?)?;
    if actual != expected {
        return Err(Pvz1051Error::WrongGameUi { expected, actual });
    }
    Ok(())
}

fn classify_pending_seed_selection(level_complete: bool, next_survival_stage_counter: i32) -> Option<(bool, i32)> {
    (level_complete || next_survival_stage_counter > 0).then_some((level_complete, next_survival_stage_counter))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_owner_token_stays_zero_sized() {
        assert_eq!(std::mem::size_of::<Pvz1051Backend>(), 0);
    }

    #[test]
    fn direct_playing_transition_is_pending_seed_selection() {
        assert_eq!(classify_pending_seed_selection(false, 1), Some((false, 1)));
        assert_eq!(classify_pending_seed_selection(true, 0), Some((true, 0)));
        assert_eq!(classify_pending_seed_selection(false, 0), None);
    }
}
