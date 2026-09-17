//! Zombie filtering and spawn-layout helpers.
use crate::{backend::SpawnScheduleBackend, model::SpawnWaveSlot};

use crate::backend::{
    GridTerrainBackend, SceneBackend, ZombiePositionWriteBackend, ZombieReadBackend, ZombieVerticalPositionBackend,
};
use crate::model::{CheckedValueError, SPAWN_SLOTS_PER_WAVE, SceneKind, SpawnList, ZombieKind, ZombieSpawnMode};

/// Error returned while updating the desired per-wave spawn list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DesiredSpawnWaveError {
    #[error("set_wave_zombies requires set_zombies first")]
    MissingBaseZombies,
    #[error("set_wave_zombies requires an exact Average set_zombies request")]
    UnsupportedBaseZombies,
}

/// Zombie-type selection accepted by [`ZombieSpawnRequest`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ZombieTypeSelection {
    /// Use exactly these types. Duplicates are retained as Average-mode ratios.
    Exact(Vec<ZombieKind>),
    /// Keep the required types and randomly fill the remaining type slots
    /// while excluding the banned types.
    Random {
        required: Vec<ZombieKind>,
        banned: Vec<ZombieKind>,
    },
}

/// Complete core-owned request behind the scripting `set_zombies` wrapper.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZombieSpawnRequest {
    selection: ZombieTypeSelection,
    mode: ZombieSpawnMode,
}

impl ZombieSpawnRequest {
    pub fn new(selection: ZombieTypeSelection, mode: ZombieSpawnMode) -> Result<Self, ZombieSpawnRequestError> {
        validate_zombie_type_selection(&selection)?;
        if let (ZombieTypeSelection::Exact(types), ZombieSpawnMode::Exact) = (&selection, mode)
            && types.len() > SPAWN_SLOTS_PER_WAVE
        {
            return Err(ZombieSpawnRequestError::TooManyExactZombies {
                len: types.len(),
                max: SPAWN_SLOTS_PER_WAVE,
            });
        }
        Ok(Self { selection, mode })
    }

    #[must_use]
    pub const fn selection(&self) -> &ZombieTypeSelection {
        &self.selection
    }

    #[must_use]
    pub const fn mode(&self) -> ZombieSpawnMode {
        self.mode
    }

    #[must_use]
    pub fn exact_average_types(&self) -> Option<&[ZombieKind]> {
        match (&self.selection, self.mode) {
            (ZombieTypeSelection::Exact(types), ZombieSpawnMode::Average) => Some(types),
            (ZombieTypeSelection::Exact(_) | ZombieTypeSelection::Random { .. }, _) => None,
        }
    }
}

