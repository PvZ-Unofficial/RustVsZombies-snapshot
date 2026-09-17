use super::*;

impl BackendIdentityBackend for PeBackend {
    fn backend_name(&self) -> &'static str {
        "pvz-emulator"
    }

    fn backend_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}

impl BoardStateBackend for PeBackend {
    fn natural_sun_generated(&self) -> Result<i32> {
        self.with_current_world(|world| {
            let sun = world.scene().sun_data().as_ptr();
            // SAFETY: sun is embedded in the borrowed world.
            pe_u32_i32(read_pe_raw_field!(sun, natural_sun_generated))
        })
    }

    fn natural_sun_countdown(&self) -> Result<i32> {
        self.with_current_world(|world| {
            let sun = world.scene().sun_data().as_ptr();
            // SAFETY: sun is embedded in the borrowed world.
            pe_u32_i32(read_pe_raw_field!(sun, natural_sun_countdown))
        })
    }

    fn dancer_clock(&self) -> Result<u32> {
        Ok(self.with_current_world(|world| world.scene().dancer_clock())?)
    }

    fn ice_path_x(&self, row: u32) -> Result<i32> {
        if row >= 6 {
            return Err(PeBackendError::InvalidGrid);
        }
        self.with_current_world(|world| {
            let ice = world.scene().ice_path_data().as_ptr();
            // SAFETY: row was bounded against PE's fixed six-row array.
            Ok::<_, pe_rs::Error>(unsafe { std::ptr::addr_of!((*ice).x.0[row as usize]).read() as i32 })
        })?
        .map_err(Into::into)
    }

    fn ice_path_countdown(&self, row: u32) -> Result<u32> {
        if row >= 6 {
            return Err(PeBackendError::InvalidGrid);
        }
        self.with_current_world(|world| {
            let ice = world.scene().ice_path_data().as_ptr();
            // SAFETY: row was bounded against PE's fixed six-row array.
            Ok::<_, pe_rs::Error>(unsafe { std::ptr::addr_of!((*ice).countdown.0[row as usize]).read() })
        })?
        .map_err(Into::into)
    }

    fn spawn_allowed(&self, raw_zombie_kind: u32) -> Result<bool> {
        if raw_zombie_kind >= 33 {
            return Err(PeBackendError::NumericOutOfRange("spawn flag index"));
        }
        self.with_current_world(|world| {
            let spawn = crate::access::spawn_data(world).as_ptr();
            // SAFETY: the index was bounded against the fixed flag array.
            Ok::<_, pe_rs::Error>(unsafe {
                std::ptr::addr_of!((*spawn).spawn_flags.0[raw_zombie_kind as usize]).read() != 0
            })
        })?
        .map_err(Into::into)
    }

    fn spawn_entry(&self, wave: u32, slot: u32) -> Result<i32> {
        if wave >= 20 || slot >= 50 {
            return Err(PeBackendError::NumericOutOfRange("spawn-list index"));
        }
        self.with_current_world(|world| world.scene().spawn_entry(wave, slot))?
            .map(pe_rs::ZombieType::to_raw)
            .map_err(Into::into)
    }
}

impl RandomControlBackend for PeBackend {
    fn set_random_mode(&mut self, mode: RandomMode) -> Result<()> {
        self.with_current_world(|world| match mode {
            RandomMode::Seeded(seed) => world.scene().seed_rngs(seed, seed),
            RandomMode::Locked(value) => world.scene().lock_rngs(value),
        })??;
        Ok(())
    }

    fn set_wave_spawn_random_seed(&mut self, base_seed: Option<u32>) -> Result<()> {
        self.with_current_world(|world| world.scene().set_wave_spawn_random_seed(base_seed))??;
        Ok(())
    }

    fn random_seed(&self, stream: RandomStreamKind) -> Result<u32> {
        self.with_current_world(|world| world.scene().rng_seed(matches!(stream, RandomStreamKind::Level)))
    }
    fn random_locked(&self, stream: RandomStreamKind) -> bool {
        self.with_current_world(|world| world.scene().rng_locked(matches!(stream, RandomStreamKind::Level)))
            .expect("PE world borrow must remain available during a shared query")
    }
    fn random_fixed(&self, stream: RandomStreamKind) -> u32 {
        self.with_current_world(|world| world.scene().rng_fixed(matches!(stream, RandomStreamKind::Level)))
            .expect("PE world borrow must remain available during a shared query")
    }
}

