//! Error helpers shared by core logic.

use crate::model::{CardSelection, GameMode, GameUi, PlantRejectReason};

/// Backend error classification used for diagnostics and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendErrorKind {
    GameNotReady,
    ObjectUnavailable,
    WrongGameUi { expected: GameUi, actual: GameUi },
    WrongGameMode { expected: GameMode, actual: GameMode },
    InvalidGrid,
    InvalidSeedSlot,
    InvalidObjectId,
    ObjectNotAlive,
    UnknownRawKind,
    KindUnavailable,
    PlantRejected(PlantRejectReason),
    AbiPreconditionFailed,
    BackendInvariantViolated,
}

/// Pure core logic errors.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CoreLogicError {
    /// 输入格子无效。
    #[error("invalid grid")]
    InvalidGrid,
    /// 没有找到可用卡槽。
    #[error("no usable seed slot")]
    NoUsableSeed,
    /// 没有找到指定卡槽。
    #[error("seed slot not found for selection: {0:?}")]
    SeedSlotNotFound(CardSelection),
    /// 重复选卡。
    #[error("duplicate card selection: {0:?}")]
    DuplicateCardSelection(CardSelection),
    /// 无效选卡语义。
    #[error("invalid card selection: {0:?}")]
    InvalidCardSelection(CardSelection),
    /// 同一套卡只允许一张模仿者。
    #[error("multiple imitator cards selected")]
    MultipleImitatorCards,
    /// 选卡数量超过槽位上限。
    #[error("too many cards selected: {selected} > {max}")]
    TooManyCards { selected: usize, max: usize },
    /// 游戏规则拒绝种植。
    #[error("plant rejected: {0:?}")]
    PlantRejected(PlantRejectReason),
    /// 候选格子中没有可种位置。
    #[error("no plantable grid for selection: {0:?}")]
    NoPlantableGrid(CardSelection),
    /// 没有找到可用玉米加农炮。
    #[error("no ready cob cannon")]
    NoReadyCob,
    /// 当前目标不存在或已不可操作。
    #[error("object is no longer actionable")]
    ObjectNotActionable,
}
