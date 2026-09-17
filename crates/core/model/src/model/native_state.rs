//! Backend-neutral vocabulary for deterministic native-state capture.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RandomStreamKind {
    Battle,
    Level,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RandomMode {
    Seeded(u32),
    Locked(u32),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SunProductionMode {
    #[default]
    DirectCredit,
    NativePickup,
}
