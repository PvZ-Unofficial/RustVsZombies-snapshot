use crate::backend::GridGeometryBackend;
use crate::model::PixelPos;
use crate::model::{Grid, GridError};

/// Composes the native x/y grid atoms while translating RSVZ row/col order at the boundary.
pub fn grid_to_pixel(grid: Grid) -> crate::runtime::RuntimeResult<PixelPos>
where
    rsvz_current::CurrentBackend: GridGeometryBackend,
{
    crate::access::with_backend(|backend| {
        Ok(PixelPos {
            x: crate::live_value::read_or_abort(backend.grid_to_pixel_x(grid.col, grid.row), "grid_to_pixel_x"),
            y: crate::live_value::read_or_abort(backend.grid_to_pixel_y(grid.col, grid.row), "grid_to_pixel_y"),
        })
    })
}

/// 把脚本格输入转换为 core `0-based` [`Grid`]。
///
/// `(i32, i32)` 按玩家使用的 `1-based (row, col)` 解释；已经类型化的
/// [`Grid`] 被视为 core 值并原样使用。因此 `(2, 3)` 与
/// `Grid::from_one_based(2, 3).unwrap()` 表示同一格，而不是
/// `Grid::new(2, 3).unwrap()`。
pub trait IntoGrid {
    /// 尝试转换，不因坐标无效而 panic。
    fn try_into_grid(self) -> Result<Grid, GridError>;

    /// 转换格子；脚本元组不是正的 `1-based` 坐标时 panic。
    ///
    /// 可恢复的用户输入应优先调用 [`IntoGrid::try_into_grid`]。
    fn into_grid(self) -> Grid
    where
        Self: Sized,
    {
        match self.try_into_grid() {
            Ok(grid) => grid,
            Err(_error) => {
                panic!("script-facing grid coordinates must be 1-based positive values")
            }
        }
    }
}

impl IntoGrid for Grid {
    fn try_into_grid(self) -> Result<Grid, GridError> {
        Ok(self)
    }
}

impl IntoGrid for (i32, i32) {
    fn try_into_grid(self) -> Result<Grid, GridError> {
        Grid::from_one_based(self.0, self.1)
    }
}
