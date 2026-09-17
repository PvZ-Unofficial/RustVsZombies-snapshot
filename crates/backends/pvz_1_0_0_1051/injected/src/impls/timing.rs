use rsvz_backend_api::backend::{WaveHealthBackend, WaveRefreshControlBackend, WaveTimingBackend};
use rsvz_model::model::{GameUi, Wave};

use crate::error::{Pvz1051Error, Result};
use crate::raw::{abi as asm, layout as ptrs};
use crate::runtime::Pvz1051Backend;

impl WaveTimingBackend for Pvz1051Backend {
    fn total_waves(&self) -> Result<i32> {
        Ok({
            let board = self.board()?;
            // SAFETY: the current board is valid and only this scalar is copied.
            unsafe { ptrs::Board::num_waves(board.as_ptr()) }
        })
    }
    fn refresh_countdown(&self) -> Result<i32> {
        let board = self.board()?;
        // SAFETY: the current board is valid and only this scalar is copied.
        Ok(unsafe { ptrs::Board::refresh_countdown(board.as_ptr()) })
    }
    fn initial_countdown(&self) -> Result<i32> {
        let board = self.board()?;
        // SAFETY: the current board is valid and only this scalar is copied.
        Ok(unsafe { ptrs::Board::initial_countdown(board.as_ptr()) })
    }
    fn huge_wave_countdown(&self) -> Result<i32> {
        let board = self.board()?;
        // SAFETY: the current board is valid and only this scalar is copied.
        Ok(unsafe { ptrs::Board::huge_wave_countdown(board.as_ptr()) })
    }
    fn level_end_countdown(&self) -> Result<i32> {
        let board = self.board()?;
        // SAFETY: the current board is valid and only this scalar is copied.
        Ok(unsafe { ptrs::Board::level_end_countdown(board.as_ptr()) })
    }
}

impl WaveHealthBackend for Pvz1051Backend {
    fn zombie_health_wave_start(&self) -> Result<i32> {
        let board = self.board()?;
        // SAFETY: `board` is active and this copies one scalar field.
        Ok(unsafe { ptrs::Board::zombie_health_wave_start(board.as_ptr()) })
    }

    fn total_zombies_health_in_wave(&self, wave: i32) -> Result<i32> {
        if wave < 0 {
            return Err(Pvz1051Error::AbiPreconditionFailed(
                "1051 wave health index is negative",
            ));
        }
        let board = self.board()?;
        // SAFETY: objdump verifies the explicit Board*/stack-wave ABI documented below.
        Ok(unsafe { asm::board_total_zombies_health_in_wave(board.as_ptr(), wave) })
    }
}

impl WaveRefreshControlBackend for Pvz1051Backend {
    fn commit_timer_only_wave_refresh(&self, expected_current_wave: Wave, initial_countdown: i32) -> Result<()> {
        if initial_countdown <= 0 {
            return Err(Pvz1051Error::AbiPreconditionFailed(
                "wave refresh initial countdown must be positive",
            ));
        }
        let remaining_countdown = initial_countdown - 1;
        self.ensure_game_ui(GameUi::Playing)?;
        let board = self.board()?;

        // SAFETY: `board` is non-null and this field is a copied scalar.
        let current_wave = unsafe { ptrs::Board::current_wave(board.as_ptr()) };
        if current_wave != expected_current_wave.0 {
            return Err(Pvz1051Error::AbiPreconditionFailed(
                "wave refresh control expected wave is not current",
            ));
        }

        // SAFETY: `board` is non-null and points to the active 1051 Board while playing. These are
        // writable scalar Board fields used by PvZ's wave-refresh state machine.
        unsafe {
            ptrs::Board::set_zombie_refresh_hp(board.as_ptr(), -1);
            ptrs::Board::set_refresh_countdown(board.as_ptr(), remaining_countdown);
            ptrs::Board::set_initial_countdown(board.as_ptr(), initial_countdown);
        }
        Ok(())
    }
}
