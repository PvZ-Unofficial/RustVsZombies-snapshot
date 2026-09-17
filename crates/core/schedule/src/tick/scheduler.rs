use std::any::Any;
use std::num::NonZeroU64;
use std::panic::{self, AssertUnwindSafe};

use rsvz_backend_api::error::RuntimeError;

use super::{
    TickAvailability, TickCommandOutcome, TickControl, TickFrameGate, TickHandle, TickLane, TickLifetime, TickMeta,
    TickOptions, TickPhase, TickPriority, TickTaskKey, TickTaskState, TickTrigger,
};

type TickCallback = Box<dyn FnMut(TickMeta) -> Result<TickControl, RuntimeError> + 'static>;
type PanicPayload = Box<dyn Any + Send + 'static>;

enum CallbackOutcome {
    Continue(TickCallback),
    Pause(TickCallback),
    Stop,
    TaskError(RuntimeError, Option<TickCallback>),
    TaskPanic(PanicPayload),
}

fn run_runtime_callback(mut callback: TickCallback, ready_once: bool, meta: TickMeta) -> CallbackOutcome {
    match panic::catch_unwind(AssertUnwindSafe(|| callback(meta))) {
        Ok(Ok(TickControl::Continue)) if !ready_once => CallbackOutcome::Continue(callback),
        Ok(Ok(TickControl::Pause)) if !ready_once => CallbackOutcome::Pause(callback),
        Ok(Ok(TickControl::Continue | TickControl::Pause | TickControl::Stop)) => CallbackOutcome::Stop,
        Ok(Err(error)) => CallbackOutcome::TaskError(error, (!ready_once).then_some(callback)),
        Err(payload) => match crate::callback::into_error(payload) {
            Ok(error) => CallbackOutcome::TaskError(error, (!ready_once).then_some(callback)),
            Err(payload) => CallbackOutcome::TaskPanic(payload),
        },
    }
}

enum FinishedCallback {
    Continue,
    StoppedSelf,
    TaskError(RuntimeError),
    TaskPanic(PanicPayload),
}

/// Result of an uncaught scheduler dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TickDispatch {
    /// No task requested self-stop during this dispatch.
    Continue,
    /// At least one task stopped itself during this dispatch.
    StoppedSelf,
}

/// Result of a panic-catching scheduler dispatch.
#[derive(Debug, PartialEq, Eq)]
pub enum TickDispatchResult {
    /// Dispatch completed without callback errors or panics.
    Continue,
    /// A task returned an error; repeating tasks retain their registration.
    TaskError(RuntimeError),
    /// A task panicked; the task was stopped.
    TaskPanic,
}

/// Pure reusable tick scheduler.
pub struct TickScheduler {
    availability: [[LaneTasks; TickLane::COUNT]; TickAvailability::COUNT],
    next_generation: NonZeroU64,
    dispatching: bool,
    dispatch_order: Vec<TickTaskKey>,
}

/// Opaque registration checkpoint used to roll back one failed installer.
#[derive(Clone, Copy, Debug)]
pub struct TickRegistrationCheckpoint(NonZeroU64);

/// Opaque dispatch order used by runtime dispatch while callbacks run outside TLS borrows.
#[doc(hidden)]
pub struct RuntimeTickDispatchOrder {
    order: Vec<TickTaskKey>,
    index: usize,
    buckets_scanned: u64,
}

impl RuntimeTickDispatchOrder {
    #[doc(hidden)]
    #[must_use]
    pub fn queued_len(&self) -> usize {
        self.order.len()
    }

    #[doc(hidden)]
    #[must_use]
    pub const fn buckets_scanned(&self) -> u64 {
        self.buckets_scanned
    }
}

/// Opaque callback taken out of scheduler storage for split runtime dispatch.
#[doc(hidden)]
pub struct RuntimeTickCallbackRun {
    key: TickTaskKey,
    ready_once: bool,
    callback: TickCallback,
}

/// Result of a split-dispatch callback run outside scheduler storage.
#[doc(hidden)]
pub struct RuntimeTickCallbackOutcome {
    key: TickTaskKey,
    outcome: CallbackOutcome,
}

#[expect(
    clippy::indexing_slicing,
    reason = "scheduler indexes fixed enum-backed arrays and intrusive pool links guarded by generation checks"
)]
impl TickScheduler {
    /// Creates an empty scheduler.
    #[must_use]
    pub fn new() -> Self {
        Self {
            availability: std::array::from_fn(|_| std::array::from_fn(|_| LaneTasks::new())),
            next_generation: NonZeroU64::MIN,
            dispatching: false,
            dispatch_order: Vec::new(),
        }
    }

    /// Registers a backend-free task and returns its generation-protected handle.
    pub fn spawn<F>(&mut self, options: TickOptions, f: F) -> TickHandle
    where
        F: FnMut(TickMeta) -> Result<TickControl, RuntimeError> + 'static,
    {
        let generation = self.allocate_generation();
        let entry = TickEntry {
            lifetime: options.lifetime,
            frame_gate: options.frame_gate,
            trigger: options.trigger,
            idle_neutral: options.idle_neutral,
            callback: Some(Box::new(f)),
        };
        let slot = self
            .lane_tasks_mut(options.availability, options.lane)
            .spawn(options.priority, generation, entry);

        TickHandle::new(options.availability, options.lane, options.priority, slot, generation)
    }

    /// Registers an internal runtime finalizer after all runnable user tasks.
    #[doc(hidden)]
    pub fn spawn_runtime_finalizer<F>(&mut self, mut options: TickOptions, f: F) -> TickHandle
    where
        F: FnMut(TickMeta) -> Result<TickControl, RuntimeError> + 'static,
    {
        options.priority = TickPriority::FINALIZER;
        self.spawn(options, f)
    }

    /// Captures the next task generation without allocating.
    #[must_use]
    pub const fn checkpoint(&self) -> TickRegistrationCheckpoint {
        TickRegistrationCheckpoint(self.next_generation)
    }

    /// Removes every task registered at or after `checkpoint`, regardless of lifetime.
    pub fn rollback_to(&mut self, checkpoint: TickRegistrationCheckpoint) {
        for lanes in &mut self.availability {
            for lane in lanes {
                lane.rollback_to(checkpoint.0);
            }
        }
    }

    /// Returns the current state, or `Stopped` if the handle no longer resolves.
    #[must_use]
    pub fn state(&self, handle: TickHandle) -> TickTaskState {
        let key = handle.key();
        self.pool(key).state(key.slot, key.generation)
    }

    /// Pauses a running task.
    pub fn pause(&mut self, handle: TickHandle) -> TickCommandOutcome {
        let key = handle.key();
        self.change_state_key(key, TickTaskState::Paused)
    }

    /// Resumes a paused task.
    pub fn resume(&mut self, handle: TickHandle) -> TickCommandOutcome {
        let key = handle.key();
        self.change_state_key(key, TickTaskState::Running)
    }

    /// Stops a task and frees its slot for a later generation.
    pub fn stop(&mut self, handle: TickHandle) -> TickCommandOutcome {
        let key = handle.key();
        self.lane_tasks_mut(key.availability, key.lane)
            .stop(key.priority, key.slot, key.generation)
    }

    /// Clears every task with the selected lifecycle.
    pub fn clear_lifetime(&mut self, lifetime: TickLifetime) {
        for lanes in &mut self.availability {
            for lane in lanes {
                lane.clear_lifetime(lifetime);
            }
        }
    }

    /// Clears all tasks.
    pub fn clear_all(&mut self) {
        for availability in TickAvailability::ALL {
            for lane in &mut self.availability[availability.index()] {
                lane.clear();
            }
        }
    }

    /// Dispatches one tick without swallowing callback panics.
    ///
    /// If a callback panics, the task is stopped before the panic is resumed.
    pub fn dispatch_tick(&mut self, meta: TickMeta) -> Result<TickDispatch, RuntimeError> {
        self.dispatching = true;
        let result = panic::catch_unwind(AssertUnwindSafe(|| self.dispatch_tick_uncaught(meta)));
        self.dispatching = false;
        match result {
            Ok(result) => result,
            Err(payload) => panic::resume_unwind(payload),
        }
    }

