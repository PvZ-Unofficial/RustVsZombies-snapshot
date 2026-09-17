use rsvz_model::FastForwardPerformance;

use crate::error::Result;

use super::spec::{PatchSet, PatchSpec};

pub(crate) struct SimulationPerformanceGuard {
    improve: Option<PatchSet>,
    accelerate: Option<PatchSet>,
}

impl SimulationPerformanceGuard {
    pub(crate) fn enable(performance: FastForwardPerformance) -> Result<Self> {
        let improve = match performance {
            FastForwardPerformance::Off => None,
            FastForwardPerformance::Basic | FastForwardPerformance::Aggressive => {
                Some(PatchSet::enable(IMPROVE_PERFORMANCE_PATCHES)?)
            }
        };
        let accelerate = match performance {
            FastForwardPerformance::Aggressive => Some(PatchSet::enable(ACCELERATE_GAME_PATCHES)?),
            FastForwardPerformance::Off | FastForwardPerformance::Basic => None,
        };
        Ok(Self { improve, accelerate })
    }

    pub(crate) fn restore(&mut self) -> Result<()> {
        let mut first_error = None;
        if let Some(accelerate) = self.accelerate.as_mut()
            && let Err(error) = accelerate.restore()
        {
            first_error.get_or_insert(error);
        }
        if let Some(improve) = self.improve.as_mut()
            && let Err(error) = improve.restore()
        {
            first_error.get_or_insert(error);
        }
        self.accelerate = None;
        self.improve = None;
        first_error.map_or(Ok(()), Err)
    }
}

impl Drop for SimulationPerformanceGuard {
    fn drop(&mut self) {
        let _restore = self.restore();
    }
}

const IMPROVE_PERFORMANCE_PATCHES: &[PatchSpec] = super::spec::checked_specs(&[PatchSpec {
    name: "ImprovePerformance",
    addr: 0x6a66f4,
    off: &[0x01],
    on: &[0x00],
}]);

const ACCELERATE_GAME_PATCHES: &[PatchSpec] = super::spec::checked_specs(&[
    PatchSpec {
        name: "AccelerateGame::BoardUpdateToolTip",
        addr: 0x40ef00,
        off: &[0x6a, 0xff, 0x64],
        on: &[0xc2, 0x04, 0x00],
    },
    PatchSpec {
        // Skips cursor-preview state refresh, not only drawing, during aggressive fast-forward.
        name: "AccelerateGame::CursorPreviewUpdate",
        addr: 0x438da0,
        off: &[0x83, 0xec, 0x08],
        on: &[0xc3, 0x90, 0x90],
    },
    PatchSpec {
        name: "AccelerateGame::LawnAppEnforceCursor",
        addr: 0x455930,
        off: &[0x80],
        on: &[0xc3],
    },
    PatchSpec {
        name: "AccelerateGame::MusicUpdateMusicBurst",
        addr: 0x45b260,
        off: &[0x8b],
        on: &[0xc3],
    },
    PatchSpec {
        name: "AccelerateGame::LawnAppPlaySample",
        addr: 0x4560c0,
        off: &[0x80, 0xb9, 0xc5],
        on: &[0xc2, 0x04, 0x00],
    },
    PatchSpec {
        name: "AccelerateGame::CobCannonReadyTrackFlash",
        addr: 0x461123,
        off: &[0x8b, 0x9f],
        on: &[0xeb, 0x6d],
    },
    PatchSpec {
        name: "AccelerateGame::PlantUpdateReanimColor",
        addr: 0x4635c0,
        off: &[0x55, 0x8b, 0xec],
        on: &[0xc2, 0x04, 0x00],
    },
    PatchSpec {
        name: "AccelerateGame::PlantDrawHeightOffset",
        addr: 0x465040,
        off: &[0x55, 0x8b, 0xec],
        on: &[0xd9, 0xee, 0xc3],
    },
    PatchSpec {
        name: "AccelerateGame::WidgetDraw",
        addr: 0x471dcf,
        off: &[0x8b, 0x4e, 0x0c],
        on: &[0xeb, 0x24, 0x90],
    },
    PatchSpec {
        // Reisen lists 0x515020 twice. PvZ 1.0.0.1051 EN decomp/objdump identify it as
        // `TodFoley::PlayFoleyPitch(FoleyType, float)`; the duplicate is idempotent, so rsvz
        // keeps one immediate `ret 4` entry.
        name: "AccelerateGame::TodFoleyPlayFoleyPitch",
        addr: 0x515020,
        off: &[0x55, 0x8b, 0xec],
        on: &[0xc2, 0x04, 0x00],
    },
    PatchSpec {
        name: "AccelerateGame::TodFoleyLoopA",
        addr: 0x517670,
        off: &[0x74, 0x17],
        on: &[0x90, 0x90],
    },
    PatchSpec {
        name: "AccelerateGame::TodFoleyLoopB",
        addr: 0x51767b,
        off: &[0x75, 0x0c],
        on: &[0x90, 0x90],
    },
]);
