#![allow(unsafe_code, reason = "this module is the checked wrapper around the Portable C ABI")]

use crate::{Error, Result, raw};

fn out_bool(call: impl FnOnce(*mut u8) -> raw::pvzp_rs_status) -> Result<bool> {
    let mut value = 0;
    Error::from_status(call(&raw mut value))?;
    Ok(value != 0)
}

fn out_i32(call: impl FnOnce(*mut i32) -> raw::pvzp_rs_status) -> Result<i32> {
    let mut value = 0;
    Error::from_status(call(&raw mut value))?;
    Ok(value)
}

macro_rules! bool_rule {
    ($get:ident, $set:ident, $raw_get:path, $raw_set:path) => {
        pub fn $get() -> bool {
            // SAFETY: the getter reads only bridge-owned scalar state and takes no pointers.
            unsafe { $raw_get() != 0 }
        }

        pub fn $set(enabled: bool) -> Result<()> {
            // SAFETY: the boolean is copied into bridge-owned modifier state.
            Error::from_status(unsafe { $raw_set(u8::from(enabled)) })
        }
    };
}

bool_rule!(
    seed_recharge_ignored,
    set_seed_recharge_ignored,
    raw::pvzp_rs_seed_recharge_ignored,
    raw::pvzp_rs_set_seed_recharge_ignored
);
bool_rule!(
    sun_cost_ignored,
    set_sun_cost_ignored,
    raw::pvzp_rs_sun_cost_ignored,
    raw::pvzp_rs_set_sun_cost_ignored
);
bool_rule!(
    fog_revealed,
    set_fog_revealed,
    raw::pvzp_rs_fog_revealed,
    raw::pvzp_rs_set_fog_revealed
);
bool_rule!(
    vase_contents_visible,
    set_vase_contents_visible,
    raw::pvzp_rs_vase_contents_visible,
    raw::pvzp_rs_set_vase_contents_visible
);
bool_rule!(
    instant_ice_and_ash_effects,
    set_instant_ice_and_ash_effects,
    raw::pvzp_rs_instant_ice_and_ash_effects,
    raw::pvzp_rs_set_instant_ice_and_ash_effects
);
bool_rule!(
    mushrooms_awake,
    set_mushrooms_awake,
    raw::pvzp_rs_mushrooms_awake,
    raw::pvzp_rs_set_mushrooms_awake
);
bool_rule!(
    cob_fixed_delay,
    set_cob_fixed_delay,
    raw::pvzp_rs_cob_fixed_delay,
    raw::pvzp_rs_set_cob_fixed_delay
);
bool_rule!(
    cob_recharge_shortened,
    set_cob_recharge_shortened,
    raw::pvzp_rs_cob_recharge_shortened,
    raw::pvzp_rs_set_cob_recharge_shortened
);
bool_rule!(
    cob_drift_fixed,
    set_cob_drift_fixed,
    raw::pvzp_rs_cob_drift_fixed,
    raw::pvzp_rs_set_cob_drift_fixed
);
bool_rule!(
    item_drop_disabled,
    set_item_drop_disabled,
    raw::pvzp_rs_item_drop_disabled,
    raw::pvzp_rs_set_item_drop_disabled
);
bool_rule!(
    natural_sun_drop_disabled,
    set_natural_sun_drop_disabled,
    raw::pvzp_rs_natural_sun_drop_disabled,
    raw::pvzp_rs_set_natural_sun_drop_disabled
);
bool_rule!(
    jack_explosions_disabled,
    set_jack_explosions_disabled,
    raw::pvzp_rs_jack_explosions_disabled,
    raw::pvzp_rs_set_jack_explosions_disabled
);
bool_rule!(
    pepper_explosions_disabled,
    set_pepper_explosions_disabled,
    raw::pvzp_rs_pepper_explosions_disabled,
    raw::pvzp_rs_set_pepper_explosions_disabled
);
bool_rule!(
    special_events_disabled,
    set_special_events_disabled,
    raw::pvzp_rs_special_events_disabled,
    raw::pvzp_rs_set_special_events_disabled
);
bool_rule!(
    zombie_spawn_stopped,
    set_zombie_spawn_stopped,
    raw::pvzp_rs_zombie_spawn_stopped,
    raw::pvzp_rs_set_zombie_spawn_stopped
);
bool_rule!(
    zombies_die_at_house,
    set_zombies_die_at_house,
    raw::pvzp_rs_zombies_die_at_house,
    raw::pvzp_rs_set_zombies_die_at_house
);
bool_rule!(
    planting_restrictions_ignored,
    set_planting_restrictions_ignored,
    raw::pvzp_rs_planting_restrictions_ignored,
    raw::pvzp_rs_set_planting_restrictions_ignored
);
bool_rule!(
    profile_readonly,
    set_profile_readonly,
    raw::pvzp_rs_profile_readonly,
    raw::pvzp_rs_set_profile_readonly
);
bool_rule!(
    normal_auto_collect_enabled,
    set_normal_auto_collect_enabled,
    raw::pvzp_rs_normal_auto_collect_enabled,
    raw::pvzp_rs_set_normal_auto_collect_enabled
);
pub fn easy_planting_cheat() -> Result<bool> {
    out_bool(|out| {
        // SAFETY: output is writable; this distinct native getter may report a missing app.
        unsafe { raw::pvzp_rs_easy_planting_cheat(out) }
    })
}

pub fn set_easy_planting_cheat(enabled: bool) -> Result<()> {
    // SAFETY: the copied boolean is validated; the bridge checks the current app.
    Error::from_status(unsafe { raw::pvzp_rs_set_easy_planting_cheat(u8::from(enabled)) })
}

macro_rules! enum_rule {
    ($get:ident, $set:ident, $raw_get:path, $raw_set:path) => {
        pub fn $get() -> Result<i32> {
            out_i32(|out| {
                // SAFETY: `out` points to writable scalar storage.
                unsafe { $raw_get(out) }
            })
        }

        pub fn $set(rule: i32) -> Result<()> {
            // SAFETY: the enum code is validated by the bridge.
            Error::from_status(unsafe { $raw_set(rule) })
        }
    };
}

enum_rule!(
    kernel_pult_projectile_rule,
    set_kernel_pult_projectile_rule,
    raw::pvzp_rs_kernel_pult_projectile_rule,
    raw::pvzp_rs_set_kernel_pult_projectile_rule
);
enum_rule!(
    plant_damage_rule,
    set_plant_damage_rule,
    raw::pvzp_rs_plant_damage_rule,
    raw::pvzp_rs_set_plant_damage_rule
);
enum_rule!(
    maid_cheat,
    set_maid_cheat,
    raw::pvzp_rs_maid_cheat,
    raw::pvzp_rs_set_maid_cheat
);
