//! Backend-neutral automatic collectible item helper.

use std::num::NonZeroI32;

use crate::backend::{AutoCollectBackend, ClockBackend, ItemClickCollectBackend, ItemReadBackend};
use crate::model::Position;
use crate::model::{AutoCollectMode, ItemKind};
use rsvz_current::CurrentBackend;

const DEFAULT_INTERVAL: i32 = 10;
const ITEM_KIND_CAPACITY: usize = 28;

/// Configuration error for automatic item collection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ItemCollectorConfigError {
    #[error("item collector interval must be at least 1")]
    InvalidInterval,
    #[error("item collector interval must not exceed i32::MAX")]
    IntervalTooLarge,
    #[error("unknown item kind: {0}")]
    UnknownItemKind(i32),
}

/// Runtime error for automatic item collection.
#[derive(Debug, thiserror::Error)]
pub enum ItemCollectorError {
    #[error("item collector config error: {0}")]
    Config(#[from] ItemCollectorConfigError),
    #[error("backend item collector operation failed: {0}")]
    Backend(crate::runtime::RuntimeError),
}

/// Per-script automatic item collection state.
#[derive(Clone, Debug)]
pub struct ItemCollector {
    enabled: bool,
    mode: AutoCollectMode,
    interval: NonZeroI32,
    allowed: ItemKindSet,
    play_sound: bool,
    allow_cursor_side_effects: bool,
}

impl ItemCollector {
    #[expect(
        clippy::expect_used,
        clippy::missing_panics_doc,
        reason = "DEFAULT_INTERVAL is a non-zero constant by construction"
    )]
    #[must_use]
    pub fn new() -> Self {
        Self {
            enabled: true,
            mode: AutoCollectMode::Normal,
            interval: NonZeroI32::new(DEFAULT_INTERVAL).expect("default interval is non-zero"),
            allowed: ItemKindSet::avz_default(),
            play_sound: true,
            allow_cursor_side_effects: false,
        }
    }

    pub fn tick(&mut self) -> Result<(), ItemCollectorError>
    where
        CurrentBackend: AutoCollectBackend + ClockBackend + ItemReadBackend + ItemClickCollectBackend,
    {
        if !self.tick_native()? {
            return Ok(());
        }
        self.tick_click()
    }

    pub fn tick_native(&mut self) -> Result<bool, ItemCollectorError>
    where
        CurrentBackend: AutoCollectBackend,
    {
        if CurrentBackend::AUTO_COLLECT_IS_NOOP {
            return Ok(false);
        }
        crate::access::with_backend(|backend| {
            if !self.enabled {
                backend
                    .set_normal_auto_collect_enabled(false)
                    .map_err(|error| ItemCollectorError::Backend(crate::access::operation_error(error)))?;
                return Ok(false);
            }

            let normal_enabled = matches!(self.mode, AutoCollectMode::Normal);
            backend
                .set_normal_auto_collect_enabled(normal_enabled)
                .map_err(|error| ItemCollectorError::Backend(crate::access::operation_error(error)))?;
            Ok(matches!(self.mode, AutoCollectMode::Click))
        })
    }

    pub fn tick_click(&mut self) -> Result<(), ItemCollectorError>
    where
        CurrentBackend: ClockBackend + ItemReadBackend + ItemClickCollectBackend,
    {
        if !CurrentBackend::ITEMS_CAN_EXIST {
            return Ok(());
        }
        if !self.enabled || !matches!(self.mode, AutoCollectMode::Click) {
            return Ok(());
        }

        crate::access::with_backend(|backend| {
            if backend
                .clock()
                .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()))
                % self.interval.get()
                != 0
            {
                return Ok(());
            }

            if let Some(item) = self.select_item(backend) {
                backend
                    .click_collect_item(item, self.play_sound)
                    .map_err(|error| ItemCollectorError::Backend(crate::access::operation_error(error)))?;
            }
            Ok(())
        })
    }

    #[must_use]
    pub fn mode(&self) -> AutoCollectMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: AutoCollectMode) {
        self.mode = mode;
        self.enabled = true;
    }

    pub fn set_interval(&mut self, interval: u32) -> Result<(), ItemCollectorConfigError> {
        if interval == 0 {
            return Err(ItemCollectorConfigError::InvalidInterval);
        }
        let interval = i32::try_from(interval).map_err(|_error| ItemCollectorConfigError::IntervalTooLarge)?;
        let interval = NonZeroI32::new(interval).ok_or(ItemCollectorConfigError::InvalidInterval)?;
        self.interval = interval;
        Ok(())
    }

    pub fn set_type_list<I>(&mut self, types: I) -> Result<(), ItemCollectorConfigError>
    where
        I: IntoIterator<Item = i32>,
    {
        let mut allowed = ItemKindSet::empty();
        for raw in types {
            let kind = ItemKind::try_from(raw).map_err(ItemCollectorConfigError::UnknownItemKind)?;
            allowed.insert(kind);
        }
        self.allowed = allowed;
        Ok(())
    }

    pub fn set_types<I>(&mut self, types: I)
    where
        I: IntoIterator<Item = ItemKind>,
    {
        self.allowed = ItemKindSet::from_kinds(types);
    }

    pub fn pause(&mut self) {
        self.enabled = false;
    }

    pub fn resume(&mut self) {
        self.enabled = true;
    }

    pub fn start(&mut self) {
        self.resume();
    }

    pub fn stop(&mut self) {
        self.enabled = false;
    }

    pub fn set_play_sound(&mut self, play_sound: bool) {
        self.play_sound = play_sound;
    }

    pub fn set_allow_cursor_side_effects(&mut self, allow: bool) {
        self.allow_cursor_side_effects = allow;
    }

    fn select_item<'a>(
        &self, backend: &'a CurrentBackend,
    ) -> Option<<CurrentBackend as ItemReadBackend>::ItemHandle<'a>>
    where
        CurrentBackend: ItemReadBackend,
    {
        first_preferred_item(crate::live_value::read_or_abort(backend.items(), "items").map(|item| {
            let kind = crate::live_value::read_or_abort(backend.item_kind(item), "item_kind");
            self.is_collectible(
                kind,
                Position {
                    x: backend.item_x(item),
                    y: backend.item_y(item),
                },
                backend.item_being_collected(item),
            )
            .then_some((item, kind))
        }))
    }

    fn is_collectible(&self, kind: ItemKind, pos: Position, being_collected: bool) -> bool {
        !being_collected
            && self.allowed.contains(kind)
            && (self.allow_cursor_side_effects || !kind.changes_cursor())
            && pos.x >= 0.0
            && pos.y >= 70.0
    }
}

