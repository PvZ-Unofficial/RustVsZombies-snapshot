#![expect(
    clippy::inline_asm_x86_att_syntax,
    reason = "the verified 1051 ABI macro emits AT&T templates intentionally"
)]

mod app;
mod board;
mod challenge;
mod effect;
mod objects;
mod plant;
mod seed;
mod widget;
mod zombie;

pub(crate) use app::*;
pub(crate) use board::*;
pub(crate) use challenge::*;
pub(crate) use effect::*;
pub(crate) use objects::*;
pub(crate) use plant::*;
pub(crate) use seed::*;
pub(crate) use widget::*;
pub(crate) use zombie::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::raw::layout::{Board, GridItem, Plant, Projectile, SeedPacket, Zombie};
    use crate::raw::types::RawRect;

    #[test]
    fn contact_wrapper_signatures_compile_without_calls() {
        let _get_rect: unsafe fn(*mut Zombie, *mut RawRect) -> *mut RawRect = zombie_get_zombie_rect;
        let _projectile_rect: unsafe fn(*mut Projectile, *mut RawRect) -> *mut RawRect = projectile_get_projectile_rect;
        let _effected: unsafe fn(*mut Zombie, u32) -> u8 = zombie_effected_by_damage;
        let _grid_x: unsafe fn(*mut Board, i32, i32) -> i32 = board_pixel_to_grid_x_keep_on_board;
        let _grid_y: unsafe fn(*mut Board, i32, i32) -> i32 = board_pixel_to_grid_y_keep_on_board;
    }

    #[test]
    fn restored_atomic_wrapper_signatures_compile_without_calls() {
        let _new_plant: unsafe fn(i32, i32, i32, i32) -> *mut Plant = board_new_plant;
        let _add_plant: unsafe fn(i32, i32, i32, i32) -> *mut Plant = board_add_plant;
        let _can_plant: unsafe fn(i32, i32, i32) -> i32 = board_can_plant_at;
        let _ladder: unsafe fn(i32, i32) -> *mut GridItem = board_add_ladder;
        let _crater: unsafe fn(i32, i32) -> *mut GridItem = board_add_crater;
        let _alloc_grid_item: unsafe fn(*mut Board) -> *mut GridItem = data_array_alloc_grid_item;
        let _add_zombie: unsafe fn(*mut Board, i32, i32, i32) -> *mut Zombie = board_add_zombie_in_row;
        let _place_zombie: unsafe fn(i32, i32, i32) = challenge_izombie_place_zombie;
        let _board_row_y: unsafe fn(*mut Board, f32, i32) -> f32 = board_get_pos_y_based_on_row;
        let _zombie_row_y: unsafe fn(*mut Zombie, i32) -> f32 = zombie_get_pos_y_based_on_row;
        let _seed_can_pick_up: unsafe fn(*mut SeedPacket) -> u8 = seed_packet_can_pick_up;
        let _seed_was_planted: unsafe fn(*mut SeedPacket) = seed_packet_was_planted;
        let _draw_screen: unsafe fn() -> u8 = widget_manager_draw_screen;
    }
}
