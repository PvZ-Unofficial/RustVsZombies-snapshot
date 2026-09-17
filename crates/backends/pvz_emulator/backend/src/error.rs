#[derive(Debug, thiserror::Error)]
pub enum PeBackendError {
    #[error("PE backend is not initialized")]
    NotInitialized,

    #[error("PE current-world scope conflict")]
    WorldScopeConflict,

    #[error("PE backend unsupported operation: {0}")]
    Unsupported(&'static str),

    #[error("PE backend rejected operation: {0}")]
    OperationRejected(&'static str),

    #[error("invalid PE grid")]
    InvalidGrid,

    #[error("invalid PE seed slot")]
    InvalidSeedSlot,

    #[error("invalid PE card selection: {0}")]
    InvalidCardSelection(&'static str),

    #[error("PE backend unsupported {kind}: {name}")]
    UnsupportedKind { kind: &'static str, name: &'static str },

    #[error("PE backend invalid {kind} raw value: {raw}")]
    InvalidKind { kind: &'static str, raw: i32 },

    #[error("PE backend numeric field out of range: {0}")]
    NumericOutOfRange(&'static str),

    #[error("PE backend error: {0}")]
    Pe(#[from] pe_rs::Error),
}

pub type Result<T> = std::result::Result<T, PeBackendError>;

impl PeBackendError {
    pub(crate) const fn unsupported_kind(kind: &'static str, name: &'static str) -> Self {
        Self::UnsupportedKind { kind, name }
    }
}

impl From<PeBackendError> for rsvz_backend_api::error::RuntimeError {
    fn from(error: PeBackendError) -> Self {
        Self::new(error.to_string())
    }
}

impl PeBackendError {
    /// An action rejected its input or a supported operation's optional mode.
    /// Required reads must still treat any error as a failed read.
    #[doc(hidden)]
    pub fn is_operation_rejection(&self) -> bool {
        matches!(
            self,
            Self::OperationRejected(_)
                | Self::Unsupported(_)
                | Self::UnsupportedKind { .. }
                | Self::InvalidGrid
                | Self::InvalidSeedSlot
                | Self::InvalidCardSelection(_)
                | Self::Pe(pe_rs::Error::InvalidArgument { .. })
        )
    }

    /// Missing root access permits an unbound pure callback, but not a required read.
    #[doc(hidden)]
    pub fn is_board_unavailable(&self) -> bool {
        matches!(self, Self::NotInitialized)
    }
}
