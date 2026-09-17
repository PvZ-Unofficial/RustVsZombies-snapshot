use std::cell::Cell;
use std::ffi::c_void;
use std::ptr::NonNull;

use rsvz_backend_api::backend::{NativeEventBackend, NativeEventSink};
use rsvz_model::model::{
    BattleStatus, BeginPlantEffect, EventToken, GargantuarAshHitFact, GargantuarSpawnedFact, Grid, HomeEntryFact,
    ImpThrownFact, PlantEffect, PlantEffectAttemptFact, PlantEffectOutcome, PlantEffectSource, PlantId, PlantKind,
    ProjectileId, ZombieId, ZombieKind, ZombiePhase,
};

use crate::{PortableBackend, Result};

thread_local! {
    static SINK: Cell<Option<NativeEventSink>> = const { Cell::new(None) };
    static FRAME_COUNTER: Cell<Option<i32>> = const { Cell::new(None) };
}

impl NativeEventBackend for PortableBackend {
    fn install_native_event_sink(&mut self, sink: NativeEventSink) -> Result<()> {
        if sink.interest.is_empty() {
            return Err(crate::PortableBackendError::OperationRejected(
                "empty native event interest",
            ));
        }
        if SINK.get().is_some() {
            return Err(crate::PortableBackendError::OperationRejected(
                "native event sink already installed",
            ));
        }
        SINK.set(Some(sink));
        if !register_battle_callbacks(sink.interest.raw()) {
            SINK.set(None);
            return Err(crate::PortableBackendError::OperationRejected(
                "native battle callback registration rejected",
            ));
        }
        Ok(())
    }

    fn remove_native_event_sink(&mut self) -> Result<()> {
        clear_native_event_sink();
        Ok(())
    }
}

#[doc(hidden)]
#[allow(dead_code, reason = "called by the native-export C ABI in generated runners")]
pub fn event_interest() -> u32 {
    SINK.get().map_or(0, |sink| sink.interest.raw())
}

pub(crate) fn clear_native_event_sink() {
    SINK.set(None);
    FRAME_COUNTER.set(None);
    #[cfg(not(test))]
    // SAFETY: revocation is idempotent and accesses no game objects.
    unsafe {
        pvzp_rs::raw::pvzp_rs_clear_battle()
    };
}

#[cfg(not(test))]
fn register_battle_callbacks(interest: u32) -> bool {
    use crate::ffi::callbacks::*;
    let callbacks = pvzp_rs::raw::pvzp_rs_battle_callbacks {
        beginLogicFrame: Some(rsvz_pvzp_begin_logic_frame),
        endLogicFrame: Some(rsvz_pvzp_end_logic_frame),
        beginPlantEffect: Some(rsvz_pvzp_begin_plant_effect),
        finishPlantEffect: Some(rsvz_pvzp_finish_plant_effect),
        emitHomeEntry: Some(rsvz_pvzp_emit_home_entry),
        emitGargantuarSpawned: Some(rsvz_pvzp_emit_gargantuar_spawned),
        emitImpThrown: Some(rsvz_pvzp_emit_imp_thrown),
        emitGargantuarAshHit: Some(rsvz_pvzp_emit_gargantuar_ash_hit),
    };
    // SAFETY: native copies these fixed callbacks; only EnterFight installs a live sink.
    unsafe { pvzp_rs::raw::pvzp_rs_register_battle(&callbacks, interest) != 0 }
}

#[cfg(test)]
fn register_battle_callbacks(_interest: u32) -> bool {
    true
}

