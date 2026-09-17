extern "C" pe_rs_status pe_rs_current_plant_cost(
    const pe_rs_world* world,
    std::int32_t seed_value,
    std::int32_t imitater_value,
    std::int32_t* out_cost) noexcept {
    if (world == nullptr || out_cost == nullptr) {
        return null_pointer();
    }
    const auto seed = static_cast<plant_type>(seed_value);
    const auto imitater = static_cast<plant_type>(imitater_value);
    const bool valid_combination = valid_plant_type(seed_value) &&
        (seed == plant_type::imitater ?
            (valid_imitater_type(imitater_value) && imitater != plant_type::none) :
            imitater == plant_type::none);
    if (!valid_combination) {
        return invalid(seed_value);
    }
    return guarded([&] {
        *out_cost = world->inner.plant_factory.get_current_plant_cost(seed, imitater);
    });
}

extern "C" pe_rs_status pe_rs_can_take_sun_money(
    const pe_rs_world* world,
    std::int32_t amount,
    std::uint8_t* out_allowed) noexcept {
    if (world == nullptr || out_allowed == nullptr) {
        return null_pointer();
    }
    if (amount < 0) {
        return invalid(amount);
    }
    *out_allowed = static_cast<std::uint8_t>(world->inner.sun.can_take_sun_money(amount));
    return ok();
}

extern "C" pe_rs_status pe_rs_take_sun_money(
    pe_rs_world* world,
    std::int32_t amount,
    std::uint8_t* out_taken) noexcept {
    if (world == nullptr || out_taken == nullptr) {
        return null_pointer();
    }
    if (amount < 0) {
        return invalid(amount);
    }
    *out_taken = static_cast<std::uint8_t>(world->inner.sun.take_sun_money(amount));
    return ok();
}

extern "C" pe_rs_status pe_rs_seed_can_pick_up(
    const pe_rs_world* world,
    const pe_rs_card* card,
    std::uint8_t* out_can_pick_up) noexcept {
    if (world == nullptr || card == nullptr || out_can_pick_up == nullptr) {
        return null_pointer();
    }
    if (!card_belongs_to(world->inner, card)) {
        return invalid(0);
    }
    *out_can_pick_up = static_cast<std::uint8_t>(world->inner.plant_factory.can_pick_up(*card));
    return ok();
}

extern "C" pe_rs_status pe_rs_seed_was_planted(pe_rs_world* world, pe_rs_card* card) noexcept {
    if (world == nullptr || card == nullptr) {
        return null_pointer();
    }
    if (!card_belongs_to(world->inner, card)) {
        return invalid(0);
    }
    world->inner.plant_factory.was_planted(*card);
    return ok();
}

extern "C" pe_rs_status pe_rs_can_plant_at(
    const pe_rs_world* world,
    std::int32_t grid_x,
    std::int32_t grid_y,
    std::int32_t planting_value,
    std::int32_t* out_reason) noexcept {
    if (world == nullptr || out_reason == nullptr) {
        return null_pointer();
    }
    if (!valid_plant_type(planting_value)) {
        return invalid(planting_value);
    }
    *out_reason = static_cast<std::int32_t>(world->inner.plant_factory.can_plant_at(
        grid_x,
        grid_y,
        static_cast<plant_type>(planting_value)));
    return ok();
}

template <typename Create>
pe_rs_status create_plant(
    pe_rs_world* world,
    std::int32_t grid_x,
    std::int32_t grid_y,
    std::int32_t seed_value,
    std::int32_t imitater_value,
    pe_rs_plant** out_plant,
    Create&& create) noexcept {
    if (world == nullptr || out_plant == nullptr) {
        return null_pointer();
    }
    *out_plant = nullptr;
    if (!valid_plant_type(seed_value) || !valid_imitater_type(imitater_value)) {
        return invalid(seed_value);
    }
    return guarded([&] {
        *out_plant = create(
            grid_x,
            grid_y,
            static_cast<plant_type>(seed_value),
            static_cast<plant_type>(imitater_value));
    });
}

extern "C" pe_rs_status pe_rs_new_plant(
    pe_rs_world* world,
    std::int32_t grid_x,
    std::int32_t grid_y,
    std::int32_t seed_value,
    std::int32_t imitater_value,
    pe_rs_plant** out_plant) noexcept {
    return create_plant(world, grid_x, grid_y, seed_value, imitater_value, out_plant,
        [&](auto x, auto y, auto seed, auto imitater) {
            return world->inner.plant_factory.new_plant(x, y, seed, imitater);
        });
}

