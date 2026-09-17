//! Current SmartRemove execution and reusable selection state.

use crate::runtime::RuntimeError;
use rsvz_current::CurrentBackend;

use crate::backend::{
    PlantReadBackend, PlantRemoveBackend, PlantVisualStateBackend, ZombieRawFactsBackend, ZombieReadBackend,
};
use crate::logic::ContactGeometryQueryError;
use crate::logic::contact::{
    attack_reaches_defense, plant_defense_bounds_from_handle, zombie_attack_bounds_from_handle,
};
use crate::logic::{ContactGeometryBackend, same_row_and_attack_reaches_plant_geometry};
use crate::model::{AbsoluteContactRange, Grid, PlantDefenseBounds, PlantId, ZombieAttackBounds, ZombieId};

use crate::model::{PlantDefenseKind, ZombieKind, ZombiePhase};
use crate::model::{PlantKind, SmartRemoveGridPlantLayer as Layer};

const ROWS: usize = 6;
const COLS: usize = 9;
const LAYERS: usize = 5;
const EMPTY_DEFENSE_RANGES: [Option<AbsoluteContactRange>; ROWS * COLS] = [None; ROWS * COLS];
const SMART_REMOVE_HAMMER_START: f32 = 0.641;
const SMART_REMOVE_HAMMER_SLOW_SWITCH: f32 = 0.643;
const SMART_REMOVE_HAMMER_END: f32 = 0.645;
const SMART_REMOVE_LAST_FRAME_LOWER: f32 = 0.639;

/// Reusable SmartRemove scratch storage. Threat geometry is rebuilt on every tick;
/// it is never reused after a backend update. No native addresses are retained.
#[derive(Debug)]
pub struct SmartRemoveState {
    plant_map: [[Option<PlantId>; LAYERS]; ROWS * COLS],
    hammer_vec: Vec<ZombieAttackBounds>,
    crush_vec: Vec<ZombieAttackBounds>,
    sparkle_vec: Vec<ZombieAttackBounds>,
    is_sparkle: bool,
}

impl Default for SmartRemoveState {
    fn default() -> Self {
        Self {
            plant_map: [[None; LAYERS]; ROWS * COLS],
            hammer_vec: Vec::new(),
            crush_vec: Vec::new(),
            sparkle_vec: Vec::new(),
            is_sparkle: false,
        }
    }
}

impl SmartRemoveState {
    /// Mirrors SmartRemove.h `SetSparkle`.
    pub fn set_highlight(&mut self, highlight: bool) {
        self.is_sparkle = highlight;
    }

    #[must_use]
    pub fn highlight(&self) -> bool {
        self.is_sparkle
    }

    fn get(&self, grid: Grid, layer: Layer) -> Option<PlantId> {
        let cell = cell_index(grid)?;
        let layer = layer_index(layer)?;
        self.plant_map[cell][layer]
    }
}

/// Error returned by SmartRemove tick execution.
#[derive(Debug, thiserror::Error)]
pub enum SmartRemoveError {
    #[error("SmartRemove backend error: {0}")]
    Backend(RuntimeError),
    #[error("SmartRemove contact geometry error: {0}")]
    ContactGeometry(#[from] ContactGeometryQueryError),
    #[error("SmartRemove plant disappeared during tick: {0:?}")]
    MissingPlant(PlantId),
    #[error("SmartRemove zombie disappeared during tick: {0:?}")]
    MissingZombie(ZombieId),
    #[error("SmartRemove invalid plant grid: {0:?}")]
    InvalidPlantGrid(Grid),
}

/// Executes the removal rules without requiring cosmetic plant-write capabilities.
pub fn tick_smart_remove(state: &mut SmartRemoveState) -> Result<(), SmartRemoveError>
where
    CurrentBackend: PlantReadBackend + ZombieReadBackend + ContactGeometryBackend + PlantRemoveBackend,
{
    tick_with_highlighter(state, |_, _, _| Ok(()))
}

/// Executes the same scan with optional native highlighting.
pub fn tick_smart_remove_with_visuals(state: &mut SmartRemoveState) -> Result<(), SmartRemoveError>
where
    CurrentBackend:
        PlantReadBackend + ZombieReadBackend + ContactGeometryBackend + PlantRemoveBackend + PlantVisualStateBackend,
{
    tick_with_highlighter(state, set_flash_and_update_color)
}

fn tick_with_highlighter<F>(state: &mut SmartRemoveState, highlight: F) -> Result<(), SmartRemoveError>
where
    CurrentBackend: PlantReadBackend + ZombieReadBackend + ContactGeometryBackend + PlantRemoveBackend,
    F: for<'a> FnMut(
        &'a CurrentBackend,
        <CurrentBackend as PlantReadBackend>::PlantHandle<'a>,
        i32,
    ) -> Result<(), SmartRemoveError>,
{
    crate::access::with_backend(|backend| {
        update_zombie(backend, state)?;
        if !state.is_sparkle && state.hammer_vec.is_empty() && state.crush_vec.is_empty() {
            return Ok(());
        }
        let ranges = update_plant_map(backend, state)?;
        let candidates = candidate_cells(
            &ranges,
            state
                .hammer_vec
                .iter()
                .chain(&state.crush_vec)
                .chain(&state.sparkle_vec),
        );
        run(backend, state, candidates, highlight)
    })
}

