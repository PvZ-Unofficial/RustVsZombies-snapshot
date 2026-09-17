#![cfg_attr(feature = "capture", feature(fn_traits, unboxed_closures, trivial_bounds))]
#![cfg_attr(feature = "capture", allow(incomplete_features, trivial_bounds))]

//! Deterministic cross-backend Witness data and optional current-world capture.

mod core;
pub use core::*;
pub use rsvz::core::model::SunProductionMode;

#[cfg(feature = "capture")]
mod capture;
#[cfg(feature = "capture")]
pub use capture::{StartWitness, start};
