//! Per-manager runtime resources, kept independent for lazy installation.

use std::collections::HashMap;

use crate::logic::{
    cob::CobManager, ice_filler::IceFiller, item_collector::ItemCollector, plant_fixer::PlantFixer,
    smart_remove::SmartRemoveState,
};
use rsvz_model::KeyCode;
use rsvz_schedule::{state_hook::StateHookHandle, tick::TickHandle};

#[doc(hidden)]
pub trait ScriptResource: Default {
    fn reset_hook(&mut self) -> &mut Option<StateHookHandle>;

    fn reset_for_script(&mut self) {
        let reset_hook = *self.reset_hook();
        *self = Self::default();
        *self.reset_hook() = reset_hook;
    }
}

#[derive(Default)]
pub struct IceFillerResource {
    pub core: IceFiller,
    pub task: Option<TickHandle>,
    reset_hook: Option<StateHookHandle>,
}

impl IceFillerResource {
    pub fn reset_for_script(&mut self) {
        ScriptResource::reset_for_script(self);
    }
}

impl ScriptResource for IceFillerResource {
    fn reset_hook(&mut self) -> &mut Option<StateHookHandle> {
        &mut self.reset_hook
    }
}

#[derive(Default)]
pub struct PlantFixerResource {
    pub core: PlantFixer,
    pub task: Option<TickHandle>,
    reset_hook: Option<StateHookHandle>,
}

impl PlantFixerResource {
    pub fn reset_for_script(&mut self) {
        ScriptResource::reset_for_script(self);
    }
}

impl ScriptResource for PlantFixerResource {
    fn reset_hook(&mut self) -> &mut Option<StateHookHandle> {
        &mut self.reset_hook
    }
}

#[derive(Default)]
pub struct ItemCollectorResource {
    pub core: ItemCollector,
    pub native_task: Option<TickHandle>,
    pub click_task: Option<TickHandle>,
    reset_hook: Option<StateHookHandle>,
}

impl ItemCollectorResource {
    pub fn reset_for_script(&mut self) {
        ScriptResource::reset_for_script(self);
    }
}

impl ScriptResource for ItemCollectorResource {
    fn reset_hook(&mut self) -> &mut Option<StateHookHandle> {
        &mut self.reset_hook
    }
}

#[derive(Debug, Default)]
pub struct SmartRemoveResource {
    pub enabled: bool,
    pub core: SmartRemoveState,
    pub task: Option<TickHandle>,
    reset_hook: Option<StateHookHandle>,
}

impl SmartRemoveResource {
    pub fn reset_for_script(&mut self) {
        ScriptResource::reset_for_script(self);
    }
}

impl ScriptResource for SmartRemoveResource {
    fn reset_hook(&mut self) -> &mut Option<StateHookHandle> {
        &mut self.reset_hook
    }
}

pub struct CobResource {
    pub core: CobManager,
    reset_hook: Option<StateHookHandle>,
}

impl Default for CobResource {
    fn default() -> Self {
        Self {
            core: CobManager::new_root(),
            reset_hook: None,
        }
    }
}

impl ScriptResource for CobResource {
    fn reset_hook(&mut self) -> &mut Option<StateHookHandle> {
        &mut self.reset_hook
    }

    fn reset_for_script(&mut self) {
        self.core.reset_before_script();
    }
}

#[derive(Default)]
pub struct UniqueKeyBindingsResource {
    pub bindings: HashMap<KeyCode, TickHandle>,
    reset_hook: Option<StateHookHandle>,
}

impl ScriptResource for UniqueKeyBindingsResource {
    fn reset_hook(&mut self) -> &mut Option<StateHookHandle> {
        &mut self.reset_hook
    }
}

