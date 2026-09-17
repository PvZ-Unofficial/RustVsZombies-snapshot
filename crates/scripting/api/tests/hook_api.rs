use rsvz::state_hook::*;
use rsvz_schedule::state_hook::{StateHookRegistry, with_state_hooks};
use std::cell::Cell;
use std::rc::Rc;
#[test]
fn callable_overloads_register_default_and_explicit_order() {
    with_state_hooks(StateHookRegistry::clear);
    __run_state_hook_installer(|| {
        let _default = on_before_tick(|| {});
        let explicit = on_before_tick(-10, || {});
        assert_eq!(remove_state_hook(explicit), StateHookCommandOutcome::Applied);
        Ok(())
    })
    .expect("installer");
}

#[test]
fn ordinary_script_registration_records_session_hook_misuse() {
    with_state_hooks(StateHookRegistry::clear);
    let called = Rc::new(Cell::new(false));
    let called_by_hook = Rc::clone(&called);
    let handle = Rc::new(Cell::new(None));
    let handle_from_script = Rc::clone(&handle);
    let error = rsvz::__run_script(|| {
        handle_from_script.set(Some(on_before_tick(move || {
            called_by_hook.set(true);
        })));
        Ok(())
    })
    .expect_err("misuse must fail registration");
    assert!(error.message().contains("#[rsvz::state_hooks]"));
    assert_eq!(
        with_state_hooks(|hooks| hooks.dispatch_catching(StateEvent::BeforeTick)),
        StateHookDispatchResult::Continue
    );
    assert!(!called.get());
    assert_eq!(
        with_state_hooks(|hooks| hooks.remove(handle.get().expect("misused handle"))),
        StateHookCommandOutcome::AlreadyRemoved
    );
}

#[test]
#[cfg(any(feature = "pvz-emulator", feature = "pvz-1-0-0-1051", feature = "pvz-portable"))]
fn ordinary_tick_callback_receives_meta_and_can_stop_itself() {
    use rsvz::tick::{TickControl, TickMeta, TickOptions, TickPhase, TickTaskState};
    rsvz::with_scheduler(|scheduler| scheduler.clear_all());
    let called = Rc::new(Cell::new(false));
    let recorded = called.clone();
    let handle = rsvz::tick::spawn(TickOptions::any_dispatch(), move |meta| {
        assert_eq!(meta.phase, TickPhase::Menu);
        recorded.set(true);
        Ok(TickControl::Stop)
    });
    let result = rsvz_game::tick::dispatch_scheduler_tick(TickMeta {
        phase: TickPhase::Menu,
        game_ui: Some(rsvz_model::GameUi::Menu),
        clock: None,
        is_new_frame: false,
    });
    assert_eq!(result, rsvz_schedule::tick::TickDispatchResult::Continue);
    assert!(called.get());
    assert_eq!(
        rsvz::with_scheduler(|scheduler| scheduler.state(handle)),
        TickTaskState::Stopped
    );
}