extern "C" pe_rs_status pe_rs_add_plant(
    pe_rs_world* world,
    std::int32_t grid_x,
    std::int32_t grid_y,
    std::int32_t seed_value,
    std::int32_t imitater_value,
    pe_rs_plant** out_plant) noexcept {
    return create_plant(world, grid_x, grid_y, seed_value, imitater_value, out_plant,
        [&](auto x, auto y, auto seed, auto imitater) {
            return world->inner.plant_factory.add_plant(x, y, seed, imitater);
        });
}

extern "C" pe_rs_status pe_rs_plant_die(pe_rs_world* world, pe_rs_plant* plant) noexcept {
    if (world == nullptr || plant == nullptr) {
        return null_pointer();
    }
    if (world->inner.scene.plants.get_id(plant) == 0) {
        return not_found();
    }
    world->inner.plant_factory.destroy(*plant);
    return ok();
}

extern "C" pe_rs_status pe_rs_plant_imitater_morph(pe_rs_world* world, pe_rs_plant* plant, pe_rs_plant** out_plant) noexcept {
    if (!world || !plant || !out_plant) return null_pointer();
    *out_plant = nullptr;
    if (world->inner.scene.plants.get_id(plant) == 0) return not_found();
    return guarded([&] {
        pvz_emulator::system::plant_imitater imitater(world->inner.scene);
        *out_plant = &imitater.morph(*plant);
    });
}

extern "C" pe_rs_status pe_rs_plant_set_sleep(pe_rs_plant* plant, std::uint8_t asleep) noexcept {
    if (plant == nullptr) {
        return null_pointer();
    }
    plant->set_sleep(asleep != 0);
    return ok();
}

extern "C" pe_rs_status pe_rs_plant_play_idle(pe_rs_plant* plant, float fps) noexcept {
    if (plant == nullptr) {
        return null_pointer();
    }
    plant->set_reanim(plant_reanim_name::anim_idle, reanim_type::repeat, fps);
    return ok();
}

extern "C" pe_rs_status pe_rs_plant_fire_cob(
    pe_rs_world* world,
    pe_rs_plant* plant,
    std::int32_t target_x,
    std::int32_t target_y,
    std::uint8_t* out_fired) noexcept {
    if (world == nullptr || plant == nullptr || out_fired == nullptr) {
        return null_pointer();
    }
    if (world->inner.scene.plants.get_id(plant) == 0) {
        return not_found();
    }
    *out_fired = static_cast<std::uint8_t>(
        system::plant_cob_cannon(world->inner.scene).launch(*plant, target_x, target_y));
    return ok();
}

extern "C" void pe_rs_plant_hit_box(pe_rs_plant* plant, pe_rs_rect* out_rect) noexcept {
    // Live input and writable output are guaranteed by the Rust borrow; weapon is validated.
    plant->get_hit_box(*out_rect);
}

extern "C" void pe_rs_plant_attack_rect(
    const pe_rs_plant* plant,
    std::int32_t weapon,
    pe_rs_rect* out_rect) noexcept {
    // Live input and writable output are guaranteed by the Rust borrow; weapon is validated.
    plant->get_plant_attack_rect(to_weapon(weapon), *out_rect);
}

extern "C" std::int32_t pe_rs_plant_damage_range_flags(const pe_rs_plant* plant, std::int32_t weapon) noexcept
{
    // Caller provides a live plant and a validated primary/secondary weapon.
    return plant->get_damage_range_flags(to_weapon(weapon));
}

extern "C" pe_rs_status pe_rs_add_zombie_in_row(
    pe_rs_world* world,
    std::int32_t zombie_value,
    std::int32_t row,
    std::int32_t from_wave,
    pe_rs_zombie** out_zombie) noexcept {
    if (world == nullptr || out_zombie == nullptr) {
        return null_pointer();
    }
    *out_zombie = nullptr;
    if (!valid_zombie_type(zombie_value)) {
        return invalid(zombie_value);
    }
    return guarded([&] {
        *out_zombie = world->inner.zombie_factory.add_zombie_in_row(
            static_cast<zombie_type>(zombie_value), row, from_wave);
    });
}

extern "C" pe_rs_status pe_rs_place_zombie(
    pe_rs_world* world,
    std::int32_t zombie_value,
    std::int32_t grid_x,
    std::int32_t grid_y,
    pe_rs_zombie** out_zombie) noexcept {
    if (world == nullptr || out_zombie == nullptr) {
        return null_pointer();
    }
    *out_zombie = nullptr;
    if (!valid_zombie_type(zombie_value)) {
        return invalid(zombie_value);
    }
    return guarded([&] {
        *out_zombie = world->inner.zombie_factory.place_zombie(
            static_cast<zombie_type>(zombie_value), grid_x, grid_y);
    });
}

