//! Backend-neutral fast-forward capability.

use crate::backend::Backend;
use rsvz_model::model::{FastForwardOptions, FastForwardStopReason, SeedChooserFastForwardOptions};

/// Capability for high-speed sequential logic-frame advancement.
///
/// Fast-forward must still process intermediate logical frames in order. It is not permission to
/// skip simulation frames or synthesize elapsed time without running gameplay updates.
pub trait FastForwardBackend: Backend {
    /// Starts physical fast-forwarding with backend optimization options.
    fn start_fast_forward(&self, options: FastForwardOptions) -> Result<(), Self::Error>;

    /// Stop fast-forwarding and restore backend state.
    fn stop_fast_forward(&self, reason: FastForwardStopReason) -> Result<(), Self::Error>;

    /// Whether the backend is currently running or able to run in high-speed fast-forward mode.
    fn fast_forward_active(&self) -> bool;
}

/// Capability for high-speed level-intro seed chooser UI advancement.
pub trait SeedChooserFastForwardBackend: Backend {
    /// Request fast-forwarding the current or next level-intro seed chooser UI.
    fn request_seed_chooser_fast_forward(&self, options: SeedChooserFastForwardOptions) -> Result<(), Self::Error>;
}
