use super::*;

impl PlantReadBackend for PortableBackend {
    type PlantHandle<'a> = PortablePlantHandle<'a>;
    type PlantIter<'a> = PortablePlantIter<'a>;

    fn plants(&self) -> Result<Self::PlantIter<'_>> {
        Ok({
            let pool = crate::access::plant_pool(&self.world()?);
            let limit = pool.max_used_count();
            crate::handles::PortablePlantIter::new(self, limit)
        })
    }

    fn plant(&self, id: PlantId) -> Result<Option<Self::PlantHandle<'_>>> {
        Ok(self.plant_in_pool(crate::access::plant_pool(&self.world()?), id))
    }

    fn plant_id<'a>(&'a self, handle: Self::PlantHandle<'a>) -> PlantId {
        self.plant_id_from_handle(handle)
    }

    fn plant_kind<'a>(&'a self, handle: Self::PlantHandle<'a>) -> Result<PlantKind> {
        let packet = read_raw!(handle, mSeedType);
        let effective = if packet == PlantKind::Imitator.code() {
            read_raw!(handle, mImitaterType)
        } else {
            packet
        };
        PlantKind::try_from_code(effective).map_err(|_| invalid_kind("plant", effective))
    }

    fn plant_raw_kind<'a>(&'a self, handle: Self::PlantHandle<'a>) -> Result<PlantKind> {
        let raw = read_raw!(handle, mSeedType);
        PlantKind::try_from_code(raw).map_err(|_| invalid_kind("plant", raw))
    }

    fn plant_x<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32 {
        read_raw!(handle, _base.mX)
    }

    fn plant_state<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32 {
        read_raw!(handle, mState)
    }

    fn plant_is_squished<'a>(&'a self, handle: Self::PlantHandle<'a>) -> bool {
        read_raw!(handle, mSquished)
    }

    fn plant_on_bungee_state<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32 {
        read_raw!(handle, mOnBungeeState)
    }

    fn plant_is_sleeping<'a>(&'a self, handle: Self::PlantHandle<'a>) -> bool {
        read_raw!(handle, mIsAsleep)
    }

    fn plant_wake_up_counter<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32 {
        read_raw!(handle, mWakeUpCounter)
    }

    fn plant_state_countdown<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32 {
        read_raw!(handle, mStateCountdown)
    }

    fn plant_effect_countdown<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32 {
        read_raw!(handle, mDoSpecialCountdown)
    }

    fn plant_disappear_countdown<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32 {
        read_raw!(handle, mDisappearCountdown)
    }

    fn plant_reanim_circulation<'a>(&'a self, handle: Self::PlantHandle<'a>) -> Result<Option<f32>> {
        let world = self.world()?;
        // SAFETY: the plant is address-valid throughout the current backend borrow.
        let plant = unsafe { pvzp_rs::Borrowed::from_non_null(handle.as_non_null()) };
        let Some(reanimation) = world.plant_reanimation(plant)? else {
            return Ok(None);
        };
        // SAFETY: native lookup resolved a live body reanimation for this immediate scalar read.
        Ok(Some(unsafe {
            std::ptr::addr_of!((*reanimation.as_ptr()).mAnimTime).read()
        }))
    }

    fn plant_eating_flash_counter<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32 {
        read_raw!(handle, mEatenFlashCountdown)
    }

    fn plant_hp<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32 {
        read_raw!(handle, mPlantHealth)
    }

    fn plant_row<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32 {
        read_raw!(handle, _base.mRow)
    }
    fn plant_col<'a>(&'a self, handle: Self::PlantHandle<'a>) -> i32 {
        read_raw!(handle, mPlantCol)
    }

    fn plant_is_alive<'a>(&'a self, handle: Self::PlantHandle<'a>) -> bool {
        handle.is_live()
    }
}

impl ImitatorMorphBackend for PortableBackend {
    fn imitator_morph_successor(&self, placeholder: PlantId) -> Result<Option<PlantId>> {
        Ok({
            self.world()?
                .imitater_successor(placeholder.raw())
                .map(PlantId::from_raw)
        })
    }
}

