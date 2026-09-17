//! `rsvz::core` reexports operations and types from their owning core modules.
//!
//! Current operations are directly usable by scripts; input adaptation and DSL live in the script API.
//!
//! `card`/`shovel` 的独立 `row, col` 参数以及炮目标元组仍使用脚本
//! `1-based` 坐标；显式 [`model::Grid`] / [`model::CobTarget`] 遵守各自的
//! core 字段约定。更完整的模型、backend trait 与自然功能模块分别位于
//! [`model`], [`backend`] 和 [`logic`]。

pub use rsvz_game::logic::cards::{
    card, card_any, card_cd, card_slot, card_slot_any, cards, cards_any, cards_any_iter,
};
pub use rsvz_game::logic::cob::{fire, fire_many, raw_fire, recover_fire, recover_fire_many};
pub use rsvz_game::logic::shovel::{shovel, shovel_many, shovel_ops, shovel_target};
pub use rsvz_game::logic::zombies::set_spawn_wave;
pub use rsvz_game::modifier::remove_plant_by_id;

pub(crate) use rsvz_game::logic::cards::{card_any_prepared, card_slot_any_prepared, validate_card_selection};

pub mod backend {
    pub use rsvz_backend_api::backend::*;
    pub use rsvz_game::logic::ContactGeometryBackend;
}

pub mod logic {
    pub use rsvz_game::logic::*;
}

pub mod model {
    pub use rsvz_game::lineup::{
        self, Lineup, LineupApplyOptions, LineupBase, LineupCell, LineupParseError, LineupPlant, LineupReloadPolicy,
        PendingLineup,
    };
    pub use rsvz_game::logic::cob::{CobListOrder, CobRecoverInfo, CobSequentialMode, IntoCobTarget};
    pub use rsvz_game::logic::zombie_motion::{VanillaZombieMotionRules, ZombieMotionRules};
    pub use rsvz_model::{model, model::*};
}

pub mod measure {
    pub use rsvz_game::measure::*;
}

pub mod modifier {
    pub use rsvz_game::modifier::*;
}

pub mod profiling {
    pub use rsvz_profiling::*;
}

pub mod runtime {
    pub use crate::runtime::*;
}

pub mod script {
    pub use crate::script::{ScriptError, ScriptResult};
}

pub mod setup {
    pub use rsvz_game::setup::*;
}

pub mod tick {
    pub use rsvz_schedule::tick::*;
}

pub mod timeline {
    pub use rsvz_schedule::timeline::*;
}

pub use backend::Backend;
pub use model::{CardSelection, Grid, KeyCode, PixelPos, PlantKind, Position, ZombieKind};

pub use rsvz_game::timing;
