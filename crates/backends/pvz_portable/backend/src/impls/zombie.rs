use super::*;

impl ZombieReadBackend for PortableBackend {
    type ZombieHandle<'a> = PortableZombieHandle<'a>;
    type ZombieIter<'a> = PortableZombieIter<'a>;

    fn zombies(&self) -> Result<Self::ZombieIter<'_>> {
        Ok({
            let pool = crate::access::zombie_pool(&self.world()?);
            let limit = pool.max_used_count();
            crate::handles::PortableZombieIter::new(self, limit)
        })
    }

    fn zombie(&self, id: ZombieId) -> Result<Option<Self::ZombieHandle<'_>>> {
        Ok(self.zombie_in_pool(crate::access::zombie_pool(&self.world()?), id))
    }

    fn zombie_id<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> ZombieId {
        self.zombie_id_from_handle(handle)
    }

    fn zombie_kind<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> Result<ZombieKind> {
        let raw = read_raw!(handle, mZombieType);
        ZombieKind::try_from_code(raw).map_err(|_| invalid_kind("zombie", raw))
    }

    fn zombie_hp<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> i32 {
        read_raw!(handle, mBodyHealth)
    }

    fn zombie_row<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> i32 {
        read_raw!(handle, _base.mRow)
    }

    fn zombie_age<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> i32 {
        read_raw!(handle, mZombieAge)
    }

    fn zombie_is_alive<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> bool {
        handle.is_live()
    }
}

impl ZombieCreateBackend for PortableBackend {
    fn add_zombie_in_row<'a>(
        &'a self, kind: ZombieKind, row: i32, from_wave: i32,
    ) -> Result<Option<PortableZombieHandle<'a>>> {
        validate_row(self, row)?;
        Ok(self
            .world()?
            .add_zombie_in_row(kind.code(), row, from_wave)?
            .map(pvzp_rs::Borrowed::as_non_null)
            .map(PortableZombieHandle::new))
    }

    fn place_zombie<'a>(&'a self, kind: ZombieKind, grid: Grid) -> Result<PortableZombieHandle<'a>> {
        validate_grid(self, grid)?;
        let world = self.world()?;
        let pool = crate::access::zombie_pool(&world);
        if !pool_has_reserved_slot(pool.active_count(), pool.capacity()) {
            return Err(PortableBackendError::OperationRejected(
                "zombie pool has no reserved slot",
            ));
        }
        world
            .place_zombie(kind.code(), grid.col, grid.row)
            .map(pvzp_rs::Borrowed::as_non_null)
            .map(PortableZombieHandle::new)
            .map_err(Into::into)
    }
}

impl ZombieRemoveBackend for PortableBackend {
    fn remove_zombie<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> Result<()> {
        let world = self.world()?;
        // SAFETY: the handle is bound to this current backend/world borrow.
        let zombie = unsafe { pvzp_rs::Borrowed::from_non_null(handle.as_non_null()) };
        world.zombie_die_no_loot(zombie).map_err(Into::into)
    }
}

impl ZombieKillBackend for PortableBackend {
    fn kill_zombie<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> Result<()> {
        // SAFETY: the handle is bound to this current backend/world borrow.
        let zombie = unsafe { pvzp_rs::Borrowed::from_non_null(handle.as_non_null()) };
        zombie.die_with_loot().map_err(Into::into)
    }
}

impl ZombiePositionWriteBackend for PortableBackend {
    fn set_zombie_row_and_y<'a>(
        &'a self, handle: Self::ZombieHandle<'a>, row: i32, y: I32RepresentableF32,
    ) -> Result<()> {
        validate_row(self, row)?;
        write_raw!(handle, row, _base.mRow);
        write_raw!(handle, y.get(), mPosY);
        write_raw!(handle, y.truncated(), _base.mY);
        Ok(())
    }
}

impl ZombieRuleEditBackend for PortableBackend {
    fn jack_explosions_disabled(&self) -> Result<bool> {
        self.world()?;
        Ok(pvzp_rs::modifier::jack_explosions_disabled())
    }

    fn set_jack_explosions_disabled(&self, enabled: bool) -> Result<()> {
        self.world()?;
        pvzp_rs::modifier::set_jack_explosions_disabled(enabled).map_err(Into::into)
    }

    fn pepper_explosions_disabled(&self) -> Result<bool> {
        self.world()?;
        Ok(pvzp_rs::modifier::pepper_explosions_disabled())
    }

