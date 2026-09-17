//! Backend-neutral SmartRemove model.

/// Options for one SmartRemove automation tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SmartRemoveOptions {
    /// Whether to apply SmartRemove.h's `_isSparkle` highlight behavior.
    pub highlight: bool,
}

/// SmartRemove plant-map layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SmartRemoveGridPlantLayer {
    Unknown,
    Container,
    Pumpkin,
    Coffee,
    Common,
    Fly,
}
