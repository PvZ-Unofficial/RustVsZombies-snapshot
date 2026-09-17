//! One-tick maintenance uses the same current operations as a direct edit.
pub use super::{
    plant::set_plant_hp as keep_plant_hp_tick, resource::set_sun as keep_sun_tick,
    zombie::set_zombie_x as pin_zombie_x_tick,
};
