//! Atomic capabilities backed by the current native App/profile.
use crate::{Error, Result, raw};

pub fn set_dance_mode(enabled: bool) -> Result<()> {
    // SAFETY: the wrapper checks Board and performs a non-recycling native operation.
    Error::from_status(unsafe { raw::pvzp_rs_set_dance_mode(u8::from(enabled)) })
}

pub fn set_common_dance(dance: i32) -> Result<()> {
    // SAFETY: the native wrapper validates the value and current Board.
    Error::from_status(unsafe { raw::pvzp_rs_set_common_dance(dance) })
}
pub fn set_cob_impact_delay(enabled: bool) -> Result<()> {
    // SAFETY: the native wrapper checks the current Board.
    Error::from_status(unsafe { raw::pvzp_rs_set_cob_impact_delay(u8::from(enabled)) })
}
pub fn coins() -> Result<u32> {
    let mut value = 0;
    // SAFETY: the out pointer is live and the native wrapper validates the profile.
    Error::from_status(unsafe { raw::pvzp_rs_coins(&mut value) })?;
    Ok(value)
}
pub fn set_coins(value: u32) -> Result<()> {
    // SAFETY: native validates the profile and amount before writing.
    Error::from_status(unsafe { raw::pvzp_rs_set_coins(value) })
}
/// # Safety
/// Must own exclusive backend access, without any live entity/Board borrows.
/// A modal input runs native UI until the modal closes, and may replace Board.
pub unsafe fn mouse(operation: i32, x: i32, y: i32, button: i32) -> Result<()> {
    // SAFETY: the caller holds exclusive access; native validates all inputs.
    Error::from_status(unsafe { raw::pvzp_rs_mouse(operation, x, y, button) })
}
pub fn sound(id: u32, stop: bool) -> Result<()> {
    // SAFETY: native validates the sound and owns its playback instances.
    Error::from_status(unsafe { raw::pvzp_rs_sound(id, u8::from(stop)) })
}
pub fn water_plant(row: i32, col: i32) -> Result<()> {
    // SAFETY: native validates the garden and plant, and does not recycle slots.
    Error::from_status(unsafe { raw::pvzp_rs_water_plant(row, col) })
}
pub fn unlock_trophy() -> Result<()> {
    // SAFETY: native validates the profile and only edits trophy records.
    Error::from_status(unsafe { raw::pvzp_rs_unlock_trophy() })
}
pub fn unlock_hidden_modes() -> Result<()> {
    // SAFETY: native validates the current screen and updates existing buttons.
    Error::from_status(unsafe { raw::pvzp_rs_unlock_hidden_modes() })
}
