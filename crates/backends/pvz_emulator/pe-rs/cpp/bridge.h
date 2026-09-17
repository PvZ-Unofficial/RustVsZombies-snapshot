#pragma once

#include "raw_layout.h"

#include <cstdint>

extern "C" {

struct pe_rs_world;
struct pe_rs_scene;
struct pe_rs_plant_pool;
struct pe_rs_zombie_pool;
struct pe_rs_griditem_pool;
struct pe_rs_projectile_pool;

typedef std::uint32_t (*pe_rs_begin_plant_effect_fn)(
    std::uint8_t source,
    std::uint32_t actor_id,
    std::uint32_t plant_id,
    std::int32_t raw_kind,
    std::int32_t effective_kind,
    std::int32_t row,
    std::int32_t col,
    std::int32_t hp,
    std::int32_t max_hp,
    std::uint8_t effect,
    std::int32_t native_requested,
    std::int32_t main_counter,
    std::uint8_t* out_suppress);
typedef void (*pe_rs_finish_plant_effect_fn)(std::uint32_t token, std::uint8_t outcome, std::int32_t applied);
typedef void (*pe_rs_emit_home_entry_fn)(
    std::uint32_t zombie_id,
    std::int32_t zombie_kind,
    std::int32_t row,
    std::int32_t main_counter);
typedef void (*pe_rs_emit_gargantuar_spawned_fn)(
    std::uint32_t parent_id,
    std::int32_t parent_kind,
    std::int32_t from_wave,
    std::int32_t row,
    std::int32_t main_counter);
typedef void (*pe_rs_emit_imp_thrown_fn)(
    std::uint32_t parent_id,
    std::uint32_t imp_id,
    std::int32_t parent_kind,
    std::int32_t from_wave,
    std::int32_t row,
    std::int32_t main_counter,
    float parent_x,
    std::int32_t parent_hp,
    std::int32_t parent_max_hp,
    std::int32_t parent_phase,
    float parent_speed_x,
    std::int32_t parent_frozen,
    std::int32_t parent_chilled,
    std::int32_t parent_buttered);
typedef void (*pe_rs_emit_gargantuar_ash_hit_fn)(
    std::uint32_t parent_id,
    std::int32_t main_counter,
    float x,
    std::int32_t hp_before,
    std::int32_t phase,
    std::int32_t frozen,
    std::int32_t chilled,
    std::int32_t buttered);

enum pe_rs_status_code : std::int32_t {
    PE_RS_STATUS_OK = 0,
    PE_RS_STATUS_NULL_POINTER = 1,
    PE_RS_STATUS_INVALID_ARGUMENT = 2,
    PE_RS_STATUS_NOT_FOUND = 3,
    PE_RS_STATUS_STD_EXCEPTION = 100,
    PE_RS_STATUS_BAD_ALLOC = 101,
    PE_RS_STATUS_UNKNOWN_EXCEPTION = 102,
};

struct pe_rs_status {
    std::int32_t code;
    std::int32_t detail;
};

pe_rs_status pe_rs_world_new_deterministic(
    std::int32_t scene_type,
    std::uint32_t battle_seed,
    std::uint32_t level_seed,
    std::uint32_t dancer_clock,
    pe_rs_world** out_world) noexcept;
pe_rs_status pe_rs_world_free(pe_rs_world* world) noexcept;
pe_rs_status pe_rs_world_set_scene_type(pe_rs_world* world, std::int32_t scene_type) noexcept;
pe_rs_status pe_rs_world_reset_deterministic(
    pe_rs_world* world,
    std::int32_t scene_type,
    std::uint32_t battle_seed,
    std::uint32_t level_seed,
    std::uint32_t dancer_clock) noexcept;
pe_rs_status pe_rs_world_update(pe_rs_world* world, std::uint8_t* out_terminal) noexcept;
pe_rs_status pe_rs_world_set_event_sink(
    pe_rs_world* world,
    std::uint32_t interest,
    pe_rs_begin_plant_effect_fn begin_plant_effect,
    pe_rs_finish_plant_effect_fn finish_plant_effect,
    pe_rs_emit_home_entry_fn emit_home_entry,
    pe_rs_emit_gargantuar_spawned_fn emit_gargantuar_spawned,
    pe_rs_emit_imp_thrown_fn emit_imp_thrown,
    pe_rs_emit_gargantuar_ash_hit_fn emit_gargantuar_ash_hit) noexcept;
pe_rs_status pe_rs_world_clear_event_sink(pe_rs_world* world) noexcept;
pe_rs_status pe_rs_world_restore_dancer_clock(pe_rs_world* world, std::uint32_t clock) noexcept;
pe_rs_status pe_rs_world_scene(pe_rs_world* world, pe_rs_scene** out_scene) noexcept;
std::uint32_t pe_rs_world_imitater_morph_successor(const pe_rs_world* world, std::uint32_t placeholder_id) noexcept;
pe_rs_status pe_rs_plant_imitater_morph(pe_rs_world* world, pe_rs_plant* plant, pe_rs_plant** out_plant) noexcept;
pe_rs_status pe_rs_world_reset_sun(pe_rs_world* world) noexcept;
pe_rs_status pe_rs_world_reset_spawn(pe_rs_world* world) noexcept;

pe_rs_status pe_rs_scene_seed_rngs(
    pe_rs_scene* scene,
    std::uint32_t battle_seed,
    std::uint32_t level_seed) noexcept;
pe_rs_status pe_rs_scene_lock_rngs(pe_rs_scene* scene, std::uint32_t value) noexcept;
pe_rs_status pe_rs_scene_set_wave_spawn_random_seed(
    pe_rs_scene* scene,
    std::uint8_t enabled,
    std::uint32_t base_seed) noexcept;
pe_rs_status pe_rs_scene_battle_randint(
    pe_rs_scene* scene,
    std::uint32_t upper,
    std::uint32_t* out_value) noexcept;
std::uint32_t pe_rs_scene_rng_seed(const pe_rs_scene* scene, bool level_stream) noexcept;
std::uint8_t pe_rs_scene_rng_locked(const pe_rs_scene* scene, bool level_stream) noexcept;
std::uint32_t pe_rs_scene_rng_fixed(const pe_rs_scene* scene, bool level_stream) noexcept;
std::uint32_t pe_rs_scene_main_counter(
    const pe_rs_scene* scene_value) noexcept;
pe_rs_status pe_rs_scene_set_main_counter(pe_rs_scene* scene, std::uint32_t counter) noexcept;
std::uint8_t pe_rs_scene_game_over(const pe_rs_scene* scene) noexcept;
std::uint32_t pe_rs_scene_dancer_clock(
    const pe_rs_scene* scene_value) noexcept;

pe_rs_spawn_data* pe_rs_scene_spawn_data(
    pe_rs_scene* scene_value) noexcept;
pe_rs_sun_data* pe_rs_scene_sun_data(pe_rs_scene* scene_value) noexcept;
pe_rs_ice_path_data* pe_rs_scene_ice_path_data(
    pe_rs_scene* scene_value) noexcept;
pe_rs_status pe_rs_scene_card_at(pe_rs_scene* scene, std::uint32_t slot, pe_rs_card** out_card) noexcept;
pe_rs_status pe_rs_scene_grid_plant_status_at(
    pe_rs_scene* scene,
    std::int32_t grid_x,
    std::int32_t grid_y,
    pe_rs_grid_plant_status** out_status) noexcept;

pe_rs_plant_pool* pe_rs_scene_plant_pool(
    pe_rs_scene* scene_value) noexcept;
pe_rs_zombie_pool* pe_rs_scene_zombie_pool(
    pe_rs_scene* scene_value) noexcept;
pe_rs_griditem_pool* pe_rs_scene_griditem_pool(
    pe_rs_scene* scene_value) noexcept;
pe_rs_projectile_pool* pe_rs_scene_projectile_pool(
    pe_rs_scene* scene_value) noexcept;

std::uint8_t pe_rs_scene_is_pool_square(
    const pe_rs_scene* scene_value,
    std::int32_t grid_x,
    std::int32_t grid_y) noexcept;
std::uint8_t pe_rs_scene_row_can_have_zombies(
    const pe_rs_scene* scene_value,
    std::int32_t row) noexcept;
pe_rs_status pe_rs_scene_grid_to_pixel_x(
    const pe_rs_scene* scene,
    std::int32_t grid_x,
    std::int32_t grid_y,
    std::int32_t* out_x) noexcept;
pe_rs_status pe_rs_scene_grid_to_pixel_y(
    const pe_rs_scene* scene,
    std::int32_t grid_x,
    std::int32_t grid_y,
    std::int32_t* out_y) noexcept;
pe_rs_status pe_rs_scene_pos_y_based_on_row(
    const pe_rs_scene* scene,
    float pos_x,
    std::int32_t row,
    float* out_y) noexcept;

#define PE_RS_DECLARE_POOL_API(prefix, pool_type, object_type) \
    std::uint32_t pe_rs_##prefix##_pool_max_used_count(const pool_type* pool) noexcept; \
    std::uint32_t pe_rs_##prefix##_pool_active_count(const pool_type* pool) noexcept; \
    std::uint32_t pe_rs_##prefix##_pool_capacity(const pool_type* pool) noexcept; \
    std::uint32_t pe_rs_##prefix##_pool_free_list_head(const pool_type* pool) noexcept; \
    std::uint32_t pe_rs_##prefix##_pool_next_id_key(const pool_type* pool) noexcept; \
    pe_rs_status pe_rs_##prefix##_pool_slot( \
        const pool_type* pool, std::uint32_t index, std::uint8_t* out_active, \
        std::uint32_t* out_id, std::uint32_t* out_next_free) noexcept; \
    object_type* pe_rs_##prefix##_pool_get(pool_type* pool, std::int32_t native_index) noexcept; \
    object_type* pe_rs_##prefix##_pool_try_to_get(pool_type* pool, std::uint32_t id) noexcept; \
    std::uint32_t pe_rs_##prefix##_pool_get_id(const pool_type* pool, const object_type* object) noexcept

