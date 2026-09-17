#![cfg(feature = "pvz-emulator")]

use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};

use rsvz::{at, try_at};
use rsvz_backend_api::error::{RuntimeError, RuntimeResult};
use rsvz_model::{
    BattleStatus, EventInterest, HomeEntryFact, SessionShard, Wave, WaveTimingSnapshot, ZombieId, ZombieKind,
};
use rsvz_pvz_emulator_backend::{DispatchInput, DispatchResult, PeWorldConfig, runner_internal::PeWorldOwner};
use rsvz_schedule::event::EventOptions;
use rsvz_schedule::state_hook::StateEvent;
use rsvz_schedule::tick::{TickControl, TickMeta, TickOptions, TickPhase};
use rsvz_schedule::timeline::TimelineDispatchResult;

thread_local! {
    static TRACE: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    static REPORTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static SCENARIO: Cell<Scenario> = const { Cell::new(Scenario::Tick) };
}

fn push(value: &'static str) {
    TRACE.with_borrow_mut(|trace| trace.push(value));
}

fn reset() {
    rsvz_game::frame::reset_runtime_state_preserving_backend();
    rsvz_game::state_hook::clear_state_hooks();
    rsvz_game::lifecycle::reset_session_resources();
    rsvz_game::session::reset_session_control();
    TRACE.with_borrow_mut(Vec::clear);
    REPORTS.with_borrow_mut(Vec::clear);
}

fn meta(clock: i32) -> TickMeta {
    TickMeta {
        clock: Some(clock),
        phase: TickPhase::Playing,
        game_ui: Some(rsvz_model::GameUi::Playing),
        is_new_frame: true,
    }
}

#[test]
fn binding_and_execution_errors_have_separate_paths() {
    reset();
    let ordinary_at: fn(i32, i32, fn()) -> Option<rsvz_schedule::TimeHandle> = rsvz::at::<()>;
    let ordinary_try_at: fn(i32, i32, fn()) -> RuntimeResult<rsvz_schedule::TimeHandle> = rsvz::timeline::try_at::<()>;
    assert!(ordinary_try_at(-1, 0, || ()).is_err());
    let _function = ordinary_at;
    let logger = rsvz::replace_logger(|record: &rsvz::LogRecord<'_>| {
        REPORTS.with_borrow_mut(|reports| reports.push(record.message.to_owned()));
    });
    assert!(try_at(-1, 0, || ()).is_err());
    REPORTS.with_borrow(|reports| assert!(reports.is_empty()));
    assert!(at(-1, 0, || ()).is_none());
    REPORTS.with_borrow(|reports| assert_eq!(reports.len(), 1));
    REPORTS.with_borrow_mut(Vec::clear);

    rsvz::__run_script(|| {
        let handle = try_at(1, 0, || push("cancelled"))?;
        rsvz_schedule::timeline::with_timeline(|timeline| {
            timeline.stop(handle);
        });
        at(1, 0, || push("unit"));
        rsvz::timeline::at(1, 0, || None::<u8>);
        at(1, 0, || vec![1]);
        at(1, 0, || Ok(()));
        at(1, 0, || {
            Ok::<_, RuntimeError>(())?;
            Ok(())
        });
        at(1, 0, || Err::<(), _>(RuntimeError::new("queued")));
        at(1, 0, || push("after-error"));
        Ok(())
    })
    .unwrap();
    let result = rsvz_game::timeline::dispatch_timeline_tick_reporting(
        WaveTimingSnapshot::minimal(100, Wave(1)),
        meta(100),
        &mut rsvz_game::diagnostics::report_runtime_error,
    );
    assert_eq!(result, TimelineDispatchResult::Continue);
    TRACE.with_borrow(|trace| assert_eq!(&**trace, ["unit", "after-error"]));
    let handle = try_at(1, 0, || Err::<(), _>(RuntimeError::new("immediate"))).unwrap();
    assert_eq!(
        rsvz::timeline::try_at(1, 0, || {
            push("immediate-success");
            Ok(())
        })
        .is_ok(),
        true
    );
    rsvz_schedule::timeline::with_timeline(|timeline| {
        timeline.stop(handle);
    });
    REPORTS.with_borrow(|reports| assert_eq!(&**reports, ["queued", "immediate"]));
    rsvz::restore_logger(logger);
    reset();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scenario {
    Tick,
    ResetThenPanic,
    BeforeTick,
    AfterTick,
    Event,
    Logger,
    EventLogger,
}

