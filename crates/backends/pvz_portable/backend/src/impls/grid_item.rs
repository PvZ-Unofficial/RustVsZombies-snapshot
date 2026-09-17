use super::*;

impl GridItemReadBackend for PortableBackend {
    type GridItemHandle<'a> = PortableGridItemHandle<'a>;
    type GridItemIter<'a> = PortableGridItemIter<'a>;

    fn grid_items(&self) -> Result<Self::GridItemIter<'_>> {
        Ok({
            let pool = crate::access::grid_item_pool(&self.world()?);
            let limit = pool.max_used_count();
            crate::handles::PortableGridItemIter::new(self, limit)
        })
    }

    fn grid_item_id<'a>(&'a self, handle: Self::GridItemHandle<'a>) -> GridItemId {
        self.grid_item_id_from_handle(handle)
    }

    fn grid_item_kind<'a>(&'a self, handle: Self::GridItemHandle<'a>) -> Result<GridItemKind> {
        let raw = read_raw!(handle, mGridItemType);
        GridItemKind::try_from_code(raw).map_err(|_| invalid_kind("grid-item", raw))
    }

    fn grid_item_row<'a>(&'a self, handle: Self::GridItemHandle<'a>) -> i32 {
        read_raw!(handle, mGridY)
    }
    fn grid_item_col<'a>(&'a self, handle: Self::GridItemHandle<'a>) -> i32 {
        read_raw!(handle, mGridX)
    }

    fn grid_item_is_alive<'a>(&'a self, handle: Self::GridItemHandle<'a>) -> bool {
        !read_raw!(handle, mDead)
    }
}

impl GridItemCreateBackend for PortableBackend {
    fn add_ladder<'a>(&'a self, grid: Grid) -> Result<PortableGridItemHandle<'a>> {
        validate_grid(self, grid)?;
        self.world()?
            .add_ladder(grid.col, grid.row)?
            .map(pvzp_rs::Borrowed::as_non_null)
            .map(PortableGridItemHandle::new)
            .ok_or(PortableBackendError::OperationRejected("add ladder"))
    }

    fn add_crater<'a>(&'a self, grid: Grid) -> Result<PortableGridItemHandle<'a>> {
        validate_grid(self, grid)?;
        self.world()?
            .add_crater(grid.col, grid.row)?
            .map(pvzp_rs::Borrowed::as_non_null)
            .map(PortableGridItemHandle::new)
            .ok_or(PortableBackendError::OperationRejected("add crater"))
    }

    fn add_gravestone<'a>(&'a self, grid: Grid) -> Result<PortableGridItemHandle<'a>> {
        validate_grid(self, grid)?;
        self.world()?
            .add_gravestone(grid.col, grid.row)?
            .map(pvzp_rs::Borrowed::as_non_null)
            .map(PortableGridItemHandle::new)
            .ok_or(PortableBackendError::OperationRejected("add gravestone"))
    }
}

impl GridItemEditBackend for PortableBackend {
    fn remove_grid_item<'a>(&'a self, handle: Self::GridItemHandle<'a>) -> Result<()> {
        let world = self.world()?;
        // SAFETY: the handle is bound to this current backend/world borrow.
        let item = unsafe { pvzp_rs::Borrowed::from_non_null(handle.as_non_null()) };
        world.grid_item_die(item).map_err(Into::into)
    }
}

impl GridItemStateBackend for PortableBackend {
    fn grid_item_countdown<'a>(&'a self, item: Self::GridItemHandle<'a>) -> i32 {
        read_raw!(item, mGridItemCounter)
    }
}
