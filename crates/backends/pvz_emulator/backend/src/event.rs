use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};

use rsvz_backend_api::backend::{NativeEventBackend, NativeEventSink};
use rsvz_model::model::{
    BeginPlantEffect, EventDecision, EventToken, GargantuarAshHitFact, GargantuarSpawnedFact, Grid, HomeEntryFact,
    ImpThrownFact, PlantEffect, PlantEffectAttemptFact, PlantEffectOutcome, PlantEffectSource, PlantId, PlantKind,
    ProjectileId, ZombieId, ZombieKind, ZombiePhase,
};

use crate::{PeBackend, PeBackendError};

thread_local! {
    static SINK: Cell<Option<NativeEventSink>> = const { Cell::new(None) };
}

pub(crate) fn sink() -> Option<NativeEventSink> {
    SINK.get()
}

unsafe extern "C" fn begin_plant_effect(
    source: u8, actor_id: u32, plant_id: u32, raw_kind: i32, effective_kind: i32, row: i32, col: i32, hp: i32,
    max_hp: i32, effect: u8, native_requested: i32, main_counter: i32, out_suppress: *mut u8,
) -> u32 {
    if !out_suppress.is_null() {
        // SAFETY: PE supplies a live stack byte for the duration of this call.
        unsafe { out_suppress.write(0) };
    }
    catch_unwind(AssertUnwindSafe(|| {
        let Some(sink) = sink() else {
            return 0;
        };
        let source = match source {
            0 => PlantEffectSource::Jack(ZombieId::from_raw(actor_id)),
            1 => PlantEffectSource::Bite(ZombieId::from_raw(actor_id)),
            2 => PlantEffectSource::Gargantuar(ZombieId::from_raw(actor_id)),
            3 => PlantEffectSource::Basketball(ProjectileId::from_raw(actor_id)),
            4 => PlantEffectSource::ZomboniCrush(ZombieId::from_raw(actor_id)),
            5 => PlantEffectSource::CatapultCrush(ZombieId::from_raw(actor_id)),
            6 => PlantEffectSource::Bungee(ZombieId::from_raw(actor_id)),
            _ => return 0,
        };
        let Ok(raw_kind) = PlantKind::try_from_code(raw_kind) else {
            return 0;
        };
        let Ok(effective_kind) = PlantKind::try_from_code(effective_kind) else {
            return 0;
        };
        let effect = match effect {
            0 => PlantEffect::HpDamage { native_requested },
            1 => PlantEffect::InstantKill,
            2 => PlantEffect::Squish,
            3 => PlantEffect::Steal,
            _ => return 0,
        };
        let BeginPlantEffect { token, decision } = (sink.begin_plant_effect)(PlantEffectAttemptFact {
            source,
            plant_id: PlantId::from_raw(plant_id),
            raw_kind,
            effective_kind,
            grid: Grid { row, col },
            hp_before: hp,
            max_hp,
            effect,
            main_counter,
        });
        if decision == EventDecision::SuppressByMeasurement && !out_suppress.is_null() {
            // SAFETY: same live out byte validated above.
            unsafe { out_suppress.write(1) };
        }
        token.raw()
    }))
    .unwrap_or(0)
}

unsafe extern "C" fn finish_plant_effect(token: u32, outcome: u8, applied: i32) {
    let _panic = catch_unwind(AssertUnwindSafe(|| {
        let Some(sink) = sink() else {
            return;
        };
        let outcome = match outcome {
            0 => PlantEffectOutcome::SuppressedByMeasurement,
            1 => PlantEffectOutcome::PreventedByRule,
            2 => PlantEffectOutcome::HpDelta { applied },
            3 => PlantEffectOutcome::Killed,
            4 => PlantEffectOutcome::Squished,
            5 => PlantEffectOutcome::Activated,
            6 => PlantEffectOutcome::NoEffect,
            7 => PlantEffectOutcome::Stolen,
            _ => return,
        };
        (sink.finish_plant_effect)(EventToken::from_raw(token), outcome);
    }));
}

unsafe extern "C" fn emit_home_entry(zombie_id: u32, zombie_kind: i32, row: i32, main_counter: i32) {
    let _panic = catch_unwind(AssertUnwindSafe(|| {
        let Some(sink) = sink() else {
            return;
        };
        let Ok(zombie_kind) = ZombieKind::try_from_code(zombie_kind) else {
            return;
        };
        (sink.emit_home_entry)(HomeEntryFact {
            zombie_id: ZombieId::from_raw(zombie_id),
            zombie_kind,
            row,
            main_counter,
        });
    }));
}

unsafe extern "C" fn emit_gargantuar_spawned(
    parent_id: u32, parent_kind: i32, from_wave: i32, row: i32, main_counter: i32,
) {
    let _panic = catch_unwind(AssertUnwindSafe(|| {
        let (Some(sink), Ok(parent_kind)) = (sink(), ZombieKind::try_from_code(parent_kind)) else {
            return;
        };
        (sink.emit_gargantuar_spawned)(GargantuarSpawnedFact {
            parent_id: ZombieId::from_raw(parent_id),
            parent_kind,
            from_wave,
            row,
            main_counter,
        });
    }));
}

