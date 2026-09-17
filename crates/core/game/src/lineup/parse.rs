use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE, URL_SAFE_NO_PAD};
use miniz_oxide::inflate::decompress_to_vec_zlib_with_limit;

use crate::model::types::DEFAULT_COL_COUNT;
use crate::model::{CardSelection, Grid, PlantKind, SceneKind};

use super::cell::{LineupBase, LineupCell, LineupPlant};
use super::error::LineupParseError;
use super::model::Lineup;
use super::validate::{may_sleep, should_apply_mushroom_awake_state};

const TOOLKIT_XOR_KEY: u8 = 0x54;
const RAKE_DEFAULT_COL: i32 = 7;

impl Lineup {
    /// Parses either a pvztoolkit Base64/Base64Url lineup code or a pvztools legacy string.
    pub fn parse_auto(input: &str) -> Result<Self, LineupParseError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(LineupParseError::EmptyInput);
        }
        if input.contains(',') {
            Self::from_text(input)
        } else {
            Self::from_code(input)
        }
    }

    /// Parses a pvztoolkit compact lineup code.
    pub fn from_code(input: &str) -> Result<Self, LineupParseError> {
        let mut payload = decode_base64_any(input.trim())?;
        if payload.len() < 2 {
            return Err(LineupParseError::InvalidPayload);
        }
        for byte in &mut payload {
            *byte ^= TOOLKIT_XOR_KEY;
        }

        let metadata = payload.pop().ok_or(LineupParseError::InvalidPayload)?;
        let scene = scene_from_toolkit_raw(metadata & 0x0f)?;
        let rake_row = rake_row_from_toolkit_raw(metadata >> 4)?;
        let expected_len = scene.row_count() * DEFAULT_COL_COUNT * size_of::<u16>();
        let decompressed = decompress_to_vec_zlib_with_limit(&payload, expected_len)
            .map_err(|_error| LineupParseError::DecompressFailed)?;
        if decompressed.len() != expected_len {
            return Err(LineupParseError::UnexpectedDecompressedLength {
                actual: decompressed.len(),
                expected: expected_len,
            });
        }

        let mut cells = Vec::with_capacity(scene.row_count() * DEFAULT_COL_COUNT);
        for chunk in decompressed.chunks_exact(size_of::<u16>()) {
            let bits = u16::from_le_bytes([chunk[0], chunk[1]]);
            cells.push(decode_toolkit_cell(scene, bits)?);
        }
        Self::try_from_parts(scene, cells, rake_row)
    }

    /// Parses a pvztools legacy `scene,item...` lineup string.
    pub fn from_text(input: &str) -> Result<Self, LineupParseError> {
        let mut parts = input.trim().split(',');
        let scene_token = parts.next().ok_or(LineupParseError::EmptyInput)?.trim();
        let scene_raw = scene_token
            .parse::<u8>()
            .map_err(|_error| LineupParseError::UnknownFormat)?;
        let scene = scene_from_legacy_raw(scene_raw)?;
        let mut cells = empty_cells(scene);
        let mut rake_row = None;

        for item in parts {
            let fields = item.split_whitespace().collect::<Vec<_>>();
            let [item_type, row, col, state_row, state_col, imitator] = fields.as_slice() else {
                return Err(LineupParseError::UnsupportedLegacyField(item.to_owned()));
            };
            let item_type = parse_legacy_hex(item_type)?;
            let row = parse_i32(row)?;
            let col = parse_i32(col)?;
            let state_row = parse_i32(state_row)?;
            let state_col = parse_i32(state_col)?;
            let imitator = parse_i32(imitator)? == 1;
            if item_type == 0x31 {
                rake_row = Some(rake_row_from_one_based(row)?);
                continue;
            }
            let grid = Grid::from_one_based(row, col).map_err(|_error| LineupParseError::InvalidGrid { row, col })?;
            if !grid.is_in_bounds(scene.row_count(), DEFAULT_COL_COUNT) {
                return Err(LineupParseError::InvalidGrid { row, col });
            }
            let index = grid.row as usize * DEFAULT_COL_COUNT + grid.col as usize;
            let cell = &mut cells[index];
            match item_type {
                0x10 => cell.base = LineupBase::LilyPad { imitator },
                0x21 => cell.base = LineupBase::FlowerPot { imitator },
                0x1e => {
                    cell.pumpkin = Some(LineupPlant::plain(selection_for_layer(PlantKind::Pumpkin, imitator)));
                }
                0x23 => {
                    cell.coffee = Some(LineupPlant::plain(selection_for_layer(PlantKind::CoffeeBean, imitator)));
                }
                0x30 => cell.ladder = true,
                0x32 => cell.base = LineupBase::Grave,
                0..=0x2f => {
                    let kind = plant_kind_from_raw(item_type)?;
                    let awake = legacy_awake_state(scene, kind, state_row);
                    let grown = legacy_grown_state(kind, state_row, state_col);
                    cell.main = Some(LineupPlant {
                        selection: selection_for_layer(kind, imitator),
                        awake,
                        grown,
                    });
                }
                _ => return Err(LineupParseError::UnknownPlant(item_type)),
            }
        }

        Self::try_from_parts(scene, cells, rake_row)
    }
}

