//! Shared interpretation of native Opening scalars.

const CHOOSE_NORMAL: i32 = 0;
const CHOOSE_VIEW_LAWN: i32 = 1;

/// Derives the shared PE/1051 dancer clock origin from the level seed.
#[must_use]
pub const fn mix_dancer_clock(level_seed: u32) -> u32 {
    let mut value = level_seed ^ 0x9e37_79b9;
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    (value ^ (value >> 16)) % 10_000
}

/// Backend-neutral interpretation of native seed-chooser lifecycle scalars.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeedChooserReadiness {
    Ready,
    MissingBoard,
    MissingCutScene,
    MissingSeedChooser,
    NotSeedChoosing,
    MouseHidden,
    ViewLawnEntering { time: i32 },
    ViewLawnStable { time: i32 },
    ViewLawnReturning { time: i32 },
    SeedsInFlight { count: i32 },
    PausedOrModal,
    Detached,
    UnexpectedState { raw: i32 },
}

/// Host action required before the runtime may finish an opening.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeedChooserOpeningAction {
    AdvanceNative,
    DispatchPrepare,
    DispatchReady,
}

impl SeedChooserReadiness {
    #[must_use]
    pub const fn opening_action(self) -> SeedChooserOpeningAction {
        match self {
            Self::Ready => SeedChooserOpeningAction::DispatchReady,
            Self::MissingSeedChooser | Self::NotSeedChoosing | Self::MouseHidden => {
                SeedChooserOpeningAction::DispatchPrepare
            }
            _ => SeedChooserOpeningAction::AdvanceNative,
        }
    }

    #[must_use]
    pub const fn allows_card_actions(self) -> bool {
        matches!(self, Self::Ready | Self::SeedsInFlight { .. })
    }

    #[must_use]
    pub const fn should_cancel_view_lawn(self) -> bool {
        matches!(self, Self::ViewLawnStable { .. })
    }

    #[must_use]
    pub const fn not_ready_message(self) -> &'static str {
        match self {
            Self::Ready => "seed chooser is ready for automatic selection",
            Self::MissingBoard => "level-intro board is not ready",
            Self::MissingCutScene => "level-intro cutscene is not ready",
            Self::MissingSeedChooser => "seed chooser is not ready",
            Self::NotSeedChoosing => "seed chooser is not choosing seeds yet",
            Self::MouseHidden => "seed chooser mouse input is hidden",
            Self::ViewLawnEntering { .. } => "seed chooser is entering View Lawn",
            Self::ViewLawnStable { .. } => "seed chooser is in View Lawn",
            Self::ViewLawnReturning { .. } => "seed chooser is returning from View Lawn",
            Self::SeedsInFlight { .. } => "seed chooser has flying seeds",
            Self::PausedOrModal => "game is paused or blocked by a modal dialog",
            Self::Detached => "seed chooser is detached from the widget manager",
            Self::UnexpectedState { .. } => "seed chooser is in an unexpected state",
        }
    }
}

