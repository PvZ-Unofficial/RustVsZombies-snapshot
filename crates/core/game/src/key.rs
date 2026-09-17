//! Keyboard edge detection and current key subscription.

use rsvz_backend_api::backend::KeyboardStateBackend;
use rsvz_backend_api::error::RuntimeError;
use rsvz_backend_api::error::RuntimeResult;
use rsvz_model::KeyCode;
use rsvz_schedule::tick::TickTrigger;
use rsvz_schedule::tick::{TickFrameGate, TickOptions};

/// Keyboard binding registration options.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyBindOptions {
    pub require_focus: bool,
    pub tick_options: TickOptions,
}

impl KeyBindOptions {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            require_focus: true,
            tick_options: TickOptions::active_phase().frame_gate(TickFrameGate::EveryDispatch),
        }
    }

    /// Changes whether callbacks require the backend input focus check to pass.
    #[must_use]
    pub const fn require_focus(mut self, require_focus: bool) -> Self {
        self.require_focus = require_focus;
        self
    }

    /// Changes tick dispatch options. The trigger is always forced to `Repeating` at registration.
    #[must_use]
    pub const fn tick_options(mut self, tick_options: TickOptions) -> Self {
        self.tick_options = tick_options;
        self
    }

    const fn repeating_tick_options(self) -> TickOptions {
        self.tick_options.trigger(TickTrigger::Repeating)
    }
}

impl Default for KeyBindOptions {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy)]
enum EdgeKind {
    Press,
    Release,
}

fn poll_edge(key: KeyCode, require_focus: bool, edge: EdgeKind, was_down: &mut bool) -> RuntimeResult<bool>
where
    rsvz_current::CurrentBackend: KeyboardStateBackend,
{
    crate::access::with_backend(|backend| {
        let down = backend.key_is_down(key);
        let focused = if require_focus {
            backend
                .input_focused()
                .map_err(|error| RuntimeError::new(error.to_string()))?
        } else {
            true
        };
        Ok(edge_triggered(down, focused, edge, was_down))
    })
}

fn edge_triggered(down: bool, focused: bool, edge: EdgeKind, was_down: &mut bool) -> bool {
    let fire = match edge {
        EdgeKind::Press => focused && down && !*was_down,
        EdgeKind::Release => focused && !down && *was_down,
    };
    *was_down = down;
    fire
}

use crate::resources::UniqueKeyBindingsResource;
use rsvz_model::KeyCodeError;
use rsvz_schedule::tick::TickHandle;
use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::Infallible;

thread_local! {
    static UNIQUE_KEY_BINDINGS: RefCell<UniqueKeyBindingsResource> =
        RefCell::new(UniqueKeyBindingsResource::default());
}

pub(crate) fn with_unique_key_bindings<R>(f: impl FnOnce(&mut HashMap<KeyCode, TickHandle>) -> R) -> R {
    crate::resources::ensure_script_reset(&UNIQUE_KEY_BINDINGS);
    UNIQUE_KEY_BINDINGS.with_borrow_mut(|resource| f(&mut resource.bindings))
}

/// Keyboard binding registration error.
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum KeyBindError {
    #[error(transparent)]
    KeyCode(#[from] KeyCodeError),
    #[error("key {key} already has an active unique binding")]
    DuplicateBinding { key: KeyCode },
    #[error("key {key} is not in the known virtual-key map")]
    UnknownKey { key: KeyCode },
}

impl From<Infallible> for KeyBindError {
    fn from(value: Infallible) -> Self {
        match value {}
    }
}

mod current;
pub use current::{
    on_press, on_press_with_options, on_release, on_release_with_options, try_on_press, try_on_press_unique,
    try_on_press_unique_with_options, try_on_press_with_options, try_on_release, try_on_release_unique,
    try_on_release_unique_with_options, try_on_release_with_options,
};

fn binding_callback<P, F>(
    mut poll: P, mut callback: F,
) -> impl FnMut(rsvz_schedule::TickMeta) -> RuntimeResult<rsvz_schedule::TickControl>
where
    P: FnMut(&mut bool) -> RuntimeResult<bool>,
    F: FnMut() -> RuntimeResult<()>,
{
    let mut was_down = false;
    move |_| {
        if poll(&mut was_down)? {
            callback()?;
        }
        Ok(rsvz_schedule::TickControl::Continue)
    }
}

#[cfg(test)]
mod tests;
