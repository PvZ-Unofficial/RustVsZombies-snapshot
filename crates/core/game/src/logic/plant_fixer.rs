//! AvZ-style automatic plant repair logic.

use crate::backend::{ClockBackend, CobFireBackend, CursorQueryBackend, SceneBackend, SunQueryBackend};
use crate::backend::{PlantCostBackend, PlantReadBackend, SeedPacketBackend};
use crate::logic::cards::CardPlantingBackend;
use crate::logic::cards::{
    PlantSeedOutcome, can_plant_seed, find_seed_slot, seed_slot_current_cost, seed_slot_usable, try_plant_seed,
};
use crate::logic::cob::cob_recover_time;
use crate::logic::grid::IntoGrid;
use crate::model::{CardSelection, GridPlantView, SeedSlot};
use crate::model::{Grid, PlantKind};

pub trait PlantFixerBackend:
    SceneBackend + CardPlantingBackend + CobFireBackend + ClockBackend + CursorQueryBackend + SunQueryBackend
{
}

impl<T> PlantFixerBackend for T where
    T: SceneBackend + CardPlantingBackend + CobFireBackend + ClockBackend + CursorQueryBackend + SunQueryBackend
{
}

#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum PlantFixerGridError {
    #[error("grid coordinate must be non-negative")]
    NegativeCoordinate,
    #[error("grid is outside the current scene")]
    OutOfScene,
    #[error("cob cannon repair grid must leave room for the right cell")]
    CobRightCellOutOfBounds,
}

#[derive(Debug, thiserror::Error)]
pub enum PlantFixerError {
    #[error("backend plant-fixer operation failed: {0}")]
    Backend(crate::runtime::RuntimeError),
    #[error("invalid plant fixer grid {grid:?}: {reason}")]
    InvalidGrid { grid: Grid, reason: PlantFixerGridError },
    #[error("plant fixer does not support imitator as target")]
    ImitatorTarget,
    #[error("invalid plant fixer threshold {0}")]
    InvalidThreshold(f32),
    #[error("invalid plant fixer run interval {0}")]
    InvalidRunInterval(u32),
    #[error("missing seed for plant fixer target {0:?}")]
    MissingSeed(PlantKind),
    #[error("plant fixer seed cost overflow")]
    CostOverflow,
}

#[derive(Clone, Debug)]
pub struct PlantFixer {
    kind: Option<PlantKind>,
    grids: Vec<Grid>,
    fix_hp: i32,
    seed_slots: [Option<SeedSlot>; 3],
    check_cards: bool,
    not_interrupt: bool,
    skip_covered_bottom: bool,
    sun_threshold: u32,
    run_interval: u32,
    use_coffee: bool,
    coffee_seed_slot: Option<SeedSlot>,
    paused: bool,
    trace: bool,
}

