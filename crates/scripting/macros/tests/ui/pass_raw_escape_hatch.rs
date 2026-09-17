include!("support.rs");

use rsvz_macros::script;

#[script]
fn script() {
    assume_wavelength(1, 601);
}

fn main() {}
