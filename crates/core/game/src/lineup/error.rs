/// Parser and portable model validation failures.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LineupParseError {
    #[error("lineup input is empty")]
    EmptyInput,
    #[error("lineup format is not recognized")]
    UnknownFormat,
    #[error("lineup code is not valid Base64/Base64Url")]
    InvalidBase64,
    #[error("lineup payload is invalid")]
    InvalidPayload,
    #[error("lineup zlib payload could not be decompressed")]
    DecompressFailed,
    #[error("unexpected decompressed length: actual {actual}, expected {expected}")]
    UnexpectedDecompressedLength { actual: usize, expected: usize },
    #[error("unexpected cell count: actual {actual}, expected {expected}")]
    UnexpectedCellCount { actual: usize, expected: usize },
    #[error("invalid lineup scene: {0}")]
    InvalidScene(u8),
    #[error("invalid lineup grid row {row} col {col}")]
    InvalidGrid { row: i32, col: i32 },
    #[error("unknown lineup plant code: {0}")]
    UnknownPlant(u16),
    #[error("invalid lineup card selection: {0}")]
    InvalidCardSelection(#[from] rsvz_model::CardSelectionError),
    #[error("unsupported lineup data: {0}")]
    UnsupportedLineupData(&'static str),
    #[error("unsupported legacy lineup field: {0}")]
    UnsupportedLegacyField(String),
}
