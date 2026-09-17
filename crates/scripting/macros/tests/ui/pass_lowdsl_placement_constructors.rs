include!("support.rs");

use rsvz_macros::script;

#[script]
fn script() {
    let _card = pot;
    399 << pot(to(2000), 4, 9);
}

fn main() {
    let _ = script();
}
