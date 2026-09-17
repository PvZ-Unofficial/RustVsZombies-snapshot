use std::ptr::{self, NonNull};

use rsvz_backend_api::backend::{GameUiBackend, SpawnScheduleBackend};
use rsvz_model::model::{DEFAULT_SPAWN_WAVES, GameUi, SPAWN_SLOTS_PER_WAVE, SpawnWaveSlot, ZombieKind};

use crate::error::{Pvz1051Error, Result};
use crate::raw::kind::PvzZombieType;
use crate::raw::layout as ptrs;
use crate::runtime::Pvz1051Backend;

const PVZ_ZOMBIE_TYPE_COUNT: usize = ZombieKind::GigaGargantuar.code() as usize + 1;

impl SpawnScheduleBackend for Pvz1051Backend {
    fn spawn_wave_count(&self) -> Result<usize> {
        ensure_spawn_list_game_ui(self)?;
        board_wave_count(self.board()?)
    }

    fn set_spawn_type_allowed(&self, kind: ZombieKind, allowed: bool) -> Result<()> {
        ensure_spawn_list_game_ui(self)?;
        let board = self.board()?;
        let zombie_type_list = board_zombie_type_list(board)?;
        write_spawn_type_allowed(zombie_type_list, kind, allowed)
    }

    fn set_spawn_slot(&self, wave: usize, slot: SpawnWaveSlot, kind: Option<ZombieKind>) -> Result<()> {
        ensure_spawn_list_game_ui(self)?;
        let board = self.board()?;
        let wave_count = board_wave_count(board)?;
        if wave >= wave_count {
            return Err(Pvz1051Error::AbiPreconditionFailed(
                "spawn list wave is outside current board range",
            ));
        }
        let zombie_list = board_zombie_list(board)?;
        write_spawn_slot(zombie_list, wave, slot.index(), kind)
    }

    fn pick_spawn_list(&self) -> Result<()> {
        ensure_spawn_list_game_ui(self)?;
        let board = self.board()?;
        // SAFETY: the UI and board checks above establish the live board
        // required by the verified Board::PickZombieWaves wrapper.
        if let Some(stage) = crate::impls::reset::pending_world_stage() {
            // SAFETY: `board` is live and its Challenge pointer is checked before the temporary
            // stage write. The original chooser stage is restored before returning.
            let challenge = unsafe { ptrs::Board::challenge(board.as_ptr()) };
            let challenge = NonNull::new(challenge).ok_or(Pvz1051Error::NullChallenge)?;
            with_survival_stage(challenge, stage, || unsafe {
                crate::raw::abi::board_pick_zombie_waves();
            });
        } else {
            // SAFETY: the UI and board checks above establish the live board required by the
            // verified Board::PickZombieWaves wrapper.
            unsafe { crate::raw::abi::board_pick_zombie_waves() };
        }
        Ok(())
    }

    fn refresh_spawn_preview(&self) -> Result<()> {
        ensure_spawn_list_game_ui(self)?;
        let board = self.board()?;
        refresh_zombie_preview_if_level_intro(self, board)
    }
}

fn with_survival_stage<T>(challenge: NonNull<ptrs::Challenge>, stage: i32, action: impl FnOnce() -> T) -> T {
    // SAFETY: `challenge` is board-owned and non-null for the duration of the synchronous native
    // picker call. Both accesses target the objdump-verified mSurvivalStage field.
    let original = unsafe { ptrs::Challenge::endless_rounds(challenge.as_ptr()) };
    unsafe { *ptrs::Challenge::endless_rounds_mut(challenge.as_ptr()) = stage };
    let output = action();
    // SAFETY: the same checked Challenge remains live because the action cannot advance or replace
    // the world; restore the chooser-visible stage before returning to the host.
    unsafe { *ptrs::Challenge::endless_rounds_mut(challenge.as_ptr()) = original };
    output
}

fn ensure_spawn_list_game_ui(backend: &Pvz1051Backend) -> Result<()> {
    match backend.game_ui()? {
        GameUi::LevelIntro | GameUi::Playing => Ok(()),
        _ => Err(Pvz1051Error::AbiPreconditionFailed(
            "spawn list commit requires level-intro or playing UI",
        )),
    }
}

fn refresh_zombie_preview_if_level_intro(backend: &Pvz1051Backend, board: NonNull<ptrs::Board>) -> Result<()> {
    if backend.game_ui()? != GameUi::LevelIntro {
        return Ok(());
    }

    // SAFETY: the caller already validated that the current UI owns a live board. This mirrors
    // AvZ's `AUpdateZombiesPreview`: Board::RemoveCutsceneZombies clears current level-intro preview
    // zombies before the cutscene flag is reset below.
    unsafe { crate::raw::abi::board_remove_cutscene_zombies() };

    // SAFETY: `board` is the non-null current board used for the spawn-list write. The cutscene
    // pointer can be absent during level-intro construction and is checked before writing.
    let cut_scene = unsafe { ptrs::Board::cut_scene(board.as_ptr()) };
    let Some(cut_scene) = NonNull::new(cut_scene) else {
        return Ok(());
    };

    // SAFETY: `cut_scene` is non-null and board-owned. Offset 0x35 is PvZ 1051's
    // CutScene::mPlacedZombies flag; setting it false asks the game to rebuild the chooser
    // zombie preview from the newly written zombie type list.
    unsafe { ptrs::CutScene::set_zombie_preview_created(cut_scene.as_ptr(), false) };
    Ok(())
}

