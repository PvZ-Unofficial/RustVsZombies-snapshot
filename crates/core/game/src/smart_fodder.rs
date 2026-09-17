//! Smart fodder solving and current scheduling.
pub use crate::logic::smart_fodder::*;
mod current;
pub use current::{ScheduledFodder, predict_c9_remove_by, smart_fodder, try_smart_fodder};
