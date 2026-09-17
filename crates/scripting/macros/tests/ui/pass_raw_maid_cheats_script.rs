include!("support.rs");

#[rsvz_macros::script]
fn script() {
    wave(1);
    100 << dancing();
}

fn main() {
    let _ = script();
}
