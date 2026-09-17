extern "C" pvzp_rs_status pvzp_rs_layout(pvzp_rs_layout_fingerprint* outLayout) noexcept
{
	return Guard([&] {
		if (!outLayout)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		*outLayout = {
			1,
			static_cast<std::uint32_t>(sizeof(void*)),
			static_cast<std::uint32_t>(sizeof(GameObject)),
			static_cast<std::uint32_t>(sizeof(Plant)),
			static_cast<std::uint32_t>(sizeof(Zombie)),
			static_cast<std::uint32_t>(sizeof(Projectile)),
			static_cast<std::uint32_t>(sizeof(GridItem)),
			static_cast<std::uint32_t>(sizeof(Coin)),
			static_cast<std::uint32_t>(sizeof(SeedPacket)),
			static_cast<std::uint32_t>(sizeof(Reanimation)),
			static_cast<std::uint32_t>(offsetof(Plant, mSeedType)),
			static_cast<std::uint32_t>(offsetof(Plant, mPlantHealth)),
			static_cast<std::uint32_t>(offsetof(Plant, mState)),
			static_cast<std::uint32_t>(offsetof(Plant, mDead)),
			static_cast<std::uint32_t>(offsetof(Zombie, mZombieType)),
			static_cast<std::uint32_t>(offsetof(Zombie, mZombiePhase)),
			static_cast<std::uint32_t>(offsetof(Zombie, mBodyHealth)),
			static_cast<std::uint32_t>(offsetof(Zombie, mDead)),
			static_cast<std::uint32_t>(offsetof(Projectile, mProjectileType)),
			static_cast<std::uint32_t>(offsetof(Projectile, mDead)),
			static_cast<std::uint32_t>(offsetof(GridItem, mGridItemType)),
			static_cast<std::uint32_t>(offsetof(GridItem, mDead)),
			static_cast<std::uint32_t>(offsetof(Coin, mType)),
			static_cast<std::uint32_t>(offsetof(Coin, mDead)),
			static_cast<std::uint32_t>(offsetof(SeedPacket, mRefreshCounter)),
			static_cast<std::uint32_t>(offsetof(SeedPacket, mPacketType)),
			static_cast<std::uint32_t>(offsetof(Reanimation, mAnimTime)),
			static_cast<std::uint32_t>(offsetof(Reanimation, mAnimRate)),
		};
		return Ok();
	});
}
#if defined(__GNUC__)
#pragma GCC diagnostic pop
#endif

extern "C" std::uint32_t pvzp_rs_plant_id(const pvzp_rs_plant* object) noexcept
{
    // A live handle already proves ownership and excludes slot reclamation.
    return object->mBoard->mPlants.DataArrayGetID(const_cast<pvzp_rs_plant*>(object));
}

extern "C" std::uint32_t pvzp_rs_zombie_id(const pvzp_rs_zombie* object) noexcept
{
    // A live handle already proves ownership and excludes slot reclamation.
    return object->mBoard->mZombies.DataArrayGetID(const_cast<pvzp_rs_zombie*>(object));
}

extern "C" std::uint32_t pvzp_rs_grid_item_id(const pvzp_rs_grid_item* object) noexcept
{
    // A live handle already proves ownership and excludes slot reclamation.
    return object->mBoard->mGridItems.DataArrayGetID(const_cast<pvzp_rs_grid_item*>(object));
}

extern "C" std::uint32_t pvzp_rs_projectile_id(const pvzp_rs_projectile* object) noexcept
{
    // A live handle already proves ownership and excludes slot reclamation.
    return object->mBoard->mProjectiles.DataArrayGetID(const_cast<pvzp_rs_projectile*>(object));
}

extern "C" std::uint32_t pvzp_rs_item_id(const pvzp_rs_item* object) noexcept
{
    // A live handle already proves ownership and excludes slot reclamation.
    return object->mBoard->mCoins.DataArrayGetID(const_cast<pvzp_rs_item*>(object));
}

extern "C" pvzp_rs_world* pvzp_rs_board_from_live_app() noexcept
{
    // The backend token is lent only inside a live LawnApp ingress.
    return reinterpret_cast<pvzp_rs_world*>(gLawnApp->mBoard);
}

