include!("support.rs");

fn install_hooks() {}

#[rsvz_macros::script(state_hooks = install_hooks)]
fn script() {}

fn main() {}
