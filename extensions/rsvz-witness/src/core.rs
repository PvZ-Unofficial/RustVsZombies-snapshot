//! Deterministic, backend-neutral witness capture and artifact comparison.

use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
#[cfg(feature = "tooling")]
use std::io::{BufRead, BufReader, Read};
#[cfg(feature = "tooling")]
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use miniz_oxide::deflate::core::{
    CompressorOxide, TDEFLFlush, TDEFLStatus, compress_to_output, create_comp_flags_from_zip_params,
};
#[cfg(test)]
use miniz_oxide::inflate::decompress_to_vec_zlib;
#[cfg(feature = "tooling")]
use miniz_oxide::inflate::decompress_to_vec_zlib_with_limit;
use serde::{Deserialize, Serialize};

use rsvz::core::backend::{
    BackendIdentityBackend, BattleStatusBackend, BoardStateBackend, ClockBackend, CurrentWaveBackend, GameUiBackend,
    GridItemStateBackend, PlantStateBackend, ProjectileReadBackend, RandomControlBackend, SceneBackend,
    SeedCooldownReadBackend, SunQueryBackend, WaveHealthBackend, WaveTimingBackend, ZombieRawFactsBackend,
    ZombieStateBackend,
};
use rsvz::core::logic::ZombieTypeSelection;
#[cfg(test)]
use rsvz::core::model::RandomMode;
use rsvz::core::model::{
    CardSelection, ContactRect, GameEvent, Grid, PlantEffect, PlantEffectOutcome, PlantEffectSource, RandomStreamKind,
    SunAmount, SunProductionMode, WorldResetConfig,
};
use rsvz::core::setup::ScriptSetup;
use rsvz::runtime::{RuntimeError, RuntimeResult};
use rsvz::{LineupBase, LineupPlant, SessionArtifact};

pub const WITNESS_ARTIFACT_VERSION: u32 = 2;
pub const WITNESS_SCHEMA_VERSION: u32 = 6;
pub const WITNESS_HASH: &str = "fnv1a-128-v1";
pub const WITNESS_ENCODING: &str = "tag-u16-len-u32-le-v1";

const FRAME_CAPACITY: usize = 4 * 1024 * 1024;
const CHUNK_CAPACITY: usize = 4 * 1024 * 1024;
const COMPRESSED_CAPACITY: usize = CHUNK_CAPACITY + CHUNK_CAPACITY / 8 + 4096;
const BASE64_CAPACITY: usize = COMPRESSED_CAPACITY.div_ceil(3) * 4;
const CHUNK_FRAMES: usize = 200;
const DIGEST_CHUNK_CAPACITY: usize = CHUNK_FRAMES * 16;
#[cfg(feature = "tooling")]
const DIGEST_BASE64_CAPACITY: usize = DIGEST_CHUNK_CAPACITY.div_ceil(3) * 4;
const ARTIFACT_LINE_CAPACITY: usize = BASE64_CAPACITY + 64 * 1024;
const ID_CAPACITY: usize = 8192;

/// Gameplay schema v6 table. Each payload's member order is the order shown in `path`.
///
/// Native evidence is adjacent by design: 1051 entries name the verified raw board/object
/// layouts and PE entries name the corresponding `Scene`/object fields. Unsupported reads are
/// errors; no table entry has a synthetic default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WitnessSchemaField {
    pub tag: u16,
    pub path: &'static str,
    pub capability: &'static str,
    pub representation: &'static str,
    pub evidence_1051: &'static str,
    pub evidence_pe: &'static str,
}

macro_rules! field {
    ($tag:literal, $path:literal, $cap:literal, $repr:literal, $native:literal, $pe:literal) => {
        WitnessSchemaField {
            tag: $tag,
            path: $path,
            capability: $cap,
            representation: $repr,
            evidence_1051: $native,
            evidence_pe: $pe,
        }
    };
}

