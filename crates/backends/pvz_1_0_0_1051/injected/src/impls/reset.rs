use std::cell::Cell;
use std::ptr::NonNull;

use rsvz_backend_api::backend::WorldResetBackend;
use rsvz_model::{MAX_SEED_SLOTS, RandomMode, RandomStreamKind, ResetCardCooldowns, WorldResetConfig};

use crate::error::{Pvz1051Error, Result};
use crate::patches::leases::{self, BoolPatchId};
use crate::raw::{abi as asm, layout as ptrs};
use crate::runtime::Pvz1051Backend;

thread_local! {
    static PENDING_STAGE: Cell<Option<i32>> = const { Cell::new(None) };
    static PENDING_WORLD_ORIGIN: Cell<Option<i32>> = const { Cell::new(None) };
    static PENDING_CARD_COOLDOWNS: Cell<Option<ResetCardCooldowns>> = const { Cell::new(None) };
    static RESET_BASELINE: Cell<Option<ResetBaseline>> = const { Cell::new(None) };
}

#[derive(Clone, Copy)]
struct ResetBaseline {
    app_rand_seed: i32,
    player_level: i32,
}

struct ResetGuard {
    app: NonNull<ptrs::LawnApp>,
    player: NonNull<ptrs::PlayerInfo>,
    baseline: ResetBaseline,
    release_profile_patch_on_failure: bool,
    active: bool,
}

const fn random_mode_after_reset(locked: bool, fixed: u32, seed: u32) -> RandomMode {
    if locked {
        RandomMode::Locked(fixed)
    } else {
        RandomMode::Seeded(seed)
    }
}

impl ResetGuard {
    fn acquire(app: NonNull<ptrs::LawnApp>) -> Result<Self> {
        // SAFETY: `app` is the checked process-global LawnApp. PlayerInfo is
        // checked before any field is read.
        let player = unsafe { ptrs::LawnApp::player_info(app.as_ptr()) };
        let player = NonNull::new(player).ok_or(Pvz1051Error::NullUserData)?;
        let release_profile_patch_on_failure = !leases::bool_patch_owned(BoolPatchId::ProfileReadonly);
        if release_profile_patch_on_failure {
            leases::set_bool_patch(BoolPatchId::ProfileReadonly, true)?;
        }
        // SAFETY: both pointers were checked above and the copied fields are
        // process-local scalars restored before this reset returns.
        let baseline = RESET_BASELINE.get().unwrap_or_else(|| unsafe {
            let baseline = ResetBaseline {
                app_rand_seed: ptrs::LawnApp::app_rand_seed(app.as_ptr()),
                player_level: ptrs::PlayerInfo::level(player.as_ptr()),
            };
            RESET_BASELINE.set(Some(baseline));
            baseline
        });
        Ok(Self {
            app,
            player,
            baseline,
            release_profile_patch_on_failure,
            active: true,
        })
    }

    fn restore_scalars(&self) {
        // SAFETY: NewGame replaces Board, not LawnApp or PlayerInfo. Restoring
        // the first reset's baseline prevents trial progress from accumulating
        // in the real in-memory profile.
        unsafe {
            ptrs::LawnApp::set_app_rand_seed(self.app.as_ptr(), self.baseline.app_rand_seed);
            ptrs::PlayerInfo::set_level(self.player.as_ptr(), self.baseline.player_level);
        }
    }

    fn rollback(&mut self) -> Result<()> {
        self.restore_scalars();
        if self.release_profile_patch_on_failure {
            leases::set_bool_patch(BoolPatchId::ProfileReadonly, false)?;
        }
        self.active = false;
        Ok(())
    }

    fn finish(mut self) {
        self.restore_scalars();
        // Keep ProfileReadonly leased until backend-host cleanup so the
        // following native trial cannot write player progress to disk.
        self.active = false;
    }
}

impl Drop for ResetGuard {
    fn drop(&mut self) {
        if self.active {
            let _result = self.rollback();
        }
    }
}

fn restore_session_baseline() -> Result<()> {
    let Some(baseline) = RESET_BASELINE.get() else {
        return Ok(());
    };
    // SAFETY: this reads the process-global LawnApp slot and checks it before use.
    let app = NonNull::new(unsafe { ptrs::lawn_app() }).ok_or(Pvz1051Error::NullLawnApp)?;
    // SAFETY: `app` is the checked process-global LawnApp.
    let player = unsafe { ptrs::LawnApp::player_info(app.as_ptr()) };
    let player = NonNull::new(player).ok_or(Pvz1051Error::NullUserData)?;
    // SAFETY: both pointers were checked and these are the exact scalars
    // captured before the first successful reset attempt.
    unsafe {
        ptrs::LawnApp::set_app_rand_seed(app.as_ptr(), baseline.app_rand_seed);
        ptrs::PlayerInfo::set_level(player.as_ptr(), baseline.player_level);
    }
    RESET_BASELINE.set(None);
    Ok(())
}

