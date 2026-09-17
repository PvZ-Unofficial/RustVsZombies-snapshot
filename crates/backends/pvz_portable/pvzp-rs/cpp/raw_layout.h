#pragma once

#include "Lawn/Coin.h"
#include "Lawn/GridItem.h"
#include "Lawn/Plant.h"
#include "Lawn/Projectile.h"
#include "Lawn/SeedPacket.h"
#include "Lawn/Zombie.h"
#include "PvzpLib/Reanimator.h"

#include <type_traits>

using pvzp_rs_plant = Plant;
using pvzp_rs_zombie = Zombie;
using pvzp_rs_projectile = Projectile;
using pvzp_rs_grid_item = GridItem;
using pvzp_rs_item = Coin;
using pvzp_rs_seed = SeedPacket;
using pvzp_rs_reanimation = Reanimation;
using pvzp_rs_rect = Rect;

static_assert(sizeof(SeedType) == sizeof(std::int32_t));
static_assert(sizeof(ZombieType) == sizeof(std::int32_t));
static_assert(sizeof(ProjectileType) == sizeof(std::int32_t));
static_assert(sizeof(GridItemType) == sizeof(std::int32_t));
static_assert(sizeof(CoinType) == sizeof(std::int32_t));
static_assert(std::is_standard_layout_v<GameObject>);
static_assert(std::is_standard_layout_v<GridItem>);
static_assert(std::is_standard_layout_v<Reanimation>);
