//! Seed capabilities. Entity operations use current borrowed handles.

use crate::backend::Backend;
use rsvz_model::model::{CheckedCardSelection, SeedSlot};

/// Live seed-bank read capability.
pub trait SeedBankReadBackend: Backend {
    /// Seed handle valid for the current read lifetime.
    type SeedHandle<'a>: Copy
    where
        Self: 'a;
    /// Live seed handle iterator.
    type SeedIter<'a>: Iterator<Item = Self::SeedHandle<'a>>
    where
        Self: 'a;

    /// Current seed-bank view.
    fn seeds(&self) -> Result<Self::SeedIter<'_>, Self::Error>;

    /// Reads seed slot from a validated handle.
    fn seed_slot<'a>(&'a self, handle: Self::SeedHandle<'a>) -> SeedSlot;
    /// Reads seed selection from a validated handle.
    fn seed_selection<'a>(&'a self, handle: Self::SeedHandle<'a>) -> Result<CheckedCardSelection, Self::Error>;
    /// Reads whether a validated seed slot is currently usable.
    fn seed_is_usable<'a>(&'a self, handle: Self::SeedHandle<'a>) -> bool;
}

/// Current card selection in bank order, whether it still lives in the chooser or in a live seed bank.
pub trait CardSelectionReadBackend: Backend {
    fn selected_card_count(&self) -> Result<usize, Self::Error>;

    fn selected_card(&self, index: usize) -> Result<CheckedCardSelection, Self::Error>;
}

/// Native cooldown remaining on one borrowed live seed packet.
pub trait SeedCooldownReadBackend: SeedBankReadBackend {
    fn seed_cooldown_remaining<'a>(&'a self, seed: Self::SeedHandle<'a>) -> i32;
}

/// Preserved chooser cooldown state used outside a live seed bank.
pub trait ChooserCooldownReadBackend: Backend {
    fn chooser_cooldown_remaining(&self, _selection: CheckedCardSelection) -> Result<Option<i32>, Self::Error> {
        Ok(None)
    }
}

/// Append one card to the current card selection.
pub trait CardAppendSelectionBackend: Backend {
    fn select_card(&self, selection: CheckedCardSelection) -> Result<(), Self::Error>;

    /// Commits a staged replacement bank. Native choosers commit each click already.
    fn finish_card_selection(&self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Native seed-packet actions over a borrowed seed-bank slot.
pub trait SeedPacketBackend: SeedBankReadBackend {
    fn seed_can_pick_up<'a>(&'a self, seed: Self::SeedHandle<'a>) -> Result<bool, Self::Error>;

    /// Commits the complete seed-packet state transition after a successful planting.
    ///
    /// Backends whose native cursor path deactivates the packet before planting must include
    /// that transition here.
    fn seed_was_planted<'a>(&'a self, seed: Self::SeedHandle<'a>) -> Result<(), Self::Error>;
}
