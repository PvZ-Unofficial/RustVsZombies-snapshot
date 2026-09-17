use std::cell::{Cell, RefCell};
use std::ptr::NonNull;

use rsvz_model::{RandomMode, RandomStreamKind};

use crate::error::{Pvz1051Error, Result};

use super::memory::{encode_rel32, read_bytes, write_bytes};

const NEXT_NO_ASSERT: usize = 0x5a9940;
const GLOBAL_STREAM: usize = 0x75a910;
const ORIGINAL: [u8; 5] = [0x81, 0xba, 0xc0, 0x09, 0x00];
const SPAWN_WAVE_CALL_SITE: usize = 0x413fdf;
const SPAWN_WAVE_TARGET: usize = 0x412ee0;
const SPAWN_WAVE_ORIGINAL: [u8; 5] = [0xe8, 0xfc, 0xee, 0xff, 0xff];
// Objdump 0x412F7B/0x412FEA/0x413050: every SpawnZombieWave path calls
// PickRowForNewZombie at 0x40DC50 with Board* in EAX and ZombieType on stack.
const WAVE_ROW_PICK_CALL_SITES: [usize; 3] = [0x412f7b, 0x412fea, 0x413050];
const WAVE_ROW_PICK_ORIGINALS: [[u8; 5]; 3] = [
    [0xe8, 0xd0, 0xac, 0xff, 0xff],
    [0xe8, 0x61, 0xac, 0xff, 0xff],
    [0xe8, 0xfb, 0xab, 0xff, 0xff],
];
const PICK_ROW_TARGET: usize = 0x40dc50;
// Objdump 0x40AD94/0x42C945: both survival-stage callers keep Board* in EAX
// and call Board::InitZombieWaves at 0x40ABB0; neither uses the return value.
const SURVIVAL_STAGE_INIT_CALL_SITE: usize = 0x40ad94;
const SURVIVAL_STAGE_INIT_ORIGINAL: [u8; 5] = [0xe8, 0x17, 0xfe, 0xff, 0xff];
const SURVIVAL_REPICK_CALL_SITE: usize = 0x42c945;
const INIT_ZOMBIE_WAVES_TARGET: usize = 0x40abb0;
const SURVIVAL_REPICK_ORIGINAL: [u8; 5] = [0xe8, 0x66, 0xe2, 0xfd, 0xff];
const MT_WORDS: usize = 625;

#[derive(Clone, Copy)]
struct StreamState {
    seed: u32,
    fixed: u32,
    locked: bool,
}

impl StreamState {
    const fn new() -> Self {
        Self {
            seed: 4357,
            fixed: 0,
            locked: false,
        }
    }

    fn reset(&mut self, mode: RandomMode) {
        match mode {
            RandomMode::Seeded(seed) => {
                self.seed = if seed == 0 { 4357 } else { seed };
                self.fixed = 0;
                self.locked = false;
            }
            RandomMode::Locked(value) => {
                self.fixed = value & 0x7fff_ffff;
                self.locked = true;
            }
        }
    }
}

struct WaveState {
    words: [u32; MT_WORDS],
    base_seed: Option<u32>,
    active: bool,
    row_pick_active: bool,
}

impl WaveState {
    const fn new() -> Self {
        Self {
            words: [0; MT_WORDS],
            base_seed: None,
            active: false,
            row_pick_active: false,
        }
    }

    fn begin(&mut self, wave: u32) {
        assert!(!self.active, "wave spawn RNG scope cannot nest");
        let base_seed = self.base_seed.expect("wave hook requires a configured base seed");
        seed_words(&mut self.words, base_seed.wrapping_add(wave));
        self.active = true;
        self.row_pick_active = false;
    }

    fn end(&mut self) {
        assert!(!self.row_pick_active, "wave spawn row RNG scope remains active");
        self.active = false;
    }

    fn begin_row_pick(&mut self) {
        if self.active {
            assert!(!self.row_pick_active, "wave spawn row RNG scope cannot nest");
            self.row_pick_active = true;
        }
    }

    fn end_row_pick(&mut self) {
        if self.active {
            assert!(self.row_pick_active, "wave spawn row RNG scope was not active");
            self.row_pick_active = false;
        }
    }