    /// Dispatches one tick while containing callback or scheduler panics.
    pub fn dispatch_tick_catching(&mut self, meta: TickMeta) -> TickDispatchResult {
        self.dispatching = true;
        let result = panic::catch_unwind(AssertUnwindSafe(|| self.dispatch_tick_uncaught(meta)));
        self.dispatching = false;
        match result {
            Ok(Ok(_)) => TickDispatchResult::Continue,
            Ok(Err(error)) => TickDispatchResult::TaskError(error),
            Err(payload) => {
                crate::timeline::resume_timeline_termination(payload);
                TickDispatchResult::TaskPanic
            }
        }
    }

    /// Whether no registered tasks remain in any availability bucket.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.availability
            .iter()
            .flatten()
            .all(|lane| lane.buckets.iter().all(TaskPool::is_blocking_idle))
    }

    /// Starts split runtime scheduler dispatch.
    #[doc(hidden)]
    pub fn begin_runtime_dispatch_tick(&mut self, meta: TickMeta) -> RuntimeTickDispatchOrder {
        self.begin_runtime_dispatch_stage(meta, false)
    }

    /// Starts split dispatch for internal finalizers only.
    #[doc(hidden)]
    pub fn begin_runtime_finalizer_dispatch_tick(&mut self, meta: TickMeta) -> RuntimeTickDispatchOrder {
        self.begin_runtime_dispatch_stage(meta, true)
    }

    fn begin_runtime_dispatch_stage(&mut self, meta: TickMeta, finalizers_only: bool) -> RuntimeTickDispatchOrder {
        self.dispatching = true;
        let mut order = std::mem::take(&mut self.dispatch_order);
        order.clear();
        let buckets_scanned = if finalizers_only {
            self.collect_order_stage(meta, &mut order, true)
        } else {
            self.collect_order(meta, &mut order)
        };
        RuntimeTickDispatchOrder {
            order,
            index: 0,
            buckets_scanned,
        }
    }

    /// Ends split runtime scheduler dispatch.
    #[doc(hidden)]
    pub fn end_runtime_dispatch_tick(&mut self, order: RuntimeTickDispatchOrder) {
        self.dispatching = false;
        self.dispatch_order = order.order;
    }

    /// Takes the next runnable callback from split dispatch order.
    #[doc(hidden)]
    pub fn take_next_runtime_callback(
        &mut self, order: &mut RuntimeTickDispatchOrder, meta: TickMeta,
    ) -> Option<RuntimeTickCallbackRun> {
        while let Some(key) = order.order.get(order.index).copied() {
            order.index += 1;
            let eligible = match key.availability {
                TickAvailability::AnyDispatch => true,
                TickAvailability::Active => matches!(meta.phase, TickPhase::LevelIntro | TickPhase::Playing),
                TickAvailability::Playing => meta.phase == TickPhase::Playing,
            };
            if !eligible {
                continue;
            }
            let Some(callback_run) = self.take_runnable_callback(key, meta) else {
                continue;
            };
            return Some(RuntimeTickCallbackRun {
                key,
                ready_once: callback_run.ready_once,
                callback: callback_run.callback,
            });
        }
        None
    }

    /// Runs a backend-free callback outside scheduler storage.
    #[doc(hidden)]
    pub fn run_runtime_callback_catching(
        callback_run: RuntimeTickCallbackRun, meta: TickMeta,
    ) -> RuntimeTickCallbackOutcome {
        RuntimeTickCallbackOutcome {
            key: callback_run.key,
            outcome: run_runtime_callback(callback_run.callback, callback_run.ready_once, meta),
        }
    }

    /// Restores or stops a callback after split runtime dispatch.
    #[doc(hidden)]
    pub fn finish_runtime_callback(&mut self, outcome: RuntimeTickCallbackOutcome) -> Option<TickDispatchResult> {
        match self.finish_callback_outcome(outcome.key, outcome.outcome, true) {
            FinishedCallback::Continue | FinishedCallback::StoppedSelf => None,
            FinishedCallback::TaskError(error) => Some(TickDispatchResult::TaskError(error)),
            FinishedCallback::TaskPanic(payload) => {
                crate::timeline::resume_timeline_termination(payload);
                Some(TickDispatchResult::TaskPanic)
            }
        }
    }

    /// Whether a dispatch is currently active.
    #[must_use]
    pub const fn is_dispatching(&self) -> bool {
        self.dispatching
    }

    fn dispatch_tick_uncaught(&mut self, meta: TickMeta) -> Result<TickDispatch, RuntimeError> {
        let mut order = std::mem::take(&mut self.dispatch_order);
        order.clear();
        self.collect_order(meta, &mut order);

        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            let mut stopped_self = false;
            for key in order.iter().copied() {
                stopped_self |= self.run_one_uncaught(key, meta)?;
            }
            Ok(if stopped_self {
                TickDispatch::StoppedSelf
            } else {
                TickDispatch::Continue
            })
        }));
        self.dispatch_order = order;
        result.unwrap_or_else(|payload| panic::resume_unwind(payload))
    }

    fn collect_order(&self, meta: TickMeta, order: &mut Vec<TickTaskKey>) -> u64 {
        self.collect_order_stage(meta, order, false) + self.collect_order_stage(meta, order, true)
    }

    fn collect_order_stage(&self, meta: TickMeta, order: &mut Vec<TickTaskKey>, finalizers_only: bool) -> u64 {
        let mut buckets_scanned = 0_u64;
        for lane in TickLane::ALL {
            buckets_scanned +=
                self.collect_availability_lane(TickAvailability::AnyDispatch, lane, meta, order, finalizers_only);
        }

        match meta.phase {
            TickPhase::LevelIntro => {
                for lane in TickLane::ALL {
                    buckets_scanned +=
                        self.collect_availability_lane(TickAvailability::Active, lane, meta, order, finalizers_only);
                }
            }
            TickPhase::Playing => {
                for lane in TickLane::ALL {
                    buckets_scanned +=
                        self.collect_availability_lane(TickAvailability::Active, lane, meta, order, finalizers_only);
                    buckets_scanned +=
                        self.collect_availability_lane(TickAvailability::Playing, lane, meta, order, finalizers_only);
                }
            }
            TickPhase::Unavailable
            | TickPhase::NotReady
            | TickPhase::Menu
            | TickPhase::RoundComplete
            | TickPhase::Finished
            | TickPhase::Other => {}
        }
        buckets_scanned
    }

    fn collect_availability_lane(
        &self, availability: TickAvailability, lane: TickLane, meta: TickMeta, order: &mut Vec<TickTaskKey>,
        finalizers_only: bool,
    ) -> u64 {
        let lane_tasks = &self.availability[availability.index()][lane.index()];
        lane_tasks.collect_order(availability, lane, meta, order, finalizers_only)
    }

    fn run_one_uncaught(&mut self, key: TickTaskKey, meta: TickMeta) -> Result<bool, RuntimeError> {
        let Some(CallbackRun { ready_once, callback }) = self.take_runnable_callback(key, meta) else {
            return Ok(false);
        };

        let outcome = run_runtime_callback(callback, ready_once, meta);
        match self.finish_callback_outcome(key, outcome, false) {
            FinishedCallback::Continue => Ok(false),
            FinishedCallback::StoppedSelf => Ok(true),
            FinishedCallback::TaskError(error) => Err(error),
            FinishedCallback::TaskPanic(payload) => panic::resume_unwind(payload),
        }
    }

    fn finish_callback_outcome(
        &mut self, key: TickTaskKey, outcome: CallbackOutcome, stop_if_restore_missing: bool,
    ) -> FinishedCallback {
        match outcome {
            CallbackOutcome::Continue(callback) => {
                if self.restore_callback(key, callback) {
                    FinishedCallback::Continue
                } else {
                    if stop_if_restore_missing {
                        let _outcome = self.stop_key(key);
                    }
                    FinishedCallback::StoppedSelf
                }
            }
            CallbackOutcome::Pause(callback) => {
                let restored = self.restore_callback(key, callback);
                let state_outcome = restored.then(|| self.change_state_key(key, TickTaskState::Paused));
                if matches!(
                    state_outcome,
                    Some(TickCommandOutcome::Applied | TickCommandOutcome::AlreadyPaused)
                ) {
                    FinishedCallback::Continue
                } else {
                    if stop_if_restore_missing {
                        let _outcome = self.stop_key(key);
                    }
                    FinishedCallback::StoppedSelf
                }
            }
            CallbackOutcome::Stop => {
                let _outcome = self.stop_key(key);
                FinishedCallback::StoppedSelf
            }
            CallbackOutcome::TaskError(error, callback) => {
                if let Some(callback) = callback {
                    // Restoring the closure preserves an explicit pause and cannot
                    // resurrect a cancelled generation or a self-stopped task.
                    self.restore_callback(key, callback);
                } else {
                    let _outcome = self.stop_key(key);
                }
                FinishedCallback::TaskError(error)
            }
            CallbackOutcome::TaskPanic(payload) => {
                let _outcome = self.stop_key(key);
                FinishedCallback::TaskPanic(payload)
            }
        }
    }

    fn take_runnable_callback(&mut self, key: TickTaskKey, meta: TickMeta) -> Option<CallbackRun> {
        self.pool_mut_for_key(key)
            .take_runnable_callback(key.slot, key.generation, meta)
    }

    fn restore_callback(&mut self, key: TickTaskKey, callback: TickCallback) -> bool {
        self.pool_mut_for_key(key)
            .restore_callback(key.slot, key.generation, callback)
    }

    fn stop_key(&mut self, key: TickTaskKey) -> TickCommandOutcome {
        self.lane_tasks_mut(key.availability, key.lane)
            .stop(key.priority, key.slot, key.generation)
    }

    fn change_state_key(&mut self, key: TickTaskKey, target: TickTaskState) -> TickCommandOutcome {
        self.lane_tasks_mut(key.availability, key.lane)
            .change_state(key.priority, key.slot, key.generation, target)
    }

    fn pool(&self, key: TickTaskKey) -> &TaskPool {
        &self.availability[key.availability.index()][key.lane.index()].buckets[key.priority.bucket_index()]
    }

    fn pool_mut_for_key(&mut self, key: TickTaskKey) -> &mut TaskPool {
        self.pool_mut(key.availability, key.lane, key.priority)
    }

    fn pool_mut(&mut self, availability: TickAvailability, lane: TickLane, priority: TickPriority) -> &mut TaskPool {
        &mut self.availability[availability.index()][lane.index()].buckets[priority.bucket_index()]
    }

    fn lane_tasks_mut(&mut self, availability: TickAvailability, lane: TickLane) -> &mut LaneTasks {
        &mut self.availability[availability.index()][lane.index()]
    }

    fn allocate_generation(&mut self) -> NonZeroU64 {
        let generation = self.next_generation;
        let next = generation.get().wrapping_add(1);
        self.next_generation = NonZeroU64::new(next).unwrap_or(NonZeroU64::MIN);
        generation
    }
}