PE_RS_DECLARE_POOL_API(plant, pe_rs_plant_pool, pe_rs_plant);
PE_RS_DECLARE_POOL_API(zombie, pe_rs_zombie_pool, pe_rs_zombie);
PE_RS_DECLARE_POOL_API(griditem, pe_rs_griditem_pool, pe_rs_griditem);
PE_RS_DECLARE_POOL_API(projectile, pe_rs_projectile_pool, pe_rs_projectile);

#undef PE_RS_DECLARE_POOL_API

std::int32_t pe_rs_spawn_total_zombies_health_in_wave(
    const pe_rs_world* world,
    std::uint32_t wave) noexcept;
std::uint32_t pe_rs_spawn_current_health(
    const pe_rs_world* world) noexcept;
pe_rs_status pe_rs_spawn_pick_list(pe_rs_world* world, std::uint8_t* out_picked) noexcept;
pe_rs_status pe_rs_spawn_entry(
    const pe_rs_scene* scene,
    std::uint32_t wave,
    std::uint32_t slot,
    std::int32_t* out_zombie_type) noexcept;
pe_rs_status pe_rs_spawn_set_entry(
    pe_rs_scene* scene,
    std::uint32_t wave,
    std::uint32_t slot,
    std::int32_t zombie_type) noexcept;
