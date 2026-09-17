//! 面向当前 runtime 的 low DSL。
//!
//! DSL 函数返回 [`Expr`]，只描述相对时间上的操作；构造表达式本身不会种卡、
//! 铲除或发炮。必须先用 [`wave`] / [`waves`] 选择波次，再用 `time << expr`
//! 将表达式连接到绝对波内时间，操作才会进入 timeline。
//!
//! ```rust,no_run
//! # #[cfg(any(feature = "pvz-emulator", feature = "pvz-1-0-0-1051", feature = "pvz-portable"))]
//! # {
//! # use rsvz::dsl::prelude::*;
//! # {
//! wave(1);
//! 359 << pp() + d(107) + p(15, 7.8);
//! # }
//! # }
//! ```
//!
//! [`P`] (`p`)/[`Pp`] (`pp`)/[`rp`] 的语义时间是炮弹命中参考时间，DSL 会根据场景换算
//! 实际发炮时刻：普通陆路 `373cs`、泳池水路 `378cs`、屋顶统一参考
//! `387cs`。通用 [`DslCard`] (`card`) 与 [`DslShovel`] (`shovel`) 的语义
//! 时间则分别是实际种卡和实际铲除时刻；具有生效时间修正的樱桃、辣椒、
//! 冰、核等别名会自行附加偏移。
//!
//! DSL 构造时发现的坐标、卡片或时间错误属于注册错误，相应无效表达式不会
//! 部分注册。执行时一个 operation 的普通 `RuntimeError` 会被报告并结束该
//! 最小工作单元，但同帧其他 operation 仍会继续。这里的 [`DslTryCard`]
//! (`try_card`) 仍然返回 `Expr`：`try` 表示普通种植失败可忽略，并不是即时
//! API 的 `RuntimeResult` 返回形式。

mod card;
mod effect;
mod ensure;
mod expr;
mod input;
mod primitives;

pub use card::*;
pub use effect::*;
pub use ensure::{IntoZombieKindArg, IntoZombieRows, ensure_exist};
pub use expr::{Expr, LowExpr, empty};
pub use input::{IntoWaveSet, WaveSet, wave, waves};
pub use primitives::*;

/// 导入 `#[rsvz::script]` 使用的完整 low DSL 表面。
pub mod prelude {
    pub use super::card::*;
    pub use super::effect::*;
    pub use super::ensure::{IntoZombieKindArg, IntoZombieRows, ensure_exist};
    pub use super::primitives::*;
    pub use super::{Expr, IntoWaveSet, LowExpr, WaveSet, empty, wave, waves};
    pub use crate::CobManager;
    pub use crate::Lineup;
    pub use crate::cards::{card_cd, set_sun_cost_ignored};
    pub use crate::cob::set_cob_columns;
    pub use crate::setup::{lineup, set_wave_zombies, set_zombies};
    pub use crate::setup::{
        random_zombie_types, reload, select_cards, skip_between, skip_seed_chooser, skip_seed_chooser_with_options,
        skip_until,
    };
    pub use rsvz_model::model::ReloadMode::*;
    pub use rsvz_model::model::ZombieSpawnMode::{Average, Exact, Natural};
    pub use rsvz_model::model::{
        CardSelection, CobTarget, Grid, PlantId, PlantKind, RelativeTime, SeedSlot, ZombieKind, ZombieSpawnMode,
    };
}

pub(crate) use ensure::begin_script;
