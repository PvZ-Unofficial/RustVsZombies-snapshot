use crate::{PortableBackend, Result};
use pvzp_rs::capabilities as native;
use rsvz_backend_api::backend::{
    AudioBackend, CobImpactDelayBackend, CoinProfileBackend, CommonZombieDanceBackend, GardenBackend,
    HiddenModeUnlockBackend, InputBackend, TrophyUnlockBackend,
};
use rsvz_model::model::{Grid, MouseButton, PixelPos, RefreshDance, SoundId};

impl rsvz_backend_api::DanceModeBackend for PortableBackend {
    fn set_dance_mode(&self, enabled: bool) -> Result<()> {
        native::set_dance_mode(enabled)?;
        Ok(())
    }
}

impl CommonZombieDanceBackend for PortableBackend {
    fn set_common_zombie_dance(&self, dance: RefreshDance) -> Result<()> {
        native::set_common_dance(match dance {
            RefreshDance::Unchanged | RefreshDance::None => 0,
            RefreshDance::Fast => 1,
            RefreshDance::Slow => 2,
        })?;
        Ok(())
    }
}
impl CobImpactDelayBackend for PortableBackend {
    fn set_cob_impact_delay(&self, enabled: bool) -> Result<()> {
        Ok(native::set_cob_impact_delay(enabled)?)
    }
}
impl CoinProfileBackend for PortableBackend {
    fn coins(&self) -> Result<u32> {
        Ok(native::coins()?)
    }
    fn set_coins(&self, value: u32) -> Result<()> {
        Ok(native::set_coins(value)?)
    }
}
impl InputBackend for PortableBackend {
    fn mouse_move(&mut self, pos: PixelPos) -> Result<()> {
        // SAFETY: &mut token excludes frames/entities; BackendScope guards implicit borrows.
        Ok(unsafe { native::mouse(0, pos.x, pos.y, 1) }?)
    }
    fn mouse_down(&mut self, pos: PixelPos, button: MouseButton) -> Result<()> {
        // SAFETY: exclusive access survives nested UI; the host prevents reentrant dispatch.
        Ok(unsafe { native::mouse(1, pos.x, pos.y, mouse_button(button)) }?)
    }
    fn mouse_up(&mut self, pos: PixelPos, button: MouseButton) -> Result<()> {
        // SAFETY: exclusive access; the existing access epoch invalidates pre-input samples.
        Ok(unsafe { native::mouse(2, pos.x, pos.y, mouse_button(button)) }?)
    }
    fn release_mouse(&mut self) -> Result<()> {
        // SAFETY: the exclusive token excludes cursor/Board/entity borrows.
        Ok(unsafe { native::mouse(3, 0, 0, 1) }?)
    }
}
fn mouse_button(button: MouseButton) -> i32 {
    match button {
        MouseButton::Left => 1,
        MouseButton::Right => -1,
    }
}
impl AudioBackend for PortableBackend {
    fn play_sound(&self, sound: SoundId) -> Result<()> {
        Ok(native::sound(sound.0, false)?)
    }
    fn stop_sound(&self, sound: SoundId) -> Result<()> {
        Ok(native::sound(sound.0, true)?)
    }
}
impl GardenBackend for PortableBackend {
    fn water_plant(&self, grid: Grid) -> Result<()> {
        Ok(native::water_plant(grid.row, grid.col)?)
    }
}
impl TrophyUnlockBackend for PortableBackend {
    fn unlock_trophy(&self) -> Result<()> {
        Ok(native::unlock_trophy()?)
    }
}
impl HiddenModeUnlockBackend for PortableBackend {
    fn unlock_hidden_modes(&self) -> Result<()> {
        Ok(native::unlock_hidden_modes()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn implicit_shared_access_rejects_exclusive_input_scope() {
        let mut backend = PortableBackend::new();
        crate::scope_backend(&mut backend, || {
            crate::with_backend_shared(|_| {
                assert!(matches!(
                    crate::try_with_backend(|_| panic!("exclusive closure must not run")),
                    Err(rsvz_backend_api::access::BackendAccessError::BorrowConflict)
                ));
            })
            .unwrap();
            let before = crate::backend_access_epoch();
            crate::try_with_backend(|_| ()).unwrap();
            assert_ne!(before, crate::backend_access_epoch());
        });
    }
}
