//! Narrow Rust bindings for PvZ-Portable's native facts and actions.
//!
//! Object pointers stay bounded by world/backend borrows and are never copied
//! into snapshots or collections.

pub mod capabilities;
pub mod raw;

mod error;
pub mod modifier;
mod world;

pub use error::{Error, Result};
pub use world::{
    Borrowed, GridItemPool, GridItemRef, ItemPool, ItemRef, PlantPool, PlantRef, PoolSlot, ProjectilePool,
    ProjectileRef, ReanimationRef, SeedRef, World, ZombiePool, ZombieRef, advanced_pause_active, app_counter,
    back_to_main_menu, click_continue_dialog_if_present, enter_endless, fast_forward_active, game_ui, input_focused,
    random_fixed, random_locked, random_seed, request_seed_chooser_fast_forward, restore_game_speed,
    seed_chooser_cancel_view_lawn, seed_chooser_choose_state, seed_chooser_modal_present, seed_chooser_mouse_visible,
    seed_chooser_parent_present, seed_chooser_seeds_in_flight, seed_chooser_view_lawn_time,
    seed_chooser_widget_manager_present, select_card, selected_card, selected_card_count, set_advanced_pause,
    set_app_counter, set_fast_forward, set_game_speed, set_random_mode, set_sun_production_mode,
    set_wave_spawn_random_seed, start_battle,
};

fn expected_layout() -> raw::pvzp_rs_layout_fingerprint {
    use std::mem::{offset_of, size_of};

    raw::pvzp_rs_layout_fingerprint {
        abi_version: 1,
        pointer_size: size_of::<*const ()>() as u32,
        game_object_size: size_of::<raw::GameObject>() as u32,
        plant_size: size_of::<raw::pvzp_rs_plant>() as u32,
        zombie_size: size_of::<raw::pvzp_rs_zombie>() as u32,
        projectile_size: size_of::<raw::pvzp_rs_projectile>() as u32,
        grid_item_size: size_of::<raw::pvzp_rs_grid_item>() as u32,
        item_size: size_of::<raw::pvzp_rs_item>() as u32,
        seed_size: size_of::<raw::pvzp_rs_seed>() as u32,
        reanimation_size: size_of::<raw::pvzp_rs_reanimation>() as u32,
        plant_seed_type: offset_of!(raw::pvzp_rs_plant, mSeedType) as u32,
        plant_health: offset_of!(raw::pvzp_rs_plant, mPlantHealth) as u32,
        plant_state: offset_of!(raw::pvzp_rs_plant, mState) as u32,
        plant_dead: offset_of!(raw::pvzp_rs_plant, mDead) as u32,
        zombie_type: offset_of!(raw::pvzp_rs_zombie, mZombieType) as u32,
        zombie_phase: offset_of!(raw::pvzp_rs_zombie, mZombiePhase) as u32,
        zombie_health: offset_of!(raw::pvzp_rs_zombie, mBodyHealth) as u32,
        zombie_dead: offset_of!(raw::pvzp_rs_zombie, mDead) as u32,
        projectile_type: offset_of!(raw::pvzp_rs_projectile, mProjectileType) as u32,
        projectile_dead: offset_of!(raw::pvzp_rs_projectile, mDead) as u32,
        grid_item_type: offset_of!(raw::pvzp_rs_grid_item, mGridItemType) as u32,
        grid_item_dead: offset_of!(raw::pvzp_rs_grid_item, mDead) as u32,
        item_type: offset_of!(raw::pvzp_rs_item, mType) as u32,
        item_dead: offset_of!(raw::pvzp_rs_item, mDead) as u32,
        seed_refresh_counter: offset_of!(raw::pvzp_rs_seed, mRefreshCounter) as u32,
        seed_packet_type: offset_of!(raw::pvzp_rs_seed, mPacketType) as u32,
        reanimation_time: offset_of!(raw::pvzp_rs_reanimation, mAnimTime) as u32,
        reanimation_rate: offset_of!(raw::pvzp_rs_reanimation, mAnimRate) as u32,
    }
}

fn layout_words(layout: &raw::pvzp_rs_layout_fingerprint) -> [u32; 28] {
    [
        layout.abi_version,
        layout.pointer_size,
        layout.game_object_size,
        layout.plant_size,
        layout.zombie_size,
        layout.projectile_size,
        layout.grid_item_size,
        layout.item_size,
        layout.seed_size,
        layout.reanimation_size,
        layout.plant_seed_type,
        layout.plant_health,
        layout.plant_state,
        layout.plant_dead,
        layout.zombie_type,
        layout.zombie_phase,
        layout.zombie_health,
        layout.zombie_dead,
        layout.projectile_type,
        layout.projectile_dead,
        layout.grid_item_type,
        layout.grid_item_dead,
        layout.item_type,
        layout.item_dead,
        layout.seed_refresh_counter,
        layout.seed_packet_type,
        layout.reanimation_time,
        layout.reanimation_rate,
    ]
}

fn layout_matches(actual: &raw::pvzp_rs_layout_fingerprint) -> bool {
    layout_words(actual) == layout_words(&expected_layout())
}

pub fn verify_layout() -> Result<()> {
    use std::mem::MaybeUninit;

    let mut actual = MaybeUninit::<raw::pvzp_rs_layout_fingerprint>::uninit();
    // SAFETY: the bridge writes one complete POD fingerprint to the supplied out pointer.
    Error::from_status(unsafe { raw::pvzp_rs_layout(actual.as_mut_ptr()) })?;
    // SAFETY: a successful status guarantees that the bridge initialized every field.
    let actual = unsafe { actual.assume_init() };
    // SAFETY: this compares fixed layout words against the plugin compiler's
    // complete native manifest; no game objects are touched.
    let raw_matches = unsafe {
        raw::pvzp_rs_check_raw_layout(
            raw_layout_checks::RAW_LAYOUT.as_ptr(),
            raw_layout_checks::RAW_LAYOUT.len() as u32,
        ) != 0
    };
    if layout_matches(&actual) && raw_matches {
        Ok(())
    } else {
        Err(Error::LayoutMismatch)
    }
}

mod raw_layout_checks {
    include!(concat!(env!("OUT_DIR"), "/raw_layout_checks.rs"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_fingerprint_rejects_each_changed_word() {
        let expected = expected_layout();
        assert!(layout_matches(&expected));
        for index in 0..layout_words(&expected).len() {
            let mut changed = expected_layout();
            // SAFETY: the fingerprint is a repr(C) struct of exactly 28 u32 fields.
            unsafe {
                std::ptr::from_mut(&mut changed)
                    .cast::<u32>()
                    .add(index)
                    .write(layout_words(&expected)[index] ^ 1)
            };
            assert!(!layout_matches(&changed), "word {index} was not checked");
        }
    }
}