fn defense<'a>(
    backend: &'a CurrentBackend, plant: Option<PlantId>,
) -> Result<Option<PlantDefenseBounds>, SmartRemoveError>
where
    CurrentBackend: ContactGeometryBackend,
{
    let Some(id) = plant else {
        return Ok(None);
    };
    let handle = require_plant(id, crate::live_value::read_or_abort(backend.plant(id), "plant"))?;
    Ok(Some(
        plant_defense_bounds_from_handle(backend, handle, PlantDefenseKind::ChewCrushSmash)?
            .ok_or(SmartRemoveError::MissingPlant(id))?,
    ))
}

fn hits(attack: ZombieAttackBounds, plant: Option<PlantDefenseBounds>) -> bool {
    plant.is_some_and(|plant| same_row_and_attack_reaches_plant_geometry(attack, plant))
}

// Hammer rules veto removal when any matching hammer reaches every protected layer.
// Vehicle rules instead remove when any one vehicle can be evaded.
fn hammer_allows_removal(
    attacks: &[ZombieAttackBounds], own: PlantDefenseBounds, base: Option<PlantDefenseBounds>,
    common: Option<PlantDefenseBounds>, pumpkin: bool,
) -> bool {
    let mut remove = false;
    for &attack in attacks {
        if !hits(attack, Some(own)) {
            continue;
        }
        if hits(attack, base) && (!pumpkin || hits(attack, common)) {
            return false;
        }
        remove = true;
    }
    remove
}

fn candidate_cells<'a>(
    ranges: &[Option<AbsoluteContactRange>; ROWS * COLS], attacks: impl Iterator<Item = &'a ZombieAttackBounds>,
) -> u64 {
    let mut mask = 0;
    for attack in attacks {
        let Ok(row) = usize::try_from(attack.row) else {
            continue;
        };
        if row >= ROWS {
            continue;
        }
        for col in 0..COLS {
            let cell = row * COLS + col;
            if ranges[cell].is_some_and(|range| attack_reaches_defense(attack.range, range)) {
                mask |= 1u64 << cell;
            }
        }
    }
    mask
}

fn plant_disappears_immediately<'a>(
    backend: &'a CurrentBackend, plant: <CurrentBackend as PlantReadBackend>::PlantHandle<'a>,
) -> Result<bool, SmartRemoveError>
where
    CurrentBackend: PlantReadBackend,
{
    let kind = backend
        .plant_raw_kind(plant)
        .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
    Ok(match kind {
        PlantKind::CherryBomb | PlantKind::Jalapeno | PlantKind::DoomShroom => {
            backend.plant_effect_countdown(plant) == 1
        }
        PlantKind::Blover => backend.plant_state(plant) == 2 && backend.plant_disappear_countdown(plant) == 1,
        PlantKind::Squash => backend.plant_state(plant) == 4 && backend.plant_state_countdown(plant) == 1,
        PlantKind::GraveBuster => backend.plant_state(plant) == 9 && backend.plant_state_countdown(plant) == 1,
        _ => false,
    })
}

fn is_certain_tick_ice(backend: &CurrentBackend) -> bool
where
    CurrentBackend: PlantReadBackend,
{
    crate::live_value::read_or_abort(backend.plants(), "plants").any(|plant| {
        backend
            .plant_raw_kind(plant)
            .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()))
            == PlantKind::IceShroom
            && backend.plant_effect_countdown(plant) == 1
    })
}

