use super::*;

impl rsvz_backend_api::DanceModeBackend for PeBackend {
    fn set_dance_mode(&self, enabled: bool) -> Result<()> {
        self.with_current_world(|world| world.scene().set_dance_mode(enabled))??;
        Ok(())
    }
}

impl SunQueryBackend for PeBackend {
    fn sun(&self) -> Result<u32> {
        self.with_current_world(|world| {
            let sun = world.scene().sun_data().as_ptr();
            read_pe_raw_field!(sun, sun)
        })
    }
}

impl SunWriteBackend for PeBackend {
    fn set_sun(&self, value: u32) -> Result<()> {
        i32::try_from(value).map_err(|_error| PeBackendError::NumericOutOfRange("sun"))?;
        let sun = self.with_current_world(|world| world.scene().sun_data().as_ptr())?;
        write_pe_raw_field!(sun, value, sun);
        Ok(())
    }
}

impl DancerClockWriteBackend for PeBackend {
    fn set_dancer_clock(&self, value: u32) -> Result<()> {
        self.with_current_world(|world| world.restore_dancer_clock(value))??;
        Ok(())
    }
}

impl SeedRuleEditBackend for PeBackend {
    fn set_seed_recharge_ignored(&self, enabled: bool) -> Result<()> {
        self.with_current_world(|world| world.scene().set_seed_recharge_ignored(enabled))??;
        Ok(())
    }

    fn seed_recharge_ignored(&self) -> Result<bool> {
        self.with_current_world(|world| world.scene().seed_recharge_ignored())
    }
}

impl SunCostRuleEditBackend for PeBackend {
    fn set_sun_cost_ignored(&self, enabled: bool) -> Result<()> {
        self.with_current_world(|world| world.scene().set_sun_cost_ignored(enabled))??;
        Ok(())
    }

    fn sun_cost_ignored(&self) -> Result<bool> {
        self.with_current_world(|world| world.scene().sun_cost_ignored())
    }
}

impl PlantingRuleEditBackend for PeBackend {
    fn easy_planting_cheat(&self) -> Result<bool> {
        self.with_current_world(|world| world.scene().easy_planting_cheat())
    }

    fn set_easy_planting_cheat(&self, enabled: bool) -> Result<()> {
        self.with_current_world(|world| world.scene().set_easy_planting_cheat(enabled))??;
        Ok(())
    }

    fn planting_restrictions_ignored(&self) -> Result<bool> {
        self.with_current_world(|world| world.scene().planting_restrictions_ignored())
    }

    fn set_planting_restrictions_ignored(&self, enabled: bool) -> Result<()> {
        self.with_current_world(|world| world.scene().set_planting_restrictions_ignored(enabled))??;
        Ok(())
    }
}

impl PlantEffectRuleEditBackend for PeBackend {
    fn instant_ice_and_ash_effects(&self) -> Result<bool> {
        self.with_current_world(|world| world.scene().instant_special_effects())
    }

    fn set_instant_ice_and_ash_effects(&self, enabled: bool) -> Result<()> {
        self.with_current_world(|world| world.scene().set_instant_special_effects(enabled))??;
        Ok(())
    }

    fn mushrooms_awake(&self) -> Result<bool> {
        self.with_current_world(|world| world.scene().mushrooms_awake())
    }

    fn set_mushrooms_awake(&self, enabled: bool) -> Result<()> {
        self.with_current_world(|world| world.scene().set_mushrooms_awake(enabled))??;
        Ok(())
    }
}

impl DropRuleEditBackend for PeBackend {
    fn item_drop_disabled(&self) -> Result<bool> {
        Ok(true)
    }

    fn set_item_drop_disabled(&self, enabled: bool) -> Result<()> {
        if enabled {
            Ok(())
        } else {
            Err(PeBackendError::Unsupported(
                "PE has no item drop subsystem to re-enable",
            ))
        }
    }

    fn natural_sun_drop_disabled(&self) -> Result<bool> {
        self.with_current_world(|world| world.scene().natural_sun_drop_disabled())
    }

    fn set_natural_sun_drop_disabled(&self, enabled: bool) -> Result<()> {
        self.with_current_world(|world| world.scene().set_natural_sun_drop_disabled(enabled))??;
        Ok(())
    }

    fn set_natural_sun_generated(&self, count: NonNegativeI32) -> Result<()> {
        let sun = self.with_current_world(|world| world.scene().sun_data().as_ptr())?;
        write_pe_raw_field!(sun, count.get() as u32, natural_sun_generated);
        Ok(())
    }

