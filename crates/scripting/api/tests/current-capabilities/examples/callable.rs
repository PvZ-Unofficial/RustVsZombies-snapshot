#![feature(fn_traits, unboxed_closures, trivial_bounds)]
#![allow(incomplete_features, trivial_bounds, unused_imports, unused_macros)]

// Compile the real declaration macro without adding a public test-only API.
#[path = "../../../src/callable.rs"]
mod callable;

use rsvz::__private::CurrentBackend;
use rsvz_backend_api::KeyboardStateBackend;

callable::callable_api! {
    pub keyboard_binding: KeyboardBinding;
    where { CurrentBackend: KeyboardStateBackend, }
    impl<>
    where {}
    call() -> () {
        rsvz::key::on_press(rsvz::core::model::KeyCode::Q, || Ok(()));
    }
}

fn main() {
    let copy = keyboard_binding;
    #[cfg(feature = "invoke")]
    copy();
    let _ = copy;
}