#[doc(hidden)]
#[allow(dead_code, reason = "called by the native-export C ABI in generated runners")]
pub unsafe fn begin_plant_effect(
    source_kind: i32, actor: *mut c_void, plant: *mut pvzp_rs::raw::pvzp_rs_plant, effect_kind: i32, requested: i32,
) -> BeginPlantEffect {
    let Some(sink) = SINK.get() else {
        return BeginPlantEffect::FAIL_OPEN;
    };
    let (Some(actor), Some(plant)) = (NonNull::new(actor), NonNull::new(plant)) else {
        return BeginPlantEffect::FAIL_OPEN;
    };
    // SAFETY: Portable calls this synchronously from an occupied native object slot.
    let Ok(world) = (unsafe { pvzp_rs::World::current() }) else {
        return BeginPlantEffect::FAIL_OPEN;
    };
    // SAFETY: the current native call owns these object borrows until this function returns.
    let plant_ref = unsafe { pvzp_rs::Borrowed::from_non_null(plant) };
    let Ok(plant_id) = crate::access::plant_pool(&world).get_id(plant_ref) else {
        return BeginPlantEffect::FAIL_OPEN;
    };
    let source = match source_kind {
        3 => {
            let actor = actor.cast::<pvzp_rs::raw::pvzp_rs_projectile>();
            // SAFETY: source kind 3 is supplied only by the basketball hook.
            let actor = unsafe { pvzp_rs::Borrowed::from_non_null(actor) };
            let Ok(id) = crate::access::projectile_pool(&world).get_id(actor) else {
                return BeginPlantEffect::FAIL_OPEN;
            };
            PlantEffectSource::Basketball(ProjectileId::from_raw(id))
        }
        0..=2 | 4..=6 => {
            let actor = actor.cast::<pvzp_rs::raw::pvzp_rs_zombie>();
            // SAFETY: these source tags carry the acting zombie; basketball is separate.
            let actor = unsafe { pvzp_rs::Borrowed::from_non_null(actor) };
            let Ok(id) = crate::access::zombie_pool(&world).get_id(actor) else {
                return BeginPlantEffect::FAIL_OPEN;
            };
            let id = ZombieId::from_raw(id);
            match source_kind {
                0 => PlantEffectSource::Jack(id),
                1 => PlantEffectSource::Bite(id),
                2 => PlantEffectSource::Gargantuar(id),
                4 => PlantEffectSource::ZomboniCrush(id),
                5 => PlantEffectSource::CatapultCrush(id),
                _ => PlantEffectSource::Bungee(id),
            }
        }
        _ => return BeginPlantEffect::FAIL_OPEN,
    };
    let effect = match effect_kind {
        0 => PlantEffect::HpDamage {
            native_requested: requested,
        },
        1 => PlantEffect::InstantKill,
        2 => PlantEffect::Squish,
        3 => PlantEffect::Steal,
        _ => return BeginPlantEffect::FAIL_OPEN,
    };
    // SAFETY: `plant` is a live occupied slot for this synchronous native hook.
    let (raw_kind, imitater_kind, row, col, hp_before, max_hp) = unsafe {
        (
            std::ptr::addr_of!((*plant.as_ptr()).mSeedType).read(),
            std::ptr::addr_of!((*plant.as_ptr()).mImitaterType).read(),
            std::ptr::addr_of!((*plant.as_ptr())._base.mRow).read(),
            std::ptr::addr_of!((*plant.as_ptr()).mPlantCol).read(),
            std::ptr::addr_of!((*plant.as_ptr()).mPlantHealth).read(),
            std::ptr::addr_of!((*plant.as_ptr()).mPlantMaxHealth).read(),
        )
    };
    let Ok(raw_kind) = PlantKind::try_from_code(raw_kind) else {
        return BeginPlantEffect::FAIL_OPEN;
    };
    let effective_kind = if raw_kind == PlantKind::Imitator {
        let Ok(kind) = PlantKind::try_from_code(imitater_kind) else {
            return BeginPlantEffect::FAIL_OPEN;
        };
        kind
    } else {
        raw_kind
    };
    let main_counter = world.main_counter() as i32;
    (sink.begin_plant_effect)(PlantEffectAttemptFact {
        source,
        plant_id: PlantId::from_raw(plant_id),
        raw_kind,
        effective_kind,
        grid: Grid { row, col },
        hp_before,
        max_hp,
        effect,
        main_counter,
    })
}

#[doc(hidden)]
#[allow(dead_code, reason = "called by the native-export C ABI in generated runners")]
pub fn finish_plant_effect(token: u32, outcome: i32, applied: i32) {
    let Some(sink) = SINK.get() else { return };
    let token = EventToken::from_raw(token);
    if !token.is_valid() {
        return;
    }
    let outcome = match outcome {
        0 => PlantEffectOutcome::SuppressedByMeasurement,
        1 => PlantEffectOutcome::PreventedByRule,
        2 => PlantEffectOutcome::HpDelta { applied },
        3 => PlantEffectOutcome::Killed,
        4 => PlantEffectOutcome::Squished,
        5 => PlantEffectOutcome::Activated,
        7 => PlantEffectOutcome::Stolen,
        _ => PlantEffectOutcome::NoEffect,
    };
    (sink.finish_plant_effect)(token, outcome);
}

