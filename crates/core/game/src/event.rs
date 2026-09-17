//! event operations for the selected backend.

use crate::diagnostics::report_runtime_error;
use crate::runtime::RuntimeError;
use crate::session::{DispatchOutcome, fail_script, record_dispatch_outcome, script_stop_requested};
use rsvz_backend_api::backend::NativeEventSink;
use rsvz_schedule::event::{EventDispatcher, PublicEventDispatch, with_events, with_events_ref};
use std::cell::Cell;

thread_local! {
    pub(crate) static EVENT_SINK_INSTALLED: Cell<bool> = const { Cell::new(false) };
}

pub(crate) fn dispatch_public_events(overflowed: bool, mut dispatch: Option<PublicEventDispatch>) {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if overflowed {
            record_dispatch_outcome(DispatchOutcome::RecoverableError);
            report_runtime_error("public event queue overflowed; newest events were dropped");
        }
        let Some(dispatch) = dispatch.as_mut() else {
            return false;
        };
        loop {
            let callback = with_events(|events| events.take_next_public_callback(dispatch));
            let Some(callback) = callback else {
                break false;
            };
            let outcome = EventDispatcher::run_public_callback_catching(callback);
            match with_events(|events| events.finish_public_callback(outcome)) {
                rsvz_schedule::event::PublicEventCallbackResult::Continue => {}
                rsvz_schedule::event::PublicEventCallbackResult::Aborted(error) => {
                    record_dispatch_outcome(DispatchOutcome::RecoverableError);
                    report_runtime_error(error);
                }
                rsvz_schedule::event::PublicEventCallbackResult::Panicked => {
                    record_dispatch_outcome(DispatchOutcome::RecoverableError);
                    report_runtime_error("public event callback panicked and was removed");
                }
                rsvz_schedule::event::PublicEventCallbackResult::TimelineTerminated => break true,
            }
            if script_stop_requested() {
                break false;
            }
        }
    }));
    if let Some(dispatch) = dispatch {
        with_events(|events| events.end_public_dispatch(dispatch));
    }
    match result {
        Ok(true) => rsvz_schedule::timeline::terminate_timeline_callback(),
        Ok(false) => {}
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

pub(crate) fn native_event_sink(interest: rsvz_model::EventInterest) -> NativeEventSink {
    NativeEventSink {
        interest,
        begin_logic_frame: native_event_begin_logic_frame,
        begin_plant_effect: native_event_begin_plant_effect,
        finish_plant_effect: native_event_finish_plant_effect,
        emit_home_entry: native_event_emit_home_entry,
        emit_gargantuar_spawned: native_event_emit_gargantuar_spawned,
        emit_imp_thrown: native_event_emit_imp_thrown,
        emit_gargantuar_ash_hit: native_event_emit_gargantuar_ash_hit,
        end_logic_frame: native_event_end_logic_frame,
    }
}

fn native_event_begin_logic_frame(board_epoch: u64, main_counter: i32) {
    native_event_call((), || {
        with_events(|events| events.begin_logic_frame(board_epoch, main_counter));
    });
}

fn native_event_begin_plant_effect(fact: rsvz_model::PlantEffectAttemptFact) -> rsvz_model::BeginPlantEffect {
    native_event_call(rsvz_model::BeginPlantEffect::FAIL_OPEN, || {
        with_events(|events| events.begin_plant_effect(fact))
    })
}

fn native_event_finish_plant_effect(token: rsvz_model::EventToken, outcome: rsvz_model::PlantEffectOutcome) {
    native_event_call((), || {
        with_events(|events| events.finish_plant_effect(token, outcome));
    });
}

fn native_event_emit_home_entry(fact: rsvz_model::HomeEntryFact) {
    native_event_call((), || {
        with_events(|events| events.emit_home_entry(fact));
    });
}

fn native_event_emit_gargantuar_spawned(fact: rsvz_model::GargantuarSpawnedFact) {
    native_event_call((), || {
        with_events(|events| events.emit_gargantuar_spawned(fact));
    });
}

fn native_event_emit_imp_thrown(fact: rsvz_model::ImpThrownFact) {
    native_event_call((), || {
        with_events(|events| events.emit_imp_thrown(fact));
    });
}

fn native_event_emit_gargantuar_ash_hit(fact: rsvz_model::GargantuarAshHitFact) {
    native_event_call((), || {
        with_events(|events| events.emit_gargantuar_ash_hit(fact));
    });
}

fn native_event_end_logic_frame(status: rsvz_model::EventFrameStatus) {
    native_event_call((), || {
        with_events(|events| events.end_logic_frame(status));
    });
}

fn native_event_call<R: Copy>(fallback: R, call: impl FnOnce() -> R) -> R {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(call));
    let Ok(value) = result else {
        fail_script(RuntimeError::new("native event ingress panicked or reentered"));
        return fallback;
    };
    if let Some(fault) = with_events_ref(EventDispatcher::fault) {
        fail_script(RuntimeError::new(fault.to_string()));
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logical_reset_does_not_forget_the_physical_sink() {
        EVENT_SINK_INSTALLED.set(true);
        crate::frame::reset_runtime_state_preserving_backend();
        assert!(EVENT_SINK_INSTALLED.get());
        EVENT_SINK_INSTALLED.set(false);
    }
}

pub fn install_internal_event_interceptor(
    interceptor: Box<dyn rsvz_schedule::event::InternalEventInterceptor>,
) -> crate::runtime::RuntimeResult<()> {
    with_events(|events| events.install_internal_interceptor(interceptor))
        .map_err(|error| RuntimeError::new(error.to_string()))
}

use rsvz_model::{EventInterest, GameEvent, HomeEntryEvent, PlantEffectEvent, PlantEffectSource};
use rsvz_schedule::event::{EventCommandOutcome, EventHandle, EventOptions, EventRegistrationError};

pub fn on<F>(options: EventOptions, callback: F) -> Result<EventHandle, EventRegistrationError>
where
    F: FnMut(&GameEvent) + 'static,
{
    with_events(|events| {
        events.register_public_handler(
            EventInterest::PLANT_EFFECT.union(EventInterest::HOME_ENTRY),
            options,
            Box::new(callback),
        )
    })
}

pub fn on_plant_effect<F>(options: EventOptions, mut callback: F) -> Result<EventHandle, EventRegistrationError>
where
    F: FnMut(&PlantEffectEvent) + 'static,
{
    with_events(|events| {
        events.register_public_handler(
            EventInterest::PLANT_EFFECT,
            options,
            Box::new(move |event| {
                if let GameEvent::PlantEffect(event) = event {
                    callback(event);
                }
            }),
        )
    })
}

pub fn on_gargantuar_smash<F>(options: EventOptions, mut callback: F) -> Result<EventHandle, EventRegistrationError>
where
    F: FnMut(&PlantEffectEvent) + 'static,
{
    with_events(|events| {
        events.register_public_handler(
            EventInterest::GARGANTUAR,
            options,
            Box::new(move |event| {
                if let GameEvent::PlantEffect(event) = event
                    && matches!(event.source, PlantEffectSource::Gargantuar(_))
                {
                    callback(event);
                }
            }),
        )
    })
}

pub fn on_home_entry<F>(options: EventOptions, mut callback: F) -> Result<EventHandle, EventRegistrationError>
where
    F: FnMut(&HomeEntryEvent) + 'static,
{
    with_events(|events| {
        events.register_public_handler(
            EventInterest::HOME_ENTRY,
            options,
            Box::new(move |event| {
                if let GameEvent::HomeEntry(event) = event {
                    callback(event);
                }
            }),
        )
    })
}

pub fn remove(handle: EventHandle) -> EventCommandOutcome {
    with_events(|events| events.remove_public_handler(handle))
}
pub fn reserve(capacity: usize) -> Result<(), EventRegistrationError> {
    with_events(|events| events.reserve_public_events(capacity))
}
