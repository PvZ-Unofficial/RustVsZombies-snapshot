//! Narrow Rust bindings for PE's 1051-shaped atomic facts and actions.
//!
//! The C++ bridge is owned by RustVsZombies. It returns only status codes and
//! out-scalars/out-pointers; borrowed object pointers are bounded by `World` and
//! backend-token lifetimes rather than copied into snapshots.

pub mod error;
pub mod kinds;
pub mod raw;
pub mod world;

pub use error::{Error, Result};
pub use kinds::{
    GridItemType, KernelPultRule, MaidCheat, PlantDamageRule, PlantType, PlantWeapon, PlantingReason, SceneType,
    ZombieDanceCheat, ZombieType,
};
pub use world::{
    Borrowed, CardRef, GridItemPool, GridItemRef, GridPlantStatusRef, IcePathDataRef, PlantPool, PlantRef, PoolSlot,
    ProjectilePool, ProjectileRef, Scene, SpawnDataRef, SunDataRef, World, ZombiePool, ZombieRef,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn reset_world(scene: SceneType) -> World {
        let mut world = World::new_deterministic(scene, 5489, 5489, 0).expect("world should construct");
        world
            .reset_deterministic(scene, 5489, 5489, 0)
            .expect("scene should reset");
        world
    }

    #[test]
    fn main_counter_tracks_logical_updates_and_reset() {
        let mut world = reset_world(SceneType::Pool);
        assert_eq!(world.scene().main_counter(), 0);
        world.update().expect("world should update");
        assert_eq!(world.scene().main_counter(), 1);
        world.scene().set_main_counter(0).expect("counter should normalize");
        assert_eq!(world.scene().main_counter(), 0);
        world
            .reset_deterministic(SceneType::Roof, 1, 2, 3)
            .expect("scene should reset");
        assert_eq!(world.scene().main_counter(), 0);
    }

    #[test]
    fn battle_randint_uses_the_31_bit_battle_stream() {
        let world = reset_world(SceneType::Pool);
        let scene = world.scene();

        assert_eq!(scene.battle_randint(49).expect("first draw"), 48);
        scene.rng_seed(false);
        scene.rng_locked(false);
        scene.rng_fixed(false);
        assert_eq!(scene.battle_randint(49).expect("second draw"), 35);
        assert_eq!(scene.battle_randint(0), Err(Error::InvalidArgument { detail: 0 }));
    }

    #[test]
    fn deterministic_world_setup_sets_both_streams_without_reseeding() {
        let mut world = World::new_deterministic(SceneType::Day, 11, 22, 33).expect("deterministic world");
        let scene = world.scene();
        assert_eq!(scene.rng_seed(false), 11);
        assert_eq!(scene.rng_seed(true), 22);
        assert_eq!(scene.dancer_clock(), 33);

        world
            .reset_deterministic(SceneType::Roof, 44, 55, 66)
            .expect("deterministic reset");
        let scene = world.scene();
        assert_eq!(scene.rng_seed(false), 44);
        assert_eq!(scene.rng_seed(true), 55);
        assert_eq!(scene.dancer_clock(), 66);
    }

    #[test]
    fn scene_type_switch_preserves_world_state() {
        let mut world = World::new_deterministic(SceneType::Day, 11, 22, 33).expect("deterministic world");
        let scene = world.scene();
        scene.set_main_counter(17).expect("counter");
        scene.set_zombie_spawn_stopped(true).expect("spawn rule");
        let battle_rng = (scene.rng_seed(false), scene.rng_locked(false), scene.rng_fixed(false));
        let level_rng = (scene.rng_seed(true), scene.rng_locked(true), scene.rng_fixed(true));
        let scene_ptr = scene.spawn_data().as_ptr();

        world.set_scene_type(SceneType::Pool).expect("scene type switch");

        let scene = world.scene();
        assert!(scene.is_pool_square(0, 2));
        assert!(scene.row_can_have_zombies(5));
        assert_eq!(scene.main_counter(), 17);
        assert!(scene.zombie_spawn_stopped());
        assert_eq!(
            (scene.rng_seed(false), scene.rng_locked(false), scene.rng_fixed(false)),
            battle_rng
        );
        assert_eq!(
            (scene.rng_seed(true), scene.rng_locked(true), scene.rng_fixed(true)),
            level_rng
        );
        assert_eq!(scene.spawn_data().as_ptr(), scene_ptr);
    }

    #[test]
    fn cached_scene_pointer_stays_stable_across_reset() {
        let mut world = reset_world(SceneType::Day);
        let before = world.scene().spawn_data().as_ptr();
        world
            .reset_deterministic(SceneType::Roof, 44, 55, 66)
            .expect("deterministic reset");
        assert_eq!(world.scene().spawn_data().as_ptr(), before);
    }

    #[test]
    fn spawn_pick_rejects_empty_or_scene_incompatible_candidates() {
        for (scene_type, allowed) in [(SceneType::Day, 11), (SceneType::Roof, 8)] {
            let mut world = World::new_deterministic(scene_type, 1, 2, 3).expect("world");
            let scene = world.scene();
            for index in 0..33 {
                scene.set_spawn_flag(index, false).expect("clear spawn flag");
            }
            assert!(!world.pick_spawn_list().expect("empty pick should be rejected"));
            world
                .scene()
                .set_spawn_flag(allowed, true)
                .expect("set incompatible flag");
            assert!(!world.pick_spawn_list().expect("incompatible pick should be rejected"));
        }
    }

    #[test]
    fn cob_modifier_toggles_round_trip_independently_and_reset() {
        let mut world = reset_world(SceneType::Roof);
        {
            let scene = world.scene();
            assert!(!scene.cob_fixed_delay());
            assert!(!scene.cob_drift_fixed());

            scene.set_cob_fixed_delay(true).unwrap();
            scene.set_cob_drift_fixed(true).unwrap();
            assert!(scene.cob_fixed_delay());
            assert!(scene.cob_drift_fixed());

            scene.set_cob_fixed_delay(false).unwrap();
            assert!(!scene.cob_fixed_delay());
            assert!(scene.cob_drift_fixed());
            scene.set_cob_fixed_delay(true).unwrap();
        }

        world
            .reset_deterministic(SceneType::Roof, 1, 2, 3)
            .expect("scene should reset");
        let scene = world.scene();
        assert!(!scene.cob_fixed_delay());
        assert!(!scene.cob_drift_fixed());
    }

    #[test]
    fn jack_explosion_modifier_round_trips_and_resets() {
        let mut world = reset_world(SceneType::Day);
        {
            let scene = world.scene();
            assert!(!scene.jack_explosions_disabled());
            scene.set_jack_explosions_disabled(true).unwrap();
            assert!(scene.jack_explosions_disabled());
        }

        world
            .reset_deterministic(SceneType::Day, 1, 2, 3)
            .expect("scene should reset");
        assert!(!world.scene().jack_explosions_disabled());
    }

    #[test]
    fn pool_supports_native_index_and_generation_id() {
        let mut world = reset_world(SceneType::Pool);
        let plant = world
            .new_plant(0, 0, PlantType::Sunflower, PlantType::None)
            .expect("plant creation should run")
            .expect("plant should be created");
        let pool = world.scene().plants();
        let id = pool.get_id(plant).unwrap();
        let index = (id & 0xffff) as i32;
        assert_eq!(pool.get(index), Some(plant));
        assert_eq!(pool.try_to_get(id), Some(plant));
        assert_eq!(pool.try_to_get(0), None);
        assert_eq!(pool.try_to_get(id & 0xffff), None);
        assert_eq!(pool.try_to_get(id.wrapping_add(1 << 16)), None);
        let scan_limit = pool.max_used_count();
        assert_eq!(pool.try_to_get((1 << 16) | scan_limit), None);
        assert_eq!(pool.try_to_get((1 << 16) | pool.capacity()), None);

        world.plant_die(plant).expect("plant should die");
        world.update().expect("dead plant slot should be reclaimed");
        assert_eq!(world.scene().plants().try_to_get(id), None);
        let replacement = world
            .new_plant(0, 0, PlantType::Sunflower, PlantType::None)
            .expect("replacement creation should run")
            .expect("replacement should be created");
        let replacement_id = world.scene().plants().get_id(replacement).unwrap();
        assert_ne!(replacement_id, id);
        assert_eq!(world.scene().plants().try_to_get(id), None);
    }

    #[test]
    fn imitater_morph_reports_exact_successor_for_one_update_boundary() {
        let mut world = reset_world(SceneType::Day);
        let placeholder = world
            .new_plant(0, 0, PlantType::Imitater, PlantType::PeaShooter)
            .expect("imitater creation should run")
            .expect("imitater should be created");
        let placeholder_id = world.scene().plants().get_id(placeholder).unwrap();

        for _ in 0..319 {
            world.update().expect("world should update");
        }
        assert_eq!(world.imitater_morph_successor(placeholder_id), None);

        world.update().expect("morph update should run");
        let successor_id = world.imitater_morph_successor(placeholder_id).unwrap();
        assert_ne!(successor_id, placeholder_id);
        assert!(world.scene().plants().try_to_get(successor_id).is_some());

        world.update().expect("placeholder reclaim update should run");
        assert_eq!(world.imitater_morph_successor(placeholder_id), None);
    }

    #[test]
    fn safe_geometry_rejects_out_of_range_inputs() {
        let world = reset_world(SceneType::Day);
        let scene = world.scene();

        assert!(matches!(
            scene.grid_to_pixel_x(-1, 0),
            Err(Error::InvalidArgument { .. })
        ));
        assert!(matches!(
            scene.grid_to_pixel_y(9, 0),
            Err(Error::InvalidArgument { .. })
        ));
        assert!(matches!(
            scene.grid_to_pixel_x(0, 5),
            Err(Error::InvalidArgument { .. })
        ));
        assert!(matches!(
            scene.pos_y_based_on_row(40.0, -1),
            Err(Error::InvalidArgument { .. })
        ));
        assert_eq!(scene.grid_to_pixel_x(0, 0).expect("valid grid x"), 40);
        assert_eq!(
            scene.grid_plant_status_at(i32::MAX, i32::MIN),
            Err(Error::InvalidArgument { detail: i32::MAX })
        );
    }

    #[test]
    fn world_operations_reject_foreign_objects() {
        let source = reset_world(SceneType::Day);
        let target = reset_world(SceneType::Day);
        let plant = source
            .new_plant(0, 0, PlantType::Sunflower, PlantType::None)
            .unwrap()
            .unwrap();
        let zombie = source.place_zombie(ZombieType::Normal, 8, 0).unwrap().unwrap();

        assert!(matches!(
            target.zombie_can_be_attacked(zombie, 0),
            Err(Error::NotFound { .. })
        ));
        assert!(matches!(
            target.zombie_can_attack_plant(zombie, plant, 0),
            Err(Error::NotFound { .. })
        ));
    }

    #[test]
    fn place_zombie_reports_rejection_and_pool_exhaustion() {
        let world = reset_world(SceneType::Day);
        assert!(world.place_zombie(ZombieType::Normal, 8, 0).unwrap().is_some());
        assert_eq!(world.place_zombie(ZombieType::Normal, -1, 0).unwrap(), None);
        for _ in 1..1023 {
            assert!(world.place_zombie(ZombieType::Normal, 8, 0).unwrap().is_some());
        }
        assert_eq!(world.place_zombie(ZombieType::Normal, 8, 0).unwrap(), None);
    }

    #[test]
    fn safe_plant_cost_rejects_invalid_packet_imitater_pairs() {
        let world = reset_world(SceneType::Pool);

        assert!(matches!(
            world.current_plant_cost(PlantType::Imitater, PlantType::None),
            Err(Error::InvalidArgument { .. })
        ));
        assert!(matches!(
            world.current_plant_cost(PlantType::PeaShooter, PlantType::Sunflower),
            Err(Error::InvalidArgument { .. })
        ));
        assert!(
            world
                .current_plant_cost(PlantType::Imitater, PlantType::Sunflower)
                .is_ok()
        );
        assert!(world.current_plant_cost(PlantType::PeaShooter, PlantType::None).is_ok());
    }
}