#[doc(hidden)]
#[allow(dead_code, reason = "called by the native-export C ABI in generated runners")]
pub unsafe fn emit_home_entry(zombie: *mut pvzp_rs::raw::pvzp_rs_zombie) {
    let (Some(sink), Some(zombie)) = (SINK.get(), NonNull::new(zombie)) else {
        return;
    };
    // SAFETY: Portable calls this synchronously from an occupied Zombie slot.
    let Ok(world) = (unsafe { pvzp_rs::World::current() }) else {
        return;
    };
    // SAFETY: the pointer remains live for this native hook.
    let zombie_ref = unsafe { pvzp_rs::Borrowed::from_non_null(zombie) };
    let Ok(id) = crate::access::zombie_pool(&world).get_id(zombie_ref) else {
        return;
    };
    // SAFETY: `zombie` is a live occupied slot.
    let (raw_kind, row) = unsafe {
        (
            std::ptr::addr_of!((*zombie.as_ptr()).mZombieType).read(),
            std::ptr::addr_of!((*zombie.as_ptr())._base.mRow).read(),
        )
    };
    let Ok(zombie_kind) = ZombieKind::try_from_code(raw_kind) else {
        return;
    };
    (sink.emit_home_entry)(HomeEntryFact {
        zombie_id: ZombieId::from_raw(id),
        zombie_kind,
        row,
        main_counter: world.main_counter() as i32,
    });
}

#[doc(hidden)]
#[allow(dead_code, reason = "called by the native-export C ABI in generated runners")]
pub unsafe fn emit_gargantuar_spawned(zombie: *mut pvzp_rs::raw::pvzp_rs_zombie) {
    let (Some(sink), Some(zombie)) = (SINK.get(), NonNull::new(zombie)) else {
        return;
    };
    // SAFETY: Portable calls this only while its current Board is live on the game thread.
    let Ok(world) = (unsafe { pvzp_rs::World::current() }) else {
        return;
    };
    // SAFETY: the native call owns this occupied slot until the callback returns.
    let zombie_ref = unsafe { pvzp_rs::Borrowed::from_non_null(zombie) };
    let Ok(id) = crate::access::zombie_pool(&world).get_id(zombie_ref) else {
        return;
    };
    // SAFETY: the same occupied Zombie slot remains live for these copied scalars.
    let (raw_kind, from_wave, row) = unsafe {
        (
            std::ptr::addr_of!((*zombie.as_ptr()).mZombieType).read(),
            std::ptr::addr_of!((*zombie.as_ptr()).mFromWave).read(),
            std::ptr::addr_of!((*zombie.as_ptr())._base.mRow).read(),
        )
    };
    let Ok(parent_kind) = ZombieKind::try_from_code(raw_kind) else {
        return;
    };
    (sink.emit_gargantuar_spawned)(GargantuarSpawnedFact {
        parent_id: ZombieId::from_raw(id),
        parent_kind,
        from_wave,
        row,
        main_counter: FRAME_COUNTER.get().unwrap_or_default(),
    });
}

