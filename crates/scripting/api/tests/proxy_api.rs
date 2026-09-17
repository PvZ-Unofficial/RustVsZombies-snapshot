#[test]
fn live_value_missing_never_equals_concrete_value() {
    let missing = rsvz::LiveValue::<i32>::missing();

    assert!(!missing.is_live());
    assert_ne!(missing, 10);
    assert_eq!(missing.unwrap_or(20), 20);
}
