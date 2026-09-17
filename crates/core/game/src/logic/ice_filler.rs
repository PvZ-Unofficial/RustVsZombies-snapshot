//! Backend-neutral ice filler state machine.

use crate::backend::{PlantReadBackend, SceneBackend, SeedPacketBackend};
use crate::logic::cards::{
    CardPlantingBackend, PlantSeedOutcome, find_seed_slot, find_usable_seed, seed_slot_usable, try_plant_seed,
};
use crate::logic::cob::find_plant_at_kind;
use crate::logic::grid::IntoGrid;
use crate::model::{CardSelection, Grid, PlantKind};
use crate::model::{MAX_SEED_SLOTS, SeedSlot};

fn try_parse_grids<I, G>(grids: I) -> Result<Vec<Grid>, IceFillerConfigError>
where
    I: IntoIterator<Item = G>,
    G: IntoGrid,
{
    grids
        .into_iter()
        .map(|grid| grid.try_into_grid().map_err(|_error| IceFillerConfigError::InvalidGrid))
        .collect()
}

/// Grid selection priority for the ice filler.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum IceFillerPriority {
    #[default]
    Grid,
    Hp,
}

pub const COFFEE_ICE_LEAD: i32 = 299;
const COFFEE: CardSelection = CardSelection::Plant(PlantKind::CoffeeBean);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoffeeIceTiming {
    pub wake_time: i32,
    pub active_kind: PlantKind,
    pub active_delay: i32,
}

#[must_use]
pub const fn coffee_ice_timing(effect_time: i32) -> CoffeeIceTiming {
    CoffeeIceTiming {
        wake_time: effect_time - COFFEE_ICE_LEAD,
        active_kind: PlantKind::IceShroom,
        active_delay: COFFEE_ICE_LEAD,
    }
}

/// Configuration error for the ice filler.
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum IceFillerConfigError {
    #[error("ice seed list cannot be empty")]
    EmptyIceSeedList,
    #[error("ice filler grid is invalid")]
    InvalidGrid,
    #[error("ice filler cannot start in night scenes")]
    NightScene,
    #[error("ice filler requires at least one configured ice seed")]
    MissingIceSeed,
    #[error("ice filler requires a coffee bean seed")]
    MissingCoffeeSeed,
    #[error("ice seed list entries must target the same plant kind")]
    MixedIceSeedList,
}

/// Runtime error for the ice filler.
#[derive(Debug, thiserror::Error)]
pub enum IceFillerError {
    #[error(transparent)]
    Config(#[from] IceFillerConfigError),
    #[error("backend ice filler operation failed: {0}")]
    Backend(crate::runtime::RuntimeError),
}

/// Per-script storage and coffee helper for ice shrooms.
#[derive(Clone, Debug)]
pub struct IceFiller {
    grids: Vec<Grid>,
    temp_grids: Vec<Grid>,
    ice_cards: Vec<CardSelection>,
    priority: IceFillerPriority,
    active: bool,
    paused: bool,
}

impl IceFiller {
    #[must_use]
    pub fn new() -> Self {
        Self {
            grids: Vec::new(),
            temp_grids: Vec::new(),
            ice_cards: vec![
                CardSelection::Plant(PlantKind::IceShroom),
                CardSelection::Imitator(PlantKind::IceShroom),
            ],
            priority: IceFillerPriority::Grid,
            active: false,
            paused: false,
        }
    }

    pub fn start<I, G>(&mut self, grids: I) -> Result<(), IceFillerError>
    where
        rsvz_current::CurrentBackend: SceneBackend + SeedPacketBackend,
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    {
        let grids = try_parse_grids(grids)?;
        self.start_prepared(grids)
    }

