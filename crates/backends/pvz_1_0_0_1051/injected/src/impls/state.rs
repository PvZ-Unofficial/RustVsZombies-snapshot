use std::ptr::{self, NonNull};

use rsvz_backend_api::backend::{
    BackendIdentityBackend, BoardStateBackend, GridItemStateBackend, PlantContactBackend, PlantStateBackend,
    ProjectileReadBackend, RandomControlBackend, SunProductionModeBackend, ZombieStateBackend,
};
use rsvz_model::{ContactRect, PlantId, ProjectileId, RandomMode, RandomStreamKind, SunProductionMode};

use super::handles::{PvzProjectileHandle, PvzProjectileIter};
use crate::error::{Pvz1051Error, Result};
use crate::raw::layout as ptrs;
use crate::raw::types::RawRect;
use crate::runtime::Pvz1051Backend;

use super::contact::native_rect;

impl BackendIdentityBackend for Pvz1051Backend {
    fn backend_name(&self) -> &'static str {
        "pvz-1.0.0.1051"
    }

    fn backend_version(&self) -> &'static str {
        "1.0.0.1051"
    }
}

impl BoardStateBackend for Pvz1051Backend {
    fn natural_sun_generated(&self) -> Result<i32> {
        Ok({
            let board = self.board()?;
            // SAFETY: Board is held by the active shared scope.
            unsafe { ptrs::Board::num_suns_fallen(board.as_ptr()) }
        })
    }

    fn natural_sun_countdown(&self) -> Result<i32> {
        Ok({
            let board = self.board()?;
            // SAFETY: Board is held by the active shared scope.
            unsafe { ptrs::Board::sun_countdown(board.as_ptr()) }
        })
    }

    fn dancer_clock(&self) -> Result<u32> {
        let app = self.app();
        // Zombie::GetDancerFrame uses LawnApp::mAppCounter directly.
        Ok(unsafe { ptrs::LawnApp::app_counter(app.as_ptr()) } as u32)
    }

    fn ice_path_x(&self, row: u32) -> Result<i32> {
        if row >= 6 {
            return Err(Pvz1051Error::InvalidGrid);
        }
        let board = self.board()?;
        // SAFETY: row is bounded against Board::mIceMinX[6].
        Ok(unsafe { ptrs::Board::ice_min_x(board.as_ptr(), row as usize) })
    }

    fn ice_path_countdown(&self, row: u32) -> Result<u32> {
        if row >= 6 {
            return Err(Pvz1051Error::InvalidGrid);
        }
        let board = self.board()?;
        // SAFETY: row is bounded against Board::mIceTimer[6].
        Ok(unsafe { ptrs::Board::ice_timer(board.as_ptr(), row as usize) } as u32)
    }

    fn spawn_allowed(&self, raw_zombie_kind: u32) -> Result<bool> {
        if raw_zombie_kind >= 33 {
            return Err(Pvz1051Error::ScriptConfig(
                "spawn flag index exceeds the frozen schema".into(),
            ));
        }
        let board = self.board()?;
        // SAFETY: index is bounded against the compared 33 native zombie kinds.
        Ok(unsafe { ptrs::Board::zombie_allowed(board.as_ptr(), raw_zombie_kind as usize) })
    }

    fn spawn_entry(&self, wave: u32, slot: u32) -> Result<i32> {
        if wave >= 20 || slot >= 50 {
            return Err(Pvz1051Error::ScriptConfig(
                "spawn-list index exceeds native bounds".into(),
            ));
        }
        let board = self.board()?;
        // SAFETY: wave and slot are bounded against Board::mZombiesInWave[20][50].
        Ok(unsafe { ptrs::Board::zombie_in_wave(board.as_ptr(), wave as usize, slot as usize) })
    }
}

impl RandomControlBackend for Pvz1051Backend {
    fn set_random_mode(&mut self, mode: RandomMode) -> Result<()> {
        crate::patches::configure_random(mode)?;
        if let RandomMode::Seeded(seed) = mode {
            // SAFETY: Sexy::SRand owns the process-global Battle stream. A Seeded mode
            // establishes a new deterministic origin; resetting telemetry alone would
            // leave the native engine at its pre-Opening position.
            unsafe { crate::raw::abi::sexy_srand(crate::patches::effective_random_seed(seed)) };
        }
        Ok(())
    }

