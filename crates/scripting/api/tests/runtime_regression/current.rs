use super::*;

#[cfg(test)]
use rsvz_backend_api::backend::FastForwardBackend;
#[cfg(test)]
use rsvz_game::SessionArtifact;
#[cfg(test)]
use rsvz_game::bench::BenchOptions;
#[cfg(test)]
use rsvz_model::{RelativeTime, WaveTimingSnapshot, WorldResetConfig};

#[cfg(test)]
use crate::{DispatchOutcome, RuntimeError, RuntimeFrameDispatch, RuntimeResult, SessionJobKey};
#[cfg(test)]
use rsvz_schedule::tick::{TickLifetime, TickMeta};

#[cfg(all(test, any(feature = "pvz-1-0-0-1051", feature = "pvz-emulator")))]
mod backend_scope_contract_tests {
    use std::panic::{self, AssertUnwindSafe};

    use super::*;

    #[test]
    fn current_backend_access_requires_an_active_scope() {
        let result = panic::catch_unwind(AssertUnwindSafe(|| with_backend(|_| ())));
        assert!(result.is_err());
    }
}

#[cfg(all(test, any(feature = "pvz-1-0-0-1051", feature = "pvz-emulator")))]
mod session_control_tests {
    use std::io::{self, Write};

    use super::*;

    static FIRST_JOB: u8 = 1;
    static SECOND_JOB: u8 = 2;

    fn merge_count(target: &mut u64, source: u64) -> RuntimeResult<()> {
        *target += source;
        Ok(())
    }

    fn write_count(value: &u64, writer: &mut dyn Write) -> io::Result<()> {
        write!(writer, "{value}")
    }

    fn artifact(value: u64) -> SessionArtifact {
        SessionArtifact::new(value, merge_count, write_count)
    }

    fn reset_probe(_backend: &mut rsvz_current::CurrentBackend, _config: WorldResetConfig) -> RuntimeResult<()> {
        Ok(())
    }

    #[test]
    fn session_job_is_idempotent_across_generations_and_rejects_duplicate_declarations() {
        reset_runtime_state_preserving_backend();
        clear_state_hooks();
        crate::reset_session_resources();
        reset_session_control();
        finish_hook_installation().expect("install hooks");
        let first_generation = begin_script_generation().expect("first generation");
        let first = SessionJobKey::new(&FIRST_JOB);
        assert!(claim_session_job(first).expect("first claim"));
        assert_eq!(
            claim_session_job(first)
                .expect_err("one script generation cannot declare the job twice")
                .to_string(),
            "the same session job was declared twice in one script generation"
        );
        assert_eq!(
            claim_session_job(SessionJobKey::new(&SECOND_JOB))
                .expect_err("different job should conflict")
                .to_string(),
            "a different session job is already installed"
        );
        finish_script_generation(first_generation).expect("activate first generation");

        let second_generation = begin_script_generation().expect("second generation");
        assert!(!claim_session_job(first).expect("same job in a later generation is idempotent"));
        abort_script_generation(second_generation).expect("abort second generation");
    }

    #[test]
    fn artifact_is_set_and_taken_once() {
        reset_session_control();
        set_session_artifact(artifact(7)).expect("first artifact");
        assert_eq!(
            set_session_artifact(artifact(8))
                .expect_err("second artifact should fail")
                .to_string(),
            "session artifact is already set"
        );
        let artifact = take_session_artifact().expect("artifact should be returned once");
        let mut json = Vec::new();
        artifact.write_json(&mut json).expect("artifact should encode");
        assert_eq!(json, b"7");
        assert!(take_session_artifact().is_none());
    }

    #[test]
    fn stop_is_sticky_across_reload_and_fatal_keeps_the_first_error() {
        reset_session_control();
        stop_script();
        reset_runtime_state_preserving_backend();
        assert!(script_stop_requested());

        fail_script(RuntimeError::new("first"));
        fail_script(RuntimeError::new("second"));
        assert_eq!(take_fatal_session_error().expect("fatal error").to_string(), "first");
        assert!(script_stop_requested());

        reset_session_control();
        assert!(!script_stop_requested());
        assert!(take_fatal_session_error().is_none());
    }