fn update_plant_map(
    backend: &CurrentBackend, state: &mut SmartRemoveState,
) -> Result<[Option<AbsoluteContactRange>; ROWS * COLS], SmartRemoveError>
where
    CurrentBackend: ContactGeometryBackend,
{
    state.plant_map.fill([None; LAYERS]);
    let mut ranges = EMPTY_DEFENSE_RANGES;
    for plant in crate::live_value::read_or_abort(backend.plants(), "plants") {
        let id = backend.plant_id(plant);
        let kind = backend
            .plant_raw_kind(plant)
            .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
        let grid = crate::plant::grid_from_handle(backend, plant);
        let layer = classify_plant_layer(
            kind,
            if kind == PlantKind::Squash {
                backend.plant_state(plant)
            } else {
                0
            },
        );
        let cell = cell_index(grid).ok_or(SmartRemoveError::InvalidPlantGrid(grid))?;
        let slot = layer_index(layer).ok_or(SmartRemoveError::InvalidPlantGrid(grid))?;
        state.plant_map[cell][slot] = Some(id);
        // Only types that can pass run's removal gate contribute to the coarse envelope.
        if kind != PlantKind::Pumpkin
            && kind != PlantKind::TallNut
            && !(matches!(kind, PlantKind::PuffShroom | PlantKind::SunShroom)
                && (1..=4).contains(&(backend.plant_x(plant) % 10)))
        {
            continue;
        }
        if let Some(defense) = plant_defense_bounds_from_handle(backend, plant, PlantDefenseKind::ChewCrushSmash)? {
            // A conservative union uses actual positions, including displaced plants and multiple layers.
            let range = defense.range;
            ranges[cell] = Some(ranges[cell].map_or(range, |old| {
                AbsoluteContactRange::new(old.left.min(range.left), old.right.max(range.right))
            }));
        }
    }
    Ok(ranges)
}

fn update_zombie(backend: &CurrentBackend, state: &mut SmartRemoveState) -> Result<(), SmartRemoveError>
where
    CurrentBackend: ContactGeometryBackend,
{
    state.crush_vec.clear();
    state.hammer_vec.clear();
    state.sparkle_vec.clear();
    let mut can_ice4 = None;
    for zombie in crate::live_value::read_or_abort(backend.zombies(), "zombies") {
        let kind = backend
            .zombie_kind(zombie)
            .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
        if !matches!(
            kind,
            ZombieKind::Gargantuar | ZombieKind::GigaGargantuar | ZombieKind::Catapult | ZombieKind::Zomboni
        ) {
            continue;
        }
        let phase = backend
            .zombie_phase(zombie)
            .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
        let action = if matches!(kind, ZombieKind::Catapult | ZombieKind::Zomboni) {
            if phase != ZombiePhase::ZombieNormal {
                continue;
            }
            None
        } else {
            if phase != ZombiePhase::GargantuarSmashing {
                continue;
            }
            let id = backend.zombie_id(zombie);
            let cr = backend
                .zombie_reanim_anim_time(zombie)
                .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()))
                .ok_or(SmartRemoveError::MissingZombie(id))?;
            if !state.is_sparkle && !(SMART_REMOVE_HAMMER_START < cr && cr < SMART_REMOVE_HAMMER_END) {
                continue;
            }
            let last = if SMART_REMOVE_HAMMER_SLOW_SWITCH < cr && cr < SMART_REMOVE_HAMMER_END {
                backend
                    .zombie_reanim_last_time(zombie)
                    .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()))
                    .ok_or(SmartRemoveError::MissingZombie(id))?
            } else {
                0.0
            };
            let frozen = backend.zombie_frozen_countdown(zombie) != 0;
            let mut action = gargantuar_action(cr, last, frozen, false, state.is_sparkle);
            if action == GargantuarAction::Hammer && *can_ice4.get_or_insert_with(|| is_certain_tick_ice(backend)) {
                action = gargantuar_action(cr, last, frozen, true, state.is_sparkle);
            }
            if action == GargantuarAction::None {
                continue;
            }
            Some(action)
        };
        let attack = zombie_attack_bounds_from_handle(backend, zombie)?
            .ok_or_else(|| SmartRemoveError::MissingZombie(backend.zombie_id(zombie)))?;
        match action {
            None => state.crush_vec.push(attack),
            Some(GargantuarAction::Hammer) => state.hammer_vec.push(attack),
            Some(GargantuarAction::Sparkle) => state.sparkle_vec.push(attack),
            Some(GargantuarAction::None) => unreachable!(),
        }
    }
    Ok(())
}