    fn set_pepper_explosions_disabled(&self, enabled: bool) -> Result<()> {
        self.world()?;
        pvzp_rs::modifier::set_pepper_explosions_disabled(enabled).map_err(Into::into)
    }

    fn special_events_disabled(&self) -> Result<bool> {
        self.world()?;
        Ok(pvzp_rs::modifier::special_events_disabled())
    }

    fn set_special_events_disabled(&self, enabled: bool) -> Result<()> {
        self.world()?;
        pvzp_rs::modifier::set_special_events_disabled(enabled).map_err(Into::into)
    }

    fn zombie_spawn_stopped(&self) -> Result<bool> {
        self.world()?;
        Ok(pvzp_rs::modifier::zombie_spawn_stopped())
    }

    fn set_zombie_spawn_stopped(&self, enabled: bool) -> Result<()> {
        self.world()?;
        pvzp_rs::modifier::set_zombie_spawn_stopped(enabled).map_err(Into::into)
    }

    fn zombies_die_at_house(&self) -> Result<bool> {
        self.world()?;
        Ok(pvzp_rs::modifier::zombies_die_at_house())
    }

    fn set_zombies_die_at_house(&self, enabled: bool) -> Result<()> {
        self.world()?;
        pvzp_rs::modifier::set_zombies_die_at_house(enabled).map_err(Into::into)
    }
}

impl ZombieBodyHealthWriteBackend for PortableBackend {
    fn set_zombie_body_hp<'a>(&'a self, handle: Self::ZombieHandle<'a>, hp: PositiveHp) -> Result<ObjectEditOutcome> {
        write_raw!(handle, hp.get(), mBodyHealth);
        Ok(ObjectEditOutcome::Applied)
    }
}

impl ZombieXWriteBackend for PortableBackend {
    fn set_zombie_x<'a>(&'a self, handle: Self::ZombieHandle<'a>, x: I32RepresentableF32) -> Result<ObjectEditOutcome> {
        write_raw!(handle, x.get(), mPosX);
        write_raw!(handle, x.truncated(), _base.mX);
        Ok(ObjectEditOutcome::Applied)
    }
}

impl ZombiePhaseCountdownWriteBackend for PortableBackend {
    fn set_zombie_phase_countdown<'a>(
        &'a self, handle: Self::ZombieHandle<'a>, countdown: NonNegativeI32,
    ) -> Result<ObjectEditOutcome> {
        write_raw!(handle, countdown.get(), mPhaseCounter);
        Ok(ObjectEditOutcome::Applied)
    }
}

impl ZombieVerticalPositionBackend for PortableBackend {
    fn zombie_pos_y<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> f32 {
        read_raw!(zombie, mPosY)
    }

    fn zombie_pos_y_based_on_row<'a>(&'a self, zombie: Self::ZombieHandle<'a>, row: i32) -> Result<f32> {
        let world = self.world()?;
        // SAFETY: the handle is bound to this current backend/world borrow.
        let zombie = unsafe { pvzp_rs::Borrowed::from_non_null(zombie.as_non_null()) };
        world.zombie_pos_y_based_on_row(zombie, row).map_err(Into::into)
    }
}

impl ZombieRawFactsBackend for PortableBackend {
    fn zombie_phase<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<ZombiePhase> {
        let raw = read_raw!(zombie, mZombiePhase);
        ZombiePhase::try_from_code(raw).map_err(|_| invalid_kind("zombie phase", raw))
    }