    fn next(&mut self) -> Option<u32> {
        if !self.active || !self.row_pick_active {
            return None;
        }
        // SAFETY: `words` is the complete private MTRand layout initialized by `seed_words`.
        Some(unsafe { mt_next(self.words.as_mut_ptr()) })
    }
}

struct HookGuard {
    next_patched: [u8; 5],
    survival_stage_init_patched: [u8; 5],
    survival_repick_patched: [u8; 5],
}

impl HookGuard {
    fn install() -> Result<Self> {
        let next_current = read_bytes(NEXT_NO_ASSERT, ORIGINAL.len());
        if next_current != ORIGINAL {
            return Err(Pvz1051Error::Patch(format!(
                "MTRand::NextNoAssert expected {ORIGINAL:02x?}, found {next_current:02x?}"
            )));
        }
        let survival_stage_init_current = read_bytes(SURVIVAL_STAGE_INIT_CALL_SITE, SURVIVAL_STAGE_INIT_ORIGINAL.len());
        if survival_stage_init_current != SURVIVAL_STAGE_INIT_ORIGINAL {
            return Err(Pvz1051Error::Patch(format!(
                "survival stage init expected {SURVIVAL_STAGE_INIT_ORIGINAL:02x?}, found {survival_stage_init_current:02x?}"
            )));
        }
        let survival_repick_current = read_bytes(SURVIVAL_REPICK_CALL_SITE, SURVIVAL_REPICK_ORIGINAL.len());
        if survival_repick_current != SURVIVAL_REPICK_ORIGINAL {
            return Err(Pvz1051Error::Patch(format!(
                "survival wave repick expected {SURVIVAL_REPICK_ORIGINAL:02x?}, found {survival_repick_current:02x?}"
            )));
        }
        let next_patched = encode_rel32(
            0xe9,
            NEXT_NO_ASSERT,
            next_no_assert_shim as *const () as usize,
            "MTRand hook",
        )?;
        let survival_stage_init_patched = encode_rel32(
            0xe8,
            SURVIVAL_STAGE_INIT_CALL_SITE,
            survival_repick_shim as *const () as usize,
            "survival stage init shim",
        )?;
        let survival_repick_patched = encode_rel32(
            0xe8,
            SURVIVAL_REPICK_CALL_SITE,
            survival_repick_shim as *const () as usize,
            "survival wave repick shim",
        )?;
        write_bytes(NEXT_NO_ASSERT, &next_patched)?;
        if let Err(error) = write_bytes(SURVIVAL_STAGE_INIT_CALL_SITE, &survival_stage_init_patched) {
            return super::memory::finish_restoration(
                Err(error),
                write_bytes(NEXT_NO_ASSERT, &ORIGINAL),
                "RNG hook install",
            )
            .map(|()| unreachable!());
        }
        if let Err(error) = write_bytes(SURVIVAL_REPICK_CALL_SITE, &survival_repick_patched) {
            let stage = write_bytes(SURVIVAL_STAGE_INIT_CALL_SITE, &SURVIVAL_STAGE_INIT_ORIGINAL);
            let next = write_bytes(NEXT_NO_ASSERT, &ORIGINAL);
            let restore = super::memory::finish_restoration(stage, next, "RNG hook rollback");
            return super::memory::finish_restoration(Err(error), restore, "RNG hook install").map(|()| unreachable!());
        }
        Ok(Self {
            next_patched,
            survival_stage_init_patched,
            survival_repick_patched,
        })
    }

    fn restore(&mut self) -> Result<()> {
        let mut result = Ok(());
        for (site, original, patched, name) in [
            (
                SURVIVAL_REPICK_CALL_SITE,
                &SURVIVAL_REPICK_ORIGINAL,
                &self.survival_repick_patched,
                "survival wave repick hook",
            ),
            (
                SURVIVAL_STAGE_INIT_CALL_SITE,
                &SURVIVAL_STAGE_INIT_ORIGINAL,
                &self.survival_stage_init_patched,
                "survival stage init hook",
            ),
            (NEXT_NO_ASSERT, &ORIGINAL, &self.next_patched, "MTRand hook"),
        ] {
            let restore = super::memory::restore_owned_bytes(site, original, patched, name);
            result = super::memory::finish_restoration(result, restore, "RNG hook restore");
        }
        result
    }
}

impl Drop for HookGuard {
    fn drop(&mut self) {
        if self.restore().is_err() {
            crate::runtime::hook::fail_and_request_unload();
        }
    }
}