    #[test]
    fn a_session_extension_can_queue_its_reset_then_forbid_another() {
        reset_session_control();
        request_world_reset(WorldResetConfig::default(), reset_probe).expect("initial reset");
        forbid_additional_world_resets();

        assert_eq!(
            request_world_reset(WorldResetConfig::default(), reset_probe)
                .expect_err("later reset must be rejected")
                .to_string(),
            "world reset is disabled for this session"
        );
        assert!(world_reset_pending());
        reset_session_control();
    }
}

#[cfg(all(test, feature = "pvz-emulator"))]
mod pe_backend_scope_tests {
    use std::panic::{self, AssertUnwindSafe};

    use rsvz_pvz_emulator_backend::PeWorldConfig;
    use rsvz_pvz_emulator_backend::runner_internal::PeWorldOwner;

    use super::*;

    #[test]
    fn scope_rejects_nested_borrows_and_restores_after_panic() {
        let owner = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("PE owner should construct");
        let mut world = owner.install_current().expect("PE world should install");

        world.with_backend(|backend| {
            scope_backend(backend, || {
                with_backend(|backend| assert!(backend.is_initialized()));
                let nested = panic::catch_unwind(AssertUnwindSafe(|| {
                    with_backend(|_| rsvz_pvz_emulator_backend::with_backend(|_| ()));
                }));
                assert!(nested.is_err());
                rsvz_pvz_emulator_backend::with_backend(|backend| assert!(backend.is_initialized()));
            });

            let unwind = panic::catch_unwind(AssertUnwindSafe(|| {
                scope_backend(backend, || panic!("scope unwind probe"));
            }));
            assert!(unwind.is_err());
            scope_backend(backend, || with_backend(|backend| assert!(backend.is_initialized())));
        });

        assert!(panic::catch_unwind(AssertUnwindSafe(|| with_backend(|_| ()))).is_err());
    }
}

