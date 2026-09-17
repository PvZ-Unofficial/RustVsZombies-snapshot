use std::cell::RefCell;

use crate::raw::layout as ptrs;

// Board::Board initializes the 1051 plant DataArray with exactly 1024 slots.
const PLANT_POOL_CAPACITY: usize = 1024;
const DATA_ARRAY_INDEX_MASK: u32 = 0xffff;

#[derive(Clone, Copy)]
struct MorphLink {
    placeholder: u32,
    successor: u32,
    batch: u32,
}

impl MorphLink {
    const EMPTY: Self = Self {
        placeholder: 0,
        successor: 0,
        batch: 0,
    };
}

struct MorphLinks {
    board: *mut ptrs::Board,
    batch: u32,
    by_placeholder_slot: [MorphLink; PLANT_POOL_CAPACITY],
}

impl MorphLinks {
    const fn new() -> Self {
        Self {
            board: std::ptr::null_mut(),
            batch: 0,
            by_placeholder_slot: [MorphLink::EMPTY; PLANT_POOL_CAPACITY],
        }
    }

    fn reset(&mut self) {
        self.board = std::ptr::null_mut();
        self.batch = 0;
        self.by_placeholder_slot.fill(MorphLink::EMPTY);
    }

    fn begin_update_batch(&mut self) {
        self.batch = self.batch.wrapping_add(1);
        if self.batch == 0 {
            self.by_placeholder_slot.fill(MorphLink::EMPTY);
            self.batch = 1;
        }
    }

    fn record(&mut self, board: *mut ptrs::Board, placeholder: u32, successor: u32) {
        if board.is_null() || placeholder == 0 || successor == 0 {
            return;
        }
        self.board = board;
        let slot = (placeholder & DATA_ARRAY_INDEX_MASK) as usize;
        let Some(link) = self.by_placeholder_slot.get_mut(slot) else {
            return;
        };
        *link = MorphLink {
            placeholder,
            successor,
            batch: self.batch,
        };
    }

    fn successor(&self, board: *mut ptrs::Board, placeholder: u32) -> Option<u32> {
        if self.board != board || placeholder == 0 {
            return None;
        }
        let slot = (placeholder & DATA_ARRAY_INDEX_MASK) as usize;
        let link = self.by_placeholder_slot.get(slot)?;
        (link.placeholder == placeholder && link.successor != 0 && link.batch == self.batch).then_some(link.successor)
    }
}

thread_local! {
    static LINKS: RefCell<MorphLinks> = const { RefCell::new(MorphLinks::new()) };
}

pub(crate) fn record(board: *mut ptrs::Board, placeholder: u32, successor: u32) {
    let _access = LINKS.try_with(|links| {
        let Ok(mut links) = links.try_borrow_mut() else {
            return;
        };
        links.record(board, placeholder, successor);
    });
}

pub(crate) fn begin_update_batch() {
    let _access = LINKS.try_with(|links| {
        let Ok(mut links) = links.try_borrow_mut() else {
            return;
        };
        links.begin_update_batch();
    });
}

pub(crate) fn successor(board: *mut ptrs::Board, placeholder: u32) -> Option<u32> {
    LINKS
        .try_with(|links| {
            let links = links.try_borrow().ok()?;
            links.successor(board, placeholder)
        })
        .ok()
        .flatten()
}

pub(crate) fn reset() {
    let _access = LINKS.try_with(|links| {
        let Ok(mut links) = links.try_borrow_mut() else {
            return;
        };
        links.reset();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_every_same_update_morph_without_scanning() {
        let mut board_tag = 0_u8;
        let board = std::ptr::from_mut(&mut board_tag).cast::<ptrs::Board>();
        let mut links = MorphLinks::new();
        links.begin_update_batch();
        links.record(board, 0x0001_0003, 0x0002_0008);
        links.record(board, 0x0003_0007, 0x0004_0009);

        assert_eq!(links.successor(board, 0x0001_0003), Some(0x0002_0008));
        assert_eq!(links.successor(board, 0x0003_0007), Some(0x0004_0009));
    }

    #[test]
    fn full_id_prevents_a_reused_slot_from_matching_an_old_placeholder() {
        let mut board_tag = 0_u8;
        let board = std::ptr::from_mut(&mut board_tag).cast::<ptrs::Board>();
        let mut links = MorphLinks::new();
        links.begin_update_batch();
        links.record(board, 0x0001_0003, 0x0002_0008);
        links.record(board, 0x0005_0003, 0x0006_000a);

        assert_eq!(links.successor(board, 0x0001_0003), None);
        assert_eq!(links.successor(board, 0x0005_0003), Some(0x0006_000a));
    }

    #[test]
    fn board_change_and_reset_discard_stale_links() {
        let (mut board_a_tag, mut board_b_tag) = (0_u8, 0_u8);
        let board_a = std::ptr::from_mut(&mut board_a_tag).cast::<ptrs::Board>();
        let board_b = std::ptr::from_mut(&mut board_b_tag).cast::<ptrs::Board>();
        let mut links = MorphLinks::new();
        links.begin_update_batch();
        links.record(board_a, 0x0001_0003, 0x0002_0008);
        links.begin_update_batch();
        links.record(board_b, 0x0003_0007, 0x0004_0009);

        assert_eq!(links.successor(board_a, 0x0001_0003), None);
        assert_eq!(links.successor(board_b, 0x0003_0007), Some(0x0004_0009));

        links.reset();
        assert_eq!(links.successor(board_b, 0x0003_0007), None);
    }

    #[test]
    fn next_update_batch_expires_links_in_constant_time() {
        let mut board_tag = 0_u8;
        let board = std::ptr::from_mut(&mut board_tag).cast::<ptrs::Board>();
        let mut links = MorphLinks::new();
        links.begin_update_batch();
        links.record(board, 0x0001_0003, 0x0002_0008);
        assert_eq!(links.successor(board, 0x0001_0003), Some(0x0002_0008));

        links.begin_update_batch();
        assert_eq!(links.successor(board, 0x0001_0003), None);
    }
}
