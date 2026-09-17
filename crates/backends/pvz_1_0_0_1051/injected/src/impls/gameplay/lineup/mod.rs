use super::*;
use crate::impls::handles::PvzGridItemHandle;

mod grid_items;
mod scene;
use grid_items::*;
use scene::*;

impl BoardSupportBackend for Pvz1051Backend {
    fn ensure_board_supported(&self) -> Result<()> {
        match self.game_ui()? {
            GameUi::LevelIntro | GameUi::Playing => {}
            _ => {
                return Err(Pvz1051Error::UnsupportedToolMode(
                    "lineup automation requires level-intro or playing UI",
                ));
            }
        }
        let _board = self.board()?;
        let mode = self.raw_game_mode()?;
        if !is_survival_mode(mode) {
            return Err(Pvz1051Error::UnsupportedToolMode(
                "lineup automation currently supports Survival modes only",
            ));
        }
        Ok(())
    }
}

impl LawnMowerClearBackend for Pvz1051Backend {
    fn clear_lawn_mowers(&self) -> Result<()> {
        let board = self.board()?;
        clear_lawn_mowers(board);
        Ok(())
    }
}

impl SceneEditBackend for Pvz1051Backend {
    fn set_scene(&mut self, scene: SceneKind) -> Result<()> {
        self.ensure_board_supported()?;
        ensure_lineup_scene_switch_supported(scene)?;
        let current = self.scene()?;
        if current == scene {
            return Ok(());
        }

        let board = self.board()?;
        match self.game_ui()? {
            GameUi::LevelIntro => {}
            GameUi::Playing => {
                // SAFETY: `board` is non-null and points to the active Board; current_wave is a
                // copied scalar. Scene switching is limited to the opening wave-0 setup window.
                if unsafe { ptrs::Board::current_wave(board.as_ptr()) } > 0 {
                    return Err(Pvz1051Error::AbiPreconditionFailed(
                        "lineup scene switching is not allowed after zombies have spawned",
                    ));
                }
            }
            _ => {
                return Err(Pvz1051Error::UnsupportedToolMode(
                    "lineup scene switching requires level-intro or opening playing UI",
                ));
            }
        }

        let switch_state = prepare_lineup_scene_switch(self, board, scene)?;

        // SAFETY: UI, Board, Survival mode, regular scene kind, and all later scene-switch
        // dependencies were checked above. 0x40A160 is Board::LoadBackgroundImages; after writing
        // mBackground it loads the resources needed by the target scene.
        unsafe {
            *ptrs::Board::scene_mut(board.as_ptr()) = scene.code();
            crate::raw::abi::board_load_background_images(board.as_ptr());
        }
        write_lineup_scene_terrain(board, scene);
        if let Some(effect_system) = switch_state.effect_system {
            clear_pool_wave_particles(effect_system, board);
        }
        reset_scene_lawn_items(board, switch_state);
        play_lineup_scene_music(scene);
        Ok(())
    }
}

impl GridItemCreateBackend for Pvz1051Backend {
    fn add_ladder<'a>(&'a self, grid: Grid) -> Result<PvzGridItemHandle<'a>> {
        let _board = grid_item_create_board(self, grid)?;
        Ok(PvzGridItemHandle::new(add_ladder(grid)?))
    }

    fn add_crater<'a>(&'a self, grid: Grid) -> Result<PvzGridItemHandle<'a>> {
        let _board = grid_item_create_board(self, grid)?;
        Ok(PvzGridItemHandle::new(add_crater(grid)?))
    }

    fn add_gravestone<'a>(&'a self, grid: Grid) -> Result<PvzGridItemHandle<'a>> {
        let board = grid_item_create_board(self, grid)?;
        Ok(PvzGridItemHandle::new(add_gravestone(board, grid)?))
    }
}

impl GridItemEditBackend for Pvz1051Backend {
    fn remove_grid_item<'a>(&'a self, handle: Self::GridItemHandle<'a>) -> Result<()> {
        // SAFETY: `handle` came from an occupied, non-dead DataArray slot.
        unsafe { crate::raw::abi::grid_item_die(handle.ptr.as_ptr()) };
        Ok(())
    }
}
