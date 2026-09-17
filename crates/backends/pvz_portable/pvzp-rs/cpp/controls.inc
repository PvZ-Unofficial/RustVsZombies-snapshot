extern "C" void rsvz_pvzp_log(const char* message, std::size_t length)
{
	PvzpTraceAndLogLn("%.*s", static_cast<int>(length), message);
}

extern "C" pvzp_rs_status pvzp_rs_set_common_dance(std::int32_t dance) noexcept
{
    if (dance < 0 || dance > 2) return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, dance);
    if (!gLawnApp || !gLawnApp->mBoard) return Fail(PVZP_RS_STATUS_NULL_POINTER);
    gModifiers.commonZombieDance = static_cast<CommonDance>(dance);
    return Ok();
}

extern "C" pvzp_rs_status pvzp_rs_set_dance_mode(std::uint8_t enabled) noexcept
{
    if (!gLawnApp || !gLawnApp->mBoard) return Fail(PVZP_RS_STATUS_NULL_POINTER);
    gLawnApp->mBoard->SetDanceMode(enabled != 0);
    return Ok();
}

extern "C" pvzp_rs_status pvzp_rs_set_cob_impact_delay(std::uint8_t enabled) noexcept
{
    if (!gLawnApp || !gLawnApp->mBoard) return Fail(PVZP_RS_STATUS_NULL_POINTER);
    gModifiers.disableCobImpactDelay = !enabled;
    return Ok();
}

extern "C" pvzp_rs_status pvzp_rs_coins(std::uint32_t* value) noexcept
{
    if (!value || !gLawnApp || !gLawnApp->mPlayerInfo) return Fail(PVZP_RS_STATUS_NULL_POINTER);
    const std::uint32_t coins = gLawnApp->mPlayerInfo->mCoins;
    if (coins > 99999) return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT);
    *value = coins;
    return Ok();
}

extern "C" pvzp_rs_status pvzp_rs_set_coins(std::uint32_t value) noexcept
{
    if (!gLawnApp || !gLawnApp->mPlayerInfo) return Fail(PVZP_RS_STATUS_NULL_POINTER);
    if (value > 99999) return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT);
    gLawnApp->mPlayerInfo->mCoins = value;
    return Ok();
}

extern "C" pvzp_rs_status pvzp_rs_mouse(std::int32_t operation, std::int32_t x, std::int32_t y, std::int32_t button) noexcept
{
    return Guard([&] {
        if (!gLawnApp || !gLawnApp->mWidgetManager) return Fail(PVZP_RS_STATUS_NULL_POINTER);
        if (gLawnApp->mShutdown) return Fail(PVZP_RS_STATUS_UNSUPPORTED);
        if (operation < 0 || operation > 3) return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, operation);
        if (operation == 3) {
            if (!gLawnApp->mBoard) return Fail(PVZP_RS_STATUS_NULL_POINTER);
            gLawnApp->mBoard->ClearCursor();
            return Ok();
        }
        if (x < 0 || y < 0 || x >= gLawnApp->mWidth || y >= gLawnApp->mHeight
            || (button != 1 && button != -1)) return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT);
        gLawnApp->mMouseIn = true;
        gLawnApp->mLastUserInputTick = gLawnApp->mLastTimerTime;
        gLawnApp->mWidgetManager->MouseMove(x, y);
        if (operation == 1) gLawnApp->mWidgetManager->MouseDown(x, y, button);
        if (operation == 2) gLawnApp->mWidgetManager->MouseUp(x, y, button);
        // Input can replace Board or run a nested modal loop. No pre-input
        // Board pointer is retained or read after the native operation.
        return Ok();
    });
}

extern "C" pvzp_rs_status pvzp_rs_sound(std::uint32_t id, std::uint8_t stop) noexcept
{
    return Guard([&] {
        if (!gLawnApp || !gLawnApp->mSoundManager) return Fail(PVZP_RS_STATUS_NULL_POINTER);
        if (id >= Sexy::MAX_SOURCE_SOUNDS) return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT);
        if (stop) return gLawnApp->mSoundManager->StopSound(id) ? Ok() : Fail(PVZP_RS_STATUS_NOT_FOUND);
        // The native sound manager owns the auto-release instance and channels.
        auto* instance = gLawnApp->mSoundManager->GetSoundInstance(id);
        if (!instance) return Fail(PVZP_RS_STATUS_NOT_FOUND);
        if (!instance->Play(false, true)) return Fail(PVZP_RS_STATUS_UNSUPPORTED);
        return Ok();
    });
}