impl PlantFixer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            kind: None,
            grids: Vec::new(),
            fix_hp: 0,
            seed_slots: [None, None, None],
            check_cards: true,
            not_interrupt: true,
            skip_covered_bottom: true,
            sun_threshold: 0,
            run_interval: 1,
            use_coffee: false,
            coffee_seed_slot: None,
            paused: false,
            trace: false,
        }
    }

    pub fn start<I, G>(&mut self, kind: PlantKind, grids: I) -> Result<(), PlantFixerError>
    where
        rsvz_current::CurrentBackend: SceneBackend + PlantReadBackend + SeedPacketBackend,
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    {
        self.start_with(kind, grids, 0.9999, true)
    }

    pub fn start_with<I, G>(
        &mut self, kind: PlantKind, grids: I, fix_threshold: f32, use_imitator: bool,
    ) -> Result<(), PlantFixerError>
    where
        rsvz_current::CurrentBackend: SceneBackend + PlantReadBackend + SeedPacketBackend,
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    {
        if kind == PlantKind::Imitator {
            return Err(PlantFixerError::ImitatorTarget);
        }
        if !fix_threshold.is_finite() || fix_threshold < 0.0 {
            return Err(PlantFixerError::InvalidThreshold(fix_threshold));
        }

        let mut grids = parse_grids(grids)?;
        {
            for grid in grids.iter().copied() {
                validate_grid(kind, grid)?;
            }

            let seed_slots =
                seed_slots_for(kind, use_imitator).map_err(|error| PlantFixerError::Backend(error.into()))?;
            let coffee_seed_slot = find_seed_slot(CardSelection::Plant(PlantKind::CoffeeBean));

            if self.check_cards && seed_slots.iter().all(Option::is_none) {
                return Err(PlantFixerError::MissingSeed(kind));
            }

            if grids.is_empty() {
                grids = auto_grids_for(kind).map_err(|error| PlantFixerError::Backend(error.into()))?;
            }
            let fix_hp = threshold_to_hp(kind, fix_threshold)?;

            self.kind = Some(kind);
            self.seed_slots = seed_slots;
            self.coffee_seed_slot = coffee_seed_slot;
            self.grids = grids;
            self.fix_hp = fix_hp;
            self.paused = false;
            Ok(())
        }
    }

    pub fn tick(&mut self) -> Result<(), PlantFixerError>
    where
        rsvz_current::CurrentBackend: PlantFixerBackend,
    {
        self._run()
    }

    pub fn set_check_cards(&mut self, enabled: bool) {
        self.check_cards = enabled;
    }

    pub fn set_not_interrupt(&mut self, enabled: bool) {
        self.not_interrupt = enabled;
    }

    pub fn set_skip_covered_bottom(&mut self, enabled: bool) {
        self.skip_covered_bottom = enabled;
    }

    pub fn set_sun_threshold(&mut self, sun: u32) {
        self.sun_threshold = sun;
    }

    pub fn set_run_interval(&mut self, interval: u32) -> Result<(), PlantFixerError> {
        if interval == 0 {
            return Err(PlantFixerError::InvalidRunInterval(interval));
        }
        self.run_interval = interval;
        Ok(())
    }

    pub fn set_list<I, G>(&mut self, grids: I)
    where
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    {
        self.grids = grids.into_iter().map(IntoGrid::into_grid).collect();
    }

    pub fn set_hp_threshold(&mut self, hp: u32) {
        self.fix_hp = hp.min(i32::MAX as u32) as i32;
    }

    pub fn set_hp(&mut self, hp: i32) {
        self.fix_hp = hp;
    }

    pub fn auto_set_list(&mut self) -> Result<(), PlantFixerError>
    where
        rsvz_current::CurrentBackend: PlantReadBackend,
    {
        let Some(kind) = self.kind else {
            self.grids.clear();
            return Ok(());
        };
        self.grids = auto_grids_for(kind).map_err(|error| PlantFixerError::Backend(error.into()))?;
        Ok(())
    }

    pub fn erase_from_list<I, G>(&mut self, grids: I)
    where
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    {
        let remove = grids.into_iter().map(IntoGrid::into_grid).collect::<Vec<_>>();
        self.grids.retain(|grid| !remove.contains(grid));
    }

    pub fn move_to_list_top<I, G>(&mut self, grids: I)
    where
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    {
        let mut items = grids.into_iter().map(IntoGrid::into_grid).collect::<Vec<_>>();
        self.grids.retain(|grid| !items.contains(grid));
        items.extend(self.grids.iter().copied());
        self.grids = items;
    }

    pub fn move_to_list_bottom<I, G>(&mut self, grids: I)
    where
        I: IntoIterator<Item = G>,
        G: IntoGrid,
    {
        let items = grids.into_iter().map(IntoGrid::into_grid).collect::<Vec<_>>();
        self.grids.retain(|grid| !items.contains(grid));
        self.grids.extend(items);
    }

    pub fn set_use_coffee(&mut self, enabled: bool) {
        self.use_coffee = enabled;
    }

    pub fn pause(&mut self) {
        self.paused = true;
    }

    /// Logs attempted repairs and their native outcomes. Disabled by default;
    /// cooldown polling and positions not selected for repair produce no records.
    pub fn set_trace(&mut self, enabled: bool) {
        self.trace = enabled;
    }

    pub fn resume(&mut self) {
        self.paused = false;
    }

    #[must_use]
    pub fn list(&self) -> &[Grid] {
        &self.grids
    }

    #[must_use]
    pub const fn hp_threshold(&self) -> i32 {
        self.fix_hp
    }

    fn _run(&mut self) -> Result<(), PlantFixerError>
    where
        rsvz_current::CurrentBackend: PlantFixerBackend,
    {
        if self.paused || self.grids.is_empty() {
            return Ok(());
        }
        let Some(kind) = self.kind else {
            return Ok(());
        };
        crate::access::with_backend(|backend| {
            let clock = backend
                .clock()
                .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
            if i64::from(clock) % i64::from(self.run_interval) != 0 {
                return Ok(());
            }
            if self.not_interrupt
                && backend
                    .cursor_type()
                    .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()))
                    != 0
            {
                return Ok(());
            }
            if crate::live_value::read_or_abort(backend.sun(), "sun") < self.sun_threshold {
                return Ok(());
            }
            if self.use_coffee && !self.seed_can_use(self.coffee_seed_slot)? {
                return Ok(());
            }
            if upgrade_base(kind).is_some() {
                if !self.seed_can_use(self.seed_slots[2])? {
                    return Ok(());
                }
            }

            let mut usable = [false; 3];
            for (index, slot) in self.seed_slots.iter().enumerate() {
                usable[index] = self.seed_can_use(*slot)?;
            }
            if !usable.iter().any(|&value| value) {
                return Ok(());
            }

            let mut cells = EMPTY_PLANT_FIXER_CELLS;
            for &grid in &self.grids {
                let Some(index) = plant_fixer_cell_index(grid) else {
                    continue;
                };
                backend
                    .for_each_plant_at_anchor_grid(grid, |plant| {
                        if backend.plant_kind(plant)? == kind {
                            if cells[index].plant.is_none() {
                                cells[index].plant = Some(crate::plant::view_from_handle(backend, plant));
                            }
                        } else {
                            cells[index].covered = true;
                        }
                        Ok(())
                    })
                    .map_err(|error| PlantFixerError::Backend(error.into()))?;
            }

            let mut min_hp = None;
            let mut eligible = EMPTY_PLANT_FIXER_ELIGIBLE;
            for grid in self.grids.iter().copied() {
                let cell = plant_fixer_cell(&cells, grid);
                if self.skip_covered_bottom && bottom_kind(kind) && cell.covered {
                    continue;
                }
                if kind == PlantKind::CobCannon
                    && let Some(cob) = cell.plant
                {
                    let recover = cob_recover_time(cob.id);
                    if i64::from(recover) <= 625 + 751 + i64::from(self.run_interval) || recover >= 3475 - (205 + 1) {
                        continue;
                    }
                }
                // Missing targets must have a legal planting action. An existing target
                // is replaced by repair_grid, so its own occupancy is not a rejection.
                if cell.plant.is_none()
                    && !self.seed_slots.iter().enumerate().any(|(index, slot)| {
                        usable[index] && slot.is_some_and(|slot| can_plant_seed(slot, grid).is_allowed())
                    })
                {
                    continue;
                }
                if let Some(index) = plant_fixer_cell_index(grid) {
                    eligible[index] = true;
                }
                let hp = cell.plant.map_or(0, |plant| plant.hp);
                min_hp = Some(min_hp.map_or(hp, |current: i32| current.min(hp)));
            }
            let Some(min_hp) = min_hp else {
                return Ok(());
            };
            if min_hp > self.fix_hp {
                return Ok(());
            }

            for grid in self.grids.iter().copied() {
                if !plant_fixer_cell_index(grid).is_some_and(|index| eligible[index]) {
                    continue;
                }
                let cell = plant_fixer_cell(&cells, grid);
                if cell.plant.map_or(0, |plant| plant.hp) != min_hp {
                    continue;
                }
                let upgraded = self.repair_grid(kind, grid)?;
                if kind == PlantKind::CobCannon && !upgraded {
                    self.repair_grid(
                        kind,
                        Grid {
                            row: grid.row,
                            col: grid.col + 1,
                        },
                    )?;
                }
            }
            Ok(())
        })
    }

    // A completed cannon upgrade already occupies both cells; do not repair its right cell again.
    fn repair_grid(&self, kind: PlantKind, grid: Grid) -> Result<bool, PlantFixerError>
    where
        rsvz_current::CurrentBackend: PlantFixerBackend,
    {
        for slot in self.seed_slots {
            let Some(slot) = slot else {
                continue;
            };
            if !self.seed_can_use(Some(slot))? {
                continue;
            }
            if kind != PlantKind::CoffeeBean {
                crate::modifier::remove_plant_kind_at(kind, grid)
                    .map_err(|error| PlantFixerError::Backend(error.into()))?;
            }
            let sun_before = self.trace.then(|| {
                crate::access::with_backend(|backend| crate::live_value::read_or_abort(backend.sun(), "repair sun"))
            });
            let outcome = try_plant_seed(slot, grid);
            if let Some(sun_before) = sun_before {
                let sun_after = crate::access::with_backend(|backend| {
                    crate::live_value::read_or_abort(backend.sun(), "repair sun")
                });
                crate::diagnostics::log(
                    crate::diagnostics::LogLevel::Debug,
                    format_args!(
                        "plant_repair worker={} epoch={} rounds={} target={kind:?} grid=({}, {}) slot={} outcome={outcome:?} sun_before={} sun_after={}",
                        crate::session::session_shard().index,
                        crate::session::world_epoch(),
                        crate::session::completed_rounds(),
                        grid.row + 1,
                        grid.col + 1,
                        slot.index(),
                        sun_before,
                        sun_after
                    ),
                );
            }
            if !matches!(outcome, PlantSeedOutcome::Planted(_)) {
                continue;
            }
            if self.use_coffee {
                self.use_coffee_if_possible(grid)?;
            }
            return Ok(Some(slot) == self.seed_slots[2]);
        }
        Ok(false)
    }

    fn use_coffee_if_possible(&self, grid: Grid) -> Result<(), PlantFixerError>
    where
        rsvz_current::CurrentBackend: PlantFixerBackend,
    {
        {
            let Some(slot) = self.coffee_seed_slot else {
                return Ok(());
            };
            if !self.seed_can_use(Some(slot))? {
                return Ok(());
            }
            let _outcome = try_plant_seed(slot, grid);
            Ok(())
        }
    }

    fn seed_can_use(&self, slot: Option<SeedSlot>) -> Result<bool, PlantFixerError>
    where
        rsvz_current::CurrentBackend: SeedPacketBackend + PlantCostBackend + SunQueryBackend,
    {
        crate::access::with_backend(|backend| {
            let Some(slot) = slot else {
                return Ok(false);
            };
            if !seed_slot_usable(slot) {
                return Ok(false);
            }
            let Some(cost) = seed_slot_current_cost(slot) else {
                return Ok(false);
            };
            Ok(cost <= crate::live_value::read_or_abort(backend.sun(), "sun"))
        })
    }
}