#[unsafe(naked)]
unsafe extern "C" fn survival_repick_shim() {
    core::arch::naked_asm!(
        "pushfd",
        "pushad",
        "mov eax, esp",
        "sub esp, 528",
        "and esp, -16",
        "mov [esp + 512], eax",
        "fxsave [esp]",
        "fninit",
        "mov dword ptr [esp + 516], 0x1f80",
        "ldmxcsr [esp + 516]",
        "cld",
        "call {begin}",
        "fxrstor [esp]",
        "mov esp, [esp + 512]",
        "popad",
        "popfd",
        "mov edx, {target}",
        "call edx",
        "pushfd",
        "pushad",
        "mov eax, esp",
        "sub esp, 528",
        "and esp, -16",
        "mov [esp + 512], eax",
        "fxsave [esp]",
        "fninit",
        "mov dword ptr [esp + 516], 0x1f80",
        "ldmxcsr [esp + 516]",
        "cld",
        "call {end}",
        "fxrstor [esp]",
        "mov esp, [esp + 512]",
        "popad",
        "popfd",
        "ret",
        begin = sym begin_survival_repick,
        end = sym end_survival_repick,
        target = const INIT_ZOMBIE_WAVES_TARGET,
    )
}

extern "C" fn begin_survival_repick() {
    if std::panic::catch_unwind(begin_survival_repick_inner).is_err() {
        crate::runtime::hook::fail_and_request_unload();
    }
}

fn begin_survival_repick_inner() {
    SURVIVAL_REPICK_STREAMS.with(|saved| {
        assert!(saved.get().is_none(), "survival wave repick RNG scope cannot nest");
        STREAMS.with(|streams| {
            let mut streams = streams.borrow_mut();
            let Some((previous, seed)) = seed_locked_streams_for_survival_init(&mut streams) else {
                return;
            };
            saved.set(Some(previous));
            // SAFETY: LastStandCompletedStage is on the game thread and this
            // scope temporarily owns the process-global Battle stream.
            unsafe { crate::raw::abi::sexy_srand(seed) };
        });
    });
}

fn seed_locked_streams_for_survival_init(streams: &mut [StreamState; 2]) -> Option<([StreamState; 2], u32)> {
    if !streams[0].locked {
        return None;
    }
    let previous = *streams;
    let seed = streams[0].seed;
    for stream in streams.iter_mut() {
        stream.reset(RandomMode::Seeded(seed));
    }
    Some((previous, seed))
}

extern "C" fn end_survival_repick() {
    if std::panic::catch_unwind(|| {
        SURVIVAL_REPICK_STREAMS.with(|saved| {
            let Some(previous) = saved.take() else {
                return;
            };
            STREAMS.with(|streams| *streams.borrow_mut() = previous);
        });
    })
    .is_err()
    {
        crate::runtime::hook::fail_and_request_unload();
    }
}

struct OwnedCallPatch {
    address: usize,
    original: [u8; 5],
    patched: [u8; 5],
    name: &'static str,
}

impl OwnedCallPatch {
    fn install(address: usize, original: [u8; 5], target: usize, name: &'static str) -> Result<Self> {
        let current = read_bytes(address, original.len());
        if current != original {
            return Err(Pvz1051Error::Patch(format!(
                "{name} expected {original:02x?} at 0x{address:x}, found {current:02x?}"
            )));
        }
        let patched = encode_rel32(0xe8, address, target, name)?;
        write_bytes(address, &patched)?;
        Ok(Self {
            address,
            original,
            patched,
            name,
        })
    }

    fn restore(&mut self) -> Result<()> {
        super::memory::restore_owned_bytes(self.address, &self.original, &self.patched, self.name)
    }
}

impl Drop for OwnedCallPatch {
    fn drop(&mut self) {
        if self.restore().is_err() {
            crate::runtime::hook::fail_and_request_unload();
        }
    }
}

struct WaveHookGuard {
    spawn: OwnedCallPatch,
    row_picks: [OwnedCallPatch; 3],
}

