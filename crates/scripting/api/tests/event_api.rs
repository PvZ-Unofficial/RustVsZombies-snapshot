use rsvz::event::*;
use rsvz_model::{
    EventFrameStatus, Grid, HomeEntryFact, PlantEffectAttemptFact, PlantId, PlantKind, ZombieId, ZombieKind,
};
use rsvz_schedule::event::{EventDispatcher, with_events, with_events_ref};
use std::cell::Cell;
use std::rc::Rc;
fn dispatch_public() {
    let Some(mut dispatch) = with_events_ref(EventDispatcher::begin_public_dispatch) else {
        return;
    };
    loop {
        let callback = with_events(|events| events.take_next_public_callback(&mut dispatch));
        let Some(callback) = callback else {
            break;
        };
        let outcome = EventDispatcher::run_public_callback_catching(callback);
        assert_eq!(
            with_events(|events| events.finish_public_callback(outcome)),
            rsvz_schedule::event::PublicEventCallbackResult::Continue
        );
    }
    with_events(|events| events.end_public_dispatch(dispatch));
}

fn attempt(source: PlantEffectSource, main_counter: i32) -> PlantEffectAttemptFact {
    PlantEffectAttemptFact {
        source,
        plant_id: PlantId::from_raw(2),
        raw_kind: PlantKind::WallNut,
        effective_kind: PlantKind::WallNut,
        grid: Grid::new(0, 0).expect("grid"),
        hp_before: 100,
        max_hp: 4_000,
        effect: PlantEffect::HpDamage { native_requested: 4 },
        main_counter,
    }
}

#[test]
fn callable_observers_filter_events_and_remove_by_handle() {
    with_events(EventDispatcher::clear_session);
    assert!(matches!(on_home_entry(|_| {}), Err(EventRegistrationError::Frozen)));
    with_events(|events| {
        events.begin_script_generation();
    });

    let plant_calls = Rc::new(Cell::new(0));
    let plant_callback_calls = Rc::clone(&plant_calls);
    on_plant_effect(move |_| plant_callback_calls.set(plant_callback_calls.get() + 1)).expect("plant observer");
    let smash_calls = Rc::new(Cell::new(0));
    let smash_callback_calls = Rc::clone(&smash_calls);
    let smash = on_gargantuar_smash(EventOptions::new().lifetime(EventLifetime::Session), move |_| {
        smash_callback_calls.set(smash_callback_calls.get() + 1)
    })
    .expect("smash observer");
    reserve(512).expect("reserve");
    with_events(EventDispatcher::freeze).expect("freeze observers");

    with_events(|events| {
        events.begin_logic_frame(1, 5);
        for source in [
            PlantEffectSource::Bite(ZombieId::from_raw(1)),
            PlantEffectSource::Gargantuar(ZombieId::from_raw(2)),
        ] {
            let effect = events.begin_plant_effect(attempt(source, 5));
            events.finish_plant_effect(effect.token, PlantEffectOutcome::HpDelta { applied: 4 });
        }
        events.emit_home_entry(HomeEntryFact {
            zombie_id: ZombieId::from_raw(3),
            zombie_kind: ZombieKind::Normal,
            row: 0,
            main_counter: 5,
        });
        events.end_logic_frame(EventFrameStatus::Running);
    });
    dispatch_public();

    assert_eq!(plant_calls.get(), 2);
    assert_eq!(smash_calls.get(), 1);
    assert_eq!(remove(smash), EventCommandOutcome::Applied);
    assert_eq!(remove(smash), EventCommandOutcome::AlreadyRemoved);
}

#[test]
fn aborted_observer_is_retained_without_resurrecting_a_removed_observer() {
    use rsvz_schedule::event::PublicEventCallbackResult;
    with_events(EventDispatcher::clear_session);
    with_events(EventDispatcher::begin_script_generation);
    let runs = Rc::new(Cell::new(0));
    let repeated_runs = Rc::clone(&runs);
    let retained = on_home_entry(move |_| {
        repeated_runs.set(repeated_runs.get() + 1);
        rsvz_game::diagnostics::abort_operation(rsvz::RuntimeError::new("observer abort"));
    })
    .unwrap();
    let removed_slot = Rc::new(Cell::new(None));
    let self_slot = Rc::clone(&removed_slot);
    removed_slot.set(Some(
        on_home_entry(move |_| {
            remove(self_slot.get().unwrap());
            rsvz_game::diagnostics::abort_operation(rsvz::RuntimeError::new("removed observer"));
        })
        .unwrap(),
    ));
    let later_runs = Rc::new(Cell::new(0));
    let later = Rc::clone(&later_runs);
    on_home_entry(move |_| later.set(later.get() + 1)).unwrap();
    with_events(EventDispatcher::freeze).unwrap();
    let mut errors = Vec::new();
    for clock in [1, 2] {
        with_events(|events| {
            events.begin_logic_frame(1, clock);
            events.emit_home_entry(HomeEntryFact {
                zombie_id: ZombieId::from_raw(1),
                zombie_kind: ZombieKind::Normal,
                row: 0,
                main_counter: clock,
            });
            events.end_logic_frame(EventFrameStatus::Running);
        });
        let mut dispatch = with_events_ref(EventDispatcher::begin_public_dispatch).unwrap();
        while let Some(run) = with_events(|events| events.take_next_public_callback(&mut dispatch)) {
            let outcome = EventDispatcher::run_public_callback_catching(run);
            match with_events(|events| events.finish_public_callback(outcome)) {
                PublicEventCallbackResult::Continue => {}
                PublicEventCallbackResult::Aborted(error) => errors.push(error.to_string()),
                result => panic!("unexpected event failure: {result:?}"),
            }
        }
        with_events(|events| events.end_public_dispatch(dispatch));
    }
    assert_eq!(runs.get(), 2);
    assert_eq!(later_runs.get(), 2);
    assert_eq!(errors, ["observer abort", "removed observer", "observer abort"]);
    assert_eq!(remove(retained), EventCommandOutcome::Applied);
    assert_eq!(remove(removed_slot.get().unwrap()), EventCommandOutcome::NotFound);
    with_events(EventDispatcher::clear_session);
}
