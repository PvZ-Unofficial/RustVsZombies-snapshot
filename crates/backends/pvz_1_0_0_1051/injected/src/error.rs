use rsvz_model::model::GameUi;

/// PvZ 1.0.0.1051 injected backend error.
#[derive(Debug, thiserror::Error)]
pub enum Pvz1051Error {
    #[error("game is not ready")]
    GameNotReady,
    #[error("LawnApp pointer is null")]
    NullLawnApp,
    #[error("Board pointer is null")]
    NullBoard,
    #[error("SeedChooser pointer is null")]
    NullSeedChooser,
    #[error("UserData pointer is null")]
    NullUserData,
    #[error("SeedBank pointer is null")]
    NullSeedBank,
    #[error("seed chooser cards are not ready: selected {selected}, slots {slots}, flying {flying}")]
    SeedChooserCardsNotReady { selected: i32, slots: i32, flying: i32 },
    #[error("Challenge pointer is null")]
    NullChallenge,
    #[error("wrong game UI: expected {expected:?}, actual {actual:?}")]
    WrongGameUi { expected: GameUi, actual: GameUi },
    #[error("invalid grid")]
    InvalidGrid,
    #[error("invalid seed slot")]
    InvalidSeedSlot,
    #[error("invalid object id")]
    InvalidObjectId,
    #[error("unsupported tool mode: {0}")]
    UnsupportedToolMode(&'static str),
    #[error("unknown raw {category} kind: {raw}")]
    UnknownRawKind { category: &'static str, raw: i32 },
    #[error("kind is unavailable in this backend or state: {0}")]
    KindUnavailable(&'static str),
    #[error("invalid game speed: {0}")]
    InvalidGameSpeed(f32),
    #[error("ABI precondition failed: {0}")]
    AbiPreconditionFailed(&'static str),
    #[error("backend invariant violated: {0}")]
    InvariantViolated(&'static str),
    #[error("patch failed: {0}")]
    Patch(String),
    #[error("script config error: {0}")]
    ScriptConfig(String),
}

pub(crate) type Result<T> = std::result::Result<T, Pvz1051Error>;

impl From<Pvz1051Error> for rsvz_backend_api::error::RuntimeError {
    fn from(error: Pvz1051Error) -> Self {
        Self::new(error.to_string())
    }
}

impl Pvz1051Error {
    /// An action rejected its input or a supported operation's optional mode.
    /// Required reads must still treat any error as a failed read.
    #[doc(hidden)]
    pub fn is_operation_rejection(&self) -> bool {
        matches!(
            self,
            Self::InvalidGrid
                | Self::InvalidSeedSlot
                | Self::InvalidObjectId
                | Self::UnsupportedToolMode(_)
                | Self::KindUnavailable(_)
                | Self::InvalidGameSpeed(_)
                | Self::ScriptConfig(_)
        )
    }

    /// Missing root access permits an unbound pure callback, but not a required read.
    #[doc(hidden)]
    pub fn is_board_unavailable(&self) -> bool {
        matches!(self, Self::NullLawnApp | Self::NullBoard)
    }
}
