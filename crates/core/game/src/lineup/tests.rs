use crate::model::types::DEFAULT_COL_COUNT;
use crate::model::{CardSelection, Grid, PlantKind, SceneKind};

use super::parse::empty_cells;
use super::*;

const PVZTOOLS_LEGACY_FIXTURE: &str = "0,10 3 1 0 0 0,0 3 1 0 0 0,1E 3 1 0 0 1,23 3 1 0 0 1,32 2 2 0 0 0,1B 2 3 0 0 1,30 2 3 0 0 0,8 5 5 0 0 0,A 5 6 1 0 0,4 6 7 1 0 0,9 6 8 1 2 0,31 2 6 0 0 0";
const PVZTOOLKIT_EMPTY_NIGHT_GOLDEN: &str = "LMg3NPRBVFRUDlRVVQ==";

#[test]
fn mushroom_garden_lineup_retains_native_scene_and_layers() {
    let lineup = parse_lineup("LI5HDH3tBiZ/13rXcldAXJjACH/tATT/lA7S4QTFwRTFwVY0ZZXhbIH0zVdU/ipHCVI=");
    assert_eq!(lineup.scene(), SceneKind::MushroomGarden);
    assert_eq!(lineup.row_count(), 5);
    assert!(lineup.scene().is_night());
    let first = fixture_cell(&lineup, 0, 0);
    assert_eq!(
        fixture_plant(first.main, "sunflower").selection,
        CardSelection::Plant(PlantKind::TwinSunflower)
    );
    assert!(first.pumpkin.is_some());
    assert!(crate::logic::zombies::zombie_row_allowed_for_kind(
        SceneKind::MushroomGarden,
        crate::model::ZombieKind::Zomboni,
        4
    ));
    assert!(!crate::logic::zombies::zombie_row_allowed_for_kind(
        SceneKind::MushroomGarden,
        crate::model::ZombieKind::Dancing,
        0
    ));
}

// Published Fog fixture from lineup-tool/tests/lineup.rs; includes '/' and padding.
const FOG_GOLDEN: &str = "LI5H/ECMfgR99J/0n1RRVsHGZ3Hjdt9sAG74IAxN5GH0ZjRxADBx7lzyelVYx1XCuEBpVw==";

#[test]
fn parses_toolkit_base64_alphabets_and_padding() {
    let url = FOG_GOLDEN.replace('+', "-").replace('/', "_");
    let standard = parse_lineup(FOG_GOLDEN);
    for code in [FOG_GOLDEN, url.as_str(), url.trim_end_matches('=')] {
        assert_eq!(parse_lineup(code), standard);
    }
    assert_eq!(standard.scene(), SceneKind::Fog);
    let first = fixture_cell(&standard, 0, 0);
    assert_eq!(
        fixture_plant(first.main, "pumpkin cell main").selection,
        CardSelection::Plant(PlantKind::Starfruit)
    );
    assert!(first.pumpkin.is_some());
}

#[test]
fn parses_real_pvztoolkit_empty_night_golden() {
    let lineup = parse_lineup(PVZTOOLKIT_EMPTY_NIGHT_GOLDEN);

    assert_eq!(lineup.scene(), SceneKind::Night);
    assert!(lineup.cells().iter().all(|cell| {
        cell.base == LineupBase::None
            && cell.main.is_none()
            && cell.pumpkin.is_none()
            && cell.coffee.is_none()
            && !cell.ladder
    }));
}

#[test]
fn parses_pvztools_legacy_fixture() {
    let lineup = parse_lineup(PVZTOOLS_LEGACY_FIXTURE);
    assert_fixture_lineup(&lineup);
}

#[test]
fn legacy_rake_ignores_column_and_uses_c8() {
    let lineup = parse_lineup(PVZTOOLS_LEGACY_FIXTURE);

    assert_eq!(lineup.rake_grid(), Some(fixture_grid(1, 7)));
}

