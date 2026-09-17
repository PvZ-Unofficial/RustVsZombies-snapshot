//! Current keyboard subscriptions.

use super::{EdgeKind, KeyBindError, KeyBindOptions, binding_callback, poll_edge, with_unique_key_bindings};
use rsvz_backend_api::{KeyboardStateBackend, error::RuntimeResult};
use rsvz_current::CurrentBackend;
use rsvz_model::KeyCode;
use rsvz_schedule::tick::{TickHandle, TickTaskState, with_scheduler};

/// Registers a callback that fires on the focused rising edge of `key`.
pub fn on_press<F>(key: KeyCode, f: F) -> TickHandle
where
    CurrentBackend: KeyboardStateBackend,
    F: FnMut() -> RuntimeResult<()> + 'static,
{
    on_press_with_options::<F>(key, KeyBindOptions::default(), f)
}

/// Registers a callback that fires on the focused falling edge of `key`.
pub fn on_release<F>(key: KeyCode, f: F) -> TickHandle
where
    CurrentBackend: KeyboardStateBackend,
    F: FnMut() -> RuntimeResult<()> + 'static,
{
    on_release_with_options::<F>(key, KeyBindOptions::default(), f)
}

/// Registers a focused rising-edge callback with custom tick options.
pub fn on_press_with_options<F>(key: KeyCode, options: KeyBindOptions, f: F) -> TickHandle
where
    CurrentBackend: KeyboardStateBackend,
    F: FnMut() -> RuntimeResult<()> + 'static,
{
    spawn_binding::<F>(key, options, EdgeKind::Press, f)
}

/// Registers a focused falling-edge callback with custom tick options.
pub fn on_release_with_options<F>(key: KeyCode, options: KeyBindOptions, f: F) -> TickHandle
where
    CurrentBackend: KeyboardStateBackend,
    F: FnMut() -> RuntimeResult<()> + 'static,
{
    spawn_binding::<F>(key, options, EdgeKind::Release, f)
}

/// Converts `key` to `KeyCode` and registers a focused rising-edge callback.
pub fn try_on_press<K, F>(key: K, f: F) -> Result<TickHandle, KeyBindError>
where
    CurrentBackend: KeyboardStateBackend,
    K: TryInto<KeyCode>,
    K::Error: Into<KeyBindError>,
    F: FnMut() -> RuntimeResult<()> + 'static,
{
    try_on_press_with_options::<K, F>(key, KeyBindOptions::default(), f)
}

/// Converts `key` to `KeyCode` and registers a focused falling-edge callback.
pub fn try_on_release<K, F>(key: K, f: F) -> Result<TickHandle, KeyBindError>
where
    CurrentBackend: KeyboardStateBackend,
    K: TryInto<KeyCode>,
    K::Error: Into<KeyBindError>,
    F: FnMut() -> RuntimeResult<()> + 'static,
{
    try_on_release_with_options::<K, F>(key, KeyBindOptions::default(), f)
}

/// Converts `key` to `KeyCode` and registers a focused rising-edge callback with options.
pub fn try_on_press_with_options<K, F>(key: K, options: KeyBindOptions, f: F) -> Result<TickHandle, KeyBindError>
where
    CurrentBackend: KeyboardStateBackend,
    K: TryInto<KeyCode>,
    K::Error: Into<KeyBindError>,
    F: FnMut() -> RuntimeResult<()> + 'static,
{
    Ok(on_press_with_options::<F>(
        key.try_into().map_err(Into::into)?,
        options,
        f,
    ))
}

/// Converts `key` to `KeyCode` and registers a focused falling-edge callback with options.
pub fn try_on_release_with_options<K, F>(key: K, options: KeyBindOptions, f: F) -> Result<TickHandle, KeyBindError>
where
    CurrentBackend: KeyboardStateBackend,
    K: TryInto<KeyCode>,
    K::Error: Into<KeyBindError>,
    F: FnMut() -> RuntimeResult<()> + 'static,
{
    Ok(on_release_with_options::<F>(
        key.try_into().map_err(Into::into)?,
        options,
        f,
    ))
}

