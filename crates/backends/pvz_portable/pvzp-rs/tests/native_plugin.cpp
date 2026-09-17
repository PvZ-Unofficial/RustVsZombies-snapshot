#include "bridge.h"
#include "PvzpLib/PluginLayout.h"
#include "Lawn/Widget/GameSelector.h"
#include "SexyAppFramework/sound/SoundManager.h"
#include "SexyAppFramework/sound/SoundInstance.h"
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdexcept>
#include <cstring>
#include <cstdio>
#include <algorithm>

static void Record(const char* text)
{
    wchar_t path[32768];
    if (!GetEnvironmentVariableW(L"PVZP_TEST_REPORT", path, 32768)) return;
    HANDLE file = CreateFileW(path, FILE_APPEND_DATA, FILE_SHARE_READ | FILE_SHARE_WRITE,
        nullptr, OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (file == INVALID_HANDLE_VALUE) return;
    DWORD written;
    WriteFile(file, text, static_cast<DWORD>(std::strlen(text)), &written, nullptr);
    WriteFile(file, "\n", 1, &written, nullptr);
    CloseHandle(file);
}
static void Check(bool value, const char* message) { if (!value) throw std::runtime_error(message); }
static void Ok(pvzp_rs_status result) { Check(result.code == 0, "native operation failed"); }
static int phase = 0;
static int dispatchCount = 0;
static int frameBegins = 0;
static int frameEnds = 0;
static int destroyedBoards = 0;
static bool destroyWasRevoked = true;
static std::uint64_t eventEpoch = 0;
static std::uint64_t firstEpoch = 0;
static std::uint32_t bitePlant = 0;
static std::uint32_t biter = 0;
static int biteHealth = 0;
static int biteAttempts = 0;
static int biteFinished = 0;
static int biteRequested = 0;
static int biteOutcome = -1;
static int biteApplied = 0;
static bool suppressBite = true;
static int pausedAppClock = 0;
static int pausedBoardClock = 0;
static void BeginFrame(std::uint64_t epoch, std::int32_t) { ++frameBegins; eventEpoch = epoch; }
static void EndFrame(std::int32_t) { ++frameEnds; }
static std::uint64_t BeginEffect(std::int32_t source, void*, void*, std::int32_t, std::int32_t requested)
{
    if (source != 1) return 0;
    ++biteAttempts;
    biteRequested = requested;
    return 1ULL | (std::uint64_t(suppressBite) << 32);
}
static void FinishEffect(std::uint32_t, std::int32_t outcome, std::int32_t applied)
{
    ++biteFinished;
    biteOutcome = outcome;
    biteApplied = applied;
}
static void ZombieEvent(void*) {}
static void ImpEvent(void*, void*) {}
static void Destroyed()
{
    ++destroyedBoards;
    destroyWasRevoked = destroyWasRevoked && gLawnApp->mPlugin.battleInterest == 0;
}
static void RegisterBattle(std::uint32_t interest = 1)
{
    const PvzpPlugin::BattleCallbacks callbacks{BeginFrame, EndFrame, BeginEffect, FinishEffect,
        ZombieEvent, ZombieEvent, ImpEvent, ZombieEvent};
    Check(PvzpPlugin::SetBattleCallbacks(&callbacks, interest), "battle registration failed");
}
static void Step()
{
    const auto before = dispatchCount;
    gLawnApp->UpdateFrames();
    Check(dispatchCount == before, "nested update dispatched another plugin callback");
}

static void ProfileChecks()
{
    std::uint32_t original;
    Ok(pvzp_rs_coins(&original));
    for (unsigned value : {0U, 99999U}) {
        Ok(pvzp_rs_set_coins(value));
        unsigned actual;
        Ok(pvzp_rs_coins(&actual));
        Check(actual == value && gLawnApp->mPlayerInfo->mCoins == value, "coins are not native units");
    }
    Check(pvzp_rs_set_coins(100000).code != 0 && pvzp_rs_set_coins(0xffffffffU).code != 0, "invalid coins accepted");
    Ok(pvzp_rs_set_coins(original));
    const auto oldLevel = gLawnApp->mPlayerInfo->mLevel;
    const auto oldPurchase = gLawnApp->mPlayerInfo->mPurchases[0];
    Ok(pvzp_rs_unlock_trophy());
    Check(gLawnApp->EarnedGoldTrophy(), "trophy criteria incomplete");
    Check(gLawnApp->mPlayerInfo->mLevel == oldLevel && gLawnApp->mPlayerInfo->mPurchases[0] == oldPurchase,
        "trophy modified unrelated profile fields");
    Check(pvzp_rs_unlock_hidden_modes().code != 0, "hidden page accepted without screen");
    Check(pvzp_rs_water_plant(0, 0).code != 0, "watering accepted outside garden");
    Check(pvzp_rs_mouse(0, -1, 0, 1).code != 0, "invalid mouse coordinate accepted");
    Ok(pvzp_rs_mouse(0, 20, 20, 1));
    Ok(pvzp_rs_mouse(1, 20, 20, 1));
    Ok(pvzp_rs_mouse(2, 20, 20, 1));
    Record("profile-input");
}

static void AudioChecks()
{
    Check(pvzp_rs_sound(256, 0).code != 0, "invalid sound accepted");
    auto* manager = gLawnApp->mSoundManager.get();
    auto* first = manager->GetSoundInstance(0);
    auto* other = manager->GetSoundInstance(1);
    Check(first && other, "test sounds unavailable");
    Check(first->Play(true, false) && other->Play(true, false), "test playback failed");
    Ok(pvzp_rs_sound(0, 1));
    Check(!first->IsPlaying() && other->IsPlaying(), "stop_sound stopped wrong channels");
    other->Stop();
    first->Release();
    other->Release();
    Record("audio");
}

static void DanceChecks()
{
    auto* board = gLawnApp->mBoard;
    board->mZombieCountDown = 100000;
    PvzpNative::gModifiers.zombieSpawnStopped = true;
    const auto clock = gLawnApp->mAppCounter;
    PvzpNative::gFastForward = true;
    PvzpNative::gFastForwardPerformance = 2;
    Step();
    Check(gLawnApp->mAppCounter == clock + 1, "nested fast-forward failed to use native update count");
    PvzpNative::gFastForward = false;
    PvzpNative::gFastForwardPerformance = 0;
    auto* normal = board->AddZombieInRow(ZombieType::ZOMBIE_NORMAL, 0, 1);
    auto* cone = board->AddZombieInRow(ZombieType::ZOMBIE_TRAFFIC_CONE, 1, 1);
    auto* bucket = board->AddZombieInRow(ZombieType::ZOMBIE_PAIL, 4, 1);
    auto* flag = board->AddZombieInRow(ZombieType::ZOMBIE_FLAG, 5, 1);
    auto* animation = gLawnApp->ReanimationTryToGet(normal->mBodyReanimID);
    const auto before = animation->mAnimTime;
    Ok(pvzp_rs_set_common_dance(1));
    Check(animation->mAnimTime == before, "dance setter eagerly changed animation");
    Step();
    Check(animation->mAnimTime == 0, "existing normal did not enter fast gait");
    Check(gLawnApp->ReanimationTryToGet(cone->mBodyReanimID)->mAnimTime == 0, "cone gait missing");
    Check(gLawnApp->ReanimationTryToGet(bucket->mBodyReanimID)->mAnimTime == 0, "bucket gait missing");
    Check(gLawnApp->ReanimationTryToGet(flag->mBodyReanimID)->mAnimTime != 0, "flag was affected by common gait");
    auto* newborn = board->AddZombieInRow(ZombieType::ZOMBIE_NORMAL, 1, 1);
    Step();
    Check(gLawnApp->ReanimationTryToGet(newborn->mBodyReanimID)->mAnimTime == 0, "newborn gait missing");
    Ok(pvzp_rs_set_common_dance(2));
    Step();
    const auto slowStart = animation->mFrameStart;
    Ok(pvzp_rs_set_common_dance(1));
    Step();
    Check(animation->mFrameStart != slowStart, "fast and slow use the same native animation");
    Ok(pvzp_rs_set_common_dance(0));
    Step();
    Check(animation->mAnimTime != 0, "none did not disable forced gait");
    for (auto* zombie : board->mZombies) zombie->DieNoLoot();
    Step();
    Record("dance");
}

static void CobChecks()
{
    auto* board = gLawnApp->mBoard;
    auto* cob = board->AddPlant(0, 0, SeedType::SEED_COBCANNON, SeedType::SEED_NONE);
    const auto cobId = board->mPlants.DataArrayGetID(cob);
    for (bool water : {false, true}) for (bool target : {false, true})
    for (bool fixed : {false, true}) for (bool delay : {false, true}) {
        for (auto* zombie : board->mZombies) zombie->DieNoLoot();
        Step();
        const int row = water ? 2 : 0;
        PvzpNative::gModifiers.cobFixedDelay = fixed;
        Ok(pvzp_rs_set_cob_impact_delay(delay));
        cob = board->mPlants.DataArrayTryToGet(cobId);
        Check(cob != nullptr, "test cob missing");
        cob->mState = PlantState::STATE_COBCANNON_READY;
        cob->CobCannonFire(600, board->GridToPixelY(7, row));
        cob->mShootingCounter = 2;
        for (int i = 0; i < 5 && board->mProjectiles.mSize == 0; ++i) Step();
        Check(board->mProjectiles.mSize == 1, "cob projectile not produced");
        auto* projectile = *board->mProjectiles.begin();
        const auto id = board->mProjectiles.DataArrayGetID(projectile);
        projectile->mProjectileAge = 30;
        projectile->mRow = row;
        projectile->mPosX = 600;
        projectile->mPosY = board->GridToPixelY(7, row);
        projectile->mPosZ = water ? -21.0f : -61.0f;
        projectile->mVelX = projectile->mVelY = projectile->mAccZ = 0;
        projectile->mVelZ = 1;
        projectile->mShadowY = projectile->mPosY + 67;
        projectile->mCobTargetRow = row;
        projectile->mCobTargetX = 600;
        if (target) {
            auto* zombie = board->AddZombieInRow(ZombieType::ZOMBIE_NORMAL, row, 1);
            zombie->mPosX = 565;
            zombie->mX = 565;
            zombie->mPosY = board->GridToPixelY(7, row);
            zombie->mY = static_cast<int>(zombie->mPosY);
        }
        Step();
        projectile = board->mProjectiles.DataArrayTryToGet(id);
        Check(projectile && !projectile->mDead, "cob bypassed original height gate");
        Step();
        projectile = board->mProjectiles.DataArrayTryToGet(id);
        const bool immediate = !delay || fixed || target || water;
        Check(projectile && projectile->mDead == immediate, "cob impact gate combination mismatch");
        if (!immediate) {
            for (int i = 0; i < 20; ++i) Step();
            projectile = board->mProjectiles.DataArrayTryToGet(id);
            Check(!projectile || projectile->mDead, "natural no-target cob never landed");
        }
        Step();
    }
    PvzpNative::gModifiers.cobFixedDelay = false;
    Ok(pvzp_rs_set_cob_impact_delay(true));
    Record("cob-matrix");
}

static void GardenAndHiddenChecks()
{
    Ok(pvzp_rs_back_to_main_menu());
    auto* selector = gLawnApp->mGameSelector;
    Check(selector != nullptr, "main menu missing");
    selector->mMinigamesLocked = false;
    selector->ButtonDepress(101);
    Check(gLawnApp->mChallengeScreen != nullptr, "challenge screen missing");
    Ok(pvzp_rs_unlock_hidden_modes());
    Check(gLawnApp->mChallengeScreen->mLimboPageUnlocked, "hidden page not opened");
    const auto cheatKeys = gLawnApp->mCheatKeys;
    Ok(pvzp_rs_unlock_hidden_modes());
    Check(gLawnApp->mCheatKeys == cheatKeys, "hidden page enabled all cheats");
    gLawnApp->KillChallengeScreen();
    auto* profile = gLawnApp->mPlayerInfo;
    profile->mNumPottedPlants = 1;
    profile->mPottedPlant[0] = {};
    auto& potted = profile->mPottedPlant[0];
    potted.mSeedType = SeedType::SEED_PEASHOOTER;
    potted.mWhichZenGarden = GardenType::GARDEN_MAIN;
    potted.mPlantAge = PottedPlantAge::PLANTAGE_SPROUT;
    potted.mFeedingsPerGrow = 3;
    gLawnApp->PreNewGame(GameMode::GAMEMODE_CHALLENGE_ZEN_GARDEN, false);
    auto* board = gLawnApp->mBoard;
    auto* garden = gLawnApp->mZenGarden;
    Check(board && garden && garden->mBoard == board, "garden not initialized");
    const auto coins = board->mCoins.mSize;
    Ok(pvzp_rs_water_plant(0, 0));
    Check(potted.mTimesFed == 1 && potted.mLastWateredTime != 0 && board->mCoins.mSize == coins + 1,
        "watering lost native side effects");
    Check(pvzp_rs_water_plant(0, 0).code != 0, "watering ignored native need cooldown");
    Check(pvzp_rs_water_plant(8, 8).code != 0, "invalid garden cell accepted");
    Record("garden-hidden");
}

static int Update(std::uint8_t, std::uint64_t rounds)
{
    ++dispatchCount;
    if (dispatchCount == 1) Record("first-update");
    try {
        if (phase == 0) {
            if (!gLawnApp->mPlayerInfo || (gLawnApp->mTitleScreen && !gLawnApp->mTitleScreen->mLoadingThreadComplete)) return 0;
            PvzpNative::gModifiers.profileReadonly = true;
            ProfileChecks();
            AudioChecks();
            Ok(pvzp_rs_enter_endless(2));
            std::uint8_t clicked;
            Ok(pvzp_rs_click_continue_dialog_if_present(&clicked));
            Ok(pvzp_rs_world_reset(reinterpret_cast<pvzp_rs_world*>(gLawnApp->mBoard), 0, 123, 8000, 1));
            phase = 1;
            return 0;
        }
        if (phase == 1) {
            if (!gLawnApp->mSeedChooserScreen || !gLawnApp->mSeedChooserScreen->mMouseVisible) return 0;
            Ok(pvzp_rs_start_battle());
            phase = 2;
            return 0;
        }
        if (phase == 2) {
            if (gLawnApp->mGameScene != GameScenes::SCENE_PLAYING) return 0;
            pausedAppClock = gLawnApp->mAppCounter;
            pausedBoardClock = gLawnApp->mBoard->mMainCounter;
            Ok(pvzp_rs_set_advanced_pause(1, 0, 0, 0, 0));
            phase = 20;
            return 0;
        }
        if (phase == 20) {
            Check(gLawnApp->mAppCounter == pausedAppClock && gLawnApp->mBoard->mMainCounter == pausedBoardClock,
                "advanced pause advanced native clocks");
            Ok(pvzp_rs_set_advanced_pause(0, 0, 0, 0, 0));
            Record("pause");
            DanceChecks();
            CobChecks();
            auto* board = gLawnApp->mBoard;
            auto* plant = board->AddPlant(4, 0, SeedType::SEED_WALLNUT, SeedType::SEED_NONE);
            auto* zombie = board->AddZombieInRow(ZombieType::ZOMBIE_NORMAL, 0, 1);
            const auto rectangle = plant->GetPlantRect();
            zombie->mPosX = static_cast<float>(rectangle.mX - zombie->mZombieAttackRect.mX + 1);
            zombie->mX = static_cast<int>(zombie->mPosX);
            zombie->mZombieAge = 3 /* native TICKS_BETWEEN_EATS is 4 */;
            bitePlant = board->mPlants.DataArrayGetID(plant);
            biter = board->mZombies.DataArrayGetID(zombie);
            biteHealth = plant->mPlantHealth;
            RegisterBattle(1U << 1);
            phase = 50;
            return 0;
        }
        if (phase == 50) {
            auto* plant = gLawnApp->mBoard->mPlants.DataArrayTryToGet(bitePlant);
            if (biteAttempts != 1 || biteFinished != 1 || !plant || plant->mPlantHealth != biteHealth) {
                auto* zombie = gLawnApp->mBoard->mZombies.DataArrayTryToGet(biter);
                char detail[256];
                std::snprintf(detail, sizeof(detail), "bite details hp=%d before=%d attempts=%d finished=%d outcome=%d applied=%d phase=%d eating=%d frames=%d",
                    plant ? plant->mPlantHealth : -1, biteHealth, biteAttempts, biteFinished, biteOutcome, biteApplied,
                    zombie ? int(zombie->mZombiePhase) : -1, zombie && zombie->mIsEating, frameBegins);
                Record(detail);
            }
            Check(plant && plant->mPlantHealth == biteHealth && biteAttempts == 1 && biteFinished == 1
                && biteOutcome == 0 && biteApplied == 0, "native bite suppression/result mismatch");
            suppressBite = false;
            gLawnApp->mBoard->mZombies.DataArrayTryToGet(biter)->mZombieAge = 3 /* native TICKS_BETWEEN_EATS is 4 */;
            phase = 51;
            return 0;
        }
        if (phase == 51) {
            auto* plant = gLawnApp->mBoard->mPlants.DataArrayTryToGet(bitePlant);
            Check(plant && plant->mPlantHealth == biteHealth - biteRequested && biteAttempts == 2
                && biteFinished == 2 && biteOutcome == 2 && biteApplied == biteRequested,
                "native bite damage/result mismatch");
            PvzpPlugin::ClearBattleCallbacks();
            gLawnApp->mBoard->mZombies.DataArrayTryToGet(biter)->DieNoLoot();
            frameBegins = frameEnds = 0;
            Record("event-suppression");
            RegisterBattle();
            phase = 3;
            return 0;
        }
        if (phase == 3) {
            Check(frameBegins == 1 && frameEnds == 1, "first fight frame not paired");
            firstEpoch = eventEpoch;
            PvzpPlugin::ClearBattleCallbacks(); // ExitFight, even though Board will survive.
            gLawnApp->mBoard->mLevelComplete = true;
            phase = 4;
            return 0;
        }
        if (phase == 4) {
            Check(frameBegins == 1 && frameEnds == 1, "stale sink ran after ExitFight");
            Check(PvzpNative::gBoardEpoch == firstEpoch, "survival stage unexpectedly replaced Board");
            if (rounds) Check(rounds == 1, "round delta was not one-shot");
            if (!gLawnApp->mSeedChooserScreen || !gLawnApp->mSeedChooserScreen->mMouseVisible) return 0;
            Ok(pvzp_rs_start_battle());
            phase = 5;
            return 0;
        }
        if (phase == 5) {
            if (gLawnApp->mGameScene != GameScenes::SCENE_PLAYING) return 0;
            RegisterBattle(); // EnterFight on the same Board.
            phase = 6;
            return 0;
        }
        if (phase == 6) {
            Check(frameBegins == 2 && frameEnds == 2 && eventEpoch == firstEpoch, "same-Board sink not rebound");
            const auto before = destroyedBoards;
            Ok(pvzp_rs_world_reset(reinterpret_cast<pvzp_rs_world*>(gLawnApp->mBoard), 0, 321, 8000, 1));
            Check(destroyedBoards == before + 1 && destroyWasRevoked && gLawnApp->mPlugin.battleInterest == 0,
                "Board destruction did not revoke battle callbacks first");
            phase = 7;
            return 0;
        }
        if (phase == 7) {
            Check(frameBegins == 2 && frameEnds == 2, "new Board inherited old sink");
            if (!gLawnApp->mSeedChooserScreen || !gLawnApp->mSeedChooserScreen->mMouseVisible) return 0;
            Ok(pvzp_rs_start_battle());
            phase = 8;
            return 0;
        }
        if (phase == 8) {
            if (gLawnApp->mGameScene != GameScenes::SCENE_PLAYING) return 0;
            RegisterBattle();
            phase = 9;
            return 0;
        }
        if (phase == 9) {
            Check(frameBegins == 3 && frameEnds == 3 && eventEpoch != firstEpoch, "new Board sink not rebound");
            PvzpPlugin::ClearBattleCallbacks();
            Record("sink-lifecycle");
            GardenAndHiddenChecks();
            Record("passed");
            phase = 10;
            PvzpPlugin::RequestStop();
        }
    } catch (const std::exception& error) {
        Record(error.what());
        PvzpPlugin::RequestStop();
    }
    return 0;
}

extern "C" __declspec(dllexport) std::uint32_t pvzp_plugin_abi_version() { return PvzpPlugin::AbiVersion; }
extern "C" __declspec(dllexport) std::int32_t pvzp_plugin_initialize()
{
    Record("initialize");
    if (!pvzp_rs_validate_abi()) return 1;
    Record("validated");
    return PvzpPlugin::SetUpdateCallback(Update) && PvzpPlugin::SetBoardDestroyingCallback(Destroyed) ? 0 : 2;
}
extern "C" __declspec(dllexport) std::int32_t pvzp_plugin_shutdown() { Record("shutdown"); return 0; }
BOOL WINAPI DllMain(HINSTANCE, DWORD reason, LPVOID reserved)
{
    if (reason == DLL_PROCESS_ATTACH) Record("attach");
    if (reason == DLL_PROCESS_DETACH && !reserved) Record("unload");
    return TRUE;
}
