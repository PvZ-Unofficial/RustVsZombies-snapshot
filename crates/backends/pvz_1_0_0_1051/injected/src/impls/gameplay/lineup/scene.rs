use super::*;

const POOL_WAVE_PARTICLE_TYPE: i32 = 34;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowType1051 {
    None,
    Land,
    Water,
}

impl RowType1051 {
    const fn raw(self) -> i32 {
        match self {
            Self::None => 0,
            Self::Land => 1,
            Self::Water => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlockType1051 {
    Land,
    Bare,
    Pool,
}

impl BlockType1051 {
    const fn raw(self) -> i32 {
        match self {
            Self::Land => 1,
            Self::Bare => 2,
            Self::Pool => 3,
        }
    }
}

pub(super) struct SceneSwitchState {
    pub(super) effect_system: Option<NonNull<ptrs::EffectSystem>>,
    pub(super) challenge: NonNull<ptrs::Challenge>,
    pub(super) cut_scene: NonNull<ptrs::CutScene>,
    pub(super) had_lawn_mowers: bool,
}
pub(super) fn ensure_lineup_scene_switch_supported(scene: SceneKind) -> Result<()> {
    if scene.supports_regular_lineup() {
        Ok(())
    } else {
        Err(Pvz1051Error::KindUnavailable(
            "lineup scene switching supports regular plant-side scenes only",
        ))
    }
}

pub(super) fn write_lineup_scene_terrain(board: NonNull<ptrs::Board>, scene: SceneKind) {
    let has_pool = scene.has_pool();
    let row_types = if has_pool {
        [
            RowType1051::Land,
            RowType1051::Land,
            RowType1051::Water,
            RowType1051::Water,
            RowType1051::Land,
            RowType1051::Land,
        ]
    } else {
        [
            RowType1051::Land,
            RowType1051::Land,
            RowType1051::Land,
            RowType1051::Land,
            RowType1051::Land,
            RowType1051::None,
        ]
    };

    // SAFETY: `board` is non-null and points to the active Board. The row-type array has six i32
    // entries and the block-type array is PvZ's column-major 9x6 terrain table.
    unsafe {
        let row_type_ptr = ptrs::Board::row_types(board.as_ptr());
        for (row, row_type) in row_types.into_iter().enumerate() {
            *row_type_ptr.add(row) = row_type.raw();
        }

        let block_type_ptr = ptrs::Board::block_types(board.as_ptr());
        for col in 0..9 {
            for row in 0..6 {
                let block_type = if has_pool {
                    if matches!(row, 2 | 3) {
                        BlockType1051::Pool
                    } else {
                        BlockType1051::Land
                    }
                } else if row == 5 {
                    BlockType1051::Bare
                } else {
                    BlockType1051::Land
                };
                *block_type_ptr.add(col * 6 + row) = block_type.raw();
            }
        }
    }
}

pub(super) fn prepare_lineup_scene_switch(
    backend: &Pvz1051Backend, board: NonNull<ptrs::Board>, scene: SceneKind,
) -> Result<SceneSwitchState> {
    let effect_system = if scene.has_pool() {
        None
    } else {
        let app = backend.app();
        // SAFETY: `app` is non-null and points to the active LawnApp; effect_system may be absent
        // only during incomplete startup and is checked before any scene state is mutated.
        let effect_system = unsafe { ptrs::LawnApp::effect_system(app.as_ptr()) };
        Some(NonNull::new(effect_system).ok_or(Pvz1051Error::InvariantViolated("effect system"))?)
    };

    // SAFETY: `board` is non-null and points to the active Board. Challenge and CutScene are
    // required later for the reset path, so validate them before the first scene mutation.
    let (challenge, cut_scene, had_lawn_mowers) = unsafe {
        (
            ptrs::Board::challenge(board.as_ptr()),
            ptrs::Board::cut_scene(board.as_ptr()),
            ptrs::Board::lawn_mower_count(board.as_ptr()) > 0,
        )
    };
    let challenge = NonNull::new(challenge).ok_or(Pvz1051Error::NullChallenge)?;
    let cut_scene = NonNull::new(cut_scene).ok_or(Pvz1051Error::AbiPreconditionFailed(
        "1051 cutscene is not ready for lineup scene switching",
    ))?;
    Ok(SceneSwitchState {
        effect_system,
        challenge,
        cut_scene,
        had_lawn_mowers,
    })
}

pub(super) fn clear_pool_wave_particles(effect_system: NonNull<ptrs::EffectSystem>, board: NonNull<ptrs::Board>) {
    // SAFETY: `effect_system` is non-null and points to PvZ's effect system. PvZ stores particle
    // systems behind one holder pointer at LawnApp->mEffectSystem+0x0; the holder may be absent
    // during incomplete startup and is checked before reading the array/count pair.
    let holder = unsafe { ptrs::EffectSystem::particle_system_holder(effect_system.as_ptr()) };
    let Some(holder) = NonNull::new(holder) else {
        // SAFETY: `board` is non-null and points to the active Board. This clears Board's cached
        // ParticleSystemID even when PvZ has no particle-system holder yet.
        unsafe { ptrs::Board::set_pool_particle_system_id(board.as_ptr(), 0) };
        return;
    };
    // SAFETY: `holder` is non-null and points to PvZ's particle-system holder. The particle storage
    // pointer can be null when no particles have been allocated yet and is checked before iterating.
    let (particles, max) = unsafe {
        (
            ptrs::ParticleSystemHolder::particle_systems(holder.as_ptr()),
            ptrs::ParticleSystemHolder::particle_system_count_max(holder.as_ptr()),
        )
    };
    if !particles.is_null() {
        for index in 0..max as usize {
            // SAFETY: `particles` points to PvZ's particle array, `max` is its documented capacity,
            // and each slot is checked for dead/type before invoking the matching delete routine.
            unsafe {
                let particle = particles.add(index);
                if !ptrs::ParticleSystem::dead(particle)
                    && ptrs::ParticleSystem::particle_type(particle) == POOL_WAVE_PARTICLE_TYPE
                {
                    crate::raw::abi::particle_system_delete(particle);
                }
            }
        }
    }

    // SAFETY: `board` is non-null and points to the active Board. This clears Board's cached
    // ParticleSystemID after deleting wave particles when switching away from pool scenes.
    unsafe { ptrs::Board::set_pool_particle_system_id(board.as_ptr(), 0) };
}

pub(super) fn reset_scene_lawn_items(board: NonNull<ptrs::Board>, state: SceneSwitchState) {
    let SceneSwitchState {
        challenge,
        cut_scene,
        had_lawn_mowers,
        effect_system: _,
    } = state;

    if had_lawn_mowers {
        clear_lawn_mowers(board);
    }

    // SAFETY: Pointers were validated before scene mutation. The survival round decrement mirrors
    // pvztoolkit's SetScene(reset=true) guard before PuzzleNextStageClear.
    unsafe {
        *ptrs::Challenge::endless_rounds_mut(challenge.as_ptr()) -= 1;
        ptrs::CutScene::set_lawn_items_placed(cut_scene.as_ptr(), false);
        crate::raw::abi::challenge_puzzle_next_stage_clear();
    }
    // SAFETY: `cut_scene` is the active board-owned CutScene validated before mutation. Native
    // PlaceLawnItems is deliberately not called here because endless scripts generally expect no
    // lawn mowers after automatic lineup scene switching.
    unsafe { ptrs::CutScene::set_lawn_items_placed(cut_scene.as_ptr(), true) };
}

pub(super) fn clear_lawn_mowers(board: NonNull<ptrs::Board>) {
    // SAFETY: `board` is non-null and the mower DataArray is embedded in Board.
    let array = unsafe { ptrs::Board::lawn_mowers(board.as_ptr()) };
    // SAFETY: `board` is non-null and points to the active Board. LawnMower::Die can consume bonus
    // mower stock to allocate replacements, so suppress that side effect while bulk-removing mowers.
    let bonus_remaining = unsafe {
        let bonus = ptrs::Board::bonus_lawn_mowers_remaining_mut(board.as_ptr());
        let value = *bonus;
        *bonus = 0;
        value
    };
    // SAFETY: `array` points to the current Board lawn mower DataArray.
    let max = unsafe { ptrs::DataArray::max_used_count(array) };
    for index in 0..max {
        // SAFETY: occupied_item_at checks bounds and generation occupancy for each mower slot.
        let Some(mower) = (unsafe { ptrs::DataArray::occupied_item_at(array, index) }) else {
            continue;
        };
        // SAFETY: `mower` came from an occupied mower DataArray slot.
        if unsafe { ptrs::LawnMower::is_dead(mower.as_ptr()) } {
            continue;
        }
        // SAFETY: same verified mower pointer; LawnMower::Die is PvZ's normal mower removal path.
        unsafe { crate::raw::abi::lawn_mower_die(mower.as_ptr()) };
    }
    // SAFETY: restores the scalar bonus mower stock on the same active Board after deletion.
    unsafe {
        *ptrs::Board::bonus_lawn_mowers_remaining_mut(board.as_ptr()) = bonus_remaining;
    }
    for index in 0..max {
        // SAFETY: occupied_item_at checks bounds and generation occupancy for each mower slot.
        let Some(mower) = (unsafe { ptrs::DataArray::occupied_item_at(array, index) }) else {
            continue;
        };
        // SAFETY: this second pass sees only current occupied mower slots; `Die` above made every
        // live mower dead, and the mower raw type has no destructor-owned resources beyond `Die`.
        unsafe { ptrs::DataArray::free_lawn_mower(array, mower) };
    }
}

pub(super) fn play_lineup_scene_music(scene: SceneKind) {
    let music_id = match scene {
        SceneKind::MoonNight | SceneKind::MushroomGarden => SceneKind::Night.code() + 1,
        _ => scene.code() + 1,
    };
    // SAFETY: scene switching is only called after LawnApp and Board support are validated. Music
    // id follows PvZ's scene+1 convention, with MoonNight sharing Night music.
    unsafe { crate::raw::abi::lawn_app_play_music(music_id) };
}
