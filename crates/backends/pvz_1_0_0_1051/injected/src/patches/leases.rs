use crate::error::{Pvz1051Error, Result};
use rsvz_model::{KernelPultProjectileRule, MaidCheat, PlantDamageRule};
use std::cell::RefCell;

use super::spec::PatchBundle;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BoolPatchId {
    SeedRechargeIgnored,
    SunCostIgnored,
    FogRevealed,
    VaseContentsVisible,
    InstantIceAndAshEffects,
    CobFixedDelay,
    CobRechargeShortened,
    JackExplosionsDisabled,
    PepperExplosionsDisabled,
    ItemDropDisabled,
    SpecialEventsDisabled,
    NaturalSunDropDisabled,
    AutoCollectNormal,
    CobDriftFixed,
    MushroomsAwake,
    ZombiesDieAtHouse,
    PlantingRestrictionsIgnored,
    ProfileReadonly,
    ZombieSpawnStopped,
}

impl BoolPatchId {
    const ALL: [Self; 19] = [
        Self::SeedRechargeIgnored,
        Self::SunCostIgnored,
        Self::FogRevealed,
        Self::VaseContentsVisible,
        Self::InstantIceAndAshEffects,
        Self::CobFixedDelay,
        Self::CobRechargeShortened,
        Self::JackExplosionsDisabled,
        Self::PepperExplosionsDisabled,
        Self::ItemDropDisabled,
        Self::SpecialEventsDisabled,
        Self::NaturalSunDropDisabled,
        Self::AutoCollectNormal,
        Self::CobDriftFixed,
        Self::MushroomsAwake,
        Self::ZombiesDieAtHouse,
        Self::PlantingRestrictionsIgnored,
        Self::ProfileReadonly,
        Self::ZombieSpawnStopped,
    ];

    const fn mask(self) -> u32 {
        1 << self as u8
    }

    fn bundle(self) -> &'static PatchBundle {
        match self {
            Self::SeedRechargeIgnored => &super::mods::seed_recharge_ignored::BUNDLE,
            Self::SunCostIgnored => &super::mods::sun_cost_ignored::BUNDLE,
            Self::FogRevealed => &super::mods::fog_revealed::BUNDLE,
            Self::VaseContentsVisible => &super::mods::vase_contents_visible::BUNDLE,
            Self::InstantIceAndAshEffects => &super::mods::instant_ice_and_ash_effects::BUNDLE,
            Self::CobFixedDelay => &super::mods::cob_fixed_delay::BUNDLE,
            Self::CobRechargeShortened => &super::mods::cob_recharge_shortened::BUNDLE,
            Self::JackExplosionsDisabled => &super::mods::jack_explosions_disabled::BUNDLE,
            Self::PepperExplosionsDisabled => &super::mods::pepper_explosions_disabled::BUNDLE,
            Self::ItemDropDisabled => &super::mods::item_drop_disabled::BUNDLE,
            Self::SpecialEventsDisabled => &super::mods::special_events_disabled::BUNDLE,
            Self::NaturalSunDropDisabled => &super::mods::natural_sun_drop_disabled::BUNDLE,
            Self::AutoCollectNormal => &super::mods::auto_collect_normal::BUNDLE,
            Self::CobDriftFixed => &super::mods::cob_drift_fixed::BUNDLE,
            Self::MushroomsAwake => &super::mods::mushrooms_awake::BUNDLE,
            Self::ZombiesDieAtHouse => &super::mods::zombies_die_at_house::BUNDLE,
            Self::PlantingRestrictionsIgnored => &super::mods::planting_restrictions_ignored::BUNDLE,
            Self::ProfileReadonly => &super::mods::profile_readonly::BUNDLE,
            Self::ZombieSpawnStopped => &super::mods::zombie_spawn_stopped::BUNDLE,
        }
    }

    pub(crate) fn enabled(self) -> Result<bool> {
        self.bundle().enabled()
    }
}

#[derive(Debug, Default)]
struct PatchLeaseManager {
    bool_patches: u32,
    original_kernel_pult_rule: Option<KernelPultProjectileRule>,
    original_maid_cheat: Option<MaidCheat>,
}

