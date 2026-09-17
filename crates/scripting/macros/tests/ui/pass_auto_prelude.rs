include!("support.rs");

use rsvz_macros::script;

#[script]
fn script() {
    reload(MainUiOrFightUi);
    wave(1);
    let op = pp() + d(107);
    359 << &op;
}

fn main() {
    let _ = script();
}