fn decode_base64_any(input: &str) -> Result<Vec<u8>, LineupParseError> {
    STANDARD
        .decode(input)
        .or_else(|_error| URL_SAFE.decode(input))
        .or_else(|_error| URL_SAFE_NO_PAD.decode(input))
        .map_err(|_error| LineupParseError::InvalidBase64)
}

pub(super) fn empty_cells(scene: SceneKind) -> Vec<LineupCell> {
    vec![LineupCell::empty(); scene.row_count() * DEFAULT_COL_COUNT]
}

fn decode_toolkit_cell(scene: SceneKind, bits: u16) -> Result<LineupCell, LineupParseError> {
    let plant_code = (bits >> 10) & 0x3f;
    let plant_imitator = ((bits >> 9) & 1) != 0;
    let awake_bit = ((bits >> 8) & 1) != 0;
    let base_code = (bits >> 6) & 0x3;
    let base_imitator = ((bits >> 5) & 1) != 0;
    let pumpkin = ((bits >> 4) & 1) != 0;
    let pumpkin_imitator = ((bits >> 3) & 1) != 0;
    let coffee = ((bits >> 2) & 1) != 0;
    let coffee_imitator = ((bits >> 1) & 1) != 0;
    let ladder = (bits & 1) != 0;

    let base = match base_code {
        0 => LineupBase::None,
        1 => LineupBase::LilyPad {
            imitator: base_imitator,
        },
        2 => LineupBase::FlowerPot {
            imitator: base_imitator,
        },
        3 => LineupBase::Grave,
        _ => unreachable!("base code is masked to two bits"),
    };
    let main = if plant_code == 0 {
        None
    } else {
        let kind = plant_kind_from_raw(plant_code - 1)?;
        Some(LineupPlant {
            selection: selection_for_layer(kind, plant_imitator),
            awake: mushroom_awake_state(scene, kind, awake_bit),
            grown: toolkit_grown_state(kind),
        })
    };
    Ok(LineupCell {
        base,
        main,
        pumpkin: pumpkin.then_some(LineupPlant::plain(selection_for_layer(
            PlantKind::Pumpkin,
            pumpkin_imitator,
        ))),
        coffee: coffee.then_some(LineupPlant::plain(selection_for_layer(
            PlantKind::CoffeeBean,
            coffee_imitator,
        ))),
        ladder,
    })
}

fn selection_for_layer(kind: PlantKind, imitator: bool) -> CardSelection {
    if imitator {
        CardSelection::Imitator(kind)
    } else {
        CardSelection::Plant(kind)
    }
}

fn mushroom_awake_state(scene: SceneKind, kind: PlantKind, awake: bool) -> Option<bool> {
    (may_sleep(kind) && should_apply_mushroom_awake_state(scene)).then_some(awake)
}

fn legacy_awake_state(scene: SceneKind, kind: PlantKind, state_row: i32) -> Option<bool> {
    mushroom_awake_state(scene, kind, state_row != 0)
}

fn toolkit_grown_state(kind: PlantKind) -> bool {
    matches!(kind, PlantKind::PotatoMine | PlantKind::SunShroom)
}

fn legacy_grown_state(kind: PlantKind, state_row: i32, state_col: i32) -> bool {
    matches!(kind, PlantKind::PotatoMine)
        || matches!(kind, PlantKind::SunShroom) && state_row == 1 && matches!(state_col, 2 | 3)
}

fn scene_from_toolkit_raw(raw: u8) -> Result<SceneKind, LineupParseError> {
    SceneKind::try_from_code(i32::from(raw))
        .ok()
        .filter(|scene| scene.supports_regular_lineup())
        .ok_or(LineupParseError::InvalidScene(raw))
}

fn scene_from_legacy_raw(raw: u8) -> Result<SceneKind, LineupParseError> {
    let scene = match raw {
        0 => SceneKind::Pool,
        1 => SceneKind::Fog,
        2 => SceneKind::Day,
        3 => SceneKind::Night,
        4 => SceneKind::Roof,
        5 => SceneKind::MoonNight,
        _ => return Err(LineupParseError::InvalidScene(raw)),
    };
    Ok(scene)
}

fn rake_row_from_toolkit_raw(raw: u8) -> Result<Option<i32>, LineupParseError> {
    if raw == 0 {
        Ok(None)
    } else {
        rake_row_from_one_based(i32::from(raw)).map(Some)
    }
}

fn rake_row_from_one_based(row: i32) -> Result<i32, LineupParseError> {
    Grid::from_one_based(row, RAKE_DEFAULT_COL + 1)
        .map(|grid| grid.row)
        .map_err(|_error| LineupParseError::InvalidGrid {
            row,
            col: RAKE_DEFAULT_COL + 1,
        })
}

fn parse_legacy_hex(value: &str) -> Result<u16, LineupParseError> {
    u16::from_str_radix(value, 16).map_err(|_error| LineupParseError::UnsupportedLegacyField(value.to_owned()))
}

fn parse_i32(value: &str) -> Result<i32, LineupParseError> {
    value
        .parse::<i32>()
        .map_err(|_error| LineupParseError::UnsupportedLegacyField(value.to_owned()))
}

fn plant_kind_from_raw(raw: u16) -> Result<PlantKind, LineupParseError> {
    PlantKind::try_from_code(i32::from(raw))
        .ok()
        .filter(|kind| *kind != PlantKind::Imitator)
        .ok_or(LineupParseError::UnknownPlant(raw))
}
