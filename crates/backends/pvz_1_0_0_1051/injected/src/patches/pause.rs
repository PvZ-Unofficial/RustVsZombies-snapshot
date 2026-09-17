use crate::error::Result;

use super::spec::{PatchSet, PatchSpec};

pub(crate) struct AdvancedPausePatch {
    patch: Option<PatchSet>,
}

impl AdvancedPausePatch {
    pub(crate) fn enable() -> Result<Self> {
        Ok(Self {
            patch: Some(PatchSet::enable(ADVANCED_PAUSE_PATCHES)?),
        })
    }

    pub(crate) fn restore(&mut self) -> Result<()> {
        let Some(patch) = self.patch.as_mut() else {
            return Ok(());
        };
        let result = patch.restore();
        self.patch = None;
        result
    }
}

impl Drop for AdvancedPausePatch {
    fn drop(&mut self) {
        let _restore = self.restore();
    }
}

const ADVANCED_PAUSE_PATCHES: &[PatchSpec] = super::spec::checked_specs(&[PatchSpec {
    name: "AdvancedPause::BoardUpdateSkip",
    addr: 0x41600e,
    off: &[0x8b, 0xfd],
    on: &[0xeb, 0x2a],
}]);