/// Builds a deferred constrained-random zombie-type selection.
#[must_use]
pub fn random_zombie_types<R, B>(required: R, banned: B) -> ZombieTypeSelection
where
    R: IntoIterator<Item = ZombieKind>,
    B: IntoIterator<Item = ZombieKind>,
{
    ZombieTypeSelection::Random {
        required: unique_zombie_types(required),
        banned: unique_zombie_types(banned),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ZombieSpawnRequestError {
    #[error("set_zombies requires at least one zombie type")]
    Empty,
    #[error("Exact set_zombies has {len} zombies; maximum is {max}")]
    TooManyExactZombies { len: usize, max: usize },
    #[error("random_zombie_types require accepts at most 9 zombie types")]
    TooManyRequired,
    #[error("random_zombie_types require and ban both contain {0:?}")]
    RequiredAndBanned(ZombieKind),
    #[error("random_zombie_types does not support zombie type {0:?}")]
    UnsupportedRandomType(ZombieKind),
    #[error("random_zombie_types cannot use {kind:?} in scene {scene:?}")]
    SceneBanned { scene: SceneKind, kind: ZombieKind },
}

/// Maximum age used by SimpleAvZ `EnsureExist` to identify zombies from the current spawn wave.
pub const NEWLY_SPAWNED_ZOMBIE_MAX_AGE: i32 = 5;

const SPAWN_FLAG_KINDS: [ZombieKind; 33] = [
    ZombieKind::Normal,
    ZombieKind::Flag,
    ZombieKind::Conehead,
    ZombieKind::PoleVaulting,
    ZombieKind::Buckethead,
    ZombieKind::Newspaper,
    ZombieKind::ScreenDoor,
    ZombieKind::Football,
    ZombieKind::Dancing,
    ZombieKind::BackupDancer,
    ZombieKind::DuckyTube,
    ZombieKind::Snorkel,
    ZombieKind::Zomboni,
    ZombieKind::Bobsled,
    ZombieKind::DolphinRider,
    ZombieKind::JackInTheBox,
    ZombieKind::Balloon,
    ZombieKind::Digger,
    ZombieKind::Pogo,
    ZombieKind::Yeti,
    ZombieKind::Bungee,
    ZombieKind::Ladder,
    ZombieKind::Catapult,
    ZombieKind::Gargantuar,
    ZombieKind::Imp,
    ZombieKind::Boss,
    ZombieKind::PeaHead,
    ZombieKind::WallNutHead,
    ZombieKind::JalapenoHead,
    ZombieKind::GatlingHead,
    ZombieKind::SquashHead,
    ZombieKind::TallNutHead,
    ZombieKind::GigaGargantuar,
];

const RANDOM_SPECIFIABLE_ZOMBIES: [ZombieKind; 17] = [
    ZombieKind::PoleVaulting,
    ZombieKind::Buckethead,
    ZombieKind::ScreenDoor,
    ZombieKind::Football,
    ZombieKind::Dancing,
    ZombieKind::Snorkel,
    ZombieKind::Zomboni,
    ZombieKind::DolphinRider,
    ZombieKind::JackInTheBox,
    ZombieKind::Balloon,
    ZombieKind::Digger,
    ZombieKind::Pogo,
    ZombieKind::Bungee,
    ZombieKind::Ladder,
    ZombieKind::Catapult,
    ZombieKind::Gargantuar,
    ZombieKind::GigaGargantuar,
];

fn validate_zombie_type_selection(selection: &ZombieTypeSelection) -> Result<(), ZombieSpawnRequestError> {
    match selection {
        ZombieTypeSelection::Exact(types) if types.is_empty() => Err(ZombieSpawnRequestError::Empty),
        ZombieTypeSelection::Exact(_) => Ok(()),
        ZombieTypeSelection::Random { required, banned } => {
            if required.len() > 9 {
                return Err(ZombieSpawnRequestError::TooManyRequired);
            }
            for kind in required.iter().chain(banned).copied() {
                if !random_zombie_type_supported(kind) {
                    return Err(ZombieSpawnRequestError::UnsupportedRandomType(kind));
                }
            }
            if let Some(kind) = required.iter().copied().find(|kind| banned.contains(kind)) {
                return Err(ZombieSpawnRequestError::RequiredAndBanned(kind));
            }
            Ok(())
        }
    }
}

/// Resolves an exact or constrained-random request against the current scene.
pub fn resolve_zombie_spawn_types(
    request: &ZombieSpawnRequest, scene: SceneKind, seed: u64,
) -> Result<Vec<ZombieKind>, ZombieSpawnRequestError> {
    match request.selection() {
        ZombieTypeSelection::Exact(types) => Ok(types.clone()),
        ZombieTypeSelection::Random { required, banned } => select_random_zombie_types(scene, required, banned, seed),
    }
}

fn select_random_zombie_types(
    scene: SceneKind, required: &[ZombieKind], banned: &[ZombieKind], seed: u64,
) -> Result<Vec<ZombieKind>, ZombieSpawnRequestError> {
    for kind in required.iter().copied() {
        if random_zombie_scene_banned(scene, kind) {
            return Err(ZombieSpawnRequestError::SceneBanned { scene, kind });
        }
    }

    let mut rng = ZombieTypeRng::new(seed);
    let prefer_cone = rng.range(5) != 0;
    let cone_allowed = random_zombie_type_allowed(scene, ZombieKind::Conehead, banned);
    let newspaper_allowed = random_zombie_type_allowed(scene, ZombieKind::Newspaper, banned);
    let (selected, other) = match (prefer_cone, cone_allowed, newspaper_allowed) {
        (_, false, false) => (None, None),
        (true, true, _) | (false, true, false) => (Some(ZombieKind::Conehead), Some(ZombieKind::Newspaper)),
        _ => (Some(ZombieKind::Newspaper), Some(ZombieKind::Conehead)),
    };

    let mut result = Vec::new();
    for kind in [ZombieKind::Normal, ZombieKind::Yeti].into_iter().chain(selected) {
        if random_zombie_type_allowed(scene, kind, banned) {
            push_unique_zombie_type(&mut result, kind);
        }
    }
    for kind in required.iter().copied() {
        push_unique_zombie_type(&mut result, kind);
    }

    let mut candidates = RANDOM_SPECIFIABLE_ZOMBIES
        .into_iter()
        .map(Some)
        .chain(other.map(Some))
        .chain([Some(ZombieKind::Flag), None])
        .filter(|candidate| {
            candidate.as_ref().is_none_or(|kind| {
                random_zombie_type_allowed(scene, *kind, banned) && !required.contains(kind) && !result.contains(kind)
            })
        })
        .collect::<Vec<_>>();
    for _ in 0..9usize.saturating_sub(required.len()) {
        if candidates.is_empty() {
            break;
        }
        if let Some(kind) = candidates.swap_remove(rng.range(candidates.len()))
            && kind != ZombieKind::Flag
        {
            push_unique_zombie_type(&mut result, kind);
        }
    }

    (!result.is_empty())
        .then_some(result)
        .ok_or(ZombieSpawnRequestError::Empty)
}

const fn random_zombie_type_supported(kind: ZombieKind) -> bool {
    matches!(
        kind,
        ZombieKind::Normal
            | ZombieKind::Conehead
            | ZombieKind::PoleVaulting
            | ZombieKind::Buckethead
            | ZombieKind::Newspaper
            | ZombieKind::ScreenDoor
            | ZombieKind::Football
            | ZombieKind::Dancing
            | ZombieKind::Snorkel
            | ZombieKind::Zomboni
            | ZombieKind::DolphinRider
            | ZombieKind::JackInTheBox
            | ZombieKind::Balloon
            | ZombieKind::Digger
            | ZombieKind::Pogo
            | ZombieKind::Yeti
            | ZombieKind::Bungee
            | ZombieKind::Ladder
            | ZombieKind::Catapult
            | ZombieKind::Gargantuar
            | ZombieKind::GigaGargantuar
    )
}

const fn random_zombie_scene_banned(scene: SceneKind, kind: ZombieKind) -> bool {
    match scene {
        SceneKind::Day | SceneKind::MushroomGarden => matches!(kind, ZombieKind::Snorkel | ZombieKind::DolphinRider),
        SceneKind::Night => matches!(
            kind,
            ZombieKind::Snorkel | ZombieKind::Zomboni | ZombieKind::DolphinRider
        ),
        SceneKind::Pool | SceneKind::Fog => false,
        SceneKind::Roof | SceneKind::MoonNight => matches!(
            kind,
            ZombieKind::Dancing | ZombieKind::Snorkel | ZombieKind::DolphinRider | ZombieKind::Digger
        ),
        SceneKind::Greenhouse | SceneKind::Zombiquarium | SceneKind::TreeOfWisdom => true,
    }
}

fn random_zombie_type_allowed(scene: SceneKind, kind: ZombieKind, banned: &[ZombieKind]) -> bool {
    random_zombie_type_supported(kind) && !random_zombie_scene_banned(scene, kind) && !banned.contains(&kind)
}

fn unique_zombie_types(types: impl IntoIterator<Item = ZombieKind>) -> Vec<ZombieKind> {
    let mut unique = Vec::new();
    for kind in types {
        push_unique_zombie_type(&mut unique, kind);
    }
    unique
}

fn push_unique_zombie_type(types: &mut Vec<ZombieKind>, kind: ZombieKind) {
    if !types.contains(&kind) {
        types.push(kind);
    }
}

#[derive(Clone, Copy, Debug)]
struct ZombieTypeRng {
    state: u64,
}

impl ZombieTypeRng {
    const fn new(seed: u64) -> Self {
        Self { state: seed | 1 }
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        (x >> 32) as u32
    }

    fn range(&mut self, upper: usize) -> usize {
        debug_assert!(upper > 0);
        (self.next_u32() as usize) % upper
    }
}

#[must_use]
pub const fn is_aquatic_zombie(kind: ZombieKind) -> bool {
    matches!(kind, ZombieKind::Snorkel | ZombieKind::DolphinRider)
}

pub const fn zombie_row_allowed_for_source(scene: SceneKind, kind: ZombieKind, row_zero_based: i32) -> bool {
    if scene.has_pool() && matches!(row_zero_based, 2 | 3) {
        is_aquatic_zombie(kind) || matches!(kind, ZombieKind::Balloon)
    } else {
        true
    }
}

#[must_use]
pub const fn zombie_row_allowed_for_kind(scene: SceneKind, kind: ZombieKind, row_zero_based: i32) -> bool {
    if scene.has_pool() {
        if is_aquatic_zombie(kind) {
            return matches!(row_zero_based, 2 | 3);
        }
        return matches!(kind, ZombieKind::Balloon) || !matches!(row_zero_based, 2 | 3);
    }

    if is_aquatic_zombie(kind) {
        return false;
    }
    if matches!(scene, SceneKind::Night) && matches!(kind, ZombieKind::Zomboni) {
        return false;
    }
    if scene.has_roof() && matches!(kind, ZombieKind::Dancing | ZombieKind::Digger) {
        return false;
    }
    if matches!(scene, SceneKind::Day | SceneKind::Night | SceneKind::MushroomGarden)
        && matches!(kind, ZombieKind::Dancing)
        && (row_zero_based == 0 || row_zero_based == scene.row_count() as i32 - 1)
    {
        return false;
    }
    true
}

/// Builds an AvZ `ASetZombieMode::AVERAGE`-style spawn list.
#[must_use]
pub fn average_spawn_list(total_waves: usize, kinds: impl IntoIterator<Item = ZombieKind>) -> SpawnList {
    let requested = kinds.into_iter().collect::<Vec<_>>();
    let fill_kinds = requested
        .iter()
        .copied()
        .filter(|kind| is_average_fill_zombie(*kind))
        .collect::<Vec<_>>();
    let has_bungee = requested.contains(&ZombieKind::Bungee);

    let mut spawn_list = SpawnList::new();
    if total_waves == 0 || fill_kinds.is_empty() {
        spawn_list.set_allowed_types(requested);
        return spawn_list;
    }

    for wave in 0..total_waves {
        let wave_start = wave.saturating_mul(SPAWN_SLOTS_PER_WAVE);
        let mut zombies = [ZombieKind::Normal; SPAWN_SLOTS_PER_WAVE];
        for (slot, zombie) in zombies.iter_mut().enumerate() {
            let fill_index = (wave_start + slot) % fill_kinds.len();
            if let Some(kind) = fill_kinds.get(fill_index).copied() {
                *zombie = kind;
            }
        }

        if (wave + 1).is_multiple_of(10) {
            zombies[0] = ZombieKind::Flag;
            if has_bungee {
                for zombie in zombies.iter_mut().take(5).skip(1) {
                    *zombie = ZombieKind::Bungee;
                }
            }
        }

        spawn_list.set_wave(wave, zombies);
    }

    spawn_list.set_allowed_types(requested);
    spawn_list
}

/// Replaces one `0-based` wave with a repeated type list.
///
/// Huge waves reserve their first slot for the flag zombie, then repeat
/// `kinds` from its first item. An empty input leaves the wave empty so normal
/// [`apply_spawn_list`] validation can reject it.
pub fn set_spawn_wave(spawn_list: &mut SpawnList, wave: usize, kinds: impl IntoIterator<Item = ZombieKind>) {
    let kinds = kinds.into_iter().collect::<Vec<_>>();
    if kinds.is_empty() {
        spawn_list.set_wave(wave, []);
        return;
    }
    let mut zombies = Vec::with_capacity(SPAWN_SLOTS_PER_WAVE);
    if (wave + 1).is_multiple_of(10) {
        zombies.push(ZombieKind::Flag);
    }
    zombies.extend(kinds.iter().copied().cycle().take(SPAWN_SLOTS_PER_WAVE - zombies.len()));
    spawn_list.set_wave(wave, zombies);
}

const fn is_average_fill_zombie(kind: ZombieKind) -> bool {
    !matches!(
        kind,
        ZombieKind::Flag
            | ZombieKind::BackupDancer
            | ZombieKind::Bobsled
            | ZombieKind::Yeti
            | ZombieKind::Bungee
            | ZombieKind::Imp
    )
}

/// Error returned when applying a configured spawn list to a backend.
#[derive(Debug, thiserror::Error)]
pub enum ApplySpawnListError {
    #[error("backend spawn-list operation failed: {0}")]
    Backend(crate::runtime::RuntimeError),
    #[error("spawn list must not be empty")]
    Empty,
    #[error("spawn list has {available} waves but backend requires {required}")]
    MissingWaves { available: usize, required: usize },
    #[error("spawn list is missing wave {wave}")]
    MissingWave { wave: usize },
    #[error("spawn list wave {wave} must not be empty")]
    EmptyWave { wave: usize },
    #[error("spawn list wave {wave} has {len} slots; maximum is {max}")]
    WaveTooLong { wave: usize, len: usize, max: usize },
}

/// Error returned while resolving and applying a general zombie spawn request.
#[derive(Debug, thiserror::Error)]
pub enum ApplyZombieSpawnRequestError {
    #[error(transparent)]
    Request(#[from] ZombieSpawnRequestError),
    #[error(transparent)]
    SpawnList(#[from] ApplySpawnListError),
    #[error("backend natural spawn-list operation failed: {0}")]
    Backend(crate::runtime::RuntimeError),
}

/// Applies Average or Exact by writing the schedule and Natural by delegating
/// weighted schedule construction to the game's native picker.
pub fn apply_zombie_spawn_request(request: &ZombieSpawnRequest, seed: u64) -> Result<(), ApplyZombieSpawnRequestError>
where
    rsvz_current::CurrentBackend: SceneBackend + SpawnScheduleBackend,
{
    crate::access::with_backend(|backend| {
        let scene = backend
            .scene()
            .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
        let types = resolve_zombie_spawn_types(request, scene, seed)?;
        match request.mode() {
            ZombieSpawnMode::Average => {
                let wave_count = backend
                    .spawn_wave_count()
                    .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
                let spawn_list = average_spawn_list(wave_count, types);
                apply_spawn_list(&spawn_list)?;
            }
            ZombieSpawnMode::Exact => {
                let wave_count = backend
                    .spawn_wave_count()
                    .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
                let mut spawn_list = SpawnList::new();
                for wave in 0..wave_count {
                    spawn_list.set_wave(wave, types.iter().copied());
                }
                apply_spawn_list(&spawn_list)?;
            }
            ZombieSpawnMode::Natural => {
                for kind in SPAWN_FLAG_KINDS {
                    backend.set_spawn_type_allowed(kind, false).map_err(|error| {
                        ApplyZombieSpawnRequestError::Backend(crate::access::operation_error(error))
                    })?;
                }
                backend
                    .set_spawn_type_allowed(ZombieKind::Normal, true)
                    .map_err(|error| ApplyZombieSpawnRequestError::Backend(crate::access::operation_error(error)))?;
                for kind in types {
                    backend.set_spawn_type_allowed(kind, true).map_err(|error| {
                        ApplyZombieSpawnRequestError::Backend(crate::access::operation_error(error))
                    })?;
                }
                backend
                    .pick_spawn_list()
                    .map_err(|error| ApplyZombieSpawnRequestError::Backend(crate::access::operation_error(error)))?;
            }
        }
        Ok(())
    })
}

/// Validates and applies a spawn list through the atomic spawn-schedule writes.
pub fn apply_spawn_list(spawn_list: &SpawnList) -> Result<(), ApplySpawnListError>
where
    rsvz_current::CurrentBackend: SpawnScheduleBackend,
{
    crate::access::with_backend(|backend| {
        if spawn_list.is_empty() {
            return Err(ApplySpawnListError::Empty);
        }

        let wave_count = backend
            .spawn_wave_count()
            .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
        if spawn_list.wave_count() < wave_count {
            return Err(ApplySpawnListError::MissingWaves {
                available: spawn_list.wave_count(),
                required: wave_count,
            });
        }

        for wave in 0..wave_count {
            let Some(len) = spawn_list.wave_len(wave) else {
                return Err(ApplySpawnListError::MissingWave { wave });
            };
            if len == 0 {
                return Err(ApplySpawnListError::EmptyWave { wave });
            }
            if len > SPAWN_SLOTS_PER_WAVE {
                return Err(ApplySpawnListError::WaveTooLong {
                    wave,
                    len,
                    max: SPAWN_SLOTS_PER_WAVE,
                });
            }
        }

        for kind in SPAWN_FLAG_KINDS {
            backend
                .set_spawn_type_allowed(kind, false)
                .map_err(|error| ApplySpawnListError::Backend(crate::access::operation_error(error)))?;
        }
        for kind in spawn_list.allowed_types().iter().copied() {
            backend
                .set_spawn_type_allowed(kind, true)
                .map_err(|error| ApplySpawnListError::Backend(crate::access::operation_error(error)))?;
        }

        for wave in 0..wave_count {
            let zombies = spawn_list.wave(wave).ok_or(ApplySpawnListError::MissingWave { wave })?;
            for slot in 0..SPAWN_SLOTS_PER_WAVE {
                let slot = SpawnWaveSlot::new(slot).expect("spawn loop is bounded by SPAWN_SLOTS_PER_WAVE");
                backend
                    .set_spawn_slot(wave, slot, zombies.get(slot.index()).copied())
                    .map_err(|error| ApplySpawnListError::Backend(crate::access::operation_error(error)))?;
            }
        }

        Ok(())
    })
}

/// Error returned by zombie spawn-layout helpers.
#[derive(Debug, thiserror::Error)]
pub enum EnsureZombieRowsError {
    /// Backend-specific operation failed.
    #[error("backend zombie spawn-layout operation failed: {0}")]
    Backend(crate::runtime::RuntimeError),
    /// This zombie kind is not supported by the row-moving algorithm.
    #[error("unsupported ensure-exist zombie kind: {0:?}")]
    UnsupportedKind(ZombieKind),
    /// Script-facing rows must be valid 1-based spawn rows for the scene.
    #[error("invalid ensure-exist row {row}; expected 1..={max_row}")]
    InvalidRow { row: i32, max_row: i32 },
    /// The requested row is not a valid spawn row for this kind in the current scene.
    #[error("zombie kind {kind:?} is disallowed on row {row} in scene {scene:?}")]
    RowDisallowed {
        kind: ZombieKind,
        row: i32,
        scene: SceneKind,
    },
    /// There are not enough fresh same-kind zombies to satisfy the target row set.
    #[error("insufficient fresh {kind:?} zombies to ensure row {row}")]
    InsufficientZombies { kind: ZombieKind, row: i32 },
    /// A backend-provided coordinate cannot be safely written through the native atom.
    #[error("computed zombie y coordinate is invalid: {0}")]
    InvalidCoordinate(CheckedValueError),
}

/// 判断僵尸是否是空中威胁。
#[must_use]
pub const fn is_flying_threat(kind: ZombieKind) -> bool {
    matches!(kind, ZombieKind::Balloon | ZombieKind::Bungee)
}

/// Moves a resolved zombie while preserving its current vertical offset from the row baseline.
#[derive(Debug, thiserror::Error)]
pub enum MoveZombieRowError {
    #[error("zombie row move backend operation failed: {0}")]
    Backend(crate::runtime::RuntimeError),
    #[error("computed zombie y coordinate is invalid: {0}")]
    InvalidCoordinate(CheckedValueError),
}

pub fn move_zombie_to_row_by_id(id: ZombieId, target_row: i32) -> Result<bool, MoveZombieRowError>
where
    rsvz_current::CurrentBackend: ZombiePositionWriteBackend,
{
    crate::access::with_backend(|backend| {
        let Some(zombie) = crate::live_value::read_or_abort(backend.zombie(id), "zombie") else {
            return Ok(false);
        };
        let old_row = backend.zombie_row(zombie);
        let old_y = backend.zombie_pos_y(zombie);
        let old_base = backend
            .zombie_pos_y_based_on_row(zombie, old_row)
            .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
        let new_base = backend
            .zombie_pos_y_based_on_row(zombie, target_row)
            .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
        let y = I32RepresentableF32::new(old_y + new_base - old_base).map_err(MoveZombieRowError::InvalidCoordinate)?;
        backend
            .set_zombie_row_and_y(zombie, target_row, y)
            .map_err(|error| MoveZombieRowError::Backend(crate::access::operation_error(error)))?;
        Ok(true)
    })
}

/// Ensures a fresh zombie of `kind` exists on each requested 1-based row.
pub fn ensure_zombie_rows_one_based<R>(kind: ZombieKind, rows: R) -> Result<(), EnsureZombieRowsError>
where
    rsvz_current::CurrentBackend: SceneBackend + GridTerrainBackend + ZombiePositionWriteBackend,
    R: IntoIterator<Item = i32>,
{
    crate::access::with_backend(|backend| {
        validate_supported_kind(kind)?;
        let scene = backend
            .scene()
            .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
        let max_row = i32::try_from(scene.row_count()).unwrap_or(i32::MAX);
        let mut target_rows = [-1; 6];
        let mut target_count = 0;
        for row in rows {
            validate_target_row(scene, kind, row, max_row)?;
            let row_zero_based = row - 1;
            if !backend
                .row_can_have_zombies(row_zero_based)
                .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()))
            {
                return Err(EnsureZombieRowsError::RowDisallowed { kind, row, scene });
            }
            if !target_rows[..target_count].contains(&row_zero_based) {
                target_rows[target_count] = row_zero_based;
                target_count += 1;
            }
        }

        let target_rows = &target_rows[..target_count];
        let mut by_row =
            count_fresh_zombies(scene, kind, max_row).map_err(|error| EnsureZombieRowsError::Backend(error.into()))?;

        for &target_row in target_rows {
            let target_index = usize::try_from(target_row).map_err(|_error| EnsureZombieRowsError::InvalidRow {
                row: target_row + 1,
                max_row,
            })?;
            if by_row[target_index] != 0 {
                continue;
            }

            let Some(source_index) = best_source_row(&by_row, target_rows, target_row) else {
                return Err(EnsureZombieRowsError::InsufficientZombies {
                    kind,
                    row: target_row + 1,
                });
            };
            let zombie = find_fresh_zombie_in_row(scene, kind, source_index as i32)
                .map_err(|error| EnsureZombieRowsError::Backend(error.into()))?
                .ok_or(EnsureZombieRowsError::InsufficientZombies {
                    kind,
                    row: target_row + 1,
                })?;
            let moved = move_zombie_to_row_by_id(zombie, target_row).map_err(|error| match error {
                MoveZombieRowError::Backend(error) => EnsureZombieRowsError::Backend(error),
                MoveZombieRowError::InvalidCoordinate(error) => EnsureZombieRowsError::InvalidCoordinate(error),
            })?;
            if !moved {
                return Err(EnsureZombieRowsError::InsufficientZombies {
                    kind,
                    row: target_row + 1,
                });
            }
            by_row[source_index] -= 1;
            by_row[target_index] += 1;
        }

        Ok(())
    })
}

fn validate_supported_kind(kind: ZombieKind) -> Result<(), EnsureZombieRowsError> {
    if matches!(
        kind,
        ZombieKind::BackupDancer | ZombieKind::Bobsled | ZombieKind::Bungee | ZombieKind::Imp
    ) {
        return Err(EnsureZombieRowsError::UnsupportedKind(kind));
    }
    Ok(())
}

fn validate_target_row(
    scene: SceneKind, kind: ZombieKind, row: i32, max_row: i32,
) -> Result<(), EnsureZombieRowsError> {
    if !(1..=max_row).contains(&row) {
        return Err(EnsureZombieRowsError::InvalidRow { row, max_row });
    }
    if zombie_row_allowed_for_kind(scene, kind, row - 1) {
        Ok(())
    } else {
        Err(EnsureZombieRowsError::RowDisallowed { kind, row, scene })
    }
}

fn count_fresh_zombies(
    scene: SceneKind, kind: ZombieKind, max_row: i32,
) -> Result<[usize; 6], crate::runtime::RuntimeError>
where
    rsvz_current::CurrentBackend: ZombieReadBackend,
{
    crate::access::with_backend(|backend| {
        let mut by_row = [0; 6];
        for zombie in crate::live_value::read_or_abort(backend.zombies(), "zombies") {
            let zombie_kind = crate::live_value::read_or_abort(backend.zombie_kind(zombie), "zombie_kind");
            let row = backend.zombie_row(zombie);
            if zombie_kind != kind
                || backend.zombie_age(zombie) > NEWLY_SPAWNED_ZOMBIE_MAX_AGE
                || !zombie_row_allowed_for_source(scene, zombie_kind, row)
            {
                continue;
            }
            let Ok(row) = usize::try_from(row) else {
                continue;
            };
            if row < usize::try_from(max_row.max(0)).unwrap_or(0) {
                by_row[row] += 1;
            }
        }
        Ok(by_row)
    })
}

fn find_fresh_zombie_in_row(
    scene: SceneKind, kind: ZombieKind, source_row: i32,
) -> Result<Option<ZombieId>, crate::runtime::RuntimeError>
where
    rsvz_current::CurrentBackend: ZombieReadBackend,
{
    crate::access::with_backend(|backend| {
        for zombie in crate::live_value::read_or_abort(backend.zombies(), "zombies") {
            let zombie_kind = crate::live_value::read_or_abort(backend.zombie_kind(zombie), "zombie_kind");
            if zombie_kind == kind
                && backend.zombie_row(zombie) == source_row
                && backend.zombie_age(zombie) <= NEWLY_SPAWNED_ZOMBIE_MAX_AGE
                && zombie_row_allowed_for_source(scene, zombie_kind, source_row)
            {
                return Ok(Some(backend.zombie_id(zombie)));
            }
        }
        Ok(None)
    })
}

fn best_source_row(by_row: &[usize], target_rows: &[i32], target_row: i32) -> Option<usize> {
    let non_target = by_row
        .iter()
        .enumerate()
        .filter(|(row, count)| !target_rows.iter().any(|target| usize::try_from(*target) == Ok(*row)) && **count != 0)
        .max_by_key(|(_row, count)| **count)
        .map(|(row, _count)| row);
    non_target.or_else(|| {
        target_rows
            .iter()
            .filter_map(|row| usize::try_from(*row).ok())
            .filter(|row| i32::try_from(*row) != Ok(target_row))
            .filter(|row| by_row.get(*row).is_some_and(|count| *count > 1))
            .max_by_key(|row| by_row[*row])
    })
}

/// Legacy placeholder predicate; this only checks row, HP, and a nonnegative threshold.
pub fn is_near_home(id: ZombieId, home_col_threshold: i32) -> Result<bool, crate::runtime::RuntimeError>
where
    rsvz_current::CurrentBackend: ZombieReadBackend,
{
    crate::access::with_backend(|backend| {
        let Some(zombie) = crate::live_value::read_or_abort(backend.zombie(id), "zombie") else {
            return Ok(false);
        };
        Ok(backend.zombie_row(zombie) >= 0 && backend.zombie_hp(zombie) > 0 && home_col_threshold >= 0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_row_selection_preserves_targets_and_prefers_larger_external_groups() {
        assert_eq!(best_source_row(&[0, 2, 0, 0, 1, 0], &[0, 1], 0), Some(4));
        assert_eq!(best_source_row(&[0, 2, 0, 0, 0, 0], &[0, 1], 0), Some(1));
        assert_eq!(best_source_row(&[0, 1, 0, 0, 0, 0], &[0, 1], 0), None);
        assert_eq!(best_source_row(&[0, 0, 0, 0, 2, 2], &[0], 0), Some(5));
        assert!(matches!(
            validate_supported_kind(ZombieKind::Imp),
            Err(EnsureZombieRowsError::UnsupportedKind(_))
        ));
        assert!(matches!(
            validate_target_row(SceneKind::Pool, ZombieKind::Normal, 0, 6),
            Err(EnsureZombieRowsError::InvalidRow { .. })
        ));
        assert!(matches!(
            validate_target_row(SceneKind::Pool, ZombieKind::Normal, 3, 6),
            Err(EnsureZombieRowsError::RowDisallowed { .. })
        ));
    }

    #[test]
    fn exact_spawn_rejects_more_than_one_native_wave() {
        let selection = ZombieTypeSelection::Exact(vec![ZombieKind::Normal; SPAWN_SLOTS_PER_WAVE + 1]);
        assert_eq!(
            ZombieSpawnRequest::new(selection, ZombieSpawnMode::Exact),
            Err(ZombieSpawnRequestError::TooManyExactZombies {
                len: SPAWN_SLOTS_PER_WAVE + 1,
                max: SPAWN_SLOTS_PER_WAVE,
            })
        );
    }

    #[test]
    fn constrained_random_selection_keeps_required_types_and_excludes_banned_types() {
        let selection = random_zombie_types([ZombieKind::Gargantuar], [ZombieKind::Football]);
        let request =
            ZombieSpawnRequest::new(selection, ZombieSpawnMode::Average).expect("valid constrained selection");
        let first = resolve_zombie_spawn_types(&request, SceneKind::Pool, 123).expect("first selection");
        let second = resolve_zombie_spawn_types(&request, SceneKind::Pool, 123).expect("second selection");

        assert_eq!(first, second);
        assert!(first.contains(&ZombieKind::Gargantuar));
        assert!(!first.contains(&ZombieKind::Football));
    }

    #[test]
    fn zombie_row_allowed_for_kind_matches_backend_spawn_rules() {
        assert!(zombie_row_allowed_for_kind(SceneKind::Pool, ZombieKind::Normal, 0));
        assert!(!zombie_row_allowed_for_kind(SceneKind::Pool, ZombieKind::Normal, 2));
        assert!(zombie_row_allowed_for_kind(SceneKind::Pool, ZombieKind::Snorkel, 2));
        assert!(!zombie_row_allowed_for_kind(SceneKind::Pool, ZombieKind::Snorkel, 0));
        assert!(zombie_row_allowed_for_kind(SceneKind::Pool, ZombieKind::Balloon, 2));

        assert!(!zombie_row_allowed_for_kind(SceneKind::Day, ZombieKind::Dancing, 0));
        assert!(zombie_row_allowed_for_kind(SceneKind::Day, ZombieKind::Dancing, 2));
        assert!(!zombie_row_allowed_for_kind(SceneKind::Day, ZombieKind::Dancing, 4));
        assert!(!zombie_row_allowed_for_kind(SceneKind::Night, ZombieKind::Zomboni, 2));
        assert!(zombie_row_allowed_for_kind(SceneKind::Day, ZombieKind::Zomboni, 2));
        assert!(!zombie_row_allowed_for_kind(SceneKind::Roof, ZombieKind::Digger, 2));
        assert!(!zombie_row_allowed_for_kind(SceneKind::Roof, ZombieKind::Dancing, 2));
        assert!(zombie_row_allowed_for_kind(SceneKind::Roof, ZombieKind::Normal, 2));
    }

    #[test]
    fn average_spawn_list_fills_twenty_waves_with_fixed_slots() {
        let spawn = average_spawn_list(
            crate::model::DEFAULT_SPAWN_WAVES,
            [ZombieKind::Normal, ZombieKind::Buckethead],
        );

        assert_eq!(spawn.wave_count(), crate::model::DEFAULT_SPAWN_WAVES);
        assert!(spawn.waves().all(|wave| wave.len() == SPAWN_SLOTS_PER_WAVE));
        assert_eq!(
            spawn.wave(0).and_then(|wave| wave.first()).copied(),
            Some(ZombieKind::Normal)
        );
        assert_eq!(
            spawn.wave(0).and_then(|wave| wave.get(1)).copied(),
            Some(ZombieKind::Buckethead)
        );
        assert_eq!(
            spawn.wave(9).and_then(|wave| wave.first()).copied(),
            Some(ZombieKind::Flag)
        );
        assert_eq!(
            spawn.wave(19).and_then(|wave| wave.first()).copied(),
            Some(ZombieKind::Flag)
        );
        assert_eq!(
            spawn.allowed_types(),
            [ZombieKind::Normal, ZombieKind::Buckethead].as_slice()
        );
    }

    #[test]
    fn average_spawn_list_filters_non_fill_zombies() {
        let spawn = average_spawn_list(
            1,
            [
                ZombieKind::Flag,
                ZombieKind::BackupDancer,
                ZombieKind::Normal,
                ZombieKind::Imp,
                ZombieKind::Buckethead,
            ],
        );
        let wave = spawn.wave(0).unwrap_or(&[]);

        assert_eq!(wave.first().copied(), Some(ZombieKind::Normal));
        assert_eq!(wave.get(1).copied(), Some(ZombieKind::Buckethead));
        assert!(
            wave.iter()
                .all(|kind| matches!(kind, ZombieKind::Normal | ZombieKind::Buckethead))
        );
        assert_eq!(
            spawn.allowed_types(),
            [
                ZombieKind::Flag,
                ZombieKind::BackupDancer,
                ZombieKind::Normal,
                ZombieKind::Imp,
                ZombieKind::Buckethead,
            ]
            .as_slice()
        );
    }

    #[test]
    fn average_spawn_list_adds_bungee_after_huge_wave_flags_when_requested() {
        let spawn = average_spawn_list(
            crate::model::DEFAULT_SPAWN_WAVES,
            [ZombieKind::Normal, ZombieKind::Bungee, ZombieKind::Buckethead],
        );
        let wave_10 = spawn.wave(9).unwrap_or(&[]);
        let wave_20 = spawn.wave(19).unwrap_or(&[]);

        assert_eq!(wave_10.first().copied(), Some(ZombieKind::Flag));
        assert_eq!(wave_20.first().copied(), Some(ZombieKind::Flag));
        assert_eq!(wave_10.get(1..5), Some([ZombieKind::Bungee; 4].as_slice()));
        assert_eq!(wave_20.get(1..5), Some([ZombieKind::Bungee; 4].as_slice()));
        assert_eq!(
            spawn.allowed_types(),
            [ZombieKind::Normal, ZombieKind::Bungee, ZombieKind::Buckethead].as_slice()
        );
    }

    #[test]
    fn set_spawn_wave_restarts_the_pattern_after_the_huge_wave_flag() {
        let mut spawn = average_spawn_list(
            crate::model::DEFAULT_SPAWN_WAVES,
            [ZombieKind::Gargantuar, ZombieKind::GigaGargantuar],
        );

        set_spawn_wave(
            &mut spawn,
            19,
            [ZombieKind::Normal, ZombieKind::PoleVaulting, ZombieKind::Conehead],
        );

        let wave = spawn.wave(19).unwrap_or(&[]);
        assert_eq!(wave.len(), SPAWN_SLOTS_PER_WAVE);
        assert_eq!(
            wave.get(..7),
            Some(
                [
                    ZombieKind::Flag,
                    ZombieKind::Normal,
                    ZombieKind::PoleVaulting,
                    ZombieKind::Conehead,
                    ZombieKind::Normal,
                    ZombieKind::PoleVaulting,
                    ZombieKind::Conehead,
                ]
                .as_slice()
            )
        );
    }
}

use crate::model::{I32RepresentableF32, ZombieId};