#[cfg(all(test, feature = "pvz-emulator"))]
mod recoverable_dispatch_tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use rsvz_model::{GameUi, Wave};

    use rsvz_schedule::tick::{TickControl, TickOptions, TickPhase, TickTaskState};
    use rsvz_schedule::timeline::TimingViolationPolicy;

    use super::*;

    fn playing_meta(clock: i32) -> TickMeta {
        TickMeta {
            phase: TickPhase::Playing,
            game_ui: Some(GameUi::Playing),
            clock: Some(clock),
            is_new_frame: true,
        }
    }

    #[test]
    fn runtime_report_time_exists_only_while_dispatch_context_is_active() {
        reset_runtime_state_preserving_backend();
        let outer = RelativeTime::new(Wave(2), 723);
        let inner = RelativeTime::new(Wave(3), -200);

        assert_eq!(runtime_report_time(), None);
        with_runtime_report_time(Some(outer), || {
            assert_eq!(runtime_report_time(), Some(outer));
            with_runtime_report_time(Some(inner), || {
                assert_eq!(runtime_report_time(), Some(inner));
            });
            assert_eq!(runtime_report_time(), Some(outer));
        });
        assert_eq!(runtime_report_time(), None);

        let panic = std::panic::catch_unwind(|| {
            with_runtime_report_time(Some(outer), || panic!("report-time unwind probe"));
        });
        assert!(panic.is_err());
        assert_eq!(runtime_report_time(), None);
    }

    #[test]
    fn reporting_dispatch_runs_later_timeline_and_tick_callbacks_in_the_same_frame() {
        reset_runtime_state_preserving_backend();
        let log = Rc::new(RefCell::new(Vec::new()));

        with_timeline(|timeline| {
            let log_for_error = Rc::clone(&log);
            let _registration = timeline
                .at(Wave(1), 0, move || {
                    log_for_error.borrow_mut().push("timeline-error");
                    Err(RuntimeError::new("timeline failure"))
                })
                .expect("timeline error callback should register");
            let log_for_success = Rc::clone(&log);
            let _registration = timeline
                .at(Wave(1), 0, move || {
                    log_for_success.borrow_mut().push("timeline-success");
                    Ok(())
                })
                .expect("timeline success callback should register");
        });

        let (failed_task, successful_task) = with_scheduler(|scheduler| {
            let log_for_error = Rc::clone(&log);
            let failed_task = scheduler.spawn(TickOptions::any_dispatch(), move |_meta| {
                log_for_error.borrow_mut().push("tick-error");
                Err(RuntimeError::new("tick failure"))
            });
            let log_for_success = Rc::clone(&log);
            let successful_task = scheduler.spawn(TickOptions::any_dispatch(), move |_meta| {
                log_for_success.borrow_mut().push("tick-success");
                Ok(TickControl::Continue)
            });
            (failed_task, successful_task)
        });

        let mut errors = Vec::new();
        let result =
            dispatch_runtime_tick_reporting(WaveTimingSnapshot::minimal(100, Wave(1)), playing_meta(100), |error| {
                errors.push(error.to_string());
            });

        assert_eq!(result, RuntimeFrameDispatch::Continue);
        assert_eq!(
            log.borrow().as_slice(),
            ["timeline-error", "timeline-success", "tick-error", "tick-success"]
        );
        assert_eq!(errors, ["timeline failure", "tick failure"]);
        with_scheduler(|scheduler| {
            assert_eq!(scheduler.state(failed_task), TickTaskState::Running);
            assert_eq!(scheduler.state(successful_task), TickTaskState::Running);
        });
        log.borrow_mut().clear();
        errors.clear();
        let next =
            dispatch_runtime_tick_reporting(WaveTimingSnapshot::minimal(101, Wave(1)), playing_meta(101), |error| {
                errors.push(error.to_string())
            });
        assert_eq!(next, RuntimeFrameDispatch::Continue);
        assert_eq!(log.borrow().as_slice(), ["tick-error", "tick-success"]);
        assert_eq!(errors, ["tick failure"]);
        reset_runtime_state_preserving_backend();
    }

    #[test]
    fn timing_violation_runs_only_internal_finalizers_in_the_same_frame() {
        reset_runtime_state_preserving_backend();
        reset_session_control();
        with_timeline(|timeline| {
            timeline.set_timing_violation_policy(TimingViolationPolicy::ReportFailure);
            timeline
                .assume_wavelengths([(1, 601)])
                .expect("wavelength assumption should register");
        });
        let first = CurrentFrameSample {
            access_epoch: None,
            meta: playing_meta(100),
            snapshot: Some(WaveTimingSnapshot::minimal(100, Wave(1))),
            report_time: None,
        };
        assert_eq!(
            dispatch_current_frame_sample(&first, |_error| {}),
            RuntimeFrameDispatch::Continue
        );

        let log = Rc::new(RefCell::new(Vec::new()));
        with_scheduler(|scheduler| {
            let user_log = Rc::clone(&log);
            scheduler.spawn(TickOptions::any_dispatch(), move |_meta| {
                user_log.borrow_mut().push("user");
                Ok(TickControl::Continue)
            });
            let finalizer_log = Rc::clone(&log);
            scheduler.spawn_runtime_finalizer(TickOptions::any_dispatch(), move |_meta| {
                finalizer_log.borrow_mut().push("finalizer");
                assert_eq!(take_dispatch_outcome(), Some(DispatchOutcome::TimingViolation));
                stop_script();
                Ok(TickControl::Stop)
            });
        });
        let violating = CurrentFrameSample {
            access_epoch: None,
            meta: playing_meta(800),
            snapshot: Some(WaveTimingSnapshot::minimal(800, Wave(2))),
            report_time: None,
        };

        assert!(matches!(
            dispatch_current_frame_sample(&violating, |_error| {}),
            RuntimeFrameDispatch::TimingViolation(_)
        ));
        assert_eq!(log.borrow().as_slice(), ["finalizer"]);
        assert!(script_stop_requested());
        assert_eq!(take_dispatch_outcome(), None);

        reset_runtime_state_preserving_backend();
        reset_session_control();
    }
}