impl WaveHookGuard {
    fn install() -> Result<Self> {
        Ok(Self {
            spawn: OwnedCallPatch::install(
                SPAWN_WAVE_CALL_SITE,
                SPAWN_WAVE_ORIGINAL,
                spawn_wave_shim as *const () as usize,
                "wave spawn RNG hook",
            )?,
            row_picks: [
                OwnedCallPatch::install(
                    WAVE_ROW_PICK_CALL_SITES[0],
                    WAVE_ROW_PICK_ORIGINALS[0],
                    wave_row_pick_shim as *const () as usize,
                    "wave bungee row RNG hook",
                )?,
                OwnedCallPatch::install(
                    WAVE_ROW_PICK_CALL_SITES[1],
                    WAVE_ROW_PICK_ORIGINALS[1],
                    wave_row_pick_shim as *const () as usize,
                    "wave bobsled replacement row RNG hook",
                )?,
                OwnedCallPatch::install(
                    WAVE_ROW_PICK_CALL_SITES[2],
                    WAVE_ROW_PICK_ORIGINALS[2],
                    wave_row_pick_shim as *const () as usize,
                    "wave normal row RNG hook",
                )?,
            ],
        })
    }

    fn restore(&mut self) -> Result<()> {
        let row_2 = self.row_picks[2].restore();
        let row_1 = self.row_picks[1].restore();
        let row_0 = self.row_picks[0].restore();
        let spawn = self.spawn.restore();
        row_2.and(row_1).and(row_0).and(spawn)
    }
}

#[unsafe(naked)]
unsafe extern "C" fn wave_row_pick_shim() {
    core::arch::naked_asm!(
        "pushfd",
        "pushad",
        "mov eax, esp",
        "sub esp, 528",
        "and esp, -16",
        "mov [esp + 512], eax",
        "fxsave [esp]",
        "fninit",
        "mov dword ptr [esp + 516], 0x1f80",
        "ldmxcsr [esp + 516]",
        "cld",
        "call {begin}",
        "fxrstor [esp]",
        "mov esp, [esp + 512]",
        "popad",
        "popfd",
        "push dword ptr [esp + 4]",
        "mov edx, {target}",
        "call edx",
        "pushfd",
        "pushad",
        "mov eax, esp",
        "sub esp, 528",
        "and esp, -16",
        "mov [esp + 512], eax",
        "fxsave [esp]",
        "fninit",
        "mov dword ptr [esp + 516], 0x1f80",
        "ldmxcsr [esp + 516]",
        "cld",
        "call {end}",
        "fxrstor [esp]",
        "mov esp, [esp + 512]",
        "popad",
        "popfd",
        "ret 4",
        begin = sym begin_wave_row_pick,
        end = sym end_wave_row_pick,
        target = const PICK_ROW_TARGET,
    )
}

extern "C" fn begin_wave_row_pick() {
    if std::panic::catch_unwind(|| WAVE_STATE.with(|state| state.borrow_mut().begin_row_pick())).is_err() {
        crate::runtime::hook::fail_and_request_unload();
    }
}

extern "C" fn end_wave_row_pick() {
    if std::panic::catch_unwind(|| WAVE_STATE.with(|state| state.borrow_mut().end_row_pick())).is_err() {
        crate::runtime::hook::fail_and_request_unload();
    }
}

thread_local! {
    static HOOK: RefCell<Option<HookGuard>> = const { RefCell::new(None) };
    static WAVE_HOOK: RefCell<Option<WaveHookGuard>> = const { RefCell::new(None) };
    static STREAMS: RefCell<[StreamState; 2]> = const { RefCell::new([StreamState::new(), StreamState::new()]) };
    static WAVE_STATE: RefCell<WaveState> = const { RefCell::new(WaveState::new()) };
    static SURVIVAL_REPICK_STREAMS: Cell<Option<[StreamState; 2]>> = const { Cell::new(None) };
}

fn ensure_hook() -> Result<()> {
    HOOK.with(|hook| {
        let mut hook = hook.borrow_mut();
        if hook.is_none() {
            *hook = Some(HookGuard::install()?);
        }
        Ok(())
    })
}

pub(crate) fn configure(mode: RandomMode) -> Result<()> {
    ensure_hook()?;
    STREAMS.with(|streams| {
        for stream in streams.borrow_mut().iter_mut() {
            stream.reset(mode);
        }
    });
    Ok(())
}

/// Construction uses a temporary stream; a fresh fight restarts at the requested origin.
pub(crate) struct ResetRandomGuard {
    after: RandomMode,
    active: bool,
}

