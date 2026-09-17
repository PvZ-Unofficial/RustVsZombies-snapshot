use super::*;

#[test]
fn selection_validation_rejects_duplicates_invalid_imitators_and_overflow() {
    let ice = CardSelection::Plant(PlantKind::IceShroom);
    assert!(matches!(
        validate_card_selection(&[ice, ice]),
        Err(CoreLogicError::DuplicateCardSelection(_))
    ));
    assert!(matches!(
        validate_card_selection(&[
            CardSelection::Imitator(PlantKind::IceShroom),
            CardSelection::Imitator(PlantKind::DoomShroom)
        ]),
        Err(CoreLogicError::MultipleImitatorCards)
    ));
    assert!(matches!(
        validate_card_selection(&[CardSelection::Plant(PlantKind::Imitator)]),
        Err(CoreLogicError::InvalidCardSelection(_))
    ));
    assert!(matches!(
        validate_card_selection(&[ice; MAX_SEED_SLOTS + 1]),
        Err(CoreLogicError::TooManyCards { .. })
    ));
}

#[cfg(all(test, feature = "backend-tests"))]
#[test]
fn backend_failure_ends_the_callback_without_requiring_backend_for_formatting() {
    let error = CardLogicError::Backend(RuntimeError::new("native failure"));
    let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        card_error_at(CardSelection::Plant(PlantKind::IceShroom), 3, 9, &error)
    }))
    .expect_err("native failure must abort");
    let error = rsvz_schedule::callback::into_error(payload).expect("local abort payload");
    assert_eq!(
        error.to_string(),
        "种植寒冰菇到 (3, 9) 失败：底层卡片操作失败：native failure"
    );
}
