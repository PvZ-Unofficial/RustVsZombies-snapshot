//! Cross-frame snapshots.

use crate::model::{
    CardSelection, Grid, GridItemId, GridItemKind, PlantId, PlantKind, ProjectileId, SeedSlot, ZombieId, ZombieKind,
};

/// 僵尸 snapshot。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZombieSnapshot {
    pub id: ZombieId,
    pub kind: ZombieKind,
    pub hp: i32,
    pub row: i32,
    pub alive: bool,
}

/// Fresh-spawn zombie snapshot used by spawn-layout edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZombieSpawnSnapshot {
    pub id: ZombieId,
    pub kind: ZombieKind,
    pub row: i32,
}

/// 植物 snapshot。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlantSnapshot {
    pub id: PlantId,
    pub kind: PlantKind,
    pub hp: i32,
    pub grid: Grid,
    pub alive: bool,
}

/// 卡槽 snapshot。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeedSnapshot {
    pub slot: SeedSlot,
    pub selection: CardSelection,
    pub usable: bool,
}

/// 场地物件 snapshot。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridItemSnapshot {
    pub id: GridItemId,
    pub kind: GridItemKind,
    pub grid: Grid,
}

/// 投射物 snapshot。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectileSnapshot {
    pub id: ProjectileId,
    pub x: f32,
    pub y: f32,
}

/// 战斗 snapshot。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BattleSnapshot {
    pub zombies: Vec<ZombieSnapshot>,
    pub plants: Vec<PlantSnapshot>,
    pub seeds: Vec<SeedSnapshot>,
}

/// Typed simulation snapshot boundary.
///
/// This is the public shape reserved for future cross-backend battle-state exchange. The current
/// feature only defines the typed boundary; no backend is required to import or export this model.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SimulationSnapshot {
    pub battle: BattleSnapshot,
}