    fn zombie_int_x<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        read_raw!(zombie, _base.mX)
    }

    fn zombie_int_y<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        read_raw!(zombie, _base.mY)
    }

    fn zombie_pos_x<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> f32 {
        read_raw!(zombie, mPosX)
    }

    fn zombie_width<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        read_raw!(zombie, _base.mWidth)
    }

    fn zombie_height<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        read_raw!(zombie, _base.mHeight)
    }

    fn zombie_from_wave<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        read_raw!(zombie, mFromWave)
    }

    fn zombie_height_state<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        read_raw!(zombie, mZombieHeight)
    }

    fn zombie_phase_counter<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        read_raw!(zombie, mPhaseCounter)
    }

    fn zombie_altitude<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> f32 {
        read_raw!(zombie, mAltitude)
    }

    fn zombie_speed_x<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> f32 {
        read_raw!(zombie, mVelX)
    }

    fn zombie_scale<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> f32 {
        read_raw!(zombie, mScaleZombie)
    }

    fn zombie_speed_z<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> f32 {
        read_raw!(zombie, mVelZ)
    }

    fn zombie_is_eating<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        read_raw!(zombie, mIsEating)
    }

    fn zombie_is_disappeared<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        read_raw!(zombie, mDead)
    }

    fn zombie_is_mind_controlled<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        read_raw!(zombie, mMindControlled)
    }

    fn zombie_is_blown_away<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        read_raw!(zombie, mBlowingAway)
    }

    fn zombie_has_flat_tires<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        read_raw!(zombie, mFlatTires)
    }

    fn zombie_has_head<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        self.zombie_is_alive(zombie) && read_raw!(zombie, mHasHead)
    }

    fn zombie_has_object<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        read_raw!(zombie, mHasObject)
    }

    fn zombie_is_in_pool<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        read_raw!(zombie, mInPool)
    }

    fn zombie_is_on_high_ground<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        read_raw!(zombie, mOnHighGround)
    }

    fn zombie_has_yucky_face<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        read_raw!(zombie, mYuckyFace)
    }

    fn zombie_yucky_face_counter<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        read_raw!(zombie, mYuckyFaceCounter)
    }

    fn zombie_frozen_countdown<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        read_raw!(zombie, mIceTrapCounter)
    }

    fn zombie_chilled_countdown<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        read_raw!(zombie, mChilledCounter)
    }

    fn zombie_buttered_countdown<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        read_raw!(zombie, mButteredCounter)
    }

    fn zombie_reanim_anim_time<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<Option<f32>> {
        if !self.zombie_is_alive(zombie) {
            return Ok(None);
        }
        // SAFETY: the zombie handle is bound to this backend/world borrow.
        let zombie = unsafe { pvzp_rs::Borrowed::from_non_null(zombie.as_non_null()) };
        let Some(reanimation) = zombie.reanimation()? else {
            return Ok(None);
        };
        let ptr = reanimation.as_ptr();
        // SAFETY: the native lookup resolved this body reanimation for the immediate read.
        Ok(Some(unsafe { std::ptr::addr_of!((*ptr).mAnimTime).read() }))
    }
    fn zombie_reanim_last_time<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<Option<f32>> {
        if !self.zombie_is_alive(zombie) {
            return Ok(None);
        }
        // SAFETY: the zombie handle is bound to this backend/world borrow.
        let zombie = unsafe { pvzp_rs::Borrowed::from_non_null(zombie.as_non_null()) };
        let Some(reanimation) = zombie.reanimation()? else {
            return Ok(None);
        };
        let ptr = reanimation.as_ptr();
        // SAFETY: the native lookup resolved this body reanimation for the immediate read.
        Ok(Some(unsafe { std::ptr::addr_of!((*ptr).mLastFrameTime).read() }))
    }
    fn zombie_reanim_rate<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<Option<f32>> {
        if !self.zombie_is_alive(zombie) {
            return Ok(None);
        }
        // SAFETY: the zombie handle is bound to this backend/world borrow.
        let zombie = unsafe { pvzp_rs::Borrowed::from_non_null(zombie.as_non_null()) };
        let Some(reanimation) = zombie.reanimation()? else {
            return Ok(None);
        };
        let ptr = reanimation.as_ptr();
        // SAFETY: the native lookup resolved this body reanimation for the immediate read.
        Ok(Some(unsafe { std::ptr::addr_of!((*ptr).mAnimRate).read() }))
    }
    fn zombie_reanim_frame_start<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<Option<i32>> {
        if !self.zombie_is_alive(zombie) {
            return Ok(None);
        }
        // SAFETY: the zombie handle is bound to this backend/world borrow.
        let zombie = unsafe { pvzp_rs::Borrowed::from_non_null(zombie.as_non_null()) };
        let Some(reanimation) = zombie.reanimation()? else {
            return Ok(None);
        };
        let ptr = reanimation.as_ptr();
        // SAFETY: the native lookup resolved this body reanimation for the immediate read.
        Ok(Some(unsafe { std::ptr::addr_of!((*ptr).mFrameStart).read() }))
    }
    fn zombie_reanim_frame_count<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<Option<i32>> {
        if !self.zombie_is_alive(zombie) {
            return Ok(None);
        }
        // SAFETY: the zombie handle is bound to this backend/world borrow.
        let zombie = unsafe { pvzp_rs::Borrowed::from_non_null(zombie.as_non_null()) };
        let Some(reanimation) = zombie.reanimation()? else {
            return Ok(None);
        };
        let ptr = reanimation.as_ptr();
        // SAFETY: the native lookup resolved this body reanimation for the immediate read.
        Ok(Some(unsafe { std::ptr::addr_of!((*ptr).mFrameCount).read() }))
    }
    fn zombie_reanim_loop_type<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<Option<i32>> {
        if !self.zombie_is_alive(zombie) {
            return Ok(None);
        }
        // SAFETY: the zombie handle is bound to this backend/world borrow.
        let zombie = unsafe { pvzp_rs::Borrowed::from_non_null(zombie.as_non_null()) };
        let Some(reanimation) = zombie.reanimation()? else {
            return Ok(None);
        };
        let ptr = reanimation.as_ptr();
        // SAFETY: the native lookup resolved this body reanimation for the immediate read.
        Ok(Some(unsafe { std::ptr::addr_of!((*ptr).mLoopType).read() }))
    }
}