pe_rs_status pe_rs_spawn_flag(
    const pe_rs_scene* scene,
    std::uint32_t zombie_type,
    std::uint8_t* out_enabled) noexcept;
pe_rs_status pe_rs_spawn_set_flag(
    pe_rs_scene* scene,
    std::uint32_t zombie_type,
    std::uint8_t enabled) noexcept;

pe_rs_status pe_rs_current_plant_cost(
    const pe_rs_world* world,
    std::int32_t seed_type,
    std::int32_t imitater_type,
    std::int32_t* out_cost) noexcept;
pe_rs_status pe_rs_can_take_sun_money(
    const pe_rs_world* world,
    std::int32_t amount,
    std::uint8_t* out_allowed) noexcept;
pe_rs_status pe_rs_take_sun_money(
    pe_rs_world* world,
    std::int32_t amount,
    std::uint8_t* out_taken) noexcept;

pe_rs_status pe_rs_seed_can_pick_up(
    const pe_rs_world* world,
    const pe_rs_card* card,
    std::uint8_t* out_can_pick_up) noexcept;
pe_rs_status pe_rs_seed_was_planted(pe_rs_world* world, pe_rs_card* card) noexcept;

pe_rs_status pe_rs_can_plant_at(
    const pe_rs_world* world,
    std::int32_t grid_x,
    std::int32_t grid_y,
    std::int32_t planting_type,
    std::int32_t* out_reason) noexcept;