#[allow(clippy::too_many_arguments, reason = "mirrors the fixed PE C event ABI")]
unsafe extern "C" fn emit_imp_thrown(
    parent_id: u32, imp_id: u32, parent_kind: i32, from_wave: i32, row: i32, main_counter: i32, parent_x: f32,
    parent_hp: i32, parent_max_hp: i32, parent_phase: i32, parent_speed_x: f32, parent_frozen: i32,
    parent_chilled: i32, parent_buttered: i32,
) {
    let _panic = catch_unwind(AssertUnwindSafe(|| {
        let (Some(sink), Ok(parent_kind), Ok(parent_phase)) = (
            sink(),
            ZombieKind::try_from_code(parent_kind),
            ZombiePhase::try_from_code(parent_phase),
        ) else {
            return;
        };
        (sink.emit_imp_thrown)(ImpThrownFact {
            parent_id: ZombieId::from_raw(parent_id),
            imp_id: ZombieId::from_raw(imp_id),
            parent_kind,
            from_wave,
            row,
            main_counter,
            parent_x,
            parent_hp,
            parent_max_hp,
            parent_phase,
            parent_speed_x,
            parent_frozen,
            parent_chilled,
            parent_buttered,
        });
    }));
}

#[allow(clippy::too_many_arguments, reason = "mirrors the fixed PE C event ABI")]
unsafe extern "C" fn emit_gargantuar_ash_hit(
    parent_id: u32, main_counter: i32, x: f32, hp_before: i32, phase: i32, frozen: i32, chilled: i32, buttered: i32,
) {
    let _panic = catch_unwind(AssertUnwindSafe(|| {
        let (Some(sink), Ok(phase)) = (sink(), ZombiePhase::try_from_code(phase)) else {
            return;
        };
        (sink.emit_gargantuar_ash_hit)(GargantuarAshHitFact {
            parent_id: ZombieId::from_raw(parent_id),
            main_counter,
            x,
            hp_before,
            phase,
            frozen,
            chilled,
            buttered,
        });
    }));
}

impl NativeEventBackend for PeBackend {
    fn install_native_event_sink(&mut self, sink: NativeEventSink) -> Result<(), Self::Error> {
        if sink.interest.is_empty() {
            return Err(PeBackendError::OperationRejected("empty native event interest"));
        }
        if SINK.get().is_some() {
            return Err(PeBackendError::OperationRejected("native event sink already installed"));
        }
        self.with_current_world_mut(|world| {
            // SAFETY: all callbacks are static C-ABI trampolines. Their
            // runtime target is removed before the native table is cleared.
            unsafe {
                world.set_event_sink(
                    sink.interest.raw(),
                    Some(begin_plant_effect),
                    Some(finish_plant_effect),
                    Some(emit_home_entry),
                    Some(emit_gargantuar_spawned),
                    Some(emit_imp_thrown),
                    Some(emit_gargantuar_ash_hit),
                )
            }
        })??;
        SINK.set(Some(sink));
        Ok(())
    }