impl ZombieContactBackend for PortableBackend {
    fn zombie_effected_by_damage<'a>(
        &'a self, zombie: Self::ZombieHandle<'a>, flags: DamageRangeFlags,
    ) -> Result<bool> {
        let world = self.world()?;
        // SAFETY: the handle is bound to this current backend/world borrow.
        let zombie = unsafe { pvzp_rs::Borrowed::from_non_null(zombie.as_non_null()) };
        world
            .zombie_effected_by_damage(zombie, flags.bits())
            .map_err(Into::into)
    }

    fn zombie_can_attack_plant<'a>(
        &'a self, zombie: <Self as ZombieReadBackend>::ZombieHandle<'a>,
        plant: <Self as PlantReadBackend>::PlantHandle<'a>, attack_type: i32,
    ) -> Result<bool> {
        let world = self.world()?;
        // SAFETY: both handles are bound to this current backend/world borrow.
        let zombie = unsafe { pvzp_rs::Borrowed::from_non_null(zombie.as_non_null()) };
        // SAFETY: both handles are bound to this current backend/world borrow.
        let plant = unsafe { pvzp_rs::Borrowed::from_non_null(plant.as_non_null()) };
        world
            .zombie_can_target_plant(zombie, plant, attack_type)
            .map_err(Into::into)
    }

    fn zombie_is_dead_or_dying<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        // SAFETY: the handle is bound to this current backend/world borrow.
        let zombie = unsafe { pvzp_rs::Borrowed::from_non_null(zombie.as_non_null()) };
        zombie.is_dead_or_dying()
    }
}

impl ZombieStateBackend for PortableBackend {
    fn zombie_action<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        read_raw!(zombie, mZombieHeight)
    }

    fn zombie_collision<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<ContactRect> {
        let world = self.world()?;
        // SAFETY: the handle is bound to this current backend/world borrow.
        let zombie = unsafe { pvzp_rs::Borrowed::from_non_null(zombie.as_non_null()) };
        let raw = world.zombie_hit_box(zombie)?;
        // Native clipping can temporarily invert a Digger's rectangle while it rises.
        ContactRect::from_pos_size(raw.x, raw.y, raw.width, raw.height).ok_or(PortableBackendError::OperationRejected(
            "native zombie rectangle overflow",
        ))
    }

    fn zombie_max_hp<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        read_raw!(zombie, mBodyMaxHealth)
    }

    fn zombie_accessory_1_hp<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        read_raw!(zombie, mHelmHealth)
    }
    fn zombie_accessory_1_max_hp<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        read_raw!(zombie, mHelmMaxHealth)
    }
    fn zombie_accessory_2_hp<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        read_raw!(zombie, mShieldHealth)
    }
    fn zombie_accessory_2_max_hp<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        read_raw!(zombie, mShieldMaxHealth)
    }

    fn zombie_related_id<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> u32 {
        // The handle remains valid for this read.
        read_raw!(zombie, mRelatedZombieID)
    }
    fn zombie_follower_id<'a>(&'a self, zombie: Self::ZombieHandle<'a>, index: usize) -> Result<u32> {
        if index >= 4 {
            return Err(PortableBackendError::NumericOutOfRange("zombie follower index"));
        }
        // The handle and bounded follower index identify one native field.
        Ok(read_raw!(zombie, mFollowerZombieID[index]))
    }

    fn zombie_target_plant<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Option<PlantId> {
        let id = read_raw!(zombie, mTargetPlantID);
        (id != 0).then(|| PlantId::from_raw(id))
    }
}
