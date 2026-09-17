use crate::model::types::DEFAULT_COL_COUNT;
use crate::model::{CardSelection, Grid, PlantKind, SceneKind};

use super::cell::{LineupBase, LineupCell, LineupPlant};
use super::error::LineupParseError;
use super::model::Lineup;

impl Lineup {
    /// Validates model invariants without mutating a backend.
    pub fn validate(&self) -> Result<(), LineupParseError> {
        if !self.scene().supports_regular_lineup() {
            return Err(LineupParseError::UnsupportedLineupData(
                "lineup scene is not a regular plant-side board",
            ));
        }
        let expected = self.row_count() * self.col_count();
        if self.cells().len() != expected {
            return Err(LineupParseError::UnexpectedCellCount {
                actual: self.cells().len(),
                expected,
            });
        }
        if let Some(grid) = self.rake_grid() {
            validate_rake_grid(grid, self.scene())?;
        }
        for (grid, cell) in self.iter() {
            validate_cell_layers(self.scene(), grid, *cell)?;
        }
        Ok(())
    }
}

fn validate_cell_layers(scene: SceneKind, grid: Grid, cell: LineupCell) -> Result<(), LineupParseError> {
    match cell.base {
        LineupBase::None => {}
        LineupBase::LilyPad { .. } => {
            if !is_pool_row(scene, grid) {
                return Err(LineupParseError::UnsupportedLineupData(
                    "lily pad base must be on a pool row",
                ));
            }
        }
        // Source tools apply explicitly encoded flower pots on any regular scene.
        LineupBase::FlowerPot { .. } => {}
        // pvztoolkit stores graves independently from plant/layer bits. Keep that source
        // compatibility and let apply_lineup lower the grave grid item after plant layers.
        LineupBase::Grave => {}
    }

    if let Some(plant) = cell.main {
        validate_main_plant(plant)?;
    }
    if let Some(plant) = cell.pumpkin {
        validate_layer_plant(plant, PlantKind::Pumpkin, "pumpkin layer")?;
    }
    if let Some(plant) = cell.coffee {
        validate_layer_plant(plant, PlantKind::CoffeeBean, "coffee layer")?;
    }
    Ok(())
}

fn validate_main_plant(plant: LineupPlant) -> Result<(), LineupParseError> {
    let kind = selection_kind(plant.selection)?;
    if matches!(
        kind,
        PlantKind::LilyPad | PlantKind::FlowerPot | PlantKind::Pumpkin | PlantKind::CoffeeBean
    ) {
        return Err(LineupParseError::UnsupportedLineupData(
            "layer-only plants cannot be encoded as main plants",
        ));
    }
    validate_plant_state(plant, kind)
}

fn validate_layer_plant(plant: LineupPlant, expected: PlantKind, layer: &'static str) -> Result<(), LineupParseError> {
    let kind = selection_kind(plant.selection)?;
    if kind != expected {
        return Err(LineupParseError::UnsupportedLineupData(layer));
    }
    validate_plant_state(plant, kind)
}

fn validate_plant_state(plant: LineupPlant, kind: PlantKind) -> Result<(), LineupParseError> {
    if plant.awake.is_some() && !may_sleep(kind) {
        return Err(LineupParseError::UnsupportedLineupData(
            "awake state is only valid for mushrooms",
        ));
    }
    if plant.grown && !matches!(kind, PlantKind::PotatoMine | PlantKind::SunShroom) {
        return Err(LineupParseError::UnsupportedLineupData(
            "grown state is only valid for potato mine and sun-shroom",
        ));
    }
    Ok(())
}

fn selection_kind(selection: CardSelection) -> Result<PlantKind, LineupParseError> {
    Ok(selection.checked()?.effective_kind())
}

pub(super) fn is_pool_row(scene: SceneKind, grid: Grid) -> bool {
    scene.has_pool() && matches!(grid.row, 2 | 3)
}

pub(super) fn may_sleep(kind: PlantKind) -> bool {
    matches!(
        kind,
        PlantKind::PuffShroom
            | PlantKind::SunShroom
            | PlantKind::FumeShroom
            | PlantKind::HypnoShroom
            | PlantKind::ScaredyShroom
            | PlantKind::IceShroom
            | PlantKind::DoomShroom
            | PlantKind::SeaShroom
            | PlantKind::MagnetShroom
            | PlantKind::GloomShroom
    )
}

pub(super) const fn should_apply_mushroom_awake_state(scene: SceneKind) -> bool {
    !scene.is_night()
}

fn validate_rake_grid(grid: Grid, scene: SceneKind) -> Result<(), LineupParseError> {
    if !grid.is_in_bounds(scene.row_count(), DEFAULT_COL_COUNT) {
        let (row, col) = grid.to_one_based();
        return Err(LineupParseError::InvalidGrid { row, col });
    }
    if is_pool_row(scene, grid) {
        return Err(LineupParseError::UnsupportedLineupData(
            "rake cannot be placed on pool rows",
        ));
    }
    Ok(())
}
