//! Backend-free runtime state-hook registry.

mod current;
pub use current::{with_state_hooks, with_state_hooks_ref};

use std::num::NonZeroU64;
use std::panic::{self, AssertUnwindSafe};

type StateHookCallback<E> = Box<dyn FnMut() -> Result<(), E> + 'static>;

/// Runtime lifecycle events exposed to state hooks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StateEvent {
    AfterAttach,
    BeforeScript,
    AfterScript,
    EnterFight,
    ExitFight,
    BeforeTick,
    AfterTick,
    BeforeExit,
}

impl StateEvent {
    const COUNT: usize = 8;

    const fn index(self) -> usize {
        match self {
            Self::AfterAttach => 0,
            Self::BeforeScript => 1,
            Self::AfterScript => 2,
            Self::EnterFight => 3,
            Self::ExitFight => 4,
            Self::BeforeTick => 5,
            Self::AfterTick => 6,
            Self::BeforeExit => 7,
        }
    }
}

/// Weak, copyable token identifying one registered state hook.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StateHookHandle {
    slot: usize,
    generation: NonZeroU64,
}

/// Outcome of removing a state hook.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StateHookCommandOutcome {
    Applied,
    NotFound,
    StaleHandle,
    AlreadyRemoved,
}

/// Failure to begin a state-event dispatch.
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq, Hash)]
pub enum StateHookDispatchError {
    #[error("nested state-event dispatch is not allowed")]
    NestedDispatch,
}

