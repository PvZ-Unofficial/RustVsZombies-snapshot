//! Compatibility path for the core host-facing frame dispatch result.

pub use crate::runtime::{RuntimeError, RuntimeResult};
pub use rsvz_game::runtime::RuntimeFrameDispatch;

pub use rsvz_game::frame::dispatch_runtime_frame;
