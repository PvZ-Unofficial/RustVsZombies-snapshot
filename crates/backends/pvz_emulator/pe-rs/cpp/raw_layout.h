#pragma once

#include "object/plant.h"
#include "object/projectile.h"
#include "object/scene.h"
#include "object/zombie.h"

#include <cstdint>
#include <type_traits>

// These are names for PE's real object types, not facade layouts.  Rust only
// receives pointers to these objects while the unique backend borrow is live.
using pe_rs_plant = pvz_emulator::object::plant;
using pe_rs_zombie = pvz_emulator::object::zombie;
using pe_rs_griditem = pvz_emulator::object::griditem;
using pe_rs_projectile = pvz_emulator::object::projectile;
using pe_rs_card = pvz_emulator::object::scene::card_data;
using pe_rs_grid_plant_status = pvz_emulator::object::grid_plant_status;
using pe_rs_spawn_data = pvz_emulator::object::scene::spawn_data;
using pe_rs_sun_data = pvz_emulator::object::scene::sun_data;
using pe_rs_ice_path_data = pvz_emulator::object::scene::ice_path_data;
using pe_rs_rect = pvz_emulator::object::rect;
using pe_rs_reanim = pvz_emulator::object::reanim;

static_assert(std::is_standard_layout_v<pe_rs_plant>);
static_assert(std::is_standard_layout_v<pe_rs_zombie>);
static_assert(std::is_standard_layout_v<pe_rs_griditem>);
static_assert(std::is_standard_layout_v<pe_rs_projectile>);
static_assert(std::is_standard_layout_v<pe_rs_card>);
static_assert(std::is_standard_layout_v<pe_rs_grid_plant_status>);
static_assert(std::is_standard_layout_v<pe_rs_spawn_data>);
static_assert(std::is_standard_layout_v<pe_rs_sun_data>);
static_assert(std::is_standard_layout_v<pe_rs_ice_path_data>);
static_assert(std::is_standard_layout_v<pe_rs_rect>);
static_assert(std::is_standard_layout_v<pe_rs_reanim>);
static_assert(sizeof(pvz_emulator::object::plant_type) == sizeof(std::int32_t));
static_assert(sizeof(pvz_emulator::object::zombie_type) == sizeof(std::int32_t));