#[cfg(all(test, feature = "pvz-emulator"))]
mod lifecycle_dispatch_tests {
    use std::cell::{Cell, RefCell};
    use std::io::{self, Write};
    use std::rc::Rc;
    use std::time::Duration;

    use rsvz_pvz_emulator_backend::PeWorldConfig;
    use rsvz_pvz_emulator_backend::runner_internal::PeWorldOwner;
    use rsvz_schedule::state_hook::{StateEvent, StateHookCommandOutcome};
    use rsvz_schedule::tick::{TickControl, TickOptions, TickTaskState};

    use super::*;

    fn reset_session() {
        reset_runtime_state_preserving_backend();
        clear_state_hooks();
        crate::reset_session_resources();
        reset_session_control();
    }

    #[test]
    fn bench_starts_battle_fast_forward_at_enter_fight() {
        reset_session();
        let owner = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("PE owner should construct");
        let mut world = owner.install_current().expect("PE world should install");

        world.with_backend(|backend| {
            scope_backend(backend, || {
                finish_hook_installation().expect("install hooks");
                let generation = begin_script_generation().expect("begin script");
                install_bench_session(BenchOptions::new(Duration::from_secs(1))).expect("install Bench");
                assert!(with_script_setup(|setup| setup.seed_chooser_fast_forward.is_some()));
                assert!(!with_backend(|backend| backend.fast_forward_active()));
                finish_script_generation(generation).expect("finish script");

                enter_fight().expect("enter fight");
                assert!(with_backend(|backend| backend.fast_forward_active()));
                finalize_session().expect("finalize session");
            });
        });
    }

    #[test]
    fn idle_attempt_still_removes_an_installed_event_sink() {
        reset_session();
        let owner = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("PE owner should construct");
        let mut world = owner.install_current().expect("PE world should install");
        world.with_backend(|backend| {
            scope_backend(backend, || {
                finish_hook_installation().expect("hooks");
                let generation = begin_script_generation().expect("generation");
                with_events(|events| {
                    events.register_public_handler(
                        rsvz_model::EventInterest::HOME_ENTRY,
                        rsvz_schedule::event::EventOptions::new(),
                        Box::new(|_| {}),
                    )
                })
                .expect("observer");
                finish_script_generation(generation).expect("freeze");
                enter_fight().expect("install native sink");
                // Reproduce an idle logical attempt while the physical sink still exists.
                crate::reset_session_resources();
                reset_runtime_state_preserving_backend();
                close_attempt().expect("idle close removes the physical sink");
            })
        });
        // Finalization without a backend scope proves no physical removal remains pending.
        finalize_session().expect("sink fact was cleared by the preceding idle close");
    }

    fn merge_probe(target: &mut u8, source: u8) -> RuntimeResult<()> {
        *target = target.saturating_add(source);
        Ok(())
    }

    fn write_probe(value: &u8, writer: &mut dyn Write) -> io::Result<()> {
        write!(writer, "{value}")
    }

    fn reset_probe(_backend: &mut rsvz_current::CurrentBackend, _config: WorldResetConfig) -> RuntimeResult<()> {
        Ok(())
    }