impl ResetRandomGuard {
    pub(crate) fn begin(seed: u32, after: RandomMode) -> Result<Self> {
        configure(RandomMode::Seeded(seed))?;
        Ok(Self { after, active: true })
    }

    fn restore(&mut self) -> Result<()> {
        self.active = false;
        let result = configure(self.after);
        if let RandomMode::Seeded(seed) = self.after {
            // SAFETY: the existing verified native Battle RNG reset, outside loader lock.
            unsafe { crate::raw::abi::sexy_srand(seed) };
        }
        result
    }

    pub(crate) fn finish(mut self, operation: Result<()>) -> Result<()> {
        let restoration = self.restore();
        super::memory::finish_restoration(operation, restoration, "world reset RNG")
    }
}

impl Drop for ResetRandomGuard {
    fn drop(&mut self) {
        if self.active && self.restore().is_err() {
            crate::runtime::hook::fail_and_request_unload();
        }
    }
}

pub(crate) fn configure_wave_seed(base_seed: Option<u32>) -> Result<()> {
    WAVE_STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.base_seed = base_seed;
        state.active = false;
    });
    WAVE_HOOK.with(|hook| {
        let mut hook = hook.borrow_mut();
        if base_seed.is_some() {
            ensure_hook()?;
            if hook.is_none() {
                *hook = Some(WaveHookGuard::install()?);
            }
        } else if let Some(current) = hook.as_mut() {
            current.restore()?;
            *hook = None;
        }
        Ok(())
    })
}

pub(crate) fn seed(kind: RandomStreamKind) -> u32 {
    STREAMS.with_borrow(|streams| streams[usize::from(matches!(kind, RandomStreamKind::Level))].seed)
}
pub(crate) fn locked(kind: RandomStreamKind) -> bool {
    STREAMS.with_borrow(|streams| streams[usize::from(matches!(kind, RandomStreamKind::Level))].locked)
}
pub(crate) fn fixed(kind: RandomStreamKind) -> u32 {
    STREAMS.with_borrow(|streams| streams[usize::from(matches!(kind, RandomStreamKind::Level))].fixed)
}

pub(crate) const fn effective_seed(seed: u32) -> u32 {
    if seed == 0 { 4357 } else { seed }
}

pub(crate) fn note_reset_seed(seed: u32) {
    let seed = effective_seed(seed);
    STREAMS.with(|streams| {
        for stream in streams.borrow_mut().iter_mut() {
            stream.seed = seed;
        }
    });
}

pub(crate) const fn level_seed(board_seed: u32, player_id: u32, survival_stage: u32, game_mode: u32) -> u32 {
    board_seed
        .wrapping_add(player_id)
        .wrapping_add(survival_stage.wrapping_mul(101))
        .wrapping_add(game_mode)
}

pub(crate) fn restore() -> Result<()> {
    let wave_result = configure_wave_seed(None);
    let hook_result = HOOK.with(|hook| {
        let mut hook = hook.borrow_mut();
        let Some(guard) = hook.as_mut() else {
            return Ok(());
        };
        guard.restore()?;
        *hook = None;
        Ok(())
    });
    wave_result.and(hook_result)
}

#[unsafe(naked)]
unsafe extern "C" fn spawn_wave_shim() {
    core::arch::naked_asm!(
        "pushfd",
        "pushad",
        "mov eax, esp",
        "sub esp, 528",
        "and esp, -16",
        "mov [esp + 512], eax",
        "fxsave [esp]",
        "fninit",
        "mov dword ptr [esp + 516], 0x1f80",
        "ldmxcsr [esp + 516]",
        "cld",
        "mov edx, [esp + 512]",
        "push dword ptr [edx]",
        "call {begin}",
        "add esp, 4",
        "fxrstor [esp]",
        "mov esp, [esp + 512]",
        "popad",
        "popfd",
        "mov eax, {target}",
        "call eax",
        "pushfd",
        "pushad",
        "mov eax, esp",
        "sub esp, 528",
        "and esp, -16",
        "mov [esp + 512], eax",
        "fxsave [esp]",
        "fninit",
        "mov dword ptr [esp + 516], 0x1f80",
        "ldmxcsr [esp + 516]",
        "cld",
        "call {end}",
        "fxrstor [esp]",
        "mov esp, [esp + 512]",
        "popad",
        "popfd",
        "ret",
        begin = sym begin_wave,
        end = sym end_wave,
        target = const SPAWN_WAVE_TARGET,
    )
}

