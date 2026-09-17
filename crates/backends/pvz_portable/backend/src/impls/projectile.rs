use super::*;

impl ProjectileRuleEditBackend for PortableBackend {
    fn kernel_pult_projectile_rule(&self) -> Result<KernelPultProjectileRule> {
        self.world()?;
        match pvzp_rs::modifier::kernel_pult_projectile_rule()? {
            0 => Ok(KernelPultProjectileRule::Normal),
            1 => Ok(KernelPultProjectileRule::AlwaysButter),
            2 => Ok(KernelPultProjectileRule::AlwaysKernel),
            value => Err(invalid_kind("kernel-pult projectile rule", value)),
        }
    }

    fn set_kernel_pult_projectile_rule(&self, rule: KernelPultProjectileRule) -> Result<()> {
        self.world()?;
        let raw = match rule {
            KernelPultProjectileRule::Normal => 0,
            KernelPultProjectileRule::AlwaysButter => 1,
            KernelPultProjectileRule::AlwaysKernel => 2,
        };
        pvzp_rs::modifier::set_kernel_pult_projectile_rule(raw).map_err(Into::into)
    }
}

impl ProjectileReadBackend for PortableBackend {
    type ProjectileHandle<'a> = PortableProjectileHandle<'a>;
    type ProjectileIter<'a> = PortableProjectileIter<'a>;

    fn projectiles(&self) -> Result<Self::ProjectileIter<'_>> {
        Ok({
            let pool = crate::access::projectile_pool(&self.world()?);
            let limit = pool.max_used_count();
            crate::handles::PortableProjectileIter::new(self, limit)
        })
    }

    fn projectile_id<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> ProjectileId {
        self.projectile_id_from_handle(projectile)
    }

    fn projectile_kind<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> i32 {
        read_raw!(projectile, mProjectileType)
    }

    fn projectile_motion<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> i32 {
        read_raw!(projectile, mMotionType)
    }

    fn projectile_row<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> i32 {
        read_raw!(projectile, _base.mRow)
    }

    fn projectile_pos_x<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32 {
        read_raw!(projectile, mPosX)
    }
    fn projectile_pos_y<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32 {
        read_raw!(projectile, mPosY)
    }
    fn projectile_pos_z<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32 {
        read_raw!(projectile, mPosZ)
    }

    fn projectile_vel_x<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32 {
        read_raw!(projectile, mVelX)
    }
    fn projectile_vel_y<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32 {
        read_raw!(projectile, mVelY)
    }
    fn projectile_vel_z<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32 {
        read_raw!(projectile, mVelZ)
    }

    fn projectile_acceleration<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32 {
        read_raw!(projectile, mAccZ)
    }

    fn projectile_collision<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> Result<ContactRect> {
        let world = self.world()?;
        // SAFETY: the handle is bound to this current backend/world borrow.
        let projectile = unsafe { pvzp_rs::Borrowed::from_non_null(projectile.as_non_null()) };
        contact_rect(world.projectile_hit_box(projectile))
    }

    fn projectile_damage_flags<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> u32 {
        read_raw!(projectile, mDamageRangeFlags) as u32
    }

    fn projectile_age<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> i32 {
        read_raw!(projectile, mProjectileAge)
    }
    fn projectile_torch_col<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> i32 {
        read_raw!(projectile, mHitTorchwoodGridX)
    }
    fn projectile_cob_target_row<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> i32 {
        read_raw!(projectile, mCobTargetRow)
    }

    fn projectile_target_zombie_id<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> u32 {
        read_raw!(projectile, mTargetZombieID)
    }
    fn projectile_cob_target_x_bits<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> u32 {
        read_raw!(projectile, mCobTargetX).to_bits()
    }
}
