use super::*;

impl ItemClickCollectBackend for PortableBackend {
    fn click_collect_item<'a>(&'a self, item: Self::ItemHandle<'a>, play_sound: bool) -> Result<()> {
        let world = self.world()?;
        // SAFETY: this item stays live in memory throughout the current backend borrow.
        let item = unsafe { pvzp_rs::Borrowed::from_non_null(item.as_non_null()) };
        world.item_collect(item, play_sound).map_err(Into::into)
    }
}

impl AutoCollectBackend for PortableBackend {
    fn set_normal_auto_collect_enabled(&self, enabled: bool) -> Result<()> {
        self.world()?;
        pvzp_rs::modifier::set_normal_auto_collect_enabled(enabled).map_err(Into::into)
    }
}

impl ItemReadBackend for PortableBackend {
    type ItemHandle<'a> = PortableItemHandle<'a>;
    type ItemIter<'a> = PortableItemIter<'a>;

    fn items(&self) -> Result<Self::ItemIter<'_>> {
        let pool = crate::access::item_pool(&self.world()?);
        let limit = pool.max_used_count();
        Ok(PortableItemIter::new(self, limit))
    }

    fn item_id<'a>(&'a self, handle: Self::ItemHandle<'a>) -> ItemId {
        self.item_id_from_handle(handle)
    }

    fn item_kind<'a>(&'a self, handle: Self::ItemHandle<'a>) -> Result<ItemKind> {
        let raw = read_raw!(handle, mType);
        ItemKind::from_raw(raw).ok_or_else(|| invalid_kind("item", raw))
    }

    fn item_x<'a>(&'a self, handle: Self::ItemHandle<'a>) -> f32 {
        read_raw!(handle, mPosX)
    }
    fn item_y<'a>(&'a self, handle: Self::ItemHandle<'a>) -> f32 {
        read_raw!(handle, mPosY)
    }

    fn item_being_collected<'a>(&'a self, handle: Self::ItemHandle<'a>) -> bool {
        read_raw!(handle, mIsBeingCollected)
    }
}