/// Result of panic-catching hook dispatch.
#[derive(Debug, PartialEq, Eq)]
pub enum StateHookDispatchResult<E> {
    Continue,
    HookError(E),
    HookAborted(rsvz_backend_api::error::RuntimeError),
    HookPanic,
    NestedDispatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HookKey {
    order: i32,
    sequence: u64,
    slot: usize,
    generation: NonZeroU64,
}

impl HookKey {
    const fn sort_key(self) -> (i32, u64) {
        (self.order, self.sequence)
    }
}

struct HookEntry<E> {
    event: StateEvent,
    key: HookKey,
    callback: Option<StateHookCallback<E>>,
}

struct HookSlot<E> {
    generation: NonZeroU64,
    entry: Option<HookEntry<E>>,
}

/// Scalar cursor for a mutation-safe state-event dispatch.
#[doc(hidden)]
pub struct StateHookDispatch {
    event: StateEvent,
    sequence_cutoff: u64,
    last_key: Option<(i32, u64)>,
}

/// Hook callback temporarily taken out of registry storage.
#[doc(hidden)]
pub struct StateHookCallbackRun<E> {
    slot: usize,
    generation: NonZeroU64,
    callback: StateHookCallback<E>,
}

/// Result of running one hook outside registry storage.
#[doc(hidden)]
pub struct StateHookCallbackOutcome<E> {
    slot: usize,
    generation: NonZeroU64,
    callback: Option<StateHookCallback<E>>,
    result: StateHookDispatchResult<E>,
}

/// Ordered, mutation-safe state-hook registry.
pub struct StateHookRegistry<E> {
    slots: Vec<HookSlot<E>>,
    free_slots: Vec<usize>,
    event_order: [Vec<HookKey>; StateEvent::COUNT],
    next_generation: NonZeroU64,
    next_sequence: u64,
    dispatching: bool,
}

impl<E> StateHookRegistry<E> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free_slots: Vec::new(),
            event_order: std::array::from_fn(|_| Vec::new()),
            next_generation: NonZeroU64::MIN,
            next_sequence: 0,
            dispatching: false,
        }
    }

    /// Registers a hook with AvZ-compatible ascending integer order.
    pub fn register<F>(&mut self, event: StateEvent, order: i32, callback: F) -> StateHookHandle
    where
        F: FnMut() -> Result<(), E> + 'static,
    {
        let generation = self.allocate_generation();
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.wrapping_add(1);
        let slot = self.free_slots.pop().unwrap_or(self.slots.len());
        let key = HookKey {
            order,
            sequence,
            slot,
            generation,
        };
        let entry = HookEntry {
            event,
            key,
            callback: Some(Box::new(callback)),
        };
        if slot == self.slots.len() {
            self.slots.push(HookSlot {
                generation,
                entry: Some(entry),
            });
        } else {
            let target = &mut self.slots[slot];
            target.generation = generation;
            target.entry = Some(entry);
        }

        let order = &mut self.event_order[event.index()];
        let insert_at = order.partition_point(|existing| existing.sort_key() < key.sort_key());
        order.insert(insert_at, key);
        StateHookHandle { slot, generation }
    }

    /// Removes a hook. A callback currently running is not restored afterward.
    pub fn remove(&mut self, handle: StateHookHandle) -> StateHookCommandOutcome {
        let Some(slot) = self.slots.get_mut(handle.slot) else {
            return StateHookCommandOutcome::NotFound;
        };
        if slot.generation != handle.generation {
            return StateHookCommandOutcome::StaleHandle;
        }
        let Some(entry) = slot.entry.take() else {
            return StateHookCommandOutcome::AlreadyRemoved;
        };
        let order = &mut self.event_order[entry.event.index()];
        if let Ok(index) = order.binary_search_by_key(&entry.key.sort_key(), |key| key.sort_key()) {
            order.remove(index);
        }
        self.free_slots.push(handle.slot);
        StateHookCommandOutcome::Applied
    }

    /// Returns whether `handle` still names a registered hook.
    #[doc(hidden)]
    #[must_use]
    pub fn is_registered(&self, handle: StateHookHandle) -> bool {
        self.slots
            .get(handle.slot)
            .is_some_and(|slot| slot.generation == handle.generation && slot.entry.is_some())
    }

    /// Clears all hooks without resetting the monotonic handle generation.
    pub fn clear(&mut self) {
        for order in &mut self.event_order {
            order.clear();
        }
        self.free_slots.clear();
        for slot_index in 0..self.slots.len() {
            let generation = self.allocate_generation();
            let slot = &mut self.slots[slot_index];
            slot.generation = generation;
            slot.entry = None;
            self.free_slots.push(slot_index);
        }
    }

    /// Returns a registration-sequence checkpoint for transactional installers.
    #[must_use]
    pub const fn checkpoint(&self) -> u64 {
        self.next_sequence
    }

    /// Removes hooks registered at or after a checkpoint.
    pub fn rollback_to(&mut self, checkpoint: u64) {
        for slot_index in 0..self.slots.len() {
            let handle = {
                let slot = &self.slots[slot_index];
                slot.entry
                    .as_ref()
                    .filter(|entry| entry.key.sequence >= checkpoint)
                    .map(|_| StateHookHandle {
                        slot: slot_index,
                        generation: slot.generation,
                    })
            };
            if let Some(handle) = handle {
                let _outcome = self.remove(handle);
            }
        }
    }

    /// Starts a state-event dispatch using a scalar registration cutoff.
    #[doc(hidden)]
    pub fn begin_dispatch(&mut self, event: StateEvent) -> Result<StateHookDispatch, StateHookDispatchError> {
        if self.dispatching {
            return Err(StateHookDispatchError::NestedDispatch);
        }
        self.dispatching = true;
        Ok(StateHookDispatch {
            event,
            sequence_cutoff: self.next_sequence,
            last_key: None,
        })
    }

    /// Takes the next callback while honoring mutations made by earlier hooks.
    #[doc(hidden)]
    pub fn take_next(&mut self, dispatch: &mut StateHookDispatch) -> Option<StateHookCallbackRun<E>> {
        let order = &self.event_order[dispatch.event.index()];
        let mut index = dispatch
            .last_key
            .map_or(0, |last| order.partition_point(|key| key.sort_key() <= last));
        while let Some(key) = order.get(index).copied() {
            index += 1;
            dispatch.last_key = Some(key.sort_key());
            if key.sequence >= dispatch.sequence_cutoff {
                continue;
            }
            let Some(slot) = self.slots.get_mut(key.slot) else {
                continue;
            };
            if slot.generation != key.generation {
                continue;
            }
            let Some(entry) = slot.entry.as_mut() else {
                continue;
            };
            let Some(callback) = entry.callback.take() else {
                continue;
            };
            return Some(StateHookCallbackRun {
                slot: key.slot,
                generation: key.generation,
                callback,
            });
        }
        None
    }

    /// Runs one callback while containing panic.
    #[doc(hidden)]
    pub fn run_callback_catching(mut run: StateHookCallbackRun<E>) -> StateHookCallbackOutcome<E> {
        let result = panic::catch_unwind(AssertUnwindSafe(|| (run.callback)()));
        match result {
            Ok(Ok(())) => StateHookCallbackOutcome {
                slot: run.slot,
                generation: run.generation,
                callback: Some(run.callback),
                result: StateHookDispatchResult::Continue,
            },
            Ok(Err(error)) => StateHookCallbackOutcome {
                slot: run.slot,
                generation: run.generation,
                callback: None,
                result: StateHookDispatchResult::HookError(error),
            },
            Err(payload) => {
                let payload = match crate::callback::into_error(payload) {
                    Ok(error) => {
                        return StateHookCallbackOutcome {
                            slot: run.slot,
                            generation: run.generation,
                            callback: Some(run.callback),
                            result: StateHookDispatchResult::HookAborted(error),
                        };
                    }
                    Err(payload) => payload,
                };
                crate::timeline::resume_timeline_termination(payload);
                StateHookCallbackOutcome {
                    slot: run.slot,
                    generation: run.generation,
                    callback: None,
                    result: StateHookDispatchResult::HookPanic,
                }
            }
        }
    }

    /// Restores a successful callback unless it removed itself while running.
    #[doc(hidden)]
    pub fn finish_callback(&mut self, mut outcome: StateHookCallbackOutcome<E>) -> StateHookDispatchResult<E> {
        if let Some(callback) = outcome.callback.take()
            && let Some(slot) = self.slots.get_mut(outcome.slot)
            && slot.generation == outcome.generation
            && let Some(entry) = slot.entry.as_mut()
            && entry.callback.is_none()
        {
            entry.callback = Some(callback);
        }
        outcome.result
    }

    /// Ends a split dispatch.
    #[doc(hidden)]
    pub const fn end_dispatch(&mut self) {
        self.dispatching = false;
    }

    /// Dispatches an event directly, stopping at the first error or panic.
    pub fn dispatch_catching(&mut self, event: StateEvent) -> StateHookDispatchResult<E> {
        let mut dispatch = match self.begin_dispatch(event) {
            Ok(dispatch) => dispatch,
            Err(StateHookDispatchError::NestedDispatch) => return StateHookDispatchResult::NestedDispatch,
        };
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            loop {
                let Some(callback) = self.take_next(&mut dispatch) else {
                    break StateHookDispatchResult::Continue;
                };
                let outcome = Self::run_callback_catching(callback);
                match self.finish_callback(outcome) {
                    StateHookDispatchResult::Continue => {}
                    result => break result,
                }
            }
        }));
        self.end_dispatch();
        result.unwrap_or_else(|payload| {
            crate::timeline::resume_timeline_termination(payload);
            StateHookDispatchResult::HookPanic
        })
    }

    #[must_use]
    pub const fn is_dispatching(&self) -> bool {
        self.dispatching
    }

    fn allocate_generation(&mut self) -> NonZeroU64 {
        let generation = self.next_generation;
        self.next_generation = NonZeroU64::new(generation.get().wrapping_add(1)).unwrap_or(NonZeroU64::MIN);
        generation
    }
}