#[doc(hidden)]
#[allow(dead_code, reason = "called by the native-export C ABI in generated runners")]
pub unsafe fn emit_imp_thrown(parent: *mut pvzp_rs::raw::pvzp_rs_zombie, imp: *mut pvzp_rs::raw::pvzp_rs_zombie) {
    let (Some(sink), Some(parent), Some(imp)) = (SINK.get(), NonNull::new(parent), NonNull::new(imp)) else {
        return;
    };
    // SAFETY: Portable calls this only while its current Board is live on the game thread.
    let Ok(world) = (unsafe { pvzp_rs::World::current() }) else {
        return;
    };
    // SAFETY: both native slots remain occupied until the synchronous callback returns.
    let parent_ref = unsafe { pvzp_rs::Borrowed::from_non_null(parent) };
    // SAFETY: same native lifetime as `parent_ref`.
    let imp_ref = unsafe { pvzp_rs::Borrowed::from_non_null(imp) };
    let (Ok(parent_id), Ok(imp_id)) = (
        crate::access::zombie_pool(&world).get_id(parent_ref),
        crate::access::zombie_pool(&world).get_id(imp_ref),
    ) else {
        return;
    };
    // SAFETY: the native throw hook supplies fully initialized, occupied parent and Imp slots.
    let (
        raw_kind,
        raw_phase,
        from_wave,
        row,
        parent_x,
        parent_hp,
        parent_max_hp,
        parent_speed_x,
        parent_frozen,
        parent_chilled,
        parent_buttered,
    ) = unsafe {
        (
            std::ptr::addr_of!((*parent.as_ptr()).mZombieType).read(),
            std::ptr::addr_of!((*parent.as_ptr()).mZombiePhase).read(),
            std::ptr::addr_of!((*parent.as_ptr()).mFromWave).read(),
            std::ptr::addr_of!((*parent.as_ptr())._base.mRow).read(),
            std::ptr::addr_of!((*parent.as_ptr()).mPosX).read(),
            std::ptr::addr_of!((*parent.as_ptr()).mBodyHealth).read(),
            std::ptr::addr_of!((*parent.as_ptr()).mBodyMaxHealth).read(),
            std::ptr::addr_of!((*parent.as_ptr()).mVelX).read(),
            std::ptr::addr_of!((*parent.as_ptr()).mIceTrapCounter).read(),
            std::ptr::addr_of!((*parent.as_ptr()).mChilledCounter).read(),
            std::ptr::addr_of!((*parent.as_ptr()).mButteredCounter).read(),
        )
    };
    let (Ok(parent_kind), Ok(parent_phase)) = (
        ZombieKind::try_from_code(raw_kind),
        ZombiePhase::try_from_code(raw_phase),
    ) else {
        return;
    };
    (sink.emit_imp_thrown)(ImpThrownFact {
        parent_id: ZombieId::from_raw(parent_id),
        imp_id: ZombieId::from_raw(imp_id),
        parent_kind,
        from_wave,
        row,
        main_counter: world.main_counter() as i32,
        parent_x,
        parent_hp,
        parent_max_hp,
        parent_phase,
        parent_speed_x,
        parent_frozen,
        parent_chilled,
        parent_buttered,
    });
}

#[doc(hidden)]
#[allow(dead_code, reason = "called by the native-export C ABI in generated runners")]
pub unsafe fn emit_gargantuar_ash_hit(zombie: *mut pvzp_rs::raw::pvzp_rs_zombie) {
    let (Some(sink), Some(zombie)) = (SINK.get(), NonNull::new(zombie)) else {
        return;
    };
    // SAFETY: Portable calls this only while its current Board is live on the game thread.
    let Ok(world) = (unsafe { pvzp_rs::World::current() }) else {
        return;
    };
    // SAFETY: the native call owns this occupied slot until the callback returns.
    let zombie_ref = unsafe { pvzp_rs::Borrowed::from_non_null(zombie) };
    let Ok(id) = crate::access::zombie_pool(&world).get_id(zombie_ref) else {
        return;
    };
    // SAFETY: the occupied Gargantuar slot remains live before burn routing begins.
    let (raw_phase, x, hp_before, frozen, chilled, buttered) = unsafe {
        (
            std::ptr::addr_of!((*zombie.as_ptr()).mZombiePhase).read(),
            std::ptr::addr_of!((*zombie.as_ptr()).mPosX).read(),
            std::ptr::addr_of!((*zombie.as_ptr()).mBodyHealth).read(),
            std::ptr::addr_of!((*zombie.as_ptr()).mIceTrapCounter).read(),
            std::ptr::addr_of!((*zombie.as_ptr()).mChilledCounter).read(),
            std::ptr::addr_of!((*zombie.as_ptr()).mButteredCounter).read(),
        )
    };
    let Ok(phase) = ZombiePhase::try_from_code(raw_phase) else {
        return;
    };
    (sink.emit_gargantuar_ash_hit)(GargantuarAshHitFact {
        parent_id: ZombieId::from_raw(id),
        main_counter: world.main_counter() as i32,
        x,
        hp_before,
        phase,
        frozen,
        chilled,
        buttered,
    });
}

#[doc(hidden)]
#[allow(dead_code, reason = "called by the native-export C ABI in generated runners")]
pub fn begin_logic_frame(board_epoch: u64, main_counter: i32) {
    FRAME_COUNTER.set(Some(main_counter));
    if let Some(sink) = SINK.get() {
        (sink.begin_logic_frame)(board_epoch, main_counter);
    }
}

