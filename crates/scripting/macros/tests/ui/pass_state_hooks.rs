include!("support.rs");

use std::sync::atomic::{AtomicUsize, Ordering};

use rsvz_macros::{script, state_hooks};

static INSTALLS: AtomicUsize = AtomicUsize::new(0);

#[state_hooks]
fn install_hooks() {
    INSTALLS.fetch_add(1, Ordering::Relaxed);
}

#[script]
fn script() {}

fn main() {
    assert!(__rsvz_install_state_hooks().is_ok());
    assert_eq!(INSTALLS.load(Ordering::Relaxed), 1);
    let _ = script();
}