    fn remove_native_event_sink(&mut self) -> Result<(), Self::Error> {
        self.with_current_world_mut(pe_rs::World::clear_event_sink)??;
        SINK.set(None);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsvz_model::model::EventFrameStatus;

    thread_local! {
        static ATTEMPT: Cell<Option<PlantEffectAttemptFact>> = const { Cell::new(None) };
        static OUTCOME: Cell<Option<(EventToken, PlantEffectOutcome)>> = const { Cell::new(None) };
        static HOME: Cell<Option<HomeEntryFact>> = const { Cell::new(None) };
    }

    fn ignore_frame(_: u64, _: i32) {}

    fn intercept(attempt: PlantEffectAttemptFact) -> BeginPlantEffect {
        ATTEMPT.set(Some(attempt));
        BeginPlantEffect {
            token: EventToken::from_raw(77),
            decision: EventDecision::SuppressByMeasurement,
        }
    }

    fn record_outcome(token: EventToken, outcome: PlantEffectOutcome) {
        OUTCOME.set(Some((token, outcome)));
    }

    fn record_home(fact: HomeEntryFact) {
        HOME.set(Some(fact));
    }

    fn ignore_status(_: EventFrameStatus) {}

    fn ignore_garg_spawn(_: GargantuarSpawnedFact) {}
    fn ignore_imp_throw(_: ImpThrownFact) {}
    fn ignore_ash(_: GargantuarAshHitFact) {}

    fn test_sink(begin: fn(PlantEffectAttemptFact) -> BeginPlantEffect) -> NativeEventSink {
        NativeEventSink {
            interest: rsvz_model::model::EventInterest::PLANT_EFFECT,
            begin_logic_frame: ignore_frame,
            begin_plant_effect: begin,
            finish_plant_effect: record_outcome,
            emit_home_entry: record_home,
            emit_gargantuar_spawned: ignore_garg_spawn,
            emit_imp_thrown: ignore_imp_throw,
            emit_gargantuar_ash_hit: ignore_ash,
            end_logic_frame: ignore_status,
        }
    }

    #[test]
    fn ffi_trampolines_translate_all_effect_and_outcome_codes() {
        SINK.set(Some(test_sink(intercept)));
        let sources = [
            PlantEffectSource::Jack(ZombieId::from_raw(9)),
            PlantEffectSource::Bite(ZombieId::from_raw(9)),
            PlantEffectSource::Gargantuar(ZombieId::from_raw(9)),
            PlantEffectSource::Basketball(ProjectileId::from_raw(9)),
            PlantEffectSource::ZomboniCrush(ZombieId::from_raw(9)),
            PlantEffectSource::CatapultCrush(ZombieId::from_raw(9)),
            PlantEffectSource::Bungee(ZombieId::from_raw(9)),
        ];
        let effects = [
            PlantEffect::HpDamage { native_requested: 75 },
            PlantEffect::InstantKill,
            PlantEffect::Squish,
            PlantEffect::Steal,
        ];
        for (source_code, expected_source) in sources.into_iter().enumerate() {
            for (effect_code, expected_effect) in effects.into_iter().enumerate() {
                let mut suppress = 0;
                // SAFETY: `suppress` is live for the callback and all values are fixture scalars.
                let token = unsafe {
                    begin_plant_effect(
                        source_code as u8,
                        9,
                        10,
                        PlantKind::WallNut.code(),
                        PlantKind::Sunflower.code(),
                        2,
                        3,
                        100,
                        4_000,
                        effect_code as u8,
                        75,
                        123,
                        &raw mut suppress,
                    )
                };
                assert_eq!(token, 77);
                assert_eq!(suppress, 1);
                let attempt = ATTEMPT.take().expect("translated attempt");
                assert_eq!(attempt.source, expected_source);
                assert_eq!(attempt.effect, expected_effect);
                assert_eq!(attempt.plant_id, PlantId::from_raw(10));
                assert_eq!(attempt.raw_kind, PlantKind::WallNut);
                assert_eq!(attempt.effective_kind, PlantKind::Sunflower);
                assert_eq!(attempt.grid, Grid { row: 2, col: 3 });
                assert_eq!(
                    (attempt.hp_before, attempt.max_hp, attempt.main_counter),
                    (100, 4_000, 123)
                );
            }
        }

        let outcomes = [
            PlantEffectOutcome::SuppressedByMeasurement,
            PlantEffectOutcome::PreventedByRule,
            PlantEffectOutcome::HpDelta { applied: 12 },
            PlantEffectOutcome::Killed,
            PlantEffectOutcome::Squished,
            PlantEffectOutcome::Activated,
            PlantEffectOutcome::NoEffect,
            PlantEffectOutcome::Stolen,
        ];
        for (code, expected) in outcomes.into_iter().enumerate() {
            // SAFETY: fixture values match the C callback ABI.
            unsafe { finish_plant_effect(77, code as u8, 12) };
            assert_eq!(OUTCOME.take(), Some((EventToken::from_raw(77), expected)));
        }

        // SAFETY: fixture values match the C callback ABI.
        unsafe { emit_home_entry(11, ZombieKind::Pogo.code(), 4, 321) };
        assert_eq!(
            HOME.take(),
            Some(HomeEntryFact {
                zombie_id: ZombieId::from_raw(11),
                zombie_kind: ZombieKind::Pogo,
                row: 4,
                main_counter: 321,
            })
        );
        SINK.set(None);
    }

    #[test]
    fn invalid_or_panicking_begin_fails_open() {
        fn panic_begin(_: PlantEffectAttemptFact) -> BeginPlantEffect {
            panic!("PE event callback panic probe")
        }

        SINK.set(Some(test_sink(intercept)));
        let mut suppress = 9;
        // SAFETY: fixture out pointer is live; invalid source must be rejected.
        let token = unsafe {
            begin_plant_effect(
                u8::MAX,
                0,
                0,
                PlantKind::WallNut.code(),
                PlantKind::WallNut.code(),
                0,
                0,
                1,
                1,
                0,
                4,
                0,
                &raw mut suppress,
            )
        };
        assert_eq!((token, suppress), (0, 0));

        SINK.set(Some(test_sink(panic_begin)));
        suppress = 9;
        // SAFETY: fixture out pointer is live; trampoline catches the probe panic.
        let token = unsafe {
            begin_plant_effect(
                1,
                1,
                1,
                PlantKind::WallNut.code(),
                PlantKind::WallNut.code(),
                0,
                0,
                1,
                1,
                0,
                4,
                0,
                &raw mut suppress,
            )
        };
        assert_eq!((token, suppress), (0, 0));
        SINK.set(None);
    }
}
