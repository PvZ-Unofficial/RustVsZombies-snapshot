use crate::model::types::DEFAULT_COL_COUNT;
use crate::model::{Grid, SceneKind};

use super::cell::LineupCell;
use super::error::LineupParseError;

/// A validated board lineup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lineup {
    scene: SceneKind,
    cells: Vec<LineupCell>,
    rake_row: Option<i32>,
}

impl Lineup {
    /// Builds a validated lineup from cells.
    pub fn new(scene: SceneKind, cells: Vec<LineupCell>, rake_grid: Option<Grid>) -> Result<Self, LineupParseError> {
        Self::try_from_parts(scene, cells, rake_grid.map(|grid| grid.row))
    }

    pub(super) fn try_from_parts(
        scene: SceneKind, cells: Vec<LineupCell>, rake_row: Option<i32>,
    ) -> Result<Self, LineupParseError> {
        let lineup = Self { scene, cells, rake_row };
        lineup.validate()?;
        Ok(lineup)
    }

    /// Target scene encoded by this lineup.
    #[must_use]
    pub const fn scene(&self) -> SceneKind {
        self.scene
    }

    /// Validated cells in row-major order.
    #[must_use]
    pub fn cells(&self) -> &[LineupCell] {
        &self.cells
    }

    /// Validated cells paired with their implicit 0-based row-major grids.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (Grid, &LineupCell)> {
        self.cells.iter().enumerate().map(|(index, cell)| {
            (
                Grid {
                    row: (index / DEFAULT_COL_COUNT) as i32,
                    col: (index % DEFAULT_COL_COUNT) as i32,
                },
                cell,
            )
        })
    }

    /// Number of rows in the target board scene.
    #[must_use]
    pub const fn row_count(&self) -> usize {
        self.scene.row_count()
    }

    /// Number of columns in the target board scene.
    #[must_use]
    pub const fn col_count(&self) -> usize {
        DEFAULT_COL_COUNT
    }

    /// One-based rake row used by pvztoolkit lineup-code metadata.
    #[must_use]
    pub const fn rake_row(&self) -> Option<i32> {
        match self.rake_row {
            Some(row) => Some(row + 1),
            None => None,
        }
    }

    /// Returns a cell by 0-based grid.
    #[must_use]
    pub fn cell(&self, grid: Grid) -> Option<&LineupCell> {
        if !grid.is_in_bounds(self.row_count(), self.col_count()) {
            return None;
        }
        let index = grid.row as usize * self.col_count() + grid.col as usize;
        Some(&self.cells[index])
    }

    /// Returns the backend grid used for rake placement.
    pub fn rake_grid(&self) -> Option<Grid> {
        self.rake_row.map(|row| Grid { row, col: 7 })
    }
}