fn current_time() -> Option<i32> {
    rsvz_schedule::timeline::with_timeline(|timeline| {
        Some(timeline.current_clock()? - timeline.wave_clocks().refresh_clock(Wave(1))?)
    })
}

fn nested_failure() {
    let Some(time) = current_time() else { return };
    push("before-failure");
    at(1, time, || -> () {
        panic!("nested immediate panic");
    });
    push("callback-returned");
}

fn install() -> RuntimeResult<()> {
    rsvz_schedule::state_hook::with_state_hooks(|hooks| {
        hooks.register(StateEvent::BeforeTick, 0, || {
            if SCENARIO.get() == Scenario::BeforeTick {
                nested_failure();
            }
            Ok(())
        });
        hooks.register(StateEvent::AfterTick, 0, || {
            if SCENARIO.get() == Scenario::AfterTick {
                nested_failure();
            }
            Ok(())
        });
        hooks.register(StateEvent::AfterTick, 1, || {
            push("after-tick");
            Ok(())
        });
        hooks.register(StateEvent::BeforeExit, 0, || {
            push("before-exit");
            Ok(())
        });
    });
    Ok(())
}

fn script() -> RuntimeResult<()> {
    rsvz::__run_script(|| {
        at(1, -599, || ());
        rsvz_schedule::tick::with_scheduler(|scheduler| {
            scheduler.spawn(TickOptions::any_dispatch(), |_| {
                match SCENARIO.get() {
                    Scenario::Tick => nested_failure(),
                    Scenario::ResetThenPanic if current_time().is_some() => {
                        rsvz_game::session::request_world_reset_with(rsvz::WorldResetConfig::default(), |_, _| {
                            push("physical-reset");
                            Ok(())
                        })?;
                        push("reset-requested");
                        nested_failure();
                    }
                    Scenario::Logger if current_time().is_some() => rsvz::log(rsvz::LogLevel::Info, "logger probe"),
                    _ => {}
                }
                Ok(TickControl::Continue)
            });
        });
        if matches!(SCENARIO.get(), Scenario::Logger | Scenario::EventLogger) {
            rsvz::set_logger(|_: &rsvz::LogRecord<'_>| nested_failure());
        }
        if matches!(SCENARIO.get(), Scenario::Event | Scenario::EventLogger) {
            rsvz_schedule::event::with_events(|events| {
                events.register_public_handler(
                    EventInterest::HOME_ENTRY,
                    EventOptions::new(),
                    Box::new(|_| {
                        if SCENARIO.get() == Scenario::EventLogger {
                            panic!("ordinary observer panic");
                        }
                        nested_failure();
                    }),
                )
            })
            .unwrap();
        }
        Ok(())
    })
}

#[test]
fn nested_immediate_panics_stop_the_real_dispatch_and_restore_borrows() {
    for scenario in [
        Scenario::Tick,
        Scenario::ResetThenPanic,
        Scenario::BeforeTick,
        Scenario::AfterTick,
        Scenario::Event,
        Scenario::Logger,
        Scenario::EventLogger,
    ] {
        reset();
        SCENARIO.set(scenario);
        let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
            .unwrap()
            .install_current()
            .unwrap();
        let mut terminal = None;
        for _ in 0..12 {
            let result = world.with_backend(|backend| {
                rsvz_game::dispatch::run(
                    backend,
                    DispatchInput {
                        shard: SessionShard::default(),
                        stop_requested: false,
                        completed_rounds: 0,
                    },
                    script,
                    install,
                )
            });
            match result {
                DispatchResult::Continue => {
                    world.update_world().unwrap();
                    if matches!(scenario, Scenario::Event | Scenario::EventLogger) && current_time().is_some() {
                        rsvz_schedule::event::with_events(|events| {
                            events.begin_logic_frame(1, 5);
                            events.emit_home_entry(HomeEntryFact {
                                zombie_id: ZombieId::from_raw(1),
                                zombie_kind: ZombieKind::Normal,
                                row: 0,
                                main_counter: 5,
                            });
                            events.end_logic_frame(BattleStatus::Running);
                        });
                    }
                }
                DispatchResult::SkipUpdate => {}
                DispatchResult::Stop { error, artifact } => {
                    assert!(artifact.is_none());
                    terminal = error;
                    break;
                }
            }
        }
        assert_eq!(
            terminal.expect("bounded run must stop").to_string(),
            "timeline immediate callback panicked",
            "{scenario:?}"
        );
        TRACE.with_borrow(|trace| {
            let failure = trace.iter().position(|entry| *entry == "before-failure").unwrap();
            assert_eq!(&trace[failure..], ["before-failure", "before-exit"], "{scenario:?}");
            if scenario == Scenario::ResetThenPanic {
                assert!(trace.contains(&"reset-requested"));
                assert!(!trace.contains(&"physical-reset"));
            }
        });
        assert!(catch_unwind(AssertUnwindSafe(|| rsvz_pvz_emulator_backend::with_backend(|_| ()))).is_err());
        world.with_backend(|backend| {
            rsvz_pvz_emulator_backend::scope_backend(backend, || {
                rsvz_pvz_emulator_backend::with_backend(|backend| assert!(backend.is_initialized()));
            })
        });
        rsvz_schedule::timeline::with_timeline(|timeline| assert!(!timeline.is_dispatching()));
        drop(world);
    }
    reset();
}

mod teardown {
    use super::*;
    use rsvz_game::lifecycle::{AttemptState, SessionState};

    thread_local! {
        static FAILURE: Cell<(StateEvent, bool, bool)> = const {
            Cell::new((StateEvent::ExitFight, false, false))
        };
    }

    fn install() -> RuntimeResult<()> {
        for (event, label) in [
            (StateEvent::ExitFight, "exit-fight"),
            (StateEvent::BeforeExit, "before-exit"),
        ] {
            rsvz_game::state_hook::register_fallible(event, 0, move || {
                push(label);
                let (failure_event, immediate, _) = FAILURE.get();
                if event == failure_event {
                    if immediate {
                        at(1, current_time().unwrap(), || -> () {
                            panic!("teardown immediate panic")
                        });
                        push("callback-returned");
                    } else {
                        panic!("ordinary teardown panic");
                    }
                }
                Ok(())
            });
        }
        Ok(())
    }

    fn script() -> RuntimeResult<()> {
        rsvz::__run_script(|| {
            at(1, -599, || ());
            rsvz::tick::spawn(TickOptions::any_dispatch(), |_| {
                if current_time().is_some() {
                    if FAILURE.get().2 {
                        rsvz::fail_script("first fatal");
                    } else {
                        rsvz::stop_script();
                    }
                }
                Ok(TickControl::Continue)
            });
            Ok(())
        })
    }

    #[test]
    fn immediate_termination_finishes_teardown_and_preserves_the_first_error() {
        for event in [StateEvent::ExitFight, StateEvent::BeforeExit] {
            for immediate in [false, true] {
                for initial_fatal in [false, true] {
                    reset();
                    FAILURE.set((event, immediate, initial_fatal));
                    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
                        .unwrap()
                        .install_current()
                        .unwrap();
                    let mut terminal = None;
                    for _ in 0..12 {
                        let result = world.with_backend(|backend| {
                            rsvz_game::dispatch::run(
                                backend,
                                DispatchInput {
                                    shard: SessionShard::default(),
                                    stop_requested: false,
                                    completed_rounds: 0,
                                },
                                script,
                                install,
                            )
                        });
                        match result {
                            DispatchResult::Continue => {
                                world.update_world().unwrap();
                            }
                            DispatchResult::SkipUpdate => {}
                            DispatchResult::Stop { error, artifact } => {
                                assert!(artifact.is_none());
                                terminal = error;
                                break;
                            }
                        }
                    }
                    let expected = if initial_fatal {
                        "first fatal"
                    } else if immediate {
                        "timeline immediate callback panicked"
                    } else {
                        "state-hook callback panicked"
                    };
                    assert_eq!(terminal.expect("bounded run must stop").to_string(), expected);
                    TRACE.with_borrow(|trace| assert_eq!(&**trace, ["exit-fight", "before-exit"]));
                    rsvz_game::lifecycle::with_lifecycle(|state| {
                        assert_eq!(state.session(), SessionState::Done);
                        assert_eq!(state.attempt(), AttemptState::Idle);
                    });
                    rsvz::with_timeline(|timeline| assert_eq!(timeline.diagnostics().pending_count, 0));
                    assert!(rsvz_game::session::take_fatal_session_error().is_none());
                    world.with_backend(|backend| {
                        rsvz_pvz_emulator_backend::scope_backend(backend, || {
                            rsvz::with_backend(|backend| assert!(backend.is_initialized()));
                        })
                    });
                }
            }
        }
        reset();
    }
}

mod local_abort {
    use super::*;
    use rsvz_schedule::state_hook::{StateHookDispatchResult, with_state_hooks};
    use rsvz_schedule::tick::{TickTaskState, with_scheduler};

    fn fail(message: &'static str) {
        rsvz_game::diagnostics::abort_operation(RuntimeError::new(message));
    }

    #[test]
    fn timeline_and_repeating_ticks_abort_locally_after_releasing_shared_access() {
        reset();
        let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
            .unwrap()
            .install_current()
            .unwrap();
        world.with_backend(|backend| {
            rsvz_pvz_emulator_backend::scope_backend(backend, || {
                let logger = rsvz::replace_logger(|record: &rsvz::LogRecord<'_>| {
                    // Error reporting runs after the failed operation releases its borrow.
                    rsvz_current::with_backend_shared(|_| ()).unwrap();
                    REPORTS.with_borrow_mut(|reports| reports.push(record.message.to_owned()));
                });
                rsvz::__run_script(|| {
                    at(1, 0, || {
                        rsvz_current::with_backend_shared(|_| fail("queued abort")).unwrap();
                        push("unreachable timeline");
                    });
                    at(1, 0, || push("later timeline"));
                    Ok(())
                })
                .unwrap();
                let repeated = rsvz::tick::spawn(TickOptions::playing_frame(), |_| {
                    push("tick");
                    fail("tick abort");
                    push("unreachable tick");
                    Ok(TickControl::Continue)
                });
                let once = rsvz::tick::spawn(TickOptions::once_playing_ready(), |_| {
                    fail("once abort");
                    Ok(TickControl::Continue)
                });
                for clock in [100, 101] {
                    assert_eq!(
                        rsvz_game::frame::dispatch_runtime_tick_reporting(
                            WaveTimingSnapshot::minimal(clock, Wave(1)),
                            meta(clock),
                            rsvz_game::diagnostics::report_runtime_error,
                        ),
                        rsvz_game::runtime::RuntimeFrameDispatch::Continue
                    );
                }
                with_scheduler(|scheduler| {
                    assert_eq!(scheduler.state(repeated), TickTaskState::Running);
                    assert_eq!(scheduler.state(once), TickTaskState::Stopped);
                });
                at(1, 1, || {
                    at(1, 1, || fail("nested abort"));
                    push("outer continues");
                });
                TRACE.with_borrow(|trace| assert_eq!(&**trace, ["later timeline", "tick", "tick", "outer continues"]));
                REPORTS.with_borrow(|reports| {
                    assert_eq!(
                        &**reports,
                        ["queued abort", "once abort", "tick abort", "tick abort", "nested abort"]
                    )
                });
                assert!(!rsvz_game::session::script_stop_requested());
                rsvz::restore_logger(logger);
            });
        });
        reset();
    }

    #[test]
    fn runtime_hooks_continue_but_registration_hooks_roll_back_the_generation() {
        for event in [
            StateEvent::BeforeTick,
            StateEvent::BeforeScript,
            StateEvent::AfterScript,
        ] {
            reset();
            let logger = rsvz::replace_logger(|record: &rsvz::LogRecord<'_>| {
                REPORTS.with_borrow_mut(|reports| reports.push(record.message.to_owned()));
            });
            let installed_task = std::rc::Rc::new(Cell::new(None));
            let task_slot = std::rc::Rc::clone(&installed_task);
            with_state_hooks(|hooks| {
                hooks.register(event, 0, move || {
                    task_slot.set(Some(rsvz::tick::spawn(TickOptions::any_dispatch(), |_| {
                        Ok(TickControl::Continue)
                    })));
                    fail("hook abort");
                    Ok(())
                });
                hooks.register(event, 1, || {
                    push("later hook");
                    Ok(())
                });
            });
            if event == StateEvent::BeforeTick {
                for _ in 0..2 {
                    assert_eq!(
                        rsvz_game::state_hook::dispatch_state_event(event),
                        StateHookDispatchResult::Continue
                    );
                }
                TRACE.with_borrow(|trace| assert_eq!(&**trace, ["later hook", "later hook"]));
                REPORTS.with_borrow(|reports| assert_eq!(&**reports, ["hook abort", "hook abort"]));
            } else {
                rsvz_game::lifecycle::finish_hook_installation().unwrap();
                for _ in 0..2 {
                    let error = match rsvz_game::registration::begin_script_generation() {
                        Ok(generation) => rsvz_game::registration::finish_script_generation(generation).unwrap_err(),
                        Err(error) => error,
                    };
                    assert_eq!(error.to_string(), "hook abort");
                    TRACE.with_borrow(|trace| assert!(trace.is_empty()));
                    with_scheduler(|scheduler| {
                        assert_eq!(scheduler.state(installed_task.get().unwrap()), TickTaskState::Stopped)
                    });
                    assert_eq!(
                        rsvz_game::lifecycle::with_lifecycle(|state| state.generation()),
                        rsvz_game::lifecycle::GenerationState::None
                    );
                }
            }
            rsvz::restore_logger(logger);
        }
        reset();
    }

    #[test]
    fn aborted_tick_does_not_override_its_explicit_pause_or_stop() {
        for pause in [false, true] {
            reset();
            let slot = std::rc::Rc::new(Cell::new(None));
            let callback_slot = std::rc::Rc::clone(&slot);
            let handle = rsvz::tick::spawn(TickOptions::playing_frame(), move |_| {
                with_scheduler(|scheduler| {
                    if pause {
                        scheduler.pause(callback_slot.get().unwrap());
                    } else {
                        scheduler.stop(callback_slot.get().unwrap());
                    }
                });
                fail("stopped callback abort");
                Ok(TickControl::Continue)
            });
            slot.set(Some(handle));
            let mut reports = 0;
            for clock in [1, 2] {
                assert_eq!(
                    rsvz_game::tick::dispatch_scheduler_tick_reporting(meta(clock), &mut |_| reports += 1),
                    rsvz_schedule::TickDispatchResult::Continue
                );
            }
            assert_eq!(reports, 1);
            with_scheduler(|scheduler| {
                assert_eq!(
                    scheduler.state(handle),
                    if pause {
                        TickTaskState::Paused
                    } else {
                        TickTaskState::Stopped
                    }
                )
            });
        }
        reset();
    }

    #[test]
    fn registration_body_abort_does_not_activate_its_tasks() {
        fn body() -> RuntimeResult<()> {
            rsvz::__run_script(|| {
                at(1, -599, || push("uncommitted callback"));
                rsvz::tick::spawn(TickOptions::any_dispatch(), |_| {
                    push("uncommitted tick");
                    Ok(TickControl::Continue)
                });
                fail("registration body abort");
                Ok(())
            })
        }
        fn install() -> RuntimeResult<()> {
            Ok(())
        }
        reset();
        let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
            .unwrap()
            .install_current()
            .unwrap();
        let result = world.with_backend(|backend| {
            rsvz_game::dispatch::run(
                backend,
                DispatchInput {
                    shard: SessionShard::default(),
                    stop_requested: false,
                    completed_rounds: 0,
                },
                body,
                install,
            )
        });
        let DispatchResult::Stop {
            error: Some(error),
            artifact: None,
        } = result
        else {
            panic!("registration must stop");
        };
        assert_eq!(error.to_string(), "registration body abort");
        TRACE.with_borrow(|trace| assert!(trace.is_empty()));
        with_scheduler(|scheduler| assert!(scheduler.is_idle()));
        reset();
    }

    #[test]
    fn logger_abort_from_an_ordinary_log_ends_only_its_callback() {
        reset();
        let logger = rsvz::replace_logger(|record: &rsvz::LogRecord<'_>| {
            if record.message == "trigger" {
                fail("logger read abort");
            }
            REPORTS.with_borrow_mut(|reports| reports.push(record.message.to_owned()));
        });
        rsvz::__run_script(|| {
            at(1, 0, || {
                rsvz::log(rsvz::LogLevel::Info, "trigger");
                push("unreachable logger");
            });
            at(1, 0, || push("later"));
            Ok(())
        })
        .unwrap();
        assert_eq!(
            rsvz_game::timeline::dispatch_timeline_tick_reporting(
                WaveTimingSnapshot::minimal(100, Wave(1)),
                meta(100),
                &mut rsvz_game::diagnostics::report_runtime_error,
            ),
            TimelineDispatchResult::Continue
        );
        TRACE.with_borrow(|trace| assert_eq!(&**trace, ["later"]));
        REPORTS.with_borrow(|reports| assert_eq!(&**reports, ["logger read abort"]));
        rsvz::restore_logger(logger);
        reset();
    }

    #[test]
    fn logger_failure_during_failure_reporting_preserves_the_primary_abort() {
        reset();
        let logger = rsvz::replace_logger(|record: &rsvz::LogRecord<'_>| {
            REPORTS.with_borrow_mut(|reports| reports.push(record.message.to_owned()));
            fail("secondary logger abort");
        });
        rsvz::__run_script(|| {
            at(1, 0, || fail("primary abort"));
            at(1, 0, || push("later"));
            Ok(())
        })
        .unwrap();
        assert_eq!(
            rsvz_game::timeline::dispatch_timeline_tick_reporting(
                WaveTimingSnapshot::minimal(100, Wave(1)),
                meta(100),
                &mut rsvz_game::diagnostics::report_runtime_error,
            ),
            TimelineDispatchResult::Continue
        );
        TRACE.with_borrow(|trace| assert_eq!(&**trace, ["later"]));
        REPORTS.with_borrow(|reports| assert_eq!(&**reports, ["primary abort"]));
        assert_eq!(
            rsvz_game::session::take_dispatch_outcome(),
            Some(rsvz_game::session::DispatchOutcome::RecoverableError)
        );
        assert!(!rsvz_game::session::script_stop_requested());
        rsvz::restore_logger(logger);
        reset();
    }
}
