include!("support.rs");

use rsvz_macros::script;

#[script]
fn script() {
    zombies("普");
}

fn main() {
    let _ = script();
}