    fn set_wave_spawn_random_seed(&mut self, base_seed: Option<u32>) -> Result<()> {
        crate::patches::configure_wave_random_seed(base_seed)
    }

    fn random_seed(&self, stream: RandomStreamKind) -> Result<u32> {
        if stream == RandomStreamKind::Level {
            let app = self.app();
            let board = self.board()?;
            let player =
                NonNull::new(unsafe { ptrs::LawnApp::player_info(app.as_ptr()) }).ok_or(Pvz1051Error::NullUserData)?;
            let challenge =
                NonNull::new(unsafe { ptrs::Board::challenge(board.as_ptr()) }).ok_or(Pvz1051Error::NullChallenge)?;
            let raw_seed = crate::patches::level_random_seed(
                unsafe { ptrs::Board::board_rand_seed(board.as_ptr()) as u32 },
                unsafe { ptrs::PlayerInfo::id(player.as_ptr()) },
                unsafe { ptrs::Challenge::endless_rounds(challenge.as_ptr()) as u32 },
                unsafe { ptrs::LawnApp::game_mode(app.as_ptr()) as u32 },
            );
            return Ok(crate::patches::effective_random_seed(raw_seed));
        }
        Ok(crate::patches::random_seed(stream))
    }
    fn random_locked(&self, stream: RandomStreamKind) -> bool {
        crate::patches::random_locked(stream)
    }
    fn random_fixed(&self, stream: RandomStreamKind) -> u32 {
        crate::patches::random_fixed(stream)
    }
}

impl SunProductionModeBackend for Pvz1051Backend {
    fn set_sun_production_mode(&mut self, mode: SunProductionMode) -> Result<()> {
        crate::patches::set_direct_credit(matches!(mode, SunProductionMode::DirectCredit))
    }
}

impl PlantStateBackend for Pvz1051Backend {
    fn plant_y<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        // SAFETY: handle is generation-validated and borrow-bound.
        unsafe { ptrs::Plant::y(plant.ptr.as_ptr()) }
    }

    fn plant_width<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        // SAFETY: the borrow-bound handle remains valid; only this scalar is copied.
        unsafe { ptrs::Plant::width(plant.ptr.as_ptr()) }
    }
    fn plant_height<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        // SAFETY: the borrow-bound handle remains valid; only this scalar is copied.
        unsafe { ptrs::Plant::height(plant.ptr.as_ptr()) }
    }

    fn plant_max_hp<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        // SAFETY: handle is generation-validated and borrow-bound.
        unsafe { ptrs::Plant::max_health(plant.ptr.as_ptr()) }
    }

    fn plant_target_x<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        // SAFETY: the borrow-bound handle remains valid; only this scalar is copied.
        unsafe { ptrs::Plant::target_x(plant.ptr.as_ptr()) }
    }
    fn plant_target_y<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        // SAFETY: the borrow-bound handle remains valid; only this scalar is copied.
        unsafe { ptrs::Plant::target_y(plant.ptr.as_ptr()) }
    }

    fn plant_target_object<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        // SAFETY: handle is generation-validated and borrow-bound.
        unsafe { ptrs::Plant::target_zombie_id(plant.ptr.as_ptr()) }
    }

    fn plant_direction<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        // Only Squash states 3..=7 use target-x as a facing direction. Other plants initialize
        // target-x to -1, so treating its sign as a general direction would report a false left.
        unsafe {
            if ptrs::Plant::seed_type(plant.ptr.as_ptr()) == 17
                && (3..=7).contains(&ptrs::Plant::state(plant.ptr.as_ptr()))
                && ptrs::Plant::target_x(plant.ptr.as_ptr()) < ptrs::Plant::x(plant.ptr.as_ptr())
            {
                1
            } else {
                -1
            }
        }
    }

    fn plant_collision<'a>(&'a self, plant: Self::PlantHandle<'a>) -> Result<ContactRect> {
        self.plant_hit_box(plant)
    }

    fn plant_imitater_kind<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        // SAFETY: handle is generation-validated and borrow-bound.
        unsafe { ptrs::Plant::imitater_type(plant.ptr.as_ptr()) }
    }

    fn plant_recently_eaten_countdown<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        // SAFETY: the borrow-bound handle remains valid; only this scalar is copied.
        unsafe { ptrs::Plant::recently_eaten_countdown(plant.ptr.as_ptr()) }
    }
    fn plant_launch_counter<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        // SAFETY: the borrow-bound handle remains valid; only this scalar is copied.
        unsafe { ptrs::Plant::launch_counter(plant.ptr.as_ptr()) }
    }
    fn plant_shooting_counter<'a>(&'a self, plant: Self::PlantHandle<'a>) -> i32 {
        // SAFETY: the borrow-bound handle remains valid; only this scalar is copied.
        unsafe { ptrs::Plant::shooting_counter(plant.ptr.as_ptr()) }
    }
}

