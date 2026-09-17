#![cfg(any(feature = "pvz-emulator", feature = "pvz-1-0-0-1051", feature = "pvz-portable"))]

use rsvz::prelude::*;

#[rsvz::state_hooks]
fn install_hooks() {
    on_after_attach(|| set_auto_enter(false));
}

#[rsvz::script]
fn script() {}

#[test]
fn separate_installer_runs_before_after_attach() {
    rsvz_game::lifecycle::reset_session_resources();
    rsvz_game::session::reset_session_control();
    rsvz_game::state_hook::clear_state_hooks();

    __rsvz_install_state_hooks().expect("install state hooks");
    assert!(rsvz_game::setup::auto_enter_enabled());
    rsvz_game::lifecycle::finish_hook_installation().expect("dispatch AfterAttach");
    assert!(!rsvz_game::setup::auto_enter_enabled());
}