pe_rs_status pe_rs_new_plant(
    pe_rs_world* world,
    std::int32_t grid_x,
    std::int32_t grid_y,
    std::int32_t seed_type,
    std::int32_t imitater_type,
    pe_rs_plant** out_plant) noexcept;
pe_rs_status pe_rs_add_plant(
    pe_rs_world* world,
    std::int32_t grid_x,
    std::int32_t grid_y,
    std::int32_t seed_type,
    std::int32_t imitater_type,
    pe_rs_plant** out_plant) noexcept;
pe_rs_status pe_rs_plant_die(pe_rs_world* world, pe_rs_plant* plant) noexcept;
pe_rs_status pe_rs_plant_set_sleep(pe_rs_plant* plant, std::uint8_t asleep) noexcept;
pe_rs_status pe_rs_plant_play_idle(pe_rs_plant* plant, float fps) noexcept;
pe_rs_status pe_rs_plant_fire_cob(
    pe_rs_world* world,
    pe_rs_plant* plant,
    std::int32_t target_x,
    std::int32_t target_y,
    std::uint8_t* out_fired) noexcept;
void pe_rs_plant_hit_box(pe_rs_plant* plant, pe_rs_rect* out_rect) noexcept;
void pe_rs_plant_attack_rect(
    const pe_rs_plant* plant,
    std::int32_t weapon,
    pe_rs_rect* out_rect) noexcept;
std::int32_t pe_rs_plant_damage_range_flags(const pe_rs_plant* plant, std::int32_t weapon) noexcept;

pe_rs_status pe_rs_add_zombie_in_row(
    pe_rs_world* world,
    std::int32_t zombie_type,
    std::int32_t row,
    std::int32_t from_wave,
    pe_rs_zombie** out_zombie) noexcept;
pe_rs_status pe_rs_place_zombie(
    pe_rs_world* world,
    std::int32_t zombie_type,
    std::int32_t grid_x,
    std::int32_t grid_y,
    pe_rs_zombie** out_zombie) noexcept;
pe_rs_status pe_rs_zombie_die_no_loot(pe_rs_world* world, pe_rs_zombie* zombie) noexcept;
pe_rs_status pe_rs_zombie_can_be_attacked(
    const pe_rs_world* world,
    const pe_rs_zombie* zombie,
    std::uint8_t flags,
    std::uint8_t* out_can_be_attacked) noexcept;
pe_rs_status pe_rs_zombie_can_attack_plant(
    pe_rs_world* world,
    pe_rs_zombie* zombie,
    pe_rs_plant* plant,
    std::int32_t attack_type,
    std::uint8_t* out_can_attack) noexcept;
