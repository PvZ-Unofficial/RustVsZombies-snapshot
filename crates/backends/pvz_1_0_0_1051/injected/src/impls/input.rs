use rsvz_backend_api::backend::KeyboardStateBackend;
use rsvz_model::model::KeyCode;

use crate::error::Pvz1051Error;
use crate::raw::layout as ptrs;
use crate::runtime::Pvz1051Backend;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

impl KeyboardStateBackend for Pvz1051Backend {
    fn key_is_down(&self, key: KeyCode) -> bool {
        key_is_down(key)
    }

    fn input_focused(&self) -> Result<bool, Self::Error> {
        input_focused(self)
    }
}

fn key_is_down(key: KeyCode) -> bool {
    // SAFETY: GetAsyncKeyState reads process-independent keyboard state for the copied virtual-key
    // value. It does not retain pointers or references into the game process.
    let state = unsafe { GetAsyncKeyState(i32::from(key.raw_virtual_key())) };
    async_key_state_is_down(state)
}

fn input_focused(backend: &Pvz1051Backend) -> Result<bool, Pvz1051Error> {
    let app = backend.app();

    // SAFETY: `app` is a non-null LawnApp pointer. The mHWnd field is copied and only compared
    // against the current foreground window handle.
    let hwnd = unsafe { ptrs::LawnApp::hwnd(app.as_ptr()) };
    if hwnd.is_null() {
        return Ok(false);
    }

    // SAFETY: GetForegroundWindow returns a borrowed OS handle value. It does not give ownership and
    // no pointer derived from it is dereferenced.
    let foreground = unsafe { GetForegroundWindow() };
    Ok(foreground == hwnd)
}

pub(crate) const fn async_key_state_is_down(state: i16) -> bool {
    state < 0
}

#[cfg(test)]
mod tests {
    #[test]
    fn async_key_state_high_bit_means_key_is_down() {
        assert!(super::async_key_state_is_down(i16::MIN));
        assert!(!super::async_key_state_is_down(0));
        assert!(!super::async_key_state_is_down(1));
    }
}