impl Default for PlantFixer {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_grids<I, G>(grids: I) -> Result<Vec<Grid>, PlantFixerError>
where
    I: IntoIterator<Item = G>,
    G: IntoGrid,
{
    let mut parsed = Vec::new();
    for grid in grids {
        let grid = grid.try_into_grid().map_err(|_error| PlantFixerError::InvalidGrid {
            grid: Grid { row: -1, col: -1 },
            reason: PlantFixerGridError::NegativeCoordinate,
        })?;
        if grid.row < 0 || grid.col < 0 {
            return Err(PlantFixerError::InvalidGrid {
                grid,
                reason: PlantFixerGridError::NegativeCoordinate,
            });
        }
        parsed.push(grid);
    }
    Ok(parsed)
}

fn validate_grid(kind: PlantKind, grid: Grid) -> Result<(), PlantFixerError>
where
    rsvz_current::CurrentBackend: SceneBackend,
{
    crate::access::with_backend(|backend| {
        if grid.row < 0 || grid.col < 0 {
            return Err(PlantFixerError::InvalidGrid {
                grid,
                reason: PlantFixerGridError::NegativeCoordinate,
            });
        }
        let field = rsvz_model::FieldInfo::from_scene(
            backend
                .scene()
                .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into())),
        );
        if !grid.is_in_bounds(field.row_count, field.col_count) {
            return Err(PlantFixerError::InvalidGrid {
                grid,
                reason: PlantFixerGridError::OutOfScene,
            });
        }
        if kind == PlantKind::CobCannon
            && !(Grid {
                row: grid.row,
                col: grid.col + 1,
            })
            .is_in_bounds(field.row_count, field.col_count)
        {
            return Err(PlantFixerError::InvalidGrid {
                grid,
                reason: PlantFixerGridError::CobRightCellOutOfBounds,
            });
        }
        Ok(())
    })
}