impl Default for TickScheduler {
    fn default() -> Self {
        Self::new()
    }
}

struct LaneTasks {
    buckets: [TaskPool; TickPriority::BUCKETS],
    active_priority_mask: u64,
}

impl LaneTasks {
    fn new() -> Self {
        Self {
            buckets: std::array::from_fn(|_| TaskPool::new()),
            active_priority_mask: 0,
        }
    }

    fn clear(&mut self) {
        for bucket in &mut self.buckets {
            bucket.clear();
        }
        self.active_priority_mask = 0;
    }

    fn clear_lifetime(&mut self, lifetime: TickLifetime) {
        for bucket_index in 0..self.buckets.len() {
            self.buckets[bucket_index].clear_lifetime(lifetime);
            if self.buckets[bucket_index].is_runnable_idle() {
                let priority = TickPriority::from_bucket_index(bucket_index);
                self.clear_priority_active(priority);
            }
        }
    }

    fn rollback_to(&mut self, checkpoint: NonZeroU64) {
        for bucket_index in 0..self.buckets.len() {
            self.buckets[bucket_index].rollback_to(checkpoint);
            if self.buckets[bucket_index].is_runnable_idle() {
                let priority = TickPriority::from_bucket_index(bucket_index);
                self.clear_priority_active(priority);
            }
        }
    }

    fn spawn(&mut self, priority: TickPriority, generation: NonZeroU64, entry: TickEntry) -> usize {
        let slot = self.buckets[priority.bucket_index()].spawn(generation, entry);
        self.set_priority_active(priority);
        slot
    }

    fn stop(&mut self, priority: TickPriority, slot_index: usize, generation: NonZeroU64) -> TickCommandOutcome {
        let bucket_index = priority.bucket_index();
        let outcome = self.buckets[bucket_index].stop(slot_index, generation);
        if matches!(outcome, TickCommandOutcome::Applied) && self.buckets[bucket_index].is_runnable_idle() {
            self.clear_priority_active(priority);
        }
        outcome
    }

    fn change_state(
        &mut self, priority: TickPriority, slot_index: usize, generation: NonZeroU64, target: TickTaskState,
    ) -> TickCommandOutcome {
        let bucket_index = priority.bucket_index();
        let outcome = self.buckets[bucket_index].change_state(slot_index, generation, target);
        if outcome == TickCommandOutcome::Applied {
            if target == TickTaskState::Running {
                self.set_priority_active(priority);
            } else if self.buckets[bucket_index].is_runnable_idle() {
                self.clear_priority_active(priority);
            }
        }
        outcome
    }

    fn collect_order(
        &self, availability: TickAvailability, lane: TickLane, meta: TickMeta, order: &mut Vec<TickTaskKey>,
        finalizers_only: bool,
    ) -> u64 {
        let finalizer = priority_mask(TickPriority::FINALIZER);
        let mut mask = if finalizers_only {
            self.active_priority_mask & finalizer
        } else {
            self.active_priority_mask & !finalizer
        };
        let mut buckets_scanned = 0_u64;
        while mask != 0 {
            let bucket_index = mask.trailing_zeros() as usize;
            let priority = TickPriority::from_bucket_index(bucket_index);
            self.buckets[bucket_index].collect_order(availability, lane, priority, meta, order);
            mask &= !(1_u64 << bucket_index);
            buckets_scanned += 1;
        }
        buckets_scanned
    }

    fn set_priority_active(&mut self, priority: TickPriority) {
        self.active_priority_mask |= priority_mask(priority);
    }

    fn clear_priority_active(&mut self, priority: TickPriority) {
        self.active_priority_mask &= !priority_mask(priority);
    }
}

fn priority_mask(priority: TickPriority) -> u64 {
    1_u64 << priority.bucket_index()
}

struct TaskPool {
    slots: Vec<TaskSlot>,
    free_head: Option<usize>,
    active_head: Option<usize>,
    active_tail: Option<usize>,
    runnable_head: Option<usize>,
    runnable_tail: Option<usize>,
}

#[expect(
    clippy::indexing_slicing,
    reason = "task pool active/free lists store slot indices created and maintained by the pool"
)]
impl TaskPool {
    const fn new() -> Self {
        Self {
            slots: Vec::new(),
            free_head: None,
            active_head: None,
            active_tail: None,
            runnable_head: None,
            runnable_tail: None,
        }
    }

