use std::ptr::NonNull;

use rsvz_model::model::GridItemKind;

use crate::error::{Pvz1051Error, Result};
use crate::raw::layout as ptrs;

pub(crate) fn board_grid_items(board: NonNull<ptrs::Board>) -> Result<NonNull<ptrs::DataArray<ptrs::GridItem>>> {
    // SAFETY: `board` is non-null and the grid-item DataArray is embedded in Board.
    let array = unsafe { ptrs::Board::grid_items(board.as_ptr()) };
    NonNull::new(array).ok_or(Pvz1051Error::InvariantViolated("grid-item array"))
}

pub(crate) fn grid_item_kind_from_raw(raw: i32) -> Option<GridItemKind> {
    GridItemKind::try_from_code(raw).ok()
}
