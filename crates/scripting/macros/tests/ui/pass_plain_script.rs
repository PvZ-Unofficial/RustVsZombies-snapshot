include!("support.rs");

use rsvz_macros::script;

#[script]
fn script() {}

fn main() {
    let _ = script();
}
