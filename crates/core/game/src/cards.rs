//! Card reporting and current operations.
use crate::logic::cards::CardResult;
use crate::runtime::RuntimeError;
use rsvz_model::PlantId;

#[doc(hidden)]
pub fn report_card(result: CardResult<PlantId>) -> Option<PlantId> {
    match result {
        Ok(plant) => Some(plant),
        Err(error) => {
            crate::diagnostics::report_operation_error(error);
            None
        }
    }
}

#[doc(hidden)]
pub fn report_cards(result: CardResult<Vec<Option<PlantId>>>) -> Vec<Option<PlantId>> {
    match result {
        Ok(plants) => {
            for (index, plant) in plants.iter().enumerate() {
                if plant.is_none() {
                    crate::diagnostics::report_operation_error(RuntimeError::new(format!(
                        "第 {} 项种卡失败：没有种下植物",
                        index + 1
                    )));
                }
            }
            plants
        }
        Err(error) => {
            crate::diagnostics::report_operation_error(error);
            Vec::new()
        }
    }
}

fn cooldown_value(
    selection: rsvz_model::CardSelection, cooldown: Option<i32>, registration: bool,
) -> crate::runtime::RuntimeResult<i32> {
    match cooldown {
        Some(value) => Ok(value),
        None if registration => Ok(0),
        None => Err(RuntimeError::new(format!(
            "card cooldown is unavailable for {selection:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn card_cd_uses_live_value_and_treats_missing_registration_state_as_ready() {
        let selection = rsvz_model::CardSelection::Plant(rsvz_model::PlantKind::IceShroom);
        assert_eq!(cooldown_value(selection, Some(123), false).unwrap(), 123);
        assert!(cooldown_value(selection, None, false).is_err());
        assert_eq!(cooldown_value(selection, None, true).unwrap(), 0);
    }
}

mod current;
pub use current::*;

#[doc(hidden)]
pub mod effect;
