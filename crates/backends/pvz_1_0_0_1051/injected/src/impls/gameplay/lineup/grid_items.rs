use super::*;

use rsvz_model::model::GridItemKind;

const GRAVESTONE_RENDER_ORDER_BASE: i32 = 0x497cb;
const RENDER_ORDER_ROW_OFFSET: i32 = 0x2710;

pub(super) fn grid_item_create_board(backend: &Pvz1051Backend, grid: Grid) -> Result<NonNull<ptrs::Board>> {
    match backend.game_ui()? {
        GameUi::LevelIntro | GameUi::Playing => {}
        _ => {
            return Err(Pvz1051Error::UnsupportedToolMode(
                "grid-item creation requires level-intro or playing UI",
            ));
        }
    }
    let board = backend.board()?;
    backend.validate_grid(grid)?;
    let items = crate::ops::grid_item::board_grid_items(board)?;
    // SAFETY: `items` is the current embedded DataArray; these are copied allocation-header fields.
    let has_slot = unsafe { ptrs::DataArray::active_count(items.as_ptr()) < ptrs::DataArray::max_size(items.as_ptr()) };
    if !has_slot {
        return Err(Pvz1051Error::AbiPreconditionFailed("grid-item pool is full"));
    }
    Ok(board)
}

fn created_grid_item(grid_item: *mut ptrs::GridItem, symbol: &'static str) -> Result<NonNull<ptrs::GridItem>> {
    NonNull::new(grid_item).ok_or(Pvz1051Error::AbiPreconditionFailed(symbol))
}

pub(super) fn add_ladder(grid: Grid) -> Result<NonNull<ptrs::GridItem>> {
    // SAFETY: the caller checked current Board, pool capacity, and grid bounds. Objdump confirms
    // Board::AddALadder receives grid_y in EDI and grid_x on the stack, returning EAX GridItem*.
    let grid_item = unsafe { crate::raw::abi::board_add_ladder(grid.row, grid.col) };
    created_grid_item(grid_item, "Board::AddALadder returned null")
}

pub(super) fn add_crater(grid: Grid) -> Result<NonNull<ptrs::GridItem>> {
    // SAFETY: the caller checked current Board, pool capacity, and grid bounds. Board::AddACrater
    // has the same verified register/stack shape as AddALadder and returns EAX GridItem*.
    let grid_item = unsafe { crate::raw::abi::board_add_crater(grid.row, grid.col) };
    created_grid_item(grid_item, "Board::AddACrater returned null")
}

pub(super) fn add_gravestone(board: NonNull<ptrs::Board>, grid: Grid) -> Result<NonNull<ptrs::GridItem>> {
    // Board::AddAGraveStone is source-level inline in 1051. This is its exact five-write body;
    // 0x408fc0 is the distinct plural Board::AddGraveStones routine.
    // SAFETY: the caller checked the current Board, pool capacity, and grid bounds.
    let grid_item = unsafe { crate::raw::abi::data_array_alloc_grid_item(board.as_ptr()) };
    let grid_item = NonNull::new(grid_item).ok_or(Pvz1051Error::AbiPreconditionFailed(
        "DataArray<GridItem>::DataArrayAlloc returned null",
    ))?;
    // SAFETY: TodRand's EAX input/return convention was checked at the 0x40900d grave callsite.
    let counter = -unsafe { crate::raw::abi::tod_rand(50) };
    // Grid bounds were validated, so row is a small non-negative board row.
    let render_order = grid.row * RENDER_ORDER_ROW_OFFSET + GRAVESTONE_RENDER_ORDER_BASE;
    // SAFETY: `grid_item` is the fresh item from Board's current grid-item array. The fields and
    // constants match Board.cpp's inline AddAGraveStone and objdump 0x409008..0x409036.
    unsafe {
        ptrs::GridItem::set_grid_item_type(grid_item.as_ptr(), GridItemKind::Grave.code());
        ptrs::GridItem::set_grid_item_counter(grid_item.as_ptr(), counter);
        ptrs::GridItem::set_render_order(grid_item.as_ptr(), render_order);
        ptrs::GridItem::set_grid_x(grid_item.as_ptr(), grid.col);
        ptrs::GridItem::set_grid_y(grid_item.as_ptr(), grid.row);
    }
    Ok(grid_item)
}
