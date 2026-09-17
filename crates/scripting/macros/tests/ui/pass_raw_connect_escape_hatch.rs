include!("support.rs");

use rsvz_macros::script;

#[script]
fn script() {
    skip_until((1, 100));
    rsvz::timeline::try_at(1, 100, || Ok(()))?;
}

fn main() {}
