//! Current ice storage and coffee operations.

use crate::logic::IntoGrid;
use crate::logic::cards::CardPlantingBackend;
use crate::logic::ice_filler::IceFiller;
use crate::runtime::{RuntimeError, RuntimeResult};
use rsvz_backend_api::backend::SceneBackend;
use rsvz_current::CurrentBackend;
use rsvz_model::model::Grid;
use rsvz_schedule::tick::with_scheduler;
use rsvz_schedule::{TickControl, TickOptions, TickTaskState};

fn with_filler<T>(f: impl FnOnce(&mut IceFiller) -> T) -> T {
    with_ice_filler(|resource| f(&mut resource.core))
}

#[doc(hidden)]
pub fn register_tick_with_scheduler(scheduler: &mut rsvz_schedule::tick::TickScheduler)
where
    CurrentBackend: CardPlantingBackend + 'static,
{
    with_ice_filler(|resource| {
        if resource
            .task
            .is_some_and(|handle| scheduler.state(handle) != TickTaskState::Stopped)
        {
            return;
        }
        resource.task = Some(scheduler.spawn(
            TickOptions::playing_frame().idle_neutral().name("ice_filler"),
            |_meta| {
                tick()?;
                Ok(TickControl::Continue)
            },
        ));
    });
}

pub fn start<I, G>(grids: I) -> RuntimeResult<()>
where
    CurrentBackend: CardPlantingBackend + SceneBackend + 'static,
    I: IntoIterator<Item = G>,
    G: IntoGrid,
{
    with_filler(|filler| filler.start(grids)).map_err(runtime_error)?;
    with_scheduler(register_tick_with_scheduler);
    Ok(())
}

pub fn coffee() -> RuntimeResult<()>
where
    CurrentBackend: CardPlantingBackend,
{
    coffee_grid().map(|_grid| ())
}

pub fn tick() -> RuntimeResult<()>
where
    CurrentBackend: CardPlantingBackend,
{
    with_filler(|filler| filler.tick()).map_err(runtime_error)
}

pub fn reset() {
    with_ice_filler(IceFillerResource::reset_for_script);
}

#[doc(hidden)]
pub fn start_prepared(grids: Vec<Grid>) -> RuntimeResult<()>
where
    CurrentBackend: CardPlantingBackend + SceneBackend,
{
    with_filler(|filler| filler.start_prepared(grids)).map_err(runtime_error)
}

#[doc(hidden)]
pub fn coffee_grid() -> Result<Option<Grid>, RuntimeError>
where
    CurrentBackend: CardPlantingBackend,
{
    with_filler(|filler| filler.coffee()).map_err(runtime_error)
}

fn runtime_error(error: impl std::fmt::Display) -> RuntimeError {
    RuntimeError::new(error.to_string())
}

pub use crate::resources::IceFillerResource;
use std::cell::RefCell;

thread_local! {
    static ICE_FILLER: RefCell<IceFillerResource> = RefCell::new(IceFillerResource::default());
}

pub fn with_ice_filler<R>(f: impl FnOnce(&mut IceFillerResource) -> R) -> R {
    crate::resources::ensure_script_reset(&ICE_FILLER);
    ICE_FILLER.with_borrow_mut(f)
}