impl ZombieStateBackend for Pvz1051Backend {
    fn zombie_action<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        // PE's action field models the original ZombieHeight scalar.
        unsafe { ptrs::Zombie::zombie_height(zombie.ptr.as_ptr()) }
    }

    fn zombie_collision<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Result<ContactRect> {
        let mut raw = RawRect::new(0, 0, 0, 0);
        // SAFETY: objdump verifies EAX=out/ECX=Zombie for GetZombieRect.
        let returned = unsafe { crate::raw::abi::zombie_get_zombie_rect(zombie.ptr.as_ptr(), &raw mut raw) };
        if !ptr::eq(returned.cast_const(), &raw const raw) {
            return Err(Pvz1051Error::InvariantViolated("Zombie::GetZombieRect"));
        }
        // Native clipping can temporarily invert a Digger's rectangle while it rises.
        ContactRect::from_pos_size(raw.x, raw.y, raw.width, raw.height)
            .ok_or(Pvz1051Error::InvariantViolated("Zombie::GetZombieRect"))
    }

    fn zombie_max_hp<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        unsafe { ptrs::Zombie::body_max_health(zombie.ptr.as_ptr()) }
    }

    fn zombie_accessory_1_hp<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        // SAFETY: the borrow-bound handle remains valid; only this scalar is copied.
        unsafe { ptrs::Zombie::accessory_1_health(zombie.ptr.as_ptr()) }
    }
    fn zombie_accessory_1_max_hp<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        // SAFETY: the borrow-bound handle remains valid; only this scalar is copied.
        unsafe { ptrs::Zombie::accessory_1_max_health(zombie.ptr.as_ptr()) }
    }
    fn zombie_accessory_2_hp<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        // SAFETY: the borrow-bound handle remains valid; only this scalar is copied.
        unsafe { ptrs::Zombie::accessory_2_health(zombie.ptr.as_ptr()) }
    }
    fn zombie_accessory_2_max_hp<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> i32 {
        // SAFETY: the borrow-bound handle remains valid; only this scalar is copied.
        unsafe { ptrs::Zombie::accessory_2_max_health(zombie.ptr.as_ptr()) }
    }

    fn zombie_related_id<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> u32 {
        // SAFETY: the borrow-bound handle remains valid for this scalar read.
        unsafe { ptrs::Zombie::related_zombie_id(zombie.ptr.as_ptr()) }
    }
    fn zombie_follower_id<'a>(&'a self, zombie: Self::ZombieHandle<'a>, index: usize) -> Result<u32> {
        if index >= 4 {
            return Err(Pvz1051Error::AbiPreconditionFailed(
                "zombie follower index must be less than 4",
            ));
        }
        // SAFETY: the borrow-bound handle and bounded follower index identify one native field.
        Ok(unsafe { ptrs::Zombie::follower_zombie_id(zombie.ptr.as_ptr(), index) })
    }

    fn zombie_target_plant<'a>(&'a self, zombie: Self::ZombieHandle<'a>) -> Option<PlantId> {
        let id = unsafe { ptrs::Zombie::target_plant_id(zombie.ptr.as_ptr()) };
        (id != 0).then(|| PlantId::from_raw(id))
    }
}

impl ProjectileReadBackend for Pvz1051Backend {
    type ProjectileHandle<'a> = PvzProjectileHandle<'a>;
    type ProjectileIter<'a> = PvzProjectileIter<'a>;