thread_local! {
    static MANAGER: RefCell<PatchLeaseManager> = RefCell::new(PatchLeaseManager::default());
}

pub(crate) fn set_bool_patch(patch: BoolPatchId, enabled: bool) -> Result<()> {
    if enabled {
        MANAGER.with_borrow_mut(|manager| manager.acquire_bool_patch(patch))
    } else {
        MANAGER.with_borrow_mut(|manager| manager.release_bool_patch(patch))
    }
}

pub(crate) fn bool_patch_owned(patch: BoolPatchId) -> bool {
    MANAGER.with_borrow(|manager| manager.bool_patches & patch.mask() != 0)
}

pub(crate) fn set_kernel_pult_projectile_rule(rule: KernelPultProjectileRule) -> Result<()> {
    MANAGER.with_borrow_mut(|manager| manager.set_kernel_pult_projectile_rule(rule))
}

pub(crate) fn set_plant_damage_rule(rule: PlantDamageRule) -> Result<()> {
    MANAGER.with_borrow_mut(|manager| manager.set_plant_damage_rule(rule))
}

pub(crate) fn set_maid_cheat(state: MaidCheat) -> Result<()> {
    MANAGER.with_borrow_mut(|manager| manager.set_maid_cheat(state))
}

pub(crate) fn release_all_owned_patches() -> Result<()> {
    MANAGER.with_borrow_mut(PatchLeaseManager::release_all)
}

