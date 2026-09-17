//! Backend-neutral advanced-pause capability.

use crate::backend::Backend;
use rsvz_model::model::AdvancedPauseOptions;

/// Capability for backends that can freeze battle logic while keeping selected UI refreshes alive.
pub trait AdvancedPauseBackend: Backend {
    /// Enables or disables advanced pause with backend-neutral options.
    fn set_advanced_pause_with_options(&self, enabled: bool, options: AdvancedPauseOptions) -> Result<(), Self::Error>;

    /// Whether advanced pause is currently active.
    fn advanced_pause_active(&self) -> bool;
}
