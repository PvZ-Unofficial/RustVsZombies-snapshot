include!("support.rs");

use rsvz_macros::{script, state_hooks};

#[state_hooks]
fn first_hooks() {}

#[state_hooks]
fn second_hooks() {}

#[script]
fn script() {}

fn main() {}
