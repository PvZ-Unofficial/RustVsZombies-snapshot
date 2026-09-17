//! Backend-neutral event pairing and synchronous library interception.

mod current;
pub use current::{with_events, with_events_ref};

use std::num::NonZeroU64;
use std::panic::{self, AssertUnwindSafe};

use rsvz_model::model::{
    BeginPlantEffect, EffectOutcomeFact, EventDecision, EventFrameStatus, EventInterest, EventToken, GameEvent,
    GargantuarAshHitFact, GargantuarSpawnedFact, HomeEntryEvent, HomeEntryFact, ImpThrownFact, PlantEffectAttemptFact,
    PlantEffectEvent, PlantEffectOutcome,
};

pub use crate::tick::TickLifetime as EventLifetime;

pub const MAX_PENDING_EFFECTS: usize = 32;
pub const DEFAULT_PUBLIC_EVENT_CAPACITY: usize = 256;

type EventCallback = Box<dyn FnMut(&GameEvent)>;

/// Public event callback ordering and lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EventOptions {
    pub order: i32,
    pub lifetime: EventLifetime,
}

impl EventOptions {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            order: 0,
            lifetime: EventLifetime::Script,
        }
    }

    #[must_use]
    pub const fn order(mut self, order: i32) -> Self {
        self.order = order;
        self
    }

    #[must_use]
    pub const fn lifetime(mut self, lifetime: EventLifetime) -> Self {
        self.lifetime = lifetime;
        self
    }
}

impl Default for EventOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// Stable identity for a public event callback.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EventHandle(NonZeroU64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EventCommandOutcome {
    Applied,
    AlreadyRemoved,
    NotFound,
}

/// Library-only synchronous consumer. User callbacks never implement this.
#[doc(hidden)]
pub trait InternalEventInterceptor {
    fn interest(&self) -> EventInterest;

    fn begin_logic_frame(&mut self, _board_epoch: u64, _main_counter: i32) {}

    fn begin_plant_effect(&mut self, _fact: PlantEffectAttemptFact) -> EventDecision {
        EventDecision::Apply
    }

    fn finish_plant_effect(&mut self, _fact: EffectOutcomeFact) {}

    fn emit_home_entry(&mut self, _fact: HomeEntryFact) {}

    fn emit_gargantuar_spawned(&mut self, _fact: GargantuarSpawnedFact) {}

    fn emit_imp_thrown(&mut self, _fact: ImpThrownFact) {}

    fn emit_gargantuar_ash_hit(&mut self, _fact: GargantuarAshHitFact) {}