#[doc(hidden)]
#[allow(dead_code, reason = "called by the native-export C ABI in generated runners")]
pub fn end_logic_frame(status: i32) {
    let status = match status {
        1 => BattleStatus::ObjectiveReached,
        2 => BattleStatus::Lost,
        _ => BattleStatus::Running,
    };
    if let Some(sink) = SINK.get() {
        (sink.end_logic_frame)(status);
    }
    FRAME_COUNTER.set(None);
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering};

    use rsvz_backend_api::backend::NativeEventBackend;
    use rsvz_model::model::{
        BeginPlantEffect, EventFrameStatus, EventInterest, EventToken, HomeEntryFact, PlantEffectAttemptFact,
        PlantEffectOutcome,
    };

    use super::*;

    static FRAME_EPOCH: AtomicU64 = AtomicU64::new(0);
    static FRAME_COUNTER: AtomicI32 = AtomicI32::new(0);
    static LAST_OUTCOME: Mutex<Option<(EventToken, PlantEffectOutcome)>> = Mutex::new(None);
    static END_STATUS: AtomicU32 = AtomicU32::new(0);

    fn on_begin_frame(epoch: u64, counter: i32) {
        FRAME_EPOCH.store(epoch, Ordering::Relaxed);
        FRAME_COUNTER.store(counter, Ordering::Relaxed);
    }

    fn on_begin_effect(_fact: PlantEffectAttemptFact) -> BeginPlantEffect {
        BeginPlantEffect::FAIL_OPEN
    }

    fn on_finish_effect(token: EventToken, outcome: PlantEffectOutcome) {
        *LAST_OUTCOME.lock().expect("outcome lock") = Some((token, outcome));
    }

    fn on_home_entry(_fact: HomeEntryFact) {}

    fn on_garg_spawn(_fact: GargantuarSpawnedFact) {}
    fn on_imp_throw(_fact: ImpThrownFact) {}
    fn on_ash(_fact: GargantuarAshHitFact) {}

    fn on_end_frame(status: EventFrameStatus) {
        let value = match status {
            BattleStatus::Running => 1,
            BattleStatus::ObjectiveReached => 2,
            BattleStatus::Lost => 3,
            _ => 4,
        };
        END_STATUS.store(value, Ordering::Relaxed);
    }

    fn sink(interest: EventInterest) -> NativeEventSink {
        NativeEventSink {
            interest,
            begin_logic_frame: on_begin_frame,
            begin_plant_effect: on_begin_effect,
            finish_plant_effect: on_finish_effect,
            emit_home_entry: on_home_entry,
            emit_gargantuar_spawned: on_garg_spawn,
            emit_imp_thrown: on_imp_throw,
            emit_gargantuar_ash_hit: on_ash,
            end_logic_frame: on_end_frame,
        }
    }

    #[test]
    fn sink_ownership_interest_frames_and_outcomes_are_exact() {
        clear_native_event_sink();
        let mut backend = PortableBackend::new();
        assert!(
            backend
                .install_native_event_sink(sink(EventInterest::default()))
                .is_err()
        );

        let interest = EventInterest::BITE.union(EventInterest::HOME_ENTRY);
        backend.install_native_event_sink(sink(interest)).expect("install sink");
        assert_eq!(event_interest(), interest.raw());
        assert!(backend.install_native_event_sink(sink(interest)).is_err());

        begin_logic_frame(7, 123);
        assert_eq!(FRAME_EPOCH.load(Ordering::Relaxed), 7);
        assert_eq!(FRAME_COUNTER.load(Ordering::Relaxed), 123);
        assert_eq!(super::FRAME_COUNTER.get(), Some(123));
        end_logic_frame(1);
        assert_eq!(END_STATUS.load(Ordering::Relaxed), 2);
        assert_eq!(super::FRAME_COUNTER.get(), None);

        let outcomes = [
            PlantEffectOutcome::SuppressedByMeasurement,
            PlantEffectOutcome::PreventedByRule,
            PlantEffectOutcome::HpDelta { applied: 19 },
            PlantEffectOutcome::Killed,
            PlantEffectOutcome::Squished,
            PlantEffectOutcome::Activated,
            PlantEffectOutcome::NoEffect,
        ];
        for (raw, expected) in outcomes.into_iter().enumerate() {
            finish_plant_effect(41, raw as i32, 19);
            assert_eq!(
                *LAST_OUTCOME.lock().expect("outcome lock"),
                Some((EventToken::from_raw(41), expected))
            );
        }
        finish_plant_effect(0, 3, 0);
        assert_eq!(
            *LAST_OUTCOME.lock().expect("outcome lock"),
            Some((EventToken::from_raw(41), PlantEffectOutcome::NoEffect))
        );

        backend.remove_native_event_sink().expect("remove sink");
        assert_eq!(event_interest(), 0);
    }
}