impl WorldResetBackend for Pvz1051Backend {
    fn reset_world(&mut self, config: WorldResetConfig) -> Result<()> {
        let stage = i32::try_from(config.completed_rounds)
            .map_err(|_error| Pvz1051Error::ScriptConfig("completed round count exceeds the 1051 range".into()))?;
        let initial_sun = i32::try_from(config.initial_sun)
            .map_err(|_error| Pvz1051Error::ScriptConfig("initial sun exceeds the 1051 range".into()))?;

        PENDING_STAGE.set(None);
        PENDING_WORLD_ORIGIN.set(None);
        PENDING_CARD_COOLDOWNS.set(None);
        crate::runtime::hook::prepare_world_reset()?;

        let app = self.app();
        let guard = ResetGuard::acquire(app)?;
        // Preserve all seed bits; PvZ stores both seed fields as signed
        // integers but treats their bit pattern as the RNG input.
        let seed = config.seed as i32;
        // GetLevelRandSeed adds player id, game mode, and 101 per survival
        // stage. Store the inverse offset so `config.seed` is the final local
        // level RNG seed on both backends.
        let player =
            NonNull::new(unsafe { ptrs::LawnApp::player_info(app.as_ptr()) }).ok_or(Pvz1051Error::NullUserData)?;
        let level_offset = unsafe { ptrs::PlayerInfo::id(player.as_ptr()) }
            .wrapping_add(config.completed_rounds.wrapping_mul(101))
            .wrapping_add(unsafe { ptrs::LawnApp::game_mode(app.as_ptr()) } as u32);
        let board_seed = config.seed.wrapping_sub(level_offset) as i32;
        // Native NewGame/InitZombieWaves must be allowed to make progress even
        // when the requested fight uses a constant RNG value. This temporary
        // stream is construction-only; the fresh fight starts again from the
        // requested reset origin below.
        let post_reset_mode = random_mode_after_reset(
            crate::patches::random_locked(RandomStreamKind::Battle),
            crate::patches::random_fixed(RandomStreamKind::Battle),
            config.seed,
        );
        let random = crate::patches::ResetRandomGuard::begin(config.seed, post_reset_mode)?;
        let reset_result = (|| {
            // SAFETY: `app` is the checked current LawnApp. NewGame is the
            // objdump-verified board-replacing body and does not run PreNewGame's
            // save deletion path.
            unsafe {
                ptrs::LawnApp::set_app_rand_seed(app.as_ptr(), seed);
                asm::sexy_srand(config.seed);
                asm::lawn_app_new_game();
            }
            crate::impls::event::note_world_reset();

            let board = self.board()?;
            // SAFETY: `board` is the fresh Board returned through LawnApp after
            // NewGame. Survival's Challenge pointer is checked before use.
            let challenge = unsafe { ptrs::Board::challenge(board.as_ptr()) };
            let challenge = NonNull::new(challenge).ok_or(Pvz1051Error::NullChallenge)?;
            // SAFETY: the raw field offsets and InitZombieWaves calling convention
            // are objdump-verified for 1.0.0.1051. The target stage is present only
            // while waves are rebuilt; stage 0 keeps the fresh seed chooser safe.
            unsafe {
                ptrs::Board::set_sun(board.as_ptr(), initial_sun);
                ptrs::Board::set_board_rand_seed(board.as_ptr(), board_seed);
                *ptrs::Challenge::endless_rounds_mut(challenge.as_ptr()) = stage;
                asm::board_init_zombie_waves(board.as_ptr());
                *ptrs::Challenge::endless_rounds_mut(challenge.as_ptr()) = 0;
                *ptrs::LawnApp::app_counter_mut(app.as_ptr()) =
                    rsvz_backend_api::opening::mix_dancer_clock(config.seed) as i32;
            }
            Ok(())
        })();
        random.finish(reset_result)?;
        guard.finish();
        crate::patches::note_random_reset_seed(config.seed);
        PENDING_STAGE.set(Some(stage));
        PENDING_WORLD_ORIGIN.set(Some(rsvz_backend_api::opening::mix_dancer_clock(config.seed) as i32));
        PENDING_CARD_COOLDOWNS.set(Some(config.card_cooldowns));
        Ok(())
    }
}

pub(crate) fn pending_world_stage() -> Option<i32> {
    PENDING_STAGE.get()
}

