#include "bridge.h"
#include "PvzpLib/NativeControls.h"
#include "PvzpLib/PluginLayout.h"

#define WIN32_LEAN_AND_MEAN
#include <windows.h>

#include <cstdlib>
#include <algorithm>
#include <cmath>
#include <cstddef>
#include <cstring>
#include <exception>
#include <climits>
#include <new>
#include <string_view>

#include "Lawn/Board.h"
#include "Lawn/Challenge.h"
#include "Lawn/CursorObject.h"
#include "Lawn/Cutscene.h"
#include "Lawn/LawnMower.h"
#include "Lawn/System/Music.h"
#include "Lawn/System/PlayerInfo.h"
#include "Lawn/Widget/GameButton.h"
#include "Lawn/Widget/SeedChooserScreen.h"
#include "Lawn/Widget/TitleScreen.h"
#include "LawnApp.h"
#include "PvzpLib/PvzpDebug.h"
#include "PvzpLib/PvzpParticle.h"
#include "SexyAppFramework/Common.h"
#include "SexyAppFramework/graphics/Graphics.h"
#include "SexyAppFramework/misc/MTRand.h"
#include "SexyAppFramework/widget/WidgetManager.h"
#include "SexyAppFramework/sound/SoundManager.h"
#include "SexyAppFramework/sound/SoundInstance.h"


using namespace PvzpNative;

extern "C" std::uint8_t pvzp_rs_check_raw_layout(const pvzp_rs_layout_entry* entries, std::uint32_t count) noexcept
{
    if (!entries || !count) return 0;
    for (std::uint32_t i = 0; i < count; ++i) {
        bool found = false;
        for (const auto& native : PvzpPlugin::Layout) {
            if (native.name != entries[i].name) continue;
            if (native.value != entries[i].value) return 0;
            found = true;
            break;
        }
        if (!found) return 0;
    }
    return 1;
}

extern "C" std::uint8_t pvzp_rs_validate_abi() noexcept
{
    return PvzpPlugin::ValidateLayout(PvzpPlugin::Layout, sizeof(PvzpPlugin::Layout) / sizeof(PvzpPlugin::Layout[0]));
}
extern "C" std::uint8_t pvzp_rs_register_update(PvzpPlugin::UpdateCallback update, PvzpPlugin::BoardDestroyingCallback destroy) noexcept
{
    return PvzpPlugin::SetUpdateCallback(update) && PvzpPlugin::SetBoardDestroyingCallback(destroy);
}
extern "C" std::uint8_t pvzp_rs_register_battle(const pvzp_rs_battle_callbacks* callbacks, std::uint32_t interest) noexcept
{
    return PvzpPlugin::SetBattleCallbacks(callbacks, interest);
}
extern "C" void pvzp_rs_clear_battle() noexcept { PvzpPlugin::ClearBattleCallbacks(); }
extern "C" void pvzp_rs_request_stop() noexcept { PvzpPlugin::RequestStop(); }

namespace {
	constexpr pvzp_rs_status Ok()
	{
		return {PVZP_RS_STATUS_OK, 0};
	}

	constexpr pvzp_rs_status Fail(pvzp_rs_status_code code, std::int32_t detail = 0)
	{
		return {code, detail};
	}

	template <typename F>
	pvzp_rs_status Guard(F&& operation) noexcept
	{
		try
		{
			return operation();
		}
		catch (const std::bad_alloc&)
		{
			return Fail(PVZP_RS_STATUS_BAD_ALLOC);
		}
		catch (const std::exception&)
		{
			return Fail(PVZP_RS_STATUS_STD_EXCEPTION);
		}
		catch (...)
		{
			return Fail(PVZP_RS_STATUS_UNKNOWN_EXCEPTION);
		}
	}
	Board* Native(pvzp_rs_world* world)
	{
		return reinterpret_cast<Board*>(world);
	}

	const Board* Native(const pvzp_rs_world* world)
	{
		return reinterpret_cast<const Board*>(world);
	}

	void ClearLawnMowers(Board* board)
	{
		const int bonus = board->mBonusLawnMowersRemaining;
		board->mBonusLawnMowersRemaining = 0;
		for (LawnMower* mower : board->mLawnMowers)
			if (!mower->mDead)
				mower->Die();
		board->mBonusLawnMowersRemaining = bonus;
		for (LawnMower* mower : board->mLawnMowers)
			if (mower->mDead)
				board->mLawnMowers.DataArrayFree(mower);
	}

	SeedChooserScreen* CurrentSeedChooser()
	{
		if (!gLawnApp || gLawnApp->mGameScene != GameScenes::SCENE_LEVEL_INTRO)
			return nullptr;
		return gLawnApp->mSeedChooserScreen;
	}

	template <typename T, typename Pool>
	DataArray<T>* NativePool(Pool* pool)
	{
		return reinterpret_cast<DataArray<T>*>(pool);
	}

	template <typename T, typename Pool>
	const DataArray<T>* NativePool(const Pool* pool)
	{
		return reinterpret_cast<const DataArray<T>*>(pool);
	}