#[test]
fn night_legacy_mushrooms_do_not_store_awake_mutation() {
    let lineup = parse_lineup("3,8 1 1 1 0 0");
    let cell = fixture_cell(&lineup, 0, 0);

    assert_eq!(fixture_plant(cell.main, "night puff").awake, None);
}

#[test]
fn lineup_derives_row_major_grids_from_cell_indices() {
    let lineup = Lineup::new(SceneKind::Day, empty_cells(SceneKind::Day), None).expect("lineup");
    assert_eq!(
        lineup.iter().map(|(grid, _cell)| grid).take(10).collect::<Vec<_>>(),
        [
            fixture_grid(0, 0),
            fixture_grid(0, 1),
            fixture_grid(0, 2),
            fixture_grid(0, 3),
            fixture_grid(0, 4),
            fixture_grid(0, 5),
            fixture_grid(0, 6),
            fixture_grid(0, 7),
            fixture_grid(0, 8),
            fixture_grid(1, 0),
        ]
    );
}

#[test]
fn rejects_oversized_toolkit_payload_before_unbounded_growth() {
    let code = "LI43NPR3VFRUOVRVVg==";

    assert!(matches!(parse_code_error(&code), LineupParseError::DecompressFailed));
}

#[test]
fn rejects_layer_only_main_plant_from_toolkit_code() {
    let error = parse_code_error("LI43NHRW7GYEUVRUR9tUElY=");

    assert!(
        matches!(error, LineupParseError::UnsupportedLineupData(_)),
        "layer-only main plant should be rejected: {error}"
    );
}

#[test]
fn rejects_lily_pad_base_off_pool_rows() {
    let mut cells = empty_cells(SceneKind::Pool);
    cell_mut(&mut cells, 0, 0).base = LineupBase::LilyPad { imitator: false };

    let error = new_lineup_error(SceneKind::Pool, cells, None);

    assert!(
        matches!(error, LineupParseError::UnsupportedLineupData(_)),
        "lily pad on grass row should be rejected: {error}"
    );
}

#[test]
fn toolkit_source_semantics_force_grown_plants() {
    let lineup = parse_lineup("LI43RDWENPRHVFRNZFRpVg==");

    assert!(fixture_plant(fixture_cell(&lineup, 0, 0).main, "potato").grown);
    assert!(fixture_plant(fixture_cell(&lineup, 0, 1).main, "sunshroom").grown);
}

#[test]
fn legacy_state_fields_distinguish_sunshroom_grown_state() {
    let lineup = parse_lineup("2,4 1 1 0 0 0,9 1 2 0 0 0,9 1 3 1 2 0");

    assert!(fixture_plant(fixture_cell(&lineup, 0, 0).main, "potato").grown);
    assert!(!fixture_plant(fixture_cell(&lineup, 0, 1).main, "sunshroom").grown);
    assert!(fixture_plant(fixture_cell(&lineup, 0, 2).main, "sunshroom").grown);
}

#[test]
fn allows_grave_with_plant_layers_for_source_compatibility() {
    let mut cells = empty_cells(SceneKind::Night);
    let cell = cell_mut(&mut cells, 0, 0);
    cell.base = LineupBase::Grave;
    cell.main = Some(LineupPlant::plain(CardSelection::Plant(PlantKind::Peashooter)));

    match Lineup::new(SceneKind::Night, cells, None) {
        Ok(_lineup) => {}
        Err(error) => panic!("grave plus plant should remain source-compatible: {error}"),
    }
}