    fn spawn(&mut self, generation: NonZeroU64, entry: TickEntry) -> usize {
        let slot_index = if let Some(slot_index) = self.free_head {
            let next_free = self.slots[slot_index].next_free;
            self.free_head = next_free;
            let slot = &mut self.slots[slot_index];
            slot.generation = generation;
            slot.state = TickTaskState::Running;
            slot.entry = Some(entry);
            slot.next_free = None;
            slot.prev_active = None;
            slot.next_active = None;
            slot.prev_runnable = None;
            slot.next_runnable = None;
            slot_index
        } else {
            let slot_index = self.slots.len();
            self.slots.push(TaskSlot {
                entry: Some(entry),
                generation,
                state: TickTaskState::Running,
                next_free: None,
                prev_active: None,
                next_active: None,
                prev_runnable: None,
                next_runnable: None,
            });
            slot_index
        };

        self.push_active_back(slot_index);
        self.push_runnable_back(slot_index);
        slot_index
    }

    fn state(&self, slot_index: usize, generation: NonZeroU64) -> TickTaskState {
        let Some(slot) = self.slots.get(slot_index) else {
            return TickTaskState::Stopped;
        };
        if slot.generation != generation || slot.entry.is_none() {
            return TickTaskState::Stopped;
        }
        slot.state
    }

    fn change_state(&mut self, slot_index: usize, generation: NonZeroU64, target: TickTaskState) -> TickCommandOutcome {
        let Some(slot) = self.slots.get(slot_index) else {
            return TickCommandOutcome::NotFound;
        };
        if slot.generation != generation {
            return TickCommandOutcome::StaleHandle;
        }
        if slot.entry.is_none() || slot.state == TickTaskState::Stopped {
            return TickCommandOutcome::AlreadyStopped;
        }
        match (slot.state, target) {
            (TickTaskState::Running, TickTaskState::Paused) => {
                self.unlink_runnable(slot_index);
                self.slots[slot_index].state = TickTaskState::Paused;
                TickCommandOutcome::Applied
            }
            (TickTaskState::Paused, TickTaskState::Running) => {
                self.slots[slot_index].state = TickTaskState::Running;
                self.insert_runnable_in_active_order(slot_index);
                TickCommandOutcome::Applied
            }
            (TickTaskState::Paused, TickTaskState::Paused) => TickCommandOutcome::AlreadyPaused,
            (TickTaskState::Running, TickTaskState::Running) => TickCommandOutcome::AlreadyRunning,
            (TickTaskState::Stopped, _) | (_, TickTaskState::Stopped) => TickCommandOutcome::AlreadyStopped,
        }
    }

    fn stop(&mut self, slot_index: usize, generation: NonZeroU64) -> TickCommandOutcome {
        let Some(slot) = self.slots.get(slot_index) else {
            return TickCommandOutcome::NotFound;
        };
        if slot.generation != generation {
            return TickCommandOutcome::StaleHandle;
        }
        if slot.entry.is_none() || slot.state == TickTaskState::Stopped {
            return TickCommandOutcome::AlreadyStopped;
        }

        if self.slots[slot_index].state == TickTaskState::Running {
            self.unlink_runnable(slot_index);
        }
        self.unlink_active(slot_index);
        let slot = &mut self.slots[slot_index];
        slot.state = TickTaskState::Stopped;
        slot.entry = None;
        slot.next_free = self.free_head;
        self.free_head = Some(slot_index);
        TickCommandOutcome::Applied
    }

    fn clear(&mut self) {
        self.slots.clear();
        self.free_head = None;
        self.active_head = None;
        self.active_tail = None;
        self.runnable_head = None;
        self.runnable_tail = None;
    }

    fn clear_lifetime(&mut self, lifetime: TickLifetime) {
        let mut current = self.active_head;
        while let Some(slot_index) = current {
            let next = self.slots[slot_index].next_active;
            let generation = self.slots[slot_index].generation;
            if self.slots[slot_index]
                .entry
                .as_ref()
                .is_some_and(|entry| entry.lifetime == lifetime)
            {
                let _outcome = self.stop(slot_index, generation);
            }
            current = next;
        }
    }

    fn rollback_to(&mut self, checkpoint: NonZeroU64) {
        let mut current = self.active_head;
        while let Some(slot_index) = current {
            let next = self.slots[slot_index].next_active;
            let generation = self.slots[slot_index].generation;
            if generation.get().wrapping_sub(checkpoint.get()) < (1_u64 << 63) {
                let _outcome = self.stop(slot_index, generation);
            }
            current = next;
        }
    }

    fn is_runnable_idle(&self) -> bool {
        self.runnable_head.is_none()
    }

    fn is_blocking_idle(&self) -> bool {
        let mut current = self.active_head;
        while let Some(slot_index) = current {
            let slot = &self.slots[slot_index];
            if slot.entry.as_ref().is_some_and(|entry| !entry.idle_neutral) {
                return false;
            }
            current = slot.next_active;
        }
        true
    }

    fn collect_order(
        &self, availability: TickAvailability, lane: TickLane, priority: TickPriority, meta: TickMeta,
        order: &mut Vec<TickTaskKey>,
    ) {
        let mut current = self.runnable_head;
        while let Some(slot_index) = current {
            let slot = &self.slots[slot_index];
            debug_assert_eq!(slot.state, TickTaskState::Running);
            if slot.entry.as_ref().is_some_and(|entry| entry.is_eligible(meta)) {
                order.push(TickTaskKey {
                    availability,
                    lane,
                    priority,
                    slot: slot_index,
                    generation: slot.generation,
                });
            }
            current = slot.next_runnable;
        }
    }

    fn take_runnable_callback(
        &mut self, slot_index: usize, generation: NonZeroU64, meta: TickMeta,
    ) -> Option<CallbackRun> {
        let slot = self.slots.get_mut(slot_index)?;
        if slot.generation != generation || slot.state != TickTaskState::Running {
            return None;
        }
        let entry = slot.entry.as_mut()?;
        if !entry.is_eligible(meta) {
            return None;
        }
        let callback = entry.callback.take()?;
        Some(CallbackRun {
            ready_once: entry.is_ready_once(),
            callback,
        })
    }

    fn restore_callback(&mut self, slot_index: usize, generation: NonZeroU64, callback: TickCallback) -> bool {
        let Some(slot) = self.slots.get_mut(slot_index) else {
            return false;
        };
        if slot.generation != generation || slot.state == TickTaskState::Stopped {
            return false;
        }
        let Some(entry) = slot.entry.as_mut() else {
            return false;
        };
        if entry.callback.is_none() {
            entry.callback = Some(callback);
            return true;
        }
        false
    }

    fn push_active_back(&mut self, slot_index: usize) {
        let previous_tail = self.active_tail;
        if let Some(tail) = previous_tail {
            self.slots[tail].next_active = Some(slot_index);
        } else {
            self.active_head = Some(slot_index);
        }

        let slot = &mut self.slots[slot_index];
        slot.prev_active = previous_tail;
        slot.next_active = None;
        self.active_tail = Some(slot_index);
    }

    fn push_runnable_back(&mut self, slot_index: usize) {
        self.link_runnable_between(slot_index, self.runnable_tail, None);
    }

    fn insert_runnable_in_active_order(&mut self, slot_index: usize) {
        let mut previous = self.slots[slot_index].prev_active;
        while let Some(previous_index) = previous {
            if self.slots[previous_index].state == TickTaskState::Running {
                let next = self.slots[previous_index].next_runnable;
                self.link_runnable_between(slot_index, Some(previous_index), next);
                return;
            }
            previous = self.slots[previous_index].prev_active;
        }
        self.link_runnable_between(slot_index, None, self.runnable_head);
    }

    fn link_runnable_between(&mut self, slot_index: usize, previous: Option<usize>, next: Option<usize>) {
        if let Some(previous) = previous {
            self.slots[previous].next_runnable = Some(slot_index);
        } else {
            self.runnable_head = Some(slot_index);
        }
        if let Some(next) = next {
            self.slots[next].prev_runnable = Some(slot_index);
        } else {
            self.runnable_tail = Some(slot_index);
        }

        let slot = &mut self.slots[slot_index];
        slot.prev_runnable = previous;
        slot.next_runnable = next;
    }

