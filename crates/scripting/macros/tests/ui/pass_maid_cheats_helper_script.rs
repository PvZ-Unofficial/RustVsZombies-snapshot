include!("support.rs");

use rsvz::dsl::prelude::*;

fn start() {
    (-599) << dancing();
}

#[rsvz_macros::script]
fn script() {
    wave(1);
    start();
}

fn main() {
    let _ = script();
}
