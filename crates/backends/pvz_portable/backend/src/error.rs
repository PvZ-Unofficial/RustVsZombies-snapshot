use thiserror::Error;

#[derive(Debug, Error)]
pub enum PortableBackendError {
    #[error("PvZ-Portable has no active Board")]
    BoardUnavailable,
    #[error("stale {kind} handle 0x{id:08x}")]
    StaleHandle { kind: &'static str, id: u32 },
    #[error("invalid {kind} code {value}")]
    InvalidKind { kind: &'static str, value: i32 },
    #[error("numeric value is out of range: {0}")]
    NumericOutOfRange(&'static str),
    #[error("operation is unsupported by PvZ-Portable: {0}")]
    Unsupported(&'static str),
    #[error("PvZ-Portable rejected operation: {0}")]
    OperationRejected(&'static str),
    #[error("PvZ-Portable bridge ABI mismatch")]
    AbiMismatch,
    #[error("PvZ-Portable bridge failed with status {0}")]
    Bridge(i32),
    #[error(transparent)]
    Native(#[from] pvzp_rs::Error),
}

pub type Result<T> = std::result::Result<T, PortableBackendError>;

impl From<PortableBackendError> for rsvz_backend_api::error::RuntimeError {
    fn from(error: PortableBackendError) -> Self {
        Self::new(error.to_string())
    }
}

impl PortableBackendError {
    /// An action rejected its input or a supported operation's optional mode.
    /// Required reads must still treat any error as a failed read.
    #[doc(hidden)]
    pub fn is_operation_rejection(&self) -> bool {
        matches!(
            self,
            Self::OperationRejected(_)
                | Self::Unsupported(_)
                | Self::Native(pvzp_rs::Error::Unsupported { .. } | pvzp_rs::Error::InvalidArgument { .. })
        )
    }

    /// Missing root access permits an unbound pure callback, but not a required read.
    #[doc(hidden)]
    pub fn is_board_unavailable(&self) -> bool {
        matches!(
            self,
            Self::BoardUnavailable | Self::Native(pvzp_rs::Error::NotFound { .. })
        )
    }
}
