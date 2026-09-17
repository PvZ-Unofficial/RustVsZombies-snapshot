use crate::backend::Backend;
use rsvz_model::model::KeyCode;

/// Backend capability for polling keyboard state.
pub trait KeyboardStateBackend: Backend {
    /// Returns whether `key` is currently physically down.
    fn key_is_down(&self, key: KeyCode) -> bool;

    /// Returns whether keyboard input should currently be accepted for this backend.
    fn input_focused(&self) -> Result<bool, Self::Error>;
}
