use std::ptr::NonNull;

use rsvz_backend_api::backend::{
    BoardSupportBackend, GameUiBackend, GridItemCreateBackend, GridItemEditBackend, LawnMowerClearBackend,
    SceneBackend, SceneEditBackend,
};
use rsvz_model::model::{GameUi, Grid, SceneKind};

use crate::error::{Pvz1051Error, Result};
use crate::raw::layout as ptrs;
use crate::runtime::Pvz1051Backend;

mod card;
mod lineup;
mod plant_edit;
mod seed_query;
mod spawn;
mod zombie;

const GAME_MODE_SURVIVAL_NORMAL: i32 = 11;
const GAME_MODE_SURVIVAL_ENDLESS: i32 = 15;

fn is_survival_mode(mode: i32) -> bool {
    (GAME_MODE_SURVIVAL_NORMAL..=GAME_MODE_SURVIVAL_ENDLESS).contains(&mode)
}

impl Pvz1051Backend {
    pub(in crate::impls::gameplay) fn raw_game_mode(&self) -> Result<i32> {
        let app = self.app();
        // SAFETY: `app` is non-null and points to the current LawnApp root.
        Ok(unsafe { ptrs::LawnApp::game_mode(app.as_ptr()) })
    }
}