    fn set_natural_sun_countdown(&self, countdown: NonNegativeI32) -> Result<()> {
        let sun = self.with_current_world(|world| world.scene().sun_data().as_ptr())?;
        write_pe_raw_field!(sun, countdown.get() as u32, natural_sun_countdown);
        Ok(())
    }
}

impl ZombieRuleEditBackend for PeBackend {
    fn jack_explosions_disabled(&self) -> Result<bool> {
        self.with_current_world(|world| world.scene().jack_explosions_disabled())
    }

    fn set_jack_explosions_disabled(&self, enabled: bool) -> Result<()> {
        self.with_current_world(|world| world.scene().set_jack_explosions_disabled(enabled))??;
        Ok(())
    }

    fn pepper_explosions_disabled(&self) -> Result<bool> {
        Err(PeBackendError::Unsupported("PE has no pepper zombie explosion rule"))
    }

    fn set_pepper_explosions_disabled(&self, _enabled: bool) -> Result<()> {
        Err(PeBackendError::Unsupported("PE has no pepper zombie explosion rule"))
    }

    fn special_events_disabled(&self) -> Result<bool> {
        self.with_current_world(|world| world.scene().special_events_disabled())
    }

    fn set_special_events_disabled(&self, enabled: bool) -> Result<()> {
        self.with_current_world(|world| world.scene().set_special_events_disabled(enabled))??;
        Ok(())
    }

    fn zombie_spawn_stopped(&self) -> Result<bool> {
        self.with_current_world(|world| world.scene().zombie_spawn_stopped())
    }

    fn set_zombie_spawn_stopped(&self, enabled: bool) -> Result<()> {
        self.with_current_world(|world| world.scene().set_zombie_spawn_stopped(enabled))??;
        Ok(())
    }

    fn zombies_die_at_house(&self) -> Result<bool> {
        self.with_current_world(|world| world.scene().zombies_die_at_house())
    }

    fn set_zombies_die_at_house(&self, enabled: bool) -> Result<()> {
        self.with_current_world(|world| world.scene().set_zombies_die_at_house(enabled))??;
        Ok(())
    }
}

impl CobRuleEditBackend for PeBackend {
    fn cob_fixed_delay(&self) -> Result<bool> {
        self.with_current_world(|world| world.scene().cob_fixed_delay())
    }

    fn set_cob_fixed_delay(&self, enabled: bool) -> Result<()> {
        self.with_current_world(|world| world.scene().set_cob_fixed_delay(enabled))??;
        Ok(())
    }

    fn cob_recharge_shortened(&self) -> Result<bool> {
        self.with_current_world(|world| world.scene().cob_recharge_shortened())
    }

    fn set_cob_recharge_shortened(&self, enabled: bool) -> Result<()> {
        self.with_current_world(|world| world.scene().set_cob_recharge_shortened(enabled))??;
        Ok(())
    }

    fn cob_drift_fixed(&self) -> Result<bool> {
        self.with_current_world(|world| world.scene().cob_drift_fixed())
    }

    fn set_cob_drift_fixed(&self, enabled: bool) -> Result<()> {
        self.with_current_world(|world| world.scene().set_cob_drift_fixed(enabled))??;
        Ok(())
    }
}

impl CommonZombieDanceBackend for PeBackend {
    fn set_common_zombie_dance(&self, dance: RefreshDance) -> Result<()> {
        let dance = match dance {
            RefreshDance::Unchanged | RefreshDance::None => pe_rs::ZombieDanceCheat::None,
            RefreshDance::Fast => pe_rs::ZombieDanceCheat::Fast,
            RefreshDance::Slow => pe_rs::ZombieDanceCheat::Slow,
        };
        self.with_current_world(|world| world.scene().set_common_zombie_dance(dance))??;
        Ok(())
    }
}

impl CobImpactDelayBackend for PeBackend {
    fn set_cob_impact_delay(&self, enabled: bool) -> Result<()> {
        self.with_current_world(|world| world.scene().set_cob_delay_disabled(!enabled))??;
        Ok(())
    }
}