extern "C" fn begin_wave(board: *mut crate::raw::layout::Board) {
    if std::panic::catch_unwind(|| begin_wave_inner(board)).is_err() {
        crate::runtime::hook::fail_and_request_unload();
    }
}

fn begin_wave_inner(board: *mut crate::raw::layout::Board) {
    let board = NonNull::new(board).expect("wave spawn requires Board");
    // SAFETY: the verified call at 0x413FDF keeps the active Board in EDI.
    let wave = unsafe { crate::raw::layout::Board::current_wave(board.as_ptr()) };
    let wave = u32::try_from(wave).expect("wave spawn index must be nonnegative");
    assert!(wave < 20, "wave spawn index must be below 20");
    WAVE_STATE.with(|state| state.borrow_mut().begin(wave));
}

extern "C" fn end_wave() {
    if std::panic::catch_unwind(|| WAVE_STATE.with(|state| state.borrow_mut().end())).is_err() {
        crate::runtime::hook::fail_and_request_unload();
    }
}

#[unsafe(naked)]
unsafe extern "C" fn next_no_assert_shim() {
    core::arch::naked_asm!(
        "push edx",
        "call {next}",
        "add esp, 4",
        "ret",
        next = sym hooked_next,
    )
}

extern "C" fn hooked_next(stream: *mut u32) -> u32 {
    std::panic::catch_unwind(|| hooked_next_inner(stream)).unwrap_or_else(|_| {
        crate::runtime::hook::fail_and_request_unload();
        0
    })
}

fn hooked_next_inner(stream: *mut u32) -> u32 {
    if stream as usize == GLOBAL_STREAM
        && let Some(value) = WAVE_STATE.with(|state| state.borrow_mut().next())
    {
        return value;
    }
    let index = usize::from(stream as usize != GLOBAL_STREAM);
    STREAMS.with(|states| {
        let mut states = states.borrow_mut();
        let state = &mut states[index];
        if state.locked {
            state.fixed
        } else {
            // SAFETY: the verified hook passes the native MTRand object in EDX. Its layout is
            // 624 u32 words followed by the current index, exactly as MTRand.h declares.
            unsafe { mt_next(stream) }
        }
    })
}

unsafe fn mt_next(stream: *mut u32) -> u32 {
    const N: usize = 624;
    const M: usize = 397;
    // SAFETY: caller established the complete native MTRand layout.
    let words = unsafe { std::slice::from_raw_parts_mut(stream, N + 1) };
    let mut index = words[N] as usize;
    if index >= N {
        for k in 0..(N - M) {
            let y = (words[k] & 0x8000_0000) | (words[k + 1] & 0x7fff_ffff);
            words[k] = words[k + M] ^ (y >> 1) ^ if y & 1 == 0 { 0 } else { 0x9908_b0df };
        }
        for k in (N - M)..(N - 1) {
            let y = (words[k] & 0x8000_0000) | (words[k + 1] & 0x7fff_ffff);
            words[k] = words[k + M - N] ^ (y >> 1) ^ if y & 1 == 0 { 0 } else { 0x9908_b0df };
        }
        let y = (words[N - 1] & 0x8000_0000) | (words[0] & 0x7fff_ffff);
        words[N - 1] = words[M - 1] ^ (y >> 1) ^ if y & 1 == 0 { 0 } else { 0x9908_b0df };
        index = 0;
    }
    let mut value = words[index];
    words[N] = (index + 1) as u32;
    value ^= value >> 11;
    value ^= (value << 7) & 0x9d2c_5680;
    value ^= (value << 15) & 0xefc6_0000;
    value ^= value >> 18;
    value & 0x7fff_ffff
}