fn set_flash_and_update_color<'a>(
    backend: &'a CurrentBackend, plant: <CurrentBackend as PlantReadBackend>::PlantHandle<'a>, value: i32,
) -> Result<(), SmartRemoveError>
where
    CurrentBackend: PlantVisualStateBackend,
{
    backend
        .set_plant_eating_flash_counter(plant, value)
        .map_err(|error| SmartRemoveError::Backend(crate::access::operation_error(error)))?;
    backend
        .update_plant_reanim_color(plant)
        .map_err(|error| SmartRemoveError::Backend(crate::access::operation_error(error)))
}

fn remove_plant_and_clear_map<'a>(
    backend: &'a CurrentBackend, state: &mut SmartRemoveState, grid: Grid, layer: Layer,
    handle: <CurrentBackend as PlantReadBackend>::PlantHandle<'a>,
) -> Result<(), SmartRemoveError>
where
    CurrentBackend: PlantRemoveBackend,
{
    let plant = backend.plant_id(handle);
    backend
        .remove_plant(handle)
        .map_err(|error| SmartRemoveError::Backend(crate::access::operation_error(error)))?;
    clear_removed_plant_from_map(state, grid, layer, plant);
    Ok(())
}

fn right_stack_index(backend: &CurrentBackend, state: &SmartRemoveState, grid: Grid) -> Result<i32, SmartRemoveError>
where
    CurrentBackend: ContactGeometryBackend,
{
    if grid.col >= COLS as i32 - 1 {
        return Ok(-1);
    }
    let right = Grid {
        row: grid.row,
        col: grid.col + 1,
    };
    let common = state
        .get(right, Layer::Common)
        .map(|id| require_plant(id, crate::live_value::read_or_abort(backend.plant(id), "plant")))
        .transpose()?;
    if let Some(common) = common
        && backend
            .plant_raw_kind(common)
            .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()))
            == PlantKind::DoomShroom
        && backend.plant_effect_countdown(common) == 1
    {
        return Ok(-1);
    }
    let plant = if let Some(pumpkin) = state.get(right, Layer::Pumpkin) {
        Some(pumpkin)
    } else if let Some(common) = common
        && !plant_disappears_immediately(backend, common)?
    {
        Some(backend.plant_id(common))
    } else {
        state.get(right, Layer::Container)
    };
    let Some(plant) = plant else {
        return Ok(-1);
    };
    let defense = defense(backend, Some(plant))?;
    if state
        .hammer_vec
        .iter()
        .filter(|&&attack| hits(attack, defense))
        .take(2)
        .count()
        == 2
    {
        return Ok(-1);
    }
    Ok(i32::from(plant.index()))
}

fn run<F>(
    backend: &CurrentBackend, state: &mut SmartRemoveState, candidates: u64, mut highlight: F,
) -> Result<(), SmartRemoveError>
where
    CurrentBackend: PlantReadBackend + ZombieReadBackend + ContactGeometryBackend + PlantRemoveBackend,
    F: for<'a> FnMut(
        &'a CurrentBackend,
        <CurrentBackend as PlantReadBackend>::PlantHandle<'a>,
        i32,
    ) -> Result<(), SmartRemoveError>,
{
    // No update, allocation of native objects or user callback occurs here. Iteration retains native order.
    for handle in crate::live_value::read_or_abort(backend.plants(), "plants") {
        if state.is_sparkle && backend.plant_eating_flash_counter(handle) == 59 {
            highlight(backend, handle, 0)?;
        }
        let kind = backend
            .plant_raw_kind(handle)
            .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
        let pumpkin = kind == PlantKind::Pumpkin;
        if !pumpkin
            && kind != PlantKind::TallNut
            && !(matches!(kind, PlantKind::PuffShroom | PlantKind::SunShroom)
                && (1..=4).contains(&(backend.plant_x(handle) % 10)))
        {
            continue;
        }
        let grid = crate::plant::grid_from_handle(backend, handle);
        let cell = cell_index(grid).ok_or(SmartRemoveError::InvalidPlantGrid(grid))?;
        if candidates & (1u64 << cell) == 0 {
            continue;
        }
        let base_id = state.get(grid, Layer::Container);
        let common_id = state.get(grid, Layer::Common);
        if base_id.is_none() && (!pumpkin || common_id.is_none()) {
            continue;
        }
        let id = backend.plant_id(handle);
        let own = plant_defense_bounds_from_handle(backend, handle, PlantDefenseKind::ChewCrushSmash)?
            .ok_or(SmartRemoveError::MissingPlant(id))?;
        let base = defense(backend, base_id)?;
        let common = if pumpkin { defense(backend, common_id)? } else { None };
        let remove = hammer_allows_removal(&state.hammer_vec, own, base, common, pumpkin);
        let sparkle = state.is_sparkle && hammer_allows_removal(&state.sparkle_vec, own, base, common, pumpkin);
        let layer = if pumpkin { Layer::Pumpkin } else { Layer::Common };
        if remove || sparkle {
            // Read the right target after earlier removals, preserving the original pool-order decisions.
            let right = right_stack_index(backend, state, grid)?;
            let stack = i32::from(id.index());
            let pumpkin_stack = state.get(grid, Layer::Pumpkin).map_or(-1, |id| i32::from(id.index()));
            let allowed = right == -1 || (stack < right && (pumpkin || pumpkin_stack == -1 || pumpkin_stack < right));
            if allowed {
                if remove {
                    remove_plant_and_clear_map(backend, state, grid, layer, handle)?;
                    continue;
                }
                highlight(backend, handle, 60)?;
            }
        }
        if state
            .crush_vec
            .iter()
            .any(|&attack| hits(attack, Some(own)) && (!hits(attack, base) || (pumpkin && !hits(attack, common))))
        {
            remove_plant_and_clear_map(backend, state, grid, layer, handle)?;
        }
    }
    Ok(())
}