impl PlantCostBackend for PortableBackend {
    fn current_plant_cost(&self, selection: CheckedCardSelection) -> Result<SunAmount> {
        let (packet, imitater) = card_parts(selection);
        let cost = self.world()?.current_plant_cost(packet, imitater)?;
        let cost = u32::try_from(cost).map_err(|_| PortableBackendError::NumericOutOfRange("plant cost"))?;
        SunAmount::new(cost).map_err(|_| PortableBackendError::NumericOutOfRange("plant cost"))
    }
}

impl PlantPlacementBackend for PortableBackend {
    fn can_plant_at(&self, selection: CheckedCardSelection, grid: Grid) -> Result<Plantability> {
        validate_grid(self, grid)?;
        let reason = self
            .world()?
            .can_plant_at(selection.effective_kind().code(), grid.col, grid.row)?;
        Ok(Plantability::from_game_rule_code(reason))
    }
}

impl PlantPoolBackend for PortableBackend {
    fn plant_pool_active_count(&self) -> Result<u32> {
        Ok(crate::access::plant_pool(&self.world()?).active_count())
    }

    fn plant_pool_capacity(&self) -> Result<u32> {
        Ok(crate::access::plant_pool(&self.world()?).capacity())
    }
}

impl PlantCreateBackend for PortableBackend {
    fn morph_imitator<'a>(&'a self, placeholder: PortablePlantHandle<'a>) -> Result<PortablePlantHandle<'a>> {
        let world = self.world()?;
        // SAFETY: the handle belongs to the current borrowed native Board.
        let plant = unsafe { pvzp_rs::Borrowed::from_non_null(placeholder.as_non_null()) };
        world
            .plant_imitater_morph(plant)?
            .map(pvzp_rs::Borrowed::as_non_null)
            .map(PortablePlantHandle::new)
            .ok_or(PortableBackendError::OperationRejected("imitater morph"))
    }
    fn new_plant<'a>(&'a self, selection: CheckedCardSelection, grid: Grid) -> Result<PortablePlantHandle<'a>> {
        validate_grid(self, grid)?;
        let (packet, imitater) = card_parts(selection);
        self.world()?
            .new_plant(grid.col, grid.row, packet, imitater)?
            .map(pvzp_rs::Borrowed::as_non_null)
            .map(PortablePlantHandle::new)
            .ok_or(PortableBackendError::OperationRejected("new plant"))
    }

    fn add_plant<'a>(&'a self, selection: CheckedCardSelection, grid: Grid) -> Result<PortablePlantHandle<'a>> {
        validate_grid(self, grid)?;
        let (packet, imitater) = card_parts(selection);
        self.world()?
            .add_plant(grid.col, grid.row, packet, imitater)?
            .map(pvzp_rs::Borrowed::as_non_null)
            .map(PortablePlantHandle::new)
            .ok_or(PortableBackendError::OperationRejected("add plant"))
    }
}

impl PlantRemoveBackend for PortableBackend {
    fn remove_plant<'a>(&'a self, handle: Self::PlantHandle<'a>) -> Result<()> {
        let world = self.world()?;
        // SAFETY: the handle is bound to this current backend/world borrow.
        let plant = unsafe { pvzp_rs::Borrowed::from_non_null(handle.as_non_null()) };
        world.plant_die(plant).map_err(Into::into)
    }
}

impl PlantSleepBackend for PortableBackend {
    fn set_plant_sleeping<'a>(&'a self, handle: Self::PlantHandle<'a>, asleep: bool) -> Result<()> {
        let world = self.world()?;
        // SAFETY: the handle is bound to this current backend/world borrow.
        let plant = unsafe { pvzp_rs::Borrowed::from_non_null(handle.as_non_null()) };
        world.plant_set_sleeping(plant, asleep).map_err(Into::into)
    }

    fn set_plant_wake_up_counter<'a>(&'a self, handle: Self::PlantHandle<'a>, counter: i32) -> Result<()> {
        write_raw!(handle, counter, mWakeUpCounter);
        Ok(())
    }
}