    #[test]
    fn full_runtime_event_trace_is_ordered_and_complete() {
        reset_session();
        let trace = Rc::new(RefCell::new(Vec::new()));
        for event in [
            StateEvent::AfterAttach,
            StateEvent::BeforeScript,
            StateEvent::AfterScript,
            StateEvent::EnterFight,
            StateEvent::ExitFight,
            StateEvent::BeforeTick,
            StateEvent::AfterTick,
            StateEvent::BeforeExit,
        ] {
            let trace = Rc::clone(&trace);
            with_state_hooks(|hooks| {
                hooks.register(event, 0, move || {
                    trace.borrow_mut().push(event);
                    Ok(())
                });
            });
        }

        finish_hook_installation().expect("AfterAttach");
        let generation = begin_script_generation().expect("BeforeScript");
        finish_script_generation(generation).expect("AfterScript");
        enter_fight().expect("EnterFight");
        begin_logic_tick().expect("BeforeTick");
        finish_logic_tick().expect("AfterTick");
        close_attempt().expect("ExitFight");
        finalize_session().expect("BeforeExit");

        assert_eq!(
            trace.borrow().as_slice(),
            [
                StateEvent::AfterAttach,
                StateEvent::BeforeScript,
                StateEvent::AfterScript,
                StateEvent::EnterFight,
                StateEvent::BeforeTick,
                StateEvent::AfterTick,
                StateEvent::ExitFight,
                StateEvent::BeforeExit,
            ]
        );
    }

    #[test]
    fn before_exit_drains_after_stop_and_preserves_late_artifact_hook() {
        reset_session();
        let ran = Rc::new(Cell::new(0_u8));
        let first = Rc::clone(&ran);
        with_state_hooks(|hooks| {
            hooks.register(StateEvent::BeforeExit, 0, move || {
                first.set(first.get() + 1);
                Ok(())
            });
            let last = Rc::clone(&ran);
            hooks.register(StateEvent::BeforeExit, i32::MAX - 1, move || {
                last.set(last.get() + 1);
                set_session_artifact(SessionArtifact::new(7_u8, merge_probe, write_probe))
            });
        });

        finish_hook_installation().expect("AfterAttach");
        stop_script();
        finalize_session().expect("BeforeExit teardown");
        assert_eq!(ran.get(), 2);
        assert!(take_session_artifact().is_some());
    }

    #[test]
    fn after_tick_drains_after_stop_and_does_not_leak_late_outcomes() {
        reset_session();
        let ran = Rc::new(Cell::new(0_u8));
        with_state_hooks(|hooks| {
            let before = Rc::clone(&ran);
            hooks.register(StateEvent::AfterTick, -1, move || {
                before.set(before.get() + 1);
                Ok(())
            });
            let seal = Rc::clone(&ran);
            hooks.register(StateEvent::AfterTick, 0, move || {
                seal.set(seal.get() + 1);
                assert_eq!(take_dispatch_outcome(), Some(DispatchOutcome::RecoverableError));
                stop_script();
                Ok(())
            });
            let after = Rc::clone(&ran);
            hooks.register(StateEvent::AfterTick, 1, move || {
                after.set(after.get() + 1);
                record_dispatch_outcome(DispatchOutcome::RecoverableError);
                Ok(())
            });
        });

        finish_hook_installation().expect("AfterAttach");
        let generation = begin_script_generation().expect("BeforeScript");
        finish_script_generation(generation).expect("AfterScript");
        enter_fight().expect("EnterFight");
        begin_logic_tick().expect("BeforeTick");
        record_dispatch_outcome(DispatchOutcome::RecoverableError);
        finish_logic_tick().expect("AfterTick");

        assert_eq!(ran.get(), 3);
        assert_eq!(take_dispatch_outcome(), None);
        finalize_session().expect("BeforeExit");
    }