fn clear_removed_plant_from_map(state: &mut SmartRemoveState, grid: Grid, layer: Layer, plant: PlantId) {
    let (Some(cell), Some(layer)) = (cell_index(grid), layer_index(layer)) else {
        return;
    };
    if state.plant_map[cell][layer] == Some(plant) {
        state.plant_map[cell][layer] = None;
    }
}

fn classify_plant_layer(kind: PlantKind, state: i32) -> Layer {
    match kind {
        PlantKind::LilyPad | PlantKind::FlowerPot => Layer::Container,
        PlantKind::Pumpkin => Layer::Pumpkin,
        PlantKind::CoffeeBean => Layer::Coffee,
        PlantKind::Squash if state == 5 || state == 6 => Layer::Fly,
        _ => Layer::Common,
    }
}

fn cell_index(grid: Grid) -> Option<usize> {
    let row = usize::try_from(grid.row).ok()?;
    let col = usize::try_from(grid.col).ok()?;
    if row >= ROWS || col >= COLS {
        return None;
    }
    Some(row * COLS + col)
}

fn layer_index(layer: Layer) -> Option<usize> {
    match layer {
        Layer::Container => Some(0),
        Layer::Pumpkin => Some(1),
        Layer::Coffee => Some(2),
        Layer::Common => Some(3),
        Layer::Fly => Some(4),
        Layer::Unknown => None,
    }
}