    fn end_logic_frame(&mut self, _status: EventFrameStatus) {}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EventRegistrationError {
    #[error("event registration is frozen for the active script generation")]
    Frozen,
    #[error("an internal event interceptor is already installed")]
    InterceptorOccupied,
    #[error("public event queue capacity could not be allocated")]
    QueueAllocation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EventBridgeFault {
    #[error("native event frame started while another frame was open")]
    NestedFrame,
    #[error("native event frame ended without a matching begin")]
    FrameNotOpen,
    #[error("native event effect occurred outside a logical frame")]
    EffectOutsideFrame,
    #[error("native event fact main counter did not match its logical frame")]
    MainCounterMismatch,
    #[error("native event pending-effect stack overflowed")]
    PendingOverflow,
    #[error("native event effect token was invalid or out of LIFO order")]
    TokenMismatch,
    #[error("native event effect outcome contradicted its synchronous decision")]
    DecisionOutcomeMismatch,
    #[error("native event frame ended with unfinished effects")]
    PendingAtFrameEnd,
    #[error("internal event interceptor panicked")]
    InterceptorPanic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PendingEffect {
    token: EventToken,
    attempt: PlantEffectAttemptFact,
    decision: EventDecision,
}

struct EventHandler {
    handle: EventHandle,
    options: EventOptions,
    interest: EventInterest,
    callback: Option<EventCallback>,
    active: bool,
}

#[doc(hidden)]
pub struct PublicEventDispatch {
    event_count: usize,
    event_index: usize,
    handler_index: usize,
}

#[doc(hidden)]
pub struct PublicEventCallbackRun {
    slot: usize,
    handle: EventHandle,
    event: GameEvent,
    callback: EventCallback,
}

#[doc(hidden)]
pub struct PublicEventCallbackOutcome {
    slot: usize,
    handle: EventHandle,
    callback: Option<EventCallback>,
    result: PublicEventCallbackResult,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventCheckpoint {
    had_interceptor: bool,
    handler_len: usize,
    requested_queue_capacity: usize,
}

/// Backend-neutral event scheduler and native fact pairer.
pub struct EventDispatcher {
    interceptor: Option<Box<dyn InternalEventInterceptor>>,
    handlers: Vec<EventHandler>,
    next_handler: NonZeroU64,
    frozen: bool,
    interest: EventInterest,
    observer_interest: EventInterest,
    public_pending: Vec<GameEvent>,
    requested_queue_capacity: usize,
    queue_limit: usize,
    public_overflowed: bool,
    frame: Option<(u64, i32)>,
    next_token: u32,
    pending: [Option<PendingEffect>; MAX_PENDING_EFFECTS],
    pending_len: usize,
    fault: Option<EventBridgeFault>,
}

impl EventDispatcher {
    #[must_use]
    pub fn new() -> Self {
        Self {
            interceptor: None,
            handlers: Vec::new(),
            next_handler: NonZeroU64::MIN,
            frozen: true,
            interest: EventInterest::default(),
            observer_interest: EventInterest::default(),
            public_pending: Vec::new(),
            requested_queue_capacity: DEFAULT_PUBLIC_EVENT_CAPACITY,
            queue_limit: 0,
            public_overflowed: false,
            frame: None,
            next_token: 1,
            pending: [None; MAX_PENDING_EFFECTS],
            pending_len: 0,
            fault: None,
        }
    }

    #[must_use]
    pub const fn checkpoint(&self) -> EventCheckpoint {
        EventCheckpoint {
            had_interceptor: self.interceptor.is_some(),
            handler_len: self.handlers.len(),
            requested_queue_capacity: self.requested_queue_capacity,
        }
    }

    pub fn begin_script_generation(&mut self) -> EventCheckpoint {
        self.clear_lifetime(EventLifetime::Script);
        let checkpoint = self.checkpoint();
        self.frozen = false;
        self.interest = EventInterest::default();
        self.observer_interest = EventInterest::default();
        checkpoint
    }

    pub fn rollback_to(&mut self, checkpoint: EventCheckpoint) {
        self.handlers.truncate(checkpoint.handler_len);
        self.requested_queue_capacity = checkpoint.requested_queue_capacity;
        if !checkpoint.had_interceptor {
            self.interceptor = None;
        }
        self.recompute_interest();
        self.frozen = true;
    }

    pub fn register_public_handler(
        &mut self, interest: EventInterest, options: EventOptions, callback: EventCallback,
    ) -> Result<EventHandle, EventRegistrationError> {
        if self.frozen {
            return Err(EventRegistrationError::Frozen);
        }
        let handle = EventHandle(self.next_handler);
        self.next_handler = NonZeroU64::new(self.next_handler.get().wrapping_add(1)).unwrap_or(NonZeroU64::MIN);
        self.handlers.push(EventHandler {
            handle,
            options,
            interest,
            callback: Some(callback),
            active: true,
        });
        Ok(handle)
    }

    pub fn reserve_public_events(&mut self, capacity: usize) -> Result<(), EventRegistrationError> {
        if self.frozen {
            return Err(EventRegistrationError::Frozen);
        }
        self.requested_queue_capacity = self.requested_queue_capacity.max(capacity);
        Ok(())
    }

    pub fn remove_public_handler(&mut self, handle: EventHandle) -> EventCommandOutcome {
        let Some(handler) = self.handlers.iter_mut().find(|handler| handler.handle == handle) else {
            return EventCommandOutcome::NotFound;
        };
        if !handler.active {
            return EventCommandOutcome::AlreadyRemoved;
        }
        handler.active = false;
        handler.callback = None;
        self.recompute_observer_interest();
        EventCommandOutcome::Applied
    }

    pub fn install_internal_interceptor(
        &mut self, interceptor: Box<dyn InternalEventInterceptor>,
    ) -> Result<(), EventRegistrationError> {
        if self.frozen {
            return Err(EventRegistrationError::Frozen);
        }
        if self.interceptor.is_some() {
            return Err(EventRegistrationError::InterceptorOccupied);
        }
        self.interceptor = Some(interceptor);
        Ok(())
    }

    pub fn freeze(&mut self) -> Result<EventInterest, EventRegistrationError> {
        self.handlers.retain(|handler| handler.active);
        let has_observers = !self.handlers.is_empty();
        let queue_limit = if has_observers {
            self.requested_queue_capacity.max(DEFAULT_PUBLIC_EVENT_CAPACITY)
        } else {
            0
        };
        self.public_pending
            .try_reserve_exact(queue_limit.saturating_sub(self.public_pending.len()))
            .map_err(|_error| EventRegistrationError::QueueAllocation)?;
        self.handlers
            .sort_unstable_by_key(|handler| (handler.options.order, handler.handle.0));
        self.queue_limit = queue_limit;
        self.recompute_interest();
        self.frozen = true;
        Ok(self.interest)
    }

    #[must_use]
    pub const fn interest(&self) -> EventInterest {
        self.interest
    }

    #[must_use]
    pub const fn is_frozen(&self) -> bool {
        self.frozen
    }

    pub fn begin_logic_frame(&mut self, board_epoch: u64, main_counter: i32) {
        if self.fault.is_some() {
            return;
        }
        if self.frame.is_some() {
            self.latch_fault(EventBridgeFault::NestedFrame);
            return;
        }
        self.frame = Some((board_epoch, main_counter));
        self.call_interceptor(|interceptor| interceptor.begin_logic_frame(board_epoch, main_counter));
    }

    #[must_use]
    pub fn begin_plant_effect(&mut self, attempt: PlantEffectAttemptFact) -> BeginPlantEffect {
        if self.fault.is_some() {
            return BeginPlantEffect::FAIL_OPEN;
        }
        let Some((_, main_counter)) = self.frame else {
            self.latch_fault(EventBridgeFault::EffectOutsideFrame);
            return BeginPlantEffect::FAIL_OPEN;
        };
        if attempt.main_counter != main_counter {
            self.latch_fault(EventBridgeFault::MainCounterMismatch);
            return BeginPlantEffect::FAIL_OPEN;
        }
        if self.pending_len == MAX_PENDING_EFFECTS {
            self.latch_fault(EventBridgeFault::PendingOverflow);
            return BeginPlantEffect::FAIL_OPEN;
        }

        let mut decision = EventDecision::Apply;
        self.call_interceptor(|interceptor| decision = interceptor.begin_plant_effect(attempt));
        if self.fault.is_some() {
            decision = EventDecision::Apply;
        }
        let token = self.allocate_token();
        self.pending[self.pending_len] = Some(PendingEffect {
            token,
            attempt,
            decision,
        });
        self.pending_len += 1;
        BeginPlantEffect { token, decision }
    }

    pub fn finish_plant_effect(&mut self, token: EventToken, outcome: PlantEffectOutcome) {
        if self.fault.is_some() {
            return;
        }
        if self.frame.is_none() {
            self.latch_fault(EventBridgeFault::EffectOutsideFrame);
            return;
        }
        let Some(index) = self.pending_len.checked_sub(1) else {
            self.latch_fault(EventBridgeFault::TokenMismatch);
            return;
        };
        let Some(pending) = self.pending[index] else {
            self.latch_fault(EventBridgeFault::TokenMismatch);
            return;
        };
        if pending.token != token {
            self.latch_fault(EventBridgeFault::TokenMismatch);
            return;
        }
        self.pending[index] = None;
        self.pending_len = index;

        let outcome_matches = matches!(
            (pending.decision, outcome),
            (
                EventDecision::SuppressByMeasurement,
                PlantEffectOutcome::SuppressedByMeasurement
            )
        ) || matches!(pending.decision, EventDecision::Apply)
            && !matches!(outcome, PlantEffectOutcome::SuppressedByMeasurement);
        if !outcome_matches {
            self.latch_fault(EventBridgeFault::DecisionOutcomeMismatch);
            return;
        }
        let fact = EffectOutcomeFact {
            token,
            key: pending.attempt.key(),
            decision_origin: pending.decision.origin(),
            outcome,
        };
        self.call_interceptor(|interceptor| interceptor.finish_plant_effect(fact));
        if self.fault.is_none() && self.observer_interest.contains(pending.attempt.source.interest()) {
            self.enqueue_public(GameEvent::PlantEffect(PlantEffectEvent {
                source: pending.attempt.source,
                plant_id: pending.attempt.plant_id,
                raw_kind: pending.attempt.raw_kind,
                effective_kind: pending.attempt.effective_kind,
                grid: pending.attempt.grid,
                requested: pending.attempt.effect,
                decision_origin: pending.decision.origin(),
                outcome,
                hp_before: pending.attempt.hp_before,
                max_hp: pending.attempt.max_hp,
                main_counter: pending.attempt.main_counter,
            }));
        }
    }

    pub fn emit_home_entry(&mut self, fact: HomeEntryFact) {
        if self.fault.is_some() {
            return;
        }
        let Some((_, main_counter)) = self.frame else {
            self.latch_fault(EventBridgeFault::EffectOutsideFrame);
            return;
        };
        if fact.main_counter != main_counter {
            self.latch_fault(EventBridgeFault::MainCounterMismatch);
            return;
        }
        self.call_interceptor(|interceptor| interceptor.emit_home_entry(fact));
        if self.fault.is_none() && self.observer_interest.contains(EventInterest::HOME_ENTRY) {
            self.enqueue_public(GameEvent::HomeEntry(HomeEntryEvent::from(fact)));
        }
    }

    pub fn emit_gargantuar_spawned(&mut self, fact: GargantuarSpawnedFact) {
        if self.accept_instant_fact(fact.main_counter) {
            self.call_interceptor(|interceptor| interceptor.emit_gargantuar_spawned(fact));
        }
    }

    pub fn emit_imp_thrown(&mut self, fact: ImpThrownFact) {
        if self.accept_instant_fact(fact.main_counter) {
            self.call_interceptor(|interceptor| interceptor.emit_imp_thrown(fact));
        }
    }

    pub fn emit_gargantuar_ash_hit(&mut self, fact: GargantuarAshHitFact) {
        if self.accept_instant_fact(fact.main_counter) {
            self.call_interceptor(|interceptor| interceptor.emit_gargantuar_ash_hit(fact));
        }
    }

    fn accept_instant_fact(&mut self, main_counter: i32) -> bool {
        if self.fault.is_some() {
            return false;
        }
        let Some((_, frame_counter)) = self.frame else {
            self.latch_fault(EventBridgeFault::EffectOutsideFrame);
            return false;
        };
        if main_counter != frame_counter {
            self.latch_fault(EventBridgeFault::MainCounterMismatch);
            return false;
        }
        true
    }

    pub fn end_logic_frame(&mut self, status: EventFrameStatus) {
        if self.fault.is_some() {
            self.frame = None;
            self.pending.fill(None);
            self.pending_len = 0;
            return;
        }
        if self.frame.is_none() {
            self.latch_fault(EventBridgeFault::FrameNotOpen);
            return;
        }
        if self.pending_len != 0 {
            self.latch_fault(EventBridgeFault::PendingAtFrameEnd);
            self.pending.fill(None);
            self.pending_len = 0;
            self.frame = None;
            return;
        }
        self.call_interceptor(|interceptor| interceptor.end_logic_frame(status));
        self.frame = None;
    }

    /// Validates abnormal attempt closure without running user code.
    pub fn close_fight(&mut self) {
        if self.frame.is_some() {
            self.latch_fault(EventBridgeFault::PendingAtFrameEnd);
        }
        self.frame = None;
        self.pending.fill(None);
        self.pending_len = 0;
        self.public_pending.clear();
        self.public_overflowed = false;
        self.clear_lifetime(EventLifetime::Fight);
    }

    #[must_use]
    pub const fn fault(&self) -> Option<EventBridgeFault> {
        self.fault
    }

    #[must_use]
    pub fn take_fault(&mut self) -> Option<EventBridgeFault> {
        self.fault.take()
    }

    #[must_use]
    pub fn take_public_overflow(&mut self) -> bool {
        std::mem::take(&mut self.public_overflowed)
    }

    #[doc(hidden)]
    #[must_use]
    pub fn begin_public_dispatch(&self) -> Option<PublicEventDispatch> {
        (!self.public_pending.is_empty()).then_some(PublicEventDispatch {
            event_count: self.public_pending.len(),
            event_index: 0,
            handler_index: 0,
        })
    }

    #[doc(hidden)]
    pub fn take_next_public_callback(&mut self, dispatch: &mut PublicEventDispatch) -> Option<PublicEventCallbackRun> {
        while dispatch.event_index < dispatch.event_count {
            let event = self.public_pending[dispatch.event_index];
            while dispatch.handler_index < self.handlers.len() {
                let slot = dispatch.handler_index;
                dispatch.handler_index += 1;
                let handler = &mut self.handlers[slot];
                if !handler.active || !handler.interest.contains(event_interest(event)) {
                    continue;
                }
                let Some(callback) = handler.callback.take() else {
                    continue;
                };
                return Some(PublicEventCallbackRun {
                    slot,
                    handle: handler.handle,
                    event,
                    callback,
                });
            }
            dispatch.event_index += 1;
            dispatch.handler_index = 0;
        }
        None
    }

    #[doc(hidden)]
    #[must_use]
    pub fn run_public_callback_catching(mut run: PublicEventCallbackRun) -> PublicEventCallbackOutcome {
        let result = panic::catch_unwind(AssertUnwindSafe(|| (run.callback)(&run.event)));
        let result = match result {
            Ok(()) => PublicEventCallbackResult::Continue,
            Err(payload) if crate::timeline::is_timeline_termination(&*payload) => {
                PublicEventCallbackResult::TimelineTerminated
            }
            Err(payload) => match crate::callback::into_error(payload) {
                Ok(error) => PublicEventCallbackResult::Aborted(error),
                Err(_) => PublicEventCallbackResult::Panicked,
            },
        };
        PublicEventCallbackOutcome {
            slot: run.slot,
            handle: run.handle,
            callback: (!matches!(
                result,
                PublicEventCallbackResult::Panicked | PublicEventCallbackResult::TimelineTerminated
            ))
            .then_some(run.callback),
            result,
        }
    }

    #[doc(hidden)]
    #[must_use]
    pub fn finish_public_callback(&mut self, mut outcome: PublicEventCallbackOutcome) -> PublicEventCallbackResult {
        let mut removed = false;
        if let Some(handler) = self.handlers.get_mut(outcome.slot)
            && handler.handle == outcome.handle
        {
            if matches!(
                outcome.result,
                PublicEventCallbackResult::Panicked | PublicEventCallbackResult::TimelineTerminated
            ) {
                handler.active = false;
                removed = true;
            } else if handler.active {
                handler.callback = outcome.callback.take();
            }
        }
        if removed {
            self.recompute_observer_interest();
        }
        outcome.result
    }

    #[doc(hidden)]
    pub fn end_public_dispatch(&mut self, dispatch: PublicEventDispatch) {
        let dispatched = dispatch.event_count.min(self.public_pending.len());
        self.public_pending.drain(..dispatched);
        self.handlers.retain(|handler| handler.active);
    }

    #[doc(hidden)]
    #[must_use]
    pub fn public_queue_capacity(&self) -> usize {
        self.public_pending.capacity()
    }

    #[doc(hidden)]
    #[must_use]
    pub fn public_queue_len(&self) -> usize {
        self.public_pending.len()
    }

    pub fn clear_session(&mut self) {
        self.close_fight();
        self.interceptor = None;
        self.handlers = Vec::new();
        self.public_pending = Vec::new();
        self.requested_queue_capacity = DEFAULT_PUBLIC_EVENT_CAPACITY;
        self.queue_limit = 0;
        self.frozen = true;
        self.interest = EventInterest::default();
        self.observer_interest = EventInterest::default();
        self.fault = None;
    }

    fn allocate_token(&mut self) -> EventToken {
        let token = EventToken::from_raw(self.next_token);
        self.next_token = self.next_token.wrapping_add(1);
        if self.next_token == 0 {
            self.next_token = 1;
        }
        token
    }

    fn call_interceptor(&mut self, call: impl FnOnce(&mut dyn InternalEventInterceptor)) {
        let Some(interceptor) = self.interceptor.as_mut() else {
            return;
        };
        if panic::catch_unwind(AssertUnwindSafe(|| call(interceptor.as_mut()))).is_err() {
            self.latch_fault(EventBridgeFault::InterceptorPanic);
        }
    }

    fn latch_fault(&mut self, fault: EventBridgeFault) {
        if self.fault.is_none() {
            self.fault = Some(fault);
        }
    }

    fn enqueue_public(&mut self, event: GameEvent) {
        if self.public_pending.len() < self.queue_limit {
            self.public_pending.push(event);
        } else {
            self.public_overflowed = true;
        }
    }

    fn clear_lifetime(&mut self, lifetime: EventLifetime) {
        for handler in &mut self.handlers {
            if handler.options.lifetime == lifetime {
                handler.active = false;
                handler.callback = None;
            }
        }
        self.handlers.retain(|handler| handler.active);
        self.recompute_observer_interest();
    }

    fn recompute_interest(&mut self) {
        self.recompute_observer_interest();
        self.interest = self.interceptor.as_ref().map_or(self.observer_interest, |interceptor| {
            self.observer_interest.union(interceptor.interest())
        });
    }

    fn recompute_observer_interest(&mut self) {
        self.observer_interest = self
            .handlers
            .iter()
            .fold(EventInterest::default(), |interest, handler| {
                if handler.active {
                    interest.union(handler.interest)
                } else {
                    interest
                }
            });
    }
}

fn event_interest(event: GameEvent) -> EventInterest {
    match event {
        GameEvent::PlantEffect(event) => event.source.interest(),
        GameEvent::HomeEntry(_) => EventInterest::HOME_ENTRY,
        _ => EventInterest::default(),
    }
}

impl Default for EventDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use rsvz_model::model::{Grid, PlantEffectSource, PlantId, PlantKind, ZombieId, ZombieKind};

    use super::*;

    fn attempt(main_counter: i32) -> PlantEffectAttemptFact {
        PlantEffectAttemptFact {
            source: PlantEffectSource::Bite(ZombieId::from_raw(1)),
            plant_id: PlantId::from_raw(2),
            raw_kind: PlantKind::WallNut,
            effective_kind: PlantKind::WallNut,
            grid: Grid::new(0, 0).expect("grid"),
            hp_before: 100,
            max_hp: 4_000,
            effect: rsvz_model::model::PlantEffect::HpDamage { native_requested: 4 },
            main_counter,
        }
    }

    struct Probe {
        log: Rc<RefCell<Vec<&'static str>>>,
        suppress: bool,
    }

    impl InternalEventInterceptor for Probe {
        fn interest(&self) -> EventInterest {
            EventInterest::BITE
        }

        fn begin_logic_frame(&mut self, _: u64, _: i32) {
            self.log.borrow_mut().push("frame-begin");
        }

        fn begin_plant_effect(&mut self, _: PlantEffectAttemptFact) -> EventDecision {
            self.log.borrow_mut().push("effect-begin");
            if self.suppress {
                EventDecision::SuppressByMeasurement
            } else {
                EventDecision::Apply
            }
        }

        fn finish_plant_effect(&mut self, _: EffectOutcomeFact) {
            self.log.borrow_mut().push("effect-finish");
        }

        fn end_logic_frame(&mut self, _: EventFrameStatus) {
            self.log.borrow_mut().push("frame-end");
        }
    }

    #[test]
    fn freezes_interest_and_pairs_one_effect() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut events = EventDispatcher::new();
        let checkpoint = events.begin_script_generation();
        events
            .install_internal_interceptor(Box::new(Probe {
                log: Rc::clone(&log),
                suppress: true,
            }))
            .expect("interceptor");
        assert_eq!(events.freeze(), Ok(EventInterest::BITE));
        assert_eq!(
            events.install_internal_interceptor(Box::new(Probe {
                log: Rc::clone(&log),
                suppress: false,
            })),
            Err(EventRegistrationError::Frozen)
        );
        events.begin_logic_frame(7, 100);
        let begin = events.begin_plant_effect(attempt(100));
        assert_eq!(begin.decision, EventDecision::SuppressByMeasurement);
        events.finish_plant_effect(begin.token, PlantEffectOutcome::SuppressedByMeasurement);
        events.end_logic_frame(EventFrameStatus::Running);
        assert_eq!(events.take_fault(), None);
        assert_eq!(
            log.borrow().as_slice(),
            ["frame-begin", "effect-begin", "effect-finish", "frame-end"]
        );

        events.rollback_to(checkpoint);
        assert_eq!(events.interest(), EventInterest::default());
    }

    #[test]
    fn enforces_lifo_frame_and_decision_protocol() {
        let mut events = EventDispatcher::new();
        events.freeze().expect("freeze");
        events.begin_logic_frame(1, 5);
        let first = events.begin_plant_effect(attempt(5));
        let second = events.begin_plant_effect(attempt(5));
        events.finish_plant_effect(first.token, PlantEffectOutcome::HpDelta { applied: 4 });
        assert_eq!(events.take_fault(), Some(EventBridgeFault::TokenMismatch));
        events.finish_plant_effect(second.token, PlantEffectOutcome::HpDelta { applied: 4 });
        events.finish_plant_effect(first.token, PlantEffectOutcome::HpDelta { applied: 4 });
        events.end_logic_frame(EventFrameStatus::Running);
    }

    #[test]
    fn overflow_and_interceptor_panic_fail_open() {
        struct PanicProbe;
        impl InternalEventInterceptor for PanicProbe {
            fn interest(&self) -> EventInterest {
                EventInterest::BITE
            }
            fn begin_plant_effect(&mut self, _: PlantEffectAttemptFact) -> EventDecision {
                panic!("probe")
            }
        }

        let mut events = EventDispatcher::new();
        events.begin_script_generation();
        events
            .install_internal_interceptor(Box::new(PanicProbe))
            .expect("interceptor");
        events.freeze().expect("freeze");
        events.begin_logic_frame(1, 5);
        let begin = events.begin_plant_effect(attempt(5));
        assert_eq!(begin.decision, EventDecision::Apply);
        assert_eq!(events.begin_plant_effect(attempt(5)), BeginPlantEffect::FAIL_OPEN);
        assert_eq!(events.take_fault(), Some(EventBridgeFault::InterceptorPanic));
        events.finish_plant_effect(begin.token, PlantEffectOutcome::HpDelta { applied: 4 });

        let mut overflow = EventDispatcher::new();
        overflow.freeze().expect("freeze");
        overflow.begin_logic_frame(1, 5);
        for _ in 0..MAX_PENDING_EFFECTS {
            assert!(overflow.begin_plant_effect(attempt(5)).token.is_valid());
        }
        assert_eq!(overflow.begin_plant_effect(attempt(5)), BeginPlantEffect::FAIL_OPEN);
        assert_eq!(overflow.take_fault(), Some(EventBridgeFault::PendingOverflow));
    }

    #[test]
    fn bridge_fault_stays_latched_and_skips_frame_commit() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut events = EventDispatcher::new();
        events.begin_script_generation();
        events
            .install_internal_interceptor(Box::new(Probe {
                log: Rc::clone(&log),
                suppress: false,
            }))
            .expect("interceptor");
        events.freeze().expect("freeze");
        events.begin_logic_frame(1, 5);
        let _unfinished = events.begin_plant_effect(attempt(5));
        events.end_logic_frame(EventFrameStatus::Running);

        assert_eq!(events.fault(), Some(EventBridgeFault::PendingAtFrameEnd));
        assert_eq!(log.borrow().as_slice(), ["frame-begin", "effect-begin"]);
        events.begin_logic_frame(1, 6);
        assert_eq!(events.begin_plant_effect(attempt(6)), BeginPlantEffect::FAIL_OPEN);
        assert_eq!(events.take_fault(), Some(EventBridgeFault::PendingAtFrameEnd));
    }

    #[test]
    fn accepts_the_full_pending_stack_in_lifo_order() {
        let mut events = EventDispatcher::new();
        events.freeze().expect("freeze");
        events.begin_logic_frame(1, 5);
        let tokens = std::array::from_fn::<_, MAX_PENDING_EFFECTS, _>(|_| events.begin_plant_effect(attempt(5)).token);
        for token in tokens.into_iter().rev() {
            events.finish_plant_effect(token, PlantEffectOutcome::HpDelta { applied: 4 });
        }
        events.end_logic_frame(EventFrameStatus::Running);
        assert_eq!(events.take_fault(), None);
    }

    #[test]
    fn rejects_frame_counter_decision_and_stale_token_mismatches() {
        let mut outside = EventDispatcher::new();
        assert_eq!(outside.begin_plant_effect(attempt(5)), BeginPlantEffect::FAIL_OPEN);
        assert_eq!(outside.take_fault(), Some(EventBridgeFault::EffectOutsideFrame));

        let mut counter = EventDispatcher::new();
        counter.begin_logic_frame(1, 5);
        assert_eq!(counter.begin_plant_effect(attempt(6)), BeginPlantEffect::FAIL_OPEN);
        assert_eq!(counter.take_fault(), Some(EventBridgeFault::MainCounterMismatch));

        let mut decision = EventDispatcher::new();
        decision.begin_logic_frame(1, 5);
        let token = decision.begin_plant_effect(attempt(5)).token;
        decision.finish_plant_effect(token, PlantEffectOutcome::SuppressedByMeasurement);
        assert_eq!(decision.take_fault(), Some(EventBridgeFault::DecisionOutcomeMismatch));

        let mut stale = EventDispatcher::new();
        stale.begin_logic_frame(1, 5);
        let token = stale.begin_plant_effect(attempt(5)).token;
        stale.end_logic_frame(EventFrameStatus::Running);
        assert_eq!(stale.take_fault(), Some(EventBridgeFault::PendingAtFrameEnd));
        stale.begin_logic_frame(1, 6);
        stale.finish_plant_effect(token, PlantEffectOutcome::HpDelta { applied: 4 });
        assert_eq!(stale.take_fault(), Some(EventBridgeFault::TokenMismatch));
    }

    #[test]
    fn abnormal_close_discards_pending_effects() {
        let mut events = EventDispatcher::new();
        events.begin_logic_frame(1, 5);
        let _pending = events.begin_plant_effect(attempt(5));
        events.close_fight();
        assert_eq!(events.take_fault(), Some(EventBridgeFault::PendingAtFrameEnd));

        events.begin_logic_frame(2, 6);
        let next = events.begin_plant_effect(attempt(6));
        events.finish_plant_effect(next.token, PlantEffectOutcome::HpDelta { applied: 4 });
        events.end_logic_frame(EventFrameStatus::Running);
        assert_eq!(events.take_fault(), None);
    }

    fn dispatch_public(events: &mut EventDispatcher) -> usize {
        let Some(mut dispatch) = events.begin_public_dispatch() else {
            return 0;
        };
        let mut panics = 0;
        while let Some(callback) = events.take_next_public_callback(&mut dispatch) {
            let outcome = EventDispatcher::run_public_callback_catching(callback);
            panics += usize::from(events.finish_public_callback(outcome) == PublicEventCallbackResult::Panicked);
        }
        events.end_public_dispatch(dispatch);
        panics
    }

    #[test]
    fn public_events_use_one_fixed_queue_and_stable_handler_order() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let mut events = EventDispatcher::new();
        assert_eq!(events.public_queue_capacity(), 0);
        events.begin_script_generation();
        events.reserve_public_events(300).expect("reserve request");

        let early_log = Rc::clone(&log);
        events
            .register_public_handler(
                EventInterest::BITE,
                EventOptions::new().order(-1),
                Box::new(move |_| early_log.borrow_mut().push("bite-early")),
            )
            .expect("bite observer");
        let all_log = Rc::clone(&log);
        events
            .register_public_handler(
                EventInterest::PLANT_EFFECT.union(EventInterest::HOME_ENTRY),
                EventOptions::new(),
                Box::new(move |event| match event {
                    GameEvent::PlantEffect(_) => all_log.borrow_mut().push("all-plant"),
                    GameEvent::HomeEntry(_) => all_log.borrow_mut().push("all-home"),
                    _ => {}
                }),
            )
            .expect("all observer");
        assert_eq!(
            events.freeze().expect("freeze"),
            EventInterest::PLANT_EFFECT.union(EventInterest::HOME_ENTRY)
        );
        assert!(events.public_queue_capacity() >= 300);

        events.begin_logic_frame(1, 5);
        let effect = events.begin_plant_effect(attempt(5));
        events.finish_plant_effect(effect.token, PlantEffectOutcome::HpDelta { applied: 4 });
        events.emit_home_entry(HomeEntryFact {
            zombie_id: ZombieId::from_raw(9),
            zombie_kind: ZombieKind::Normal,
            row: 0,
            main_counter: 5,
        });
        events.end_logic_frame(EventFrameStatus::Running);

        assert_eq!(events.public_queue_len(), 2);
        assert_eq!(dispatch_public(&mut events), 0);
        assert_eq!(log.borrow().as_slice(), ["bite-early", "all-plant", "all-home"]);
        assert_eq!(events.public_queue_len(), 0);
    }

