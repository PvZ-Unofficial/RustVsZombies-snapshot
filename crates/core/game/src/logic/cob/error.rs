use crate::model::Grid;

/// 纯炮管理器逻辑产生的错误。
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum CobManagerError {
    #[error("炮列表为空")]
    EmptyList,
    #[error("下一门炮的游标无效")]
    InvalidNext,
    #[error("炮落点无效")]
    InvalidTarget,
    #[error("当前炮序模式不支持该操作")]
    InvalidModeForOperation,
    #[error("炮位 {0:?} 上没有玉米加农炮")]
    CobNotFound(Grid),
    #[error("炮位 {0:?} 不在炮列表中")]
    CobNotInList(Grid),
    #[error("没有可用的玉米加农炮")]
    NoReadyCob,
    #[error("没有可恢复的玉米加农炮")]
    NoRecoverableCob,
    #[error("没有最近一次发炮记录")]
    NoLatestCob,
    #[error("该操作只适用于屋顶场景")]
    NonRoofScene,
}

/// 炮管理器单项详细操作产生的 backend/core 分层错误。
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CobManagerCallError {
    #[error(transparent)]
    Backend(crate::runtime::RuntimeError),
    #[error(transparent)]
    Manager(#[from] CobManagerError),
}