/// Classifies copied native seed-chooser scalars without owning backend state.
#[must_use]
pub const fn classify_seed_chooser_readiness(
    seed_choosing: bool, paused_or_modal: bool, mouse_visible: bool, parent_present: bool,
    widget_manager_present: bool, choose_state: i32, view_lawn_time: i32, seeds_in_flight: i32,
) -> SeedChooserReadiness {
    if paused_or_modal {
        return SeedChooserReadiness::PausedOrModal;
    }
    if !seed_choosing {
        return SeedChooserReadiness::NotSeedChoosing;
    }
    if !mouse_visible {
        return SeedChooserReadiness::MouseHidden;
    }
    if !parent_present || !widget_manager_present {
        return SeedChooserReadiness::Detached;
    }
    match choose_state {
        CHOOSE_NORMAL if seeds_in_flight > 0 => SeedChooserReadiness::SeedsInFlight { count: seeds_in_flight },
        CHOOSE_NORMAL => SeedChooserReadiness::Ready,
        CHOOSE_VIEW_LAWN if view_lawn_time <= 100 => SeedChooserReadiness::ViewLawnEntering { time: view_lawn_time },
        CHOOSE_VIEW_LAWN if view_lawn_time <= 250 => SeedChooserReadiness::ViewLawnStable { time: view_lawn_time },
        CHOOSE_VIEW_LAWN => SeedChooserReadiness::ViewLawnReturning { time: view_lawn_time },
        raw => SeedChooserReadiness::UnexpectedState { raw },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dancer_clock_seed_vector_is_stable() {
        assert_eq!(mix_dancer_clock(0), 994);
        assert_eq!(mix_dancer_clock(0x5eed_1051), 3_040);
    }

    #[test]
    fn seed_chooser_scalars_have_one_shared_classification() {
        assert_eq!(
            classify_seed_chooser_readiness(true, false, true, true, true, 0, 0, 0),
            SeedChooserReadiness::Ready
        );
        assert_eq!(
            classify_seed_chooser_readiness(false, false, true, true, true, 0, 0, 0),
            SeedChooserReadiness::NotSeedChoosing
        );
        assert_eq!(
            classify_seed_chooser_readiness(true, false, false, true, true, 0, 0, 0),
            SeedChooserReadiness::MouseHidden
        );
        assert_eq!(
            classify_seed_chooser_readiness(true, false, true, true, true, 0, 0, 2),
            SeedChooserReadiness::SeedsInFlight { count: 2 }
        );
        assert_eq!(
            classify_seed_chooser_readiness(true, false, true, true, true, 1, 100, 0),
            SeedChooserReadiness::ViewLawnEntering { time: 100 }
        );
        assert_eq!(
            classify_seed_chooser_readiness(true, false, true, true, true, 1, 101, 0),
            SeedChooserReadiness::ViewLawnStable { time: 101 }
        );
        assert_eq!(
            classify_seed_chooser_readiness(true, false, true, true, true, 1, 251, 0),
            SeedChooserReadiness::ViewLawnReturning { time: 251 }
        );
        assert_eq!(
            classify_seed_chooser_readiness(true, true, true, true, true, 0, 0, 0),
            SeedChooserReadiness::PausedOrModal
        );
        assert_eq!(
            classify_seed_chooser_readiness(true, false, true, false, true, 0, 0, 0),
            SeedChooserReadiness::Detached
        );
        assert_eq!(
            classify_seed_chooser_readiness(true, false, true, true, false, 0, 0, 0),
            SeedChooserReadiness::Detached
        );
        assert_eq!(
            classify_seed_chooser_readiness(true, false, true, true, true, 7, 0, 0),
            SeedChooserReadiness::UnexpectedState { raw: 7 }
        );
    }

    #[test]
    fn seed_chooser_opening_action_preserves_prepare_and_native_waits() {
        assert_eq!(
            SeedChooserReadiness::Ready.opening_action(),
            SeedChooserOpeningAction::DispatchReady
        );
        for readiness in [
            SeedChooserReadiness::MissingSeedChooser,
            SeedChooserReadiness::NotSeedChoosing,
            SeedChooserReadiness::MouseHidden,
        ] {
            assert_eq!(readiness.opening_action(), SeedChooserOpeningAction::DispatchPrepare);
        }
        for readiness in [
            SeedChooserReadiness::MissingBoard,
            SeedChooserReadiness::MissingCutScene,
            SeedChooserReadiness::PausedOrModal,
            SeedChooserReadiness::SeedsInFlight { count: 1 },
            SeedChooserReadiness::ViewLawnStable { time: 101 },
            SeedChooserReadiness::Detached,
        ] {
            assert_eq!(readiness.opening_action(), SeedChooserOpeningAction::AdvanceNative);
        }
    }

    #[test]
    fn seed_chooser_card_and_view_lawn_actions_share_the_same_state() {
        assert!(SeedChooserReadiness::Ready.allows_card_actions());
        assert!(SeedChooserReadiness::SeedsInFlight { count: 2 }.allows_card_actions());
        assert!(!SeedChooserReadiness::NotSeedChoosing.allows_card_actions());
        assert!(SeedChooserReadiness::ViewLawnStable { time: 143 }.should_cancel_view_lawn());
        assert!(!SeedChooserReadiness::ViewLawnEntering { time: 80 }.should_cancel_view_lawn());
        assert!(!SeedChooserReadiness::ViewLawnReturning { time: 260 }.should_cancel_view_lawn());
    }
}
