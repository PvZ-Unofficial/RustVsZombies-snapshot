#pragma once

#include "raw_layout.h"
#include "PvzpLib/Plugin.h"

#include <cstddef>
#include <cstdint>

#define PVZP_RS_EXPORT

extern "C" {

using pvzp_rs_battle_callbacks = PvzpPlugin::BattleCallbacks;
using pvzp_rs_layout_entry = PvzpPlugin::LayoutEntry;
std::uint8_t pvzp_rs_check_raw_layout(const pvzp_rs_layout_entry* entries, std::uint32_t count) noexcept;
std::uint8_t pvzp_rs_validate_abi() noexcept;
std::uint8_t pvzp_rs_register_update(PvzpPlugin::UpdateCallback update, PvzpPlugin::BoardDestroyingCallback destroy) noexcept;
std::uint8_t pvzp_rs_register_battle(const pvzp_rs_battle_callbacks* callbacks, std::uint32_t interest) noexcept;
void pvzp_rs_clear_battle() noexcept;
void pvzp_rs_request_stop() noexcept;

struct pvzp_rs_world;
struct pvzp_rs_plant_pool;
struct pvzp_rs_zombie_pool;
struct pvzp_rs_projectile_pool;
struct pvzp_rs_grid_item_pool;
struct pvzp_rs_item_pool;

enum pvzp_rs_status_code : std::int32_t {
	PVZP_RS_STATUS_OK = 0,
	PVZP_RS_STATUS_NULL_POINTER = 1,
	PVZP_RS_STATUS_INVALID_ARGUMENT = 2,
	PVZP_RS_STATUS_NOT_FOUND = 3,
	PVZP_RS_STATUS_UNSUPPORTED = 4,
	PVZP_RS_STATUS_STD_EXCEPTION = 100,
	PVZP_RS_STATUS_BAD_ALLOC = 101,
	PVZP_RS_STATUS_UNKNOWN_EXCEPTION = 102,
};

struct pvzp_rs_status {
	std::int32_t code;
	std::int32_t detail;
};

struct pvzp_rs_rect_i32 {
	std::int32_t x;
	std::int32_t y;
	std::int32_t width;
	std::int32_t height;
};





struct pvzp_rs_layout_fingerprint {
	std::uint32_t abi_version;
	std::uint32_t pointer_size;
	std::uint32_t game_object_size;
	std::uint32_t plant_size;
	std::uint32_t zombie_size;
	std::uint32_t projectile_size;
	std::uint32_t grid_item_size;
	std::uint32_t item_size;
	std::uint32_t seed_size;
	std::uint32_t reanimation_size;
	std::uint32_t plant_seed_type;
	std::uint32_t plant_health;
	std::uint32_t plant_state;
	std::uint32_t plant_dead;
	std::uint32_t zombie_type;
	std::uint32_t zombie_phase;
	std::uint32_t zombie_health;
	std::uint32_t zombie_dead;
	std::uint32_t projectile_type;
	std::uint32_t projectile_dead;
	std::uint32_t grid_item_type;
	std::uint32_t grid_item_dead;
	std::uint32_t item_type;
	std::uint32_t item_dead;
	std::uint32_t seed_refresh_counter;
	std::uint32_t seed_packet_type;
	std::uint32_t reanimation_time;
	std::uint32_t reanimation_rate;
};

#define PVZP_RS_API PVZP_RS_EXPORT pvzp_rs_status

PVZP_RS_API pvzp_rs_set_common_dance(std::int32_t dance) noexcept;
PVZP_RS_API pvzp_rs_set_dance_mode(std::uint8_t enabled) noexcept;
PVZP_RS_API pvzp_rs_set_cob_impact_delay(std::uint8_t enabled) noexcept;
PVZP_RS_API pvzp_rs_coins(std::uint32_t* value) noexcept;
PVZP_RS_API pvzp_rs_set_coins(std::uint32_t value) noexcept;
PVZP_RS_API pvzp_rs_mouse(std::int32_t operation, std::int32_t x, std::int32_t y, std::int32_t button) noexcept;
PVZP_RS_API pvzp_rs_sound(std::uint32_t id, std::uint8_t stop) noexcept;
PVZP_RS_API pvzp_rs_water_plant(std::int32_t row, std::int32_t col) noexcept;
PVZP_RS_API pvzp_rs_unlock_trophy() noexcept;
PVZP_RS_API pvzp_rs_unlock_hidden_modes() noexcept;

// Each object must be an occupied slot kept alive by its backend borrow.
PVZP_RS_EXPORT std::uint32_t pvzp_rs_plant_id(const pvzp_rs_plant* object) noexcept;
PVZP_RS_EXPORT std::uint32_t pvzp_rs_zombie_id(const pvzp_rs_zombie* object) noexcept;
PVZP_RS_EXPORT std::uint32_t pvzp_rs_grid_item_id(const pvzp_rs_grid_item* object) noexcept;
PVZP_RS_EXPORT std::uint32_t pvzp_rs_projectile_id(const pvzp_rs_projectile* object) noexcept;
PVZP_RS_EXPORT std::uint32_t pvzp_rs_item_id(const pvzp_rs_item* object) noexcept;

PVZP_RS_API pvzp_rs_layout(pvzp_rs_layout_fingerprint* out_layout) noexcept;
PVZP_RS_API pvzp_rs_current_world(pvzp_rs_world** out_world) noexcept;
// Requires live gLawnApp on the game thread. Board may be null.
PVZP_RS_EXPORT pvzp_rs_world* pvzp_rs_board_from_live_app() noexcept;
PVZP_RS_API pvzp_rs_game_ui(std::int32_t* out_game_ui) noexcept;
PVZP_RS_API pvzp_rs_seed_chooser_mouse_visible(std::uint8_t* out_visible) noexcept;
PVZP_RS_API pvzp_rs_seed_chooser_parent_present(std::uint8_t* out_present) noexcept;
PVZP_RS_API pvzp_rs_seed_chooser_widget_manager_present(std::uint8_t* out_present) noexcept;
PVZP_RS_API pvzp_rs_seed_chooser_modal_present(std::uint8_t* out_present) noexcept;
PVZP_RS_API pvzp_rs_seed_chooser_choose_state(std::int32_t* out_state) noexcept;
PVZP_RS_API pvzp_rs_seed_chooser_view_lawn_time(std::int32_t* out_time) noexcept;
PVZP_RS_API pvzp_rs_seed_chooser_seeds_in_flight(std::int32_t* out_count) noexcept;
PVZP_RS_API pvzp_rs_seed_chooser_cancel_view_lawn() noexcept;
PVZP_RS_API pvzp_rs_click_continue_dialog_if_present(std::uint8_t* out_clicked) noexcept;
PVZP_RS_API pvzp_rs_input_focused(std::uint8_t* out_focused) noexcept;
PVZP_RS_EXPORT std::int32_t pvzp_rs_world_scene(const pvzp_rs_world* world) noexcept;
PVZP_RS_EXPORT std::uint8_t pvzp_rs_world_paused(const pvzp_rs_world* world) noexcept;
PVZP_RS_API pvzp_rs_world_seed_choosing(const pvzp_rs_world* world, std::uint8_t* out_choosing) noexcept;
PVZP_RS_EXPORT std::uint32_t pvzp_rs_world_main_counter(const pvzp_rs_world* world) noexcept;
PVZP_RS_EXPORT std::uint8_t pvzp_rs_world_imitater_successor(const pvzp_rs_world* world, std::uint32_t placeholder_id, std::uint32_t* out_successor_id) noexcept;
PVZP_RS_API pvzp_rs_world_cursor_type(const pvzp_rs_world* world, std::int32_t* out_cursor_type) noexcept;
PVZP_RS_EXPORT std::int32_t pvzp_rs_world_current_wave(const pvzp_rs_world* world) noexcept;
PVZP_RS_EXPORT std::int32_t pvzp_rs_world_sun(const pvzp_rs_world* world) noexcept;
PVZP_RS_EXPORT std::uint8_t pvzp_rs_world_level_complete(const pvzp_rs_world* world) noexcept;
PVZP_RS_EXPORT std::int32_t pvzp_rs_world_next_survival_stage_counter(const pvzp_rs_world* world) noexcept;
PVZP_RS_API pvzp_rs_world_set_sun(pvzp_rs_world* world, std::uint32_t sun) noexcept;
PVZP_RS_EXPORT std::int32_t pvzp_rs_world_natural_sun_generated(const pvzp_rs_world* world) noexcept;
PVZP_RS_EXPORT std::int32_t pvzp_rs_world_natural_sun_countdown(const pvzp_rs_world* world) noexcept;
PVZP_RS_API pvzp_rs_world_set_natural_sun_generated(pvzp_rs_world* world, std::int32_t count) noexcept;
PVZP_RS_API pvzp_rs_world_set_natural_sun_countdown(pvzp_rs_world* world, std::int32_t countdown) noexcept;
PVZP_RS_API pvzp_rs_app_counter(std::uint32_t* out_counter) noexcept;
PVZP_RS_API pvzp_rs_set_app_counter(std::uint32_t counter) noexcept;
PVZP_RS_API pvzp_rs_world_ice_path_x(const pvzp_rs_world* world, std::uint32_t row, std::int32_t* out_value) noexcept;
PVZP_RS_API pvzp_rs_world_ice_path_countdown(const pvzp_rs_world* world, std::uint32_t row, std::uint32_t* out_value) noexcept;
PVZP_RS_API pvzp_rs_world_row_pick_weight_bits(const pvzp_rs_world* world, std::uint32_t row, std::uint32_t* out_value) noexcept;
PVZP_RS_API pvzp_rs_world_row_pick_last_picked_bits(const pvzp_rs_world* world, std::uint32_t row, std::uint32_t* out_value) noexcept;
PVZP_RS_API pvzp_rs_world_row_pick_second_last_picked_bits(const pvzp_rs_world* world, std::uint32_t row, std::uint32_t* out_value) noexcept;
PVZP_RS_API pvzp_rs_world_spawn_allowed(const pvzp_rs_world* world, std::uint32_t zombie_type, std::uint8_t* out_allowed) noexcept;
PVZP_RS_API pvzp_rs_world_spawn_entry(const pvzp_rs_world* world, std::uint32_t wave, std::uint32_t slot, std::int32_t* out_zombie_type) noexcept;
PVZP_RS_EXPORT std::int32_t pvzp_rs_world_total_waves(const pvzp_rs_world* world) noexcept;
PVZP_RS_EXPORT std::int32_t pvzp_rs_world_refresh_countdown(const pvzp_rs_world* world) noexcept;
PVZP_RS_EXPORT std::int32_t pvzp_rs_world_initial_countdown(const pvzp_rs_world* world) noexcept;
PVZP_RS_EXPORT std::int32_t pvzp_rs_world_huge_wave_countdown(const pvzp_rs_world* world) noexcept;
PVZP_RS_EXPORT std::int32_t pvzp_rs_world_level_end_countdown(const pvzp_rs_world* world) noexcept;
PVZP_RS_API pvzp_rs_world_commit_wave_refresh(pvzp_rs_world* world, std::int32_t expected_wave, std::int32_t initial_countdown) noexcept;
PVZP_RS_API pvzp_rs_world_zombie_health_wave_start(const pvzp_rs_world* world, std::int32_t* out_health) noexcept;
PVZP_RS_API pvzp_rs_world_total_zombie_health_in_wave(pvzp_rs_world* world, std::int32_t wave, std::int32_t* out_health) noexcept;
PVZP_RS_API pvzp_rs_world_set_scene(pvzp_rs_world* world, std::int32_t scene) noexcept;
PVZP_RS_API pvzp_rs_world_clear_lawn_mowers(pvzp_rs_world* world) noexcept;
PVZP_RS_API pvzp_rs_world_set_spawn_allowed(pvzp_rs_world* world, std::int32_t zombie_type, std::uint8_t allowed) noexcept;
PVZP_RS_API pvzp_rs_world_set_spawn_entry(pvzp_rs_world* world, std::uint32_t wave, std::uint32_t slot, std::int32_t zombie_type) noexcept;
PVZP_RS_API pvzp_rs_world_pick_spawn_list(pvzp_rs_world* world) noexcept;
PVZP_RS_API pvzp_rs_start_battle() noexcept;
PVZP_RS_API pvzp_rs_enter_endless(std::int32_t scene) noexcept;
PVZP_RS_API pvzp_rs_back_to_main_menu() noexcept;
PVZP_RS_API pvzp_rs_world_reset(
	pvzp_rs_world* world, std::uint32_t completed_rounds, std::uint32_t seed,
	std::uint32_t initial_sun, std::uint8_t ready_cooldowns) noexcept;
PVZP_RS_API pvzp_rs_set_game_speed(float speed) noexcept;
PVZP_RS_API pvzp_rs_restore_game_speed() noexcept;
PVZP_RS_API pvzp_rs_set_fast_forward(std::uint8_t enabled, std::int32_t performance, std::uint8_t suppress_window) noexcept;
PVZP_RS_EXPORT std::uint8_t pvzp_rs_fast_forward_active() noexcept;
PVZP_RS_API pvzp_rs_request_seed_chooser_fast_forward(std::uint32_t max_frames) noexcept;
PVZP_RS_API pvzp_rs_set_advanced_pause(
	std::uint8_t enabled, std::uint8_t draw_mask, std::uint32_t rgba,
	std::uint8_t play_sound, std::uint8_t refresh_cursor_preview) noexcept;
PVZP_RS_EXPORT std::uint8_t pvzp_rs_advanced_pause_active() noexcept;
PVZP_RS_API pvzp_rs_set_random_mode(std::int32_t mode, std::uint32_t value) noexcept;
PVZP_RS_API pvzp_rs_set_wave_spawn_random_seed(std::uint8_t enabled, std::uint32_t seed) noexcept;
PVZP_RS_API pvzp_rs_random_seed(std::int32_t stream, std::uint32_t* out_value) noexcept;
PVZP_RS_EXPORT std::uint8_t pvzp_rs_random_locked(bool level_stream) noexcept;
PVZP_RS_EXPORT std::uint32_t pvzp_rs_random_fixed(bool level_stream) noexcept;
PVZP_RS_API pvzp_rs_set_sun_production_mode(std::int32_t mode) noexcept;
PVZP_RS_API pvzp_rs_world_grid_to_pixel_x(const pvzp_rs_world* world, std::int32_t grid_x, std::int32_t grid_y, std::int32_t* out_x) noexcept;
PVZP_RS_API pvzp_rs_world_grid_to_pixel_y(const pvzp_rs_world* world, std::int32_t grid_x, std::int32_t grid_y, std::int32_t* out_y) noexcept;
PVZP_RS_API pvzp_rs_world_pos_y_based_on_row(const pvzp_rs_world* world, float pos_x, std::int32_t row, float* out_y) noexcept;
PVZP_RS_API pvzp_rs_world_is_pool_square(const pvzp_rs_world* world, std::int32_t grid_x, std::int32_t grid_y, std::uint8_t* out_is_pool) noexcept;
PVZP_RS_API pvzp_rs_world_row_can_have_zombies(const pvzp_rs_world* world, std::int32_t row, std::uint8_t* out_allowed) noexcept;
PVZP_RS_API pvzp_rs_world_seed_count(const pvzp_rs_world* world, std::uint32_t* out_count) noexcept;
PVZP_RS_API pvzp_rs_world_seed_at(pvzp_rs_world* world, std::uint32_t index, pvzp_rs_seed** out_seed) noexcept;
PVZP_RS_API pvzp_rs_seed_can_pick_up(pvzp_rs_seed* seed, std::uint8_t* out_can_pick_up) noexcept;
PVZP_RS_API pvzp_rs_seed_was_planted(pvzp_rs_seed* seed) noexcept;
PVZP_RS_API pvzp_rs_selected_card_count(std::uint32_t* out_count) noexcept;
PVZP_RS_API pvzp_rs_selected_card(std::uint32_t index, std::int32_t* out_packet_type, std::int32_t* out_imitater_type) noexcept;
PVZP_RS_API pvzp_rs_select_card(std::int32_t packet_type, std::int32_t imitater_type) noexcept;
PVZP_RS_API pvzp_rs_world_current_plant_cost(const pvzp_rs_world* world, std::int32_t packet_type, std::int32_t imitater_type, std::int32_t* out_cost) noexcept;
PVZP_RS_API pvzp_rs_world_can_take_sun(const pvzp_rs_world* world, std::int32_t amount, std::uint8_t* out_allowed) noexcept;
PVZP_RS_API pvzp_rs_world_take_sun(pvzp_rs_world* world, std::int32_t amount, std::uint8_t* out_taken) noexcept;
PVZP_RS_API pvzp_rs_world_can_plant_at(const pvzp_rs_world* world, std::int32_t packet_type, std::int32_t grid_x, std::int32_t grid_y, std::int32_t* out_reason) noexcept;
PVZP_RS_API pvzp_rs_world_new_plant(pvzp_rs_world* world, std::int32_t grid_x, std::int32_t grid_y, std::int32_t packet_type, std::int32_t imitater_type, pvzp_rs_plant** out_plant) noexcept;
PVZP_RS_API pvzp_rs_world_add_plant(pvzp_rs_world* world, std::int32_t grid_x, std::int32_t grid_y, std::int32_t packet_type, std::int32_t imitater_type, pvzp_rs_plant** out_plant) noexcept;
PVZP_RS_API pvzp_rs_plant_die(pvzp_rs_plant* plant) noexcept;
PVZP_RS_API pvzp_rs_plant_imitater_morph(pvzp_rs_plant* plant, pvzp_rs_plant** out_plant) noexcept;
PVZP_RS_API pvzp_rs_plant_set_sleeping(pvzp_rs_plant* plant, std::uint8_t asleep) noexcept;
PVZP_RS_API pvzp_rs_plant_play_idle(pvzp_rs_plant* plant, float fps) noexcept;
PVZP_RS_API pvzp_rs_plant_update_reanim_color(pvzp_rs_plant* plant) noexcept;
PVZP_RS_API pvzp_rs_plant_fire_cob(pvzp_rs_plant* plant, std::int32_t target_x, std::int32_t target_y) noexcept;
PVZP_RS_EXPORT void pvzp_rs_plant_hit_box(pvzp_rs_plant* plant, pvzp_rs_rect_i32* outRect) noexcept;
PVZP_RS_EXPORT void pvzp_rs_plant_attack_rect(pvzp_rs_plant* plant, std::int32_t weapon, pvzp_rs_rect_i32* outRect) noexcept;
PVZP_RS_EXPORT std::uint32_t pvzp_rs_plant_damage_range_flags(pvzp_rs_plant* plant, std::int32_t weapon) noexcept;
PVZP_RS_API pvzp_rs_plant_reanimation(pvzp_rs_plant* plant, pvzp_rs_reanimation** out_reanimation) noexcept;
PVZP_RS_API pvzp_rs_zombie_pos_y_based_on_row(pvzp_rs_zombie* zombie, std::int32_t row, float* out_y) noexcept;
PVZP_RS_API pvzp_rs_zombie_hit_box(pvzp_rs_zombie* zombie, pvzp_rs_rect_i32* out_rect) noexcept;
PVZP_RS_API pvzp_rs_zombie_effected_by_damage(pvzp_rs_zombie* zombie, std::uint32_t flags, std::uint8_t* out_effected) noexcept;
PVZP_RS_API pvzp_rs_zombie_can_target_plant(pvzp_rs_zombie* zombie, pvzp_rs_plant* plant, std::int32_t attack_type, std::uint8_t* out_can_target) noexcept;
PVZP_RS_EXPORT std::uint8_t pvzp_rs_zombie_is_dead_or_dying(pvzp_rs_zombie* zombie) noexcept;
PVZP_RS_API pvzp_rs_zombie_reanimation(pvzp_rs_zombie* zombie, pvzp_rs_reanimation** out_reanimation) noexcept;
PVZP_RS_API pvzp_rs_world_add_zombie_in_row(pvzp_rs_world* world, std::int32_t zombie_type, std::int32_t row, std::int32_t from_wave, pvzp_rs_zombie** out_zombie) noexcept;
PVZP_RS_API pvzp_rs_world_place_zombie(pvzp_rs_world* world, std::int32_t zombie_type, std::int32_t grid_x, std::int32_t grid_y, pvzp_rs_zombie** out_zombie) noexcept;
PVZP_RS_API pvzp_rs_zombie_die_no_loot(pvzp_rs_zombie* zombie) noexcept;
PVZP_RS_API pvzp_rs_zombie_die_with_loot(pvzp_rs_zombie* zombie) noexcept;
PVZP_RS_EXPORT void pvzp_rs_projectile_hit_box(pvzp_rs_projectile* projectile, pvzp_rs_rect_i32* outRect) noexcept;
PVZP_RS_API pvzp_rs_world_add_ladder(pvzp_rs_world* world, std::int32_t grid_x, std::int32_t grid_y, pvzp_rs_grid_item** out_item) noexcept;
PVZP_RS_API pvzp_rs_world_add_crater(pvzp_rs_world* world, std::int32_t grid_x, std::int32_t grid_y, pvzp_rs_grid_item** out_item) noexcept;
PVZP_RS_API pvzp_rs_world_add_gravestone(pvzp_rs_world* world, std::int32_t grid_x, std::int32_t grid_y, pvzp_rs_grid_item** out_item) noexcept;
PVZP_RS_API pvzp_rs_grid_item_die(pvzp_rs_grid_item* item) noexcept;
PVZP_RS_API pvzp_rs_item_collect(pvzp_rs_item* item, std::uint8_t play_sound) noexcept;

#define PVZP_RS_DECLARE_BOOL_RULE(name) \
	PVZP_RS_EXPORT std::uint8_t pvzp_rs_##name() noexcept; \
	PVZP_RS_API pvzp_rs_set_##name(std::uint8_t enabled) noexcept

PVZP_RS_DECLARE_BOOL_RULE(seed_recharge_ignored);
PVZP_RS_DECLARE_BOOL_RULE(sun_cost_ignored);
PVZP_RS_DECLARE_BOOL_RULE(fog_revealed);
PVZP_RS_DECLARE_BOOL_RULE(vase_contents_visible);
PVZP_RS_DECLARE_BOOL_RULE(instant_ice_and_ash_effects);
PVZP_RS_DECLARE_BOOL_RULE(mushrooms_awake);
PVZP_RS_DECLARE_BOOL_RULE(cob_fixed_delay);
PVZP_RS_DECLARE_BOOL_RULE(cob_recharge_shortened);
PVZP_RS_DECLARE_BOOL_RULE(cob_drift_fixed);
PVZP_RS_DECLARE_BOOL_RULE(item_drop_disabled);
PVZP_RS_DECLARE_BOOL_RULE(natural_sun_drop_disabled);
PVZP_RS_DECLARE_BOOL_RULE(jack_explosions_disabled);
PVZP_RS_DECLARE_BOOL_RULE(pepper_explosions_disabled);
PVZP_RS_DECLARE_BOOL_RULE(special_events_disabled);
PVZP_RS_DECLARE_BOOL_RULE(zombie_spawn_stopped);
PVZP_RS_DECLARE_BOOL_RULE(zombies_die_at_house);
PVZP_RS_DECLARE_BOOL_RULE(planting_restrictions_ignored);
PVZP_RS_DECLARE_BOOL_RULE(profile_readonly);
PVZP_RS_DECLARE_BOOL_RULE(normal_auto_collect_enabled);

#undef PVZP_RS_DECLARE_BOOL_RULE

PVZP_RS_API pvzp_rs_easy_planting_cheat(std::uint8_t* out_enabled) noexcept;
PVZP_RS_API pvzp_rs_set_easy_planting_cheat(std::uint8_t enabled) noexcept;
PVZP_RS_API pvzp_rs_kernel_pult_projectile_rule(std::int32_t* out_rule) noexcept;
PVZP_RS_API pvzp_rs_set_kernel_pult_projectile_rule(std::int32_t rule) noexcept;
PVZP_RS_API pvzp_rs_plant_damage_rule(std::int32_t* out_rule) noexcept;
PVZP_RS_API pvzp_rs_set_plant_damage_rule(std::int32_t rule) noexcept;
PVZP_RS_API pvzp_rs_maid_cheat(std::int32_t* out_cheat) noexcept;
PVZP_RS_API pvzp_rs_set_maid_cheat(std::int32_t cheat) noexcept;
PVZP_RS_EXPORT pvzp_rs_plant_pool* pvzp_rs_world_plant_pool(pvzp_rs_world* world) noexcept;
PVZP_RS_EXPORT pvzp_rs_zombie_pool* pvzp_rs_world_zombie_pool(pvzp_rs_world* world) noexcept;
PVZP_RS_EXPORT pvzp_rs_projectile_pool* pvzp_rs_world_projectile_pool(pvzp_rs_world* world) noexcept;
PVZP_RS_EXPORT pvzp_rs_grid_item_pool* pvzp_rs_world_grid_item_pool(pvzp_rs_world* world) noexcept;
PVZP_RS_EXPORT pvzp_rs_item_pool* pvzp_rs_world_item_pool(pvzp_rs_world* world) noexcept;

#define PVZP_RS_DECLARE_POOL_API(prefix, pool_type, object_type) \
	PVZP_RS_EXPORT std::uint32_t pvzp_rs_##prefix##_pool_max_used_count(const pool_type* pool) noexcept; \
	PVZP_RS_EXPORT std::uint32_t pvzp_rs_##prefix##_pool_active_count(const pool_type* pool) noexcept; \
	PVZP_RS_EXPORT std::uint32_t pvzp_rs_##prefix##_pool_capacity(const pool_type* pool) noexcept; \
	PVZP_RS_EXPORT std::uint32_t pvzp_rs_##prefix##_pool_free_list_head(const pool_type* pool) noexcept; \
	PVZP_RS_EXPORT std::uint32_t pvzp_rs_##prefix##_pool_next_id_key(const pool_type* pool) noexcept; \
	PVZP_RS_API pvzp_rs_##prefix##_pool_slot(const pool_type* pool, std::uint32_t index, std::uint8_t* out_active, std::uint32_t* out_id, std::uint32_t* out_next_free) noexcept; \
	PVZP_RS_API pvzp_rs_##prefix##_pool_get(pool_type* pool, std::int32_t index, object_type** out_object) noexcept; \
	PVZP_RS_EXPORT object_type* pvzp_rs_##prefix##_pool_try_to_get(pool_type* pool, std::uint32_t id) noexcept; \
	PVZP_RS_API pvzp_rs_##prefix##_pool_get_id(const pool_type* pool, const object_type* object, std::uint32_t* out_id) noexcept

PVZP_RS_DECLARE_POOL_API(plant, pvzp_rs_plant_pool, pvzp_rs_plant);
PVZP_RS_DECLARE_POOL_API(zombie, pvzp_rs_zombie_pool, pvzp_rs_zombie);
PVZP_RS_DECLARE_POOL_API(projectile, pvzp_rs_projectile_pool, pvzp_rs_projectile);
PVZP_RS_DECLARE_POOL_API(grid_item, pvzp_rs_grid_item_pool, pvzp_rs_grid_item);
PVZP_RS_DECLARE_POOL_API(item, pvzp_rs_item_pool, pvzp_rs_item);

#undef PVZP_RS_DECLARE_POOL_API

PVZP_RS_EXPORT void rsvz_pvzp_log(const char* message, std::size_t length);
PVZP_RS_EXPORT void rsvz_pvzp_restore_all();

#undef PVZP_RS_API

}

#undef PVZP_RS_EXPORT