fn require_plant<T>(plant: PlantId, read: Option<T>) -> Result<T, SmartRemoveError> {
    read.ok_or(SmartRemoveError::MissingPlant(plant))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GargantuarAction {
    Hammer,
    Sparkle,
    None,
}

fn gargantuar_action(cr: f32, cr_last: f32, frozen: bool, can_ice: bool, highlight: bool) -> GargantuarAction {
    let slow_switch = SMART_REMOVE_HAMMER_SLOW_SWITCH < cr
        && cr < SMART_REMOVE_HAMMER_END
        && SMART_REMOVE_LAST_FRAME_LOWER < cr_last
        && cr_last < SMART_REMOVE_HAMMER_START;
    let hammer_window = SMART_REMOVE_HAMMER_START < cr && cr < SMART_REMOVE_HAMMER_SLOW_SWITCH;
    if !frozen && !can_ice && (hammer_window || slow_switch) {
        GargantuarAction::Hammer
    } else if highlight && (cr < SMART_REMOVE_HAMMER_START || (hammer_window && !frozen) || slow_switch) {
        GargantuarAction::Sparkle
    } else {
        GargantuarAction::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_cells_use_actual_ranges_not_nominal_column_positions() {
        let mut ranges = [None; ROWS * COLS];
        ranges[0] = Some(AbsoluteContactRange::new(1000, 1040));
        ranges[9] = ranges[0];
        let attack = ZombieAttackBounds {
            id: ZombieId::from_raw(1),
            kind: ZombieKind::Zomboni,
            row: 0,
            x: 1000,
            range: AbsoluteContactRange::new(1040, 1100),
        };
        assert_eq!(candidate_cells(&ranges, [&attack].into_iter()), 1);
        let outside = ZombieAttackBounds {
            range: AbsoluteContactRange::new(1041, 1100),
            ..attack
        };
        assert_eq!(candidate_cells(&ranges, [&outside].into_iter()), 0);
    }

    #[test]
    fn a_hammer_reaching_all_inner_layers_vetoes_removal_in_either_order() {
        let own = PlantDefenseBounds {
            id: PlantId::from_raw(1),
            kind: PlantKind::Pumpkin,
            defense_kind: PlantDefenseKind::ChewCrushSmash,
            grid: Grid { row: 0, col: 0 },
            x: 0,
            range: AbsoluteContactRange::new(100, 160),
        };
        let inner = PlantDefenseBounds {
            range: AbsoluteContactRange::new(100, 120),
            ..own
        };
        let outer_only = ZombieAttackBounds {
            id: ZombieId::from_raw(1),
            kind: ZombieKind::Gargantuar,
            row: 0,
            x: 150,
            range: AbsoluteContactRange::new(150, 170),
        };
        let both = ZombieAttackBounds {
            range: AbsoluteContactRange::new(110, 130),
            ..outer_only
        };
        assert!(hammer_allows_removal(
            &[outer_only],
            own,
            Some(inner),
            Some(inner),
            true
        ));
        for attacks in [[outer_only, both], [both, outer_only]] {
            assert!(!hammer_allows_removal(&attacks, own, Some(inner), Some(inner), true));
        }
        // Preserve the existing missing-layer semantics rather than changing the rule during optimization.
        assert!(hammer_allows_removal(&[both], own, Some(inner), None, true));
        assert!(!hammer_allows_removal(&[both], own, Some(inner), None, false));
    }

    #[test]
    fn hammer_timing_uses_current_and_last_native_frames_without_prediction() {
        use GargantuarAction::*;
        for (cr, last, frozen, ice, expected) in [
            (0.642, 0.63, false, false, Hammer),
            (0.63, 0.0, false, false, Sparkle),
            (0.63, 0.0, true, false, Sparkle),
            (0.642, 0.63, true, false, None),
            (0.644, 0.6395, true, false, Sparkle),
            (0.642, 0.63, false, true, Sparkle),
            (0.645, 0.63, false, false, None),
        ] {
            assert_eq!(gargantuar_action(cr, last, frozen, ice, true), expected);
        }
    }

    #[test]
    fn clearing_a_removed_id_does_not_erase_a_replacement_at_the_same_grid() {
        let grid = Grid { row: 0, col: 0 };
        let old = PlantId::from_raw(0x10000);
        let replacement = PlantId::from_raw(0x20000);
        let mut state = SmartRemoveState::default();
        state.plant_map[0][1] = Some(replacement);
        clear_removed_plant_from_map(&mut state, grid, Layer::Pumpkin, old);
        assert_eq!(state.get(grid, Layer::Pumpkin), Some(replacement));
        clear_removed_plant_from_map(&mut state, grid, Layer::Pumpkin, replacement);
        assert_eq!(state.get(grid, Layer::Pumpkin), None);
    }

    #[test]
    fn missing_entity_keeps_its_id_and_present_values_pass_through() {
        let id = PlantId::from_raw(0x10000);
        assert!(matches!(require_plant::<()>(id, None), Err(SmartRemoveError::MissingPlant(found)) if found == id));
        assert_eq!(require_plant(id, Some(42)).unwrap(), 42);
    }

    #[test]
    fn plant_layers_and_cell_bounds_keep_the_native_mapping() {
        assert_eq!(classify_plant_layer(PlantKind::Pumpkin, 0), Layer::Pumpkin);
        assert_eq!(classify_plant_layer(PlantKind::Squash, 5), Layer::Fly);
        assert_eq!(classify_plant_layer(PlantKind::Squash, 4), Layer::Common);
        assert_eq!(classify_plant_layer(PlantKind::LilyPad, 0), Layer::Container);
        assert_eq!(cell_index(Grid { row: -1, col: 0 }), None);
        assert_eq!(cell_index(Grid { row: 6, col: 0 }), None);
    }
}