    #[test]
    fn public_callback_panic_removes_only_that_handler() {
        let calls = Rc::new(Cell::new(0));
        let mut events = EventDispatcher::new();
        events.begin_script_generation();
        let callback_calls = Rc::clone(&calls);
        events
            .register_public_handler(
                EventInterest::BITE,
                EventOptions::new(),
                Box::new(move |_| {
                    callback_calls.set(callback_calls.get() + 1);
                    panic!("observer probe");
                }),
            )
            .expect("observer");
        events.freeze().expect("freeze");
        events.begin_logic_frame(1, 5);
        for _ in 0..2 {
            let effect = events.begin_plant_effect(attempt(5));
            events.finish_plant_effect(effect.token, PlantEffectOutcome::HpDelta { applied: 4 });
        }
        events.end_logic_frame(EventFrameStatus::Running);

        assert_eq!(dispatch_public(&mut events), 1);
        assert_eq!(calls.get(), 1);

        events.begin_logic_frame(2, 6);
        let effect = events.begin_plant_effect(attempt(6));
        events.finish_plant_effect(effect.token, PlantEffectOutcome::HpDelta { applied: 4 });
        events.end_logic_frame(EventFrameStatus::Running);
        assert_eq!(events.public_queue_len(), 0);
    }

    #[test]
    fn public_observer_lifetimes_clear_at_their_existing_boundaries() {
        let session_calls = Rc::new(Cell::new(0));
        let script_calls = Rc::new(Cell::new(0));
        let fight_calls = Rc::new(Cell::new(0));
        let mut events = EventDispatcher::new();
        events.begin_script_generation();
        for (lifetime, calls) in [
            (EventLifetime::Session, Rc::clone(&session_calls)),
            (EventLifetime::Script, Rc::clone(&script_calls)),
            (EventLifetime::Fight, Rc::clone(&fight_calls)),
        ] {
            events
                .register_public_handler(
                    EventInterest::HOME_ENTRY,
                    EventOptions::new().lifetime(lifetime),
                    Box::new(move |_| calls.set(calls.get() + 1)),
                )
                .expect("observer");
        }
        events.freeze().expect("freeze");
        events.close_fight();
        events.begin_logic_frame(1, 5);
        events.emit_home_entry(HomeEntryFact {
            zombie_id: ZombieId::from_raw(9),
            zombie_kind: ZombieKind::Normal,
            row: 0,
            main_counter: 5,
        });
        events.end_logic_frame(EventFrameStatus::Running);
        dispatch_public(&mut events);
        assert_eq!((session_calls.get(), script_calls.get(), fight_calls.get()), (1, 1, 0));

        events.begin_script_generation();
        events.freeze().expect("next generation");
        events.begin_logic_frame(2, 6);
        events.emit_home_entry(HomeEntryFact {
            zombie_id: ZombieId::from_raw(10),
            zombie_kind: ZombieKind::Normal,
            row: 1,
            main_counter: 6,
        });
        events.end_logic_frame(EventFrameStatus::Running);
        dispatch_public(&mut events);
        assert_eq!((session_calls.get(), script_calls.get(), fight_calls.get()), (2, 1, 0));

        events.clear_session();
        assert_eq!(events.public_queue_capacity(), 0);
    }