fn assert_fixture_lineup(lineup: &Lineup) {
    assert_eq!(lineup.scene(), SceneKind::Pool);
    assert_eq!(lineup.row_count(), 6);
    assert_eq!(lineup.col_count(), 9);
    assert_eq!(lineup.rake_row(), Some(2));

    let cell = fixture_cell(lineup, 2, 0);
    assert_eq!(cell.base, LineupBase::LilyPad { imitator: false });
    assert_eq!(
        fixture_plant(cell.main, "main").selection,
        CardSelection::Plant(PlantKind::Peashooter)
    );
    assert_eq!(
        fixture_plant(cell.pumpkin, "pumpkin").selection,
        CardSelection::Imitator(PlantKind::Pumpkin)
    );
    assert_eq!(
        fixture_plant(cell.coffee, "coffee").selection,
        CardSelection::Imitator(PlantKind::CoffeeBean)
    );

    let grave = fixture_cell(lineup, 1, 1);
    assert!(grave.base.is_grave());

    let imitator = fixture_cell(lineup, 1, 2);
    assert_eq!(
        fixture_plant(imitator.main, "imitator main").selection,
        CardSelection::Imitator(PlantKind::Blover)
    );
    assert!(imitator.ladder);

    let asleep = fixture_cell(lineup, 4, 4);
    assert_eq!(fixture_plant(asleep.main, "puff").awake, Some(false));
    let awake = fixture_cell(lineup, 4, 5);
    assert_eq!(fixture_plant(awake.main, "fume").awake, Some(true));

    let potato = fixture_cell(lineup, 5, 6);
    assert!(fixture_plant(potato.main, "potato").grown);
    let sunshroom = fixture_cell(lineup, 5, 7);
    assert!(fixture_plant(sunshroom.main, "sunshroom").grown);
}

fn parse_lineup(input: &str) -> Lineup {
    Lineup::parse_auto(input).expect("valid fixture lineup")
}

fn parse_code_error(input: &str) -> LineupParseError {
    Lineup::from_code(input).expect_err("invalid lineup code")
}

fn new_lineup_error(scene: SceneKind, cells: Vec<LineupCell>, rake_grid: Option<Grid>) -> LineupParseError {
    Lineup::new(scene, cells, rake_grid).expect_err("invalid lineup")
}

fn cell_mut(cells: &mut [LineupCell], row: usize, col: usize) -> &mut LineupCell {
    let index = row * DEFAULT_COL_COUNT + col;
    cells
        .get_mut(index)
        .unwrap_or_else(|| panic!("test cell index out of bounds: row {row} col {col}"))
}

fn fixture_cell(lineup: &Lineup, row: i32, col: i32) -> LineupCell {
    lineup
        .cell(fixture_grid(row, col))
        .copied()
        .unwrap_or_else(|| panic!("fixture cell missing at row {row} col {col}"))
}

fn fixture_grid(row: i32, col: i32) -> Grid {
    Grid::new(row, col).expect("valid fixture grid")
}

fn fixture_plant(plant: Option<LineupPlant>, label: &str) -> LineupPlant {
    plant.unwrap_or_else(|| panic!("fixture plant layer missing: {label}"))
}

#[test]
fn toolkit_multilayer_golden_keeps_imitators_awake_state_and_rake() {
    // Frozen protocol fixture matching PVZTOOLS_LEGACY_FIXTURE; no test encoder.
    assert_fixture_lineup(&parse_lineup("LI43NJRQU0xM38WtJfhYkFdBUgl49vZYzlhYVNmNVnN2"));
}

#[test]
fn legacy_and_model_grid_bounds_are_checked_before_indexing() {
    for code in ["2,0 0 1 0 0 0", "2,0 6 1 0 0 0", "2,0 1 10 0 0 0"] {
        assert!(matches!(
            Lineup::from_text(code),
            Err(LineupParseError::InvalidGrid { .. })
        ));
    }
    let lineup = parse_lineup("2,0 1 1 0 0 0,1 5 9 0 0 0");
    assert!(lineup.cell(Grid { row: -1, col: 0 }).is_none());
    assert!(fixture_cell(&lineup, 0, 0).main.is_some());
    assert!(fixture_cell(&lineup, 4, 8).main.is_some());
}