    #[doc(hidden)]
    pub fn start_prepared(&mut self, grids: Vec<Grid>) -> Result<(), IceFillerError>
    where
        rsvz_current::CurrentBackend: SceneBackend + SeedPacketBackend,
    {
        crate::access::with_backend(|backend| {
            if backend
                .scene()
                .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()))
                .is_night()
            {
                return Err(IceFillerConfigError::NightScene.into());
            }
            if !self
                .has_any_ice_seed()
                .map_err(|error| IceFillerError::Backend(error.into()))?
            {
                return Err(IceFillerConfigError::MissingIceSeed.into());
            }
            if find_seed_slot(COFFEE).is_none() {
                return Err(IceFillerConfigError::MissingCoffeeSeed.into());
            }

            self.grids = grids;
            self.active = true;
            self.paused = false;
            Ok(())
        })
    }

    pub fn tick(&mut self) -> Result<(), IceFillerError>
    where
        rsvz_current::CurrentBackend: CardPlantingBackend,
    {
        if !self.active || self.paused {
            return Ok(());
        }
        let mut used_slots = [false; MAX_SEED_SLOTS];
        if self.priority == IceFillerPriority::Grid {
            for grid in self.temp_grids.iter().chain(&self.grids).copied() {
                if !self
                    .fill_grid(grid, &mut used_slots)
                    .map_err(|error| IceFillerError::Backend(error.into()))?
                {
                    break;
                }
            }
            return Ok(());
        }

        let hp_cells = self
            .ice_hp_cells()
            .map_err(|error| IceFillerError::Backend(error.into()))?;
        let candidate_count = self.temp_grids.len() + self.grids.len();
        let mut previous = None;
        for _ in 0..candidate_count {
            let Some((key, grid)) =
                next_grid_by_hp(self.temp_grids.iter().chain(&self.grids).copied(), &hp_cells, previous)
            else {
                break;
            };
            previous = Some(key);
            if !self
                .fill_grid(grid, &mut used_slots)
                .map_err(|error| IceFillerError::Backend(error.into()))?
            {
                break;
            }
        }
        Ok(())
    }

    pub fn coffee(&mut self) -> Result<Option<Grid>, IceFillerError>
    where
        rsvz_current::CurrentBackend: CardPlantingBackend,
    {
        for index in (0..self.temp_grids.len()).rev() {
            let grid = self.temp_grids[index];
            if self.coffee_at(grid)?.is_some() {
                self.temp_grids.remove(index);
                return Ok(Some(grid));
            }
        }
        if self.priority == IceFillerPriority::Grid {
            for index in (0..self.grids.len()).rev() {
                let grid = self.grids[index];
                if self.coffee_at(grid)?.is_some() {
                    return Ok(Some(grid));
                }
            }
            return Ok(None);
        }

        let hp_cells = self
            .ice_hp_cells()
            .map_err(|error| IceFillerError::Backend(error.into()))?;
        let mut previous = None;
        for _ in 0..self.grids.len() {
            let Some((key, grid)) = next_grid_by_hp(self.grids.iter().rev().copied(), &hp_cells, previous) else {
                break;
            };
            previous = Some(key);
            if self.coffee_at(grid)?.is_some() {
                return Ok(Some(grid));
            }
        }
        Ok(None)
    }

    pub fn coffee_at<G>(&mut self, grid: G) -> Result<Option<Grid>, IceFillerError>
    where
        rsvz_current::CurrentBackend: CardPlantingBackend,
        G: IntoGrid,
    {
        {
            let grid = grid
                .try_into_grid()
                .map_err(|_error| IceFillerConfigError::InvalidGrid)?;
            if find_plant_at_kind(grid, self.ice_kind()).is_none() {
                return Ok(None);
            }
            let Some(slot) = find_usable_seed([COFFEE]) else {
                return Ok(None);
            };
            if matches!(try_plant_seed(slot, grid), PlantSeedOutcome::Planted(_)) {
                Ok(Some(grid))
            } else {
                Ok(None)
            }
        }
    }

    pub fn pause(&mut self) {
        self.paused = true;
    }

    pub fn resume(&mut self) {
        self.paused = false;
    }

    pub fn stop(&mut self) {
        self.active = false;
        self.paused = false;
        self.temp_grids.clear();
    }

    pub fn set_list<I, G>(&mut self, grids: I)
    where
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    {
        self.grids = grids.into_iter().map(IntoGrid::into_grid).collect();
    }

    pub fn set_checked_list<I, G>(&mut self, grids: I) -> Result<(), IceFillerConfigError>
    where
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    {
        self.grids = try_parse_grids(grids)?;
        Ok(())
    }

    pub fn set_temp_positions<I, G>(&mut self, grids: I)
    where
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    {
        self.temp_grids = grids.into_iter().map(IntoGrid::into_grid).collect();
    }

    pub fn set_checked_temp_positions<I, G>(&mut self, grids: I) -> Result<(), IceFillerConfigError>
    where
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    {
        self.temp_grids = try_parse_grids(grids)?;
        Ok(())
    }

    pub fn add_temp_position<G>(&mut self, grid: G)
    where
        G: IntoGrid,
    {
        self.temp_grids.push(grid.into_grid());
    }

    pub fn set_priority_mode(&mut self, mode: IceFillerPriority) {
        self.priority = mode;
    }

    pub fn set_ice_seed_list(
        &mut self, cards: impl IntoIterator<Item = CardSelection>,
    ) -> Result<(), IceFillerConfigError> {
        let cards = cards.into_iter().collect::<Vec<_>>();
        if cards.is_empty() {
            return Err(IceFillerConfigError::EmptyIceSeedList);
        }
        let ice_kind = card_selection_kind(cards[0]);
        if cards.iter().any(|card| card_selection_kind(*card) != ice_kind) {
            return Err(IceFillerConfigError::MixedIceSeedList);
        }
        self.ice_cards = cards;
        Ok(())
    }

    fn ice_kind(&self) -> PlantKind {
        card_selection_kind(self.ice_cards[0])
    }

    fn fill_grid(
        &self, grid: Grid, used_slots: &mut [bool; MAX_SEED_SLOTS],
    ) -> Result<bool, rsvz_current::CurrentBackendError>
    where
        rsvz_current::CurrentBackend: CardPlantingBackend,
    {
        {
            let Some(slot) = self.next_usable_ice_slot(used_slots)? else {
                return Ok(false);
            };
            if matches!(try_plant_seed(slot, grid), PlantSeedOutcome::Planted(_)) {
                used_slots[slot.index()] = true;
            }
            Ok(true)
        }
    }

    fn next_usable_ice_slot(
        &self, used_slots: &[bool; MAX_SEED_SLOTS],
    ) -> Result<Option<SeedSlot>, rsvz_current::CurrentBackendError>
    where
        rsvz_current::CurrentBackend: SeedPacketBackend,
    {
        {
            for selection in self.ice_cards.iter().copied() {
                let Some(slot) = find_seed_slot(selection) else {
                    continue;
                };
                if seed_slot_usable(slot) && !used_slots[slot.index()] {
                    return Ok(Some(slot));
                }
            }
            Ok(None)
        }
    }

    fn ice_hp_cells(&self) -> Result<[i32; ICE_FILLER_CELL_COUNT], rsvz_current::CurrentBackendError>
    where
        rsvz_current::CurrentBackend: PlantReadBackend,
    {
        crate::access::with_backend(|backend| {
            let mut cells = EMPTY_ICE_HP_CELLS;
            for plant in crate::live_value::read_or_abort(backend.plants(), "plants") {
                let plant = crate::plant::view_from_handle(backend, plant);
                if plant.kind != self.ice_kind() {
                    continue;
                }
                if let Some(index) = ice_filler_cell_index(plant.grid) {
                    cells[index] = cells[index].min(plant.hp);
                }
            }
            Ok(cells)
        })
    }

    fn has_any_ice_seed(&self) -> Result<bool, rsvz_current::CurrentBackendError>
    where
        rsvz_current::CurrentBackend: SeedPacketBackend,
    {
        {
            for selection in self.ice_cards.iter().copied() {
                if find_seed_slot(selection).is_some() {
                    return Ok(true);
                }
            }
            Ok(false)
        }
    }
}