pub(crate) fn restore_pending_world_stage(backend: &Pvz1051Backend) -> Result<bool> {
    if PENDING_STAGE.get().is_none() {
        return Ok(false);
    }
    let board = backend.board()?;
    // SAFETY: the current Board is checked above and its Challenge pointer is validated before
    // restoring the stage ahead of the native seed-packet commit or first native update.
    let challenge = unsafe { ptrs::Board::challenge(board.as_ptr()) };
    let challenge = NonNull::new(challenge).ok_or(Pvz1051Error::NullChallenge)?;
    Ok(apply_pending_world_stage(challenge))
}

pub(crate) fn apply_pending_card_cooldowns(seed_bank: NonNull<ptrs::SeedBank>) -> bool {
    let Some(policy) = PENDING_CARD_COOLDOWNS.take() else {
        return false;
    };
    if policy == ResetCardCooldowns::Ready {
        // SAFETY: `seed_bank` is the current Board's checked bank after the chooser committed its
        // packets. Clamping the native count keeps packet traversal inside the fixed ten slots.
        let count = unsafe { ptrs::SeedBank::packet_count(seed_bank.as_ptr()) }.clamp(0, MAX_SEED_SLOTS as i32);
        for index in 0..count as usize {
            // SAFETY: `index` is inside the clamped fixed SeedBank range. These writes mirror the
            // state changes made by native SeedBank::RefreshAllPackets, excluding its visual flash.
            unsafe {
                let packet = ptrs::SeedBank::packet(seed_bank.as_ptr(), index);
                ptrs::SeedPacket::set_refresh_counter(packet, 0);
                ptrs::SeedPacket::set_refreshing(packet, false);
                ptrs::SeedPacket::set_active(packet, true);
            }
        }
    }
    true
}

pub(crate) fn normalize_pending_world_origin(backend: &Pvz1051Backend) -> Result<bool> {
    if PENDING_WORLD_ORIGIN.get().is_none() {
        return Ok(false);
    }
    let board = backend.board()?;
    let app = backend.app();
    // SAFETY: this is the fresh reset Board at the first Playing boundary. Native chooser preview
    // objects are already marked dead; reclaiming them here removes UI-only residue before frame 0.
    unsafe { asm::board_process_delete_queue() };
    // SAFETY: the delete queue just reclaimed every chooser-only zombie. Reset only the empty
    // allocator's slot/free-list provenance so later gameplay objects start from the common pool
    // origin; the native generation key remains monotonic.
    if unsafe { !ptrs::DataArray::reset_empty_allocator(ptrs::Board::zombies(board.as_ptr())) } {
        return Err(crate::error::Pvz1051Error::InvariantViolated(
            "chooser zombie pool is not empty after origin cleanup",
        ));
    }
    Ok(apply_pending_world_origin(board, app))
}

fn apply_pending_world_origin(board: NonNull<ptrs::Board>, app: NonNull<ptrs::LawnApp>) -> bool {
    let Some(dancer_clock) = PENDING_WORLD_ORIGIN.take() else {
        return false;
    };
    // SAFETY: `board` is the checked Board of the reset world. Native closes
    // the chooser through host updates before the first Playing dispatch. Reset
    // those transport-only clocks so both backends expose the same fight origin.
    unsafe {
        *ptrs::Board::clock_mut(board.as_ptr()) = 0;
        *ptrs::LawnApp::app_counter_mut(app.as_ptr()) = dancer_clock;
    }
    true
}

fn apply_pending_world_stage(challenge: NonNull<ptrs::Challenge>) -> bool {
    let Some(stage) = PENDING_STAGE.take() else {
        return false;
    };
    // SAFETY: `challenge` is non-null and the stage is a copied scalar.
    unsafe { *ptrs::Challenge::endless_rounds_mut(challenge.as_ptr()) = stage };
    true
}

pub(crate) fn finish_session() -> Result<()> {
    PENDING_STAGE.set(None);
    PENDING_WORLD_ORIGIN.set(None);
    PENDING_CARD_COOLDOWNS.set(None);
    restore_session_baseline()
}

pub(crate) fn profile_isolation_active() -> bool {
    RESET_BASELINE.get().is_some()
}

#[cfg(test)]
mod tests {
    use std::ptr::NonNull;

    use super::{
        PENDING_CARD_COOLDOWNS, PENDING_STAGE, PENDING_WORLD_ORIGIN, apply_pending_card_cooldowns,
        apply_pending_world_origin, apply_pending_world_stage,
    };
    use crate::raw::layout as ptrs;
    use rsvz_model::{RandomMode, ResetCardCooldowns};

