use crate::model::{CardSelection, PlantKind};

/// One cell in a parsed lineup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineupCell {
    pub base: LineupBase,
    pub main: Option<LineupPlant>,
    pub pumpkin: Option<LineupPlant>,
    pub coffee: Option<LineupPlant>,
    pub ladder: bool,
}

impl LineupCell {
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            base: LineupBase::None,
            main: None,
            pumpkin: None,
            coffee: None,
            ladder: false,
        }
    }
}

/// A plant layer entry from a lineup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineupPlant {
    pub selection: CardSelection,
    pub awake: Option<bool>,
    pub grown: bool,
}

impl LineupPlant {
    #[must_use]
    pub const fn plain(selection: CardSelection) -> Self {
        Self {
            selection,
            awake: None,
            grown: false,
        }
    }
}

/// Base layer encoded by lineup formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineupBase {
    None,
    LilyPad { imitator: bool },
    FlowerPot { imitator: bool },
    Grave,
}

impl LineupBase {
    #[must_use]
    pub const fn plant_selection(self) -> Option<CardSelection> {
        match self {
            Self::None | Self::Grave => None,
            Self::LilyPad { imitator: false } => Some(CardSelection::Plant(PlantKind::LilyPad)),
            Self::LilyPad { imitator: true } => Some(CardSelection::Imitator(PlantKind::LilyPad)),
            Self::FlowerPot { imitator: false } => Some(CardSelection::Plant(PlantKind::FlowerPot)),
            Self::FlowerPot { imitator: true } => Some(CardSelection::Imitator(PlantKind::FlowerPot)),
        }
    }

    #[must_use]
    pub const fn is_grave(self) -> bool {
        matches!(self, Self::Grave)
    }
}
