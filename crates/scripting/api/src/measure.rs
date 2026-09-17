//! Measurement declarations and script coordinate adapters.

use rsvz_game::logic::IntoGrid;
pub use rsvz_game::measure::{
    __current_setup, ExpectedPassesEnd, broad_pass_for, broad_pass_trials, completed_rounds, damage_narrow_for,
    damage_narrow_trials, end_at, expected_passes_for, expected_passes_for_with_end, expected_passes_trials,
    imp_leak_detection, imp_leak_threshold, pogo_for, pogo_trials, protect_add, protect_only, protect_remove,
    protect_unrepairable_from_cards, refresh_activate, refresh_cob_delay, refresh_dance, refresh_for, refresh_trials,
    smash_for, smash_trials, trace_plant_losses, trace_round_end_sun,
};
use rsvz_model::{Grid, PlantKind, ProtectTarget};

pub mod protect {
    use super::*;

    /// Protect the key plant layer at a script-facing 1-based grid.
    ///
    /// # Panics
    ///
    /// Panics when `row` or `col` is not a positive one-based coordinate.
    #[must_use]
    pub fn grid(row: i32, col: i32) -> ProtectTarget {
        ProtectTarget::grid(Grid::from_one_based(row, col).unwrap_or_else(|_error| {
            panic!("script-facing protected grid coordinates must be 1-based positive values")
        }))
    }

    /// Protect only the specified plant kind at a script-facing 1-based grid.
    ///
    /// # Panics
    ///
    /// Panics when `row` or `col` is not a positive one-based coordinate.
    #[must_use]
    pub fn plant(row: i32, col: i32, kind: PlantKind) -> ProtectTarget {
        ProtectTarget::plant(
            Grid::from_one_based(row, col).unwrap_or_else(|_error| {
                panic!("script-facing protected plant coordinates must be 1-based positive values")
            }),
            kind,
        )
    }
}

#[must_use]
pub fn target_grid(grid: impl IntoGrid) -> ProtectTarget {
    ProtectTarget::grid(grid.into_grid())
}

#[must_use]
pub fn target_plant(grid: impl IntoGrid, kind: PlantKind) -> ProtectTarget {
    ProtectTarget::plant(grid.into_grid(), kind)
}
