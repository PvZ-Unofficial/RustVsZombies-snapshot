include!("support.rs");

use rsvz_macros::script;

#[script]
fn script() -> rsvz::core::runtime::RuntimeResult<()> {
    Ok(())
}

fn main() {
    let _ = script();
}
