extern "C" pvzp_rs_status pvzp_rs_world_seed_count(const pvzp_rs_world* world, std::uint32_t* outCount) noexcept
{
	return Guard([&] {
		if (!world || !outCount)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		const auto* bank = Native(world)->mSeedBank.get();
		if (!bank)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		if (bank->mNumPackets < 0 || bank->mNumPackets > SEEDBANK_MAX)
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, bank->mNumPackets);
		*outCount = static_cast<std::uint32_t>(bank->mNumPackets);
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_world_seed_at(pvzp_rs_world* world, std::uint32_t index, pvzp_rs_seed** outSeed) noexcept
{
	return Guard([&] {
		if (!world || !outSeed)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		auto* bank = Native(world)->mSeedBank.get();
		if (!bank)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		if (index >= static_cast<std::uint32_t>(bank->mNumPackets) || index >= SEEDBANK_MAX)
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, static_cast<std::int32_t>(index));
		*outSeed = &bank->mSeedPackets[index];
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_seed_can_pick_up(pvzp_rs_seed* seed, std::uint8_t* outCanPickUp) noexcept
{
	return Guard([&] {
		if (!seed || !outCanPickUp)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		*outCanPickUp = static_cast<std::uint8_t>(seed->CanPickUp());
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_seed_was_planted(pvzp_rs_seed* seed) noexcept
{
	return Guard([&] {
		if (!seed)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		seed->Deactivate();
		seed->WasPlanted();
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_selected_card_count(std::uint32_t* outCount) noexcept
{
	return Guard([&] {
		if (!outCount)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (!gLawnApp)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		if (gLawnApp->mGameScene == GameScenes::SCENE_LEVEL_INTRO && gLawnApp->mSeedChooserScreen)
		{
			const int count = gLawnApp->mSeedChooserScreen->mSeedsInBank;
			if (count < 0 || count > SEEDBANK_MAX)
				return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, count);
			*outCount = static_cast<std::uint32_t>(count);
			return Ok();
		}
		if (gLawnApp->mGameScene == GameScenes::SCENE_PLAYING && gLawnApp->mBoard && gLawnApp->mBoard->mSeedBank)
		{
			const int count = gLawnApp->mBoard->mSeedBank->mNumPackets;
			if (count < 0 || count > SEEDBANK_MAX)
				return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, count);
			*outCount = static_cast<std::uint32_t>(count);
			return Ok();
		}
		return Fail(PVZP_RS_STATUS_NOT_FOUND);
	});
}

extern "C" pvzp_rs_status pvzp_rs_selected_card(std::uint32_t index, std::int32_t* outPacketType, std::int32_t* outImitaterType) noexcept
{
	return Guard([&] {
		if (!outPacketType || !outImitaterType)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (!gLawnApp)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		if (gLawnApp->mGameScene == GameScenes::SCENE_LEVEL_INTRO && gLawnApp->mSeedChooserScreen)
		{
			auto* chooser = gLawnApp->mSeedChooserScreen;
			if (index >= static_cast<std::uint32_t>(chooser->mSeedsInBank))
				return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, static_cast<std::int32_t>(index));
			const SeedType packet = chooser->FindSeedInBank(static_cast<int>(index));
			if (packet < SeedType::SEED_PEASHOOTER || packet >= SeedType::NUM_SEED_TYPES)
				return Fail(PVZP_RS_STATUS_NOT_FOUND);
			const auto& chosen = chooser->mChosenSeeds[static_cast<std::size_t>(packet)];
			*outPacketType = static_cast<std::int32_t>(packet);
			*outImitaterType = static_cast<std::int32_t>(chosen.mImitaterType);
			return Ok();
		}
		if (gLawnApp->mGameScene == GameScenes::SCENE_PLAYING && gLawnApp->mBoard && gLawnApp->mBoard->mSeedBank)
		{
			auto* bank = gLawnApp->mBoard->mSeedBank.get();
			if (index >= static_cast<std::uint32_t>(bank->mNumPackets))
				return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, static_cast<std::int32_t>(index));
			const auto& seed = bank->mSeedPackets[index];
			*outPacketType = static_cast<std::int32_t>(seed.mPacketType);
			*outImitaterType = static_cast<std::int32_t>(seed.mImitaterType);
			return Ok();
		}
		return Fail(PVZP_RS_STATUS_NOT_FOUND);
	});
}

extern "C" pvzp_rs_status pvzp_rs_select_card(std::int32_t packetType, std::int32_t imitaterType) noexcept
{
	return Guard([&] {
		if (!gLawnApp || gLawnApp->mGameScene != GameScenes::SCENE_LEVEL_INTRO || !gLawnApp->mSeedChooserScreen)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		if (!IsPacketType(packetType) || !IsImitaterTarget(imitaterType))
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, packetType);
		auto* chooser = gLawnApp->mSeedChooserScreen;
		const auto packet = static_cast<SeedType>(packetType);
		if (packet == SeedType::SEED_IMITATER)
		{
			if (imitaterType == static_cast<std::int32_t>(SeedType::SEED_NONE) || !chooser->mImitaterButton)
				return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, imitaterType);
			auto& imitater = chooser->mChosenSeeds[SeedType::SEED_IMITATER];
			imitater.mSeedState = ChosenSeedState::SEED_IN_CHOOSER;
			imitater.mImitaterType = static_cast<SeedType>(imitaterType);
			imitater.mX = chooser->mImitaterButton->mX;
			imitater.mY = chooser->mImitaterButton->mY;
			chooser->ClickedSeedInChooser(imitater);
			chooser->UpdateImitaterButton();
			return Ok();
		}
		if (imitaterType != static_cast<std::int32_t>(SeedType::SEED_NONE))
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, imitaterType);
		chooser->ClickedSeedInChooser(chooser->mChosenSeeds[static_cast<std::size_t>(packet)]);
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_world_current_plant_cost(const pvzp_rs_world* world, std::int32_t packetType, std::int32_t imitaterType, std::int32_t* outCost) noexcept
{
	return Guard([&] {
		if (!world || !outCost)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (!IsPacketType(packetType) || !IsImitaterTarget(imitaterType))
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, packetType);
		*outCost = const_cast<Board*>(Native(world))->GetCurrentPlantCost(
			static_cast<SeedType>(packetType), static_cast<SeedType>(imitaterType));
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_world_can_take_sun(const pvzp_rs_world* world, std::int32_t amount, std::uint8_t* outAllowed) noexcept
{
	return Guard([&] {
		if (!world || !outAllowed)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		*outAllowed = static_cast<std::uint8_t>(const_cast<Board*>(Native(world))->CanTakeSunMoney(amount));
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_world_take_sun(pvzp_rs_world* world, std::int32_t amount, std::uint8_t* outTaken) noexcept
{
	return Guard([&] {
		if (!world || !outTaken)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		*outTaken = static_cast<std::uint8_t>(Native(world)->TakeSunMoney(amount));
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_world_can_plant_at(const pvzp_rs_world* world, std::int32_t packetType, std::int32_t gridX, std::int32_t gridY, std::int32_t* outReason) noexcept
{
	return Guard([&] {
		if (!world || !outReason)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (!IsPacketType(packetType))
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, packetType);
		*outReason = static_cast<std::int32_t>(const_cast<Board*>(Native(world))->CanPlantAt(
			gridX, gridY, static_cast<SeedType>(packetType)));
		return Ok();
	});
}

#define PVZP_RS_DEFINE_CREATE_PLANT(name, method) \
	extern "C" pvzp_rs_status pvzp_rs_world_##name(pvzp_rs_world* world, std::int32_t gridX, std::int32_t gridY, std::int32_t packetType, std::int32_t imitaterType, pvzp_rs_plant** outPlant) noexcept \
	{ \
		return Guard([&] { \
			if (!world || !outPlant) return Fail(PVZP_RS_STATUS_NULL_POINTER); \
			if (!IsPacketType(packetType) || !IsImitaterTarget(imitaterType)) return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, packetType); \
			*outPlant = Native(world)->method(gridX, gridY, static_cast<SeedType>(packetType), static_cast<SeedType>(imitaterType)); \
			return Ok(); \
		}); \
	}

PVZP_RS_DEFINE_CREATE_PLANT(new_plant, NewPlant)
PVZP_RS_DEFINE_CREATE_PLANT(add_plant, AddPlant)

#undef PVZP_RS_DEFINE_CREATE_PLANT

extern "C" pvzp_rs_status pvzp_rs_plant_die(pvzp_rs_plant* plant) noexcept
{
	return Guard([&] { if (!plant) return Fail(PVZP_RS_STATUS_NULL_POINTER); plant->Die(); return Ok(); });
}

extern "C" pvzp_rs_status pvzp_rs_plant_imitater_morph(pvzp_rs_plant* plant, pvzp_rs_plant** outPlant) noexcept
{
    return Guard([&] {
        if (!plant || !outPlant || !plant->mBoard) return Fail(PVZP_RS_STATUS_NULL_POINTER);
        *outPlant = nullptr;
        if (plant->mDead || plant->mSeedType != SeedType::SEED_IMITATER) return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT);
        auto& pool = plant->mBoard->mPlants;
        if (pool.mSize >= pool.mMaxSize || pool.mFreeListHead >= pool.mMaxSize) return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT);
        auto* successor = &pool.DataArrayGetItemAt(pool.mFreeListHead);
        plant->ImitaterMorph();
        *outPlant = successor;
        return Ok();
    });
}

extern "C" pvzp_rs_status pvzp_rs_plant_set_sleeping(pvzp_rs_plant* plant, std::uint8_t asleep) noexcept
{
	return Guard([&] { if (!plant) return Fail(PVZP_RS_STATUS_NULL_POINTER); plant->SetSleeping(asleep != 0); return Ok(); });
}

extern "C" pvzp_rs_status pvzp_rs_plant_play_idle(pvzp_rs_plant* plant, float fps) noexcept
{
	return Guard([&] { if (!plant) return Fail(PVZP_RS_STATUS_NULL_POINTER); plant->PlayIdleAnim(fps); return Ok(); });
}

extern "C" pvzp_rs_status pvzp_rs_plant_update_reanim_color(pvzp_rs_plant* plant) noexcept
{
	return Guard([&] { if (!plant) return Fail(PVZP_RS_STATUS_NULL_POINTER); plant->UpdateReanimColor(); return Ok(); });
}

extern "C" pvzp_rs_status pvzp_rs_plant_fire_cob(pvzp_rs_plant* plant, std::int32_t targetX, std::int32_t targetY) noexcept
{
	return Guard([&] {
		if (!plant)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (plant->mSeedType != SeedType::SEED_COBCANNON)
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, static_cast<std::int32_t>(plant->mSeedType));
		plant->CobCannonFire(targetX, targetY);
		return Ok();
	});
}

extern "C" void pvzp_rs_plant_hit_box(pvzp_rs_plant* plant, pvzp_rs_rect_i32* outRect) noexcept {
    // Live input and writable output are guaranteed by the Rust borrow; weapon is validated.
    CopyRect(plant->GetPlantRect(), outRect);
}

extern "C" void pvzp_rs_plant_attack_rect(pvzp_rs_plant* plant, std::int32_t weapon, pvzp_rs_rect_i32* outRect) noexcept {
    // Live input and writable output are guaranteed by the Rust borrow; weapon is validated.
    CopyRect(plant->GetPlantAttackRect(static_cast<PlantWeapon>(weapon)), outRect);
}

extern "C" std::uint32_t pvzp_rs_plant_damage_range_flags(pvzp_rs_plant* plant, std::int32_t weapon) noexcept
{
    // Caller provides a live plant and a validated primary/secondary weapon.
    return static_cast<std::uint32_t>(plant->GetDamageRangeFlags(static_cast<PlantWeapon>(weapon)));
}

extern "C" pvzp_rs_status pvzp_rs_plant_reanimation(pvzp_rs_plant* plant, pvzp_rs_reanimation** outReanimation) noexcept
{
	return Guard([&] {
		if (!plant || !outReanimation)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (!gLawnApp)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		*outReanimation = gLawnApp->ReanimationTryToGet(plant->mBodyReanimID);
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_zombie_pos_y_based_on_row(pvzp_rs_zombie* zombie, std::int32_t row, float* outY) noexcept
{
	return Guard([&] {
		if (!zombie || !outY)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		*outY = zombie->GetPosYBasedOnRow(row);
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_zombie_hit_box(pvzp_rs_zombie* zombie, pvzp_rs_rect_i32* outRect) noexcept
{
	return Guard([&] {
		if (!zombie || !outRect)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		CopyRect(zombie->GetZombieRect(), outRect);
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_zombie_effected_by_damage(pvzp_rs_zombie* zombie, std::uint32_t flags, std::uint8_t* outEffected) noexcept
{
	return Guard([&] {
		if (!zombie || !outEffected)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		*outEffected = static_cast<std::uint8_t>(zombie->EffectedByDamage(flags));
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_zombie_can_target_plant(pvzp_rs_zombie* zombie, pvzp_rs_plant* plant, std::int32_t attackType, std::uint8_t* outCanTarget) noexcept
{
	return Guard([&] {
		if (!zombie || !plant || !outCanTarget)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (attackType < static_cast<std::int32_t>(ZombieAttackType::ATTACKTYPE_CHEW) || attackType > static_cast<std::int32_t>(ZombieAttackType::ATTACKTYPE_LADDER))
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, attackType);
		*outCanTarget = static_cast<std::uint8_t>(zombie->CanTargetPlant(plant, static_cast<ZombieAttackType>(attackType)));
		return Ok();
	});
}

extern "C" std::uint8_t pvzp_rs_zombie_is_dead_or_dying(pvzp_rs_zombie* zombie) noexcept { return static_cast<std::uint8_t>(zombie->IsDeadOrDying()); }

extern "C" pvzp_rs_status pvzp_rs_zombie_reanimation(pvzp_rs_zombie* zombie, pvzp_rs_reanimation** outReanimation) noexcept
{
	return Guard([&] {
		if (!zombie || !outReanimation)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		// GameObject binds mApp at construction. The borrowed zombie keeps its
		// owning world alive; resolve only the optional child for this read.
		Reanimation* reanimation = zombie->mApp->ReanimationTryToGet(zombie->mBodyReanimID);
		*outReanimation = reanimation && !reanimation->mDead ? reanimation : nullptr;
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_world_add_zombie_in_row(pvzp_rs_world* world, std::int32_t zombieType, std::int32_t row, std::int32_t fromWave, pvzp_rs_zombie** outZombie) noexcept
{
	return Guard([&] {
		if (!world || !outZombie)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (!IsZombieType(zombieType))
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, zombieType);
		*outZombie = Native(world)->AddZombieInRow(static_cast<ZombieType>(zombieType), row, fromWave);
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_world_place_zombie(pvzp_rs_world* world, std::int32_t zombieType, std::int32_t gridX, std::int32_t gridY, pvzp_rs_zombie** outZombie) noexcept
{
	return Guard([&] {
		if (!world || !outZombie)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (!IsZombieType(zombieType))
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, zombieType);
		if (!Native(world)->mChallenge)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		*outZombie = Native(world)->mChallenge->IZombiePlaceZombie(static_cast<ZombieType>(zombieType), gridX, gridY);
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_zombie_die_no_loot(pvzp_rs_zombie* zombie) noexcept
{
	return Guard([&] { if (!zombie) return Fail(PVZP_RS_STATUS_NULL_POINTER); zombie->DieNoLoot(); return Ok(); });
}

extern "C" pvzp_rs_status pvzp_rs_zombie_die_with_loot(pvzp_rs_zombie* zombie) noexcept
{
	return Guard([&] { if (!zombie) return Fail(PVZP_RS_STATUS_NULL_POINTER); zombie->DieWithLoot(); return Ok(); });
}

extern "C" void pvzp_rs_projectile_hit_box(pvzp_rs_projectile* projectile, pvzp_rs_rect_i32* outRect) noexcept {
    // Live input and writable output are guaranteed by the Rust borrow; weapon is validated.
    CopyRect(projectile->GetProjectileRect(), outRect);
}

#define PVZP_RS_DEFINE_CREATE_GRID_ITEM(name, method) \
	extern "C" pvzp_rs_status pvzp_rs_world_##name(pvzp_rs_world* world, std::int32_t gridX, std::int32_t gridY, pvzp_rs_grid_item** outItem) noexcept \
	{ \
		return Guard([&] { \
			if (!world || !outItem) return Fail(PVZP_RS_STATUS_NULL_POINTER); \
			*outItem = Native(world)->method(gridX, gridY); \
			return Ok(); \
		}); \
	}

PVZP_RS_DEFINE_CREATE_GRID_ITEM(add_ladder, AddALadder)
PVZP_RS_DEFINE_CREATE_GRID_ITEM(add_crater, AddACrater)
PVZP_RS_DEFINE_CREATE_GRID_ITEM(add_gravestone, AddAGraveStone)

#undef PVZP_RS_DEFINE_CREATE_GRID_ITEM

extern "C" pvzp_rs_status pvzp_rs_grid_item_die(pvzp_rs_grid_item* item) noexcept
{
	return Guard([&] { if (!item) return Fail(PVZP_RS_STATUS_NULL_POINTER); item->GridItemDie(); return Ok(); });
}

extern "C" pvzp_rs_status pvzp_rs_item_collect(pvzp_rs_item* item, std::uint8_t playSound) noexcept
{
	return Guard([&] {
		if (!item)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (playSound != 0)
			item->PlayCollectSound();
		item->Collect();
		return Ok();
	});
}

#define PVZP_RS_DEFINE_BOOL_RULE(name, member) \
    extern "C" std::uint8_t pvzp_rs_##name() noexcept \
    { return static_cast<std::uint8_t>(gModifiers.member); } \
	extern "C" pvzp_rs_status pvzp_rs_set_##name(std::uint8_t enabled) noexcept \
	{ \
		return Guard([&] { gModifiers.member = enabled != 0; return Ok(); }); \
	}

PVZP_RS_DEFINE_BOOL_RULE(seed_recharge_ignored, seedRechargeIgnored)
PVZP_RS_DEFINE_BOOL_RULE(sun_cost_ignored, sunCostIgnored)
PVZP_RS_DEFINE_BOOL_RULE(fog_revealed, fogRevealed)
PVZP_RS_DEFINE_BOOL_RULE(vase_contents_visible, vaseContentsVisible)
PVZP_RS_DEFINE_BOOL_RULE(instant_ice_and_ash_effects, instantIceAndAshEffects)
PVZP_RS_DEFINE_BOOL_RULE(mushrooms_awake, mushroomsAwake)
PVZP_RS_DEFINE_BOOL_RULE(cob_fixed_delay, cobFixedDelay)
PVZP_RS_DEFINE_BOOL_RULE(cob_recharge_shortened, cobRechargeShortened)
PVZP_RS_DEFINE_BOOL_RULE(cob_drift_fixed, cobDriftFixed)
PVZP_RS_DEFINE_BOOL_RULE(item_drop_disabled, itemDropDisabled)
PVZP_RS_DEFINE_BOOL_RULE(natural_sun_drop_disabled, naturalSunDropDisabled)
PVZP_RS_DEFINE_BOOL_RULE(jack_explosions_disabled, jackExplosionsDisabled)
PVZP_RS_DEFINE_BOOL_RULE(pepper_explosions_disabled, pepperExplosionsDisabled)
PVZP_RS_DEFINE_BOOL_RULE(special_events_disabled, specialEventsDisabled)
PVZP_RS_DEFINE_BOOL_RULE(zombie_spawn_stopped, zombieSpawnStopped)
PVZP_RS_DEFINE_BOOL_RULE(zombies_die_at_house, zombiesDieAtHouse)
PVZP_RS_DEFINE_BOOL_RULE(planting_restrictions_ignored, plantingRestrictionsIgnored)
PVZP_RS_DEFINE_BOOL_RULE(profile_readonly, profileReadonly)
PVZP_RS_DEFINE_BOOL_RULE(normal_auto_collect_enabled, normalAutoCollectEnabled)

#undef PVZP_RS_DEFINE_BOOL_RULE

extern "C" pvzp_rs_status pvzp_rs_easy_planting_cheat(std::uint8_t* outEnabled) noexcept
{
	return Guard([&] {
		if (!outEnabled)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (!gLawnApp)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		*outEnabled = static_cast<std::uint8_t>(gLawnApp->mEasyPlantingCheat);
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_set_easy_planting_cheat(std::uint8_t enabled) noexcept
{
	return Guard([&] {
		if (!gLawnApp)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		if (!gEasyPlantingCaptured)
		{
			gEasyPlantingOriginal = gLawnApp->mEasyPlantingCheat;
			gEasyPlantingCaptured = true;
		}
		gLawnApp->mEasyPlantingCheat = enabled != 0;
		return Ok();
	});
}

#define PVZP_RS_DEFINE_ENUM_RULE(name, member, maximum) \
	extern "C" pvzp_rs_status pvzp_rs_##name(std::int32_t* outRule) noexcept \
	{ \
		return Guard([&] { if (!outRule) return Fail(PVZP_RS_STATUS_NULL_POINTER); *outRule = gModifiers.member; return Ok(); }); \
	} \
	extern "C" pvzp_rs_status pvzp_rs_set_##name(std::int32_t rule) noexcept \
	{ \
		return Guard([&] { if (rule < 0 || rule > maximum) return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, rule); gModifiers.member = rule; return Ok(); }); \
	}

PVZP_RS_DEFINE_ENUM_RULE(kernel_pult_projectile_rule, kernelPultProjectileRule, 2)
PVZP_RS_DEFINE_ENUM_RULE(plant_damage_rule, plantDamageRule, 2)
PVZP_RS_DEFINE_ENUM_RULE(maid_cheat, maidCheat, 3)

#undef PVZP_RS_DEFINE_ENUM_RULE

#define PVZP_RS_DEFINE_WORLD_POOL(name, member, pool_type) \
    extern "C" pool_type* pvzp_rs_world_##name##_pool(pvzp_rs_world* world) noexcept \
    { return reinterpret_cast<pool_type*>(&Native(world)->member); }


PVZP_RS_DEFINE_WORLD_POOL(plant, mPlants, pvzp_rs_plant_pool)
PVZP_RS_DEFINE_WORLD_POOL(zombie, mZombies, pvzp_rs_zombie_pool)
PVZP_RS_DEFINE_WORLD_POOL(projectile, mProjectiles, pvzp_rs_projectile_pool)
PVZP_RS_DEFINE_WORLD_POOL(grid_item, mGridItems, pvzp_rs_grid_item_pool)
PVZP_RS_DEFINE_WORLD_POOL(item, mCoins, pvzp_rs_item_pool)

#undef PVZP_RS_DEFINE_WORLD_POOL

#define PVZP_RS_DEFINE_POOL_API(prefix, pool_type, object_type) \
	extern "C" std::uint32_t pvzp_rs_##prefix##_pool_max_used_count(const pool_type* pool) noexcept \
    { return NativePool<object_type>(pool)->mMaxUsedCount; } \
	extern "C" std::uint32_t pvzp_rs_##prefix##_pool_active_count(const pool_type* pool) noexcept \
    { return NativePool<object_type>(pool)->mSize; } \
	extern "C" std::uint32_t pvzp_rs_##prefix##_pool_capacity(const pool_type* pool) noexcept \
    { return NativePool<object_type>(pool)->mMaxSize; } \
	extern "C" std::uint32_t pvzp_rs_##prefix##_pool_free_list_head(const pool_type* pool) noexcept \
    { return NativePool<object_type>(pool)->mFreeListHead; } \
	extern "C" std::uint32_t pvzp_rs_##prefix##_pool_next_id_key(const pool_type* pool) noexcept \
    { return NativePool<object_type>(pool)->mNextKey; } \
	extern "C" pvzp_rs_status pvzp_rs_##prefix##_pool_slot(const pool_type* pool, std::uint32_t index, std::uint8_t* outActive, std::uint32_t* outId, std::uint32_t* outNextFree) noexcept \
	{ \
		return Guard([&] { return PoolSlot<object_type>(pool, index, outActive, outId, outNextFree); }); \
	} \
	extern "C" pvzp_rs_status pvzp_rs_##prefix##_pool_get(pool_type* pool, std::int32_t index, object_type** outObject) noexcept \
	{ \
		return Guard([&] { return PoolGet<object_type>(pool, index, outObject); }); \
	} \
	extern "C" object_type* pvzp_rs_##prefix##_pool_try_to_get(pool_type* pool, std::uint32_t id) noexcept \
    { return NativePool<object_type>(pool)->DataArrayTryToGet(id); } \
	extern "C" pvzp_rs_status pvzp_rs_##prefix##_pool_get_id(const pool_type* pool, const object_type* object, std::uint32_t* outId) noexcept \
	{ \
		return Guard([&] { return PoolGetId<object_type>(pool, object, outId); }); \
	}

PVZP_RS_DEFINE_POOL_API(plant, pvzp_rs_plant_pool, pvzp_rs_plant)
PVZP_RS_DEFINE_POOL_API(zombie, pvzp_rs_zombie_pool, pvzp_rs_zombie)
PVZP_RS_DEFINE_POOL_API(projectile, pvzp_rs_projectile_pool, pvzp_rs_projectile)
PVZP_RS_DEFINE_POOL_API(grid_item, pvzp_rs_grid_item_pool, pvzp_rs_grid_item)
PVZP_RS_DEFINE_POOL_API(item, pvzp_rs_item_pool, pvzp_rs_item)

#undef PVZP_RS_DEFINE_POOL_API
