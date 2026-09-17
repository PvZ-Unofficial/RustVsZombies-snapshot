//! Projectile capabilities. Entity operations use current borrowed handles.

use crate::backend::Backend;
use rsvz_model::model::{ContactRect, ProjectileId};

pub trait ProjectileReadBackend: Backend {
    type ProjectileHandle<'a>: Copy
    where
        Self: 'a;
    type ProjectileIter<'a>: Iterator<Item = Self::ProjectileHandle<'a>>
    where
        Self: 'a;

    /// In-flight gameplay projectiles, excluding permanently targetless homing needles.
    /// Temporary target invulnerability does not make a projectile irrelevant.
    fn projectiles(&self) -> Result<Self::ProjectileIter<'_>, Self::Error>;
    fn projectile_id<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> ProjectileId;
    fn projectile_kind<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> i32;
    fn projectile_motion<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> i32;
    fn projectile_row<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> i32;
    fn projectile_pos_x<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32;
    fn projectile_pos_y<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32;
    fn projectile_pos_z<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32;
    fn projectile_vel_x<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32;
    fn projectile_vel_y<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32;
    fn projectile_vel_z<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32;
    fn projectile_acceleration<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> f32;
    fn projectile_collision<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> Result<ContactRect, Self::Error>;
    fn projectile_damage_flags<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> u32;
    fn projectile_age<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> i32;
    fn projectile_torch_col<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> i32;
    fn projectile_cob_target_row<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> i32;
    fn projectile_target_zombie_id<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> u32;
    fn projectile_cob_target_x_bits<'a>(&'a self, projectile: Self::ProjectileHandle<'a>) -> u32;
}
