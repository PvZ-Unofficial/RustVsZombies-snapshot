use rsvz_backend_api::backend::{AutoCollectBackend, ItemClickCollectBackend, ItemReadBackend};
use rsvz_model::model::{GameUi, ItemId, ItemKind};

use crate::error::Result;
use crate::impls::handles::{PvzItemHandle, PvzItemIter};
use crate::raw::kind::item_kind_from_raw;
use crate::raw::layout as ptrs;
use crate::runtime::Pvz1051Backend;

impl ItemReadBackend for Pvz1051Backend {
    type ItemHandle<'a> = PvzItemHandle<'a>;
    type ItemIter<'a> = PvzItemIter<'a>;

    fn items(&self) -> Result<Self::ItemIter<'_>> {
        self.ensure_game_ui(GameUi::Playing)?;
        let board = self.board()?;
        // SAFETY: `board` is non-null; the coin DataArray is embedded in Board.
        let array = unsafe { ptrs::Board::coins(board.as_ptr()) };
        // SAFETY: `array` points to an embedded DataArray.
        let max = unsafe { ptrs::DataArray::max_used_count(array) };
        Ok(PvzItemIter {
            backend: self,
            next: 0,
            max,
        })
    }

    fn item_id<'a>(&'a self, handle: Self::ItemHandle<'a>) -> ItemId {
        // SAFETY: handles are created only from occupied coin DataArray entries.
        ItemId::from_raw(unsafe { ptrs::DataArray::<ptrs::Coin>::coin_item_id(handle.ptr.as_ptr()) })
    }

    fn item_kind<'a>(&'a self, handle: Self::ItemHandle<'a>) -> Result<ItemKind> {
        // SAFETY: handle came from a verified live item path and reads a copied scalar.
        let raw_kind = unsafe { ptrs::Coin::coin_type(handle.ptr.as_ptr()) };
        item_kind_from_raw(raw_kind)
    }

    fn item_x<'a>(&'a self, handle: Self::ItemHandle<'a>) -> f32 {
        // SAFETY: the borrow-bound item handle is valid for this scalar read.
        unsafe { ptrs::Coin::pos_x(handle.ptr.as_ptr()) }
    }
    fn item_y<'a>(&'a self, handle: Self::ItemHandle<'a>) -> f32 {
        // SAFETY: the borrow-bound item handle is valid for this scalar read.
        unsafe { ptrs::Coin::pos_y(handle.ptr.as_ptr()) }
    }

    fn item_being_collected<'a>(&'a self, handle: Self::ItemHandle<'a>) -> bool {
        // SAFETY: handle came from a verified live item path.
        unsafe { ptrs::Coin::is_being_collected(handle.ptr.as_ptr()) }
    }
}

impl ItemClickCollectBackend for Pvz1051Backend {
    fn click_collect_item<'a>(&'a self, item: Self::ItemHandle<'a>, play_sound: bool) -> Result<()> {
        self.ensure_game_ui(GameUi::Playing)?;
        let board = self.board()?;
        // SAFETY: `board` is non-null and points to the active playing board.
        if unsafe { ptrs::Board::paused(board.as_ptr()) } {
            return Ok(());
        }
        // SAFETY: `board` is non-null and points to the active playing board.
        if unsafe { ptrs::Board::time_stop_counter(board.as_ptr()) } > 0 {
            return Ok(());
        }

        let coin = item.ptr;
        // SAFETY: the handle remains address-valid within this backend borrow.
        if unsafe { ptrs::Coin::is_dead(coin.as_ptr()) } {
            return Ok(());
        }
        // SAFETY: the borrowed coin remains address-valid.
        if unsafe { ptrs::Coin::is_being_collected(coin.as_ptr()) } {
            return Ok(());
        }
        if play_sound {
            // SAFETY: `coin` is held by the current borrowed handle and is not dead or
            // already being collected. ABI uses edx as `this` per 0x432b00 disassembly.
            unsafe { crate::raw::abi::coin_play_collect_sound(coin.as_ptr()) };
        }
        // SAFETY: `coin` is held by the current borrowed handle and is not dead or
        // already being collected. `Coin::Collect` uses ecx as `this` and performs PvZ's semantic
        // item collection without host mouse input.
        unsafe { crate::raw::abi::coin_collect(coin.as_ptr()) };
        Ok(())
    }
}

impl AutoCollectBackend for Pvz1051Backend {
    fn set_normal_auto_collect_enabled(&self, enabled: bool) -> Result<()> {
        crate::patches::leases::set_bool_patch(crate::patches::leases::BoolPatchId::AutoCollectNormal, enabled)
    }
}