impl SunProductionModeBackend for PeBackend {
    fn set_sun_production_mode(&mut self, mode: SunProductionMode) -> Result<()> {
        match mode {
            SunProductionMode::DirectCredit => Ok(()),
            SunProductionMode::NativePickup => Err(PeBackendError::Unsupported("native sun pickup")),
        }
    }
}

impl PlantStateBackend for PeBackend {
    fn plant_y<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        read_pe_raw_field!(plant.as_ptr(), y)
    }

    fn plant_width<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        read_pe_raw_field!(plant.as_ptr(), attack_box.width)
    }
    fn plant_height<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        read_pe_raw_field!(plant.as_ptr(), attack_box.height)
    }

    fn plant_max_hp<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        read_pe_raw_field!(plant.as_ptr(), max_hp)
    }

    fn plant_target_x<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        read_pe_raw_field!(plant.as_ptr(), cannon.x)
    }
    fn plant_target_y<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        read_pe_raw_field!(plant.as_ptr(), cannon.y)
    }

    fn plant_target_object<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        let target = read_pe_raw_field!(plant.as_ptr(), target);
        if target == -1 { 0 } else { target }
    }

    fn plant_direction<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        let kind = read_pe_raw_field!(plant.as_ptr(), type_);
        let state = read_pe_raw_field!(plant.as_ptr(), status);
        if kind == 17
            && (3..=7).contains(&state)
            && read_pe_raw_field!(plant.as_ptr(), cannon.x) < read_pe_raw_field!(plant.as_ptr(), x)
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
        read_pe_raw_field!(plant.as_ptr(), imitater_target)
    }

    fn plant_recently_eaten_countdown<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        read_pe_raw_field!(plant.as_ptr(), countdown.eaten)
    }
    fn plant_launch_counter<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        read_pe_raw_field!(plant.as_ptr(), countdown.generate)
    }
    fn plant_shooting_counter<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        read_pe_raw_field!(plant.as_ptr(), countdown.launch)
    }
}

impl ZombieStateBackend for PeBackend {
    fn zombie_action<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        read_pe_raw_field!(zombie.as_ptr(), action)
    }

    fn zombie_collision<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<ContactRect> {
        let raw = self.with_current_world(|world| {
            // SAFETY: the raw pointer is bound to this current backend borrow.
            world.zombie_hit_box(unsafe { pe_rs::ZombieRef::from_non_null(zombie.ptr) })
        })?;
        // Native clipping can temporarily invert a Digger's rectangle while it rises.
        ContactRect::from_pos_size(raw.x, raw.y, raw.width, raw.height).ok_or(PeBackendError::OperationRejected(
            "PE zombie contact rectangle endpoint overflow",
        ))
    }

    fn zombie_max_hp<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        pe_u32_i32(read_pe_raw_field!(zombie.as_ptr(), max_hp))
    }

    fn zombie_accessory_1_hp<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        pe_u32_i32(read_pe_raw_field!(zombie.as_ptr(), accessory_1.hp))
    }
    fn zombie_accessory_1_max_hp<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        pe_u32_i32(read_pe_raw_field!(zombie.as_ptr(), accessory_1.max_hp))
    }
    fn zombie_accessory_2_hp<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        pe_u32_i32(read_pe_raw_field!(zombie.as_ptr(), accessory_2.hp))
    }
    fn zombie_accessory_2_max_hp<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        pe_u32_i32(read_pe_raw_field!(zombie.as_ptr(), accessory_2.max_hp))
    }

    fn zombie_related_id<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> u32 {
        // The handle remains valid for this read.
        canonical_relation_id(read_pe_raw_field!(zombie.as_ptr(), master_id))
    }
    fn zombie_follower_id<'a>(&'a self, zombie: Self::ZombieHandle<'a>, index: usize) -> Result<u32> {
        if index >= 4 {
            return Err(PeBackendError::NumericOutOfRange("zombie follower index"));
        }
        // The handle and bounded follower index identify one native field.
        Ok(canonical_relation_id(
            read_pe_raw_field!(zombie.as_ptr(), partners.0[index]) as i32,
        ))
    }

    fn zombie_target_plant<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Option<PlantId> {
        let raw = read_pe_raw_field!(zombie.as_ptr(), bungee_target);
        (raw != -1).then(|| PlantId::from_raw(raw as u32))
    }
}

impl ProjectileReadBackend for PeBackend {
    type ProjectileHandle<'a> = crate::handles::PeProjectileHandle<'a>;
    type ProjectileIter<'a> = crate::handles::PeProjectileIter<'a>;