    fn unlink_runnable(&mut self, slot_index: usize) {
        let previous = self.slots[slot_index].prev_runnable;
        let next = self.slots[slot_index].next_runnable;

        if let Some(previous) = previous {
            self.slots[previous].next_runnable = next;
        } else {
            self.runnable_head = next;
        }
        if let Some(next) = next {
            self.slots[next].prev_runnable = previous;
        } else {
            self.runnable_tail = previous;
        }

        let slot = &mut self.slots[slot_index];
        slot.prev_runnable = None;
        slot.next_runnable = None;
    }

    fn unlink_active(&mut self, slot_index: usize) {
        let prev = self.slots[slot_index].prev_active;
        let next = self.slots[slot_index].next_active;

        if let Some(prev) = prev {
            self.slots[prev].next_active = next;
        } else {
            self.active_head = next;
        }

        if let Some(next) = next {
            self.slots[next].prev_active = prev;
        } else {
            self.active_tail = prev;
        }

        let slot = &mut self.slots[slot_index];
        slot.prev_active = None;
        slot.next_active = None;
    }
}

struct TaskSlot {
    entry: Option<TickEntry>,
    generation: NonZeroU64,
    state: TickTaskState,
    next_free: Option<usize>,
    prev_active: Option<usize>,
    next_active: Option<usize>,
    prev_runnable: Option<usize>,
    next_runnable: Option<usize>,
}

struct TickEntry {
    lifetime: TickLifetime,
    frame_gate: TickFrameGate,
    trigger: TickTrigger,
    idle_neutral: bool,
    callback: Option<TickCallback>,
}

impl TickEntry {
    fn is_eligible(&self, meta: TickMeta) -> bool {
        match self.frame_gate {
            TickFrameGate::EveryDispatch => {}
            TickFrameGate::NewPlayingFrame => {
                if !(meta.phase == TickPhase::Playing && meta.is_new_frame) {
                    return false;
                }
            }
        }

        match self.trigger {
            TickTrigger::Repeating => true,
            TickTrigger::OnceReady(target) => meta.phase == target,
        }
    }

    const fn is_ready_once(&self) -> bool {
        matches!(self.trigger, TickTrigger::OnceReady(_))
    }
}

struct CallbackRun {
    ready_once: bool,
    callback: TickCallback,
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::panic::{self, AssertUnwindSafe};
    use std::rc::Rc;

    use super::*;
    use crate::model::GameUi;

    fn playing_meta(clock: i32) -> TickMeta {
        TickMeta {
            phase: TickPhase::Playing,
            game_ui: Some(GameUi::Playing),
            clock: Some(clock),
            is_new_frame: true,
        }
    }

    fn phase_meta(phase: TickPhase) -> TickMeta {
        TickMeta {
            phase,
            game_ui: None,
            clock: None,
            is_new_frame: false,
        }
    }

    fn dispatch_ok(scheduler: &mut TickScheduler, meta: TickMeta) {
        let result = scheduler.dispatch_tick(meta);
        assert!(result.is_ok(), "dispatch should succeed: {result:?}");
    }

    type Log = Rc<RefCell<Vec<&'static str>>>;