    fn projectiles(&self) -> Result<Self::ProjectileIter<'_>> {
        Ok({
            let board = self.board()?;
            // SAFETY: this pool is embedded in the Board held by the current shared scope.
            let array = unsafe { NonNull::new_unchecked(ptrs::Board::projectiles(board.as_ptr())) };
            // SAFETY: the pool remains alive throughout the returned iterator's backend borrow.
            let max = unsafe { ptrs::DataArray::max_used_count(array.as_ptr()) };
            PvzProjectileIter {
                backend: self,
                next: 0,
                max,
            }
        })
    }

    fn projectile_id<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> ProjectileId {
        ProjectileId::from_raw(unsafe { ptrs::DataArray::<ptrs::Projectile>::item_id(projectile.ptr.as_ptr()) })
    }

    fn projectile_kind<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> i32 {
        unsafe { ptrs::Projectile::projectile_type(projectile.ptr.as_ptr()) }
    }

    fn projectile_motion<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> i32 {
        unsafe { ptrs::Projectile::motion(projectile.ptr.as_ptr()) }
    }

    fn projectile_row<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> i32 {
        // SAFETY: the borrow-bound handle is valid for this scalar read.
        unsafe { ptrs::Projectile::row(projectile.ptr.as_ptr()) }
    }

    fn projectile_pos_x<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32 {
        // SAFETY: the borrow-bound handle remains valid; only this scalar is copied.
        unsafe { ptrs::Projectile::pos_x(projectile.ptr.as_ptr()) }
    }
    fn projectile_pos_y<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32 {
        // SAFETY: the borrow-bound handle remains valid; only this scalar is copied.
        unsafe { ptrs::Projectile::pos_y(projectile.ptr.as_ptr()) }
    }
    fn projectile_pos_z<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32 {
        // SAFETY: the borrow-bound handle remains valid; only this scalar is copied.
        unsafe { ptrs::Projectile::pos_z(projectile.ptr.as_ptr()) }
    }

    fn projectile_vel_x<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32 {
        // SAFETY: the borrow-bound handle remains valid; only this scalar is copied.
        unsafe { ptrs::Projectile::vel_x(projectile.ptr.as_ptr()) }
    }
    fn projectile_vel_y<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32 {
        // SAFETY: the borrow-bound handle remains valid; only this scalar is copied.
        unsafe { ptrs::Projectile::vel_y(projectile.ptr.as_ptr()) }
    }
    fn projectile_vel_z<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32 {
        // SAFETY: the borrow-bound handle remains valid; only this scalar is copied.
        unsafe { ptrs::Projectile::vel_z(projectile.ptr.as_ptr()) }
    }

    fn projectile_acceleration<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32 {
        unsafe { ptrs::Projectile::acc_z(projectile.ptr.as_ptr()) }
    }

    fn projectile_collision<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> Result<ContactRect> {
        native_rect("Projectile::GetProjectileRect", |out| {
            // SAFETY: objdump verifies ESI=Projectile/ECX=out for GetProjectileRect.
            unsafe { crate::raw::abi::projectile_get_projectile_rect(projectile.ptr.as_ptr(), out) }
        })
    }

    fn projectile_damage_flags<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> u32 {
        // SAFETY: this handle is borrow-bound and the field is copied.
        unsafe { ptrs::Projectile::damage_flags(projectile.ptr.as_ptr()) as u32 }
    }

    fn projectile_age<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> i32 {
        // SAFETY: the borrow-bound handle remains valid; only this scalar is copied.
        unsafe { ptrs::Projectile::age(projectile.ptr.as_ptr()) }
    }
    fn projectile_torch_col<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> i32 {
        // SAFETY: the borrow-bound handle remains valid; only this scalar is copied.
        unsafe { ptrs::Projectile::torch_col(projectile.ptr.as_ptr()) }
    }
    fn projectile_cob_target_row<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> i32 {
        // SAFETY: the borrow-bound handle remains valid; only this scalar is copied.
        unsafe { ptrs::Projectile::cob_target_row(projectile.ptr.as_ptr()) }
    }

    fn projectile_target_zombie_id<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> u32 {
        // SAFETY: the borrow-bound handle remains valid; only this scalar is copied.
        unsafe { ptrs::Projectile::target_zombie_id(projectile.ptr.as_ptr()) }
    }
    fn projectile_cob_target_x_bits<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> u32 {
        // SAFETY: the borrow-bound handle remains valid; only this scalar is copied.
        unsafe { ptrs::Projectile::cob_target_x(projectile.ptr.as_ptr()).to_bits() }
    }
}

impl GridItemStateBackend for Pvz1051Backend {
    fn grid_item_countdown<'a>(&'a self, item: Self::GridItemHandle<'a>) -> i32 {
        unsafe { ptrs::GridItem::grid_item_counter(item.ptr.as_ptr()) }
    }
}