    #[test]
    fn failed_generation_rolls_back_script_observers_and_reserve_request() {
        let calls = Rc::new(Cell::new(0));
        let mut events = EventDispatcher::new();
        events.begin_script_generation();
        let session_calls = Rc::clone(&calls);
        events
            .register_public_handler(
                EventInterest::HOME_ENTRY,
                EventOptions::new().lifetime(EventLifetime::Session),
                Box::new(move |_| session_calls.set(session_calls.get() + 1)),
            )
            .expect("session observer");
        events.freeze().expect("first freeze");

        let checkpoint = events.begin_script_generation();
        events.reserve_public_events(1_024).expect("temporary reserve");
        events
            .register_public_handler(
                EventInterest::HOME_ENTRY,
                EventOptions::new(),
                Box::new(|_| panic!("stale")),
            )
            .expect("temporary observer");
        events.rollback_to(checkpoint);
        events.begin_logic_frame(1, 5);
        events.emit_home_entry(HomeEntryFact {
            zombie_id: ZombieId::from_raw(9),
            zombie_kind: ZombieKind::Normal,
            row: 0,
            main_counter: 5,
        });
        events.end_logic_frame(EventFrameStatus::Running);

        assert_eq!(dispatch_public(&mut events), 0);
        assert_eq!(calls.get(), 1);
        assert!(events.public_queue_capacity() < 1_024);
    }

