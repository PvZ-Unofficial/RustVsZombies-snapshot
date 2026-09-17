//! PE does not create collectible coin objects.

use std::convert::Infallible;

use rsvz_backend_api::{AutoCollectBackend, ItemClickCollectBackend, ItemReadBackend};
use rsvz_model::{ItemId, ItemKind};

use crate::{PeBackend, Result};

impl AutoCollectBackend for PeBackend {
    const AUTO_COLLECT_IS_NOOP: bool = true;

    fn set_normal_auto_collect_enabled(&self, _enabled: bool) -> Result<()> {
        Ok(())
    }
}

impl ItemReadBackend for PeBackend {
    const ITEMS_CAN_EXIST: bool = false;
    type ItemHandle<'a> = Infallible;
    type ItemIter<'a> = std::iter::Empty<Infallible>;

    fn items(&self) -> Result<Self::ItemIter<'_>> {
        Ok(std::iter::empty())
    }

    fn item_id<'a>(&'a self, item: Self::ItemHandle<'a>) -> ItemId {
        match item {}
    }

    fn item_kind<'a>(&'a self, item: Self::ItemHandle<'a>) -> Result<ItemKind> {
        match item {}
    }

    fn item_x<'a>(&'a self, handle: Self::ItemHandle<'a>) -> f32 {
        match handle {}
    }
    fn item_y<'a>(&'a self, handle: Self::ItemHandle<'a>) -> f32 {
        match handle {}
    }

    fn item_being_collected<'a>(&'a self, item: Self::ItemHandle<'a>) -> bool {
        match item {}
    }
}

impl ItemClickCollectBackend for PeBackend {
    fn click_collect_item<'a>(&'a self, item: Self::ItemHandle<'a>, _play_sound: bool) -> Result<()> {
        match item {}
    }
}
