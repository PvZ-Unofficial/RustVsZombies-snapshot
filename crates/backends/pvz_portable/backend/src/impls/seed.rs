use super::*;

impl SeedBankReadBackend for PortableBackend {
    type SeedHandle<'a> = PortableSeedHandle<'a>;
    type SeedIter<'a> = PortableSeedIter<'a>;

    fn seeds(&self) -> Result<Self::SeedIter<'_>> {
        let world = self.world()?;
        let count = world.seed_count()?;
        Ok(PortableSeedIter::new(self, count))
    }

    fn seed_slot<'a>(&'a self, handle: Self::SeedHandle<'a>) -> SeedSlot {
        let raw = read_raw!(handle, mIndex).max(0) as usize;
        SeedSlot::new(raw).expect("Portable seed handles use native seed-bank indices")
    }

    fn seed_selection<'a>(&'a self, handle: Self::SeedHandle<'a>) -> Result<CheckedCardSelection> {
        card_selection(read_raw!(handle, mPacketType), read_raw!(handle, mImitaterType))
    }

    fn seed_is_usable<'a>(&'a self, handle: Self::SeedHandle<'a>) -> bool {
        SeedPacketBackend::seed_can_pick_up(self, handle).unwrap_or(false)
    }
}

impl SeedCooldownReadBackend for PortableBackend {
    fn seed_cooldown_remaining<'a>(&'a self, seed: Self::SeedHandle<'a>) -> i32 {
        if !read_raw!(seed, mRefreshing) {
            return 0;
        }
        read_raw!(seed, mRefreshTime)
            .saturating_sub(read_raw!(seed, mRefreshCounter))
            .saturating_add(1)
            .max(0)
    }
}

impl SeedPacketBackend for PortableBackend {
    fn seed_can_pick_up<'a>(&'a self, seed: Self::SeedHandle<'a>) -> Result<bool> {
        let world = self.world()?;
        // SAFETY: the seed handle was produced from this current world's bank.
        let seed = unsafe { pvzp_rs::Borrowed::from_non_null(seed.as_non_null()) };
        world.seed_can_pick_up(seed).map_err(Into::into)
    }

    fn seed_was_planted<'a>(&'a self, seed: Self::SeedHandle<'a>) -> Result<()> {
        let world = self.world()?;
        // SAFETY: the seed handle was produced from this current world's bank.
        let seed = unsafe { pvzp_rs::Borrowed::from_non_null(seed.as_non_null()) };
        world.seed_was_planted(seed).map_err(Into::into)
    }
}

impl CardSelectionReadBackend for PortableBackend {
    fn selected_card_count(&self) -> Result<usize> {
        usize::try_from(pvzp_rs::selected_card_count()?)
            .map_err(|_| PortableBackendError::NumericOutOfRange("selected card count"))
    }

    fn selected_card(&self, index: usize) -> Result<CheckedCardSelection> {
        let index = u32::try_from(index).map_err(|_| PortableBackendError::NumericOutOfRange("selected card index"))?;
        let (packet, imitater) = pvzp_rs::selected_card(index)?;
        card_selection(packet, imitater)
    }
}

impl ChooserCooldownReadBackend for PortableBackend {}

impl CardAppendSelectionBackend for PortableBackend {
    fn select_card(&self, selection: CheckedCardSelection) -> Result<()> {
        self.ensure_seed_chooser_ready_for_auto_selection()?;
        let (packet, imitater) = card_parts(selection);
        pvzp_rs::select_card(packet, imitater).map_err(Into::into)
    }
}

impl SeedChooserFastForwardBackend for PortableBackend {
    fn request_seed_chooser_fast_forward(&self, options: SeedChooserFastForwardOptions) -> Result<()> {
        pvzp_rs::request_seed_chooser_fast_forward(options.max_frames).map_err(Into::into)
    }
}

impl SeedRuleEditBackend for PortableBackend {
    fn set_seed_recharge_ignored(&self, enabled: bool) -> Result<()> {
        self.world()?;
        pvzp_rs::modifier::set_seed_recharge_ignored(enabled).map_err(Into::into)
    }

    fn seed_recharge_ignored(&self) -> Result<bool> {
        self.world()?;
        Ok(pvzp_rs::modifier::seed_recharge_ignored())
    }
}