    #[test]
    fn reset_reseeds_unlocked_mode_and_preserves_locked_value() {
        assert_eq!(super::random_mode_after_reset(false, 0, 22), RandomMode::Seeded(22));
        assert_eq!(super::random_mode_after_reset(true, 7, 22), RandomMode::Locked(7));
    }

    #[test]
    fn pending_stage_is_applied_once_before_seed_packets_are_committed() {
        let mut storage = [0_u8; 0x70];
        let challenge = NonNull::new(storage.as_mut_ptr().cast::<ptrs::Challenge>()).expect("challenge storage");
        // SAFETY: `storage` covers the verified mSurvivalStage offset used by the layout accessor.
        unsafe { *ptrs::Challenge::endless_rounds_mut(challenge.as_ptr()) = 0 };
        PENDING_STAGE.set(Some(63));

        assert!(apply_pending_world_stage(challenge));
        assert!(!apply_pending_world_stage(challenge));
        // SAFETY: `challenge` still points into the live local storage.
        assert_eq!(unsafe { ptrs::Challenge::endless_rounds(challenge.as_ptr()) }, 63);
    }

    #[test]
    fn pending_card_cooldown_policy_is_applied_once_after_selection() {
        let mut storage = [0_u32; 0x350 / std::mem::size_of::<u32>()];
        let bank = NonNull::new(storage.as_mut_ptr().cast::<ptrs::SeedBank>()).expect("seed bank storage");
        // SAFETY: local storage covers the verified SeedBank count and first SeedPacket fields.
        unsafe {
            bank.as_ptr().cast::<u8>().add(0x24).cast::<i32>().write_unaligned(1);
            let packet = ptrs::SeedBank::packet(bank.as_ptr(), 0);
            ptrs::SeedPacket::set_refresh_counter(packet, 123);
            ptrs::SeedPacket::set_refreshing(packet, true);
            ptrs::SeedPacket::set_active(packet, false);
        }
        PENDING_CARD_COOLDOWNS.set(Some(ResetCardCooldowns::Ready));

        assert!(apply_pending_card_cooldowns(bank));
        assert!(!apply_pending_card_cooldowns(bank));
        // SAFETY: the first packet still points inside live local storage.
        unsafe {
            let packet = ptrs::SeedBank::packet(bank.as_ptr(), 0);
            assert_eq!(ptrs::SeedPacket::refresh_counter(packet), 0);
            assert!(!ptrs::SeedPacket::is_refreshing(packet));
            assert!(ptrs::SeedPacket::is_active(packet));

            ptrs::SeedPacket::set_refresh_counter(packet, 456);
            ptrs::SeedPacket::set_refreshing(packet, true);
            ptrs::SeedPacket::set_active(packet, false);
        }
        PENDING_CARD_COOLDOWNS.set(Some(ResetCardCooldowns::PreserveNative));
        assert!(apply_pending_card_cooldowns(bank));
        // SAFETY: PreserveNative must leave the same live packet untouched.
        unsafe {
            let packet = ptrs::SeedBank::packet(bank.as_ptr(), 0);
            assert_eq!(ptrs::SeedPacket::refresh_counter(packet), 456);
            assert!(ptrs::SeedPacket::is_refreshing(packet));
            assert!(!ptrs::SeedPacket::is_active(packet));
        }
    }

    #[test]
    fn pending_reset_origin_is_normalized_once_before_the_first_playing_dispatch() {
        let mut board_storage = [0_u32; 0x5574 / std::mem::size_of::<u32>()];
        let mut app_storage = [0_u32; 0x83c / std::mem::size_of::<u32>()];
        let board = NonNull::new(board_storage.as_mut_ptr().cast::<ptrs::Board>()).expect("board storage");
        let app = NonNull::new(app_storage.as_mut_ptr().cast::<ptrs::LawnApp>()).expect("app storage");
        // SAFETY: local storage covers the verified mMainCounter and mAppCounter offsets.
        unsafe { *ptrs::Board::clock_mut(board.as_ptr()) = 1 };
        unsafe { *ptrs::LawnApp::app_counter_mut(app.as_ptr()) = 3_877 };
        PENDING_WORLD_ORIGIN.set(Some(3_040));

        assert!(apply_pending_world_origin(board, app));
        assert!(!apply_pending_world_origin(board, app));
        // SAFETY: both pointers still point into live local storage.
        assert_eq!(unsafe { ptrs::Board::clock(board.as_ptr()) }, 0);
        assert_eq!(unsafe { ptrs::LawnApp::app_counter(app.as_ptr()) }, 3_040);
    }
}

pub(crate) fn world_reset_transition_pending() -> bool {
    PENDING_STAGE.get().is_some()
}