impl<E> Default for StateHookRegistry<E> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    struct TestError;

    type Log = Rc<RefCell<Vec<&'static str>>>;

    fn log_hook(log: &Log, label: &'static str) -> impl FnMut() -> Result<(), TestError> + 'static {
        let log = Rc::clone(log);
        move || {
            log.borrow_mut().push(label);
            Ok(())
        }
    }

    fn dispatch_split(
        registry: &Rc<RefCell<StateHookRegistry<TestError>>>, event: StateEvent,
    ) -> StateHookDispatchResult<TestError> {
        let mut dispatch = registry.borrow_mut().begin_dispatch(event).expect("dispatch");
        let result = loop {
            let callback = registry.borrow_mut().take_next(&mut dispatch);
            let Some(callback) = callback else {
                break StateHookDispatchResult::Continue;
            };
            let outcome = StateHookRegistry::run_callback_catching(callback);
            match registry.borrow_mut().finish_callback(outcome) {
                StateHookDispatchResult::Continue => {}
                result => break result,
            }
        };
        registry.borrow_mut().end_dispatch();
        result
    }

    #[test]
    fn timeline_termination_restores_direct_hook_dispatch() {
        let mut registry = StateHookRegistry::<()>::new();
        registry.register(StateEvent::BeforeTick, 0, || {
            crate::timeline::terminate_timeline_callback()
        });
        let failure = panic::catch_unwind(AssertUnwindSafe(|| registry.dispatch_catching(StateEvent::BeforeTick)));
        assert!(
            failure
                .err()
                .is_some_and(|payload| crate::timeline::is_timeline_termination(&*payload))
        );
        assert!(!registry.is_dispatching());
        assert_eq!(
            registry.dispatch_catching(StateEvent::BeforeTick),
            StateHookDispatchResult::Continue
        );
    }

    #[test]
    fn order_is_ascending_and_stable_for_equal_order() {
        let log = Log::default();
        let mut registry = StateHookRegistry::new();
        registry.register(StateEvent::BeforeTick, 0, log_hook(&log, "zero-a"));
        registry.register(StateEvent::BeforeTick, -10, log_hook(&log, "early"));
        registry.register(StateEvent::BeforeTick, 0, log_hook(&log, "zero-b"));
        registry.register(StateEvent::BeforeTick, 10, log_hook(&log, "late"));

        assert_eq!(
            registry.dispatch_catching(StateEvent::BeforeTick),
            StateHookDispatchResult::Continue
        );
        assert_eq!(log.borrow().as_slice(), ["early", "zero-a", "zero-b", "late"]);
    }

    #[test]
    fn additions_during_dispatch_start_with_the_next_event() {
        let log = Log::default();
        let registry = Rc::new(RefCell::new(StateHookRegistry::new()));
        let registry_for_hook = Rc::clone(&registry);
        let log_for_hook = Rc::clone(&log);
        registry.borrow_mut().register(StateEvent::BeforeTick, 0, move || {
            log_for_hook.borrow_mut().push("installer");
            for (order, label) in [(-10, "new-early"), (0, "new-same"), (10, "new-late")] {
                registry_for_hook
                    .borrow_mut()
                    .register(StateEvent::BeforeTick, order, log_hook(&log_for_hook, label));
            }
            Ok(())
        });
        registry
            .borrow_mut()
            .register(StateEvent::BeforeTick, 5, log_hook(&log, "existing"));

        assert_eq!(
            dispatch_split(&registry, StateEvent::BeforeTick),
            StateHookDispatchResult::Continue
        );
        assert_eq!(log.borrow().as_slice(), ["installer", "existing"]);

        log.borrow_mut().clear();
        assert_eq!(
            dispatch_split(&registry, StateEvent::BeforeTick),
            StateHookDispatchResult::Continue
        );
        assert_eq!(
            log.borrow().as_slice(),
            ["new-early", "installer", "new-same", "existing", "new-late"]
        );
    }

    #[test]
    fn remove_future_self_and_stale_reused_handle_are_safe() {
        let log = Log::default();
        let registry = Rc::new(RefCell::new(StateHookRegistry::new()));
        let future = registry
            .borrow_mut()
            .register(StateEvent::BeforeTick, 10, log_hook(&log, "future"));
        let self_handle = Rc::new(RefCell::new(None));
        let registry_for_hook = Rc::clone(&registry);
        let self_handle_for_hook = Rc::clone(&self_handle);
        let log_for_hook = Rc::clone(&log);
        let handle = registry.borrow_mut().register(StateEvent::BeforeTick, 0, move || {
            log_for_hook.borrow_mut().push("self");
            assert_eq!(
                registry_for_hook.borrow_mut().remove(future),
                StateHookCommandOutcome::Applied
            );
            assert_eq!(
                registry_for_hook
                    .borrow_mut()
                    .remove(self_handle_for_hook.borrow().expect("self handle")),
                StateHookCommandOutcome::Applied
            );
            Ok(())
        });
        *self_handle.borrow_mut() = Some(handle);

        assert_eq!(
            dispatch_split(&registry, StateEvent::BeforeTick),
            StateHookDispatchResult::Continue
        );
        assert_eq!(log.borrow().as_slice(), ["self"]);

        let replacement = registry
            .borrow_mut()
            .register(StateEvent::BeforeTick, 0, log_hook(&log, "replacement"));
        assert_ne!(handle, replacement);
        assert_eq!(
            registry.borrow_mut().remove(handle),
            StateHookCommandOutcome::StaleHandle
        );
    }

    #[test]
    fn registered_state_rejects_removed_and_reused_handles() {
        let mut registry = StateHookRegistry::<TestError>::new();
        let removed = registry.register(StateEvent::BeforeScript, 0, || Ok(()));
        assert!(registry.is_registered(removed));
        assert_eq!(registry.remove(removed), StateHookCommandOutcome::Applied);
        assert!(!registry.is_registered(removed));

        let replacement = registry.register(StateEvent::BeforeScript, 0, || Ok(()));
        assert!(registry.is_registered(replacement));
        assert!(!registry.is_registered(removed));
    }

    #[test]
    fn error_panic_nested_dispatch_and_clear_have_explicit_results() {
        let mut errors = StateHookRegistry::new();
        errors.register(StateEvent::AfterTick, 0, || Err(TestError));
        errors.register(StateEvent::AfterTick, 1, || Ok(()));
        assert_eq!(
            errors.dispatch_catching(StateEvent::AfterTick),
            StateHookDispatchResult::HookError(TestError)
        );
        assert!(!errors.is_dispatching());

        let mut panics = StateHookRegistry::<TestError>::new();
        panics.register(StateEvent::AfterTick, 0, || panic!("probe"));
        assert_eq!(
            panics.dispatch_catching(StateEvent::AfterTick),
            StateHookDispatchResult::HookPanic
        );
        assert!(!panics.is_dispatching());

        let mut nested = StateHookRegistry::<TestError>::new();
        let _dispatch = nested.begin_dispatch(StateEvent::BeforeTick).expect("outer dispatch");
        assert!(matches!(
            nested.begin_dispatch(StateEvent::AfterTick),
            Err(StateHookDispatchError::NestedDispatch)
        ));
        nested.end_dispatch();

        let handle = nested.register(StateEvent::BeforeTick, 0, || Ok(()));
        nested.clear();
        let replacement = nested.register(StateEvent::BeforeTick, 0, || Ok(()));
        assert_ne!(handle, replacement);
        assert_eq!(nested.remove(handle), StateHookCommandOutcome::StaleHandle);
    }
}