fn seed_words(words: &mut [u32; MT_WORDS], seed: u32) {
    words[0] = effective_seed(seed);
    for index in 1..624 {
        words[index] = 1_812_433_253_u32
            .wrapping_mul(words[index - 1] ^ (words[index - 1] >> 30))
            .wrapping_add(index as u32);
    }
    words[624] = 624;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded(seed: u32) -> [u32; MT_WORDS] {
        let mut words = [0; MT_WORDS];
        seed_words(&mut words, seed);
        words
    }

    #[test]
    fn mt_matches_verified_31_bit_vector() {
        let mut words = seeded(0x5eed_1051);
        let mut actual = [0; 5];
        for value in &mut actual {
            // SAFETY: `words` is the exact test layout expected by mt_next.
            *value = unsafe { mt_next(words.as_mut_ptr()) };
        }
        assert_eq!(
            actual,
            [253_997_743, 1_118_501_867, 1_017_248_747, 1_169_085_152, 223_323_071]
        );
    }

    #[test]
    fn level_seed_matches_get_level_rand_seed_wrapping_semantics() {
        assert_eq!(effective_seed(0), 4357);
        assert_eq!(level_seed(7, 3, 1, 1), 112);
        assert_eq!(level_seed(u32::MAX, 1, 0, 0), 0);
    }

    #[test]
    fn locked_metadata_keeps_the_effective_reset_seed() {
        STREAMS.with(|streams| {
            for stream in streams.borrow_mut().iter_mut() {
                stream.reset(RandomMode::Locked(9));
            }
        });
        note_reset_seed(0x5eed_1051);
        for kind in [RandomStreamKind::Battle, RandomStreamKind::Level] {
            assert_eq!(seed(kind), 0x5eed_1051);
            assert!(locked(kind));
            assert_eq!(fixed(kind), 9);
        }
    }

    #[test]
    fn private_wave_stream_uses_base_plus_zero_based_wave() {
        let mut state = WaveState::new();
        state.base_seed = Some(0x5eed_1051);
        state.begin(2);
        let mut expected = seeded(0x5eed_1053);
        assert_eq!(state.next(), None);
        state.begin_row_pick();
        // SAFETY: `expected` is a complete initialized MTRand layout.
        assert_eq!(state.next(), Some(unsafe { mt_next(expected.as_mut_ptr()) }));
        state.end_row_pick();
        assert_eq!(state.next(), None);
        state.begin_row_pick();
        // SAFETY: leaving the row scope must not advance the private stream.
        assert_eq!(state.next(), Some(unsafe { mt_next(expected.as_mut_ptr()) }));
        state.end_row_pick();
        state.end();
        assert_eq!(state.next(), None);
    }

    #[test]
    fn survival_wave_init_temporarily_seeds_and_can_restore_locked_streams() {
        let mut streams = [StreamState::new(), StreamState::new()];
        assert!(seed_locked_streams_for_survival_init(&mut streams).is_none());

        for stream in &mut streams {
            stream.reset(RandomMode::Seeded(22));
            stream.reset(RandomMode::Locked(7));
        }
        let (previous, seed) = seed_locked_streams_for_survival_init(&mut streams).expect("locked scope");
        assert_eq!(seed, 22);
        assert!(previous.iter().all(|stream| stream.locked && stream.fixed == 7));
        assert!(streams.iter().all(|stream| !stream.locked && stream.seed == 22));

        streams = previous;
        assert!(streams.iter().all(|stream| stream.locked && stream.fixed == 7));
    }

    #[test]
    fn verified_wave_spawn_call_targets_native_function() {
        assert_eq!(
            encode_rel32(0xe8, SPAWN_WAVE_CALL_SITE, SPAWN_WAVE_TARGET, "wave spawn").unwrap(),
            SPAWN_WAVE_ORIGINAL
        );
        for (site, original) in WAVE_ROW_PICK_CALL_SITES.into_iter().zip(WAVE_ROW_PICK_ORIGINALS) {
            assert_eq!(
                encode_rel32(0xe8, site, PICK_ROW_TARGET, "wave row pick").unwrap(),
                original
            );
        }
    }

    #[test]
    fn verified_survival_wave_init_calls_target_native_function() {
        assert_eq!(
            encode_rel32(
                0xe8,
                SURVIVAL_STAGE_INIT_CALL_SITE,
                INIT_ZOMBIE_WAVES_TARGET,
                "survival stage init"
            )
            .unwrap(),
            SURVIVAL_STAGE_INIT_ORIGINAL
        );
        assert_eq!(
            encode_rel32(
                0xe8,
                SURVIVAL_REPICK_CALL_SITE,
                INIT_ZOMBIE_WAVES_TARGET,
                "survival wave repick"
            )
            .unwrap(),
            SURVIVAL_REPICK_ORIGINAL
        );
    }
}