    #[test]
    fn public_events_run_before_before_tick_and_can_remove_themselves() {
        use rsvz_model::{EventInterest, HomeEntryFact, ZombieId, ZombieKind};
        use rsvz_schedule::event::{EventCommandOutcome, EventHandle, EventOptions};

        reset_session();
        let owner = PeWorldOwner::new_reset(PeWorldConfig::default()).expect("PE owner should construct");
        let mut world = owner.install_current().expect("PE world should install");
        world.with_backend(|backend| {
            scope_backend(backend, || {
                let log = Rc::new(RefCell::new(Vec::new()));
                let handle = Rc::new(Cell::new(None::<EventHandle>));

                finish_hook_installation().expect("AfterAttach");
                let generation = begin_script_generation().expect("BeforeScript");
                let callback_log = Rc::clone(&log);
                let callback_handle = Rc::clone(&handle);
                let registered = with_events(|events| {
                    events.register_public_handler(
                        EventInterest::HOME_ENTRY,
                        EventOptions::new(),
                        Box::new(move |_| {
                            callback_log.borrow_mut().push("event");
                            assert_eq!(
                                with_events(
                                    |events| events.remove_public_handler(callback_handle.get().expect("handle"))
                                ),
                                EventCommandOutcome::Applied
                            );
                        }),
                    )
                })
                .expect("observer");
                handle.set(Some(registered));
                with_events(|events| {
                    events.register_public_handler(
                        EventInterest::HOME_ENTRY,
                        EventOptions::new(),
                        Box::new(|_| panic!("event callback probe")),
                    )
                })
                .expect("panic observer");
                let hook_log = Rc::clone(&log);
                with_state_hooks(|hooks| {
                    hooks.register(StateEvent::BeforeTick, 0, move || {
                        hook_log.borrow_mut().push("before-tick");
                        Ok(())
                    });
                });
                finish_script_generation(generation).expect("AfterScript");
                enter_fight().expect("EnterFight");

                with_events(|events| {
                    events.begin_logic_frame(1, 5);
                    for zombie_id in [1, 2] {
                        events.emit_home_entry(HomeEntryFact {
                            zombie_id: ZombieId::from_raw(zombie_id),
                            zombie_kind: ZombieKind::Normal,
                            row: 0,
                            main_counter: 5,
                        });
                    }
                    events.end_logic_frame(rsvz_model::BattleStatus::Running);
                });
                begin_logic_tick().expect("event dispatch and BeforeTick");
                finish_logic_tick().expect("AfterTick");

                assert_eq!(log.borrow().as_slice(), ["event", "before-tick"]);
                assert_eq!(take_dispatch_outcome(), None);
                close_attempt().expect("ExitFight");
                finalize_session().expect("BeforeExit");
            });
        });
    }

    #[test]
    fn failed_generation_rolls_back_hooks_and_epochs_never_reuse() {
        reset_session();
        finish_hook_installation().expect("install");
        let failed = begin_script_generation().expect("failed generation");
        let failed_epoch = crate::with_lifecycle(|lifecycle| lifecycle.generation());
        let handle = with_state_hooks(|hooks| hooks.register(StateEvent::AfterScript, 0, || Ok(())));
        abort_script_generation(failed).expect("abort");
        assert_eq!(
            with_state_hooks(|hooks| hooks.remove(handle)),
            StateHookCommandOutcome::AlreadyRemoved
        );

        let next = begin_script_generation().expect("next generation");
        assert!(
            matches!((failed_epoch, crate::with_lifecycle(|lifecycle| lifecycle.generation())), (rsvz_game::lifecycle::GenerationState::Building(before), rsvz_game::lifecycle::GenerationState::Building(after)) if after > before)
        );
        abort_script_generation(next).expect("abort next");
        finalize_session().expect("finalize");
    }

    #[test]
    fn before_script_failure_rolls_back_hooks_registered_during_the_event() {
        reset_session();
        let added = Rc::new(RefCell::new(None));
        let added_by_hook = Rc::clone(&added);
        with_state_hooks(|hooks| {
            hooks.register(StateEvent::BeforeScript, 0, move || {
                let handle = with_state_hooks(|hooks| hooks.register(StateEvent::BeforeExit, 0, || Ok(())));
                *added_by_hook.borrow_mut() = Some(handle);
                Err(RuntimeError::new("BeforeScript failure"))
            });
        });

        finish_hook_installation().expect("install");
        assert_eq!(
            begin_script_generation()
                .expect_err("BeforeScript must fail")
                .to_string(),
            "BeforeScript failure"
        );
        let handle = added.borrow().expect("hook registered during BeforeScript");
        assert_eq!(
            with_state_hooks(|hooks| hooks.remove(handle)),
            StateHookCommandOutcome::AlreadyRemoved
        );
        assert_eq!(
            crate::with_lifecycle(|lifecycle| lifecycle.generation()),
            rsvz_game::lifecycle::GenerationState::None
        );
        finalize_session().expect("finalize");
    }