impl PlantIdleAnimationBackend for PortableBackend {
    fn play_plant_idle_animation<'a>(&'a self, handle: Self::PlantHandle<'a>, fps: PositiveFiniteF32) -> Result<()> {
        let world = self.world()?;
        // SAFETY: the handle is bound to this current backend/world borrow.
        let plant = unsafe { pvzp_rs::Borrowed::from_non_null(handle.as_non_null()) };
        world.plant_play_idle(plant, fps.get()).map_err(Into::into)
    }
}

impl PlantStateCountdownWriteBackend for PortableBackend {
    fn set_plant_state_countdown<'a>(&'a self, handle: Self::PlantHandle<'a>, countdown: i32) -> Result<()> {
        write_raw!(handle, countdown, mStateCountdown);
        Ok(())
    }
}

impl PlantStateWriteBackend for PortableBackend {
    fn set_plant_state<'a>(&'a self, handle: Self::PlantHandle<'a>, state: i32) -> Result<()> {
        write_raw!(handle, state, mState);
        Ok(())
    }
}

impl PlantVisualStateBackend for PortableBackend {
    fn set_plant_eating_flash_counter<'a>(&'a self, plant: Self::PlantHandle<'a>, value: i32) -> Result<()> {
        write_raw!(plant, value, mEatenFlashCountdown);
        Ok(())
    }

    fn update_plant_reanim_color<'a>(&'a self, plant: Self::PlantHandle<'a>) -> Result<()> {
        let world = self.world()?;
        // SAFETY: the handle is bound to this current backend/world borrow.
        let plant = unsafe { pvzp_rs::Borrowed::from_non_null(plant.as_non_null()) };
        world.plant_update_reanim_color(plant).map_err(Into::into)
    }
}

impl CobFireBackend for PortableBackend {
    fn fire_cob<'a>(&'a self, cob: Self::PlantHandle<'a>, target: PixelPos) -> Result<()> {
        if self.plant_kind(cob)? != PlantKind::CobCannon {
            return Err(PortableBackendError::OperationRejected("plant is not CobCannon"));
        }
        let world = self.world()?;
        // SAFETY: the handle is bound to this current backend/world borrow.
        let cob = unsafe { pvzp_rs::Borrowed::from_non_null(cob.as_non_null()) };
        world.plant_fire_cob(cob, target.x, target.y).map_err(Into::into)
    }
}

impl PlantEffectRuleEditBackend for PortableBackend {
    fn instant_ice_and_ash_effects(&self) -> Result<bool> {
        self.world()?;
        Ok(pvzp_rs::modifier::instant_ice_and_ash_effects())
    }

    fn set_instant_ice_and_ash_effects(&self, enabled: bool) -> Result<()> {
        self.world()?;
        pvzp_rs::modifier::set_instant_ice_and_ash_effects(enabled).map_err(Into::into)
    }

    fn mushrooms_awake(&self) -> Result<bool> {
        self.world()?;
        Ok(pvzp_rs::modifier::mushrooms_awake())
    }

    fn set_mushrooms_awake(&self, enabled: bool) -> Result<()> {
        self.world()?;
        pvzp_rs::modifier::set_mushrooms_awake(enabled).map_err(Into::into)
    }
}

impl PlantingRuleEditBackend for PortableBackend {
    fn easy_planting_cheat(&self) -> Result<bool> {
        self.world()?;
        pvzp_rs::modifier::easy_planting_cheat().map_err(Into::into)
    }

    fn set_easy_planting_cheat(&self, enabled: bool) -> Result<()> {
        self.world()?;
        pvzp_rs::modifier::set_easy_planting_cheat(enabled).map_err(Into::into)
    }

    fn planting_restrictions_ignored(&self) -> Result<bool> {
        self.world()?;
        Ok(pvzp_rs::modifier::planting_restrictions_ignored())
    }

    fn set_planting_restrictions_ignored(&self, enabled: bool) -> Result<()> {
        self.world()?;
        pvzp_rs::modifier::set_planting_restrictions_ignored(enabled).map_err(Into::into)
    }
}

impl PlantDamageRuleEditBackend for PortableBackend {
    fn plant_damage_rule(&self) -> Result<PlantDamageRule> {
        self.world()?;
        match pvzp_rs::modifier::plant_damage_rule()? {
            0 => Ok(PlantDamageRule::Normal),
            1 => Ok(PlantDamageRule::Invincible),
            2 => Ok(PlantDamageRule::Weak),
            value => Err(invalid_kind("plant damage rule", value)),
        }
    }

