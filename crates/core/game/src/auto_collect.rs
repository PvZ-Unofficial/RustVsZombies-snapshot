//! Automatic item collection for the selected backend.

pub use crate::resources::ItemCollectorResource;
use std::cell::RefCell;

pub use crate::logic::item_collector::{ItemCollector, ItemCollectorConfigError, ItemCollectorError};
pub use rsvz_model::AutoCollectMode;

use crate::runtime::{RuntimeError, RuntimeResult};
use rsvz_backend_api::backend::{AutoCollectBackend, ClockBackend, ItemClickCollectBackend, ItemReadBackend};
use rsvz_current::CurrentBackend;
use rsvz_model::model::ItemKind;
use rsvz_schedule::tick::with_scheduler;
use rsvz_schedule::{TickControl, TickLane, TickOptions, TickPriority, TickTaskState};

fn with_collector<R>(f: impl FnOnce(&mut ItemCollector) -> R) -> R {
    with_item_collector(|resource| f(&mut resource.core))
}

pub fn set_mode(mode: AutoCollectMode) -> RuntimeResult<()> {
    with_collector(|collector| collector.set_mode(mode));
    Ok(())
}

#[must_use]
pub fn mode() -> AutoCollectMode {
    with_collector(|collector| collector.mode())
}

pub fn normal() -> RuntimeResult<()> {
    set_mode(AutoCollectMode::Normal)
}

pub fn click() -> RuntimeResult<()> {
    set_mode(AutoCollectMode::Click)
}

pub fn off() -> RuntimeResult<()> {
    set_mode(AutoCollectMode::Off)
}

pub fn set_interval(interval: u32) -> RuntimeResult<()> {
    with_collector(|collector| collector.set_interval(interval)).map_err(runtime_error)
}

pub fn set_type_list<I>(types: I) -> RuntimeResult<()>
where
    I: IntoIterator<Item = i32>,
{
    with_collector(|collector| collector.set_type_list(types)).map_err(runtime_error)
}

pub fn set_types<I>(types: I)
where
    I: IntoIterator<Item = ItemKind>,
{
    with_collector(|collector| collector.set_types(types));
}

pub fn set_play_sound(play_sound: bool) {
    with_collector(|collector| collector.set_play_sound(play_sound));
}

pub fn set_allow_cursor_side_effects(allow: bool) {
    with_collector(|collector| collector.set_allow_cursor_side_effects(allow));
}

pub fn tick() -> RuntimeResult<()>
where
    CurrentBackend: AutoCollectBackend,
{
    let click_enabled = with_collector(|collector| collector.tick_native()).map_err(runtime_error)?;
    if click_enabled {
        return Err(RuntimeError::new(
            "click auto collect requires ItemReadBackend + ItemClickCollectBackend; call tick_click instead",
        ));
    }
    Ok(())
}

pub fn tick_click() -> RuntimeResult<()>
where
    CurrentBackend: AutoCollectBackend + ClockBackend + ItemReadBackend + ItemClickCollectBackend,
{
    with_collector(|collector| collector.tick()).map_err(runtime_error)
}

pub fn register_tick() -> RuntimeResult<()>
where
    CurrentBackend: AutoCollectBackend + 'static,
{
    if CurrentBackend::AUTO_COLLECT_IS_NOOP {
        return Ok(());
    }
    with_item_collector(|resource| {
        with_scheduler(|scheduler| {
            if resource
                .native_task
                .is_some_and(|handle| scheduler.state(handle) != TickTaskState::Stopped)
            {
                return;
            }
            resource.native_task = Some(
                scheduler.spawn(
                    TickOptions::playing_frame()
                        .lane(TickLane::After)
                        .priority(TickPriority::NORMAL)
                        .idle_neutral()
                        .name("auto_collect"),
                    |_meta| {
                        tick()?;
                        Ok(TickControl::Continue)
                    },
                ),
            );
        });
    });
    Ok(())
}

pub fn register_click_tick() -> RuntimeResult<()>
where
    CurrentBackend: ClockBackend + ItemReadBackend + ItemClickCollectBackend + 'static,
{
    if !CurrentBackend::ITEMS_CAN_EXIST {
        return Ok(());
    }
    with_item_collector(|resource| {
        with_scheduler(|scheduler| {
            if resource
                .click_task
                .is_some_and(|handle| scheduler.state(handle) != TickTaskState::Stopped)
            {
                return;
            }
            resource.click_task = Some(
                scheduler.spawn(
                    TickOptions::playing_frame()
                        .lane(TickLane::After)
                        .priority(TickPriority::NORMAL)
                        .idle_neutral()
                        .name("auto_collect_click"),
                    |_meta| {
                        with_collector(|collector| collector.tick_click()).map_err(runtime_error)?;
                        Ok(TickControl::Continue)
                    },
                ),
            );
        });
    });
    Ok(())
}

pub fn reset() {
    with_item_collector(ItemCollectorResource::reset_for_script);
}

fn runtime_error(error: impl std::fmt::Display) -> RuntimeError {
    RuntimeError::new(error.to_string())
}

thread_local! {
    static ITEM_COLLECTOR: RefCell<ItemCollectorResource> = RefCell::new(ItemCollectorResource::default());
}

pub fn with_item_collector<R>(f: impl FnOnce(&mut ItemCollectorResource) -> R) -> R {
    crate::resources::ensure_script_reset(&ITEM_COLLECTOR);
    ITEM_COLLECTOR.with_borrow_mut(f)
}
