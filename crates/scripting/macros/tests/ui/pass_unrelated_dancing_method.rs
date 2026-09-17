include!("support.rs");

struct Dancer;

impl Dancer {
    fn dancing(self) {}
}

#[rsvz_macros::script]
fn script() {
    Dancer.dancing();
}

fn main() {
    let _ = script();
}