    fn set_plant_damage_rule(&self, rule: PlantDamageRule) -> Result<()> {
        self.world()?;
        let raw = match rule {
            PlantDamageRule::Normal => 0,
            PlantDamageRule::Invincible => 1,
            PlantDamageRule::Weak => 2,
        };
        pvzp_rs::modifier::set_plant_damage_rule(raw).map_err(Into::into)
    }
}

impl PlantHealthWriteBackend for PortableBackend {
    fn set_plant_hp<'a>(&'a self, handle: Self::PlantHandle<'a>, hp: PositiveHp) -> Result<ObjectEditOutcome> {
        write_raw!(handle, hp.get(), mPlantHealth);
        Ok(ObjectEditOutcome::Applied)
    }
}

impl PlantEffectCountdownWriteBackend for PortableBackend {
    fn set_plant_effect_countdown<'a>(
        &'a self, handle: Self::PlantHandle<'a>, target_countdown: NonNegativeI32,
    ) -> Result<ObjectEditOutcome> {
        write_raw!(handle, target_countdown.get(), mDoSpecialCountdown);
        Ok(ObjectEditOutcome::Applied)
    }
}

impl PlantContactBackend for PortableBackend {
    fn plant_hit_box<'a>(&'a self, plant: Self::PlantHandle<'a>) -> Result<ContactRect> {
        let world = self.world()?;
        // SAFETY: the handle is bound to this current backend/world borrow.
        let plant = unsafe { pvzp_rs::Borrowed::from_non_null(plant.as_non_null()) };
        contact_rect(world.plant_hit_box(plant))
    }

    fn plant_attack_rect<'a>(&'a self, plant: Self::PlantHandle<'a>, weapon: PlantWeapon) -> Result<ContactRect> {
        let world = self.world()?;
        // SAFETY: the handle is bound to this current backend/world borrow.
        let plant = unsafe { pvzp_rs::Borrowed::from_non_null(plant.as_non_null()) };
        contact_rect(world.plant_attack_rect(plant, weapon as i32)?)
    }

    fn plant_damage_range_flags<'a>(&'a self, plant: Self::PlantHandle<'a>, weapon: PlantWeapon) -> DamageRangeFlags {
        // SAFETY: the live handle and typed weapon satisfy the scalar atom's preconditions.
        let bits =
            unsafe { pvzp_rs::raw::pvzp_rs_plant_damage_range_flags(plant.as_non_null().as_ptr(), weapon as i32) };
        DamageRangeFlags::from_bits_retain(bits)
    }
}

impl PlantStateBackend for PortableBackend {
    fn plant_y<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        read_raw!(plant, _base.mY)
    }

    fn plant_width<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        read_raw!(plant, _base.mWidth)
    }
    fn plant_height<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        read_raw!(plant, _base.mHeight)
    }

    fn plant_max_hp<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        read_raw!(plant, mPlantMaxHealth)
    }

    fn plant_target_x<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        read_raw!(plant, mTargetX)
    }
    fn plant_target_y<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        read_raw!(plant, mTargetY)
    }

    fn plant_target_object<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        read_raw!(plant, mTargetZombieID) as i32
    }

    fn plant_direction<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        if read_raw!(plant, mSeedType) == 17
            && (3..=7).contains(&read_raw!(plant, mState))
            && read_raw!(plant, mTargetX) < read_raw!(plant, _base.mX)
        {
            1
        } else {
            -1
        }
    }

    fn plant_collision<'a>(&'a self, plant: Self::PlantHandle<'a>) -> Result<ContactRect> {
        self.plant_hit_box(plant)
    }

    fn plant_imitater_kind<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        read_raw!(plant, mImitaterType)
    }

    fn plant_recently_eaten_countdown<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        read_raw!(plant, mRecentlyEatenCountdown)
    }
    fn plant_launch_counter<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        read_raw!(plant, mLaunchCounter)
    }
    fn plant_shooting_counter<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        read_raw!(plant, mShootingCounter)
    }
}