fn board_wave_count(board: NonNull<ptrs::Board>) -> Result<usize> {
    // SAFETY: `board` is non-null and points to the active Board.
    let raw_count = unsafe { ptrs::Board::num_waves(board.as_ptr()) };
    let Ok(wave_count) = usize::try_from(raw_count) else {
        return Err(Pvz1051Error::AbiPreconditionFailed("board wave count is negative"));
    };
    if !(1..=DEFAULT_SPAWN_WAVES).contains(&wave_count) {
        return Err(Pvz1051Error::AbiPreconditionFailed(
            "board wave count is outside supported 1051 spawn-list range",
        ));
    }
    Ok(wave_count)
}

fn board_zombie_type_list(board: NonNull<ptrs::Board>) -> Result<NonNull<u8>> {
    // SAFETY: `board` is non-null and the zombie type list pointer is copied immediately.
    let zombie_type_list = unsafe { ptrs::Board::zombie_type_list(board.as_ptr()) };
    NonNull::new(zombie_type_list).ok_or(Pvz1051Error::InvariantViolated("zombie type list"))
}

fn board_zombie_list(board: NonNull<ptrs::Board>) -> Result<NonNull<u32>> {
    // SAFETY: `board` is non-null and the zombie list pointer is copied immediately.
    let zombie_list = unsafe { ptrs::Board::zombie_list(board.as_ptr()) };
    NonNull::new(zombie_list).ok_or(Pvz1051Error::InvariantViolated("zombie list"))
}

fn write_spawn_type_allowed(zombie_type_list: NonNull<u8>, kind: ZombieKind, allowed: bool) -> Result<()> {
    let raw = PvzZombieType::from(kind).raw();
    let Ok(index) = usize::try_from(raw) else {
        return Err(Pvz1051Error::InvariantViolated("zombie type id"));
    };
    if index >= PVZ_ZOMBIE_TYPE_COUNT {
        return Err(Pvz1051Error::InvariantViolated("zombie type id"));
    }
    let type_ptr = zombie_type_list.as_ptr().wrapping_add(index);
    // SAFETY: `index` was checked against the explicit 1051 zombie type count.
    unsafe { ptr::write_unaligned(type_ptr, u8::from(allowed)) };
    Ok(())
}

fn write_spawn_slot(zombie_list: NonNull<u32>, wave: usize, slot: usize, kind: Option<ZombieKind>) -> Result<()> {
    let raw = match kind {
        Some(kind) => u32::try_from(PvzZombieType::from(kind).raw())
            .map_err(|_error| Pvz1051Error::InvariantViolated("zombie list type id"))?,
        None => u32::MAX,
    };
    let index = wave.saturating_mul(SPAWN_SLOTS_PER_WAVE).saturating_add(slot);
    let slot_ptr = zombie_list.as_ptr().wrapping_add(index);
    // SAFETY: `zombie_list` points to the board-owned contiguous wave list. The caller checked the
    // active board wave count and `slot < SPAWN_SLOTS_PER_WAVE`.
    unsafe { ptr::write_unaligned(slot_ptr, raw) };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_spawn_slot_uses_native_invalid_sentinel() {
        let mut list = [0_u32; SPAWN_SLOTS_PER_WAVE];
        let list = NonNull::new(list.as_mut_ptr()).expect("array pointer");

        write_spawn_slot(list, 0, 3, None).expect("clear spawn slot");

        // SAFETY: `list` points to the live local array and index 3 is in bounds.
        assert_eq!(unsafe { list.as_ptr().add(3).read() }, u32::MAX);
    }

    #[test]
    fn spawn_picker_temporarily_uses_the_pending_survival_stage() {
        let mut storage = [0_u8; 0x70];
        let challenge = NonNull::new(storage.as_mut_ptr().cast::<ptrs::Challenge>()).expect("challenge storage");
        // SAFETY: `storage` covers the verified mSurvivalStage offset used by the layout accessor.
        unsafe { *ptrs::Challenge::endless_rounds_mut(challenge.as_ptr()) = 0 };

        let observed = with_survival_stage(challenge, 63, || unsafe {
            ptrs::Challenge::endless_rounds(challenge.as_ptr())
        });

        assert_eq!(observed, 63);
        // SAFETY: `challenge` still points into the live local storage.
        assert_eq!(unsafe { ptrs::Challenge::endless_rounds(challenge.as_ptr()) }, 0);
    }
}
