//! Current plant repair and configuration.

pub use crate::logic::plant_fixer::{PlantFixer, PlantFixerBackend, PlantFixerError, PlantFixerGridError};

use crate::logic::IntoGrid;
use crate::runtime::{RuntimeError, RuntimeResult};
use rsvz_backend_api::backend::PlantReadBackend;
use rsvz_current::CurrentBackend;
use rsvz_model::model::PlantKind;
use rsvz_schedule::tick::with_scheduler;
use rsvz_schedule::{TickControl, TickOptions, TickTaskState};

fn with_fixer<T>(f: impl FnOnce(&mut PlantFixer) -> T) -> T {
    with_plant_fixer(|resource| f(&mut resource.core))
}

fn register_tick()
where
    CurrentBackend: PlantFixerBackend + 'static,
{
    with_plant_fixer(|resource| {
        with_scheduler(|scheduler| {
            if resource
                .task
                .is_some_and(|handle| scheduler.state(handle) != TickTaskState::Stopped)
            {
                return;
            }
            resource.task = Some(scheduler.spawn(
                TickOptions::playing_frame().idle_neutral().name("plant_fixer"),
                |_meta| {
                    tick()?;
                    Ok(TickControl::Continue)
                },
            ));
        });
    });
}

pub fn start_plant_fixer<I, G>(kind: PlantKind, grids: I) -> RuntimeResult<()>
where
    CurrentBackend: PlantFixerBackend + 'static,
    I: IntoIterator<Item = G>,
    G: IntoGrid,
{
    with_fixer(|fixer| fixer.start(kind, grids)).map_err(runtime_error)?;
    register_tick();
    Ok(())
}

pub fn start_with<I, G>(kind: PlantKind, grids: I, fix_threshold: f32, use_imitator: bool) -> RuntimeResult<()>
where
    CurrentBackend: PlantFixerBackend + 'static,
    I: IntoIterator<Item = G>,
    G: IntoGrid,
{
    with_fixer(|fixer| fixer.start_with(kind, grids, fix_threshold, use_imitator)).map_err(runtime_error)?;
    register_tick();
    Ok(())
}

pub fn set_plant_fixer_hp(hp: i32) -> RuntimeResult<()> {
    with_fixer(|fixer| fixer.set_hp(hp));
    Ok(())
}

pub fn set_check_cards(enabled: bool) -> RuntimeResult<()> {
    with_fixer(|fixer| fixer.set_check_cards(enabled));
    Ok(())
}

pub fn set_not_interrupt(enabled: bool) -> RuntimeResult<()> {
    with_fixer(|fixer| fixer.set_not_interrupt(enabled));
    Ok(())
}

pub fn set_skip_covered_bottom(enabled: bool) -> RuntimeResult<()> {
    with_fixer(|fixer| fixer.set_skip_covered_bottom(enabled));
    Ok(())
}

pub fn set_sun_threshold(sun: u32) -> RuntimeResult<()> {
    with_fixer(|fixer| fixer.set_sun_threshold(sun));
    Ok(())
}

pub fn set_run_interval(interval: u32) -> RuntimeResult<()> {
    with_fixer(|fixer| fixer.set_run_interval(interval)).map_err(runtime_error)
}

pub fn set_list<I, G>(grids: I) -> RuntimeResult<()>
where
    I: IntoIterator<Item = G>,
    G: IntoGrid,
{
    with_fixer(|fixer| fixer.set_list(grids));
    Ok(())
}

pub fn set_hp_threshold(hp: u32) -> RuntimeResult<()> {
    with_fixer(|fixer| fixer.set_hp_threshold(hp));
    Ok(())
}

pub fn auto_set_list() -> RuntimeResult<()>
where
    CurrentBackend: PlantReadBackend,
{
    with_fixer(|fixer| fixer.auto_set_list()).map_err(runtime_error)
}

pub fn erase_from_list<I, G>(grids: I) -> RuntimeResult<()>
where
    I: IntoIterator<Item = G>,
    G: IntoGrid,
{
    with_fixer(|fixer| fixer.erase_from_list(grids));
    Ok(())
}

pub fn move_to_list_top<I, G>(grids: I) -> RuntimeResult<()>
where
    I: IntoIterator<Item = G>,
    G: IntoGrid,
{
    with_fixer(|fixer| fixer.move_to_list_top(grids));
    Ok(())
}

pub fn move_to_list_bottom<I, G>(grids: I) -> RuntimeResult<()>
where
    I: IntoIterator<Item = G>,
    G: IntoGrid,
{
    with_fixer(|fixer| fixer.move_to_list_bottom(grids));
    Ok(())
}

pub fn set_use_coffee(enabled: bool) -> RuntimeResult<()> {
    with_fixer(|fixer| fixer.set_use_coffee(enabled));
    Ok(())
}

pub fn tick() -> RuntimeResult<()>
where
    CurrentBackend: PlantFixerBackend,
{
    with_fixer(|fixer| fixer.tick()).map_err(runtime_error)
}

pub fn reset() {
    with_plant_fixer(PlantFixerResource::reset_for_script);
}

fn runtime_error(error: impl std::fmt::Display) -> RuntimeError {
    RuntimeError::new(error.to_string())
}

pub use crate::resources::PlantFixerResource;
use std::cell::RefCell;

thread_local! {
    static PLANT_FIXER: RefCell<PlantFixerResource> = RefCell::new(PlantFixerResource::default());
}

pub fn with_plant_fixer<R>(f: impl FnOnce(&mut PlantFixerResource) -> R) -> R {
    crate::resources::ensure_script_reset(&PLANT_FIXER);
    PLANT_FIXER.with_borrow_mut(f)
}