const ICE_FILLER_ROWS: usize = 6;
const ICE_FILLER_COLS: usize = 9;
const ICE_FILLER_CELL_COUNT: usize = ICE_FILLER_ROWS * ICE_FILLER_COLS;
// Keep array-size evaluation outside a possibly unsatisfied backend bound.
const EMPTY_ICE_HP_CELLS: [i32; ICE_FILLER_CELL_COUNT] = [i32::MAX; ICE_FILLER_CELL_COUNT];

fn ice_filler_cell_index(grid: Grid) -> Option<usize> {
    let row = usize::try_from(grid.row).ok()?;
    let col = usize::try_from(grid.col).ok()?;
    (row < ICE_FILLER_ROWS && col < ICE_FILLER_COLS).then_some(row * ICE_FILLER_COLS + col)
}

fn ice_hp_at(cells: &[i32; ICE_FILLER_CELL_COUNT], grid: Grid) -> i32 {
    ice_filler_cell_index(grid).map_or(i32::MAX, |index| cells[index])
}

fn next_grid_by_hp(
    grids: impl Iterator<Item = Grid>, cells: &[i32; ICE_FILLER_CELL_COUNT], previous: Option<(i32, usize)>,
) -> Option<((i32, usize), Grid)> {
    // ponytail: lists are lawn-sized; add a setup-time order cache only if HP mode profiles hot.
    grids
        .enumerate()
        .filter_map(|(index, grid)| {
            let key = (ice_hp_at(cells, grid), index);
            previous.is_none_or(|previous| key > previous).then_some((key, grid))
        })
        .min_by_key(|(key, _grid)| *key)
}