fn try_each_best_effort<T, E>(
    items: impl IntoIterator<Item = T>, mut try_item: impl FnMut(T) -> std::result::Result<(), E>,
) -> std::result::Result<(), E> {
    let mut first_error = None;
    for item in items {
        if let Err(error) = try_item(item)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

impl PatchLeaseManager {
    fn acquire_bool_patch(&mut self, patch: BoolPatchId) -> Result<()> {
        if self.bool_patches & patch.mask() != 0 {
            return Ok(());
        }
        if patch.bundle().enabled()? {
            return Err(Pvz1051Error::Patch(format!(
                "{} is already enabled without the 1051 runner owning it",
                patch.bundle().name
            )));
        }
        patch.bundle().set_enabled(true)?;
        self.bool_patches |= patch.mask();
        Ok(())
    }

    fn release_bool_patch(&mut self, patch: BoolPatchId) -> Result<()> {
        if self.bool_patches & patch.mask() == 0 {
            return Ok(());
        }
        patch.bundle().set_enabled(false)?;
        self.bool_patches &= !patch.mask();
        Ok(())
    }

    fn set_kernel_pult_projectile_rule(&mut self, rule: KernelPultProjectileRule) -> Result<()> {
        let original = match self.original_kernel_pult_rule {
            Some(original) => original,
            None => super::variant::kernel_pult_projectile_rule::rule()?,
        };
        super::variant::kernel_pult_projectile_rule::set_rule(rule)?;
        self.original_kernel_pult_rule.get_or_insert(original);
        Ok(())
    }

    fn set_plant_damage_rule(&mut self, rule: PlantDamageRule) -> Result<()> {
        crate::impls::event::set_owned_damage_rule(rule)
    }

    fn set_maid_cheat(&mut self, state: MaidCheat) -> Result<()> {
        let original = match self.original_maid_cheat {
            Some(original) => original,
            None => super::variant::maid_cheat::state()?,
        };
        super::variant::maid_cheat::set(state)?;
        self.original_maid_cheat.get_or_insert(original);
        Ok(())
    }

    fn release_all(&mut self) -> Result<()> {
        let bool_result = try_each_best_effort(BoolPatchId::ALL, |patch| self.release_bool_patch(patch));
        let variant_result = self.restore_variant_rules();
        bool_result.and(variant_result)
    }

    fn restore_variant_rules(&mut self) -> Result<()> {
        let mut first_error = None;
        if let Some(original) = self.original_kernel_pult_rule
            && let Err(error) = super::variant::kernel_pult_projectile_rule::set_rule(original)
        {
            first_error = Some(error);
        } else {
            self.original_kernel_pult_rule = None;
        }
        if let Err(error) = crate::impls::event::restore_owned_damage_rule()
            && first_error.is_none()
        {
            first_error = Some(error);
        }
        if let Some(original) = self.original_maid_cheat {
            match super::variant::maid_cheat::set(original) {
                Ok(()) => self.original_maid_cheat = None,
                Err(error) => {
                    first_error.get_or_insert(error);
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug)]
    struct PatchRange {
        owner: &'static str,
        start: usize,
        end: usize,
    }

    impl PatchRange {
        fn new(owner: &'static str, start: usize, len: usize) -> Self {
            Self {
                owner,
                start,
                end: start.checked_add(len).expect("test patch range must not overflow"),
            }
        }

        fn overlaps(self, other: Self) -> bool {
            self.start < other.end && other.start < self.end
        }
    }

    fn current_lease_and_rule_ranges() -> Vec<PatchRange> {
        let mut ranges = Vec::new();
        for patch in &BoolPatchId::ALL {
            for spec in patch.bundle().specs {
                ranges.push(PatchRange::new(spec.name, spec.addr, spec.len()));
            }
        }
        for entry in super::super::variant::kernel_pult_projectile_rule::BUNDLE.entries {
            ranges.push(PatchRange::new(entry.name, entry.addr, entry.variants[0].1.len()));
        }
        for entry in super::super::variant::plant_damage_rule::BUNDLE.entries {
            ranges.push(PatchRange::new(entry.name, entry.addr, entry.variants[0].1.len()));
        }
        for entry in super::super::variant::maid_cheat::BUNDLE.entries {
            ranges.push(PatchRange::new(entry.name, entry.addr, entry.variants[0].1.len()));
        }
        ranges.push(PatchRange::new(
            "measurement logical-frame Board::Update call",
            0x4526f7,
            5,
        ));
        ranges
    }

    #[test]
    fn best_effort_release_keeps_trying_after_the_first_error() {
        let mut attempted = Vec::new();
        let result = try_each_best_effort([1, 2, 3], |item| {
            attempted.push(item);
            match item {
                1 => Err("first restore failed"),
                3 => Err("later restore failed"),
                _ => Ok(()),
            }
        });

        assert_eq!(attempted, [1, 2, 3]);
        assert_eq!(result, Err("first restore failed"));
    }

    #[test]
    fn current_lease_and_rule_patch_ranges_do_not_overlap() {
        let ranges = current_lease_and_rule_ranges();
        for (index, left) in ranges.iter().copied().enumerate() {
            for right in ranges[index + 1..].iter().copied() {
                assert!(
                    !left.overlaps(right),
                    "current patch owners overlap: {left:?} and {right:?}"
                );
            }
        }
    }

    #[test]
    fn future_measurement_single_owner_ranges_find_expected_m20_conflicts() {
        let current = current_lease_and_rule_ranges();
        let cases = [
            (
                PatchRange::new("future Jack action trampoline", 0x41cc2f, 0x0f),
                &["Board::KillAllPlantsInRadius::hit_action_branch_opcode"][..],
            ),
            (
                PatchRange::new("future bite HP-write trampoline", 0x52fcf0, 4),
                &[
                    "Zombie::EatPlant::hp_write_modrm_byte",
                    "Zombie::EatPlant::hp_write_immediate_byte",
                ][..],
            ),
            (
                PatchRange::new("future basketball HP-write trampoline", 0x46d7a6, 3),
                &["Projectile::UpdateLobMotion::basketball_hp_write"][..],
            ),
        ];

        for (future, expected) in cases {
            let mut actual = current
                .iter()
                .copied()
                .filter(|range| future.overlaps(*range))
                .map(|range| range.owner)
                .collect::<Vec<_>>();
            actual.sort_unstable();
            let mut expected = expected.to_vec();
            expected.sort_unstable();
            assert_eq!(actual, expected, "unexpected current owners for {future:?}");
        }
    }
}
