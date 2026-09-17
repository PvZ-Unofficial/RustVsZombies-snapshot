use super::*;

impl ZombieReadBackend for PeBackend {
    type ZombieHandle<'a> = crate::handles::PeZombieHandle<'a>;
    type ZombieIter<'a> = crate::handles::PeZombieIter<'a>;

    fn zombies(&self) -> Result<Self::ZombieIter<'_>> {
        self.with_current_world(|world| {
            let pool = crate::access::zombie_pool(world);
            let limit = pool.max_used_count();
            crate::handles::PeZombieIter::new(self, limit)
        })
    }

    fn zombie(&self, id: ZombieId) -> Result<Option<Self::ZombieHandle<'_>>> {
        self.with_current_world(|world| {
            self.zombie_handle_from_pool(crate::access::zombie_pool(world), id)
                .filter(|handle| self.pe_zombie_is_live(*handle))
        })
    }

    fn zombie_id<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> ZombieId {
        self.zombie_id_from_handle(handle)
    }

    fn zombie_kind<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> Result<ZombieKind> {
        pe_zombie_kind_from_handle(handle)
    }

    fn zombie_hp<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> i32 {
        read_pe_raw_field!(handle.as_ptr(), hp)
    }

    fn zombie_row<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> i32 {
        read_pe_raw_field!(handle.as_ptr(), row) as i32
    }

    fn zombie_age<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> i32 {
        i32::try_from(read_pe_raw_field!(handle.as_ptr(), time_since_spawn)).unwrap_or(i32::MAX)
    }

    fn zombie_is_alive<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> bool {
        self.pe_zombie_is_live(handle)
    }
}

impl ZombieVerticalPositionBackend for PeBackend {
    fn zombie_pos_y<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> f32 {
        read_pe_raw_field!(zombie.as_ptr(), y)
    }

    fn zombie_pos_y_based_on_row<'a>(&'a self, zombie: Self::ZombieHandle<'a>, row: i32) -> Result<f32> {
        let x = read_pe_raw_field!(zombie.as_ptr(), x);
        let kind = pe_zombie_kind_from_handle(zombie)?;
        let mut y = self.with_current_world(|world| world.scene().pos_y_based_on_row(x + 40.0, row))?? - 30.0;
        if kind == ZombieKind::Balloon {
            y -= 30.0;
        } else if kind == ZombieKind::Pogo {
            y -= 16.0;
        }
        Ok(y)
    }
}