    fn projectiles(&self) -> Result<Self::ProjectileIter<'_>> {
        self.with_current_world(|world| {
            let pool = crate::access::projectile_pool(world);
            let limit = pool.max_used_count();
            crate::handles::PeProjectileIter::new(self, limit)
        })
    }

    fn projectile_id<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> ProjectileId {
        // SAFETY: the backend handle is tied to this current backend borrow.
        let projectile_ref = unsafe { pe_rs::ProjectileRef::from_non_null(projectile.ptr) };
        let id = self
            .with_current_world(|world| crate::access::projectile_pool(world).get_id(projectile_ref))
            .expect("PE world must remain installed while a projectile handle is live")
            .expect("PE projectile handle must belong to its current pool");
        ProjectileId::from_raw(id)
    }

    fn projectile_kind<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> i32 {
        read_pe_raw_field!(projectile.as_ptr(), type_)
    }

    fn projectile_motion<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> i32 {
        read_pe_raw_field!(projectile.as_ptr(), motion_type)
    }

    fn projectile_row<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> i32 {
        read_pe_raw_field!(projectile.as_ptr(), row)
    }

    fn projectile_pos_x<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32 {
        read_pe_raw_field!(projectile.as_ptr(), x)
    }
    fn projectile_pos_y<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32 {
        read_pe_raw_field!(projectile.as_ptr(), y)
    }
    fn projectile_pos_z<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32 {
        read_pe_raw_field!(projectile.as_ptr(), dy1)
    }

    fn projectile_vel_x<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32 {
        read_pe_raw_field!(projectile.as_ptr(), dx)
    }
    fn projectile_vel_y<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32 {
        read_pe_raw_field!(projectile.as_ptr(), dy2)
    }
    fn projectile_vel_z<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32 {
        read_pe_raw_field!(projectile.as_ptr(), ddy)
    }

    fn projectile_acceleration<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32 {
        read_pe_raw_field!(projectile.as_ptr(), dddy)
    }

    fn projectile_collision<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> Result<ContactRect> {
        let raw = self.with_current_world(|world| {
            // SAFETY: the raw pointer is bound to this current backend borrow.
            world.projectile_attack_box(unsafe { pe_rs::ProjectileRef::from_non_null(projectile.ptr) })
        })?;
        pe_contact_rect(raw)
    }

    fn projectile_damage_flags<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> u32 {
        read_pe_raw_field!(projectile.as_ptr(), flags)
    }

    fn projectile_age<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> i32 {
        pe_u32_i32(read_pe_raw_field!(projectile.as_ptr(), time_since_created))
    }
    fn projectile_torch_col<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> i32 {
        read_pe_raw_field!(projectile.as_ptr(), last_torchwood_col)
    }
    fn projectile_cob_target_row<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> i32 {
        read_pe_raw_field!(projectile.as_ptr(), cannon_row)
    }

    fn projectile_target_zombie_id<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> u32 {
        canonical_relation_id(read_pe_raw_field!(projectile.as_ptr(), target))
    }
    fn projectile_cob_target_x_bits<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> u32 {
        read_pe_raw_field!(projectile.as_ptr(), cannon_x).to_bits()
    }
}

impl GridItemStateBackend for PeBackend {
    fn grid_item_countdown<'a>(&'a self, item: Self::GridItemHandle<'a>) -> i32 {
        read_pe_raw_field!(item.as_ptr(), countdown)
    }
}

#[cfg(test)]
mod witness_mapping_tests {
    use std::ptr::NonNull;

    use rsvz_backend_api::backend::{
        GridItemStateBackend, PlantCreateBackend, PlantReadBackend, PlantStateBackend, ProjectileReadBackend,
        ZombieRawFactsBackend, ZombieStateBackend,
    };
    use rsvz_model::model::{CardSelection, Grid, PlantId, PlantKind};