    #[test]
    fn impossible_public_capacity_returns_a_typed_registration_error() {
        let mut events = EventDispatcher::new();
        let checkpoint = events.begin_script_generation();
        events
            .register_public_handler(EventInterest::BITE, EventOptions::new(), Box::new(|_| {}))
            .expect("observer");
        events.reserve_public_events(usize::MAX).expect("reserve request");

        assert_eq!(events.freeze(), Err(EventRegistrationError::QueueAllocation));
        events.rollback_to(checkpoint);
        assert!(events.is_frozen());
        assert_eq!(events.interest(), EventInterest::default());
        assert_eq!(events.public_queue_capacity(), 0);
    }

    #[test]
    fn full_public_queue_drops_new_events_without_growing() {
        let mut events = EventDispatcher::new();
        events.begin_script_generation();
        events
            .register_public_handler(EventInterest::BITE, EventOptions::new(), Box::new(|_| {}))
            .expect("observer");
        events.freeze().expect("freeze");
        let capacity = events.public_queue_capacity();
        events.begin_logic_frame(1, 5);
        for _ in 0..=DEFAULT_PUBLIC_EVENT_CAPACITY {
            let effect = events.begin_plant_effect(attempt(5));
            events.finish_plant_effect(effect.token, PlantEffectOutcome::HpDelta { applied: 4 });
        }
        events.end_logic_frame(EventFrameStatus::Running);

        assert_eq!(events.public_queue_len(), DEFAULT_PUBLIC_EVENT_CAPACITY);
        assert!(events.take_public_overflow());
        assert!(!events.take_public_overflow());
        assert_eq!(events.public_queue_capacity(), capacity);
    }

    #[test]
    fn approved_default_queue_size_is_fifteen_kibibytes() {
        assert_eq!(std::mem::size_of::<GameEvent>(), 60);
        assert_eq!(DEFAULT_PUBLIC_EVENT_CAPACITY * std::mem::size_of::<GameEvent>(), 15_360);
    }
}

/// Structural result after callback state has been cleaned up.
#[doc(hidden)]
#[derive(Debug, PartialEq, Eq)]
pub enum PublicEventCallbackResult {
    Continue,
    Aborted(rsvz_backend_api::error::RuntimeError),
    Panicked,
    TimelineTerminated,
}
