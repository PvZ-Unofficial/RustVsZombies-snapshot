//! Grid item capabilities. Entity operations use current borrowed handles.

use crate::backend::Backend;
use rsvz_model::model::{Grid, GridItemId, GridItemKind};

pub trait GridItemStateBackend: GridItemReadBackend {
    fn grid_item_countdown<'a>(&'a self, item: Self::GridItemHandle<'a>) -> i32;
}

/// Live grid-item read capability.
pub trait GridItemReadBackend: Backend {
    /// Grid-item handle valid for the current read lifetime.
    type GridItemHandle<'a>: Copy
    where
        Self: 'a;
    /// Live grid-item handle iterator.
    type GridItemIter<'a>: Iterator<Item = Self::GridItemHandle<'a>>
    where
        Self: 'a;

    /// Current readable live grid-item view.
    fn grid_items(&self) -> Result<Self::GridItemIter<'_>, Self::Error>;

    /// Reads grid-item ID from a validated handle.
    fn grid_item_id<'a>(&'a self, handle: Self::GridItemHandle<'a>) -> GridItemId;
    /// Reads grid-item kind from a validated handle.
    fn grid_item_kind<'a>(&'a self, handle: Self::GridItemHandle<'a>) -> Result<GridItemKind, Self::Error>;
    /// Reads the native grid-item row and column independently.
    fn grid_item_row<'a>(&'a self, handle: Self::GridItemHandle<'a>) -> i32;
    fn grid_item_col<'a>(&'a self, handle: Self::GridItemHandle<'a>) -> i32;
    /// Re-checks whether a validated handle is still live.
    fn grid_item_is_alive<'a>(&'a self, handle: Self::GridItemHandle<'a>) -> bool;
}

/// Three distinct native grid-item creation atoms.
pub trait GridItemCreateBackend: GridItemEditBackend {
    fn add_ladder<'a>(&'a self, grid: Grid) -> Result<Self::GridItemHandle<'a>, Self::Error>;
    fn add_crater<'a>(&'a self, grid: Grid) -> Result<Self::GridItemHandle<'a>, Self::Error>;
    fn add_gravestone<'a>(&'a self, grid: Grid) -> Result<Self::GridItemHandle<'a>, Self::Error>;
}

/// Grid-item editing capability.
pub trait GridItemEditBackend: GridItemReadBackend {
    /// Removes a current-frame grid-item handle.
    fn remove_grid_item<'a>(&'a self, handle: Self::GridItemHandle<'a>) -> Result<(), Self::Error>;
}
