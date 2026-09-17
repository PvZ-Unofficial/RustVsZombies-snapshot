use std::fmt::Debug;

use rsvz_model::model::{KernelPultProjectileRule, MaidCheat, PlantDamageRule};

use crate::error::{Pvz1051Error, Result};

use super::memory::{PatchMemory, RealPatchMemory};

#[derive(Clone, Copy, Debug)]
pub(crate) struct VariantPatchBundle<M: 'static> {
    pub(super) name: &'static str,
    pub(super) entries: &'static [VariantPatchEntry<M>],
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct VariantPatchEntry<M: 'static> {
    pub(super) name: &'static str,
    pub(super) addr: usize,
    pub(super) variants: &'static [(M, &'static [u8])],
}

#[derive(Clone, Debug)]
struct VariantPatchRestore<M: Copy + Debug + Eq + 'static> {
    entry: VariantPatchEntry<M>,
    bytes: Vec<u8>,
}

impl<M: Copy + Debug + Eq + 'static> VariantPatchBundle<M> {
    pub(crate) fn state(&self) -> Result<M> {
        self.state_with(&mut RealPatchMemory)
    }

    pub(crate) fn set(&self, mode: M) -> Result<()> {
        self.set_with(mode, &mut RealPatchMemory)
    }

    fn state_with(&self, memory: &mut impl PatchMemory) -> Result<M> {
        let intersection = self.matching_intersection(memory)?;
        match intersection.as_slice() {
            [mode] => Ok(*mode),
            [] => Err(Pvz1051Error::Patch(format!(
                "{} is in an unknown or mixed variant patch state",
                self.name
            ))),
            modes => Err(Pvz1051Error::Patch(format!(
                "{} is in an ambiguous variant patch state: {modes:?}",
                self.name
            ))),
        }
    }

    fn set_with(&self, mode: M, memory: &mut impl PatchMemory) -> Result<()> {
        let mut candidates = Vec::with_capacity(self.entries.len());
        for entry in self.entries {
            let current = memory.read_bytes(entry.addr, entry_len(*entry));
            if matching_modes(*entry, &current).is_empty() {
                return Err(unrecognized_variant_error(self.name, entry, &current));
            }
            candidates.push(VariantPatchRestore {
                entry: *entry,
                bytes: current,
            });
        }

        let mut restores = Vec::new();
        for restore in candidates {
            let target = bytes_for_mode(restore.entry, mode)?;
            if restore.bytes == target {
                continue;
            }
            if let Err(error) = memory.write_bytes(restore.entry.addr, target) {
                return super::memory::finish_restoration(
                    Err(error),
                    rollback_exact(restores.iter().rev(), memory),
                    "variant toggle",
                );
            }
            restores.push(restore);
        }
        Ok(())
    }

    fn matching_intersection(&self, memory: &mut impl PatchMemory) -> Result<Vec<M>> {
        let mut intersection: Option<Vec<M>> = None;
        for entry in self.entries {
            let current = memory.read_bytes(entry.addr, entry_len(*entry));
            let modes = matching_modes(*entry, &current);
            if modes.is_empty() {
                return Err(unrecognized_variant_error(self.name, entry, &current));
            }
            intersection = Some(match intersection {
                None => modes,
                Some(prev) => prev.into_iter().filter(|mode| modes.contains(mode)).collect(),
            });
        }
        Ok(intersection.unwrap_or_default())
    }
}

const fn checked_entries<M>(entries: &'static [VariantPatchEntry<M>]) -> &'static [VariantPatchEntry<M>] {
    assert!(!entries.is_empty(), "empty variant bundle");
    let mut i = 0;
    while i < entries.len() {
        let entry = &entries[i];
        assert!(!entry.variants.is_empty(), "missing variants");
        let len = entry.variants[0].1.len();
        assert!(len > 0, "empty variant bytes");
        assert!(entry.addr.checked_add(len).is_some(), "variant address overflow");
        let mut v = 0;
        while v < entry.variants.len() {
            assert!(entry.variants[v].1.len() == len, "mismatched variant lengths");
            v += 1;
        }
        let mut j = 0;
        while j < i {
            let other = &entries[j];
            assert!(
                entry.addr >= other.addr + other.variants[0].1.len() || other.addr >= entry.addr + len,
                "overlapping variant entries"
            );
            j += 1;
        }
        i += 1;
    }
    entries
}

fn entry_len<M: Copy>(entry: VariantPatchEntry<M>) -> usize {
    entry.variants[0].1.len()
}

fn matching_modes<M: Copy + Eq>(entry: VariantPatchEntry<M>, bytes: &[u8]) -> Vec<M> {
    entry
        .variants
        .iter()
        .filter_map(|(mode, candidate)| (*candidate == bytes).then_some(*mode))
        .collect()
}

fn bytes_for_mode<M: Copy + Debug + Eq>(entry: VariantPatchEntry<M>, mode: M) -> Result<&'static [u8]> {
    entry
        .variants
        .iter()
        .find_map(|(candidate, bytes)| (*candidate == mode).then_some(*bytes))
        .ok_or_else(|| Pvz1051Error::Patch(format!("{} has no bytes for mode {mode:?}", entry.name)))
}

