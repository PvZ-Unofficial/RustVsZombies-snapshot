include!("support.rs");

use rsvz_macros::script;

#[script]
fn script() {
    wave(1);
    359 << pp();
}

fn main() {}
