use std::marker::PhantomData;
use std::rc::Rc;

use rsvz_backend_api::backend::Backend;
use rsvz_backend_api::opening::{SeedChooserReadiness, classify_seed_chooser_readiness};

use crate::PortableBackendError;

/// ZST token lent on the native game thread while LawnApp is alive.
/// A token does not imply a Board; menu callbacks may run without one.
pub struct PortableBackend {
    _thread_bound: PhantomData<Rc<()>>,
}

impl PortableBackend {
    pub(crate) const fn new() -> Self {
        Self {
            _thread_bound: PhantomData,
        }
    }

    pub(crate) fn world(&self) -> Result<pvzp_rs::World<'_>, PortableBackendError> {
        // SAFETY: the host lends this token only inside a live LawnApp ingress.
        // The root is read on demand; a menu is allowed to have no Board.
        let raw = unsafe { pvzp_rs::raw::pvzp_rs_board_from_live_app() };
        let board = std::ptr::NonNull::new(raw).ok_or(PortableBackendError::BoardUnavailable)?;
        // SAFETY: this shared token borrow prevents Board invalidation.
        Ok(unsafe { pvzp_rs::World::from_non_null(board) })
    }

    pub(crate) fn seed_chooser_readiness(&self) -> Result<SeedChooserReadiness, PortableBackendError> {
        let world = match self.world() {
            Ok(world) => world,
            Err(error) if error.is_board_unavailable() => {
                return Ok(SeedChooserReadiness::MissingBoard);
            }
            Err(error) => return Err(error),
        };
        let seed_choosing = match world.seed_choosing() {
            Ok(seed_choosing) => seed_choosing,
            Err(pvzp_rs::Error::NotFound { .. }) => return Ok(SeedChooserReadiness::MissingCutScene),
            Err(error) => return Err(error.into()),
        };
        let mouse_visible = match pvzp_rs::seed_chooser_mouse_visible() {
            Ok(mouse_visible) => mouse_visible,
            Err(pvzp_rs::Error::NotFound { .. }) => return Ok(SeedChooserReadiness::MissingSeedChooser),
            Err(error) => return Err(error.into()),
        };
        Ok(classify_seed_chooser_readiness(
            seed_choosing,
            world.paused() || pvzp_rs::seed_chooser_modal_present()?,
            mouse_visible,
            pvzp_rs::seed_chooser_parent_present()?,
            pvzp_rs::seed_chooser_widget_manager_present()?,
            pvzp_rs::seed_chooser_choose_state()?,
            pvzp_rs::seed_chooser_view_lawn_time()?,
            pvzp_rs::seed_chooser_seeds_in_flight()?,
        ))
    }

    pub(crate) fn ensure_seed_chooser_ready_for_auto_selection(&self) -> Result<(), PortableBackendError> {
        let readiness = self.seed_chooser_readiness()?;
        if readiness.allows_card_actions() {
            Ok(())
        } else {
            Err(PortableBackendError::OperationRejected(readiness.not_ready_message()))
        }
    }
}

impl Backend for PortableBackend {
    type Error = PortableBackendError;
}