    fn push_task(
        log: &Log, label: &'static str, control: TickControl,
    ) -> impl FnMut(TickMeta) -> Result<TickControl, RuntimeError> + 'static {
        let log = Rc::clone(log);
        move |_meta| {
            log.borrow_mut().push(label);
            Ok(control)
        }
    }

    fn logged(log: &Log) -> Vec<&'static str> {
        log.borrow().clone()
    }

    #[test]
    fn priority_range_is_checked() {
        assert_eq!(TickPriority::try_new(TickPriority::MIN), Ok(TickPriority::new(-20)));
        assert_eq!(TickPriority::try_new(TickPriority::MAX), Ok(TickPriority::CRITICAL));
        assert!(TickPriority::try_new(-21).is_err());
        assert!(TickPriority::try_new(21).is_err());

        let panic_result = panic::catch_unwind(|| TickPriority::new(21));
        assert!(
            panic_result.is_err(),
            "unchecked constructor should panic on invalid priority"
        );
    }

    #[test]
    fn orders_high_priority_first_then_fifo() {
        let mut scheduler = TickScheduler::new();
        let log = Log::default();
        scheduler.spawn(
            TickOptions::playing_frame(),
            push_task(&log, "normal-a", TickControl::Continue),
        );
        scheduler.spawn(
            TickOptions::playing_frame().priority(TickPriority::HIGH),
            push_task(&log, "high", TickControl::Continue),
        );
        scheduler.spawn(
            TickOptions::playing_frame(),
            push_task(&log, "normal-b", TickControl::Continue),
        );

        dispatch_ok(&mut scheduler, playing_meta(1));

        assert_eq!(logged(&log), ["high", "normal-a", "normal-b"]);
    }

    #[test]
    fn internal_finalizer_runs_after_every_user_availability() {
        let mut scheduler = TickScheduler::new();
        let log = Log::default();
        scheduler.spawn_runtime_finalizer(
            TickOptions::any_dispatch().lane(TickLane::After),
            push_task(&log, "finalizer", TickControl::Continue),
        );
        for (options, name) in [
            (TickOptions::any_dispatch(), "any-user"),
            (TickOptions::active_phase(), "active-user"),
            (TickOptions::playing_frame(), "playing-user"),
        ] {
            scheduler.spawn(
                options
                    .lane(TickLane::After)
                    .priority(TickPriority::new(TickPriority::MIN)),
                push_task(&log, name, TickControl::Continue),
            );
        }

        dispatch_ok(&mut scheduler, playing_meta(1));

        assert_eq!(logged(&log), ["any-user", "active-user", "playing-user", "finalizer"]);
    }

    #[test]
    fn fifo_survives_slot_reuse() {
        let mut scheduler = TickScheduler::new();
        let log = Log::default();
        let first = scheduler.spawn(
            TickOptions::playing_frame(),
            push_task(&log, "first", TickControl::Continue),
        );
        scheduler.spawn(
            TickOptions::playing_frame(),
            push_task(&log, "second", TickControl::Continue),
        );
        assert_eq!(scheduler.stop(first), TickCommandOutcome::Applied);
        scheduler.spawn(
            TickOptions::playing_frame(),
            push_task(&log, "third", TickControl::Continue),
        );

        dispatch_ok(&mut scheduler, playing_meta(1));

        assert_eq!(logged(&log), ["second", "third"]);
    }

    #[test]
    fn lane_order_is_system_observe_act_after() {
        let mut scheduler = TickScheduler::new();
        let log = Log::default();
        for (lane, label) in [
            (TickLane::After, "after"),
            (TickLane::Act, "act"),
            (TickLane::System, "system"),
            (TickLane::Observe, "observe"),
        ] {
            scheduler.spawn(
                TickOptions::active_phase().lane(lane),
                push_task(&log, label, TickControl::Continue),
            );
        }

        dispatch_ok(&mut scheduler, playing_meta(1));

        assert_eq!(logged(&log), ["system", "observe", "act", "after"]);
    }

    #[test]
    fn active_scope_runs_before_only_fight_in_same_lane() {
        let mut scheduler = TickScheduler::new();
        let log = Log::default();
        scheduler.spawn(
            TickOptions::playing_frame(),
            push_task(&log, "only-fight", TickControl::Continue),
        );
        scheduler.spawn(
            TickOptions::active_phase(),
            push_task(&log, "active", TickControl::Continue),
        );

        dispatch_ok(&mut scheduler, playing_meta(1));

        assert_eq!(logged(&log), ["active", "only-fight"]);
    }

    #[test]
    fn pause_skips_task_until_resume() {
        let mut scheduler = TickScheduler::new();
        let log = Log::default();
        let handle = scheduler.spawn(
            TickOptions::playing_frame(),
            push_task(&log, "run", TickControl::Continue),
        );

        assert_eq!(scheduler.pause(handle), TickCommandOutcome::Applied);
        let paused_order = scheduler.begin_runtime_dispatch_tick(playing_meta(1));
        assert_eq!(
            paused_order.buckets_scanned(),
            0,
            "a bucket containing only paused tasks must leave the runtime hot path"
        );
        assert_eq!(
            paused_order.queued_len(),
            0,
            "paused tasks must not enter the dispatch snapshot"
        );
        scheduler.end_runtime_dispatch_tick(paused_order);
        dispatch_ok(&mut scheduler, playing_meta(1));
        assert!(log.borrow().is_empty(), "paused task should not run");

        assert_eq!(scheduler.resume(handle), TickCommandOutcome::Applied);
        dispatch_ok(&mut scheduler, playing_meta(2));
        assert_eq!(logged(&log), ["run"]);

        let later = scheduler.spawn(
            TickOptions::playing_frame(),
            push_task(&log, "later", TickControl::Continue),
        );
        assert_eq!(scheduler.pause(later), TickCommandOutcome::Applied);
        scheduler.spawn(
            TickOptions::playing_frame().priority(TickPriority::HIGH),
            push_task(&log, "resumer", TickControl::Continue),
        );
        assert_eq!(scheduler.resume(later), TickCommandOutcome::Applied);

        dispatch_ok(&mut scheduler, playing_meta(3));
        assert_eq!(logged(&log), ["run", "resumer", "run", "later"]);
    }

    #[test]
    fn paused_tasks_leave_the_runnable_chain_and_resume_in_fifo_order() {
        let mut scheduler = TickScheduler::new();
        let log = Log::default();
        let first = scheduler.spawn(
            TickOptions::playing_frame(),
            push_task(&log, "first", TickControl::Continue),
        );
        let middle = scheduler.spawn(
            TickOptions::playing_frame(),
            push_task(&log, "middle", TickControl::Continue),
        );
        let last = scheduler.spawn(
            TickOptions::playing_frame(),
            push_task(&log, "last", TickControl::Continue),
        );

        assert_eq!(scheduler.pause(middle), TickCommandOutcome::Applied);
        let pool = scheduler.pool(first.key());
        assert_eq!(pool.runnable_head, Some(first.key().slot));
        assert_eq!(pool.slots[first.key().slot].next_runnable, Some(last.key().slot));
        assert_eq!(pool.slots[last.key().slot].prev_runnable, Some(first.key().slot));
        assert_eq!(pool.slots[last.key().slot].next_runnable, None);
        let paused_order = scheduler.begin_runtime_dispatch_tick(playing_meta(1));
        assert_eq!(paused_order.buckets_scanned(), 1);
        assert_eq!(paused_order.queued_len(), 2);
        scheduler.end_runtime_dispatch_tick(paused_order);
        dispatch_ok(&mut scheduler, playing_meta(1));
        assert_eq!(logged(&log), ["first", "last"]);

        assert_eq!(scheduler.pause(first), TickCommandOutcome::Applied);
        assert_eq!(scheduler.resume(middle), TickCommandOutcome::Applied);
        assert_eq!(scheduler.resume(first), TickCommandOutcome::Applied);
        log.borrow_mut().clear();
        dispatch_ok(&mut scheduler, playing_meta(2));
        assert_eq!(logged(&log), ["first", "middle", "last"]);
    }

    #[test]
    fn stopping_a_paused_task_keeps_other_runnable_links_intact() {
        let mut scheduler = TickScheduler::new();
        let log = Log::default();
        let paused = scheduler.spawn(
            TickOptions::playing_frame(),
            push_task(&log, "paused", TickControl::Continue),
        );
        scheduler.spawn(
            TickOptions::playing_frame(),
            push_task(&log, "running", TickControl::Continue),
        );

        assert_eq!(scheduler.pause(paused), TickCommandOutcome::Applied);
        assert_eq!(scheduler.stop(paused), TickCommandOutcome::Applied);
        dispatch_ok(&mut scheduler, playing_meta(1));
        assert_eq!(logged(&log), ["running"]);
    }

    #[test]
    fn paused_sibling_does_not_keep_an_empty_priority_bucket_hot() {
        let mut scheduler = TickScheduler::new();
        let paused = scheduler.spawn(TickOptions::playing_frame(), |_meta| Ok(TickControl::Continue));
        let running = scheduler.spawn(TickOptions::playing_frame(), |_meta| Ok(TickControl::Continue));

        assert_eq!(scheduler.pause(paused), TickCommandOutcome::Applied);
        assert_eq!(scheduler.stop(running), TickCommandOutcome::Applied);
        let idle_order = scheduler.begin_runtime_dispatch_tick(playing_meta(1));
        assert_eq!((idle_order.buckets_scanned(), idle_order.queued_len()), (0, 0));
        scheduler.end_runtime_dispatch_tick(idle_order);

        assert_eq!(scheduler.resume(paused), TickCommandOutcome::Applied);
        let resumed_order = scheduler.begin_runtime_dispatch_tick(playing_meta(2));
        assert_eq!((resumed_order.buckets_scanned(), resumed_order.queued_len()), (1, 1));
        scheduler.end_runtime_dispatch_tick(resumed_order);
    }

    #[test]
    fn callback_can_pause_itself_without_losing_its_allocation() {
        let mut scheduler = TickScheduler::new();
        let log = Log::default();
        let handle = scheduler.spawn(
            TickOptions::playing_frame(),
            push_task(&log, "pause", TickControl::Pause),
        );

        dispatch_ok(&mut scheduler, playing_meta(1));
        assert_eq!(scheduler.state(handle), TickTaskState::Paused);
        assert_eq!(logged(&log), ["pause"]);

        let paused_order = scheduler.begin_runtime_dispatch_tick(playing_meta(2));
        assert_eq!((paused_order.buckets_scanned(), paused_order.queued_len()), (0, 0));
        scheduler.end_runtime_dispatch_tick(paused_order);

        dispatch_ok(&mut scheduler, playing_meta(2));
        assert_eq!(logged(&log), ["pause"]);
        assert_eq!(scheduler.resume(handle), TickCommandOutcome::Applied);
        dispatch_ok(&mut scheduler, playing_meta(3));
        assert_eq!(scheduler.state(handle), TickTaskState::Paused);
        assert_eq!(logged(&log), ["pause", "pause"]);
    }

    #[test]
    fn split_callback_pause_is_idempotent_with_handle_pause() {
        let mut scheduler = TickScheduler::new();
        let handle = scheduler.spawn(TickOptions::playing_frame(), |_meta| Ok(TickControl::Pause));
        let meta = playing_meta(1);
        let mut order = scheduler.begin_runtime_dispatch_tick(meta);
        let callback = scheduler
            .take_next_runtime_callback(&mut order, meta)
            .expect("callback must be present in the split snapshot");

        assert_eq!(scheduler.pause(handle), TickCommandOutcome::Applied);
        let outcome = TickScheduler::run_runtime_callback_catching(callback, meta);
        assert!(scheduler.finish_runtime_callback(outcome).is_none());
        scheduler.end_runtime_dispatch_tick(order);

        assert_eq!(scheduler.state(handle), TickTaskState::Paused);
        assert_eq!(scheduler.resume(handle), TickCommandOutcome::Applied);
        let resumed = scheduler.begin_runtime_dispatch_tick(playing_meta(2));
        assert_eq!((resumed.buckets_scanned(), resumed.queued_len()), (1, 1));
        scheduler.end_runtime_dispatch_tick(resumed);
    }

    #[test]
    fn stale_generation_does_not_control_reused_slot() {
        let mut scheduler = TickScheduler::new();
        let first = scheduler.spawn(TickOptions::playing_frame(), |_ctx| Ok(TickControl::Continue));
        assert_eq!(scheduler.stop(first), TickCommandOutcome::Applied);
        let second = scheduler.spawn(TickOptions::playing_frame(), |_ctx| Ok(TickControl::Continue));

        assert_ne!(first, second, "reused slot should receive a new generation");
        assert_eq!(scheduler.pause(first), TickCommandOutcome::StaleHandle);
        assert_eq!(scheduler.pause(second), TickCommandOutcome::Applied);
        let paused_order = scheduler.begin_runtime_dispatch_tick(playing_meta(1));
        assert_eq!((paused_order.buckets_scanned(), paused_order.queued_len()), (0, 0));
        scheduler.end_runtime_dispatch_tick(paused_order);
        assert_eq!(scheduler.resume(first), TickCommandOutcome::StaleHandle);
        assert_eq!(scheduler.resume(second), TickCommandOutcome::Applied);
        let resumed_order = scheduler.begin_runtime_dispatch_tick(playing_meta(2));
        assert_eq!((resumed_order.buckets_scanned(), resumed_order.queued_len()), (1, 1));
        scheduler.end_runtime_dispatch_tick(resumed_order);
    }

    #[test]
    fn new_task_registered_during_tick_runs_next_tick() {
        let mut scheduler = TickScheduler::new();
        let log = Log::default();
        scheduler.spawn(
            TickOptions::playing_frame(),
            push_task(&log, "a", TickControl::Continue),
        );

        dispatch_ok(&mut scheduler, playing_meta(1));
        assert_eq!(logged(&log), ["a"]);

        scheduler.spawn(
            TickOptions::playing_frame(),
            push_task(&log, "b", TickControl::Continue),
        );

        dispatch_ok(&mut scheduler, playing_meta(2));
        assert_eq!(logged(&log), ["a", "a", "b"]);
    }

    #[test]
    fn runtime_dispatch_runs_backend_free_tasks_without_backend_context() {
        let mut scheduler = TickScheduler::new();
        scheduler.spawn(TickOptions::any_dispatch(), |meta| {
            assert_eq!(meta.phase, TickPhase::Menu);
            Ok(TickControl::Stop)
        });

        assert!(matches!(
            scheduler.dispatch_tick_catching(phase_meta(TickPhase::Menu)),
            TickDispatchResult::Continue
        ));
    }

    #[test]
    fn idle_neutral_task_does_not_block_idle() {
        let mut scheduler = TickScheduler::new();
        assert!(scheduler.is_idle());

        scheduler.spawn(TickOptions::playing_frame().idle_neutral(), |_backend| {
            Ok(TickControl::Continue)
        });
        assert!(scheduler.is_idle());

        let handle = scheduler.spawn(TickOptions::playing_frame(), |_meta| Ok(TickControl::Continue));
        assert!(!scheduler.is_idle());

        assert_eq!(scheduler.stop(handle), TickCommandOutcome::Applied);
        assert!(scheduler.is_idle());
    }

    #[test]
    fn runtime_dispatch_reports_actual_non_empty_buckets_scanned() {
        let mut scheduler = TickScheduler::new();

        let empty = scheduler.begin_runtime_dispatch_tick(playing_meta(1));
        assert_eq!(empty.buckets_scanned(), 0);
        assert_eq!(empty.queued_len(), 0);
        scheduler.end_runtime_dispatch_tick(empty);

        let handle = scheduler.spawn(TickOptions::playing_frame(), |_meta| Ok(TickControl::Continue));
        let one_bucket = scheduler.begin_runtime_dispatch_tick(playing_meta(2));
        assert_eq!(one_bucket.buckets_scanned(), 1);
        assert_eq!(one_bucket.queued_len(), 1);
        scheduler.end_runtime_dispatch_tick(one_bucket);

        assert_eq!(scheduler.stop(handle), TickCommandOutcome::Applied);
        let stopped = scheduler.begin_runtime_dispatch_tick(playing_meta(3));
        assert_eq!(stopped.buckets_scanned(), 0);
        assert_eq!(stopped.queued_len(), 0);
        scheduler.end_runtime_dispatch_tick(stopped);
    }

    #[test]
    fn self_stop_via_return_removes_only_current_task() {
        let mut scheduler = TickScheduler::new();
        let log = Log::default();
        let log_a = Rc::clone(&log);
        scheduler.spawn(TickOptions::playing_frame(), move |_meta| {
            log_a.borrow_mut().push("a");
            Ok(TickControl::Stop)
        });
        scheduler.spawn(
            TickOptions::playing_frame(),
            push_task(&log, "b", TickControl::Continue),
        );

        dispatch_ok(&mut scheduler, playing_meta(1));
        dispatch_ok(&mut scheduler, playing_meta(2));

        assert_eq!(logged(&log), ["a", "b", "b"]);
    }

    #[test]
    fn stop_prevents_callback_restore() {
        let mut scheduler = TickScheduler::new();
        let log = Log::default();
        let handle = scheduler.spawn(
            TickOptions::playing_frame(),
            push_task(&log, "self", TickControl::Continue),
        );
        assert_eq!(scheduler.stop(handle), TickCommandOutcome::Applied);

        dispatch_ok(&mut scheduler, playing_meta(1));

        assert!(log.borrow().is_empty());
        assert_eq!(scheduler.state(handle), TickTaskState::Stopped);
    }

    #[test]
    fn stopped_task_is_skipped_in_snapshot_order() {
        let mut scheduler = TickScheduler::new();
        let log = Log::default();
        let later = scheduler.spawn(
            TickOptions::playing_frame(),
            push_task(&log, "later", TickControl::Continue),
        );
        scheduler.spawn(
            TickOptions::playing_frame().priority(TickPriority::HIGH),
            push_task(&log, "stopper", TickControl::Continue),
        );
        assert_eq!(scheduler.stop(later), TickCommandOutcome::Applied);

        dispatch_ok(&mut scheduler, playing_meta(1));

        assert_eq!(logged(&log), ["stopper"]);
        assert_eq!(scheduler.state(later), TickTaskState::Stopped);
    }

    #[test]
    fn playing_frame_requires_playing_new_frame() {
        let mut scheduler = TickScheduler::new();
        let log = Log::default();
        scheduler.spawn(
            TickOptions::playing_frame(),
            push_task(&log, "run", TickControl::Continue),
        );

        dispatch_ok(
            &mut scheduler,
            TickMeta {
                is_new_frame: false,
                ..playing_meta(1)
            },
        );
        dispatch_ok(&mut scheduler, phase_meta(TickPhase::LevelIntro));
        dispatch_ok(&mut scheduler, playing_meta(2));

        assert_eq!(logged(&log), ["run"]);
    }

    #[test]
    fn active_phase_runs_only_in_level_intro_or_playing() {
        let mut scheduler = TickScheduler::new();
        let log = Log::default();
        scheduler.spawn(
            TickOptions::active_phase(),
            push_task(&log, "run", TickControl::Continue),
        );

        dispatch_ok(&mut scheduler, phase_meta(TickPhase::Menu));
        dispatch_ok(&mut scheduler, phase_meta(TickPhase::LevelIntro));
        dispatch_ok(&mut scheduler, playing_meta(1));
        dispatch_ok(&mut scheduler, phase_meta(TickPhase::Unavailable));

        assert_eq!(logged(&log), ["run", "run"]);
    }

    #[test]
    fn any_dispatch_runs_in_not_ready_and_menu() {
        let mut scheduler = TickScheduler::new();
        let log = Log::default();
        scheduler.spawn(
            TickOptions::any_dispatch(),
            push_task(&log, "run", TickControl::Continue),
        );

        dispatch_ok(&mut scheduler, phase_meta(TickPhase::NotReady));
        dispatch_ok(&mut scheduler, phase_meta(TickPhase::Menu));

        assert_eq!(logged(&log), ["run", "run"]);
    }

    #[test]
    fn once_ready_runs_once_when_registered_before_or_after_ready() {
        let mut scheduler = TickScheduler::new();
        let log = Log::default();
        scheduler.spawn(
            TickOptions::once_level_intro_ready(),
            push_task(&log, "intro-before", TickControl::Continue),
        );

        dispatch_ok(&mut scheduler, phase_meta(TickPhase::Menu));
        dispatch_ok(&mut scheduler, phase_meta(TickPhase::LevelIntro));
        scheduler.spawn(
            TickOptions::once_level_intro_ready(),
            push_task(&log, "intro-after", TickControl::Continue),
        );
        dispatch_ok(&mut scheduler, phase_meta(TickPhase::LevelIntro));
        dispatch_ok(&mut scheduler, phase_meta(TickPhase::LevelIntro));

        assert_eq!(logged(&log), ["intro-before", "intro-after"]);
    }

    #[test]
    fn once_playing_ready_registered_during_playing_runs_next_eligible_dispatch() {
        let mut scheduler = TickScheduler::new();
        let log = Log::default();
        scheduler.spawn(
            TickOptions::any_dispatch(),
            push_task(&log, "bootstrap", TickControl::Stop),
        );

        dispatch_ok(&mut scheduler, playing_meta(1));
        assert_eq!(logged(&log), ["bootstrap"]);

        scheduler.spawn(
            TickOptions::once_playing_ready(),
            push_task(&log, "once", TickControl::Continue),
        );

        dispatch_ok(&mut scheduler, playing_meta(2));
        assert_eq!(logged(&log), ["bootstrap", "once"]);
    }

    #[test]
    fn dispatch_tick_propagates_panic_after_stopping_task() {
        let mut scheduler = TickScheduler::new();
        let handle = scheduler.spawn(
            TickOptions::playing_frame(),
            |_ctx| -> Result<TickControl, RuntimeError> {
                panic!("pure path panic should propagate");
            },
        );

        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            let _dispatch = scheduler.dispatch_tick(playing_meta(1));
        }));

        assert!(result.is_err(), "pure dispatch path should propagate panic");
        assert_eq!(scheduler.state(handle), TickTaskState::Stopped);
        assert!(!scheduler.is_dispatching(), "panic should restore dispatch flag");
    }

    #[test]
    fn dispatch_tick_catching_reports_task_panic() {
        let mut scheduler = TickScheduler::new();
        let handle = scheduler.spawn(
            TickOptions::playing_frame(),
            |_ctx| -> Result<TickControl, RuntimeError> {
                panic!("catching path should contain panic");
            },
        );

        let result = scheduler.dispatch_tick_catching(playing_meta(1));

        assert_eq!(result, TickDispatchResult::TaskPanic);
        assert_eq!(scheduler.state(handle), TickTaskState::Stopped);
        assert!(!scheduler.is_dispatching(), "panic should restore dispatch flag");
    }

    #[test]
    fn dispatch_tick_catching_reports_task_error() {
        let mut scheduler = TickScheduler::new();
        let handle = scheduler.spawn(TickOptions::playing_frame(), |_ctx| {
            Err(RuntimeError::new("expected failure"))
        });

        let result = scheduler.dispatch_tick_catching(playing_meta(1));

        assert_eq!(
            result,
            TickDispatchResult::TaskError(RuntimeError::new("expected failure"))
        );
        assert_eq!(scheduler.state(handle), TickTaskState::Running);
        assert_eq!(
            scheduler.dispatch_tick_catching(playing_meta(2)),
            TickDispatchResult::TaskError(RuntimeError::new("expected failure"))
        );
    }

    #[test]
    fn failed_split_callback_preserves_explicit_state_and_consumes_one_shots() {
        for (options, state) in [
            (TickOptions::playing_frame(), TickTaskState::Paused),
            (TickOptions::playing_frame(), TickTaskState::Stopped),
            (TickOptions::once_playing_ready(), TickTaskState::Running),
        ] {
            let mut scheduler = TickScheduler::new();
            let handle = scheduler.spawn(options, |_| Err(RuntimeError::new("failure")));
            let meta = playing_meta(1);
            let mut order = scheduler.begin_runtime_dispatch_tick(meta);
            let callback = scheduler
                .take_next_runtime_callback(&mut order, meta)
                .expect("callback");
            match state {
                TickTaskState::Paused => {
                    scheduler.pause(handle);
                }
                TickTaskState::Stopped => {
                    scheduler.stop(handle);
                }
                TickTaskState::Running => {}
            }
            let outcome = TickScheduler::run_runtime_callback_catching(callback, meta);
            assert!(matches!(
                scheduler.finish_runtime_callback(outcome),
                Some(TickDispatchResult::TaskError(_))
            ));
            scheduler.end_runtime_dispatch_tick(order);
            let expected = if state == TickTaskState::Paused {
                state
            } else {
                TickTaskState::Stopped
            };
            assert_eq!(scheduler.state(handle), expected);
            let next = scheduler.begin_runtime_dispatch_tick(playing_meta(2));
            assert_eq!(next.queued_len(), 0);
            scheduler.end_runtime_dispatch_tick(next);
        }
    }

    #[test]
    fn clear_script_lifetime_preserves_session_tasks_across_availability() {
        let mut scheduler = TickScheduler::new();
        let log = Log::default();
        let bootstrap = scheduler.spawn(
            TickOptions::any_dispatch().lifetime(TickLifetime::Session),
            push_task(&log, "bootstrap", TickControl::Continue),
        );
        let active = scheduler.spawn(TickOptions::active_phase(), |_meta| Ok(TickControl::Continue));
        let fight = scheduler.spawn(TickOptions::playing_frame(), |_meta| Ok(TickControl::Continue));

        scheduler.clear_lifetime(TickLifetime::Script);

        assert_eq!(scheduler.state(bootstrap), TickTaskState::Running);
        assert_eq!(scheduler.state(active), TickTaskState::Stopped);
        assert_eq!(scheduler.state(fight), TickTaskState::Stopped);
        dispatch_ok(&mut scheduler, phase_meta(TickPhase::Menu));
        assert_eq!(logged(&log), ["bootstrap"]);
    }

    #[test]
    fn registration_rollback_removes_new_session_tasks_and_preserves_older_ones() {
        let mut scheduler = TickScheduler::new();
        let older = scheduler.spawn(TickOptions::any_dispatch().lifetime(TickLifetime::Session), |_meta| {
            Ok(TickControl::Continue)
        });
        let checkpoint = scheduler.checkpoint();
        let session = scheduler.spawn(TickOptions::any_dispatch().lifetime(TickLifetime::Session), |_meta| {
            Ok(TickControl::Continue)
        });
        let script = scheduler.spawn(TickOptions::active_phase(), |_meta| Ok(TickControl::Continue));

        scheduler.rollback_to(checkpoint);

        assert_eq!(scheduler.state(older), TickTaskState::Running);
        assert_eq!(scheduler.state(session), TickTaskState::Stopped);
        assert_eq!(scheduler.state(script), TickTaskState::Stopped);
    }

    #[test]
    fn availability_and_lifetime_are_orthogonal_for_all_nine_combinations() {
        let mut scheduler = TickScheduler::new();
        let mut handles = Vec::new();
        for availability in [
            TickAvailability::AnyDispatch,
            TickAvailability::Active,
            TickAvailability::Playing,
        ] {
            for lifetime in [TickLifetime::Session, TickLifetime::Script, TickLifetime::Fight] {
                handles.push((
                    lifetime,
                    scheduler.spawn(
                        TickOptions::any_dispatch()
                            .availability(availability)
                            .lifetime(lifetime),
                        |_meta| Ok(TickControl::Continue),
                    ),
                ));
            }
        }

        scheduler.clear_lifetime(TickLifetime::Fight);
        for (lifetime, handle) in &handles {
            assert_eq!(
                scheduler.state(*handle),
                if *lifetime == TickLifetime::Fight {
                    TickTaskState::Stopped
                } else {
                    TickTaskState::Running
                }
            );
        }

        scheduler.clear_lifetime(TickLifetime::Script);
        for (lifetime, handle) in &handles {
            assert_eq!(
                scheduler.state(*handle),
                if *lifetime == TickLifetime::Session {
                    TickTaskState::Running
                } else {
                    TickTaskState::Stopped
                }
            );
        }

        scheduler.clear_lifetime(TickLifetime::Session);
        assert!(
            handles
                .iter()
                .all(|(_, handle)| scheduler.state(*handle) == TickTaskState::Stopped)
        );
        assert!(scheduler.is_idle());
    }
}