fn auto_grids_for(kind: PlantKind) -> Result<Vec<Grid>, rsvz_current::CurrentBackendError>
where
    rsvz_current::CurrentBackend: PlantReadBackend,
{
    crate::access::with_backend(|backend| {
        let mut grids = Vec::new();
        for plant in crate::live_value::read_or_abort(backend.plants(), "plants") {
            if crate::live_value::read_or_abort(backend.plant_kind(plant), "plant_kind") == kind {
                grids.push(crate::plant::grid_from_handle(backend, plant));
            }
        }
        Ok(grids)
    })
}

fn seed_slots_for(
    kind: PlantKind, use_imitator: bool,
) -> Result<[Option<SeedSlot>; 3], rsvz_current::CurrentBackendError>
where
    rsvz_current::CurrentBackend: SeedPacketBackend,
{
    {
        let pre = pre_plant(kind);
        Ok([
            find_seed_slot(CardSelection::Plant(pre)),
            if use_imitator {
                find_seed_slot(CardSelection::Imitator(pre))
            } else {
                None
            },
            if upgrade_base(kind).is_some() {
                find_seed_slot(CardSelection::Plant(kind))
            } else {
                None
            },
        ])
    }
}

fn threshold_to_hp(kind: PlantKind, threshold: f32) -> Result<i32, PlantFixerError> {
    if !threshold.is_finite() || threshold < 0.0 {
        return Err(PlantFixerError::InvalidThreshold(threshold));
    }
    if threshold > 1.0 {
        Ok(threshold as i32)
    } else {
        Ok((max_hp(kind) as f32 * threshold) as i32)
    }
}