impl ZombieRawFactsBackend for PeBackend {
    fn zombie_phase<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<ZombiePhase> {
        pe_zombie_phase_from_raw(read_pe_raw_field!(zombie.as_ptr(), status))
    }
    fn zombie_int_x<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        read_pe_raw_field!(zombie.as_ptr(), int_x)
    }
    fn zombie_int_y<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        read_pe_raw_field!(zombie.as_ptr(), int_y)
    }
    fn zombie_pos_x<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> f32 {
        read_pe_raw_field!(zombie.as_ptr(), x)
    }
    fn zombie_width<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        read_pe_raw_field!(zombie.as_ptr(), hit_box.offset_x)
    }
    fn zombie_height<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        read_pe_raw_field!(zombie.as_ptr(), hit_box.offset_y)
    }
    fn zombie_from_wave<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        pe_u32_i32(read_pe_raw_field!(zombie.as_ptr(), spawn_wave))
    }
    fn zombie_height_state<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        match read_pe_raw_field!(zombie.as_ptr(), action) {
            1 => 1,
            2 => 2,
            3 => 3,
            6 => 6,
            7 => 7,
            9 => 9,
            _ => 0,
        }
    }
    fn zombie_phase_counter<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        read_pe_raw_field!(zombie.as_ptr(), countdown.action)
    }
    fn zombie_altitude<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> f32 {
        read_pe_raw_field!(zombie.as_ptr(), dy)
    }
    fn zombie_speed_x<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> f32 {
        read_pe_raw_field!(zombie.as_ptr(), dx)
    }
    fn zombie_scale<'a>(&'a self, _zombie: Self::ZombieHandle<'a>) -> f32 {
        // PE has no per-zombie scale state; its geometry is always represented at native scale.
        1.0
    }
    fn zombie_speed_z<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> f32 {
        read_pe_raw_field!(zombie.as_ptr(), d2y)
    }
    fn zombie_is_eating<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        read_pe_raw_field!(zombie.as_ptr(), is_eating)
    }
    fn zombie_is_disappeared<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        read_pe_raw_field!(zombie.as_ptr(), is_dead)
    }
    fn zombie_is_mind_controlled<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        read_pe_raw_field!(zombie.as_ptr(), is_hypno)
    }
    fn zombie_is_blown_away<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        read_pe_raw_field!(zombie.as_ptr(), is_blown)
    }
    fn zombie_has_flat_tires<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        read_pe_raw_field!(zombie.as_ptr(), type_) == pe_rs::ZombieType::Zomboni as i32
            && read_pe_raw_field!(zombie.as_ptr(), status) == 1
    }
    fn zombie_has_head<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        self.pe_zombie_is_live(zombie) && read_pe_raw_field!(zombie.as_ptr(), is_not_dying)
    }
    fn zombie_has_object<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        read_pe_raw_field!(zombie.as_ptr(), has_item_or_walk_left)
    }
    fn zombie_is_in_pool<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        read_pe_raw_field!(zombie.as_ptr(), is_in_water)
    }
    fn zombie_is_on_high_ground<'a>(&'a self, _zombie: Self::ZombieHandle<'a>) -> bool {
        // PE folds roof terrain into row y; it has no separate high-ground zombie flag.
        false
    }
    fn zombie_has_yucky_face<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        read_pe_raw_field!(zombie.as_ptr(), has_eaten_garlic)
    }
    fn zombie_yucky_face_counter<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        pe_u32_i32(read_pe_raw_field!(zombie.as_ptr(), time_since_ate_garlic))
    }
    fn zombie_frozen_countdown<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        pe_u32_i32(read_pe_raw_field!(zombie.as_ptr(), countdown.freeze))
    }
    fn zombie_chilled_countdown<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        pe_u32_i32(read_pe_raw_field!(zombie.as_ptr(), countdown.slow))
    }
    fn zombie_buttered_countdown<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        pe_u32_i32(read_pe_raw_field!(zombie.as_ptr(), countdown.butter))
    }
    fn zombie_reanim_anim_time<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<Option<f32>> {
        if !self.pe_zombie_is_live(zombie) {
            return Ok(None);
        }
        Ok(Some(read_pe_raw_field!(zombie.as_ptr(), reanim.progress)))
    }
    fn zombie_reanim_last_time<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<Option<f32>> {
        if !self.pe_zombie_is_live(zombie) {
            return Ok(None);
        }
        Ok(Some(read_pe_raw_field!(zombie.as_ptr(), reanim.prev_progress)))
    }
    fn zombie_reanim_rate<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<Option<f32>> {
        if !self.pe_zombie_is_live(zombie) {
            return Ok(None);
        }
        Ok(Some(read_pe_raw_field!(zombie.as_ptr(), reanim.fps)))
    }
    fn zombie_reanim_frame_start<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<Option<i32>> {
        if !self.pe_zombie_is_live(zombie) {
            return Ok(None);
        }
        Ok(Some(pe_u32_i32(read_pe_raw_field!(
            zombie.as_ptr(),
            reanim.begin_frame
        ))))
    }
    fn zombie_reanim_frame_count<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<Option<i32>> {
        if !self.pe_zombie_is_live(zombie) {
            return Ok(None);
        }
        Ok(Some(pe_u32_i32(read_pe_raw_field!(zombie.as_ptr(), reanim.n_frames))))
    }
    fn zombie_reanim_loop_type<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<Option<i32>> {
        if !self.pe_zombie_is_live(zombie) {
            return Ok(None);
        }
        Ok(Some(read_pe_raw_field!(zombie.as_ptr(), reanim.type_)))
    }
}

impl ZombieContactBackend for PeBackend {
    fn zombie_effected_by_damage<'a>(
        &'a self, zombie: Self::ZombieHandle<'a>, flags: DamageRangeFlags,
    ) -> Result<bool> {
        let flags = u8::try_from(flags.bits()).map_err(|_| PeBackendError::Unsupported("PE damage flags above u8"))?;
        self.with_current_world(|world| {
            // SAFETY: the raw pointer is bound to this current backend borrow.
            world.zombie_can_be_attacked(unsafe { pe_rs::ZombieRef::from_non_null(zombie.ptr) }, flags)
        })?
        .map_err(Into::into)
    }

    fn zombie_can_attack_plant<'a>(
        &'a self, zombie: <Self as ZombieReadBackend>::ZombieHandle<'a>,
        plant: <Self as PlantReadBackend>::PlantHandle<'a>, attack_type: i32,
    ) -> Result<bool> {
        self.with_current_world(|world| {
            // SAFETY: both raw pointers are bound to this current backend borrow.
            world.zombie_can_attack_plant(
                unsafe { pe_rs::ZombieRef::from_non_null(zombie.ptr) },
                unsafe { pe_rs::PlantRef::from_non_null(plant.ptr) },
                attack_type,
            )
        })?
        .map_err(Into::into)
    }

    fn zombie_is_dead_or_dying<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> bool {
        let phase = read_pe_raw_field!(zombie.as_ptr(), status);
        read_pe_raw_field!(zombie.as_ptr(), is_dead) || matches!(phase, 1..=3)
    }
}
