//! Delayed, backend-neutral game event observers.
//!
//! Native hooks only record resolved events. User callbacks run safely at the
//! start of the next logic dispatch, before `BeforeTick`, and cannot suppress
//! the native effect.
//!
//! The shorthand below requires a selected game backend.
//!
//! ```rust,ignore
//! use rsvz::event::on_gargantuar_smash;
//! use rsvz::prelude::*;
//!
//! let _observer = on_gargantuar_smash(|event| {
//!     let _plant = card(PlantKind::CherryBomb, event.grid.row + 1, event.grid.col + 1);
//! })
//! .expect("event registration");
//! ```

pub use rsvz_model::model::{
    EventDecisionOrigin, GameEvent, HomeEntryEvent, PlantEffect, PlantEffectEvent, PlantEffectOutcome,
    PlantEffectSource,
};
pub use rsvz_schedule::event::{EventCommandOutcome, EventHandle, EventLifetime, EventOptions, EventRegistrationError};

crate::callable::callable_api! {
    /// Observes every resolved game event in native completion order.
    pub on: OnEvent;
    where {}
    impl<F> where { F: FnMut(&GameEvent) + 'static, }
    call(callback: F) -> Result<EventHandle, EventRegistrationError> { rsvz_game::event::on(EventOptions::new(), callback) }
    impl<F> where { F: FnMut(&GameEvent) + 'static, }
    call(options: EventOptions, callback: F) -> Result<EventHandle, EventRegistrationError> { rsvz_game::event::on(options, callback) }
}

crate::callable::callable_api! {
    /// Observes every completed plant effect.
    pub on_plant_effect: OnPlantEffect;
    where {}
    impl<F> where { F: FnMut(&PlantEffectEvent) + 'static, }
    call(callback: F) -> Result<EventHandle, EventRegistrationError> { rsvz_game::event::on_plant_effect(EventOptions::new(), callback) }
    impl<F> where { F: FnMut(&PlantEffectEvent) + 'static, }
    call(options: EventOptions, callback: F) -> Result<EventHandle, EventRegistrationError> { rsvz_game::event::on_plant_effect(options, callback) }
}

crate::callable::callable_api! {
    /// Observes completed Gargantuar smash effects.
    pub on_gargantuar_smash: OnGargantuarSmash;
    where {}
    impl<F> where { F: FnMut(&PlantEffectEvent) + 'static, }
    call(callback: F) -> Result<EventHandle, EventRegistrationError> { rsvz_game::event::on_gargantuar_smash(EventOptions::new(), callback) }
    impl<F> where { F: FnMut(&PlantEffectEvent) + 'static, }
    call(options: EventOptions, callback: F) -> Result<EventHandle, EventRegistrationError> { rsvz_game::event::on_gargantuar_smash(options, callback) }
}

crate::callable::callable_api! {
    /// Observes zombies crossing the house boundary.
    pub on_home_entry: OnHomeEntry;
    where {}
    impl<F> where { F: FnMut(&HomeEntryEvent) + 'static, }
    call(callback: F) -> Result<EventHandle, EventRegistrationError> { rsvz_game::event::on_home_entry(EventOptions::new(), callback) }
    impl<F> where { F: FnMut(&HomeEntryEvent) + 'static, }
    call(options: EventOptions, callback: F) -> Result<EventHandle, EventRegistrationError> { rsvz_game::event::on_home_entry(options, callback) }
}

/// Removes a public event observer. Removal is allowed while callbacks run.
pub use rsvz_game::event::remove;

/// Raises the fixed public event queue capacity for this session.
pub use rsvz_game::event::reserve;