#[doc(hidden)]
pub fn ensure_script_reset<R>(slot: &'static std::thread::LocalKey<std::cell::RefCell<R>>)
where
    R: ScriptResource + 'static,
{
    slot.with_borrow_mut(|resource| {
        let registered = (*resource.reset_hook())
            .is_some_and(|handle| rsvz_schedule::state_hook::with_state_hooks(|hooks| hooks.is_registered(handle)));
        if registered {
            return;
        }

        resource.reset_for_script();
        let handle = rsvz_schedule::state_hook::with_state_hooks(|hooks| {
            hooks.register(
                rsvz_schedule::state_hook::StateEvent::BeforeScript,
                i32::MIN,
                move || {
                    slot.with_borrow_mut(ScriptResource::reset_for_script);
                    Ok(())
                },
            )
        });
        *resource.reset_hook() = Some(handle);
    });
}

#[cfg(test)]
mod tests {

    use crate::logic::cob::CobSequentialMode;
    use rsvz_model::{Grid, KeyCode};
    use rsvz_schedule::state_hook::{StateEvent, StateHookDispatchResult};
    use rsvz_schedule::tick::{TickControl, TickHandle, TickOptions};

    use crate::auto_collect::with_item_collector;
    use crate::ice_filler::with_ice_filler;
    use crate::key::with_unique_key_bindings;
    use crate::logic::cob::current_cob_manager;
    use crate::plant_fixer::with_plant_fixer;
    use crate::smart_remove::with_smart_remove;

    fn spawn_task() -> TickHandle {
        rsvz_schedule::tick::with_scheduler(|scheduler| {
            scheduler.spawn(TickOptions::any_dispatch(), |_| Ok(TickControl::Continue))
        })
    }

    #[test]
    fn manager_resources_reset_through_independent_before_script_hooks() {
        crate::frame::reset_runtime_state_preserving_backend();
        crate::state_hook::clear_state_hooks();

        let cob = current_cob_manager();
        cob.set_sequential_mode(CobSequentialMode::Time);
        cob.set_list_unchecked([
            Grid::new(0, 0).expect("valid grid"),
            Grid::new(1, 0).expect("valid grid"),
        ]);
        cob.set_next_slot(2).expect("valid cob slot");

        with_ice_filler(|resource| resource.task = Some(spawn_task()));
        with_plant_fixer(|resource| resource.task = Some(spawn_task()));
        with_item_collector(|resource| {
            resource.native_task = Some(spawn_task());
            resource.click_task = Some(spawn_task());
        });
        with_smart_remove(|resource| {
            resource.enabled = true;
            resource.task = Some(spawn_task());
        });
        with_unique_key_bindings(|bindings| {
            bindings.insert(KeyCode::A, spawn_task());
        });

        assert_eq!(
            crate::state_hook::dispatch_state_event(StateEvent::BeforeScript),
            StateHookDispatchResult::Continue
        );
        assert_eq!(cob.next_index_raw(), 0);
        with_ice_filler(|resource| assert!(resource.task.is_none()));
        with_plant_fixer(|resource| assert!(resource.task.is_none()));
        with_item_collector(|resource| {
            assert!(resource.native_task.is_none());
            assert!(resource.click_task.is_none());
        });
        with_smart_remove(|resource| {
            assert!(!resource.enabled);
            assert!(resource.task.is_none());
        });
        with_unique_key_bindings(|bindings| assert!(bindings.is_empty()));
    }

    #[test]
    fn rolled_back_lazy_hook_is_reinstalled_before_resource_reuse() {
        crate::frame::reset_runtime_state_preserving_backend();
        crate::state_hook::clear_state_hooks();
        let checkpoint = crate::state_hook::state_hook_checkpoint();
        with_smart_remove(|resource| resource.enabled = true);
        crate::state_hook::rollback_state_hooks_to(checkpoint);

        with_smart_remove(|resource| assert!(!resource.enabled));
        with_smart_remove(|resource| resource.enabled = true);
        assert_eq!(
            crate::state_hook::dispatch_state_event(StateEvent::BeforeScript),
            StateHookDispatchResult::Continue
        );
        with_smart_remove(|resource| assert!(!resource.enabled));
    }
}
