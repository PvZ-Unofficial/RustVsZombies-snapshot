//! Smart removal with optional cosmetic highlighting.

use crate::logic::smart_remove::tick_smart_remove_with_visuals as tick_impl;
use crate::logic::{ContactGeometryBackend, smart_remove::SmartRemoveState};
use crate::runtime::{RuntimeError, RuntimeResult};
use rsvz_backend_api::{PlantReadBackend, PlantRemoveBackend, PlantVisualStateBackend, ZombieReadBackend};
use rsvz_current::CurrentBackend;
use rsvz_model::SmartRemoveOptions;
use rsvz_schedule::tick::{TickControl, TickLane, TickOptions, TickPriority, TickTaskState, with_scheduler};

pub use crate::resources::SmartRemoveResource;
use std::cell::RefCell;

thread_local! {
    static SMART_REMOVE: RefCell<SmartRemoveResource> = RefCell::new(SmartRemoveResource::default());
}

pub fn with_smart_remove<R>(f: impl FnOnce(&mut SmartRemoveResource) -> R) -> R {
    crate::resources::ensure_script_reset(&SMART_REMOVE);
    SMART_REMOVE.with_borrow_mut(f)
}

fn start_state(options: SmartRemoveOptions) {
    with_smart_remove(|resource| {
        let mut core = SmartRemoveState::default();
        core.set_highlight(options.highlight);
        resource.enabled = true;
        resource.core = core;
    });
}

fn register_tick()
where
    CurrentBackend: PlantReadBackend
        + ZombieReadBackend
        + ContactGeometryBackend
        + PlantRemoveBackend
        + PlantVisualStateBackend
        + 'static,
{
    with_smart_remove(|resource| {
        with_scheduler(|scheduler| {
            if resource
                .task
                .is_some_and(|handle| scheduler.state(handle) != TickTaskState::Stopped)
            {
                return;
            }
            resource.task = Some(
                scheduler.spawn(
                    TickOptions::playing_frame()
                        .lane(TickLane::After)
                        .priority(TickPriority::HIGH)
                        .idle_neutral()
                        .name("smart_remove"),
                    |_meta| {
                        tick()?;
                        Ok(TickControl::Continue)
                    },
                ),
            );
        });
    });
}

pub fn start() -> RuntimeResult<()>
where
    CurrentBackend: PlantReadBackend
        + ZombieReadBackend
        + ContactGeometryBackend
        + PlantRemoveBackend
        + PlantVisualStateBackend
        + 'static,
{
    start_with_options(SmartRemoveOptions::default())
}

pub fn start_with_options(options: SmartRemoveOptions) -> RuntimeResult<()>
where
    CurrentBackend: PlantReadBackend
        + ZombieReadBackend
        + ContactGeometryBackend
        + PlantRemoveBackend
        + PlantVisualStateBackend
        + 'static,
{
    start_state(options);
    register_tick();
    Ok(())
}

pub fn stop() {
    with_smart_remove(|resource| resource.enabled = false);
}

pub fn set_highlight(highlight: bool) {
    with_smart_remove(|resource| resource.core.set_highlight(highlight));
}

#[must_use]
pub fn options() -> SmartRemoveOptions {
    with_smart_remove(|resource| SmartRemoveOptions {
        highlight: resource.core.highlight(),
    })
}

#[must_use]
pub fn is_running() -> bool {
    with_smart_remove(|resource| resource.enabled)
}

pub fn tick() -> RuntimeResult<()>
where
    CurrentBackend:
        PlantReadBackend + ZombieReadBackend + ContactGeometryBackend + PlantRemoveBackend + PlantVisualStateBackend,
{
    with_smart_remove(|resource| {
        if !resource.enabled {
            return Ok(());
        }
        tick_impl(&mut resource.core).map_err(runtime_error)
    })
}

pub fn reset() {
    with_smart_remove(SmartRemoveResource::reset_for_script);
}

fn runtime_error(error: impl std::fmt::Display) -> RuntimeError {
    RuntimeError::new(error.to_string())
}