void pe_rs_zombie_hit_box(
    const pe_rs_zombie* zombie,
    pe_rs_rect* out_rect) noexcept;
void pe_rs_projectile_attack_box(
    const pe_rs_projectile* projectile,
    pe_rs_rect* out_rect) noexcept;

pe_rs_status pe_rs_add_ladder(
    pe_rs_world* world,
    std::int32_t grid_x,
    std::int32_t grid_y,
    pe_rs_griditem** out_item) noexcept;
pe_rs_status pe_rs_add_crater(
    pe_rs_world* world,
    std::int32_t grid_x,
    std::int32_t grid_y,
    pe_rs_griditem** out_item) noexcept;
pe_rs_status pe_rs_add_gravestone(
    pe_rs_world* world,
    std::int32_t grid_x,
    std::int32_t grid_y,
    pe_rs_griditem** out_item) noexcept;
pe_rs_status pe_rs_griditem_die(pe_rs_world* world, pe_rs_griditem* item) noexcept;

#define PE_RS_DECLARE_BOOL_MODIFIER(name) \
    std::uint8_t pe_rs_scene_##name(const pe_rs_scene* scene) noexcept; \
    pe_rs_status pe_rs_scene_set_##name(pe_rs_scene* scene, std::uint8_t enabled) noexcept

PE_RS_DECLARE_BOOL_MODIFIER(seed_recharge_ignored);
PE_RS_DECLARE_BOOL_MODIFIER(sun_cost_ignored);
PE_RS_DECLARE_BOOL_MODIFIER(instant_special_effects);
PE_RS_DECLARE_BOOL_MODIFIER(easy_planting_cheat);
PE_RS_DECLARE_BOOL_MODIFIER(planting_restrictions_ignored);
PE_RS_DECLARE_BOOL_MODIFIER(mushrooms_awake);
PE_RS_DECLARE_BOOL_MODIFIER(cob_delay_disabled);
PE_RS_DECLARE_BOOL_MODIFIER(cob_fixed_delay);
PE_RS_DECLARE_BOOL_MODIFIER(cob_recharge_shortened);
PE_RS_DECLARE_BOOL_MODIFIER(cob_drift_fixed);
PE_RS_DECLARE_BOOL_MODIFIER(natural_sun_drop_disabled);
PE_RS_DECLARE_BOOL_MODIFIER(jack_explosions_disabled);
PE_RS_DECLARE_BOOL_MODIFIER(special_events_disabled);
PE_RS_DECLARE_BOOL_MODIFIER(zombie_spawn_stopped);
PE_RS_DECLARE_BOOL_MODIFIER(zombies_die_at_house);

#undef PE_RS_DECLARE_BOOL_MODIFIER

std::int32_t pe_rs_scene_kernel_pult_rule(const pe_rs_scene* scene) noexcept;
pe_rs_status pe_rs_scene_set_kernel_pult_rule(pe_rs_scene* scene, std::int32_t rule) noexcept;
std::int32_t pe_rs_scene_plant_damage_rule(const pe_rs_scene* scene) noexcept;
pe_rs_status pe_rs_scene_set_plant_damage_rule(pe_rs_scene* scene, std::int32_t rule) noexcept;
std::int32_t pe_rs_scene_maid_cheat(const pe_rs_scene* scene) noexcept;
pe_rs_status pe_rs_scene_set_maid_cheat(pe_rs_scene* scene, std::int32_t state) noexcept;
pe_rs_status pe_rs_scene_set_common_zombie_dance(pe_rs_scene* scene, std::int32_t state) noexcept;
pe_rs_status pe_rs_scene_set_dance_mode(pe_rs_scene* scene, std::uint8_t enabled) noexcept;
pe_rs_status pe_rs_world_select_plants(pe_rs_world* world, const std::int32_t* cards, std::uint32_t count, std::int32_t imitater) noexcept;

}