/// Converts `key` to a known `KeyCode` and registers an AvZ2-style unique press callback.
pub fn try_on_press_unique<K, F>(key: K, f: F) -> Result<TickHandle, KeyBindError>
where
    CurrentBackend: KeyboardStateBackend,
    K: TryInto<KeyCode>,
    K::Error: Into<KeyBindError>,
    F: FnMut() -> RuntimeResult<()> + 'static,
{
    try_on_press_unique_with_options::<K, F>(key, KeyBindOptions::default(), f)
}

/// Converts `key` to a known `KeyCode` and registers a unique press callback with options.
pub fn try_on_press_unique_with_options<K, F>(key: K, options: KeyBindOptions, f: F) -> Result<TickHandle, KeyBindError>
where
    CurrentBackend: KeyboardStateBackend,
    K: TryInto<KeyCode>,
    K::Error: Into<KeyBindError>,
    F: FnMut() -> RuntimeResult<()> + 'static,
{
    try_spawn_unique_binding::<K, F>(key, options, EdgeKind::Press, f)
}

/// Converts `key` to a known `KeyCode` and registers an AvZ2-style unique release callback.
pub fn try_on_release_unique<K, F>(key: K, f: F) -> Result<TickHandle, KeyBindError>
where
    CurrentBackend: KeyboardStateBackend,
    K: TryInto<KeyCode>,
    K::Error: Into<KeyBindError>,
    F: FnMut() -> RuntimeResult<()> + 'static,
{
    try_on_release_unique_with_options::<K, F>(key, KeyBindOptions::default(), f)
}

/// Converts `key` to a known `KeyCode` and registers a unique release callback with options.
pub fn try_on_release_unique_with_options<K, F>(
    key: K, options: KeyBindOptions, f: F,
) -> Result<TickHandle, KeyBindError>
where
    CurrentBackend: KeyboardStateBackend,
    K: TryInto<KeyCode>,
    K::Error: Into<KeyBindError>,
    F: FnMut() -> RuntimeResult<()> + 'static,
{
    try_spawn_unique_binding::<K, F>(key, options, EdgeKind::Release, f)
}

fn spawn_binding<F>(key: KeyCode, options: KeyBindOptions, edge: EdgeKind, callback: F) -> TickHandle
where
    CurrentBackend: KeyboardStateBackend,
    F: FnMut() -> RuntimeResult<()> + 'static,
{
    let task = binding_callback(
        move |was_down| poll_edge(key, options.require_focus, edge, was_down),
        callback,
    );
    with_scheduler(|scheduler| scheduler.spawn(options.repeating_tick_options(), task))
}

fn try_spawn_unique_binding<K, F>(
    key: K, options: KeyBindOptions, edge: EdgeKind, callback: F,
) -> Result<TickHandle, KeyBindError>
where
    CurrentBackend: KeyboardStateBackend,
    K: TryInto<KeyCode>,
    K::Error: Into<KeyBindError>,
    F: FnMut() -> RuntimeResult<()> + 'static,
{
    let key = key.try_into().map_err(Into::into)?;
    if !key.is_known_virtual_key() {
        return Err(KeyBindError::UnknownKey { key });
    }
    if unique_binding_is_active(key) {
        return Err(KeyBindError::DuplicateBinding { key });
    }

    let handle = spawn_binding::<F>(key, options, edge, callback);
    register_unique_binding(key, handle);
    Ok(handle)
}

fn unique_binding_is_active(key: KeyCode) -> bool {
    let handle = with_unique_key_bindings(|bindings| bindings.get(&key).copied());
    let Some(handle) = handle else {
        return false;
    };
    match with_scheduler(|scheduler| scheduler.state(handle)) {
        TickTaskState::Running | TickTaskState::Paused => true,
        TickTaskState::Stopped => {
            remove_unique_binding(key);
            false
        }
    }
}

fn register_unique_binding(key: KeyCode, handle: TickHandle) {
    with_unique_key_bindings(|bindings| {
        bindings.insert(key, handle);
    });
}

fn remove_unique_binding(key: KeyCode) {
    with_unique_key_bindings(|bindings| {
        bindings.remove(&key);
    });
}
