use crate::backend::Backend;
use rsvz_model::model::{ItemId, ItemKind};

/// Backend capability for reading falling items.
pub trait ItemReadBackend: Backend {
    /// Whether this backend can ever expose a collectible item.
    const ITEMS_CAN_EXIST: bool = true;
    /// Item handle valid for the current read lifetime.
    type ItemHandle<'a>: Copy
    where
        Self: 'a;
    /// Live item handle iterator.
    type ItemIter<'a>: Iterator<Item = Self::ItemHandle<'a>>
    where
        Self: 'a;

    /// Current readable live item handles.
    fn items(&self) -> Result<Self::ItemIter<'_>, Self::Error>;

    /// Reads item ID from a validated handle.
    fn item_id<'a>(&'a self, handle: Self::ItemHandle<'a>) -> ItemId;
    /// Reads item kind from a validated handle.
    fn item_kind<'a>(&'a self, handle: Self::ItemHandle<'a>) -> Result<ItemKind, Self::Error>;
    /// Reads item coordinates independently from a validated handle.
    fn item_x<'a>(&'a self, handle: Self::ItemHandle<'a>) -> f32;
    fn item_y<'a>(&'a self, handle: Self::ItemHandle<'a>) -> f32;
    /// Reads whether a validated handle is already being collected.
    fn item_being_collected<'a>(&'a self, handle: Self::ItemHandle<'a>) -> bool;
}

/// Backend capability for collecting one falling item without host mouse input.
pub trait ItemClickCollectBackend: ItemReadBackend {
    fn click_collect_item<'a>(&'a self, item: Self::ItemHandle<'a>, play_sound: bool) -> Result<(), Self::Error>;
}

/// Backend capability for toggling the backend's native automatic collection behavior.
pub trait AutoCollectBackend: Backend {
    /// True when the collection contract is already satisfied without native work.
    const AUTO_COLLECT_IS_NOOP: bool = false;

    fn set_normal_auto_collect_enabled(&self, enabled: bool) -> Result<(), Self::Error>;
}