    #[test]
    fn witness_mappings_use_native_semantics_instead_of_adjacent_pe_fields() {
        let mut owner = crate::runtime::PeWorldOwner::new_reset(crate::PeWorldConfig::default()).expect("PE world");
        // SAFETY: all handles and raw fixtures remain inside the installed-world closure.
        unsafe {
            owner
                .with_current(|backend| {
                    let mut projectile: pe_rs::raw::pe_rs_projectile = std::mem::zeroed();
                    projectile.x = 1.0;
                    projectile.y = 2.0;
                    projectile.shadow_y = 99.0;
                    projectile.dy1 = 3.0;
                    projectile.dx = 4.0;
                    projectile.dy2 = 5.0;
                    projectile.ddy = 6.0;
                    projectile.dddy = 7.0;
                    projectile.flags = 9;
                    projectile.is_visible = true;
                    projectile.target = -1;
                    projectile.cannon_x = -0.0;
                    let handle = crate::handles::PeProjectileHandle::new(NonNull::from(&mut projectile));
                    assert_eq!(
                        [
                            backend.projectile_pos_x(handle),
                            backend.projectile_pos_y(handle),
                            backend.projectile_pos_z(handle)
                        ],
                        [1.0, 2.0, 3.0]
                    );
                    assert_eq!(
                        [
                            backend.projectile_vel_x(handle),
                            backend.projectile_vel_y(handle),
                            backend.projectile_vel_z(handle)
                        ],
                        [4.0, 5.0, 6.0]
                    );
                    assert_eq!(backend.projectile_acceleration(handle), 7.0);
                    assert_eq!(backend.projectile_cob_target_x_bits(handle), (-0.0_f32).to_bits());
                    assert_eq!(backend.projectile_damage_flags(handle), 9);
                    std::ptr::addr_of_mut!((*handle.as_ptr().cast_mut()).is_disappeared).write(true);
                    assert_eq!(backend.projectile_damage_flags(handle), 9);
                    assert_eq!(backend.projectile_target_zombie_id(handle), 0);
                    let mut high_projectile: pe_rs::raw::pe_rs_projectile = std::mem::zeroed();
                    high_projectile.target = i32::MIN;
                    let high_handle = crate::handles::PeProjectileHandle::new(NonNull::from(&mut high_projectile));
                    assert_eq!(backend.projectile_target_zombie_id(high_handle), 0x8000_0000);

                    let direction = |kind, state, target_x| {
                        let mut plant: pe_rs::raw::pe_rs_plant = std::mem::zeroed();
                        plant.type_ = kind;
                        plant.status = state;
                        plant.x = 100;
                        plant.cannon.x = target_x;
                        plant.direction = 1;
                        backend.plant_direction(crate::handles::PePlantHandle::new(NonNull::from(&mut plant)))
                    };
                    assert_eq!(direction(0, 0, -1), -1);
                    assert_eq!(direction(17, 3, 50), 1);
                    assert_eq!(direction(17, 3, 150), -1);

                    let mut plant: pe_rs::raw::pe_rs_plant = std::mem::zeroed();
                    plant.target = -1;
                    let handle = crate::handles::PePlantHandle::new(NonNull::from(&mut plant));
                    assert_eq!(backend.plant_target_object(handle), 0);
                    std::ptr::addr_of_mut!((*handle.as_mut_ptr()).countdown.eaten).write(17);
                    std::ptr::addr_of_mut!((*handle.as_mut_ptr()).countdown.generate).write(18);
                    std::ptr::addr_of_mut!((*handle.as_mut_ptr()).countdown.launch).write(19);
                    assert_eq!(backend.plant_recently_eaten_countdown(handle), 17);
                    assert_eq!(backend.plant_eating_flash_counter(handle), 0);
                    assert_eq!(backend.plant_launch_counter(handle), 18);
                    assert_eq!(backend.plant_shooting_counter(handle), 19);

                    let mut zombie: pe_rs::raw::pe_rs_zombie = std::mem::zeroed();
                    zombie.hit_box.offset_x = 120;
                    zombie.hit_box.offset_y = 120;
                    zombie.hit_box.width = 42;
                    zombie.hit_box.height = 115;
                    zombie.type_ = pe_rs::ZombieType::Zomboni as i32;
                    zombie.status = 1;
                    zombie.master_id = i32::MIN;
                    zombie.partners.0 = [0xffff_fffe, 12, 13, 14];
                    zombie.accessory_1.hp = 101;
                    zombie.accessory_1.max_hp = 102;
                    zombie.accessory_2.hp = 103;
                    zombie.accessory_2.max_hp = 104;
                    zombie.bungee_target = i32::MIN;
                    zombie.action = 9;
                    zombie.dx = -0.5;
                    zombie.reanim.progress = 0.25;
                    zombie.reanim.prev_progress = 0.5;
                    zombie.reanim.begin_frame = 17;
                    zombie.reanim.n_frames = 19;
                    zombie.reanim.fps = -24.0;
                    zombie.reanim.type_ = 1;
                    let handle = crate::handles::PeZombieHandle::new(NonNull::from(&mut zombie));
                    assert_eq!(
                        (backend.zombie_width(handle), backend.zombie_height(handle)),
                        (120, 120)
                    );
                    assert_eq!(
                        [
                            backend.zombie_related_id(handle),
                            backend.zombie_follower_id(handle, 0).unwrap()
                        ],
                        [0x8000_0000, 0xffff_fffe]
                    );
                    assert_eq!(
                        [
                            backend.zombie_accessory_1_hp(handle),
                            backend.zombie_accessory_1_max_hp(handle),
                            backend.zombie_accessory_2_hp(handle),
                            backend.zombie_accessory_2_max_hp(handle),
                        ],
                        [101, 102, 103, 104]
                    );
                    for (index, expected) in [0xffff_fffe, 12, 13, 14].into_iter().enumerate() {
                        assert_eq!(backend.zombie_follower_id(handle, index).unwrap(), expected);
                    }
                    assert!(backend.zombie_follower_id(handle, 4).is_err());
                    assert_eq!(backend.zombie_target_plant(handle).unwrap().raw(), 0x8000_0000);
                    assert_eq!(backend.zombie_height_state(handle), 9);
                    assert_eq!(backend.zombie_speed_x(handle), -0.5);
                    assert!(backend.zombie_has_flat_tires(handle));
                    std::ptr::addr_of_mut!((*handle.as_mut_ptr()).is_not_dying).write(true);
                    std::ptr::addr_of_mut!((*handle.as_mut_ptr()).status).write(0);
                    assert!(backend.zombie_has_head(handle));
                    assert_eq!(backend.zombie_reanim_anim_time(handle).unwrap(), Some(0.25));
                    assert_eq!(backend.zombie_reanim_last_time(handle).unwrap(), Some(0.5));
                    assert_eq!(backend.zombie_reanim_frame_start(handle).unwrap(), Some(17));
                    assert_eq!(backend.zombie_reanim_frame_count(handle).unwrap(), Some(19));
                    assert_eq!(backend.zombie_reanim_rate(handle).unwrap(), Some(-24.0));
                    assert_eq!(backend.zombie_reanim_loop_type(handle).unwrap(), Some(1));
                    std::ptr::addr_of_mut!((*handle.as_mut_ptr()).is_dead).write(true);
                    assert_eq!(backend.zombie_reanim_anim_time(handle).unwrap(), None);
                    assert_eq!(backend.zombie_reanim_last_time(handle).unwrap(), None);
                    assert_eq!(backend.zombie_reanim_rate(handle).unwrap(), None);
                    assert_eq!(backend.zombie_reanim_frame_start(handle).unwrap(), None);
                    assert_eq!(backend.zombie_reanim_frame_count(handle).unwrap(), None);
                    assert_eq!(backend.zombie_reanim_loop_type(handle).unwrap(), None);
                    assert_eq!(backend.zombie_target_plant(handle).map(PlantId::raw), Some(0x8000_0000));
                    let mut no_target: pe_rs::raw::pe_rs_zombie = std::mem::zeroed();
                    no_target.bungee_target = -1;
                    let no_target = crate::handles::PeZombieHandle::new(NonNull::from(&mut no_target));
                    assert_eq!(backend.zombie_target_plant(no_target), None);

                    let mut gargantuar: pe_rs::raw::pe_rs_zombie = std::mem::zeroed();
                    gargantuar.hit_box.offset_x = 180;
                    gargantuar.hit_box.offset_y = 180;
                    gargantuar.hit_box.width = 125;
                    gargantuar.hit_box.height = 154;
                    let gargantuar = crate::handles::PeZombieHandle::new(NonNull::from(&mut gargantuar));
                    assert_eq!(
                        (backend.zombie_width(gargantuar), backend.zombie_height(gargantuar)),
                        (180, 180)
                    );
                })
                .expect("install fixture");
        }
    }

    #[test]
    fn live_state_view_excludes_objects_awaiting_reclamation() {
        let mut owner = crate::runtime::PeWorldOwner::new_reset(crate::PeWorldConfig::default()).expect("PE world");
        // SAFETY: the handle and its raw mutation stay inside the installed-world closure.
        unsafe {
            owner
                .with_current(|backend| {
                    crate::scope_backend(backend, || {
                        crate::with_backend_shared(|backend| {
                            let selection = CardSelection::Plant(PlantKind::Peashooter)
                                .checked()
                                .expect("valid selection");
                            let plant = backend.new_plant(selection, Grid { row: 0, col: 0 }).expect("plant");
                            let id = backend.plant_id(plant);
                            std::ptr::addr_of_mut!((*plant.as_mut_ptr()).is_dead).write(true);

                            assert!(!backend.plants().unwrap().any(|plant| backend.plant_id(plant) == id));
                        })
                        .expect("Board scope")
                    });
                })
                .expect("install fixture");
        }
    }
}