pub const WITNESS_SCHEMA_FIELDS: &[WitnessSchemaField] = &[
    field!(
        1,
        "schema.version",
        "canonical",
        "u32",
        "visitor constant",
        "visitor constant"
    ),
    field!(
        2,
        "board.{ui,battle_status,scene,clock,wave,sun}",
        "GameUi/BattleStatus/Scene/Clock/CurrentWave/SunQuery",
        "i32/u32 LE",
        "Board raw layout + LawnApp UI",
        "Scene/UI scalars"
    ),
    field!(
        3,
        "board.wave_timing.{total,refresh,initial,huge,end}",
        "WaveTimingBackend",
        "tagged option i32",
        "Board timing offsets",
        "Spawn/round timers"
    ),
    field!(
        4,
        "board.wave_health.{start,current}",
        "WaveHealthBackend",
        "i32 LE",
        "Board health thresholds",
        "Scene wave health"
    ),
    field!(
        5,
        "board.sun.{generated,countdown}",
        "BoardStateBackend",
        "i32 LE",
        "Board sun offsets",
        "SunData"
    ),
    field!(
        6,
        "board.dancer_clock",
        "BoardStateBackend",
        "u32 LE",
        "reset-controlled LawnApp mAppCounter",
        "Scene zombie_dancing_clock"
    ),
    field!(
        7,
        "board.ice_path[6].{x,countdown}",
        "BoardStateBackend",
        "i32/u32 LE",
        "Board mIceMinX/mIceTimer",
        "IcePathData"
    ),
    field!(
        8,
        "board.spawn_allowed[33]",
        "BoardStateBackend",
        "bool byte",
        "Board mZombieAllowed",
        "SpawnData.spawn_flags"
    ),
    field!(
        9,
        "board.spawn_table[20][50]",
        "BoardStateBackend",
        "i32 LE",
        "Board mZombiesInWave",
        "SpawnData waves"
    ),
    field!(
        11,
        "board.seeds[].{slot,packet,imitator,cooldown,usable}",
        "SeedCooldownReadBackend",
        "i32/u32 LE",
        "SeedBank/SeedPacket",
        "SeedArray"
    ),
    field!(
        30,
        "plants[].{witness_id,kind,grid,pos,size,state,hp,target,direction,collision,imitater,sleeping,timers}",
        "PlantStateBackend",
        "integer/f32 bits LE",
        "Plant raw layout + GetPlantRect",
        "Plant fields + attack_box"
    ),
    field!(
        40,
        "zombies[].{witness_id,kind,phase,action,row,pos,speed,size,wave,age,collision,hp,accessories,relations,target,timers,lifecycle,reanim}",
        "ZombieStateBackend+ZombieRawFactsBackend",
        "integer/f32 bits LE",
        "Zombie raw layout + GetZombieRect",
        "Zombie fields + native hit box"
    ),
    field!(
        50,
        "projectiles[].{witness_id,kind,motion,grid,pos,velocity,acceleration,collision,flags,timers,relations}",
        "ProjectileReadBackend",
        "integer/f32 bits LE",
        "Projectile raw layout + GetProjectileRect",
        "Projectile fields + native attack box"
    ),
    field!(
        60,
        "grid_items[].{witness_id,kind,grid,countdown}",
        "GridItemStateBackend",
        "integer/f32 bits LE",
        "GridItem raw layout",
        "GridItem fields"
    ),
    field!(
        80,
        "random.{stream,seed,locked,fixed}",
        "RandomControlBackend",
        "integer LE; seed and mode only",
        "global/local MTRand hooks",
        "battle/level random_stream"
    ),
    field!(
        90,
        "events[]",
        "public GameEvent observer",
        "stable discriminants + integer LE",
        "native completion bridge",
        "PE native event sink"
    ),
    field!(
        91,
        "transition.outcome",
        "TickTask dispatch outcome",
        "u8",
        "runtime dispatch",
        "runtime dispatch"
    ),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WitnessCapture {
    Full,
    Digest,
    ReplayWindow { start: u64, end: u64 },
}

impl WitnessCapture {
    #[must_use]
    pub const fn captures(self, frame: u64) -> bool {
        match self {
            Self::Full => true,
            Self::Digest => false,
            Self::ReplayWindow { start, end } => frame >= start && frame <= end,
        }
    }

    pub fn validate(self) -> Result<(), WitnessError> {
        if let Self::ReplayWindow { start, end } = self
            && start > end
        {
            return Err(WitnessError::InvalidOptions("ReplayWindow start exceeds end"));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WitnessLimit {
    Frames(u64),
    CompletedRounds(u64),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WitnessRepro {
    pub case_id: String,
    pub scenario_seed: Option<u64>,
    pub script_id: String,
    pub build_id: String,
    pub expanded_setup: serde_json::Value,
}

impl Default for WitnessRepro {
    fn default() -> Self {
        Self {
            case_id: "default".to_owned(),
            scenario_seed: None,
            script_id: "unspecified".to_owned(),
            build_id: env!("CARGO_PKG_VERSION").to_owned(),
            expanded_setup: serde_json::json!({"plants": [], "zombies": []}),
        }
    }
}

impl WitnessRepro {
    pub fn set_expanded_setup_json(&mut self, json: &str) -> Result<(), WitnessError> {
        self.expanded_setup =
            serde_json::from_str(json).map_err(|error| WitnessError::InvalidRepro(error.to_string()))?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WitnessOptions {
    pub reset: WorldResetConfig,
    pub locked_random: u32,
    pub wave_spawn_random: bool,
    pub sun: SunProductionMode,
    pub capture: WitnessCapture,
    pub limit: WitnessLimit,
    pub repro: WitnessRepro,
}

impl Default for WitnessOptions {
    fn default() -> Self {
        Self {
            reset: WorldResetConfig {
                completed_rounds: 63,
                seed: 0x5eed_1051,
                ..WorldResetConfig::default()
            },
            locked_random: 0x5eed_1051,
            wave_spawn_random: false,
            sun: SunProductionMode::DirectCredit,
            capture: WitnessCapture::Full,
            limit: WitnessLimit::Frames(100),
            repro: WitnessRepro::default(),
        }
    }
}

impl WitnessOptions {
    pub fn validate(&self) -> Result<(), WitnessError> {
        self.capture.validate()?;
        if SunAmount::new(self.reset.initial_sun).is_err() {
            return Err(WitnessError::InvalidOptions("initial_sun exceeds i32::MAX"));
        }
        let Some(expanded) = self.repro.expanded_setup.as_object() else {
            return Err(WitnessError::InvalidRepro(
                "expanded setup must be a JSON object".to_owned(),
            ));
        };
        if !expanded.get("plants").is_some_and(serde_json::Value::is_array)
            || !expanded.get("zombies").is_some_and(serde_json::Value::is_array)
        {
            return Err(WitnessError::InvalidRepro(
                "expanded setup plants and zombies must be arrays".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WitnessError {
    #[error("invalid witness options: {0}")]
    InvalidOptions(&'static str),
    #[error("invalid expanded reproduction setup: {0}")]
    InvalidRepro(String),
    #[error("witness read {path} failed: {message}")]
    Read { path: &'static str, message: String },
    #[error("witness fixed buffer overflow at {0}")]
    Overflow(&'static str),
    #[error("witness artifact I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("witness artifact encoding failed: {0}")]
    Encoding(&'static str),
}

pub(super) fn read_error(path: &'static str, error: impl std::fmt::Display) -> WitnessError {
    WitnessError::Read {
        path,
        message: error.to_string(),
    }
}

pub trait WitnessCapabilities:
    BackendIdentityBackend
    + BattleStatusBackend
    + BoardStateBackend
    + ClockBackend
    + CurrentWaveBackend
    + GameUiBackend
    + GridItemStateBackend
    + PlantStateBackend
    + ProjectileReadBackend
    + RandomControlBackend
    + SceneBackend
    + SeedCooldownReadBackend
    + SunQueryBackend
    + WaveHealthBackend
    + WaveTimingBackend
    + ZombieRawFactsBackend
    + ZombieStateBackend
{
}

impl<T> WitnessCapabilities for T where
    T: BackendIdentityBackend
        + BattleStatusBackend
        + BoardStateBackend
        + ClockBackend
        + CurrentWaveBackend
        + GameUiBackend
        + GridItemStateBackend
        + PlantStateBackend
        + ProjectileReadBackend
        + RandomControlBackend
        + SceneBackend
        + SeedCooldownReadBackend
        + SunQueryBackend
        + WaveHealthBackend
        + WaveTimingBackend
        + ZombieRawFactsBackend
        + ZombieStateBackend
{
}

#[derive(Clone, Copy)]
struct StableHasher(u128);

impl StableHasher {
    const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;

    const fn new() -> Self {
        Self(Self::OFFSET)
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u128::from(*byte);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    const fn finish(self) -> [u8; 16] {
        self.0.to_le_bytes()
    }
}

struct Encoder<'a> {
    hasher: StableHasher,
    full: Option<&'a mut Vec<u8>>,
}

impl<'a> Encoder<'a> {
    fn new(full: Option<&'a mut Vec<u8>>) -> Self {
        Self {
            hasher: StableHasher::new(),
            full,
        }
    }

    fn emit(&mut self, tag: u16, payload: &[u8]) -> Result<(), WitnessError> {
        let len = u32::try_from(payload.len()).map_err(|_error| WitnessError::Overflow("field payload"))?;
        let tag = tag.to_le_bytes();
        let len = len.to_le_bytes();
        self.hasher.write(&tag);
        self.hasher.write(&len);
        self.hasher.write(payload);
        if let Some(full) = &mut self.full {
            let required = 6usize.saturating_add(payload.len());
            if full.len().saturating_add(required) > FRAME_CAPACITY {
                return Err(WitnessError::Overflow("frame"));
            }
            full.extend_from_slice(&tag);
            full.extend_from_slice(&len);
            full.extend_from_slice(payload);
        }
        Ok(())
    }
}

// Borrow-local scratch contains only Copy handles, never object snapshots or retained addresses.
// Native generation order supplies creation order; physical pool slots are not canonical order.
fn with_sorted_entities<T: Copy, R>(
    entities: impl Iterator<Item = T>, key: impl Fn(T) -> u32, visit: impl FnOnce(&[T]) -> Result<R, WitnessError>,
) -> Result<R, WitnessError> {
    let mut scratch = [const { std::mem::MaybeUninit::<T>::uninit() }; ID_CAPACITY];
    let mut len = 0;
    for entity in entities {
        scratch
            .get_mut(len)
            .ok_or(WitnessError::Overflow("live entity count"))?
            .write(entity);
        len += 1;
    }
    // SAFETY: exactly the first len slots were initialized. T is Copy, and neither this
    // slice nor a handle is retained in the visitor across a world update.
    let entities = unsafe { std::slice::from_raw_parts_mut(scratch.as_mut_ptr().cast::<T>(), len) };
    entities.sort_unstable_by_key(|entity| key(*entity));
    visit(entities)
}

struct WitnessIdMap {
    slots: Vec<Option<(u64, u32, u32)>>,
    next: u32,
    birth_epoch: Option<u64>,
    last_birth_key: u16,
}

impl WitnessIdMap {
    fn with_capacity() -> Self {
        Self {
            slots: vec![None; ID_CAPACITY],
            next: 1,
            birth_epoch: None,
            last_birth_key: 0,
        }
    }

    fn birth_origin(&self, epoch: u64) -> u16 {
        if self.birth_epoch == Some(epoch) {
            self.last_birth_key
        } else {
            0
        }
    }

    fn id(&mut self, epoch: u64, raw: u32) -> Result<u32, WitnessError> {
        let slot = self
            .slots
            .get_mut((raw & 0xffff) as usize)
            .ok_or(WitnessError::Overflow("logical object identity slot"))?;
        if let Some((known_epoch, known_raw, id)) = slot
            && *known_epoch == epoch
            && *known_raw == raw
        {
            return Ok(*id);
        }
        let id = self.next;
        self.next = self
            .next
            .checked_add(1)
            .ok_or(WitnessError::Overflow("logical object id"))?;
        *slot = Some((epoch, raw, id));
        self.birth_epoch = Some(epoch);
        self.last_birth_key = (raw >> 16) as u16;
        Ok(id)
    }

    fn get(&self, epoch: u64, raw: u32) -> Option<u32> {
        self.slots
            .get((raw & 0xffff) as usize)
            .and_then(|slot| *slot)
            .and_then(|(known_epoch, known_raw, id)| (known_epoch == epoch && known_raw == raw).then_some(id))
    }
}

struct WitnessIds {
    plants: WitnessIdMap,
    zombies: WitnessIdMap,
    projectiles: WitnessIdMap,
    grid_items: WitnessIdMap,
}

impl WitnessIds {
    fn with_capacity() -> Self {
        Self {
            plants: WitnessIdMap::with_capacity(),
            zombies: WitnessIdMap::with_capacity(),
            projectiles: WitnessIdMap::with_capacity(),
            grid_items: WitnessIdMap::with_capacity(),
        }
    }
}

pub struct CapturedFrame {
    pub frame: u64,
    records: Vec<u8>,
    full: bool,
    hasher: StableHasher,
}

impl CapturedFrame {
    pub fn finish(mut self, outcome: WitnessTransitionOutcome) -> Result<FinishedFrame, WitnessError> {
        let mut encoder = Encoder {
            hasher: self.hasher,
            full: self.full.then_some(&mut self.records),
        };
        encoder.emit(91, &[outcome as u8])?;
        Ok(FinishedFrame {
            frame: self.frame,
            digest: encoder.hasher.finish(),
            records: self.records,
            full: self.full,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum WitnessTransitionOutcome {
    #[default]
    Clean = 0,
    RecoverableError = 1,
    TimingViolation = 2,
}

pub struct FinishedFrame {
    pub frame: u64,
    pub digest: [u8; 16],
    records: Vec<u8>,
    full: bool,
}

pub struct CanonicalVisitor {
    ids: WitnessIds,
    frame: Vec<u8>,
    payload: Vec<u8>,
}

impl CanonicalVisitor {
    #[must_use]
    pub fn new() -> Self {
        Self {
            ids: WitnessIds::with_capacity(),
            frame: Vec::with_capacity(FRAME_CAPACITY),
            payload: Vec::with_capacity(FRAME_CAPACITY / 2),
        }
    }

    pub fn recycle(&mut self, mut frame: Vec<u8>) {
        frame.clear();
        self.frame = frame;
    }

    pub fn capture<B: WitnessCapabilities>(
        &mut self, backend: &B, world_epoch: u64, frame: u64, events: &[GameEvent], full: bool,
    ) -> Result<CapturedFrame, WitnessError> {
        self.frame.clear();
        let mut encoder = Encoder::new(full.then_some(&mut self.frame));
        encoder.emit(1, &WITNESS_SCHEMA_VERSION.to_le_bytes())?;

        self.payload.clear();
        put_i32(
            &mut self.payload,
            backend.game_ui().map_err(|error| read_error("board.ui", error))?.code(),
        );
        put_i32(
            &mut self.payload,
            battle_status_code(
                backend
                    .battle_status()
                    .map_err(|error| read_error("board.battle_status", error))?,
            ),
        );
        put_i32(
            &mut self.payload,
            backend
                .scene()
                .map_err(|error| read_error("board.scene", error))?
                .code(),
        );
        put_i32(
            &mut self.payload,
            backend.clock().map_err(|error| read_error("board.clock", error))?,
        );
        put_i32(
            &mut self.payload,
            backend
                .current_wave()
                .map_err(|error| read_error("board.wave", error))?
                .0,
        );
        put_u32(
            &mut self.payload,
            backend.sun().map_err(|error| read_error("board.sun", error))?,
        );
        encoder.emit(2, &self.payload)?;

        let total_waves = backend
            .total_waves()
            .map_err(|error| read_error("board.wave_timing", error))?;
        let refresh_countdown = backend
            .refresh_countdown()
            .map_err(|error| read_error("board.wave_timing", error))?;
        let initial_countdown = backend
            .initial_countdown()
            .map_err(|error| read_error("board.wave_timing", error))?;
        let huge_wave_countdown = backend
            .huge_wave_countdown()
            .map_err(|error| read_error("board.wave_timing", error))?;
        let level_end_countdown = backend
            .level_end_countdown()
            .map_err(|error| read_error("board.wave_timing", error))?;
        self.payload.clear();
        put_option_i32(&mut self.payload, Some(total_waves));
        put_option_i32(&mut self.payload, Some(refresh_countdown));
        put_option_i32(&mut self.payload, Some(initial_countdown));
        put_option_i32(&mut self.payload, Some(huge_wave_countdown));
        put_option_i32(&mut self.payload, Some(level_end_countdown));
        encoder.emit(3, &self.payload)?;

        self.payload.clear();
        put_i32(
            &mut self.payload,
            backend
                .zombie_health_wave_start()
                .map_err(|error| read_error("board.wave_health.start", error))?,
        );
        let current_wave_health = match backend
            .current_wave()
            .map_err(|error| read_error("board.wave_timing", error))?
            .0
            .checked_sub(1)
        {
            Some(wave) if wave >= 0 => backend
                .total_zombies_health_in_wave(wave)
                .map_err(|error| read_error("board.wave_health.current", error))?,
            _ => 0,
        };
        put_i32(&mut self.payload, current_wave_health);
        encoder.emit(4, &self.payload)?;

        self.payload.clear();
        put_i32(
            &mut self.payload,
            backend
                .natural_sun_generated()
                .map_err(|error| read_error("board.natural_sun", error))?,
        );
        put_i32(
            &mut self.payload,
            backend
                .natural_sun_countdown()
                .map_err(|error| read_error("board.natural_sun", error))?,
        );
        encoder.emit(5, &self.payload)?;
        encoder.emit(
            6,
            &backend
                .dancer_clock()
                .map_err(|error| read_error("board.dancer_clock", error))?
                .to_le_bytes(),
        )?;

        self.payload.clear();
        for row in 0..6 {
            put_i32(
                &mut self.payload,
                backend
                    .ice_path_x(row)
                    .map_err(|error| read_error("board.ice_path.x", error))?,
            );
            put_u32(
                &mut self.payload,
                backend
                    .ice_path_countdown(row)
                    .map_err(|error| read_error("board.ice_path.countdown", error))?,
            );
        }
        encoder.emit(7, &self.payload)?;

        self.payload.clear();
        for kind in 0..33 {
            put_bool(
                &mut self.payload,
                backend
                    .spawn_allowed(kind)
                    .map_err(|error| read_error("board.spawn_allowed", error))?,
            );
        }
        encoder.emit(8, &self.payload)?;

        self.payload.clear();
        for wave in 0..20 {
            for slot in 0..50 {
                put_i32(
                    &mut self.payload,
                    backend
                        .spawn_entry(wave, slot)
                        .map_err(|error| read_error("board.spawn_table", error))?,
                );
            }
        }
        encoder.emit(9, &self.payload)?;

        self.payload.clear();
        let seeds = backend.seeds().map_err(|error| read_error("board.seeds", error))?;
        for seed in seeds {
            let selection = backend
                .seed_selection(seed)
                .map_err(|error| read_error("board.seeds.selection", error))?;
            put_u32(&mut self.payload, backend.seed_slot(seed).index() as u32);
            put_i32(&mut self.payload, selection.packet_kind().code());
            put_i32(
                &mut self.payload,
                selection
                    .imitator_target()
                    .map_or(-1, rsvz::core::model::PlantKind::code),
            );
            put_i32(&mut self.payload, backend.seed_cooldown_remaining(seed));
            put_bool(&mut self.payload, backend.seed_is_usable(seed));
        }
        encoder.emit(11, &self.payload)?;

        // Register each live gameplay object before encoding cross-pool relationships.
        let birth_origin = self.ids.plants.birth_origin(world_epoch);
        with_sorted_entities(
            backend.plants().map_err(|error| read_error("plants", error))?,
            |entity| ((backend.plant_id(entity).raw() >> 16) as u16).wrapping_sub(birth_origin) as u32,
            |entities| {
                for &plant in entities {
                    self.ids.plants.id(world_epoch, backend.plant_id(plant).raw())?;
                }
                Ok(())
            },
        )?;
        let birth_origin = self.ids.zombies.birth_origin(world_epoch);
        with_sorted_entities(
            backend.zombies().map_err(|error| read_error("zombies", error))?,
            |entity| ((backend.zombie_id(entity).raw() >> 16) as u16).wrapping_sub(birth_origin) as u32,
            |entities| {
                for &zombie in entities {
                    self.ids.zombies.id(world_epoch, backend.zombie_id(zombie).raw())?;
                }
                Ok(())
            },
        )?;
        let birth_origin = self.ids.projectiles.birth_origin(world_epoch);
        with_sorted_entities(
            backend
                .projectiles()
                .map_err(|error| read_error("projectiles", error))?,
            |entity| ((backend.projectile_id(entity).raw() >> 16) as u16).wrapping_sub(birth_origin) as u32,
            |entities| {
                for &projectile in entities {
                    self.ids
                        .projectiles
                        .id(world_epoch, backend.projectile_id(projectile).raw())?;
                }
                Ok(())
            },
        )?;
        let birth_origin = self.ids.grid_items.birth_origin(world_epoch);
        with_sorted_entities(
            backend.grid_items().map_err(|error| read_error("grid_items", error))?,
            |entity| ((backend.grid_item_id(entity).raw() >> 16) as u16).wrapping_sub(birth_origin) as u32,
            |entities| {
                for &item in entities {
                    self.ids.grid_items.id(world_epoch, backend.grid_item_id(item).raw())?;
                }
                Ok(())
            },
        )?;

        with_sorted_entities(
            backend.plants().map_err(|error| read_error("plants", error))?,
            |entity| {
                self.ids
                    .plants
                    .get(world_epoch, backend.plant_id(entity).raw())
                    .expect("registered live entity")
            },
            |entities| {
                for &plant in entities {
                    let raw_id = backend.plant_id(plant).raw();
                    let witness_id = self
                        .ids
                        .plants
                        .get(world_epoch, raw_id)
                        .expect("registered live entity");
                    let grid = Grid {
                        row: backend.plant_row(plant),
                        col: backend.plant_col(plant),
                    };
                    let size = [backend.plant_width(plant), backend.plant_height(plant)];
                    let target = [backend.plant_target_x(plant), backend.plant_target_y(plant)];
                    let collision = backend
                        .plant_collision(plant)
                        .map_err(|error| read_error("plants.collision", error))?;
                    self.payload.clear();
                    put_u32(&mut self.payload, witness_id);
                    put_i32(
                        &mut self.payload,
                        backend
                            .plant_raw_kind(plant)
                            .map_err(|error| read_error("plants.raw_kind", error))?
                            .code(),
                    );
                    put_i32(
                        &mut self.payload,
                        backend
                            .plant_kind(plant)
                            .map_err(|error| read_error("plants.kind", error))?
                            .code(),
                    );
                    put_grid(&mut self.payload, grid);
                    put_i32(&mut self.payload, backend.plant_x(plant));
                    put_i32(&mut self.payload, backend.plant_y(plant));
                    put_i32(&mut self.payload, size[0]);
                    put_i32(&mut self.payload, size[1]);
                    put_i32(&mut self.payload, backend.plant_state(plant));
                    put_i32(&mut self.payload, backend.plant_hp(plant));
                    put_i32(&mut self.payload, backend.plant_max_hp(plant));
                    put_i32(&mut self.payload, target[0]);
                    put_i32(&mut self.payload, target[1]);
                    let target_zombie = backend.plant_target_object(plant);
                    put_u32(
                        &mut self.payload,
                        (target_zombie != 0)
                            .then(|| self.ids.zombies.get(world_epoch, target_zombie as u32))
                            .flatten()
                            .unwrap_or(0),
                    );
                    put_i32(&mut self.payload, backend.plant_direction(plant));
                    put_rect(&mut self.payload, collision);
                    put_i32(&mut self.payload, backend.plant_imitater_kind(plant));
                    put_u32(&mut self.payload, u32::from(backend.plant_is_sleeping(plant)));
                    for timer in [
                        backend.plant_state_countdown(plant),
                        backend.plant_effect_countdown(plant),
                        backend.plant_disappear_countdown(plant),
                        backend.plant_recently_eaten_countdown(plant),
                        backend.plant_wake_up_counter(plant),
                        backend.plant_launch_counter(plant),
                        backend.plant_shooting_counter(plant),
                    ] {
                        put_i32(&mut self.payload, timer);
                    }
                    encoder.emit(30, &self.payload)?;
                }
                Ok(())
            },
        )?;

        with_sorted_entities(
            backend.zombies().map_err(|error| read_error("zombies", error))?,
            |entity| {
                self.ids
                    .zombies
                    .get(world_epoch, backend.zombie_id(entity).raw())
                    .expect("registered live entity")
            },
            |entities| {
                for &zombie in entities {
                    let raw_id = backend.zombie_id(zombie).raw();
                    let witness_id = self
                        .ids
                        .zombies
                        .get(world_epoch, raw_id)
                        .expect("registered live entity");
                    let collision = backend
                        .zombie_collision(zombie)
                        .map_err(|error| read_error("zombies.collision", error))?;
                    let reanim = (|| -> Result<_, B::Error> {
                        let Some(anim_time) = backend.zombie_reanim_anim_time(zombie)? else {
                            return Ok(None);
                        };
                        let Some(last_time) = backend.zombie_reanim_last_time(zombie)? else {
                            return Ok(None);
                        };
                        let Some(rate) = backend.zombie_reanim_rate(zombie)? else {
                            return Ok(None);
                        };
                        let Some(frame_start) = backend.zombie_reanim_frame_start(zombie)? else {
                            return Ok(None);
                        };
                        let Some(frame_count) = backend.zombie_reanim_frame_count(zombie)? else {
                            return Ok(None);
                        };
                        let Some(loop_type) = backend.zombie_reanim_loop_type(zombie)? else {
                            return Ok(None);
                        };
                        Ok(Some(rsvz::core::model::ZombieReanimationFacts {
                            anim_time,
                            last_time,
                            rate,
                            frame_start,
                            frame_count,
                            loop_type,
                        }))
                    })()
                    .map_err(|error| read_error("zombies.reanim", error))?;
                    self.payload.clear();
                    put_u32(&mut self.payload, witness_id);
                    put_i32(
                        &mut self.payload,
                        backend
                            .zombie_kind(zombie)
                            .map_err(|error| read_error("zombies.kind", error))?
                            .code(),
                    );
                    put_i32(
                        &mut self.payload,
                        backend
                            .zombie_phase(zombie)
                            .map_err(|error| read_error("zombies.phase", error))? as i32,
                    );
                    put_i32(&mut self.payload, backend.zombie_action(zombie));
                    put_i32(&mut self.payload, backend.zombie_row(zombie));
                    put_i32(&mut self.payload, backend.zombie_int_x(zombie));
                    put_i32(&mut self.payload, backend.zombie_int_y(zombie));
                    put_f32(&mut self.payload, backend.zombie_pos_x(zombie));
                    put_f32(&mut self.payload, backend.zombie_pos_y(zombie));
                    put_f32(&mut self.payload, backend.zombie_speed_x(zombie));
                    put_f32(&mut self.payload, backend.zombie_speed_z(zombie));
                    put_i32(&mut self.payload, backend.zombie_width(zombie));
                    put_i32(&mut self.payload, backend.zombie_height(zombie));
                    put_i32(&mut self.payload, backend.zombie_height_state(zombie));
                    put_f32(&mut self.payload, backend.zombie_altitude(zombie));
                    put_f32(&mut self.payload, backend.zombie_scale(zombie));
                    put_i32(&mut self.payload, backend.zombie_from_wave(zombie));
                    put_i32(&mut self.payload, backend.zombie_age(zombie));
                    put_i32(&mut self.payload, backend.zombie_phase_counter(zombie));
                    put_rect(&mut self.payload, collision);
                    put_i32(&mut self.payload, backend.zombie_hp(zombie));
                    put_i32(&mut self.payload, backend.zombie_max_hp(zombie));
                    for value in [
                        backend.zombie_accessory_1_hp(zombie),
                        backend.zombie_accessory_1_max_hp(zombie),
                        backend.zombie_accessory_2_hp(zombie),
                        backend.zombie_accessory_2_max_hp(zombie),
                    ] {
                        put_i32(&mut self.payload, value);
                    }
                    let related = backend.zombie_related_id(zombie);
                    put_u32(
                        &mut self.payload,
                        self.ids.zombies.get(world_epoch, related).unwrap_or(0),
                    );
                    for index in 0..4 {
                        let follower = backend
                            .zombie_follower_id(zombie, index)
                            .map_err(|error| read_error("zombies.relations", error))?;
                        put_u32(
                            &mut self.payload,
                            self.ids.zombies.get(world_epoch, follower).unwrap_or(0),
                        );
                    }
                    let target_plant = backend
                        .zombie_target_plant(zombie)
                        .and_then(|id| self.ids.plants.get(world_epoch, id.raw()))
                        .unwrap_or(0);
                    // Both legacy slots are part of the frozen schema.
                    put_u32(&mut self.payload, target_plant);
                    put_u32(&mut self.payload, target_plant);
                    for timer in [
                        backend.zombie_chilled_countdown(zombie),
                        backend.zombie_buttered_countdown(zombie),
                        backend.zombie_frozen_countdown(zombie),
                        backend.zombie_yucky_face_counter(zombie),
                        backend.zombie_phase_counter(zombie),
                    ] {
                        put_i32(&mut self.payload, timer);
                    }
                    let flags = u32::from(backend.zombie_is_alive(zombie))
                        | (u32::from(backend.zombie_is_eating(zombie)) << 1)
                        | (u32::from(backend.zombie_is_disappeared(zombie)) << 2)
                        | (u32::from(backend.zombie_is_mind_controlled(zombie)) << 3)
                        | (u32::from(backend.zombie_is_blown_away(zombie)) << 4)
                        | (u32::from(backend.zombie_has_flat_tires(zombie)) << 5)
                        | (u32::from(backend.zombie_has_head(zombie)) << 6)
                        | (u32::from(backend.zombie_has_object(zombie)) << 7)
                        | (u32::from(backend.zombie_is_in_pool(zombie)) << 8)
                        | (u32::from(backend.zombie_is_on_high_ground(zombie)) << 9)
                        | (u32::from(backend.zombie_has_yucky_face(zombie)) << 10);
                    put_u32(&mut self.payload, flags);
                    put_i32(&mut self.payload, backend.zombie_yucky_face_counter(zombie));
                    put_i32(&mut self.payload, backend.zombie_frozen_countdown(zombie));
                    put_i32(&mut self.payload, backend.zombie_chilled_countdown(zombie));
                    put_i32(&mut self.payload, backend.zombie_buttered_countdown(zombie));
                    match reanim {
                        Some(reanim) => {
                            put_bool(&mut self.payload, true);
                            put_f32(&mut self.payload, reanim.anim_time);
                            put_f32(&mut self.payload, reanim.last_time);
                            put_f32(&mut self.payload, reanim.rate);
                            put_i32(&mut self.payload, reanim.frame_start);
                            put_i32(&mut self.payload, reanim.frame_count);
                            put_i32(&mut self.payload, reanim.loop_type);
                        }
                        None => put_bool(&mut self.payload, false),
                    }
                    encoder.emit(40, &self.payload)?;
                }
                Ok(())
            },
        )?;

        with_sorted_entities(
            backend
                .projectiles()
                .map_err(|error| read_error("projectiles", error))?,
            |entity| {
                self.ids
                    .projectiles
                    .get(world_epoch, backend.projectile_id(entity).raw())
                    .expect("registered live entity")
            },
            |entities| {
                for &projectile in entities {
                    let raw_id = backend.projectile_id(projectile).raw();
                    let witness_id = self
                        .ids
                        .projectiles
                        .get(world_epoch, raw_id)
                        .expect("registered live entity");
                    let grid = Grid {
                        row: backend.projectile_row(projectile),
                        col: -1,
                    };
                    let position = [
                        backend.projectile_pos_x(projectile),
                        backend.projectile_pos_y(projectile),
                        backend.projectile_pos_z(projectile),
                    ];
                    let velocity = [
                        backend.projectile_vel_x(projectile),
                        backend.projectile_vel_y(projectile),
                        backend.projectile_vel_z(projectile),
                    ];
                    let acceleration = backend.projectile_acceleration(projectile);
                    let collision = backend
                        .projectile_collision(projectile)
                        .map_err(|error| read_error("projectiles.collision", error))?;
                    self.payload.clear();
                    put_u32(&mut self.payload, witness_id);
                    put_i32(&mut self.payload, backend.projectile_kind(projectile));
                    put_i32(&mut self.payload, backend.projectile_motion(projectile));
                    put_grid(&mut self.payload, grid);
                    for value in position {
                        put_f32(&mut self.payload, value);
                    }
                    for value in velocity {
                        put_f32(&mut self.payload, value);
                    }
                    put_f32(&mut self.payload, acceleration);
                    put_rect(&mut self.payload, collision);
                    put_u32(&mut self.payload, backend.projectile_damage_flags(projectile));
                    for value in [
                        backend.projectile_age(projectile),
                        backend.projectile_torch_col(projectile),
                        backend.projectile_cob_target_row(projectile),
                    ] {
                        put_i32(&mut self.payload, value);
                    }
                    let target = backend.projectile_target_zombie_id(projectile);
                    put_u32(
                        &mut self.payload,
                        self.ids.zombies.get(world_epoch, target).unwrap_or(0),
                    );
                    put_u32(&mut self.payload, backend.projectile_cob_target_x_bits(projectile));
                    encoder.emit(50, &self.payload)?;
                }
                Ok(())
            },
        )?;

        with_sorted_entities(
            backend.grid_items().map_err(|error| read_error("grid_items", error))?,
            |entity| {
                self.ids
                    .grid_items
                    .get(world_epoch, backend.grid_item_id(entity).raw())
                    .expect("registered live entity")
            },
            |entities| {
                for &item in entities {
                    let raw_id = backend.grid_item_id(item).raw();
                    let witness_id = self
                        .ids
                        .grid_items
                        .get(world_epoch, raw_id)
                        .expect("registered live entity");
                    self.payload.clear();
                    put_u32(&mut self.payload, witness_id);
                    put_i32(
                        &mut self.payload,
                        backend
                            .grid_item_kind(item)
                            .map_err(|error| read_error("grid_items.kind", error))?
                            .code(),
                    );
                    put_grid(
                        &mut self.payload,
                        Grid {
                            row: backend.grid_item_row(item),
                            col: backend.grid_item_col(item),
                        },
                    );
                    put_i32(&mut self.payload, backend.grid_item_countdown(item));
                    encoder.emit(60, &self.payload)?;
                }
                Ok(())
            },
        )?;

        self.payload.clear();
        for stream in [RandomStreamKind::Battle, RandomStreamKind::Level] {
            let seed = backend
                .random_seed(stream)
                .map_err(|error| read_error("random", error))?;
            let locked = backend.random_locked(stream);
            let fixed = backend.random_fixed(stream);
            put_u32(&mut self.payload, random_stream_code(stream));
            put_u32(&mut self.payload, seed);
            put_bool(&mut self.payload, locked);
            put_u32(&mut self.payload, fixed);
        }
        encoder.emit(80, &self.payload)?;

        self.payload.clear();
        for event in events {
            put_event(&mut self.payload, *event, world_epoch, &self.ids);
        }
        encoder.emit(90, &self.payload)?;

        let hasher = encoder.hasher;
        Ok(CapturedFrame {
            frame,
            records: std::mem::take(&mut self.frame),
            full,
            hasher,
        })
    }
}

impl Default for CanonicalVisitor {
    fn default() -> Self {
        Self::new()
    }
}

fn battle_status_code(status: rsvz::core::model::BattleStatus) -> i32 {
    match status {
        rsvz::core::model::BattleStatus::Running => 0,
        rsvz::core::model::BattleStatus::Lost => 1,
        rsvz::core::model::BattleStatus::Ended => 2,
        rsvz::core::model::BattleStatus::ObjectiveReached => 3,
        rsvz::core::model::BattleStatus::Unknown => 4,
    }
}

const fn random_stream_code(stream: RandomStreamKind) -> u32 {
    match stream {
        RandomStreamKind::Battle => 0,
        RandomStreamKind::Level => 1,
    }
}

fn put_bool(output: &mut Vec<u8>, value: bool) {
    output.push(u8::from(value));
}
fn put_i32(output: &mut Vec<u8>, value: i32) {
    output.extend_from_slice(&value.to_le_bytes());
}
fn put_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}
fn put_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}
fn put_f32(output: &mut Vec<u8>, value: f32) {
    put_u32(output, value.to_bits());
}
fn put_grid(output: &mut Vec<u8>, grid: Grid) {
    put_i32(output, grid.row);
    put_i32(output, grid.col);
}
fn put_rect(output: &mut Vec<u8>, rect: ContactRect) {
    put_i32(output, rect.left);
    put_i32(output, rect.top);
    put_i32(output, rect.right);
    put_i32(output, rect.bottom);
}
fn put_option_i32(output: &mut Vec<u8>, value: Option<i32>) {
    put_bool(output, value.is_some());
    if let Some(value) = value {
        put_i32(output, value);
    }
}
fn put_event(output: &mut Vec<u8>, event: GameEvent, world_epoch: u64, ids: &WitnessIds) {
    let zombie_id = |raw| ids.zombies.get(world_epoch, raw).unwrap_or(0);
    let plant_id = |raw| ids.plants.get(world_epoch, raw).unwrap_or(0);
    match event {
        GameEvent::PlantEffect(event) => {
            // Schema v6 keeps unsupported events as its existing unknown marker.
            // Do not assign new wire tags without an intentional schema migration.
            let (source, actor) = match event.source {
                PlantEffectSource::Jack(id) => (0, zombie_id(id.raw())),
                PlantEffectSource::Bite(id) => (1, zombie_id(id.raw())),
                PlantEffectSource::Gargantuar(id) => (2, zombie_id(id.raw())),
                PlantEffectSource::Basketball(id) => (3, zombie_id(id.raw())),
                PlantEffectSource::ZomboniCrush(_)
                | PlantEffectSource::CatapultCrush(_)
                | PlantEffectSource::Bungee(_) => return output.push(u8::MAX),
            };
            let (requested, requested_damage) = match event.requested {
                PlantEffect::HpDamage { native_requested } => (0, Some(native_requested)),
                PlantEffect::InstantKill => (1, None),
                PlantEffect::Squish => (2, None),
                PlantEffect::Steal => return output.push(u8::MAX),
            };
            let (outcome, applied) = match event.outcome {
                PlantEffectOutcome::SuppressedByMeasurement => (0, None),
                PlantEffectOutcome::PreventedByRule => (1, None),
                PlantEffectOutcome::HpDelta { applied } => (2, Some(applied)),
                PlantEffectOutcome::Killed => (3, None),
                PlantEffectOutcome::Squished => (4, None),
                PlantEffectOutcome::Activated => (5, None),
                PlantEffectOutcome::NoEffect => (6, None),
                PlantEffectOutcome::Stolen => return output.push(u8::MAX),
            };
            output.push(0);
            output.push(source);
            put_u32(output, actor);
            put_u32(output, plant_id(event.plant_id.raw()));
            put_i32(output, event.raw_kind.code());
            put_i32(output, event.effective_kind.code());
            put_grid(output, event.grid);
            output.push(requested);
            if let Some(damage) = requested_damage {
                put_i32(output, damage);
            }
            output.push(match event.decision_origin {
                rsvz::core::model::EventDecisionOrigin::Native => 0,
                rsvz::core::model::EventDecisionOrigin::Measurement => 1,
            });
            output.push(outcome);
            if let Some(damage) = applied {
                put_i32(output, damage);
            }
            put_i32(output, event.hp_before);
            put_i32(output, event.max_hp);
            put_i32(output, event.main_counter);
        }
        GameEvent::HomeEntry(event) => {
            output.push(1);
            put_u32(output, zombie_id(event.zombie_id.raw()));
            put_i32(output, event.zombie_kind.code());
            put_i32(output, event.row);
            put_i32(output, event.main_counter);
        }
        _ => output.push(u8::MAX),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WitnessSemanticConfig {
    pub reset_completed_rounds: u32,
    pub reset_seed: u32,
    pub initial_sun: u32,
    pub random_mode: String,
    pub random_value: u32,
    #[serde(default)]
    pub wave_spawn_random: bool,
    pub sun_mode: String,
    pub capture_mode: String,
    pub replay_start: Option<u64>,
    pub replay_end: Option<u64>,
    pub limit_kind: String,
    pub limit_value: u64,
}

impl From<&WitnessOptions> for WitnessSemanticConfig {
    fn from(options: &WitnessOptions) -> Self {
        let (capture_mode, replay_start, replay_end) = match options.capture {
            WitnessCapture::Full => ("full", None, None),
            WitnessCapture::Digest => ("digest", None, None),
            WitnessCapture::ReplayWindow { start, end } => ("replay_window", Some(start), Some(end)),
        };
        let (limit_kind, limit_value) = match options.limit {
            WitnessLimit::Frames(value) => ("frames", value),
            WitnessLimit::CompletedRounds(value) => ("completed_rounds", value),
        };
        Self {
            reset_completed_rounds: options.reset.completed_rounds,
            reset_seed: options.reset.seed,
            initial_sun: options.reset.initial_sun,
            random_mode: "locked".to_owned(),
            random_value: options.locked_random,
            wave_spawn_random: options.wave_spawn_random,
            sun_mode: match options.sun {
                SunProductionMode::DirectCredit => "direct_credit",
                SunProductionMode::NativePickup => "native_pickup",
            }
            .to_owned(),
            capture_mode: capture_mode.to_owned(),
            replay_start,
            replay_end,
            limit_kind: limit_kind.to_owned(),
            limit_value,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReproManifest {
    pub case_id: String,
    pub scenario_seed: Option<u64>,
    pub script_id: String,
    pub build_id: String,
    pub expanded_setup: serde_json::Value,
}

impl ReproManifest {
    #[must_use]
    pub fn from_setup(setup: &ScriptSetup, repro: &WitnessRepro) -> Self {
        let spawn_list = setup.spawn_list.as_ref().map(|spawn| {
            serde_json::json!({
                "allowed": spawn.allowed_types().iter().map(|kind| kind.code()).collect::<Vec<_>>(),
                "waves": spawn.waves().map(|wave| wave.iter().map(|kind| kind.code()).collect::<Vec<_>>()).collect::<Vec<_>>(),
            })
        });
        let lineup = setup.lineup.as_ref().map(|pending| {
            let lineup = pending.lineup();
            serde_json::json!({
                "scene": lineup.scene().code(),
                "rake_row": lineup.rake_row(),
                "source": pending.source(),
                "cells": lineup.iter().map(|(grid, cell)| lineup_cell_json(grid, cell)).collect::<Vec<_>>(),
            })
        });
        let zombie_request = setup.zombie_spawn_request.as_ref().map(|request| {
            let selection = match request.selection() {
                ZombieTypeSelection::Exact(kinds) => {
                    serde_json::json!({"exact": kinds.iter().map(|kind| kind.code()).collect::<Vec<_>>()})
                }
                ZombieTypeSelection::Random { required, banned } => serde_json::json!({
                    "random": {
                        "required": required.iter().map(|kind| kind.code()).collect::<Vec<_>>(),
                        "banned": banned.iter().map(|kind| kind.code()).collect::<Vec<_>>(),
                    }
                }),
            };
            serde_json::json!({"mode": format!("{:?}", request.mode()), "selection": selection})
        });
        let cards = setup
            .desired_cards
            .as_ref()
            .map(|cards| cards.iter().copied().map(card_json).collect::<Vec<_>>());
        Self {
            case_id: repro.case_id.clone(),
            scenario_seed: repro.scenario_seed,
            script_id: repro.script_id.clone(),
            build_id: repro.build_id.clone(),
            expanded_setup: serde_json::json!({
                "scenario": repro.expanded_setup.clone(),
                "spawn_list": spawn_list,
                "zombie_request": zombie_request,
                "lineup": lineup,
                "cards": cards,
                "reload_mode": format!("{:?}", setup.reload_mode),
            }),
        }
    }

    pub fn record_resolved_spawn<B: BoardStateBackend + SceneBackend>(
        &mut self, backend: &B,
    ) -> Result<(), WitnessError> {
        let mut waves = Vec::with_capacity(20);
        for wave in 0..20 {
            let mut slots = Vec::with_capacity(50);
            for slot in 0..50 {
                slots.push(
                    backend
                        .spawn_entry(wave, slot)
                        .map_err(|error| read_error("manifest.resolved_spawn_table", error))?,
                );
            }
            waves.push(slots);
        }
        let allowed = (0..33)
            .map(|kind| {
                backend
                    .spawn_allowed(kind)
                    .map_err(|error| read_error("manifest.resolved_spawn_allowed", error))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let setup = self
            .expanded_setup
            .as_object_mut()
            .ok_or(WitnessError::Encoding("expanded setup must be a JSON object"))?;
        setup.insert(
            "resolved_scene".to_owned(),
            serde_json::json!(
                backend
                    .scene()
                    .map_err(|error| read_error("manifest.resolved_scene", error))?
                    .code()
            ),
        );
        setup.insert("resolved_spawn_allowed".to_owned(), serde_json::json!(allowed));
        setup.insert("resolved_spawn_table".to_owned(), serde_json::json!(waves));
        Ok(())
    }
}

fn card_json(selection: CardSelection) -> serde_json::Value {
    match selection {
        CardSelection::Plant(kind) => serde_json::json!({"packet": kind.code()}),
        CardSelection::Imitator(kind) => serde_json::json!({"packet": 48, "imitator": kind.code()}),
    }
}

fn lineup_plant_json(plant: LineupPlant) -> serde_json::Value {
    serde_json::json!({"selection": card_json(plant.selection), "awake": plant.awake, "grown": plant.grown})
}

fn lineup_cell_json(grid: Grid, cell: &rsvz::LineupCell) -> serde_json::Value {
    let base = match cell.base {
        LineupBase::None => serde_json::json!(null),
        LineupBase::LilyPad { imitator } => serde_json::json!({"kind": 16, "imitator": imitator}),
        LineupBase::FlowerPot { imitator } => serde_json::json!({"kind": 33, "imitator": imitator}),
        LineupBase::Grave => serde_json::json!({"grid_item": 1}),
    };
    serde_json::json!({
        "row": grid.row,
        "col": grid.col,
        "base": base,
        "main": cell.main.map(lineup_plant_json),
        "pumpkin": cell.pumpkin.map(lineup_plant_json),
        "coffee": cell.coffee.map(lineup_plant_json),
        "ladder": cell.ladder,
    })
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WitnessHeader {
    pub artifact_version: u32,
    pub schema_version: u32,
    pub hash: String,
    pub encoding: String,
    pub backend_name: String,
    pub backend_version: String,
    pub config: WitnessSemanticConfig,
    pub repro: ReproManifest,
}

impl WitnessHeader {
    #[must_use]
    pub fn new<B: BackendIdentityBackend>(backend: &B, options: &WitnessOptions, repro: ReproManifest) -> Self {
        Self {
            artifact_version: WITNESS_ARTIFACT_VERSION,
            schema_version: WITNESS_SCHEMA_VERSION,
            hash: WITNESS_HASH.to_owned(),
            encoding: WITNESS_ENCODING.to_owned(),
            backend_name: backend.backend_name().to_owned(),
            backend_version: backend.backend_version().to_owned(),
            config: options.into(),
            repro,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum WitnessCompletion {
    Complete {
        frames: u64,
    },
    Invalid {
        frames: u64,
        first_error_frame: u64,
        message: String,
    },
    Incomplete {
        frames: u64,
    },
}

#[cfg(feature = "tooling")]
impl WitnessCompletion {
    fn complete_frames(&self) -> Option<u64> {
        match self {
            Self::Complete { frames } => Some(*frames),
            Self::Incomplete { .. } | Self::Invalid { .. } => None,
        }
    }
}

static NEXT_SPOOL: AtomicU64 = AtomicU64::new(1);

pub struct WitnessSpool {
    path: PathBuf,
    writer: Option<BufWriter<File>>,
    first_chunk: bool,
    start_frame: u64,
    frame_count: usize,
    digests: Vec<u8>,
    full: Vec<u8>,
    compressed: Vec<u8>,
    base64: Vec<u8>,
}

impl WitnessSpool {
    pub fn new(header: &WitnessHeader) -> Result<Self, WitnessError> {
        const HEADER_PREFIX: &[u8] = b"{\"header\":";
        const HEADER_SUFFIX: &[u8] = b",\"chunks\":[\n";
        let header_json = serde_json::to_vec(header).map_err(io::Error::other)?;
        if HEADER_PREFIX
            .len()
            .saturating_add(header_json.len())
            .saturating_add(HEADER_SUFFIX.len())
            > ARTIFACT_LINE_CAPACITY
        {
            return Err(WitnessError::Overflow("artifact header line"));
        }
        let mut last_error = None;
        for _ in 0..32 {
            let suffix = NEXT_SPOOL.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("rsvz-witness-{}-{suffix}.json", std::process::id()));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => {
                    let mut spool = Self {
                        path,
                        writer: Some(BufWriter::new(file)),
                        first_chunk: true,
                        start_frame: 0,
                        frame_count: 0,
                        digests: Vec::with_capacity(CHUNK_FRAMES * 16),
                        full: Vec::with_capacity(CHUNK_CAPACITY),
                        compressed: Vec::with_capacity(COMPRESSED_CAPACITY),
                        base64: Vec::with_capacity(BASE64_CAPACITY),
                    };
                    let writer = spool.writer.as_mut().ok_or(WitnessError::Encoding("closed spool"))?;
                    writer.write_all(HEADER_PREFIX)?;
                    writer.write_all(&header_json)?;
                    writer.write_all(HEADER_SUFFIX)?;
                    return Ok(spool);
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => last_error = Some(error),
                Err(error) => return Err(error.into()),
            }
        }
        Err(last_error
            .unwrap_or_else(|| io::Error::new(io::ErrorKind::AlreadyExists, "spool name collision"))
            .into())
    }

    pub fn push(&mut self, frame: FinishedFrame) -> Result<Vec<u8>, WitnessError> {
        let full_entry = if frame.full {
            8 + 16 + 4 + frame.records.len()
        } else {
            0
        };
        if self.frame_count > 0
            && (self.frame_count >= CHUNK_FRAMES || self.full.len().saturating_add(full_entry) > CHUNK_CAPACITY)
        {
            self.flush_chunk()?;
        }
        if full_entry > CHUNK_CAPACITY {
            return Err(WitnessError::Overflow("full frame exceeds chunk capacity"));
        }
        if self.frame_count == 0 {
            self.start_frame = frame.frame;
        }
        if self.digests.len().saturating_add(16) > DIGEST_CHUNK_CAPACITY {
            return Err(WitnessError::Overflow("digest chunk"));
        }
        self.digests.extend_from_slice(&frame.digest);
        if frame.full {
            put_u64(&mut self.full, frame.frame);
            self.full.extend_from_slice(&frame.digest);
            put_u32(
                &mut self.full,
                u32::try_from(frame.records.len()).map_err(|_error| WitnessError::Overflow("full frame length"))?,
            );
            self.full.extend_from_slice(&frame.records);
        }
        self.frame_count += 1;
        Ok(frame.records)
    }

    fn flush_chunk(&mut self) -> Result<(), WitnessError> {
        if self.frame_count == 0 {
            return Ok(());
        }
        let writer = self.writer.as_mut().ok_or(WitnessError::Encoding("closed spool"))?;
        if !self.first_chunk {
            writer.write_all(b",\n")?;
        }
        self.first_chunk = false;
        write!(
            writer,
            "{{\"start\":{},\"count\":{},\"digests\":\"",
            self.start_frame, self.frame_count
        )?;
        encode_base64_into(&self.digests, &mut self.base64)?;
        writer.write_all(&self.base64)?;
        writer.write_all(b"\"")?;
        if !self.full.is_empty() {
            compress_zlib_into(&self.full, &mut self.compressed)?;
            encode_base64_into(&self.compressed, &mut self.base64)?;
            write!(writer, ",\"full_raw_len\":{},\"full\":\"", self.full.len())?;
            writer.write_all(&self.base64)?;
            writer.write_all(b"\"")?;
        }
        writer.write_all(b"}")?;
        self.digests.clear();
        self.full.clear();
        self.frame_count = 0;
        Ok(())
    }

    pub fn finish(mut self, completion: WitnessCompletion) -> Result<SessionArtifact, WitnessError> {
        self.flush_chunk()?;
        let mut writer = self.writer.take().ok_or(WitnessError::Encoding("closed spool"))?;
        writer.write_all(if self.first_chunk {
            b"],\"completion\":"
        } else {
            b"\n],\"completion\":"
        })?;
        serde_json::to_writer(&mut writer, &completion).map_err(io::Error::other)?;
        writer.write_all(b"}\n")?;
        writer.flush()?;
        writer.get_ref().sync_all()?;
        let artifact = WitnessArtifact {
            path: std::mem::take(&mut self.path),
        };
        Ok(SessionArtifact::new(
            artifact,
            merge_witness_artifacts,
            write_witness_artifact,
        ))
    }
}

impl Drop for WitnessSpool {
    fn drop(&mut self) {
        if !self.path.as_os_str().is_empty() {
            let _removed = std::fs::remove_file(&self.path);
        }
    }
}

fn compress_zlib_into(input: &[u8], output: &mut Vec<u8>) -> Result<(), WitnessError> {
    output.clear();
    let flags = create_comp_flags_from_zip_params(6, 1, 0);
    let mut compressor = CompressorOxide::new(flags);
    let mut overflow = false;
    let (status, consumed) = compress_to_output(&mut compressor, input, TDEFLFlush::Finish, |bytes| {
        if output.len().saturating_add(bytes.len()) > COMPRESSED_CAPACITY {
            overflow = true;
            return false;
        }
        output.extend_from_slice(bytes);
        true
    });
    if overflow {
        return Err(WitnessError::Overflow("compressed chunk"));
    }
    if status != TDEFLStatus::Done || consumed != input.len() {
        return Err(WitnessError::Encoding("deflate did not finish"));
    }
    Ok(())
}

fn encode_base64_into(input: &[u8], output: &mut Vec<u8>) -> Result<(), WitnessError> {
    let required = input.len().div_ceil(3) * 4;
    if required > BASE64_CAPACITY {
        return Err(WitnessError::Overflow("base64 chunk"));
    }
    output.resize(required, 0);
    let written = STANDARD
        .encode_slice(input, output)
        .map_err(|_error| WitnessError::Encoding("base64 output length"))?;
    output.truncate(written);
    Ok(())
}

struct WitnessArtifact {
    path: PathBuf,
}

impl Drop for WitnessArtifact {
    fn drop(&mut self) {
        let _removed = std::fs::remove_file(&self.path);
    }
}

fn merge_witness_artifacts(_target: &mut WitnessArtifact, _source: WitnessArtifact) -> RuntimeResult<()> {
    Err(RuntimeError::new("witness requires exactly one worker"))
}

fn write_witness_artifact(artifact: &WitnessArtifact, writer: &mut dyn Write) -> io::Result<()> {
    io::copy(&mut File::open(&artifact.path)?, writer)?;
    Ok(())
}

#[cfg(feature = "tooling")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WitnessDiffResult {
    Equal {
        frames: u64,
    },
    Different {
        first_frame: u64,
        last_contiguous_frame: u64,
        reconverged_at: Option<u64>,
        field: Option<String>,
    },
    Invalid(String),
}

#[cfg(feature = "tooling")]
impl WitnessDiffResult {
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Equal { .. } => 0,
            Self::Different { .. } => 1,
            Self::Invalid(_) => 2,
        }
    }
}

#[cfg(feature = "tooling")]
#[derive(Deserialize)]
struct ChunkLine {
    start: u64,
    count: usize,
    digests: String,
    #[serde(default)]
    full_raw_len: Option<usize>,
    #[serde(default)]
    full: Option<String>,
}

#[cfg(feature = "tooling")]
struct DecodedChunk {
    start: u64,
    count: usize,
    digests: Vec<u8>,
    full: Vec<FullFrame>,
}

#[cfg(feature = "tooling")]
struct FullFrame {
    frame: u64,
    digest: [u8; 16],
    records: Vec<u8>,
    transition: WitnessTransitionOutcome,
}

#[cfg(feature = "tooling")]
struct ArtifactReader {
    reader: BufReader<File>,
    pub header: WitnessHeader,
    completion: Option<WitnessCompletion>,
    chunk: Option<DecodedChunk>,
    index: usize,
    chunks_seen: bool,
    chunk_separator_pending: bool,
}

#[cfg(feature = "tooling")]
fn read_artifact_line(reader: &mut BufReader<File>, line: &mut String, description: &str) -> Result<usize, String> {
    line.clear();
    let mut bounded = reader.by_ref().take((ARTIFACT_LINE_CAPACITY + 1) as u64);
    let read = bounded
        .read_line(line)
        .map_err(|error| format!("read {description}: {error}"))?;
    if read > ARTIFACT_LINE_CAPACITY {
        return Err(format!("{description} line exceeds schema limit"));
    }
    Ok(read)
}

#[cfg(feature = "tooling")]
fn validate_artifact_header(header: &WitnessHeader) -> Result<(), String> {
    if header.artifact_version != WITNESS_ARTIFACT_VERSION || header.schema_version != WITNESS_SCHEMA_VERSION {
        return Err("unsupported artifact or schema version".to_owned());
    }
    if header.hash != WITNESS_HASH || header.encoding != WITNESS_ENCODING {
        return Err("unsupported hash or encoding".to_owned());
    }
    match header.config.random_mode.as_str() {
        "seeded" if header.config.random_value == header.config.reset_seed => {}
        "seeded" => return Err("seeded random value differs from reset seed".to_owned()),
        "locked" => {}
        _ => return Err("invalid witness random mode".to_owned()),
    }
    if !matches!(header.config.sun_mode.as_str(), "direct_credit" | "native_pickup") {
        return Err("invalid witness sun mode".to_owned());
    }
    if !matches!(header.config.limit_kind.as_str(), "frames" | "completed_rounds") {
        return Err("invalid witness limit kind".to_owned());
    }
    if SunAmount::new(header.config.initial_sun).is_err() {
        return Err("invalid witness initial sun".to_owned());
    }
    match (
        header.config.capture_mode.as_str(),
        header.config.replay_start,
        header.config.replay_end,
    ) {
        ("full" | "digest", None, None) => {}
        ("replay_window", Some(start), Some(end)) if start <= end => {}
        _ => return Err("invalid witness capture configuration".to_owned()),
    }
    let scenario = header
        .repro
        .expanded_setup
        .get("scenario")
        .unwrap_or(&header.repro.expanded_setup);
    if !scenario.get("plants").is_some_and(serde_json::Value::is_array)
        || !scenario.get("zombies").is_some_and(serde_json::Value::is_array)
    {
        return Err("invalid expanded reproduction setup".to_owned());
    }
    Ok(())
}

#[cfg(feature = "tooling")]
fn capture_requires_full(config: &WitnessSemanticConfig, frame: u64) -> bool {
    match config.capture_mode.as_str() {
        "full" => true,
        "replay_window" => config
            .replay_start
            .zip(config.replay_end)
            .is_some_and(|(start, end)| frame >= start && frame <= end),
        _ => false,
    }
}

#[cfg(feature = "tooling")]
impl ArtifactReader {
    fn open(path: &Path) -> Result<Self, String> {
        let mut reader = BufReader::new(File::open(path).map_err(|error| format!("open {}: {error}", path.display()))?);
        let mut line = String::new();
        if read_artifact_line(&mut reader, &mut line, "header")? == 0 {
            return Err("witness artifact lacks a header".to_owned());
        }
        let prefix = "{\"header\":";
        let suffix = ",\"chunks\":[\n";
        let header = line
            .strip_prefix(prefix)
            .and_then(|line| line.strip_suffix(suffix))
            .ok_or_else(|| "invalid witness header line".to_owned())?;
        let header = serde_json::from_str(header).map_err(|error| format!("decode witness header: {error}"))?;
        validate_artifact_header(&header)?;
        Ok(Self {
            reader,
            header,
            completion: None,
            chunk: None,
            index: 0,
            chunks_seen: false,
            chunk_separator_pending: false,
        })
    }

    fn next_frame(&mut self) -> Result<Option<FrameRecord>, String> {
        if self.completion.is_some() {
            return Ok(None);
        }
        loop {
            if let Some(chunk) = &self.chunk
                && self.index < chunk.count
            {
                let index = self.index;
                self.index += 1;
                let start = index
                    .checked_mul(16)
                    .ok_or_else(|| "digest index overflow".to_owned())?;
                let digest: [u8; 16] = chunk
                    .digests
                    .get(start..start + 16)
                    .ok_or_else(|| "truncated digest chunk".to_owned())?
                    .try_into()
                    .map_err(|_error| "invalid digest".to_owned())?;
                let frame = chunk
                    .start
                    .checked_add(index as u64)
                    .ok_or_else(|| "frame number overflow".to_owned())?;
                let (full, transition) = if let Some(entry) = chunk.full.iter().find(|entry| entry.frame == frame) {
                    if entry.digest != digest {
                        return Err("full-frame digest does not match digest stream".to_owned());
                    }
                    (Some(entry.records.clone()), Some(entry.transition))
                } else {
                    (None, None)
                };
                if full.is_some() != capture_requires_full(&self.header.config, frame) {
                    return Err("full-frame coverage does not match capture mode".to_owned());
                }
                return Ok(Some(FrameRecord {
                    frame,
                    digest,
                    full,
                    transition,
                }));
            }
            self.chunk = None;
            self.index = 0;
            let mut line = String::new();
            if read_artifact_line(&mut self.reader, &mut line, "chunk")? == 0 {
                return Err("witness artifact ended before completion".to_owned());
            }
            if line.starts_with("],\"completion\":") {
                if self.chunk_separator_pending {
                    return Err("dangling chunk separator".to_owned());
                }
                let completion = line
                    .strip_prefix("],\"completion\":")
                    .and_then(|line| line.strip_suffix("}\n").or_else(|| line.strip_suffix('}')))
                    .ok_or_else(|| "invalid completion line".to_owned())?;
                self.completion =
                    Some(serde_json::from_str(completion).map_err(|error| format!("decode completion: {error}"))?);
                if !self
                    .reader
                    .fill_buf()
                    .map_err(|error| format!("read artifact tail: {error}"))?
                    .is_empty()
                {
                    return Err("witness artifact has trailing data".to_owned());
                }
                return Ok(None);
            }
            let line = line.trim_end();
            if self.chunks_seen && !self.chunk_separator_pending {
                return Err("missing chunk separator".to_owned());
            }
            let (line, has_separator) = line.strip_suffix(',').map_or((line, false), |line| (line, true));
            let encoded: ChunkLine = serde_json::from_str(line).map_err(|error| format!("decode chunk: {error}"))?;
            self.chunks_seen = true;
            self.chunk_separator_pending = has_separator;
            if encoded.count == 0 || encoded.count > CHUNK_FRAMES {
                return Err("invalid digest chunk frame count".to_owned());
            }
            let chunk_end = encoded
                .start
                .checked_add(encoded.count as u64)
                .ok_or_else(|| "chunk frame range overflow".to_owned())?;
            if encoded.digests.len() > DIGEST_BASE64_CAPACITY {
                return Err("digest chunk exceeds schema limit".to_owned());
            }
            let digests = STANDARD
                .decode(encoded.digests)
                .map_err(|error| format!("decode digests: {error}"))?;
            if digests.len() != encoded.count.saturating_mul(16) {
                return Err("digest chunk count mismatch".to_owned());
            }
            let full = match encoded.full {
                Some(full) => {
                    let raw_len = encoded
                        .full_raw_len
                        .ok_or_else(|| "full payload without full_raw_len".to_owned())?;
                    if raw_len > CHUNK_CAPACITY {
                        return Err("full chunk exceeds schema limit".to_owned());
                    }
                    if full.len() > BASE64_CAPACITY {
                        return Err("compressed full chunk exceeds schema limit".to_owned());
                    }
                    let compressed = STANDARD
                        .decode(full)
                        .map_err(|error| format!("decode full chunk: {error}"))?;
                    if compressed.len() > COMPRESSED_CAPACITY {
                        return Err("compressed full chunk exceeds schema limit".to_owned());
                    }
                    let raw = decompress_to_vec_zlib_with_limit(&compressed, raw_len)
                        .map_err(|error| format!("inflate full chunk: {error:?}"))?;
                    if raw.len() != raw_len {
                        return Err("full chunk length mismatch".to_owned());
                    }
                    let frames = decode_full_frames(&raw)?;
                    let mut previous = None;
                    for frame in &frames {
                        if frame.frame < encoded.start || frame.frame >= chunk_end {
                            return Err("full frame lies outside its digest chunk".to_owned());
                        }
                        if previous.is_some_and(|previous| frame.frame <= previous) {
                            return Err("full frames must be unique and ordered".to_owned());
                        }
                        previous = Some(frame.frame);
                    }
                    frames
                }
                None if encoded.full_raw_len.is_none() => Vec::new(),
                None => return Err("full_raw_len without full payload".to_owned()),
            };
            self.chunk = Some(DecodedChunk {
                start: encoded.start,
                count: encoded.count,
                digests,
                full,
            });
        }
    }
}

#[cfg(feature = "tooling")]
struct FrameRecord {
    frame: u64,
    digest: [u8; 16],
    full: Option<Vec<u8>>,
    transition: Option<WitnessTransitionOutcome>,
}

#[cfg(feature = "tooling")]
fn decode_full_frames(mut raw: &[u8]) -> Result<Vec<FullFrame>, String> {
    let mut frames = Vec::new();
    while !raw.is_empty() {
        if raw.len() < 28 {
            return Err("truncated full frame header".to_owned());
        }
        let frame = u64::from_le_bytes(raw[0..8].try_into().map_err(|_error| "full frame number".to_owned())?);
        let digest = raw[8..24].try_into().map_err(|_error| "full frame digest".to_owned())?;
        let len = u32::from_le_bytes(
            raw[24..28]
                .try_into()
                .map_err(|_error| "full frame length".to_owned())?,
        ) as usize;
        raw = &raw[28..];
        if raw.len() < len {
            return Err("truncated full frame records".to_owned());
        }
        let records = &raw[..len];
        let transition = validate_full_records(records)?;
        let mut hasher = StableHasher::new();
        hasher.write(records);
        if hasher.finish() != digest {
            return Err("full-frame records do not match their digest".to_owned());
        }
        frames.push(FullFrame {
            frame,
            digest,
            records: records.to_vec(),
            transition,
        });
        raw = &raw[len..];
    }
    Ok(frames)
}

#[cfg(feature = "tooling")]
fn validate_full_records(mut records: &[u8]) -> Result<WitnessTransitionOutcome, String> {
    let mut schema_index = 0;
    let mut transition = None;
    while !records.is_empty() {
        let Some((tag, payload)) = take_record(&mut records).map_err(|()| "malformed full record stream".to_owned())?
        else {
            break;
        };
        while let Some(field) = WITNESS_SCHEMA_FIELDS.get(schema_index)
            && field.tag < tag
            && repeatable_record_tag(field.tag)
        {
            schema_index += 1;
        }
        let Some(field) = WITNESS_SCHEMA_FIELDS.get(schema_index) else {
            return Err("full record tags do not match the schema".to_owned());
        };
        if field.tag != tag {
            return Err(format!("full record stream lacks required tag {}", field.tag));
        }
        validate_record_payload(tag, payload)?;
        if tag == 91 {
            transition = Some(match payload[0] {
                0 => WitnessTransitionOutcome::Clean,
                1 => WitnessTransitionOutcome::RecoverableError,
                2 => WitnessTransitionOutcome::TimingViolation,
                _ => unreachable!("validated transition outcome"),
            });
        }
        if !repeatable_record_tag(tag) {
            schema_index += 1;
        }
    }
    while WITNESS_SCHEMA_FIELDS
        .get(schema_index)
        .is_some_and(|field| repeatable_record_tag(field.tag))
    {
        schema_index += 1;
    }
    if let Some(field) = WITNESS_SCHEMA_FIELDS.get(schema_index) {
        return Err(format!("full record stream lacks required tag {}", field.tag));
    }
    transition.ok_or_else(|| "full record stream lacks transition outcome".to_owned())
}

#[cfg(feature = "tooling")]
const fn repeatable_record_tag(tag: u16) -> bool {
    matches!(tag, 30 | 40 | 50 | 60)
}

#[cfg(feature = "tooling")]
fn validate_record_payload(tag: u16, payload: &[u8]) -> Result<(), String> {
    let fixed = match tag {
        1 => Some(4),
        2 => Some(24),
        4 | 5 => Some(8),
        6 => Some(4),
        7 => Some(48),
        8 => Some(33),
        9 => Some(4_000),
        30 => Some(116),
        50 => Some(88),
        60 => Some(20),
        80 => Some(26),
        91 => Some(1),
        _ => None,
    };
    if let Some(expected) = fixed
        && payload.len() != expected
    {
        return Err(format!("tag {tag} payload length does not match schema"));
    }
    match tag {
        1 if payload != WITNESS_SCHEMA_VERSION.to_le_bytes() => {
            Err("full frame carries a different schema version".to_owned())
        }
        3 => validate_option_payload(payload, 5),
        8 if payload.iter().any(|value| *value > 1) => Err("tag 8 contains an invalid bool".to_owned()),
        11 if !payload.len().is_multiple_of(17) || payload.iter().skip(16).step_by(17).any(|value| *value > 1) => {
            Err("tag 11 seed records do not match the schema".to_owned())
        }
        40 => validate_optional_tail(tag, payload, 184, 24),
        80 if payload[0..4] != 0_u32.to_le_bytes()
            || payload[13..17] != 1_u32.to_le_bytes()
            || payload[8] > 1
            || payload[21] > 1 =>
        {
            Err("tag 80 random streams do not match the schema".to_owned())
        }
        90 => validate_event_payload(payload),
        91 if payload[0] > WitnessTransitionOutcome::TimingViolation as u8 => {
            Err("tag 91 contains an invalid transition outcome".to_owned())
        }
        _ => Ok(()),
    }
}

#[cfg(feature = "tooling")]
fn validate_option_payload(mut payload: &[u8], count: usize) -> Result<(), String> {
    for _ in 0..count {
        let Some((&present, rest)) = payload.split_first() else {
            return Err("option payload is truncated".to_owned());
        };
        payload = match present {
            0 => rest,
            1 if rest.len() >= 4 => &rest[4..],
            _ => return Err("option payload does not match the schema".to_owned()),
        };
    }
    if payload.is_empty() {
        Ok(())
    } else {
        Err("option payload has trailing bytes".to_owned())
    }
}

#[cfg(feature = "tooling")]
fn validate_optional_tail(tag: u16, payload: &[u8], fixed: usize, optional: usize) -> Result<(), String> {
    let Some(&present) = payload.get(fixed) else {
        return Err(format!("tag {tag} payload is truncated"));
    };
    let expected = fixed + 1 + usize::from(present == 1) * optional;
    if present > 1 || payload.len() != expected {
        return Err(format!("tag {tag} optional payload does not match the schema"));
    }
    Ok(())
}

#[cfg(feature = "tooling")]
fn validate_event_payload(mut payload: &[u8]) -> Result<(), String> {
    while let Some((&kind, rest)) = payload.split_first() {
        payload = match kind {
            1 if rest.len() >= 16 => &rest[16..],
            u8::MAX => rest,
            0 => {
                if rest.len() < 40 || rest[0] > 3 {
                    return Err("plant-effect event is malformed".to_owned());
                }
                let requested = rest[25];
                let requested_extra = match requested {
                    0 => 4,
                    1 | 2 => 0,
                    _ => return Err("plant-effect request is invalid".to_owned()),
                };
                let decision_at = 26 + requested_extra;
                let outcome_at = decision_at + 1;
                let Some((&decision, tail)) = rest.get(decision_at).zip(rest.get(outcome_at..)) else {
                    return Err("plant-effect decision is truncated".to_owned());
                };
                if decision > 1 {
                    return Err("plant-effect decision is invalid".to_owned());
                }
                let Some(&outcome) = tail.first() else {
                    return Err("plant-effect outcome is truncated".to_owned());
                };
                let outcome_extra = match outcome {
                    2 => 4,
                    0 | 1 | 3..=6 => 0,
                    _ => return Err("plant-effect outcome is invalid".to_owned()),
                };
                let len = outcome_at + 1 + outcome_extra + 12;
                if rest.len() < len {
                    return Err("plant-effect event is truncated".to_owned());
                }
                &rest[len..]
            }
            _ => return Err("event kind does not match the schema".to_owned()),
        };
    }
    Ok(())
}

#[cfg(feature = "tooling")]
pub fn diff_witness_paths(
    left: &Path, right: &Path, reference_left: Option<&Path>, reference_right: Option<&Path>,
) -> WitnessDiffResult {
    if reference_left.is_some() != reference_right.is_some() {
        return WitnessDiffResult::Invalid("both replay references are required".to_owned());
    }
    if let (Some(reference_left), Some(reference_right)) = (reference_left, reference_right) {
        if let Err(error) = validate_replay(left, reference_left) {
            return WitnessDiffResult::Invalid(format!("left replay invalid: {error}"));
        }
        if let Err(error) = validate_replay(right, reference_right) {
            return WitnessDiffResult::Invalid(format!("right replay invalid: {error}"));
        }
    } else {
        for (side, path) in [("left", left), ("right", right)] {
            let reader = match ArtifactReader::open(path) {
                Ok(reader) => reader,
                Err(error) => return WitnessDiffResult::Invalid(format!("{side} artifact invalid: {error}")),
            };
            if reader.header.config.capture_mode == "replay_window" {
                return WitnessDiffResult::Invalid(
                    "ReplayWindow artifacts require both original Digest references".to_owned(),
                );
            }
        }
    }
    match compare_artifacts(left, right, false) {
        Ok(result) => result,
        Err(error) => WitnessDiffResult::Invalid(error),
    }
}

#[cfg(feature = "tooling")]
fn validate_replay(replay: &Path, reference: &Path) -> Result<(), String> {
    validate_complete(replay)?;
    validate_complete(reference)?;
    let replay_reader = ArtifactReader::open(replay)?;
    let reference_reader = ArtifactReader::open(reference)?;
    if replay_reader.header.config.capture_mode != "replay_window" {
        return Err("artifact is not ReplayWindow".to_owned());
    }
    if reference_reader.header.config.capture_mode != "digest" {
        return Err("replay reference is not Digest".to_owned());
    }
    if replay_reader.header.backend_name != reference_reader.header.backend_name
        || replay_reader.header.backend_version != reference_reader.header.backend_version
    {
        return Err("replay reference belongs to a different backend".to_owned());
    }
    let end = replay_reader
        .header
        .config
        .replay_end
        .ok_or_else(|| "ReplayWindow end missing".to_owned())?;
    match compare_artifacts_until(replay, reference, true, Some(end))? {
        WitnessDiffResult::Equal { frames } if frames == end.saturating_add(1) => Ok(()),
        WitnessDiffResult::Equal { frames } => Err(format!(
            "replay ended after {frames} frames, expected through frame {end}"
        )),
        WitnessDiffResult::Different { first_frame, .. } => {
            Err(format!("digest differs from reference at frame {first_frame}"))
        }
        WitnessDiffResult::Invalid(error) => Err(error),
    }
}

#[cfg(feature = "tooling")]
fn validate_complete(path: &Path) -> Result<(), String> {
    let mut reader = ArtifactReader::open(path)?;
    let mut frames = 0_u64;
    let mut non_clean_full = false;
    while let Some(frame) = reader.next_frame()? {
        if frame.frame != frames {
            return Err("frame stream must start at zero and be contiguous".to_owned());
        }
        non_clean_full |= frame
            .transition
            .is_some_and(|outcome| outcome != WitnessTransitionOutcome::Clean);
        frames = frames.saturating_add(1);
    }
    validate_completion(&reader.header, reader.completion.as_ref(), frames, non_clean_full)
        .map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(feature = "tooling")]
fn validate_completion(
    header: &WitnessHeader, completion: Option<&WitnessCompletion>, frames: u64, non_clean_full: bool,
) -> Result<(), String> {
    let Some(recorded) = completion.and_then(WitnessCompletion::complete_frames) else {
        return Err("artifact is incomplete or invalid".to_owned());
    };
    if recorded != frames {
        return Err("completion frame count is invalid".to_owned());
    }
    if frames == 0 {
        return Err("complete artifact must contain frame zero".to_owned());
    }
    if non_clean_full {
        return Err("complete artifact contains a non-clean full-frame transition".to_owned());
    }
    if header.config.limit_kind == "frames" && frames > header.config.limit_value.saturating_add(1) {
        return Err("complete artifact exceeds its frame limit".to_owned());
    }
    Ok(())
}

#[cfg(feature = "tooling")]
fn compare_artifacts(left: &Path, right: &Path, ignore_capture: bool) -> Result<WitnessDiffResult, String> {
    compare_artifacts_until(left, right, ignore_capture, None)
}

#[cfg(feature = "tooling")]
fn compare_artifacts_until(
    left: &Path, right: &Path, ignore_capture: bool, end: Option<u64>,
) -> Result<WitnessDiffResult, String> {
    let mut left = ArtifactReader::open(left)?;
    let mut right = ArtifactReader::open(right)?;
    validate_headers(&left.header, &right.header, ignore_capture)?;
    let mut shared_frames = 0;
    let mut left_frames = 0;
    let mut right_frames = 0;
    let mut first = None;
    let mut last_contiguous = None;
    let mut reconverged = None;
    let mut field = None;
    let mut expected_frame = 0;
    let mut left_non_clean_full = false;
    let mut right_non_clean_full = false;
    let mut left_done = false;
    let mut right_done = false;
    loop {
        if end.is_some_and(|end| expected_frame > end) {
            break;
        }
        let left_frame = if left_done { None } else { left.next_frame()? };
        let right_frame = if right_done { None } else { right.next_frame()? };
        left_done |= left_frame.is_none();
        right_done |= right_frame.is_none();
        match (left_frame, right_frame) {
            (Some(left_frame), Some(right_frame)) => {
                if left_frame.frame != right_frame.frame {
                    return Err("frame numbering differs".to_owned());
                }
                if left_frame.frame != expected_frame {
                    return Err("frame stream must start at zero and be contiguous".to_owned());
                }
                expected_frame = expected_frame.saturating_add(1);
                shared_frames += 1;
                left_frames += 1;
                right_frames += 1;
                left_non_clean_full |= left_frame
                    .transition
                    .is_some_and(|outcome| outcome != WitnessTransitionOutcome::Clean);
                right_non_clean_full |= right_frame
                    .transition
                    .is_some_and(|outcome| outcome != WitnessTransitionOutcome::Clean);
                if left_frame.digest != right_frame.digest {
                    if first.is_none() {
                        first = Some(left_frame.frame);
                        field = first_field_difference(left_frame.full.as_deref(), right_frame.full.as_deref());
                    }
                    if reconverged.is_none() {
                        last_contiguous = Some(left_frame.frame);
                    }
                } else if first.is_some() && reconverged.is_none() {
                    reconverged = Some(left_frame.frame);
                }
            }
            (Some(frame), None) => {
                if frame.frame != expected_frame {
                    return Err("frame stream must start at zero and be contiguous".to_owned());
                }
                left_frames += 1;
                left_non_clean_full |= frame
                    .transition
                    .is_some_and(|outcome| outcome != WitnessTransitionOutcome::Clean);
                first.get_or_insert(frame.frame);
                if reconverged.is_none() {
                    last_contiguous = Some(frame.frame);
                }
                expected_frame = expected_frame.saturating_add(1);
            }
            (None, Some(frame)) => {
                if frame.frame != expected_frame {
                    return Err("frame stream must start at zero and be contiguous".to_owned());
                }
                right_frames += 1;
                right_non_clean_full |= frame
                    .transition
                    .is_some_and(|outcome| outcome != WitnessTransitionOutcome::Clean);
                first.get_or_insert(frame.frame);
                if reconverged.is_none() {
                    last_contiguous = Some(frame.frame);
                }
                expected_frame = expected_frame.saturating_add(1);
            }
            (None, None) => break,
        }
    }
    if end.is_none() {
        validate_completion(&left.header, left.completion.as_ref(), left_frames, left_non_clean_full)?;
        validate_completion(
            &right.header,
            right.completion.as_ref(),
            right_frames,
            right_non_clean_full,
        )?;
    }
    Ok(match first {
        Some(first_frame) => WitnessDiffResult::Different {
            first_frame,
            last_contiguous_frame: last_contiguous.unwrap_or(first_frame),
            reconverged_at: reconverged,
            field,
        },
        None => WitnessDiffResult::Equal { frames: shared_frames },
    })
}

#[cfg(feature = "tooling")]
fn validate_headers(left: &WitnessHeader, right: &WitnessHeader, ignore_capture: bool) -> Result<(), String> {
    let mut left_config = left.config.clone();
    let mut right_config = right.config.clone();
    if ignore_capture {
        left_config.capture_mode.clear();
        left_config.replay_start = None;
        left_config.replay_end = None;
        left_config.limit_kind.clear();
        left_config.limit_value = 0;
        right_config.capture_mode.clear();
        right_config.replay_start = None;
        right_config.replay_end = None;
        right_config.limit_kind.clear();
        right_config.limit_value = 0;
    }
    if left_config != right_config {
        return Err("semantic witness config differs".to_owned());
    }
    if !same_repro_inputs(&left.repro, &right.repro) {
        return Err("reproduction manifest differs".to_owned());
    }
    Ok(())
}

#[cfg(feature = "tooling")]
fn same_repro_inputs(left: &ReproManifest, right: &ReproManifest) -> bool {
    left.case_id == right.case_id
        && left.scenario_seed == right.scenario_seed
        && left.script_id == right.script_id
        && left.build_id == right.build_id
        && same_expanded_inputs(&left.expanded_setup, &right.expanded_setup)
}

#[cfg(feature = "tooling")]
fn same_expanded_inputs(left: &serde_json::Value, right: &serde_json::Value) -> bool {
    let (Some(left), Some(right)) = (left.as_object(), right.as_object()) else {
        return left == right;
    };
    let is_input = |key: &str| !key.starts_with("resolved_");
    left.iter()
        .filter(|(key, _value)| is_input(key))
        .all(|(key, value)| right.get(key) == Some(value))
        && right
            .iter()
            .filter(|(key, _value)| is_input(key))
            .all(|(key, value)| left.get(key) == Some(value))
}

#[cfg(feature = "tooling")]
fn first_field_difference(left: Option<&[u8]>, right: Option<&[u8]>) -> Option<String> {
    let (Some(mut left), Some(mut right)) = (left, right) else {
        return None;
    };
    let mut occurrence = 0_u32;
    let mut previous_tag = None;
    loop {
        let left_record = take_record(&mut left);
        let right_record = take_record(&mut right);
        match (left_record, right_record) {
            (Ok(Some((left_tag, left_value))), Ok(Some((right_tag, right_value))))
                if left_tag == right_tag && left_value == right_value =>
            {
                occurrence = if previous_tag == Some(left_tag) {
                    occurrence.saturating_add(1)
                } else {
                    0
                };
                previous_tag = Some(left_tag);
            }
            (Ok(Some((left_tag, left_value))), Ok(Some((right_tag, right_value)))) => {
                if left_tag != right_tag {
                    let (tag, value, missing_side) = if left_tag < right_tag {
                        (left_tag, left_value, "right")
                    } else {
                        (right_tag, right_value, "left")
                    };
                    let current_occurrence = if previous_tag == Some(tag) {
                        occurrence.saturating_add(1)
                    } else {
                        0
                    };
                    let path = match tag {
                        30 => format!("plants[{current_occurrence}]"),
                        40 => format!("zombies[{current_occurrence}]"),
                        50 => format!("projectiles[{current_occurrence}]"),
                        60 => format!("grid_items[{current_occurrence}]"),
                        _ => WITNESS_SCHEMA_FIELDS
                            .iter()
                            .find(|field| field.tag == tag)
                            .map_or_else(|| "unknown".to_owned(), |field| field.path.to_owned()),
                    };
                    let object = if matches!(tag, 30 | 40 | 50 | 60) {
                        format!(" object#{current_occurrence} (witness={})", object_id(value))
                    } else {
                        String::new()
                    };
                    return Some(format!(
                        "{path}{object} is missing on {missing_side} (tag {left_tag}/{right_tag})"
                    ));
                }
                let tag = left_tag;
                let current_occurrence = if previous_tag == Some(tag) {
                    occurrence.saturating_add(1)
                } else {
                    0
                };
                let byte = left_value
                    .iter()
                    .zip(right_value)
                    .position(|(left, right)| left != right)
                    .unwrap_or_else(|| left_value.len().min(right_value.len()));
                let path = difference_path(tag, current_occurrence, byte, left_value);
                let object = if matches!(tag, 30 | 40 | 50 | 60) {
                    format!(
                        " object#{current_occurrence} left(witness={}) right(witness={})",
                        object_id(left_value),
                        object_id(right_value)
                    )
                } else {
                    String::new()
                };
                let left_byte = left_value
                    .get(byte)
                    .map_or_else(|| "missing".to_owned(), |value| format!("0x{value:02x}"));
                let right_byte = right_value
                    .get(byte)
                    .map_or_else(|| "missing".to_owned(), |value| format!("0x{value:02x}"));
                return Some(format!(
                    "{path}{object} (tag {left_tag}/{right_tag}, byte {byte}: {left_byte} != {right_byte})"
                ));
            }
            (Ok(None), Ok(None)) => return None,
            _ => return Some("malformed full record stream".to_owned()),
        }
    }
}

#[cfg(feature = "tooling")]
fn difference_path(tag: u16, occurrence: u32, byte: usize, value: &[u8]) -> String {
    let fixed = |prefix: &str, fields: &[(&str, usize)]| {
        named_field(byte, fields).map_or_else(|| prefix.to_owned(), |field| format!("{prefix}.{field}"))
    };
    match tag {
        1 => "schema.version".to_owned(),
        2 => fixed(
            "board",
            &[
                ("ui", 4),
                ("battle_status", 4),
                ("scene", 4),
                ("clock", 4),
                ("wave", 4),
                ("sun", 4),
            ],
        ),
        3 => option_field_path(
            "board.wave_timing",
            &["total", "refresh", "initial", "huge", "end"],
            byte,
            value,
        ),
        4 => fixed("board.wave_health", &[("start", 4), ("current", 4)]),
        5 => fixed("board.sun", &[("generated", 4), ("countdown", 4)]),
        6 => "board.dancer_clock".to_owned(),
        7 => {
            let row = byte / 8;
            let field = if byte % 8 < 4 { "x" } else { "countdown" };
            format!("board.ice_path[{row}].{field}")
        }
        8 => format!("board.spawn_allowed[{byte}]"),
        9 => {
            let index = byte / 4;
            format!("board.spawn_table[{}][{}]", index / 50, index % 50)
        }

        11 => {
            let index = byte / 17;
            let field = named_field(
                byte % 17,
                &[
                    ("slot", 4),
                    ("packet", 4),
                    ("imitator", 4),
                    ("cooldown", 4),
                    ("usable", 1),
                ],
            )
            .unwrap_or("unknown");
            format!("board.seeds[{index}].{field}")
        }
        30 => fixed(
            &format!("plants[{occurrence}]"),
            &[
                ("witness_id", 4),
                ("raw_kind", 4),
                ("kind", 4),
                ("grid.row", 4),
                ("grid.col", 4),
                ("position.x", 4),
                ("position.y", 4),
                ("size.width", 4),
                ("size.height", 4),
                ("state", 4),
                ("hp", 4),
                ("max_hp", 4),
                ("target.x", 4),
                ("target.y", 4),
                ("target.object", 4),
                ("direction", 4),
                ("collision.left", 4),
                ("collision.top", 4),
                ("collision.right", 4),
                ("collision.bottom", 4),
                ("imitater_kind", 4),
                ("sleeping", 4),
                ("timers.state", 4),
                ("timers.special", 4),
                ("timers.disappear", 4),
                ("timers.recently_eaten", 4),
                ("timers.wake", 4),
                ("timers.launch", 4),
                ("timers.shooting", 4),
            ],
        ),
        40 => fixed(
            &format!("zombies[{occurrence}]"),
            &[
                ("witness_id", 4),
                ("kind", 4),
                ("phase", 4),
                ("action", 4),
                ("row", 4),
                ("int_position.x", 4),
                ("int_position.y", 4),
                ("position.x", 4),
                ("position.y", 4),
                ("speed.x", 4),
                ("speed.z", 4),
                ("size.width", 4),
                ("size.height", 4),
                ("height_state", 4),
                ("altitude", 4),
                ("scale", 4),
                ("from_wave", 4),
                ("age", 4),
                ("phase_counter", 4),
                ("collision.left", 4),
                ("collision.top", 4),
                ("collision.right", 4),
                ("collision.bottom", 4),
                ("hp", 4),
                ("max_hp", 4),
                ("accessory_1.hp", 4),
                ("accessory_1.max_hp", 4),
                ("accessory_2.hp", 4),
                ("accessory_2.max_hp", 4),
                ("relations.related_witness", 4),
                ("relations.follower_witness[0]", 4),
                ("relations.follower_witness[1]", 4),
                ("relations.follower_witness[2]", 4),
                ("relations.follower_witness[3]", 4),
                ("relations.target_plant_witness", 4),
                ("target_plant", 4),
                ("timers.slow", 4),
                ("timers.butter", 4),
                ("timers.freeze", 4),
                ("timers.garlic", 4),
                ("timers.action", 4),
                ("lifecycle", 4),
                ("yucky_face_counter", 4),
                ("frozen_countdown", 4),
                ("chilled_countdown", 4),
                ("buttered_countdown", 4),
                ("reanim.present", 1),
                ("reanim.anim_time", 4),
                ("reanim.last_time", 4),
                ("reanim.rate", 4),
                ("reanim.frame_start", 4),
                ("reanim.frame_count", 4),
                ("reanim.loop_type", 4),
            ],
        ),
        50 => fixed(
            &format!("projectiles[{occurrence}]"),
            &[
                ("witness_id", 4),
                ("kind", 4),
                ("motion", 4),
                ("grid.row", 4),
                ("grid.col", 4),
                ("position.x", 4),
                ("position.y", 4),
                ("position.z", 4),
                ("velocity.x", 4),
                ("velocity.y", 4),
                ("velocity.z", 4),
                ("acceleration.z", 4),
                ("collision.left", 4),
                ("collision.top", 4),
                ("collision.right", 4),
                ("collision.bottom", 4),
                ("damage_flags", 4),
                ("timers.age", 4),
                ("timers.torch_col", 4),
                ("timers.cob_target_row", 4),
                ("relations.target_zombie_witness", 4),
                ("relations.cob_target_x", 4),
            ],
        ),
        60 => fixed(
            &format!("grid_items[{occurrence}]"),
            &[
                ("witness_id", 4),
                ("kind", 4),
                ("grid.row", 4),
                ("grid.col", 4),
                ("countdown", 4),
            ],
        ),

        80 => {
            let stream = byte / 13;
            let field =
                named_field(byte % 13, &[("kind", 4), ("seed", 4), ("locked", 1), ("fixed", 4)]).unwrap_or("unknown");
            format!("random.{}.{field}", if stream == 0 { "battle" } else { "level" })
        }
        90 => event_difference_path(byte, value),
        91 => "transition.outcome".to_owned(),
        _ => WITNESS_SCHEMA_FIELDS
            .iter()
            .find(|field| field.tag == tag)
            .map_or_else(|| "unknown".to_owned(), |field| field.path.to_owned()),
    }
}

#[cfg(feature = "tooling")]
fn named_field<'a>(mut byte: usize, fields: &'a [(&'a str, usize)]) -> Option<&'a str> {
    for (name, width) in fields {
        if byte < *width {
            return Some(name);
        }
        byte -= width;
    }
    None
}

#[cfg(feature = "tooling")]
fn option_field_path(prefix: &str, names: &[&str], byte: usize, value: &[u8]) -> String {
    let mut start = 0;
    for name in names {
        let width = 1 + usize::from(value.get(start).copied().unwrap_or(0) != 0) * 4;
        if byte < start.saturating_add(width) {
            return format!("{prefix}.{name}");
        }
        start = start.saturating_add(width);
    }
    prefix.to_owned()
}

#[cfg(feature = "tooling")]
fn event_difference_path(byte: usize, value: &[u8]) -> String {
    let mut start = 0;
    let mut index = 0;
    while start < value.len() {
        let kind = value[start];
        let (len, field) = if kind == 1 {
            (
                17,
                named_field(
                    byte.saturating_sub(start),
                    &[
                        ("kind", 1),
                        ("zombie_id", 4),
                        ("zombie_kind", 4),
                        ("row", 4),
                        ("main_counter", 4),
                    ],
                ),
            )
        } else if kind == 0 {
            let requested_extra = usize::from(value.get(start + 26).copied() == Some(0)) * 4;
            let outcome_at = start + 28 + requested_extra;
            let outcome_extra = usize::from(value.get(outcome_at).copied() == Some(2)) * 4;
            let local = byte.saturating_sub(start);
            let mut field = named_field(
                local,
                &[
                    ("kind", 1),
                    ("source.kind", 1),
                    ("source.id", 4),
                    ("plant_id", 4),
                    ("raw_kind", 4),
                    ("effective_kind", 4),
                    ("grid.row", 4),
                    ("grid.col", 4),
                    ("requested.kind", 1),
                ],
            );
            let mut cursor = 27;
            if field.is_none() && requested_extra != 0 {
                field = named_field(local.saturating_sub(cursor), &[("requested.damage", 4)]);
                cursor += 4;
            }
            if field.is_none() {
                field = named_field(local.saturating_sub(cursor), &[("decision", 1), ("outcome.kind", 1)]);
            }
            cursor += 2;
            if field.is_none() && outcome_extra != 0 {
                field = named_field(local.saturating_sub(cursor), &[("outcome.applied", 4)]);
                cursor += 4;
            }
            if field.is_none() {
                field = named_field(
                    local.saturating_sub(cursor),
                    &[("hp_before", 4), ("max_hp", 4), ("main_counter", 4)],
                );
            }
            (cursor + 12, field)
        } else {
            (1, Some("kind"))
        };
        if byte < start.saturating_add(len) {
            return format!("events[{index}].{}", field.unwrap_or("unknown"));
        }
        start = start.saturating_add(len);
        index += 1;
    }
    "events".to_owned()
}

#[cfg(feature = "tooling")]
fn object_id(value: &[u8]) -> u32 {
    value
        .get(0..4)
        .and_then(|value| value.try_into().ok())
        .map(u32::from_le_bytes)
        .unwrap_or(0)
}

#[cfg(feature = "tooling")]
fn take_record<'a>(input: &mut &'a [u8]) -> Result<Option<(u16, &'a [u8])>, ()> {
    if input.is_empty() {
        return Ok(None);
    }
    if input.len() < 6 {
        return Err(());
    }
    let tag = u16::from_le_bytes(input[0..2].try_into().map_err(|_error| ())?);
    let len = u32::from_le_bytes(input[2..6].try_into().map_err(|_error| ())?) as usize;
    *input = &input[6..];
    if input.len() < len {
        return Err(());
    }
    let value = &input[..len];
    *input = &input[len..];
    Ok(Some((tag, value)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsvz::core::backend::WorldResetBackend;
    #[cfg(feature = "tooling")]
    use rsvz::core::backend::{
        BattleEntryBackend, CardAppendSelectionBackend, GridItemCreateBackend, PlantCreateBackend, PlantReadBackend,
        ProjectileReadBackend, RandomControlBackend, ZombieCreateBackend, ZombieReadBackend,
    };
    #[cfg(feature = "tooling")]
    use rsvz::core::model::{
        CardSelection, EventDecisionOrigin, HomeEntryEvent, PlantEffectEvent, PlantKind, ZombieId, ZombieKind,
    };
    use rsvz_pvz_emulator_backend::{PeBackend, PeWorldConfig, runner_internal::PeWorldOwner};

    struct TempArtifact(PathBuf);

    impl Drop for TempArtifact {
        fn drop(&mut self) {
            let _removed = std::fs::remove_file(&self.0);
        }
    }

    fn test_header(capture: WitnessCapture, initial_sun: u32) -> WitnessHeader {
        let mut options = WitnessOptions {
            capture,
            ..WitnessOptions::default()
        };
        options.reset.initial_sun = initial_sun;
        WitnessHeader {
            artifact_version: WITNESS_ARTIFACT_VERSION,
            schema_version: WITNESS_SCHEMA_VERSION,
            hash: WITNESS_HASH.to_owned(),
            encoding: WITNESS_ENCODING.to_owned(),
            backend_name: "test".to_owned(),
            backend_version: "1".to_owned(),
            config: (&options).into(),
            repro: ReproManifest {
                case_id: "fixture".to_owned(),
                scenario_seed: Some(1),
                script_id: "test".to_owned(),
                build_id: "test".to_owned(),
                expanded_setup: serde_json::json!({"scenario": {"plants": [], "zombies": []}}),
            },
        }
    }

    fn test_frame(frame: u64, value: u32, full: bool, outcome: WitnessTransitionOutcome) -> FinishedFrame {
        let mut records = Vec::with_capacity(8 * 1024);
        let mut encoder = Encoder::new(full.then_some(&mut records));
        for field in WITNESS_SCHEMA_FIELDS {
            let payload = match field.tag {
                1 => WITNESS_SCHEMA_VERSION.to_le_bytes().to_vec(),
                2 => vec![0; 24],
                3 => vec![0; 5],
                4 | 5 => vec![0; 8],
                6 => value.to_le_bytes().to_vec(),
                7 => vec![0; 48],
                8 => vec![0; 33],
                9 => vec![0; 4_000],
                11 | 90 => Vec::new(),
                30 | 40 | 50 | 60 | 91 => continue,
                80 => {
                    let mut payload = vec![0; 26];
                    payload[13..17].copy_from_slice(&1_u32.to_le_bytes());
                    payload
                }
                _ => unreachable!("frozen schema tag"),
            };
            encoder.emit(field.tag, &payload).expect("test field");
        }
        let hasher = encoder.hasher;
        CapturedFrame {
            frame,
            records,
            full,
            hasher,
        }
        .finish(outcome)
        .expect("finish test frame")
    }

    #[test]
    fn logical_ids_are_stable_per_generation_and_reuse_pool_slots_indefinitely() {
        let mut ids = WitnessIdMap::with_capacity();
        assert_eq!(ids.id(0, 0x0001_0007).expect("first generation"), 1);
        assert_eq!(ids.id(0, 0x0001_0007).expect("same generation"), 1);
        assert_eq!(ids.id(0, 0x0002_0007).expect("reused slot"), 2);
        assert_eq!(ids.id(1, 0x0002_0007).expect("reconstructed world"), 3);
    }

    #[test]
    fn witness_options_reject_sun_above_the_shared_native_range() {
        let mut options = WitnessOptions::default();
        options.reset.initial_sun = i32::MAX as u32;
        options
            .validate()
            .expect("i32::MAX sun is representable by both backends");
        options.reset.initial_sun += 1;
        assert!(matches!(options.validate(), Err(WitnessError::InvalidOptions(_))));
    }

    #[test]
    fn reproduction_manifest_keeps_the_expanded_scenario() {
        let mut repro = WitnessRepro::default();
        repro
            .set_expanded_setup_json(
                r#"{"plants":[{"kind":0,"row":0,"col":2}],"zombies":[{"kind":0,"row":0,"x":700}],"level":{"scene":2}}"#,
            )
            .expect("expanded setup JSON");
        let manifest = ReproManifest::from_setup(&ScriptSetup::default(), &repro);
        assert_eq!(manifest.expanded_setup["scenario"], repro.expanded_setup);
    }

    #[cfg(feature = "tooling")]
    #[test]
    fn backend_resolved_manifest_values_do_not_hide_state_differences() {
        let mut left = test_header(WitnessCapture::Full, 50);
        let mut right = left.clone();
        left.repro.expanded_setup["resolved_scene"] = serde_json::json!(0);
        left.repro.expanded_setup["resolved_spawn_table"] = serde_json::json!([[0]]);
        right.repro.expanded_setup["resolved_scene"] = serde_json::json!(2);
        right.repro.expanded_setup["resolved_spawn_table"] = serde_json::json!([[1]]);
        assert!(validate_headers(&left, &right, false).is_ok());

        right.repro.expanded_setup["scenario"]["plants"] = serde_json::json!([{"kind": 0}]);
        assert_eq!(
            validate_headers(&left, &right, false),
            Err("reproduction manifest differs".to_owned())
        );
    }

    #[test]
    fn default_witness_reset_is_supported_by_the_real_pe_backend() {
        let options = WitnessOptions::default();
        let owner = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("PE fixture reset");
        let mut installed = owner.install_current().expect("install PE fixture");
        installed.with_backend(|backend: &mut PeBackend| {
            backend
                .set_random_mode(RandomMode::Locked(options.locked_random))
                .expect("lock PE streams");
            backend.reset_world(options.reset).expect("default Witness reset");
            rsvz_pvz_emulator_backend::scope_backend(backend, || {
                rsvz_pvz_emulator_backend::with_backend_shared(|access| {
                    assert_eq!(access.sun().unwrap(), options.reset.initial_sun);
                })
                .expect("Board scope");
            });
        });
    }

    #[cfg(feature = "tooling")]
    #[test]
    fn pe_partial_opening_is_random_filled_before_witness_captures_seed_bank() {
        let owner = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("PE fixture reset");
        let mut installed = owner.install_current().expect("install PE fixture");
        installed.with_backend(|backend: &mut PeBackend| {
            backend
                .set_random_mode(RandomMode::Seeded(5489))
                .expect("seed PE streams");
            backend
                .select_card(
                    CardSelection::Plant(PlantKind::Peashooter)
                        .checked()
                        .expect("selection"),
                )
                .expect("select peashooter");
            backend.start_battle().expect("fill missing cards");

            rsvz_pvz_emulator_backend::scope_backend(backend, || {
                rsvz_pvz_emulator_backend::with_backend_shared(|access| {
                    let backend = access;
                    let full = CanonicalVisitor::new()
                        .capture(backend, 0, 0, &[], true)
                        .expect("full canonical capture")
                        .finish(WitnessTransitionOutcome::Clean)
                        .expect("finish full");
                    let mut records = full.records.as_slice();
                    let seed_payload = loop {
                        let Some((tag, payload)) = take_record(&mut records).expect("canonical record") else {
                            panic!("seed-bank record is present");
                        };
                        if tag == 11 {
                            break payload;
                        }
                    };
                    assert_eq!(seed_payload.len(), rsvz::core::model::MAX_SEED_SLOTS * 17);
                })
                .expect("Board scope");
            });
        });
    }

    fn write_test_artifact(
        values: &[u32], capture: WitnessCapture, initial_sun: u32, completion: WitnessCompletion,
    ) -> TempArtifact {
        write_test_artifact_with_header(values, capture, test_header(capture, initial_sun), completion)
    }

    fn write_test_artifact_with_header(
        values: &[u32], capture: WitnessCapture, header: WitnessHeader, completion: WitnessCompletion,
    ) -> TempArtifact {
        let mut spool = WitnessSpool::new(&header).expect("spool");
        for (frame, value) in values.iter().copied().enumerate() {
            let frame = frame as u64;
            spool
                .push(test_frame(
                    frame,
                    value,
                    capture.captures(frame),
                    WitnessTransitionOutcome::Clean,
                ))
                .expect("push frame");
        }
        let artifact = spool.finish(completion).expect("finish spool");
        let path = std::env::temp_dir().join(format!(
            "rsvz-witness-test-{}-{}.json",
            std::process::id(),
            NEXT_SPOOL.fetch_add(1, Ordering::Relaxed),
        ));
        let mut file = File::create(&path).expect("create fixture");
        artifact.write_json(&mut file).expect("copy artifact");
        TempArtifact(path)
    }

    #[cfg(feature = "tooling")]
    fn write_test_artifact_with_outcome(outcome: WitnessTransitionOutcome) -> TempArtifact {
        let header = test_header(WitnessCapture::Full, 50);
        let mut spool = WitnessSpool::new(&header).expect("spool");
        spool.push(test_frame(0, 7, true, outcome)).expect("push frame");
        let artifact = spool
            .finish(WitnessCompletion::Complete { frames: 1 })
            .expect("finish spool");
        let path = std::env::temp_dir().join(format!(
            "rsvz-witness-test-{}-{}.json",
            std::process::id(),
            NEXT_SPOOL.fetch_add(1, Ordering::Relaxed),
        ));
        let mut file = File::create(&path).expect("create fixture");
        artifact.write_json(&mut file).expect("copy artifact");
        TempArtifact(path)
    }

    #[test]
    fn fnv1a_128_has_a_frozen_test_vector() {
        let mut hash = StableHasher::new();
        hash.write(b"hello");
        assert_eq!(
            hash.finish(),
            0xe3e1_efd5_4283_d94f_7081_314b_599d_31b3_u128.to_le_bytes()
        );
    }

    #[test]
    fn logical_identity_ignores_slots_and_handles_generation_wrap() {
        fn register(map: &mut WitnessIdMap, raw: &[u32]) -> Vec<u32> {
            let origin = map.birth_origin(0);
            with_sorted_entities(
                raw.iter().copied(),
                |id| ((id >> 16) as u16).wrapping_sub(origin) as u32,
                |ids| {
                    for &id in ids {
                        map.id(0, id)?;
                    }
                    Ok(())
                },
            )
            .unwrap();
            raw.iter().map(|&id| map.get(0, id).unwrap()).collect()
        }
        let mut left = WitnessIdMap::with_capacity();
        let mut right = WitnessIdMap::with_capacity();
        assert_eq!(register(&mut left, &[0xfffe_0002, 0xffff_0001]), vec![1, 2]);
        assert_eq!(register(&mut right, &[0xffff_0007, 0xfffe_0009]), vec![2, 1]);
        assert_eq!(register(&mut left, &[0x0001_0003, 0xffff_0001]), vec![3, 2]);
        assert_eq!(register(&mut right, &[0xffff_0007, 0x0001_0008]), vec![2, 3]);
    }

    #[test]
    fn schema_tags_are_unique_and_ordered() {
        assert!(WITNESS_SCHEMA_FIELDS.windows(2).all(|pair| pair[0].tag < pair[1].tag));
        let mut hash = StableHasher::new();
        for field in WITNESS_SCHEMA_FIELDS {
            hash.write(&field.tag.to_le_bytes());
            hash.write(field.path.as_bytes());
        }
        assert_eq!(
            hash.finish(),
            [
                147, 96, 100, 153, 89, 189, 246, 120, 21, 137, 165, 222, 59, 242, 120, 28
            ],
            "schema v6 tag/path vector changed; bump the schema version intentionally",
        );
    }

    #[test]
    fn compression_and_base64_reuse_round_trip() {
        let input = b"one canonical field stream, one digest".repeat(200);
        let mut compressed = Vec::with_capacity(COMPRESSED_CAPACITY);
        compress_zlib_into(&input, &mut compressed).expect("compress");
        assert_eq!(decompress_to_vec_zlib(&compressed).expect("inflate"), input);
        let mut encoded = Vec::with_capacity(BASE64_CAPACITY);
        encode_base64_into(&compressed, &mut encoded).expect("base64");
        assert_eq!(STANDARD.decode(encoded).expect("decode"), compressed);
    }

    #[cfg(feature = "tooling")]
    #[test]
    fn full_record_diff_uses_frozen_path() {
        let mut left = Vec::with_capacity(256);
        let mut right = Vec::with_capacity(256);
        Encoder::new(Some(&mut left))
            .emit(6, &1_u32.to_le_bytes())
            .expect("left");
        Encoder::new(Some(&mut right))
            .emit(6, &2_u32.to_le_bytes())
            .expect("right");
        assert!(first_field_difference(Some(&left), Some(&right)).is_some_and(|value| value.contains("dancer_clock")));

        let plant_left = vec![0; 117];
        let mut plant_right = plant_left.clone();
        plant_right[40] = 1;
        left.clear();
        right.clear();
        Encoder::new(Some(&mut left)).emit(30, &plant_left).expect("left plant");
        Encoder::new(Some(&mut right))
            .emit(30, &plant_right)
            .expect("right plant");
        assert!(first_field_difference(Some(&left), Some(&right)).is_some_and(|value| value.contains("plants[0].hp")));

        let mut extra_plant = plant_left.clone();
        extra_plant[0..4].copy_from_slice(&2_u32.to_le_bytes());
        let zombie = vec![0; 8];
        left.clear();
        right.clear();
        Encoder::new(Some(&mut left))
            .emit(30, &plant_left)
            .expect("left plant 0");
        Encoder::new(Some(&mut left))
            .emit(30, &extra_plant)
            .expect("left extra plant");
        Encoder::new(Some(&mut left)).emit(40, &zombie).expect("left zombie");
        Encoder::new(Some(&mut right))
            .emit(30, &plant_left)
            .expect("right plant 0");
        Encoder::new(Some(&mut right)).emit(40, &zombie).expect("right zombie");
        assert_eq!(
            first_field_difference(Some(&left), Some(&right)),
            Some("plants[1] object#1 (witness=2) is missing on right (tag 30/40)".to_owned())
        );

        let mut event_left = Vec::new();
        let mut ids = WitnessIds::with_capacity();
        ids.zombies.id(1, 1).expect("first event zombie id");
        ids.zombies.id(1, 2).expect("second event zombie id");
        for (id, row) in [(1, 0), (2, 1)] {
            put_event(
                &mut event_left,
                GameEvent::HomeEntry(HomeEntryEvent {
                    zombie_id: ZombieId::from_raw(id),
                    zombie_kind: ZombieKind::Normal,
                    row,
                    main_counter: 10,
                }),
                1,
                &ids,
            );
        }
        let mut event_right = event_left.clone();
        event_right[26] ^= 1;
        left.clear();
        right.clear();
        Encoder::new(Some(&mut left))
            .emit(90, &event_left)
            .expect("left events");
        Encoder::new(Some(&mut right))
            .emit(90, &event_right)
            .expect("right events");
        assert!(first_field_difference(Some(&left), Some(&right)).is_some_and(|value| value.contains("events[1].row")));
    }

    #[test]
    fn capture_modes_and_transition_are_part_of_the_same_stream() {
        assert!(WitnessCapture::Full.captures(0));
        assert!(!WitnessCapture::Digest.captures(0));
        let window = WitnessCapture::ReplayWindow { start: 2, end: 4 };
        assert!(!(0..2).any(|frame| window.captures(frame)));
        assert!((2..=4).all(|frame| window.captures(frame)));
        assert!(!window.captures(5));

        let clean = test_frame(0, 7, true, WitnessTransitionOutcome::Clean);
        let interrupted = test_frame(0, 7, true, WitnessTransitionOutcome::RecoverableError);
        assert_ne!(clean.digest, interrupted.digest);
        let transition = &interrupted.records[interrupted.records.len() - 7..];
        assert_eq!(u16::from_le_bytes(transition[0..2].try_into().expect("tag")), 91);
    }

    #[test]
    fn spool_is_valid_json_and_chunks_without_growing_with_runtime() {
        let values = vec![3; CHUNK_FRAMES + 1];
        let mut header = test_header(WitnessCapture::Full, 50);
        header.config.limit_value = values.len() as u64;
        let artifact = write_test_artifact_with_header(
            &values,
            WitnessCapture::Full,
            header,
            WitnessCompletion::Complete {
                frames: values.len() as u64,
            },
        );
        let root: serde_json::Value =
            serde_json::from_reader(File::open(&artifact.0).expect("fixture")).expect("valid JSON root");
        assert_eq!(root["chunks"].as_array().expect("chunks").len(), 2);
        assert_eq!(root["completion"]["status"], "complete");
        #[cfg(feature = "tooling")]
        {
            validate_complete(&artifact.0).expect("multi-chunk artifact must stream back through its reader");
            assert_eq!(
                diff_witness_paths(&artifact.0, &artifact.0, None, None),
                WitnessDiffResult::Equal {
                    frames: values.len() as u64
                }
            );

            let encoded = std::fs::read_to_string(&artifact.0).expect("serialized artifact");
            let malformed = [
                encoded.replacen("},\n{", "}\n{", 1),
                encoded.replacen("},\n{", "},\n,{", 1),
                encoded.replacen("}\n],\"completion\":", "},\n],\"completion\":", 1),
            ];
            for (index, contents) in malformed.into_iter().enumerate() {
                let path = std::env::temp_dir().join(format!(
                    "rsvz-witness-delimiter-{}-{}-{index}.json",
                    std::process::id(),
                    NEXT_SPOOL.fetch_add(1, Ordering::Relaxed),
                ));
                let malformed = TempArtifact(path);
                std::fs::write(&malformed.0, contents.as_bytes()).expect("malformed fixture");
                assert!(validate_complete(&malformed.0).is_err());
            }
        }

        let empty = write_test_artifact(
            &[],
            WitnessCapture::Digest,
            50,
            WitnessCompletion::Invalid {
                frames: 0,
                first_error_frame: 0,
                message: "unsupported".to_owned(),
            },
        );
        let _: serde_json::Value = serde_json::from_reader(File::open(&empty.0).expect("empty fixture"))
            .expect("zero-frame artifact is valid JSON");
    }

    #[test]
    fn spool_rejects_a_header_larger_than_its_reader_limit() {
        let mut header = test_header(WitnessCapture::Digest, 50);
        header.repro.expanded_setup["padding"] = serde_json::Value::String("x".repeat(ARTIFACT_LINE_CAPACITY));
        assert!(matches!(
            WitnessSpool::new(&header),
            Err(WitnessError::Overflow("artifact header line"))
        ));
    }

    #[test]
    fn spool_flushes_at_the_byte_limit_and_cleans_up_after_a_writer_failure() {
        let header = test_header(WitnessCapture::Full, 50);
        let mut spool = WitnessSpool::new(&header).expect("spool");
        for frame in 0..2 {
            spool
                .push(FinishedFrame {
                    frame,
                    digest: [frame as u8; 16],
                    records: vec![frame as u8; CHUNK_CAPACITY / 2 + 1],
                    full: true,
                })
                .expect("large frame");
        }
        let artifact = spool
            .finish(WitnessCompletion::Complete { frames: 2 })
            .expect("finish byte-limited spool");
        let path = std::env::temp_dir().join(format!(
            "rsvz-witness-byte-test-{}-{}.json",
            std::process::id(),
            NEXT_SPOOL.fetch_add(1, Ordering::Relaxed),
        ));
        let mut file = File::create(&path).expect("create byte fixture");
        artifact.write_json(&mut file).expect("copy byte fixture");
        drop(file);
        let root: serde_json::Value =
            serde_json::from_reader(File::open(&path).expect("byte fixture")).expect("valid byte fixture");
        assert_eq!(root["chunks"].as_array().expect("chunks").len(), 2);
        drop(TempArtifact(path));

        let mut failed = WitnessSpool::new(&header).expect("failure spool");
        let failed_path = failed.path.clone();
        drop(failed.writer.take());
        for frame in 0..CHUNK_FRAMES {
            failed
                .push(test_frame(
                    frame as u64,
                    frame as u32,
                    false,
                    WitnessTransitionOutcome::Clean,
                ))
                .expect("buffer before flush");
        }
        assert!(
            failed
                .push(test_frame(
                    CHUNK_FRAMES as u64,
                    0,
                    false,
                    WitnessTransitionOutcome::Clean,
                ))
                .is_err()
        );
        drop(failed);
        assert!(!failed_path.exists());
    }

    #[cfg(feature = "tooling")]
    #[test]
    fn comparator_reports_difference_range_reconvergence_and_invalid_inputs() {
        let left = write_test_artifact(
            &[1, 2, 3, 4],
            WitnessCapture::Full,
            50,
            WitnessCompletion::Complete { frames: 4 },
        );
        let equal = write_test_artifact(
            &[1, 2, 3, 4],
            WitnessCapture::Full,
            50,
            WitnessCompletion::Complete { frames: 4 },
        );
        assert_eq!(
            diff_witness_paths(&left.0, &equal.0, None, None),
            WitnessDiffResult::Equal { frames: 4 },
        );

        let different = write_test_artifact(
            &[1, 9, 8, 4],
            WitnessCapture::Full,
            50,
            WitnessCompletion::Complete { frames: 4 },
        );
        assert!(matches!(
            diff_witness_paths(&left.0, &different.0, None, None),
            WitnessDiffResult::Different {
                first_frame: 1,
                last_contiguous_frame: 2,
                reconverged_at: Some(3),
                field: Some(_),
            }
        ));

        let shorter = write_test_artifact(
            &[1, 2],
            WitnessCapture::Full,
            50,
            WitnessCompletion::Complete { frames: 2 },
        );
        assert!(matches!(
            diff_witness_paths(&left.0, &shorter.0, None, None),
            WitnessDiffResult::Different {
                first_frame: 2,
                last_contiguous_frame: 3,
                reconverged_at: None,
                field: None,
            }
        ));

        let config_mismatch = write_test_artifact(
            &[1, 2, 3, 4],
            WitnessCapture::Full,
            75,
            WitnessCompletion::Complete { frames: 4 },
        );
        assert!(matches!(
            diff_witness_paths(&left.0, &config_mismatch.0, None, None),
            WitnessDiffResult::Invalid(_)
        ));
        let incomplete = write_test_artifact(
            &[1],
            WitnessCapture::Full,
            50,
            WitnessCompletion::Incomplete { frames: 1 },
        );
        assert!(matches!(
            diff_witness_paths(&incomplete.0, &incomplete.0, None, None),
            WitnessDiffResult::Invalid(_)
        ));

        let non_clean = write_test_artifact_with_outcome(WitnessTransitionOutcome::RecoverableError);
        assert!(validate_complete(&non_clean.0).is_err());
        assert!(matches!(
            diff_witness_paths(&non_clean.0, &non_clean.0, None, None),
            WitnessDiffResult::Invalid(_)
        ));

        let bad_count = write_test_artifact(
            &[1, 2],
            WitnessCapture::Digest,
            50,
            WitnessCompletion::Complete { frames: 1 },
        );
        assert!(matches!(
            diff_witness_paths(&bad_count.0, &bad_count.0, None, None),
            WitnessDiffResult::Invalid(_)
        ));

        let no_baseline = write_test_artifact(
            &[],
            WitnessCapture::Digest,
            50,
            WitnessCompletion::Complete { frames: 0 },
        );
        assert!(validate_complete(&no_baseline.0).is_err());
        assert!(matches!(
            diff_witness_paths(&no_baseline.0, &no_baseline.0, None, None),
            WitnessDiffResult::Invalid(_)
        ));

        let mut frames_zero = test_header(WitnessCapture::Digest, 50);
        frames_zero.config.limit_value = 0;
        let over_limit = write_test_artifact_with_header(
            &[1, 2],
            WitnessCapture::Digest,
            frames_zero,
            WitnessCompletion::Complete { frames: 2 },
        );
        assert!(validate_complete(&over_limit.0).is_err());
        assert!(matches!(
            diff_witness_paths(&over_limit.0, &over_limit.0, None, None),
            WitnessDiffResult::Invalid(_)
        ));
    }

    #[cfg(feature = "tooling")]
    #[test]
    fn artifact_reader_enforces_line_capture_and_full_digest_integrity() {
        let path = std::env::temp_dir().join(format!(
            "rsvz-witness-oversized-{}-{}.json",
            std::process::id(),
            NEXT_SPOOL.fetch_add(1, Ordering::Relaxed)
        ));
        let oversized = TempArtifact(path);
        std::fs::write(&oversized.0, vec![b'x'; ARTIFACT_LINE_CAPACITY + 1]).expect("oversized fixture");
        assert!(ArtifactReader::open(&oversized.0).is_err());

        let mut invalid_headers = Vec::new();
        let mut invalid = test_header(WitnessCapture::Full, 50);
        invalid.config.random_mode = "bogus".to_owned();
        invalid_headers.push(invalid);
        let mut invalid = test_header(WitnessCapture::Full, 50);
        invalid.config.sun_mode = "bogus".to_owned();
        invalid_headers.push(invalid);
        let mut invalid = test_header(WitnessCapture::Full, 50);
        invalid.config.limit_kind = "bogus".to_owned();
        invalid_headers.push(invalid);
        let mut invalid = test_header(WitnessCapture::Full, 50);
        invalid.config.initial_sun = i32::MAX as u32 + 1;
        invalid_headers.push(invalid);
        let mut invalid = test_header(WitnessCapture::Full, 50);
        invalid.config.random_mode = "seeded".to_owned();
        invalid.config.random_value = invalid.config.reset_seed ^ 1;
        invalid_headers.push(invalid);
        for header in invalid_headers {
            let artifact = write_test_artifact_with_header(
                &[7],
                WitnessCapture::Full,
                header,
                WitnessCompletion::Complete { frames: 1 },
            );
            assert!(ArtifactReader::open(&artifact.0).is_err());
        }

        let mut completed_rounds = test_header(WitnessCapture::Full, 50);
        completed_rounds.config.limit_kind = "completed_rounds".to_owned();
        completed_rounds.config.limit_value = 1;
        let artifact = write_test_artifact_with_header(
            &[7],
            WitnessCapture::Full,
            completed_rounds,
            WitnessCompletion::Complete { frames: 1 },
        );
        ArtifactReader::open(&artifact.0).expect("locked completed-round artifact");

        let mut legacy_seeded = test_header(WitnessCapture::Full, 50);
        legacy_seeded.config.random_mode = "seeded".to_owned();
        legacy_seeded.config.random_value = legacy_seeded.config.reset_seed;
        let artifact = write_test_artifact_with_header(
            &[7],
            WitnessCapture::Full,
            legacy_seeded,
            WitnessCompletion::Complete { frames: 1 },
        );
        ArtifactReader::open(&artifact.0).expect("legacy seeded artifact remains readable");

        let wrong_coverage = write_test_artifact_with_header(
            &[7],
            WitnessCapture::Digest,
            test_header(WitnessCapture::Full, 50),
            WitnessCompletion::Complete { frames: 1 },
        );
        assert!(validate_complete(&wrong_coverage.0).is_err());

        let frame = test_frame(0, 7, true, WitnessTransitionOutcome::Clean);
        assert!(validate_full_records(&frame.records).is_ok());
        let mut tampered = frame.records.clone();
        *tampered.last_mut().expect("transition payload") ^= 1;
        let mut raw = Vec::new();
        raw.extend_from_slice(&frame.frame.to_le_bytes());
        raw.extend_from_slice(&frame.digest);
        raw.extend_from_slice(&(tampered.len() as u32).to_le_bytes());
        raw.extend_from_slice(&tampered);
        assert!(decode_full_frames(&raw).is_err());

        let mut incomplete = Vec::new();
        incomplete.extend_from_slice(&6_u16.to_le_bytes());
        incomplete.extend_from_slice(&4_u32.to_le_bytes());
        incomplete.extend_from_slice(&7_u32.to_le_bytes());
        incomplete.extend_from_slice(&91_u16.to_le_bytes());
        incomplete.extend_from_slice(&1_u32.to_le_bytes());
        incomplete.push(0);
        assert!(validate_full_records(&incomplete).is_err());
    }

    #[cfg(feature = "tooling")]
    #[test]
    fn replay_must_match_each_original_even_when_new_replays_match_each_other() {
        let reference_left = write_test_artifact(
            &[1, 2, 3, 4],
            WitnessCapture::Digest,
            50,
            WitnessCompletion::Complete { frames: 4 },
        );
        let reference_right = write_test_artifact(
            &[1, 2, 3, 4],
            WitnessCapture::Digest,
            50,
            WitnessCompletion::Complete { frames: 4 },
        );
        let replay_left = write_test_artifact(
            &[1, 9, 3, 4],
            WitnessCapture::ReplayWindow { start: 2, end: 3 },
            50,
            WitnessCompletion::Complete { frames: 4 },
        );
        let replay_right = write_test_artifact(
            &[1, 9, 3, 4],
            WitnessCapture::ReplayWindow { start: 2, end: 3 },
            50,
            WitnessCompletion::Complete { frames: 4 },
        );
        assert!(matches!(
            diff_witness_paths(&replay_left.0, &replay_right.0, None, None),
            WitnessDiffResult::Invalid(_)
        ));
        assert!(matches!(
            diff_witness_paths(
                &replay_left.0,
                &replay_right.0,
                Some(&reference_left.0),
                Some(&reference_right.0),
            ),
            WitnessDiffResult::Invalid(_)
        ));

        let mut reference_header = test_header(WitnessCapture::Digest, 50);
        reference_header.backend_name = "1051".to_owned();
        reference_header.backend_version = "1.0.0.1051".to_owned();
        reference_header.config.limit_value = 100;
        let reference = write_test_artifact_with_header(
            &[1, 2, 3, 4, 5],
            WitnessCapture::Digest,
            reference_header,
            WitnessCompletion::Complete { frames: 5 },
        );
        let mut replay_header = test_header(WitnessCapture::ReplayWindow { start: 1, end: 3 }, 50);
        replay_header.backend_name = "1051".to_owned();
        replay_header.backend_version = "1.0.0.1051".to_owned();
        replay_header.config.limit_value = 3;
        let replay = write_test_artifact_with_header(
            &[1, 2, 3, 4],
            WitnessCapture::ReplayWindow { start: 1, end: 3 },
            replay_header.clone(),
            WitnessCompletion::Complete { frames: 4 },
        );
        assert!(validate_replay(&replay.0, &reference.0).is_ok());

        replay_header.backend_name = "pe".to_owned();
        replay_header.backend_version = "0.1".to_owned();
        let wrong_backend = write_test_artifact_with_header(
            &[1, 2, 3, 4],
            WitnessCapture::ReplayWindow { start: 1, end: 3 },
            replay_header,
            WitnessCompletion::Complete { frames: 4 },
        );
        assert!(validate_replay(&wrong_backend.0, &reference.0).is_err());
    }

    #[cfg(feature = "tooling")]
    #[test]
    fn canonical_visitor_covers_live_identity_rng_float_bits_and_event_order() {
        let owner = PeWorldOwner::new_reset(PeWorldConfig {
            // This test freezes one canonical byte fixture, not current defaults.
            initial_sun: 9_990,
            ..PeWorldConfig::default()
        })
        .expect("PE fixture reset");
        let mut installed = owner.install_current().expect("install PE fixture");
        let (plant_id, zombie_id) = installed.with_backend(|backend: &mut PeBackend| {
            let selection = CardSelection::Plant(PlantKind::Peashooter)
                .checked()
                .expect("selection");
            let plant = backend
                .add_plant(selection, Grid::new(0, 2).expect("grid"))
                .expect("plant");
            let plant_id = backend.plant_id(plant);
            let zombie = backend
                .add_zombie_in_row(ZombieKind::Normal, 0, 0)
                .expect("zombie")
                .expect("zombie handle");
            let zombie_id = backend.zombie_id(zombie);
            backend
                .add_gravestone(Grid::new(1, 7).expect("grave grid"))
                .expect("grave");
            (plant_id, zombie_id)
        });
        for _frame in 0..600 {
            if installed.with_backend(|backend| {
                rsvz_pvz_emulator_backend::scope_backend(backend, || {
                    rsvz_pvz_emulator_backend::with_backend_shared(|access| {
                        access.projectiles().unwrap().next().is_some()
                    })
                    .expect("Board scope")
                })
            }) {
                break;
            }
            installed.update_world().expect("advance PE fixture");
        }
        installed.with_backend(|backend: &mut PeBackend| {
            rsvz_pvz_emulator_backend::scope_backend(backend, || {
                rsvz_pvz_emulator_backend::with_backend_shared(|access| {
                    let backend = access;

                    assert!(backend.projectiles().unwrap().next().is_some());
                    let zombie = backend
                        .zombies()
                        .unwrap()
                        .find(|zombie| backend.zombie_id(*zombie) == zombie_id)
                        .expect("original zombie");
                    let zombie_x_bits = backend.zombie_pos_x(zombie).to_bits().to_le_bytes();
                    let events = [
                        GameEvent::HomeEntry(HomeEntryEvent {
                            zombie_id,
                            zombie_kind: ZombieKind::Normal,
                            row: 0,
                            main_counter: 9,
                        }),
                        GameEvent::PlantEffect(PlantEffectEvent {
                            source: PlantEffectSource::Bite(zombie_id),
                            plant_id,
                            raw_kind: PlantKind::Peashooter,
                            effective_kind: PlantKind::Peashooter,
                            grid: Grid::new(0, 2).expect("grid"),
                            requested: PlantEffect::HpDamage { native_requested: 4 },
                            decision_origin: EventDecisionOrigin::Native,
                            outcome: PlantEffectOutcome::HpDelta { applied: 4 },
                            hp_before: 300,
                            max_hp: 300,
                            main_counter: 9,
                        }),
                    ];

                    let GameEvent::PlantEffect(mut unsupported) = events[1] else {
                        unreachable!()
                    };
                    for source in [
                        PlantEffectSource::ZomboniCrush(zombie_id),
                        PlantEffectSource::CatapultCrush(zombie_id),
                        PlantEffectSource::Bungee(zombie_id),
                    ] {
                        unsupported.source = source;
                        let mut bytes = Vec::new();
                        put_event(
                            &mut bytes,
                            GameEvent::PlantEffect(unsupported),
                            1,
                            &WitnessIds::with_capacity(),
                        );
                        assert_eq!(bytes, [u8::MAX]);
                        validate_event_payload(&bytes).expect("schema v6 unknown event");
                    }
                    unsupported.source = PlantEffectSource::Bite(zombie_id);
                    unsupported.requested = PlantEffect::Steal;
                    unsupported.outcome = PlantEffectOutcome::Stolen;
                    let mut bytes = Vec::new();
                    put_event(
                        &mut bytes,
                        GameEvent::PlantEffect(unsupported),
                        1,
                        &WitnessIds::with_capacity(),
                    );
                    assert_eq!(bytes, [u8::MAX]);

                    let full = CanonicalVisitor::new()
                        .capture(backend, 1, 0, &events, true)
                        .expect("full canonical capture")
                        .finish(WitnessTransitionOutcome::Clean)
                        .expect("finish full");
                    let digest = CanonicalVisitor::new()
                        .capture(backend, 1, 0, &events, false)
                        .expect("digest canonical capture")
                        .finish(WitnessTransitionOutcome::Clean)
                        .expect("finish digest");
                    assert_eq!(
                        full.digest, digest.digest,
                        "Full and Digest must share one field stream"
                    );
                    validate_full_records(&full.records).expect("canonical Full frame must satisfy its frozen schema");
                    assert_eq!(
                        full.digest,
                        [
                            112, 178, 194, 158, 73, 244, 136, 247, 212, 147, 242, 212, 191, 198, 163, 207
                        ],
                        "canonical schema v6 bytes changed; bump the schema version intentionally",
                    );
                    assert!(digest.records.is_empty());

                    let mut records = full.records.as_slice();
                    let mut plant_payload = None;
                    let mut zombie_payload = None;
                    let mut wave_health_payload = None;
                    let mut random_payload = None;
                    let mut event_payload = None;
                    let mut tags = Vec::new();
                    while let Some((tag, payload)) = take_record(&mut records).expect("canonical record") {
                        tags.push(tag);
                        match tag {
                            4 => wave_health_payload = Some(payload),
                            30 => plant_payload = Some(payload),
                            40 => zombie_payload = Some(payload),
                            80 => random_payload = Some(payload),
                            90 => event_payload = Some(payload),
                            _ => {}
                        }
                    }
                    assert_eq!(
                        tags,
                        WITNESS_SCHEMA_FIELDS.iter().map(|field| field.tag).collect::<Vec<_>>()
                    );
                    let wave_health = wave_health_payload.expect("wave health");
                    assert_eq!(
                        i32::from_le_bytes(wave_health[4..8].try_into().expect("current health")),
                        i32::try_from(backend.current_wave_health().expect("PE current wave health"))
                            .expect("PE current wave health fits i32")
                    );
                    let plant = plant_payload.expect("plant state");
                    assert_eq!(object_id(plant), 1);
                    let zombie = zombie_payload.expect("zombie state");
                    assert_eq!(object_id(zombie), 1);
                    assert!(zombie.windows(4).any(|value| value == zombie_x_bits));
                    assert!(!random_payload.expect("random streams").is_empty());
                    let events = event_payload.expect("events");
                    assert_eq!(events[0], 1, "HomeEntry must remain first");
                    assert_eq!(u32::from_le_bytes(events[1..5].try_into().expect("home zombie id")), 1);
                    assert_eq!(events[17], 0, "PlantEffect must remain second");
                    assert_eq!(
                        u32::from_le_bytes(events[19..23].try_into().expect("effect zombie id")),
                        1
                    );
                    assert_eq!(
                        u32::from_le_bytes(events[23..27].try_into().expect("effect plant id")),
                        1
                    );
                })
                .expect("Board scope")
            });
        });
    }
}
