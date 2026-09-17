//! Exact planted-component receipts used by timed cleanup.
use crate::runtime::{RuntimeError, RuntimeResult};
use rsvz_backend_api::{ImitatorMorphBackend, PlantReadBackend, PlantRemoveBackend};
use rsvz_model::PlantId;
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default)]
pub struct RetentionState {
    pub main: Option<PlantId>,
    pub container: Option<PlantId>,
}

pub fn resolve_imitator_successor(state: &Cell<RetentionState>) -> RuntimeResult<()>
where
    rsvz_current::CurrentBackend: ImitatorMorphBackend,
{
    let Some(placeholder) = state.get().main else {
        return Ok(());
    };
    let (successor, placeholder_is_live) = crate::access::with_backend(|backend| {
        let successor = crate::live_value::read_or_abort(
            backend.imitator_morph_successor(placeholder),
            "imitator_morph_successor",
        );
        let live =
            successor.is_none() && crate::live_value::read_or_abort(backend.plant(placeholder), "plant").is_some();
        (successor, live)
    });
    apply_successor(state, placeholder, successor, placeholder_is_live)
}

fn apply_successor(
    state: &Cell<RetentionState>, placeholder: PlantId, successor: Option<PlantId>, placeholder_is_live: bool,
) -> RuntimeResult<()> {
    if let Some(successor) = successor
        && state.get().main == Some(placeholder)
    {
        let mut current = state.get();
        current.main = Some(successor);
        state.set(current);
    } else if placeholder_is_live {
        return Err(RuntimeError::new(
            "imitator placeholder did not morph at the native +320 boundary",
        ));
    } else if state.get().main == Some(placeholder) {
        let mut current = state.get();
        current.main = None;
        state.set(current);
    }
    Ok(())
}

pub fn cleanup_retained(state: &Cell<RetentionState>) -> RuntimeResult<()>
where
    rsvz_current::CurrentBackend: PlantRemoveBackend,
{
    let state = state.replace(RetentionState::default());
    if let Some(id) = state.main {
        crate::modifier::remove_plant_by_id(id)?;
    }
    if let Some(id) = state.container {
        crate::modifier::remove_plant_by_id(id)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_morph_lineage_and_missing_placeholder_preserve_container() {
        let old = PlantId::from_raw(1);
        let new = PlantId::from_raw(2);
        let container = PlantId::from_raw(3);
        let state = Cell::new(RetentionState {
            main: Some(old),
            container: Some(container),
        });
        assert!(apply_successor(&state, old, None, true).is_err());
        assert_eq!(state.get().main, Some(old));
        apply_successor(&state, old, Some(new), false).unwrap();
        assert_eq!(state.get().main, Some(new));
        assert_eq!(state.get().container, Some(container));
        apply_successor(&state, old, None, false).unwrap();
        assert_eq!(state.get().main, Some(new));
        apply_successor(&state, new, None, false).unwrap();
        assert_eq!(state.get().main, None);
        assert_eq!(state.get().container, Some(container));
    }
}