extern "C" pvzp_rs_status pvzp_rs_water_plant(std::int32_t row, std::int32_t col) noexcept
{
    return Guard([&] {
        if (!gLawnApp || !gLawnApp->mBoard || !gLawnApp->mZenGarden || !gLawnApp->mPlayerInfo)
            return Fail(PVZP_RS_STATUS_NULL_POINTER);
        auto* board = gLawnApp->mBoard;
        auto* garden = gLawnApp->mZenGarden;
        if (gLawnApp->mGameMode != GameMode::GAMEMODE_CHALLENGE_ZEN_GARDEN || garden->mBoard != board
            || (board->mBackground != BackgroundType::BACKGROUND_GREENHOUSE
                && board->mBackground != BackgroundType::BACKGROUND_MUSHROOM_GARDEN
                && board->mBackground != BackgroundType::BACKGROUND_ZOMBIQUARIUM))
            return Fail(PVZP_RS_STATUS_UNSUPPORTED);
        if (row < 0 || col < 0 || row >= ZEN_MAX_GRIDSIZE_Y || col >= ZEN_MAX_GRIDSIZE_X
            || garden->GridToPixelX(col, row) < 0) return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT);
        for (Plant* plant : board->mPlants) {
            if (plant->mDead || plant->mRow != row || plant->mPlantCol != col || plant->mPottedPlantIndex < 0) continue;
            if (plant->mPottedPlantIndex >= gLawnApp->mPlayerInfo->mNumPottedPlants
                || plant->mPottedPlantIndex >= MAX_POTTED_PLANTS) return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT);
            const auto& potted = gLawnApp->mPlayerInfo->mPottedPlant[plant->mPottedPlantIndex];
            if (potted.mWhichZenGarden != garden->mGardenType) return Fail(PVZP_RS_STATUS_UNSUPPORTED);
            if (!garden->PlantCanBeWatered(plant)) return Fail(PVZP_RS_STATUS_UNSUPPORTED);
            garden->PlantWatered(plant);
            return Ok();
        }
        return Fail(PVZP_RS_STATUS_NOT_FOUND);
    });
}

extern "C" pvzp_rs_status pvzp_rs_unlock_trophy() noexcept
{
    return Guard([&] {
        if (!gLawnApp || !gLawnApp->mPlayerInfo) return Fail(PVZP_RS_STATUS_NULL_POINTER);
        auto* profile = gLawnApp->mPlayerInfo;
        profile->mFinishedAdventure = std::max(profile->mFinishedAdventure, 1U);
        for (int i = 0; i < NUM_CHALLENGE_MODES; ++i) {
            const auto& definition = GetChallengeDefinition(i);
            const auto mode = definition.mChallengeMode;
            if (definition.mPage != CHALLENGE_PAGE_SURVIVAL && definition.mPage != CHALLENGE_PAGE_CHALLENGE
                && definition.mPage != CHALLENGE_PAGE_PUZZLE) continue;
            if (gLawnApp->IsSurvivalEndless(mode) || gLawnApp->IsEndlessScaryPotter(mode) || gLawnApp->IsEndlessIZombie(mode)) continue;
            const std::uint32_t required = gLawnApp->IsSurvivalNormal(mode) ? SURVIVAL_NORMAL_FLAGS
                : gLawnApp->IsSurvivalHard(mode) ? SURVIVAL_HARD_FLAGS : 1;
            auto& record = profile->mChallengeRecords[mode - GameMode::GAMEMODE_SURVIVAL_NORMAL_STAGE_1];
            record = std::max(record, required);
        }
        return gLawnApp->EarnedGoldTrophy() ? Ok() : Fail(PVZP_RS_STATUS_UNSUPPORTED);
    });
}

extern "C" pvzp_rs_status pvzp_rs_unlock_hidden_modes() noexcept
{
    return Guard([&] {
        if (!gLawnApp || !gLawnApp->mChallengeScreen) return Fail(PVZP_RS_STATUS_NULL_POINTER);
        gLawnApp->mChallengeScreen->mLimboPageUnlocked = true;
        gLawnApp->mChallengeScreen->UpdateButtons();
        return Ok();
    });
}

extern "C" void rsvz_pvzp_restore_all() { PvzpNative::RestoreAll(); }

#if defined(__GNUC__)
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Winvalid-offsetof"
#endif
