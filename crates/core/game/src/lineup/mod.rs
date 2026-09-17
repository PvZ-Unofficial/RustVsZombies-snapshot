//! Parsed lineup model and portable lineup-code parsers.

mod apply;
mod cell;
mod error;
mod model;
mod parse;
mod validate;

#[cfg(test)]
mod tests;

pub use apply::LineupApplyOptions;
pub use apply::apply_lineup;
pub use apply::{ApplyLineupError, LineupApplyBackend};
pub use cell::{LineupBase, LineupCell, LineupPlant};
pub use error::LineupParseError;
pub use model::Lineup;

pub use crate::setup::{LineupReloadPolicy, PendingLineup};