	template <typename T, typename Pool>
	pvzp_rs_status PoolSlot(
		const Pool* opaque,
		std::uint32_t index,
		std::uint8_t* outActive,
		std::uint32_t* outId,
		std::uint32_t* outNextFree)
	{
		if (!opaque || !outActive || !outId || !outNextFree)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		auto* pool = const_cast<DataArray<T>*>(NativePool<T>(opaque));
		if (index >= pool->mMaxSize)
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, static_cast<std::int32_t>(index));
		const std::uint32_t id = pool->DataArrayGetIDAt(index);
		const bool active = (id & DATA_ARRAY_KEY_MASK) != 0;
		*outActive = static_cast<std::uint8_t>(active);
		*outId = active ? id : 0;
		*outNextFree = active ? 0 : id;
		return Ok();
	}

	template <typename T, typename Pool>
	pvzp_rs_status PoolGet(Pool* opaque, std::int32_t index, T** outObject)
	{
		if (!opaque || !outObject)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		auto* pool = NativePool<T>(opaque);
		if (index < 0 || static_cast<std::uint32_t>(index) >= pool->mMaxUsedCount)
			return Fail(PVZP_RS_STATUS_INVALID_ARGUMENT, index);
		const std::uint32_t id = pool->DataArrayGetIDAt(static_cast<std::uint32_t>(index));
		if (!(id & DATA_ARRAY_KEY_MASK))
		{
			*outObject = nullptr;
			return Ok();
		}
		*outObject = &pool->DataArrayGetItemAt(static_cast<std::uint32_t>(index));
		return Ok();
	}


	constexpr bool PoolContainsAddress(std::uintptr_t address, std::uintptr_t begin,
		std::uintptr_t stride, std::uint32_t count)
	{
		return stride != 0 && address >= begin && (address - begin) % stride == 0 &&
			(address - begin) / stride < count;
	}

	static_assert(PoolContainsAddress(100, 100, 16, 2));
	static_assert(PoolContainsAddress(116, 100, 16, 2));
	static_assert(!PoolContainsAddress(99, 100, 16, 2));
	static_assert(!PoolContainsAddress(101, 100, 16, 2));
	static_assert(!PoolContainsAddress(132, 100, 16, 2));
	static_assert(!PoolContainsAddress(100, 100, 16, 0));
	static_assert(!PoolContainsAddress(100, 100, 0, 2));

	template <typename T, typename Pool>
	pvzp_rs_status PoolGetId(const Pool* opaque, const T* object, std::uint32_t* outId)
	{
		if (!opaque || !object || !outId)
			return Fail(PVZP_RS_STATUS_NULL_POINTER);
		auto* pool = const_cast<DataArray<T>*>(NativePool<T>(opaque));
		const auto count = std::min(pool->mMaxUsedCount, pool->mMaxSize);
		if (count == 0)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		const auto begin = reinterpret_cast<std::uintptr_t>(&pool->DataArrayGetItemAt(0));
		const auto address = reinterpret_cast<std::uintptr_t>(object);
		const auto stride = pool->mMaxSize > 1
			? reinterpret_cast<std::uintptr_t>(&pool->DataArrayGetItemAt(1)) - begin
			: sizeof(T);
		// Check membership before indexing IDs; DataArrayGetID assumes this relation already holds.
		if (!PoolContainsAddress(address, begin, stride, count))
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		const auto index = static_cast<std::uint32_t>((address - begin) / stride);
		const auto id = pool->DataArrayGetIDAt(index);
		if ((id & DATA_ARRAY_KEY_MASK) == 0)
			return Fail(PVZP_RS_STATUS_NOT_FOUND);
		*outId = id;
		return Ok();
	}

	void CopyRect(const Rect& source, pvzp_rs_rect_i32* destination)
	{
		destination->x = source.mX;
		destination->y = source.mY;
		destination->width = source.mWidth;
		destination->height = source.mHeight;
	}

	bool IsPacketType(std::int32_t value)
	{
		return value >= static_cast<std::int32_t>(SeedType::SEED_PEASHOOTER)
			&& value <= static_cast<std::int32_t>(SeedType::SEED_IMITATER);
	}

	bool IsImitaterTarget(std::int32_t value)
	{
		return value == static_cast<std::int32_t>(SeedType::SEED_NONE)
			|| (value >= static_cast<std::int32_t>(SeedType::SEED_PEASHOOTER)
				&& value < static_cast<std::int32_t>(SeedType::SEED_IMITATER));
	}

	bool IsZombieType(std::int32_t value)
	{
		return value >= static_cast<std::int32_t>(ZombieType::ZOMBIE_NORMAL)
			&& value < static_cast<std::int32_t>(ZombieType::NUM_ZOMBIE_TYPES);
	}

	std::uint32_t MixDancerClock(std::uint32_t seed)
	{
		std::uint32_t value = seed ^ 0x9e3779b9U;
		value ^= value >> 16;
		value *= 0x7feb352dU;
		value ^= value >> 15;
		value *= 0x846ca68bU;
		return (value ^ (value >> 16)) % 10000U;
	}

	GameMode EndlessMode(std::int32_t scene)
	{
		switch (scene)
		{
		case 0: return GameMode::GAMEMODE_SURVIVAL_ENDLESS_STAGE_1;
		case 1: return GameMode::GAMEMODE_SURVIVAL_ENDLESS_STAGE_2;
		case 2: return GameMode::GAMEMODE_SURVIVAL_ENDLESS_STAGE_3;
		case 3: return GameMode::GAMEMODE_SURVIVAL_ENDLESS_STAGE_4;
		case 4: return GameMode::GAMEMODE_SURVIVAL_ENDLESS_STAGE_5;
		default: return GameMode::NUM_GAME_MODES;
		}
	}

}

#include "controls.inc"
#include "world.inc"
#include "plants.inc"