const fn card_selection_kind(selection: CardSelection) -> PlantKind {
    match selection {
        CardSelection::Plant(kind) | CardSelection::Imitator(kind) => kind,
    }
}

impl Default for IceFiller {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_rejection_keeps_existing_lists() {
        let mut filler = IceFiller::new();
        filler.set_checked_list([(1, 1)]).unwrap();
        assert_eq!(
            filler.set_checked_list([(0, 1)]),
            Err(IceFillerConfigError::InvalidGrid)
        );
        assert_eq!(filler.grids, [Grid { row: 0, col: 0 }]);
        let original = filler.ice_cards.clone();
        assert_eq!(
            filler.set_ice_seed_list([]),
            Err(IceFillerConfigError::EmptyIceSeedList)
        );
        assert_eq!(
            filler.set_ice_seed_list([
                CardSelection::Plant(PlantKind::IceShroom),
                CardSelection::Imitator(PlantKind::DoomShroom)
            ]),
            Err(IceFillerConfigError::MixedIceSeedList)
        );
        assert_eq!(filler.ice_cards, original);
    }

    #[test]
    fn hp_selection_is_stable_and_respects_the_supplied_traversal_order() {
        let a = Grid { row: 0, col: 0 };
        let b = Grid { row: 1, col: 0 };
        let mut hp = [i32::MAX; ICE_FILLER_CELL_COUNT];
        hp[ice_filler_cell_index(a).unwrap()] = 100;
        hp[ice_filler_cell_index(b).unwrap()] = 100;
        let first = next_grid_by_hp([a, b].into_iter(), &hp, None).unwrap();
        assert_eq!(first.1, a);
        assert_eq!(next_grid_by_hp([a, b].into_iter(), &hp, Some(first.0)).unwrap().1, b);
        assert_eq!(next_grid_by_hp([a, b].into_iter().rev(), &hp, None).unwrap().1, b);
        hp[ice_filler_cell_index(b).unwrap()] = 1;
        assert_eq!(next_grid_by_hp([a, b].into_iter(), &hp, None).unwrap().1, b);
    }
}