extern "C" pe_rs_status pe_rs_zombie_die_no_loot(pe_rs_world* world, pe_rs_zombie* zombie) noexcept {
    if (world == nullptr || zombie == nullptr) {
        return null_pointer();
    }
    if (world->inner.scene.zombies.get_id(zombie) == 0) {
        return not_found();
    }
    world->inner.zombie_factory.destroy(*zombie);
    return ok();
}

extern "C" pe_rs_status pe_rs_zombie_can_be_attacked(
    const pe_rs_world* world,
    const pe_rs_zombie* zombie,
    std::uint8_t flags,
    std::uint8_t* out_can_be_attacked) noexcept {
    if (world == nullptr || zombie == nullptr || out_can_be_attacked == nullptr) {
        return null_pointer();
    }
    if (world->inner.scene.zombies.get_id(zombie) == 0) {
        return not_found();
    }
    *out_can_be_attacked = static_cast<std::uint8_t>(
        system::damage(const_cast<scene&>(world->inner.scene)).can_be_attacked(*zombie, flags));
    return ok();
}

extern "C" pe_rs_status pe_rs_zombie_can_attack_plant(
    pe_rs_world* world,
    pe_rs_zombie* zombie,
    pe_rs_plant* plant,
    std::int32_t attack_type,
    std::uint8_t* out_can_attack) noexcept {
    if (world == nullptr || zombie == nullptr || plant == nullptr || out_can_attack == nullptr) {
        return null_pointer();
    }
    if (world->inner.scene.zombies.get_id(zombie) == 0 ||
        world->inner.scene.plants.get_id(plant) == 0) {
        return not_found();
    }
    if (attack_type < 0 || attack_type > 3) {
        return invalid(attack_type);
    }
    *out_can_attack = static_cast<std::uint8_t>(
        system::zombie_base(world->inner.scene).can_attack_plant(
            *zombie, *plant, static_cast<system::zombie_attack_type>(attack_type)));
    return ok();
}

extern "C" void pe_rs_zombie_hit_box(
    const pe_rs_zombie* zombie,
    pe_rs_rect* out_rect) noexcept {
    // Live input and writable output are guaranteed by the Rust borrow; weapon is validated.
    zombie->get_hit_box(*out_rect);
}

extern "C" void pe_rs_projectile_attack_box(
    const pe_rs_projectile* projectile,
    pe_rs_rect* out_rect) noexcept {
    // Live input and writable output are guaranteed by the Rust borrow; weapon is validated.
    projectile->get_attack_box(*out_rect);
}

extern "C" pe_rs_status pe_rs_add_ladder(
    pe_rs_world* world,
    std::int32_t grid_x,
    std::int32_t grid_y,
    pe_rs_griditem** out_item) noexcept {
    if (world == nullptr || out_item == nullptr) {
        return null_pointer();
    }
    *out_item = nullptr;
    return guarded([&] { *out_item = world->inner.griditem_factory.add_ladder(grid_x, grid_y); });
}

extern "C" pe_rs_status pe_rs_add_crater(
    pe_rs_world* world,
    std::int32_t grid_x,
    std::int32_t grid_y,
    pe_rs_griditem** out_item) noexcept {
    if (world == nullptr || out_item == nullptr) {
        return null_pointer();
    }
    *out_item = nullptr;
    return guarded([&] { *out_item = world->inner.griditem_factory.add_crater(grid_x, grid_y); });
}

extern "C" pe_rs_status pe_rs_add_gravestone(
    pe_rs_world* world,
    std::int32_t grid_x,
    std::int32_t grid_y,
    pe_rs_griditem** out_item) noexcept {
    if (world == nullptr || out_item == nullptr) {
        return null_pointer();
    }
    *out_item = nullptr;
    return guarded([&] { *out_item = world->inner.griditem_factory.add_grave_stone(grid_x, grid_y); });
}

extern "C" pe_rs_status pe_rs_griditem_die(pe_rs_world* world, pe_rs_griditem* item) noexcept {
    if (world == nullptr || item == nullptr) {
        return null_pointer();
    }
    if (world->inner.scene.griditems.get_id(item) == 0) {
        return not_found();
    }
    world->inner.griditem_factory.destroy(*item);
    return ok();
}