impl Default for ItemCollector {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug)]
struct ItemKindSet {
    allowed: [bool; ITEM_KIND_CAPACITY],
}

impl ItemKindSet {
    fn empty() -> Self {
        Self {
            allowed: [false; ITEM_KIND_CAPACITY],
        }
    }

    fn avz_default() -> Self {
        Self::from_kinds(
            (1..=27)
                .filter_map(ItemKind::from_raw)
                .filter(|kind| !kind.changes_cursor()),
        )
    }

    fn from_kinds(kinds: impl IntoIterator<Item = ItemKind>) -> Self {
        let mut set = Self::empty();
        for kind in kinds {
            set.insert(kind);
        }
        set
    }

    fn insert(&mut self, kind: ItemKind) {
        let index = kind.raw() as usize;
        self.allowed[index] = true;
    }

    fn contains(&self, kind: ItemKind) -> bool {
        let index = kind.raw() as usize;
        self.allowed.get(index).copied().unwrap_or(false)
    }
}

fn first_preferred_item<T>(candidates: impl IntoIterator<Item = Option<(T, ItemKind)>>) -> Option<T> {
    let mut first = None;
    for candidate in candidates {
        let Some((item, kind)) = candidate else {
            continue;
        };
        if kind.is_sun() {
            return Some(item);
        }
        first.get_or_insert(item);
    }
    first
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        reason = "configuration tests fail directly on invalid setup"
    )]

    use super::*;
    use crate::model::{ItemId, Position};

    #[test]
    fn item_kind_raw_conversions_cover_game_and_avz_documented_range() {
        assert_eq!(ItemKind::try_from(1), Ok(ItemKind::SilverCoin));
        assert_eq!(ItemKind::try_from(27), Ok(ItemKind::PresentSurvivalMode));
        assert!(ItemKind::try_from(0).is_err());
        assert!(ItemKind::try_from(28).is_err());
    }

    #[test]
    fn item_kind_classification_is_explicit() {
        assert!(ItemKind::Sun.is_sun());
        assert!(ItemKind::SmallSun.is_sun());
        assert!(ItemKind::LargeSun.is_sun());
        assert!(!ItemKind::GoldCoin.is_sun());
        assert!(ItemKind::UsableSeedPacket.changes_cursor());
        assert!(!ItemKind::FinalSeedPacket.changes_cursor());
    }

    #[test]
    fn default_collector_matches_avz_like_policy() {
        let collector = ItemCollector::new();
        assert!(collector.enabled);
        assert_eq!(collector.mode(), AutoCollectMode::Normal);
        assert_eq!(collector.interval.get(), DEFAULT_INTERVAL);
        assert!(collector.play_sound);
        assert!(!collector.allow_cursor_side_effects);
        assert!(collector.allowed.contains(ItemKind::Sun));
        assert!(!collector.allowed.contains(ItemKind::UsableSeedPacket));
    }

    #[test]
    fn invalid_interval_is_rejected() {
        assert_eq!(
            ItemCollector::new().set_interval(0),
            Err(ItemCollectorConfigError::InvalidInterval)
        );
        assert_eq!(
            ItemCollector::new().set_interval(i32::MAX as u32 + 1),
            Err(ItemCollectorConfigError::IntervalTooLarge)
        );
    }

    #[test]
    fn raw_type_list_replaces_allowed_types() {
        let mut collector = ItemCollector::new();
        collector.set_type_list([1, 2, 3]).unwrap();
        assert_eq!(collector.mode(), AutoCollectMode::Normal);
        assert!(collector.allowed.contains(ItemKind::SilverCoin));
        assert!(!collector.allowed.contains(ItemKind::Sun));
        assert_eq!(
            collector.set_type_list([1, 99]),
            Err(ItemCollectorConfigError::UnknownItemKind(99))
        );
    }

    #[test]
    fn candidates_prefer_first_sun_and_stop_before_later_failures() {
        let coin = (ItemId::from_raw(1), ItemKind::GoldCoin);
        let sun = (ItemId::from_raw(2), ItemKind::Sun);
        let candidates = [Some(coin), Some(sun)]
            .into_iter()
            .chain(std::iter::once_with(|| panic!("must not read past the first sun")));
        assert_eq!(first_preferred_item(candidates), Some(sun.0));
        assert_eq!(first_preferred_item([None, Some(coin)]), Some(coin.0));
    }

    #[test]
    fn eligibility_filters_position_state_and_cursor_effects() {
        let mut collector = ItemCollector::new();
        let pos = Position { x: 10.0, y: 80.0 };
        assert!(collector.is_collectible(ItemKind::Sun, pos, false));
        assert!(!collector.is_collectible(ItemKind::Sun, pos, true));
        assert!(!collector.is_collectible(ItemKind::Sun, Position { x: -1.0, y: 80.0 }, false));
        assert!(!collector.is_collectible(ItemKind::Sun, Position { x: 10.0, y: 69.0 }, false));
        assert!(!collector.is_collectible(ItemKind::UsableSeedPacket, pos, false));
        collector.set_type_list([ItemKind::UsableSeedPacket.raw()]).unwrap();
        assert!(!collector.is_collectible(ItemKind::UsableSeedPacket, pos, false));
        collector.set_allow_cursor_side_effects(true);
        assert!(collector.is_collectible(ItemKind::UsableSeedPacket, pos, false));
    }
}