fn rollback_exact<'a, M: Copy + Debug + Eq + 'static>(
    restores: impl IntoIterator<Item = &'a VariantPatchRestore<M>>, memory: &mut impl PatchMemory,
) -> Result<()> {
    let mut first_error = None;
    for restore in restores {
        if let Err(error) = memory.write_bytes(restore.entry.addr, &restore.bytes) {
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn unrecognized_variant_error<M: Copy + Debug + Eq>(
    bundle_name: &str, entry: &VariantPatchEntry<M>, actual: &[u8],
) -> Pvz1051Error {
    Pvz1051Error::Patch(format!(
        "{bundle_name}::{} at 0x{:x} has unrecognized bytes {:02x?}",
        entry.name, entry.addr, actual
    ))
}

pub(crate) mod kernel_pult_projectile_rule {
    use super::*;

    pub(crate) static BUNDLE: VariantPatchBundle<KernelPultProjectileRule> = VariantPatchBundle {
        name: "kernel_pult_projectile_rule",
        entries: &[VariantPatchEntry {
            name: "Plant::UpdateKernelPult::projectile_branch",
            addr: 0x45f1ec,
            variants: &[
                (KernelPultProjectileRule::Normal, &[0x75]),
                (KernelPultProjectileRule::AlwaysButter, &[0x70]),
                (KernelPultProjectileRule::AlwaysKernel, &[0xeb]),
            ],
        }],
    };
    const _: () = {
        checked_entries(BUNDLE.entries);
    };

    pub(crate) fn rule() -> Result<KernelPultProjectileRule> {
        BUNDLE.state()
    }

    pub(crate) fn set_rule(rule: KernelPultProjectileRule) -> Result<()> {
        BUNDLE.set(rule)
    }
}

pub(crate) mod plant_damage_rule {
    use super::*;

    pub(crate) static BUNDLE: VariantPatchBundle<PlantDamageRule> = VariantPatchBundle {
        name: "plant_damage_rule",
        entries: &[
            VariantPatchEntry {
                // 0x41cc2d `test al, al`; this entry owns the hit-action branch opcode only.
                // Native hit action at 0x41cc31..0x41cc3d increments mPlantsEaten and calls Plant::Die.
                name: "Board::KillAllPlantsInRadius::hit_action_branch_opcode",
                addr: 0x41cc2f,
                variants: &[
                    (PlantDamageRule::Normal, &[0x74]),
                    (PlantDamageRule::Invincible, &[0xeb]),
                    (PlantDamageRule::Weak, &[0x74]),
                ],
            },
            VariantPatchEntry {
                // Full instruction starts at 0x45ec63: `83 46 40 ce`
                // (`add dword ptr [esi+0x40], -50`). This entry owns only its immediate byte.
                name: "Plant::SpikeRockTakeDamage::minus_50_immediate_byte",
                addr: 0x45ec66,
                variants: &[
                    (PlantDamageRule::Normal, &[0xce]),
                    (PlantDamageRule::Invincible, &[0x00]),
                    // Reisen `PlantWeak` lists `0x00` here, but this byte is the `-50`
                    // immediate in `SpikeRockTakeDamage`. pvztools/pvztoolkit weak mode keeps
                    // it at `0xce`; otherwise spike-rock damage becomes invincible, not weak.
                    (PlantDamageRule::Weak, &[0xce]),
                ],
            },
            VariantPatchEntry {
                // DoRowAreaDamage branches between SpikeRockTakeDamage and Plant::Die for the
                // attacking spike plant. This is not a generic Plant::TakeDamage entry.
                name: "Plant::DoRowAreaDamage::spike_self_damage_branch_opcode",
                addr: 0x45ee0a,
                variants: &[
                    (PlantDamageRule::Normal, &[0x75]),
                    (PlantDamageRule::Invincible, &[0x70]),
                    (PlantDamageRule::Weak, &[0xeb]),
                ],
            },
            VariantPatchEntry {
                name: "Plant::Squish::entry",
                addr: 0x462b80,
                variants: &[
                    (PlantDamageRule::Normal, &[0x53, 0x55, 0x8b]),
                    (PlantDamageRule::Invincible, &[0xc2, 0x04, 0x00]),
                    (PlantDamageRule::Weak, &[0x53, 0x55, 0x8b]),
                ],
            },
            VariantPatchEntry {
                // Projectile::CheckForCollision zombie-pea path; owns the complete three-byte HP write.
                name: "Projectile::CheckForCollision::zombie_pea_hp_write",
                addr: 0x46cfeb,
                variants: &[
                    (PlantDamageRule::Normal, &[0x29, 0x50, 0x40]),
                    (PlantDamageRule::Invincible, &[0x90, 0x90, 0x90]),
                    (PlantDamageRule::Weak, &[0x29, 0x40, 0x40]),
                ],
            },
            VariantPatchEntry {
                // Projectile::UpdateLobMotion basketball path; owns the complete three-byte HP write.
                name: "Projectile::UpdateLobMotion::basketball_hp_write",
                addr: 0x46d7a6,
                variants: &[
                    (PlantDamageRule::Normal, &[0x29, 0x4e, 0x40]),
                    (PlantDamageRule::Invincible, &[0x90, 0x90, 0x90]),
                    (PlantDamageRule::Weak, &[0x29, 0x76, 0x40]),
                ],
            },
            VariantPatchEntry {
                // UpdateZombieJalapeno same-row filter. The native mPlantsEaten/Plant::Die action is
                // later at 0x52771f/0x527729; this byte is not Zombie::EatPlant.
                name: "Zombie::UpdateZombieJalapeno::same_row_branch_opcode",
                addr: 0x5276ea,
                variants: &[
                    (PlantDamageRule::Normal, &[0x75]),
                    (PlantDamageRule::Invincible, &[0xeb]),
                    (PlantDamageRule::Weak, &[0x75]),
                ],
            },
            VariantPatchEntry {
                // IteratePlants exhaustion gate in SquishAllInSquare. Per-plant count/Squish is at
                // 0x52e971..0x52e97f; this byte is not a BurnRow damage action.
                name: "Zombie::SquishAllInSquare::iteration_gate_opcode",
                addr: 0x52e93b,
                variants: &[
                    (PlantDamageRule::Normal, &[0x74]),
                    (PlantDamageRule::Invincible, &[0xeb]),
                    (PlantDamageRule::Weak, &[0x74]),
                ],
            },
            VariantPatchEntry {
                // Full bite HP write starts at 0x52fcf0: `83 46 40 fc`
                // (`add dword ptr [esi+0x40], -4`). This entry owns its ModR/M byte only.
                name: "Zombie::EatPlant::hp_write_modrm_byte",
                addr: 0x52fcf1,
                variants: &[
                    (PlantDamageRule::Normal, &[0x46]),
                    (PlantDamageRule::Invincible, &[0x46]),
                    (PlantDamageRule::Weak, &[0x66]),
                ],
            },
            VariantPatchEntry {
                // Immediate byte of the same complete instruction beginning at 0x52fcf0.
                name: "Zombie::EatPlant::hp_write_immediate_byte",
                addr: 0x52fcf3,
                variants: &[
                    (PlantDamageRule::Normal, &[0xfc]),
                    (PlantDamageRule::Invincible, &[0x00]),
                    (PlantDamageRule::Weak, &[0x00]),
                ],
            },
        ],
    };
    const _: () = {
        checked_entries(BUNDLE.entries);
    };

    pub(crate) fn rule() -> Result<PlantDamageRule> {
        BUNDLE.state()
    }

    pub(crate) fn set_rule(rule: PlantDamageRule) -> Result<()> {
        BUNDLE.set(rule)
    }
}

pub(crate) mod maid_cheat {
    use super::*;

    pub(crate) static BUNDLE: VariantPatchBundle<MaidCheat> = VariantPatchBundle {
        name: "maid_cheat",
        entries: &[VariantPatchEntry {
            name: "Zombie::UpdateZombieDancer::phase_code",
            addr: 0x52dfc9,
            variants: &[
                (MaidCheat::Stop, &[0x8b, 0x80, 0x38, 0x08]),
                (MaidCheat::CallPartner, &[0x90, 0xb8, 0xf0, 0x00]),
                (MaidCheat::Dancing, &[0x90, 0xb8, 0x40, 0x01]),
                (MaidCheat::Move, &[0x90, 0xb8, 0xe9, 0x00]),
            ],
        }],
    };
    const _: () = {
        checked_entries(BUNDLE.entries);
    };

    pub(crate) fn state() -> Result<MaidCheat> {
        BUNDLE.state()
    }

    pub(crate) fn set(state: MaidCheat) -> Result<()> {
        BUNDLE.set(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AMBIGUOUS_A: VariantPatchEntry<PlantDamageRule> = VariantPatchEntry {
        name: "a",
        addr: 0,
        variants: &[
            (PlantDamageRule::Normal, &[0x10]),
            (PlantDamageRule::Invincible, &[0x11]),
            (PlantDamageRule::Weak, &[0x10]),
        ],
    };
    const AMBIGUOUS_B: VariantPatchEntry<PlantDamageRule> = VariantPatchEntry {
        name: "b",
        addr: 1,
        variants: &[
            (PlantDamageRule::Normal, &[0x20]),
            (PlantDamageRule::Invincible, &[0x21]),
            (PlantDamageRule::Weak, &[0x22]),
        ],
    };
    static BUNDLE: VariantPatchBundle<PlantDamageRule> = VariantPatchBundle {
        name: "test",
        entries: &[AMBIGUOUS_A, AMBIGUOUS_B],
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
    fn state_uses_intersection_across_ambiguous_entries() {
        let mut memory = FakeMemory::new(&[0x10, 0x20]);
        assert_eq!(BUNDLE.state_with(&mut memory).unwrap(), PlantDamageRule::Normal);

        let mut memory = FakeMemory::new(&[0x10, 0x22]);
        assert_eq!(BUNDLE.state_with(&mut memory).unwrap(), PlantDamageRule::Weak);
    }

    #[test]
    fn state_rejects_unknown_and_ambiguous_intersection() {
        let mut memory = FakeMemory::new(&[0xff, 0x20]);
        assert!(matches!(BUNDLE.state_with(&mut memory), Err(Pvz1051Error::Patch(_))));

        static AMBIGUOUS_BUNDLE: VariantPatchBundle<PlantDamageRule> = VariantPatchBundle {
            name: "ambiguous",
            entries: &[AMBIGUOUS_A],
        };
        let mut memory = FakeMemory::new(&[0x10]);
        assert!(
            matches!(AMBIGUOUS_BUNDLE.state_with(&mut memory), Err(Pvz1051Error::Patch(message)) if message.contains("ambiguous"))
        );
    }

    #[test]
    fn set_switches_from_recognized_ambiguous_bytes_and_rolls_back() {
        let mut memory = FakeMemory::new(&[0x10, 0x20]);
        BUNDLE.set_with(PlantDamageRule::Weak, &mut memory).unwrap();
        assert_eq!(memory.bytes, [0x10, 0x22]);

        let mut memory = FakeMemory::new(&[0x10, 0x20]);
        memory.fail_write = Some(1);
        assert!(BUNDLE.set_with(PlantDamageRule::Invincible, &mut memory).is_err());
        assert_eq!(memory.bytes, [0x10, 0x20]);
    }

    #[test]
    fn generated_rule_modules_expose_uniform_functions() {
        let _kernel_rule: fn() -> Result<KernelPultProjectileRule> = kernel_pult_projectile_rule::rule;
        let _set_kernel_rule: fn(KernelPultProjectileRule) -> Result<()> = kernel_pult_projectile_rule::set_rule;
        let _plant_rule: fn() -> Result<PlantDamageRule> = plant_damage_rule::rule;
        let _set_plant_rule: fn(PlantDamageRule) -> Result<()> = plant_damage_rule::set_rule;
    }

    #[test]
    fn maid_cheat_variants_preserve_verified_phase_bytes() {
        assert_eq!(
            maid_cheat::BUNDLE.entries[0].variants,
            [
                (MaidCheat::Stop, &[0x8b, 0x80, 0x38, 0x08][..]),
                (MaidCheat::CallPartner, &[0x90, 0xb8, 0xf0, 0x00][..]),
                (MaidCheat::Dancing, &[0x90, 0xb8, 0x40, 0x01][..]),
                (MaidCheat::Move, &[0x90, 0xb8, 0xe9, 0x00][..]),
            ]
        );
    }
}