const PLANT_FIXER_ROWS: usize = 6;
const PLANT_FIXER_COLS: usize = 9;
const PLANT_FIXER_CELL_COUNT: usize = PLANT_FIXER_ROWS * PLANT_FIXER_COLS;
const EMPTY_PLANT_FIXER_ELIGIBLE: [bool; PLANT_FIXER_CELL_COUNT] = [false; PLANT_FIXER_CELL_COUNT];
// Keep array-size evaluation outside a possibly unsatisfied backend bound.
const EMPTY_PLANT_FIXER_CELLS: [PlantFixerCell; PLANT_FIXER_CELL_COUNT] =
    [PlantFixerCell::EMPTY; PLANT_FIXER_CELL_COUNT];

#[derive(Clone, Copy)]
struct PlantFixerCell {
    plant: Option<GridPlantView>,
    covered: bool,
}

impl PlantFixerCell {
    const EMPTY: Self = Self {
        plant: None,
        covered: false,
    };
}

fn plant_fixer_cell_index(grid: Grid) -> Option<usize> {
    let row = usize::try_from(grid.row).ok()?;
    let col = usize::try_from(grid.col).ok()?;
    (row < PLANT_FIXER_ROWS && col < PLANT_FIXER_COLS).then_some(row * PLANT_FIXER_COLS + col)
}

fn plant_fixer_cell(cells: &[PlantFixerCell; PLANT_FIXER_CELL_COUNT], grid: Grid) -> PlantFixerCell {
    plant_fixer_cell_index(grid).map_or(PlantFixerCell::EMPTY, |index| cells[index])
}

const fn bottom_kind(kind: PlantKind) -> bool {
    matches!(kind, PlantKind::LilyPad | PlantKind::FlowerPot)
}

const fn pre_plant(kind: PlantKind) -> PlantKind {
    match upgrade_base(kind) {
        Some(base) => base,
        None => kind,
    }
}

const fn upgrade_base(kind: PlantKind) -> Option<PlantKind> {
    match kind {
        PlantKind::GatlingPea => Some(PlantKind::Repeater),
        PlantKind::TwinSunflower => Some(PlantKind::Sunflower),
        PlantKind::GloomShroom => Some(PlantKind::FumeShroom),
        PlantKind::Cattail => Some(PlantKind::LilyPad),
        PlantKind::WinterMelon => Some(PlantKind::MelonPult),
        PlantKind::GoldMagnet => Some(PlantKind::MagnetShroom),
        PlantKind::Spikerock => Some(PlantKind::Spikeweed),
        PlantKind::CobCannon => Some(PlantKind::KernelPult),
        _ => None,
    }
}

const fn max_hp(kind: PlantKind) -> i32 {
    match kind {
        PlantKind::WallNut | PlantKind::Pumpkin => 4000,
        PlantKind::TallNut => 8000,
        PlantKind::Garlic => 400,
        PlantKind::Spikerock => 450,
        _ => 300,
    }
}

#[cfg(test)]
mod tests;
