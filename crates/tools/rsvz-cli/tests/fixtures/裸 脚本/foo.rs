mod sibling;

#[rsvz::script]
fn script() {
    let _ = sibling::VALUE;
}