extern "C" pvzp_rs_status pvzp_rs_current_world(pvzp_rs_world** outWorld) noexcept
{
	return Guard([&] {
		if (!outWorld)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (!gLawnApp || !gLawnApp->mBoard)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		*outWorld = reinterpret_cast<pvzp_rs_world*>(gLawnApp->mBoard);
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_game_ui(std::int32_t* outGameUi) noexcept
{
	return Guard([&] {
		if (!outGameUi)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (!gLawnApp)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		*outGameUi = static_cast<std::int32_t>(gLawnApp->mGameScene);
		return Ok();
	});
}

#define PVZP_RS_DEFINE_SEED_CHOOSER_SCALAR(name, type, expression) \
	extern "C" pvzp_rs_status pvzp_rs_seed_chooser_##name(type* outValue) noexcept \
	{ \
		return Guard([&] { \
			if (!outValue) \
				return Fail(PVZP_RS_STATUS_NULL_POINTER); \
			SeedChooserScreen* chooser = CurrentSeedChooser(); \
			if (!chooser) \
				return Fail(PVZP_RS_STATUS_NOT_FOUND); \
			*outValue = (expression); \
			return Ok(); \
		}); \
	}

PVZP_RS_DEFINE_SEED_CHOOSER_SCALAR(mouse_visible, std::uint8_t, static_cast<std::uint8_t>(chooser->mMouseVisible))
PVZP_RS_DEFINE_SEED_CHOOSER_SCALAR(parent_present, std::uint8_t, static_cast<std::uint8_t>(chooser->mParent != nullptr))
PVZP_RS_DEFINE_SEED_CHOOSER_SCALAR(widget_manager_present, std::uint8_t, static_cast<std::uint8_t>(chooser->mWidgetManager != nullptr))
PVZP_RS_DEFINE_SEED_CHOOSER_SCALAR(modal_present, std::uint8_t, static_cast<std::uint8_t>(chooser->mWidgetManager && chooser->mWidgetManager->mBaseModalWidget))
PVZP_RS_DEFINE_SEED_CHOOSER_SCALAR(choose_state, std::int32_t, static_cast<std::int32_t>(chooser->mChooseState))
PVZP_RS_DEFINE_SEED_CHOOSER_SCALAR(view_lawn_time, std::int32_t, chooser->mViewLawnTime)
PVZP_RS_DEFINE_SEED_CHOOSER_SCALAR(seeds_in_flight, std::int32_t, chooser->mSeedsInFlight)

#undef PVZP_RS_DEFINE_SEED_CHOOSER_SCALAR

extern "C" pvzp_rs_status pvzp_rs_seed_chooser_cancel_view_lawn() noexcept
{
	return Guard([&] {
		SeedChooserScreen* chooser = CurrentSeedChooser();
		if (!chooser || chooser->mChooseState != SeedChooserState::CHOOSE_VIEW_LAWN)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		chooser->mViewLawnTime = 251;
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_click_continue_dialog_if_present(std::uint8_t* outClicked) noexcept
{
	return Guard([&] {
		if (!outClicked)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (!gLawnApp)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		*outClicked = static_cast<std::uint8_t>(gLawnApp->KillDialog(Dialogs::DIALOG_CONTINUE));
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_input_focused(std::uint8_t* outFocused) noexcept
{
	return Guard([&] {
		if (!outFocused)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (!gLawnApp)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		*outFocused = static_cast<std::uint8_t>(gLawnApp->mHasFocus);
		return Ok();
	});
}

#define PVZP_RS_DEFINE_WORLD_SCALAR(name, type, expression) \
    extern "C" type pvzp_rs_world_##name(const pvzp_rs_world* world) noexcept \
    { return (expression); }


PVZP_RS_DEFINE_WORLD_SCALAR(scene, std::int32_t, static_cast<std::int32_t>(Native(world)->mBackground))
PVZP_RS_DEFINE_WORLD_SCALAR(paused, std::uint8_t, static_cast<std::uint8_t>(Native(world)->mPaused))
PVZP_RS_DEFINE_WORLD_SCALAR(main_counter, std::uint32_t, Native(world)->mMainCounter)
PVZP_RS_DEFINE_WORLD_SCALAR(current_wave, std::int32_t, Native(world)->mCurrentWave)
PVZP_RS_DEFINE_WORLD_SCALAR(sun, std::int32_t, Native(world)->mSunMoney)
PVZP_RS_DEFINE_WORLD_SCALAR(level_complete, std::uint8_t, static_cast<std::uint8_t>(Native(world)->mLevelComplete))
PVZP_RS_DEFINE_WORLD_SCALAR(next_survival_stage_counter, std::int32_t, Native(world)->mNextSurvivalStageCounter)

PVZP_RS_DEFINE_WORLD_SCALAR(total_waves, std::int32_t, Native(world)->mNumWaves)
PVZP_RS_DEFINE_WORLD_SCALAR(refresh_countdown, std::int32_t, Native(world)->mZombieCountDown)
PVZP_RS_DEFINE_WORLD_SCALAR(initial_countdown, std::int32_t, Native(world)->mZombieCountDownStart)
PVZP_RS_DEFINE_WORLD_SCALAR(huge_wave_countdown, std::int32_t, Native(world)->mHugeWaveCountDown)
PVZP_RS_DEFINE_WORLD_SCALAR(level_end_countdown, std::int32_t, Native(world)->mFinalWaveSoundCounter)

#undef PVZP_RS_DEFINE_WORLD_SCALAR

extern "C" pvzp_rs_status pvzp_rs_world_seed_choosing(const pvzp_rs_world* world, std::uint8_t* outChoosing) noexcept
{
	return Guard([&] {
		if (!world || !outChoosing)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		CutScene* cutScene = Native(world)->mCutScene.get();
		if (!cutScene)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		*outChoosing = static_cast<std::uint8_t>(cutScene->mSeedChoosing);
		return Ok();
	});
}

extern "C" std::uint8_t pvzp_rs_world_imitater_successor(
    const pvzp_rs_world*, std::uint32_t placeholderId, std::uint32_t* outSuccessorId) noexcept
{
    const bool found = placeholderId != 0 && placeholderId == gMorphPlaceholder;
    *outSuccessorId = found ? gMorphSuccessor : 0;
    return static_cast<std::uint8_t>(found);
}

extern "C" pvzp_rs_status pvzp_rs_world_set_sun(pvzp_rs_world* world, std::uint32_t sun) noexcept
{
	return Guard([&] {
		if (!world)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (sun > static_cast<std::uint32_t>(INT32_MAX))
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, INT32_MAX);
		Native(world)->mSunMoney = static_cast<std::int32_t>(sun);
		return Ok();
	});
}

extern "C" std::int32_t pvzp_rs_world_natural_sun_generated(const pvzp_rs_world* world) noexcept { return Native(world)->mNumSunsFallen; }

extern "C" std::int32_t pvzp_rs_world_natural_sun_countdown(const pvzp_rs_world* world) noexcept { return Native(world)->mSunCountDown; }

extern "C" pvzp_rs_status pvzp_rs_world_set_natural_sun_generated(pvzp_rs_world* world, std::int32_t count) noexcept
{
	return Guard([&] { if (!world) return Fail(PVZP_RS_STATUS_NULL_POINTER); Native(world)->mNumSunsFallen = count; return Ok(); });
}

extern "C" pvzp_rs_status pvzp_rs_world_set_natural_sun_countdown(pvzp_rs_world* world, std::int32_t countdown) noexcept
{
	return Guard([&] { if (!world) return Fail(PVZP_RS_STATUS_NULL_POINTER); Native(world)->mSunCountDown = countdown; return Ok(); });
}





extern "C" pvzp_rs_status pvzp_rs_app_counter(std::uint32_t* outCounter) noexcept
{
	return Guard([&] {
		if (!outCounter)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (!gLawnApp)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		*outCounter = static_cast<std::uint32_t>(gLawnApp->mAppCounter);
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_set_app_counter(std::uint32_t counter) noexcept
{
	return Guard([&] {
		if (!gLawnApp)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		gLawnApp->mAppCounter = static_cast<std::int32_t>(counter);
		return Ok();
	});
}

#define PVZP_RS_DEFINE_WORLD_ROW_VALUE(name, type, writeValue) \
	extern "C" pvzp_rs_status pvzp_rs_world_##name(const pvzp_rs_world* world, std::uint32_t row, type* outValue) noexcept \
	{ \
		return Guard([&] { \
			if (!world || !outValue) return Fail(PVZP_RS_STATUS_NULL_POINTER); \
			if (row >= MAX_GRID_SIZE_Y) return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, static_cast<std::int32_t>(row)); \
			writeValue; \
			return Ok(); \
		}); \
	}

static_assert(sizeof(float) == sizeof(std::uint32_t));
PVZP_RS_DEFINE_WORLD_ROW_VALUE(ice_path_x, std::int32_t, *outValue = Native(world)->mIceMinX[row])
PVZP_RS_DEFINE_WORLD_ROW_VALUE(ice_path_countdown, std::uint32_t, *outValue = static_cast<std::uint32_t>(Native(world)->mIceTimer[row]))
PVZP_RS_DEFINE_WORLD_ROW_VALUE(row_pick_weight_bits, std::uint32_t, std::memcpy(outValue, &Native(world)->mRowPickingArray[row].mWeight, sizeof(float)))
PVZP_RS_DEFINE_WORLD_ROW_VALUE(row_pick_last_picked_bits, std::uint32_t, std::memcpy(outValue, &Native(world)->mRowPickingArray[row].mLastPicked, sizeof(float)))
PVZP_RS_DEFINE_WORLD_ROW_VALUE(row_pick_second_last_picked_bits, std::uint32_t, std::memcpy(outValue, &Native(world)->mRowPickingArray[row].mSecondLastPicked, sizeof(float)))

#undef PVZP_RS_DEFINE_WORLD_ROW_VALUE

extern "C" pvzp_rs_status pvzp_rs_world_spawn_allowed(const pvzp_rs_world* world, std::uint32_t zombieType, std::uint8_t* outAllowed) noexcept
{
	return Guard([&] {
		if (!world || !outAllowed)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (zombieType >= static_cast<std::uint32_t>(ZombieType::NUM_ZOMBIE_TYPES))
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, static_cast<std::int32_t>(zombieType));
		*outAllowed = static_cast<std::uint8_t>(Native(world)->mZombieAllowed[zombieType]);
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_world_spawn_entry(const pvzp_rs_world* world, std::uint32_t wave, std::uint32_t slot, std::int32_t* outZombieType) noexcept
{
	return Guard([&] {
		if (!world || !outZombieType)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (wave >= MAX_ZOMBIE_WAVES || slot >= MAX_ZOMBIES_IN_WAVE)
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, static_cast<std::int32_t>(wave));
		*outZombieType = static_cast<std::int32_t>(Native(world)->mZombiesInWave[wave][slot]);
		return Ok();
	});
}





extern "C" pvzp_rs_status pvzp_rs_world_commit_wave_refresh(pvzp_rs_world* world, std::int32_t expectedWave, std::int32_t initialCountdown) noexcept
{
	return Guard([&] {
		if (!world)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (initialCountdown <= 0)
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, initialCountdown);
		auto* board = Native(world);
		if (board->mCurrentWave != expectedWave)
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, board->mCurrentWave);
		board->mZombieHealthToNextWave = -1;
		board->mZombieCountDown = initialCountdown - 1;
		board->mZombieCountDownStart = initialCountdown;
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_world_zombie_health_wave_start(const pvzp_rs_world* world, std::int32_t* outHealth) noexcept
{
	return Guard([&] { if (!world || !outHealth) return Fail(PVZP_RS_STATUS_NULL_POINTER); *outHealth = Native(world)->mZombieHealthWaveStart; return Ok(); });
}

extern "C" pvzp_rs_status pvzp_rs_world_total_zombie_health_in_wave(pvzp_rs_world* world, std::int32_t wave, std::int32_t* outHealth) noexcept
{
	return Guard([&] {
		if (!world || !outHealth)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (wave < 0 || wave >= Native(world)->mNumWaves)
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, wave);
		*outHealth = Native(world)->TotalZombiesHealthInWave(wave);
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_world_set_scene(pvzp_rs_world* world, std::int32_t scene) noexcept
{
	return Guard([&] {
		Board* board = Native(world);
		if (!board || !gLawnApp || board != gLawnApp->mBoard)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (scene < static_cast<std::int32_t>(BackgroundType::BACKGROUND_1_DAY)
			|| scene > static_cast<std::int32_t>(BackgroundType::BACKGROUND_MUSHROOM_GARDEN))
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, scene);
		if (!gLawnApp->IsSurvivalMode() || !board->mChallenge || !board->mCutScene)
			return Fail(PVZP_RS_STATUS_UNSUPPORTED);

		const BackgroundType background = static_cast<BackgroundType>(scene);
		const bool hasPool = background == BackgroundType::BACKGROUND_3_POOL
			|| background == BackgroundType::BACKGROUND_4_FOG;
		const bool hadLawnMowers = board->mLawnMowers.mSize > 0;

		board->mBackground = background;
		board->LoadBackgroundImages();
		for (int row = 0; row < MAX_GRID_SIZE_Y; ++row)
		{
			const bool poolRow = hasPool && (row == 2 || row == 3);
			board->mPlantRow[row] = poolRow ? PlantRowType::PLANTROW_POOL
				: row == 5 && !hasPool ? PlantRowType::PLANTROW_DIRT : PlantRowType::PLANTROW_NORMAL;
			for (int col = 0; col < MAX_GRID_SIZE_X; ++col)
				board->mGridSquareType[col][row] = poolRow ? GridSquareType::GRIDSQUARE_POOL
					: row == 5 && !hasPool ? GridSquareType::GRIDSQUARE_DIRT : GridSquareType::GRIDSQUARE_GRASS;
		}

		if (!hasPool)
		{
			if (PvzpParticleSystem* particle = gLawnApp->ParticleTryToGet(board->mPoolSparklyParticleID))
				particle->ParticleSystemDie();
			board->mPoolSparklyParticleID = ParticleSystemID::PARTICLESYSTEMID_NULL;
		}

		if (hadLawnMowers)
			ClearLawnMowers(board);

		--board->mChallenge->mSurvivalStage;
		board->mCutScene->mPlacedLawnItems = false;
		board->mChallenge->PuzzleNextStageClear();
		board->mCutScene->mPlacedLawnItems = true;
		const MusicTune tune = (background == BackgroundType::BACKGROUND_6_BOSS
			|| background == BackgroundType::BACKGROUND_MUSHROOM_GARDEN)
			? MusicTune::MUSIC_TUNE_NIGHT_MOONGRAINS
			: static_cast<MusicTune>(scene + 1);
		gLawnApp->mMusic->PlayMusic(tune);
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_world_clear_lawn_mowers(pvzp_rs_world* world) noexcept
{
	return Guard([&] {
		if (!world)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		ClearLawnMowers(Native(world));
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_world_set_spawn_allowed(pvzp_rs_world* world, std::int32_t zombieType, std::uint8_t allowed) noexcept
{
	return Guard([&] {
		if (!world)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (!IsZombieType(zombieType))
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, zombieType);
		Native(world)->mZombieAllowed[zombieType] = allowed != 0;
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_world_set_spawn_entry(pvzp_rs_world* world, std::uint32_t wave, std::uint32_t slot, std::int32_t zombieType) noexcept
{
	return Guard([&] {
		if (!world)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (wave >= MAX_ZOMBIE_WAVES || slot >= MAX_ZOMBIES_IN_WAVE
			|| (zombieType != static_cast<std::int32_t>(ZombieType::ZOMBIE_INVALID) && !IsZombieType(zombieType)))
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, zombieType);
		Native(world)->mZombiesInWave[wave][slot] = static_cast<ZombieType>(zombieType);
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_world_pick_spawn_list(pvzp_rs_world* world) noexcept
{
	return Guard([&] { if (!world) return Fail(PVZP_RS_STATUS_NULL_POINTER); Native(world)->PickZombieWaves(); return Ok(); });
}

extern "C" pvzp_rs_status pvzp_rs_start_battle() noexcept
{
	return Guard([&] {
		SeedChooserScreen* chooser = CurrentSeedChooser();
		if (!chooser || !gLawnApp->mBoard || !gLawnApp->mBoard->mCutScene || !gLawnApp->mBoard->mSeedBank)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		const int selected = chooser->mSeedsInBank;
		const int slots = gLawnApp->mBoard->mSeedBank->mNumPackets;
		if (selected > slots)
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, selected);
		if (selected < slots && gRandomStreams[0].locked)
			return Fail(PVZP_RS_STATUS_UNSUPPORTED);
		if (gPendingSurvivalStage >= 0 && gLawnApp->mBoard && gLawnApp->mBoard->mChallenge)
			gLawnApp->mBoard->mChallenge->mSurvivalStage = gPendingSurvivalStage;
		chooser->PickRandomSeeds();
		if (gPendingReadyCooldowns && gLawnApp->mBoard && gLawnApp->mBoard->mSeedBank)
		{
			SeedBank* bank = gLawnApp->mBoard->mSeedBank.get();
			for (int i = 0; i < bank->mNumPackets; ++i)
				bank->mSeedPackets[i].mRefreshCounter = bank->mSeedPackets[i].mRefreshTime;
		}
		gPendingSurvivalStage = -1;
		gPendingReadyCooldowns = false;
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_enter_endless(std::int32_t scene) noexcept
{
	return Guard([&] {
		if (!gLawnApp || !gLawnApp->mPlayerInfo)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		const GameMode mode = EndlessMode(scene);
		if (mode == GameMode::NUM_GAME_MODES)
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, scene);
		switch (gLawnApp->mGameScene)
		{
		case GameScenes::SCENE_LOADING:
			if (!gLawnApp->mTitleScreen || !gLawnApp->mTitleScreen->mLoadingThreadComplete)
				return Fail(PVZP_RS_STATUS_NOT_FOUND);
			gLawnApp->LoadingCompleted();
			gLawnApp->KillGameSelector();
			break;
		case GameScenes::SCENE_MENU:
			gLawnApp->KillGameSelector();
			break;
		case GameScenes::SCENE_CHALLENGE:
			gLawnApp->KillChallengeScreen();
			break;
		default:
			return Fail(PVZP_RS_STATUS_UNSUPPORTED, static_cast<std::int32_t>(gLawnApp->mGameScene));
		}
		gLawnApp->PreNewGame(mode, true);
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_back_to_main_menu() noexcept
{
	return Guard([&] {
		if (!gLawnApp || !gLawnApp->mBoard || gLawnApp->mGameScene != GameScenes::SCENE_PLAYING)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		gLawnApp->DoBackToMain();
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_world_reset(
	pvzp_rs_world* world, std::uint32_t completedRounds, std::uint32_t seed,
	std::uint32_t initialSun, std::uint8_t readyCooldowns) noexcept
{
	return Guard([&] {
		if (!world || !gLawnApp || Native(world) != gLawnApp->mBoard || !gLawnApp->mPlayerInfo)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		if (!gLawnApp->IsSurvivalMode() || !Native(world)->mChallenge)
			return Fail(PVZP_RS_STATUS_UNSUPPORTED);
		if (completedRounds > static_cast<std::uint32_t>(INT32_MAX) || initialSun > static_cast<std::uint32_t>(INT32_MAX))
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT);
		RandomConstructionGuard randomConstruction(seed);
		gPendingSurvivalStage = -1;
		gPendingReadyCooldowns = false;
		gLawnApp->mAppRandSeed = seed;
		Sexy::SRand(seed);
		gLawnApp->NewGame();
		Board* board = gLawnApp->mBoard;
		if (!board || !board->mChallenge)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		const std::uint32_t offset = static_cast<std::uint32_t>(gLawnApp->mPlayerInfo->mId)
			+ completedRounds * 101U + static_cast<std::uint32_t>(gLawnApp->mGameMode);
		board->mSunMoney = static_cast<std::int32_t>(initialSun);
		board->mBoardRandSeed = static_cast<std::int32_t>(seed - offset);
		board->mChallenge->mSurvivalStage = static_cast<std::int32_t>(completedRounds);
		board->InitZombieWaves();
		board->mChallenge->mSurvivalStage = 0;
		gLawnApp->mAppCounter = static_cast<std::int32_t>(MixDancerClock(seed));
		gPendingDancerClock = MixDancerClock(seed);
		gPendingWorldOrigin = true;
		gPendingCompletedRounds = 0;
		gPendingSurvivalStage = static_cast<std::int32_t>(completedRounds);
		gPendingReadyCooldowns = readyCooldowns != 0;
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_set_fast_forward(
	std::uint8_t enabled, std::int32_t performance, std::uint8_t suppressWindow) noexcept
{
	return Guard([&] {
		if (performance < 0 || performance > 2)
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, performance);
		if (enabled && gAdvancedPause)
			return Fail(PVZP_RS_STATUS_UNSUPPORTED);
		gFastForward = enabled != 0;
		gFastForwardPerformance = enabled ? performance : 0;
		gFastForwardSuppressWindow = enabled && suppressWindow != 0;
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_set_game_speed(float speed) noexcept
{
	return Guard([&] {
		if (!gLawnApp)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		if (!std::isfinite(speed) || speed < 0.05f || speed > 100.0f)
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT);
		if (!gGameSpeedCaptured)
		{
			gOriginalFrameTime = gLawnApp->mFrameTime;
			gOriginalUpdateMultiplier = gLawnApp->mUpdateMultiplier;
			gGameSpeedCaptured = true;
		}
		if (speed <= 10.0f)
		{
			gLawnApp->mFrameTime = static_cast<int>(10.0f / speed + 0.5f);
			gLawnApp->mUpdateMultiplier = 1.0;
		}
		else
		{
			gLawnApp->mFrameTime = 10;
			gLawnApp->mUpdateMultiplier = speed;
		}
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_restore_game_speed() noexcept
{
	return Guard([&] { RestoreGameSpeed(); return Ok(); });
}

extern "C" std::uint8_t pvzp_rs_fast_forward_active() noexcept { return static_cast<std::uint8_t>(gFastForward); }

extern "C" pvzp_rs_status pvzp_rs_request_seed_chooser_fast_forward(std::uint32_t maxFrames) noexcept
{
	return Guard([&] {
		if (maxFrames == 0)
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT);
		if (gAdvancedPause)
			return Fail(PVZP_RS_STATUS_UNSUPPORTED);
		gFastForward = false;
		gFastForwardPerformance = 0;
		gFastForwardSuppressWindow = false;
		gSeedChooserFastForwardRemaining = maxFrames;
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_set_advanced_pause(
	std::uint8_t enabled, std::uint8_t drawMask, std::uint32_t rgba,
	std::uint8_t playSound, std::uint8_t refreshCursorPreview) noexcept
{
	return Guard([&] {
		if (enabled && gFastForward)
			return Fail(PVZP_RS_STATUS_UNSUPPORTED);
		gAdvancedPause = enabled != 0;
		gAdvancedPauseDrawMask = drawMask != 0;
		gAdvancedPauseMaskRgba = rgba;
		gAdvancedPauseRefreshCursor = refreshCursorPreview != 0;
		if (playSound && gLawnApp)
			gLawnApp->PlaySample(SOUND_PAUSE);
		return Ok();
	});
}

extern "C" std::uint8_t pvzp_rs_advanced_pause_active() noexcept { return static_cast<std::uint8_t>(gAdvancedPause); }

extern "C" pvzp_rs_status pvzp_rs_set_random_mode(std::int32_t mode, std::uint32_t value) noexcept
{
	return Guard([&] {
		if (mode < 0 || mode > 1)
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, mode);
		for (RandomState& state : gRandomStreams)
			state.Reset(mode, value);
		if (mode == 0)
			Sexy::SRand(value == 0 ? 4357 : value);
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_set_wave_spawn_random_seed(std::uint8_t enabled, std::uint32_t seed) noexcept
{
	return Guard([&] {
		if (gWaveSpawnActive || gWaveRowPickActive)
			return Fail(PVZP_RS_STATUS_UNSUPPORTED);
		gWaveSpawnRandomEnabled = enabled != 0;
		gWaveSpawnRandomSeed = seed;
		return Ok();
	});
}

#define PVZP_RS_DEFINE_RANDOM_SCALAR(name, type, expression) \
	extern "C" pvzp_rs_status pvzp_rs_random_##name(std::int32_t stream, type* outValue) noexcept \
	{ \
		return Guard([&] { \
			if (!outValue) return Fail(PVZP_RS_STATUS_NULL_POINTER); \
			if (stream < 0 || stream > 1) return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, stream); \
			const RandomState& state = gRandomStreams[stream]; \
			*outValue = (expression); \
			return Ok(); \
		}); \
	}

PVZP_RS_DEFINE_RANDOM_SCALAR(seed, std::uint32_t, stream == 1 && gLawnApp && gLawnApp->mBoard ? static_cast<std::uint32_t>(gLawnApp->mBoard->GetLevelRandSeed()) : state.seed)
extern "C" std::uint8_t pvzp_rs_random_locked(bool level_stream) noexcept { return static_cast<std::uint8_t>(gRandomStreams[level_stream ? 1 : 0].locked); }
extern "C" std::uint32_t pvzp_rs_random_fixed(bool level_stream) noexcept { return gRandomStreams[level_stream ? 1 : 0].fixed; }

#undef PVZP_RS_DEFINE_RANDOM_SCALAR


extern "C" pvzp_rs_status pvzp_rs_set_sun_production_mode(std::int32_t mode) noexcept
{
	return Guard([&] {
		if (mode < 0 || mode > 1)
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, mode);
		gDirectSunProduction = mode == 0;
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_world_cursor_type(const pvzp_rs_world* world, std::int32_t* outCursorType) noexcept
{
	return Guard([&] {
		if (!world || !outCursorType)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		const auto* cursor = Native(world)->mCursorObject.get();
		if (!cursor)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		*outCursorType = static_cast<std::int32_t>(cursor->mCursorType);
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_world_grid_to_pixel_x(const pvzp_rs_world* world, std::int32_t gridX, std::int32_t gridY, std::int32_t* outX) noexcept
{
	return Guard([&] {
		if (!world || !outX)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		*outX = const_cast<Board*>(Native(world))->GridToPixelX(gridX, gridY);
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_world_grid_to_pixel_y(const pvzp_rs_world* world, std::int32_t gridX, std::int32_t gridY, std::int32_t* outY) noexcept
{
	return Guard([&] {
		if (!world || !outY)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		*outY = const_cast<Board*>(Native(world))->GridToPixelY(gridX, gridY);
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_world_pos_y_based_on_row(const pvzp_rs_world* world, float posX, std::int32_t row, float* outY) noexcept
{
	return Guard([&] {
		if (!world || !outY)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		*outY = const_cast<Board*>(Native(world))->GetPosYBasedOnRow(posX, row);
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_world_is_pool_square(const pvzp_rs_world* world, std::int32_t gridX, std::int32_t gridY, std::uint8_t* outIsPool) noexcept
{
	return Guard([&] {
		if (!world || !outIsPool)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		*outIsPool = static_cast<std::uint8_t>(const_cast<Board*>(Native(world))->IsPoolSquare(gridX, gridY));
		return Ok();
	});
}

extern "C" pvzp_rs_status pvzp_rs_world_row_can_have_zombies(const pvzp_rs_world* world, std::int32_t row, std::uint8_t* outAllowed) noexcept
{
	return Guard([&] {
		if (!world || !outAllowed)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		*outAllowed = static_cast<std::uint8_t>(const_cast<Board*>(Native(world))->RowCanHaveZombies(row));
		return Ok();
	});
}