impl ProjectileRuleEditBackend for PeBackend {
    fn kernel_pult_projectile_rule(&self) -> Result<KernelPultProjectileRule> {
        let rule = self.with_current_world(|world| world.scene().kernel_pult_rule())??;
        Ok(match rule {
            pe_rs::KernelPultRule::Normal => KernelPultProjectileRule::Normal,
            pe_rs::KernelPultRule::AlwaysButter => KernelPultProjectileRule::AlwaysButter,
            pe_rs::KernelPultRule::AlwaysKernel => KernelPultProjectileRule::AlwaysKernel,
        })
    }

    fn set_kernel_pult_projectile_rule(&self, rule: KernelPultProjectileRule) -> Result<()> {
        let rule = match rule {
            KernelPultProjectileRule::Normal => pe_rs::KernelPultRule::Normal,
            KernelPultProjectileRule::AlwaysButter => pe_rs::KernelPultRule::AlwaysButter,
            KernelPultProjectileRule::AlwaysKernel => pe_rs::KernelPultRule::AlwaysKernel,
        };
        self.with_current_world(|world| world.scene().set_kernel_pult_rule(rule))??;
        Ok(())
    }
}

impl PlantDamageRuleEditBackend for PeBackend {
    fn plant_damage_rule(&self) -> Result<PlantDamageRule> {
        let rule = self.with_current_world(|world| world.scene().plant_damage_rule())??;
        Ok(match rule {
            pe_rs::PlantDamageRule::Normal => PlantDamageRule::Normal,
            pe_rs::PlantDamageRule::Invincible => PlantDamageRule::Invincible,
            pe_rs::PlantDamageRule::Weak => PlantDamageRule::Weak,
        })
    }

    fn set_plant_damage_rule(&self, rule: PlantDamageRule) -> Result<()> {
        let rule = match rule {
            PlantDamageRule::Normal => pe_rs::PlantDamageRule::Normal,
            PlantDamageRule::Invincible => pe_rs::PlantDamageRule::Invincible,
            PlantDamageRule::Weak => pe_rs::PlantDamageRule::Weak,
        };
        self.with_current_world(|world| world.scene().set_plant_damage_rule(rule))??;
        Ok(())
    }
}

impl ProfileReadonlyBackend for PeBackend {
    fn set_profile_readonly(&self, _readonly: bool) -> Result<()> {
        Ok(())
    }
}

impl FastForwardBackend for PeBackend {
    fn start_fast_forward(&self, _options: FastForwardOptions) -> Result<()> {
        self.set_fast_forward_active(true)
    }

    fn stop_fast_forward(&self, _reason: FastForwardStopReason) -> Result<()> {
        self.set_fast_forward_active(false)
    }

    fn fast_forward_active(&self) -> bool {
        self.fast_forward_hint_active()
    }
}

impl SeedChooserFastForwardBackend for PeBackend {
    fn request_seed_chooser_fast_forward(&self, _options: SeedChooserFastForwardOptions) -> Result<()> {
        Ok(())
    }
}

impl PlantHealthWriteBackend for PeBackend {
    fn set_plant_hp<'a>(&'a self, handle: Self::PlantHandle<'a>, hp: PositiveHp) -> Result<ObjectEditOutcome> {
        write_pe_raw_field!(handle.as_mut_ptr(), hp.get(), hp);
        Ok(ObjectEditOutcome::Applied)
    }
}

impl ZombieBodyHealthWriteBackend for PeBackend {
    fn set_zombie_body_hp<'a>(&'a self, handle: Self::ZombieHandle<'a>, hp: PositiveHp) -> Result<ObjectEditOutcome> {
        write_pe_raw_field!(handle.as_mut_ptr(), hp.get(), hp);
        Ok(ObjectEditOutcome::Applied)
    }
}

impl ZombieXWriteBackend for PeBackend {
    fn set_zombie_x<'a>(&'a self, handle: Self::ZombieHandle<'a>, x: I32RepresentableF32) -> Result<ObjectEditOutcome> {
        write_pe_raw_field!(handle.as_mut_ptr(), x.get(), x);
        write_pe_raw_field!(handle.as_mut_ptr(), x.truncated(), int_x);
        Ok(ObjectEditOutcome::Applied)
    }
}

impl ZombiePhaseCountdownWriteBackend for PeBackend {
    fn set_zombie_phase_countdown<'a>(
        &'a self, handle: Self::ZombieHandle<'a>, countdown: NonNegativeI32,
    ) -> Result<ObjectEditOutcome> {
        write_pe_raw_field!(handle.as_mut_ptr(), countdown.get(), countdown.action);
        Ok(ObjectEditOutcome::Applied)
    }
}
