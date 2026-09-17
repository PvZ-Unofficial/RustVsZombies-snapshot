use crate::error::{Pvz1051Error, Result};

use super::memory::{PatchMemory, RealPatchMemory, read_bytes, write_bytes};

#[derive(Clone, Copy, Debug)]
pub(crate) struct PatchSpec {
    pub(crate) name: &'static str,
    pub(crate) addr: usize,
    pub(crate) off: &'static [u8],
    pub(crate) on: &'static [u8],
}

impl PatchSpec {
    pub(super) const fn len(self) -> usize {
        self.off.len()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpecState {
    Off,
    On,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PatchState {
    Off,
    On,
    Mixed,
}

#[derive(Debug)]
pub(crate) struct PatchBundle {
    pub(crate) name: &'static str,
    pub(crate) specs: &'static [PatchSpec],
}

#[derive(Clone, Copy, Debug)]
struct PatchRestore {
    spec: PatchSpec,
    restore_to: SpecState,
}

impl PatchBundle {
    pub(crate) fn enabled(&self) -> Result<bool> {
        self.enabled_with(&mut RealPatchMemory)
    }

    fn enabled_with(&self, memory: &mut impl PatchMemory) -> Result<bool> {
        match self.state_with(memory)? {
            PatchState::On => Ok(true),
            PatchState::Off => Ok(false),
            PatchState::Mixed => Err(Pvz1051Error::Patch(format!("{} is in a mixed patch state", self.name))),
        }
    }

    pub(crate) fn set_enabled(&self, enabled: bool) -> Result<()> {
        self.set_enabled_with(enabled, &mut RealPatchMemory)
    }

    fn state_with(&self, memory: &mut impl PatchMemory) -> Result<PatchState> {
        let Some((first, rest)) = self.specs.split_first() else {
            return Err(Pvz1051Error::Patch(format!("{} has no patch entries", self.name)));
        };
        let first = spec_state(first, memory)?;
        let mut mixed = false;
        for spec in rest {
            if spec_state(spec, memory)? != first {
                mixed = true;
            }
        }
        Ok(match (first, mixed) {
            (SpecState::Off, false) => PatchState::Off,
            (SpecState::On, false) => PatchState::On,
            (_state, true) => PatchState::Mixed,
        })
    }

    fn set_enabled_with(&self, enabled: bool, memory: &mut impl PatchMemory) -> Result<()> {
        let target = if enabled { SpecState::On } else { SpecState::Off };
        let mut states = Vec::with_capacity(self.specs.len());
        for spec in self.specs {
            states.push(spec_state(spec, memory)?);
        }

        let mut restores = Vec::new();
        for (spec, state) in self.specs.iter().copied().zip(states) {
            if state == target {
                continue;
            }
            let bytes = bytes_for_state(spec, target);
            if let Err(error) = memory.write_bytes(spec.addr, bytes) {
                return super::memory::finish_restoration(
                    Err(error),
                    rollback(restores.iter().rev().copied(), memory),
                    "patch toggle",
                );
            }
            restores.push(PatchRestore {
                spec,
                restore_to: state,
            });
        }
        Ok(())
    }
}

struct AppliedPatch {
    spec: PatchSpec,
    changed: bool,
}

#[derive(Default)]
pub(crate) struct PatchSet {
    applied: Vec<AppliedPatch>,
}

impl PatchSet {
    pub(crate) fn enable(specs: &'static [PatchSpec]) -> Result<Self> {
        let mut set = Self::default();
        for spec in specs {
            if let Err(error) = set.enable_one(*spec) {
                if let Err(restore) = set.restore() {
                    crate::runtime::hook::fail_and_request_unload();
                    return Err(Pvz1051Error::Patch(format!(
                        "patch set install failed: {error}; rollback also failed: {restore}"
                    )));
                }
                return Err(error);
            }
        }
        Ok(set)
    }

    pub(crate) fn restore(&mut self) -> Result<()> {
        let mut first_error = None;
        let mut retained = Vec::new();
        while let Some(applied) = self.applied.pop() {
            if !applied.changed {
                continue;
            }
            let current = read_bytes(applied.spec.addr, applied.spec.len());
            if current != applied.spec.on {
                first_error.get_or_insert_with(|| unrecognized_patch_error(&applied.spec, &current));
                retained.push(applied);
                continue;
            }
            if let Err(error) = write_bytes(applied.spec.addr, applied.spec.off) {
                first_error.get_or_insert(error);
                retained.push(applied);
            }
        }
        retained.reverse();
        self.applied = retained;
        first_error.map_or(Ok(()), Err)
    }

    fn enable_one(&mut self, spec: PatchSpec) -> Result<()> {
        let current = read_bytes(spec.addr, spec.len());
        if current == spec.on {
            self.applied.push(AppliedPatch { spec, changed: false });
            return Ok(());
        }
        if current != spec.off {
            return Err(unrecognized_patch_error(&spec, &current));
        }

        write_bytes(spec.addr, spec.on)?;
        self.applied.push(AppliedPatch { spec, changed: true });
        Ok(())
    }
}

impl Drop for PatchSet {
    fn drop(&mut self) {
        let _restore = self.restore();
    }
}

/// All production tables are checked during constant evaluation, never during a query.
pub(super) const fn checked_specs(specs: &'static [PatchSpec]) -> &'static [PatchSpec] {
    assert!(!specs.is_empty(), "empty patch bundle");
    let mut i = 0;
    while i < specs.len() {
        let spec = &specs[i];
        assert!(!spec.off.is_empty(), "empty patch bytes");
        assert!(spec.off.len() == spec.on.len(), "mismatched patch lengths");
        assert!(
            spec.addr.checked_add(spec.off.len()).is_some(),
            "patch address overflow"
        );
        let mut j = 0;
        while j < i {
            let other = &specs[j];
            assert!(
                spec.addr >= other.addr + other.off.len() || other.addr >= spec.addr + spec.off.len(),
                "overlapping patch entries"
            );
            j += 1;
        }
        i += 1;
    }
    specs
}

fn spec_state(spec: &PatchSpec, memory: &mut impl PatchMemory) -> Result<SpecState> {
    let current = memory.read_bytes(spec.addr, spec.len());
    if current == spec.off {
        return Ok(SpecState::Off);
    }
    if current == spec.on {
        return Ok(SpecState::On);
    }
    Err(unrecognized_patch_error(spec, &current))
}

fn bytes_for_state(spec: PatchSpec, state: SpecState) -> &'static [u8] {
    match state {
        SpecState::Off => spec.off,
        SpecState::On => spec.on,
    }
}

fn rollback(restores: impl IntoIterator<Item = PatchRestore>, memory: &mut impl PatchMemory) -> Result<()> {
    let mut first_error = None;
    for restore in restores {
        if let Err(error) = memory.write_bytes(restore.spec.addr, bytes_for_state(restore.spec, restore.restore_to)) {
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn unrecognized_patch_error(spec: &PatchSpec, actual: &[u8]) -> Pvz1051Error {
    Pvz1051Error::Patch(format!(
        "{} at 0x{:x} expected off bytes {:02x?} or on bytes {:02x?}, got {:02x?}",
        spec.name, spec.addr, spec.off, spec.on, actual
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn transaction_errors_preserve_operation_and_restoration_failures() {
        use super::super::memory::finish_restoration;
        let failure = |message: &str| Err::<(), _>(Pvz1051Error::Patch(message.to_owned()));
        assert!(finish_restoration(Ok(()), Ok(()), "test").is_ok());
        let error = finish_restoration(failure("operation"), Ok(()), "test").unwrap_err();
        assert!(error.to_string().contains("operation"));
        let error = finish_restoration(Ok(()), failure("restore"), "test").unwrap_err();
        assert!(error.to_string().contains("restore"));
        let error = finish_restoration(failure("operation"), failure("restore"), "test").unwrap_err();
        assert!(error.to_string().contains("operation") && error.to_string().contains("restore"));
    }

    use super::*;

    const SPEC_A: PatchSpec = PatchSpec {
        name: "a",
        addr: 0,
        off: &[0x10],
        on: &[0x11],
    };
    const SPEC_B: PatchSpec = PatchSpec {
        name: "b",
        addr: 1,
        off: &[0x20],
        on: &[0x21],
    };
    const IDEMPOTENT_DUP_A: PatchSpec = PatchSpec {
        name: "a duplicate",
        addr: 0,
        off: &[0x10],
        on: &[0x11],
    };
    static BUNDLE: PatchBundle = PatchBundle {
        name: "bundle",
        specs: &[SPEC_A, SPEC_B],
    };
    static DUP_BUNDLE: PatchBundle = PatchBundle {
        name: "dups",
        specs: &[SPEC_A, IDEMPOTENT_DUP_A],
    };
    #[derive(Debug)]
    struct FakeMemory {
        bytes: Vec<u8>,
        writes: Vec<(usize, Vec<u8>)>,
        fail_write: Option<usize>,
    }

    impl FakeMemory {
        fn new(bytes: &[u8]) -> Self {
            Self {
                bytes: bytes.to_vec(),
                writes: Vec::new(),
                fail_write: None,
            }
        }
    }

    impl PatchMemory for FakeMemory {
        fn read_bytes(&mut self, addr: usize, len: usize) -> Vec<u8> {
            self.bytes[addr..addr + len].to_vec()
        }

        fn write_bytes(&mut self, addr: usize, bytes: &[u8]) -> Result<()> {
            self.writes.push((addr, bytes.to_vec()));
            if self.fail_write == Some(addr) {
                return Err(Pvz1051Error::Patch("fake write failure".to_owned()));
            }
            self.bytes[addr..addr + bytes.len()].copy_from_slice(bytes);
            Ok(())
        }
    }

    #[test]
    fn spec_state_classifies_off_on_and_unknown() {
        let mut memory = FakeMemory::new(&[0x10]);
        assert_eq!(spec_state(&SPEC_A, &mut memory).unwrap(), SpecState::Off);

        let mut memory = FakeMemory::new(&[0x11]);
        assert_eq!(spec_state(&SPEC_A, &mut memory).unwrap(), SpecState::On);

        let mut memory = FakeMemory::new(&[0xff]);
        assert!(matches!(
            spec_state(&SPEC_A, &mut memory),
            Err(Pvz1051Error::Patch(message)) if message.contains("expected off bytes")
        ));
    }

    #[test]
    fn bundle_state_classifies_off_on_and_mixed() {
        let mut memory = FakeMemory::new(&[0x10, 0x20]);
        assert_eq!(BUNDLE.state_with(&mut memory).unwrap(), PatchState::Off);

        let mut memory = FakeMemory::new(&[0x11, 0x21]);
        assert_eq!(BUNDLE.state_with(&mut memory).unwrap(), PatchState::On);

        let mut memory = FakeMemory::new(&[0x11, 0x20]);
        assert_eq!(BUNDLE.state_with(&mut memory).unwrap(), PatchState::Mixed);
    }

    #[test]
    fn enabled_rejects_mixed_state() {
        let mut memory = FakeMemory::new(&[0x11, 0x20]);
        assert!(matches!(
            BUNDLE.enabled_with(&mut memory),
            Err(Pvz1051Error::Patch(message)) if message.contains("mixed")
        ));
    }

    #[test]
    fn set_enabled_normalizes_mixed_state() {
        let mut memory = FakeMemory::new(&[0x11, 0x20]);
        BUNDLE.set_enabled_with(true, &mut memory).unwrap();
        assert_eq!(memory.bytes, [0x11, 0x21]);

        BUNDLE.set_enabled_with(false, &mut memory).unwrap();
        assert_eq!(memory.bytes, [0x10, 0x20]);
    }

    #[test]
    fn set_enabled_unknown_state_writes_nothing() {
        let mut memory = FakeMemory::new(&[0xff, 0x20]);
        assert!(BUNDLE.set_enabled_with(true, &mut memory).is_err());
        assert!(memory.writes.is_empty());
    }

    #[test]
    fn set_enabled_rolls_back_completed_writes_on_failure() {
        let mut memory = FakeMemory::new(&[0x10, 0x20]);
        memory.fail_write = Some(1);
        assert!(BUNDLE.set_enabled_with(true, &mut memory).is_err());
        assert_eq!(memory.bytes, [0x10, 0x20]);
        assert_eq!(memory.writes, [(0, vec![0x11]), (1, vec![0x21]), (0, vec![0x10])]);
    }

    #[test]
    fn static_tables_reject_duplicate_ranges_and_invalid_lengths() {
        assert!(std::panic::catch_unwind(|| checked_specs(DUP_BUNDLE.specs)).is_err());
        for specs in [
            &[PatchSpec {
                name: "empty",
                addr: 0,
                off: &[],
                on: &[],
            }][..],
            &[PatchSpec {
                name: "length",
                addr: 0,
                off: &[0],
                on: &[0, 1],
            }][..],
            &[PatchSpec {
                name: "overflow",
                addr: usize::MAX,
                off: &[0],
                on: &[1],
            }][..],
            &[
                SPEC_A,
                PatchSpec {
                    name: "overlap",
                    addr: 0,
                    off: &[1, 2],
                    on: &[3, 4],
                },
            ][..],
        ] {
            assert!(std::panic::catch_unwind(|| checked_specs(specs)).is_err());
        }
    }
}