    #[test]
    fn after_script_failure_rolls_back_hooks_and_script_tasks_before_activation() {
        static TEST_JOB: u8 = 1;

        reset_session();
        let added = Rc::new(RefCell::new(None));
        let added_by_hook = Rc::clone(&added);
        with_state_hooks(|hooks| {
            hooks.register(StateEvent::AfterScript, 0, move || {
                let handle = with_state_hooks(|hooks| hooks.register(StateEvent::BeforeExit, 0, || Ok(())));
                *added_by_hook.borrow_mut() = Some(handle);
                Err(RuntimeError::new("AfterScript failure"))
            });
        });

        finish_hook_installation().expect("install");
        let generation = begin_script_generation().expect("BeforeScript");
        let task = with_scheduler(|scheduler| {
            scheduler.spawn(TickOptions::any_dispatch().lifetime(TickLifetime::Session), |_| {
                Ok(TickControl::Continue)
            })
        });
        assert!(claim_session_job(SessionJobKey::new(&TEST_JOB)).expect("claim job"));
        request_world_reset(WorldResetConfig::default(), reset_probe).expect("request reset");
        set_session_artifact(SessionArtifact::new(1_u8, merge_probe, write_probe)).expect("publish artifact");
        stop_script();
        record_dispatch_outcome(DispatchOutcome::RecoverableError);
        set_auto_enter(false);
        assert_eq!(
            finish_script_generation(generation)
                .expect_err("AfterScript must fail")
                .to_string(),
            "AfterScript failure"
        );
        let handle = added.borrow().expect("hook registered during AfterScript");
        assert_eq!(
            with_state_hooks(|hooks| hooks.remove(handle)),
            StateHookCommandOutcome::AlreadyRemoved
        );
        assert_eq!(
            with_scheduler(|scheduler| scheduler.state(task)),
            TickTaskState::Stopped
        );
        assert!(!world_reset_pending());
        assert!(!script_stop_requested());
        assert!(take_session_artifact().is_none());
        assert_eq!(take_dispatch_outcome(), None);
        assert!(auto_enter_enabled());
        assert_eq!(
            crate::with_lifecycle(|lifecycle| lifecycle.generation()),
            rsvz_game::lifecycle::GenerationState::None
        );
        static RETRY_JOB: u8 = 0;
        let retry = begin_script_generation().expect("retry registration");
        assert!(claim_session_job(SessionJobKey::new(&RETRY_JOB)).expect("rollback freed the old job slot"));
        abort_script_generation(retry).expect("abort retry");
        finalize_session().expect("finalize");
    }

    #[test]
    fn before_script_tasks_survive_generation_cleanup_and_before_tick_failure_aborts_tick() {
        reset_session();
        let task = Rc::new(RefCell::new(None));
        let task_for_hook = Rc::clone(&task);
        with_state_hooks(|hooks| {
            hooks.register(StateEvent::BeforeScript, 0, move || {
                let handle = with_scheduler(|scheduler| {
                    scheduler.spawn(TickOptions::any_dispatch(), |_| Ok(TickControl::Continue))
                });
                *task_for_hook.borrow_mut() = Some(handle);
                Ok(())
            });
            hooks.register(StateEvent::BeforeTick, 0, || {
                Err(RuntimeError::new("before tick failure"))
            });
        });

        finish_hook_installation().expect("install");
        let generation = begin_script_generation().expect("generation");
        let handle = task.borrow().expect("BeforeScript task");
        assert_eq!(
            with_scheduler(|scheduler| scheduler.state(handle)),
            TickTaskState::Running
        );
        finish_script_generation(generation).expect("activate");
        enter_fight().expect("fight");
        assert_eq!(
            begin_logic_tick().expect_err("BeforeTick must fail").to_string(),
            "before tick failure"
        );
        close_attempt().expect("tick was aborted before close");
        finalize_session().expect("finalize");
    }
}
