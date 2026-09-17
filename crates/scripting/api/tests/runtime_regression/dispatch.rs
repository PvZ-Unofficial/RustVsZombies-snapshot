//! Transitional dispatch export.

pub use rsvz_game::dispatch::run;
#[cfg(all(test, feature = "pvz-emulator"))]
mod tests {
    use crate::RuntimeResult;
    use std::cell::Cell;

    use rsvz_backend_api::backend::{BoardStateBackend, WaveTimingBackend, ZombieCreateBackend, ZombieXWriteBackend};
    use rsvz_model::{
        GameUi, I32RepresentableF32, ReloadMode, SessionShard, Wave, WaveTimingSnapshot, WorldResetConfig, ZombieKind,
    };
    use rsvz_pvz_emulator_backend::runner_internal::{PeWorldOwner, PeWorldRun};
    use rsvz_pvz_emulator_backend::{DispatchInput, DispatchResult, PeUpdateOutcome, PeWorldConfig};
    use rsvz_schedule::state_hook::StateEvent;
    use rsvz_schedule::tick::{TickControl, TickMeta, TickOptions, TickPhase};
    use rsvz_schedule::timeline::{TimelineDispatchResult, TimingViolationPolicy};

    use super::*;

    thread_local! {
        static INSTALLS: Cell<u32> = const { Cell::new(0) };
        static ATTACHES: Cell<u32> = const { Cell::new(0) };
        static REGISTRATIONS: Cell<u32> = const { Cell::new(0) };
        static TICKS: Cell<u32> = const { Cell::new(0) };
        static INSTALLED_SHARD: Cell<u32> = const { Cell::new(u32::MAX) };
        static MINUS_600_HITS: Cell<u32> = const { Cell::new(0) };
        static MINUS_599_HITS: Cell<u32> = const { Cell::new(0) };
        static RELOAD_MODE: Cell<ReloadMode> = const { Cell::new(ReloadMode::None) };
    }

    fn reset_counts() {
        INSTALLS.set(0);
        ATTACHES.set(0);
        REGISTRATIONS.set(0);
        TICKS.set(0);
        INSTALLED_SHARD.set(u32::MAX);
    }

    fn install_hooks() -> RuntimeResult<()> {
        assert!(crate::with_backend(|backend| backend.is_initialized()));
        INSTALLS.set(INSTALLS.get() + 1);
        INSTALLED_SHARD.set(crate::session_shard().index);
        crate::with_state_hooks(|hooks| {
            hooks.register(StateEvent::AfterAttach, 0, || {
                ATTACHES.set(ATTACHES.get() + 1);
                Ok(())
            });
        });
        Ok(())
    }

    fn script() -> RuntimeResult<()> {
        REGISTRATIONS.set(REGISTRATIONS.get() + 1);
        crate::with_script_setup(|setup| setup.reload_mode = rsvz_model::ReloadMode::MainUiOrFightUi);
        crate::with_scheduler(|scheduler| {
            scheduler.spawn(TickOptions::any_dispatch(), |_| {
                TICKS.set(TICKS.get() + 1);
                Ok(TickControl::Continue)
            });
        });
        Ok(())
    }

    fn opening_timing_script() -> RuntimeResult<()> {
        crate::with_timeline(|timeline| {
            let _minus_600 = timeline
                .at(Wave(1), -600, || {
                    MINUS_600_HITS.set(MINUS_600_HITS.get() + 1);
                    Ok(())
                })
                .expect("register wave 1 -600 probe");
            let _minus_599 = timeline
                .at(Wave(1), -599, || {
                    MINUS_599_HITS.set(MINUS_599_HITS.get() + 1);
                    Ok(())
                })
                .expect("register wave 1 -599 probe");
        });
        Ok(())
    }

    fn reload_script() -> RuntimeResult<()> {
        crate::with_script_setup(|setup| setup.reload_mode = RELOAD_MODE.get());
        Ok(())
    }

    fn input(shard: SessionShard, completed_rounds: u64, stop_requested: bool) -> DispatchInput {
        DispatchInput {
            shard,
            stop_requested,
            completed_rounds,
        }
    }

    fn dispatch(world: &mut PeWorldRun, input: DispatchInput) -> DispatchResult {
        world.with_backend(|backend| run(backend, input, script, install_hooks))
    }

    fn dispatch_opening_timing(world: &mut PeWorldRun, input: DispatchInput) -> DispatchResult {
        world.with_backend(|backend| run(backend, input, opening_timing_script, install_hooks))
    }

    fn dispatch_reload(world: &mut PeWorldRun, input: DispatchInput) -> DispatchResult {
        world.with_backend(|backend| run(backend, input, reload_script, install_hooks))
    }

    fn new_world() -> PeWorldRun {
        PeWorldOwner::new_reset_deferred_spawn(PeWorldConfig::default())
            .expect("PE world")
            .install_current()
            .expect("PE current world")
    }

    thread_local! {
        static BOUNDARY_PLAYING: Cell<u32> = const { Cell::new(0) };
        static BOUNDARY_TIMELINE: Cell<u32> = const { Cell::new(0) };
        static BOUNDARY_NOTICES: Cell<u32> = const { Cell::new(0) };
        static BOUNDARY_HOOKS: Cell<u32> = const { Cell::new(0) };
        static BOUNDARY_ACTION: Cell<u8> = const { Cell::new(0) };
    }

    fn boundary_script() -> RuntimeResult<()> {
        use rsvz_schedule::tick::{TickAvailability, TickLifetime};
        crate::with_script_setup(|setup| setup.reload_mode = ReloadMode::MainUiOrFightUi);
        crate::with_scheduler(|scheduler| {
            scheduler.spawn(
                TickOptions::any_dispatch()
                    .availability(TickAvailability::Playing)
                    .lifetime(TickLifetime::Session),
                |_| {
                    BOUNDARY_PLAYING.set(BOUNDARY_PLAYING.get() + 1);
                    Ok(TickControl::Continue)
                },
            );
            scheduler.spawn(TickOptions::any_dispatch().lifetime(TickLifetime::Fight), |meta| {
                if meta.phase == TickPhase::RoundComplete {
                    assert!(!meta.is_new_frame);
                    assert_eq!(meta.clock, None);
                    assert_eq!(rsvz_game::session::completed_rounds(), 1);
                    BOUNDARY_NOTICES.set(BOUNDARY_NOTICES.get() + 1);
                    match BOUNDARY_ACTION.get() {
                        1 => rsvz_game::session::stop_script(),
                        2 => rsvz_game::session::request_world_reset(WorldResetConfig {
                            completed_rounds: 63,
                            ..WorldResetConfig::default()
                        })?,
                        _ => {}
                    }
                }
                Ok(TickControl::Continue)
            });
        });
        crate::with_timeline(|timeline| {
            timeline
                .at(Wave(1), -598, || {
                    BOUNDARY_TIMELINE.set(BOUNDARY_TIMELINE.get() + 1);
                    Ok(())
                })
                .unwrap();
        });
        for event in [StateEvent::BeforeTick, StateEvent::AfterTick] {
            crate::with_state_hooks(|hooks| {
                hooks.register(event, 0, || {
                    if rsvz_game::frame::runtime_frame_meta().phase == TickPhase::RoundComplete {
                        BOUNDARY_HOOKS.set(BOUNDARY_HOOKS.get() + 1);
                    }
                    Ok(())
                });
            });
        }
        Ok(())
    }

    fn dispatch_boundary(world: &mut PeWorldRun, rounds: u64, stop: bool) -> DispatchResult {
        world.with_backend(|backend| {
            run(
                backend,
                input(SessionShard::default(), rounds, stop),
                boundary_script,
                install_hooks,
            )
        })
    }

    #[test]
    fn round_completion_settles_without_old_playing_or_timeline_callbacks() {
        BOUNDARY_ACTION.set(0);
        for counter in [
            &BOUNDARY_PLAYING,
            &BOUNDARY_TIMELINE,
            &BOUNDARY_NOTICES,
            &BOUNDARY_HOOKS,
        ] {
            counter.set(0);
        }
        let mut world = new_world();
        assert!(matches!(
            dispatch_boundary(&mut world, 0, false),
            DispatchResult::Continue
        ));
        world.update_world().unwrap();
        assert!(matches!(
            dispatch_boundary(&mut world, 0, false),
            DispatchResult::Continue
        ));
        assert_eq!(BOUNDARY_PLAYING.get(), 1);
        world.update_world().unwrap();
        assert!(matches!(
            dispatch_boundary(&mut world, 1, false),
            DispatchResult::SkipUpdate
        ));
        assert_eq!(
            BOUNDARY_PLAYING.get(),
            1,
            "Session lifetime must not bypass Playing availability"
        );
        assert_eq!(BOUNDARY_TIMELINE.get(), 0);
        assert_eq!(
            BOUNDARY_NOTICES.get(),
            1,
            "Fight lifetime must not suppress explicit AnyDispatch settlement"
        );
        assert_eq!(BOUNDARY_HOOKS.get(), 2);
        assert!(matches!(
            dispatch_boundary(&mut world, 0, true),
            DispatchResult::Stop { error: None, .. }
        ));
    }

    #[test]
    fn round_completion_stop_and_reset_precede_reload() {
        for action in [1, 2] {
            rsvz_game::frame::set_runtime_dispatch_state(rsvz_game::session::RuntimeDispatchState::new());
            BOUNDARY_ACTION.set(action);
            BOUNDARY_PLAYING.set(0);
            BOUNDARY_NOTICES.set(0);
            BOUNDARY_HOOKS.set(0);
            let mut world = new_world();
            assert!(matches!(
                dispatch_boundary(&mut world, 0, false),
                DispatchResult::Continue
            ));
            world.update_world().unwrap();
            assert!(matches!(
                dispatch_boundary(&mut world, 0, false),
                DispatchResult::Continue
            ));
            world.update_world().unwrap();
            let epoch = crate::world_epoch();
            let output = dispatch_boundary(&mut world, 1, false);
            assert_eq!(BOUNDARY_PLAYING.get(), 1);
            assert_eq!(BOUNDARY_NOTICES.get(), 1);
            assert_eq!(BOUNDARY_HOOKS.get(), 2);
            if action == 1 {
                assert!(matches!(output, DispatchResult::Stop { error: None, .. }));
                assert_eq!(crate::world_epoch(), epoch);
            } else {
                assert!(matches!(output, DispatchResult::SkipUpdate));
                assert_eq!(crate::world_epoch(), epoch + 1);
                assert!(!rsvz_game::frame::runtime_dispatch_state().registered);
                assert!(matches!(
                    dispatch_boundary(&mut world, 0, true),
                    DispatchResult::Stop { error: None, .. }
                ));
            }
        }
        BOUNDARY_ACTION.set(0);
    }

    #[test]
    fn host_stop_settles_the_last_round_without_reloading_or_updating() {
        rsvz_game::frame::set_runtime_dispatch_state(rsvz_game::session::RuntimeDispatchState::new());
        reset_counts();
        BOUNDARY_ACTION.set(0);
        BOUNDARY_PLAYING.set(0);
        BOUNDARY_NOTICES.set(0);
        BOUNDARY_HOOKS.set(0);
        let mut world = new_world();
        assert!(matches!(
            dispatch_boundary(&mut world, 0, false),
            DispatchResult::Continue
        ));
        world.update_world().unwrap();
        assert!(matches!(
            dispatch_boundary(&mut world, 0, false),
            DispatchResult::Continue
        ));
        assert_eq!(BOUNDARY_PLAYING.get(), 1);
        let clock = world.with_backend(|b| rsvz_backend_api::ClockBackend::clock(b).unwrap());
        assert!(matches!(
            dispatch_boundary(&mut world, 1, true),
            DispatchResult::Stop { error: None, .. }
        ));
        assert_eq!(BOUNDARY_NOTICES.get(), 1);
        assert_eq!(BOUNDARY_HOOKS.get(), 2);
        assert_eq!(BOUNDARY_PLAYING.get(), 1);
        assert_eq!(
            world.with_backend(|b| rsvz_backend_api::ClockBackend::clock(b).unwrap()),
            clock
        );
    }

    #[test]
    fn one_bound_dispatch_installs_once_and_never_ticks_on_registration_frames() {
        reset_counts();
        let shard = SessionShard {
            index: 0,
            count: 1,
            seed_base: 17,
        };
        let mut world = new_world();

        assert!(matches!(
            dispatch(&mut world, input(shard, 0, false)),
            DispatchResult::Continue
        ));
        assert_eq!(
            (INSTALLS.get(), ATTACHES.get(), REGISTRATIONS.get(), TICKS.get()),
            (1, 1, 1, 0)
        );
        assert_eq!(
            world.update_world().expect("opening transition update"),
            PeUpdateOutcome::Normal
        );

        assert!(matches!(
            dispatch(&mut world, input(shard, 0, false)),
            DispatchResult::Continue
        ));
        assert_eq!(TICKS.get(), 1);
        assert_eq!(
            world.update_world().expect("first gameplay update"),
            PeUpdateOutcome::Normal
        );

        let epoch = crate::world_epoch();
        assert!(matches!(
            dispatch(&mut world, input(shard, 1, false)),
            DispatchResult::SkipUpdate
        ));
        assert_eq!(crate::world_epoch(), epoch);
        assert_eq!(
            (INSTALLS.get(), ATTACHES.get(), REGISTRATIONS.get(), TICKS.get()),
            (1, 1, 1, 2),
            "the completed update must dispatch before its reload"
        );

        assert!(matches!(
            dispatch(&mut world, input(shard, 0, false)),
            DispatchResult::SkipUpdate
        ));
        assert_eq!(
            (INSTALLS.get(), ATTACHES.get(), REGISTRATIONS.get(), TICKS.get()),
            (1, 1, 2, 2),
            "the next dispatch performs the deferred reload without updating"
        );

        assert!(matches!(
            dispatch(&mut world, input(shard, 0, false)),
            DispatchResult::Continue
        ));
        assert_eq!(
            (INSTALLS.get(), ATTACHES.get(), REGISTRATIONS.get(), TICKS.get()),
            (1, 1, 2, 2),
            "the already-sampled post-update frame must not be dispatched twice"
        );

        assert!(matches!(
            dispatch(&mut world, input(shard, 0, true)),
            DispatchResult::Stop {
                artifact: None,
                error: None
            }
        ));
    }

    #[test]
    fn pe_first_user_timeline_tick_is_minus_599() {
        MINUS_600_HITS.set(0);
        MINUS_599_HITS.set(0);
        let shard = SessionShard::default();
        let mut world = new_world();

        assert!(matches!(
            dispatch_opening_timing(&mut world, input(shard, 0, false)),
            DispatchResult::Continue
        ));
        assert_eq!((MINUS_600_HITS.get(), MINUS_599_HITS.get()), (0, 0));

        assert_eq!(
            world.update_world().expect("opening transition update"),
            PeUpdateOutcome::Normal
        );
        world.with_backend(|backend| {
            assert_eq!(backend.clock_value().expect("PE clock after opening transition"), 0);
            assert_eq!(
                backend
                    .dancer_clock()
                    .expect("PE dancer clock after opening transition"),
                994
            );
            assert_eq!(
                backend.refresh_countdown().expect("PE timing after opening transition"),
                599
            );
        });

        assert!(matches!(
            dispatch_opening_timing(&mut world, input(shard, 0, false)),
            DispatchResult::Continue
        ));
        assert_eq!((MINUS_600_HITS.get(), MINUS_599_HITS.get()), (0, 1));
    }

    #[test]
    fn duplicate_suppression_ends_when_the_backend_ui_changes() {
        let mut state = crate::RuntimeDispatchState::new();
        state.post_update_dispatched = true;
        state.last_ui = Some(GameUi::Playing);
        state.retain_current_ui_dispatch(Some(GameUi::Playing));
        assert!(state.post_update_dispatched);
        state.retain_current_ui_dispatch(Some(GameUi::LevelIntro));
        assert!(!state.post_update_dispatched);
    }

    #[test]
    fn finished_state_waits_only_for_main_ui_reload_modes() {
        let waits = rsvz_game::session::waits_for_main_ui_reload;
        assert!(!waits(rsvz_model::ReloadMode::None, true));
        assert!(waits(rsvz_model::ReloadMode::MainUi, true));
        assert!(waits(rsvz_model::ReloadMode::MainUiOrFightUi, true));
        assert!(!waits(rsvz_model::ReloadMode::MainUi, false));
        assert!(!waits(rsvz_model::ReloadMode::MainUiOrFightUi, false));
    }

    #[test]
    fn pe_game_over_stops_for_every_reload_mode() {
        std::thread::scope(|scope| {
            let handles = [ReloadMode::None, ReloadMode::MainUi, ReloadMode::MainUiOrFightUi].map(|mode| {
                scope.spawn(move || {
                    RELOAD_MODE.set(mode);
                    let input = input(SessionShard::default(), 0, false);
                    let mut world = new_world();
                    assert!(matches!(dispatch_reload(&mut world, input), DispatchResult::Continue));
                    assert_eq!(world.update_world().unwrap(), PeUpdateOutcome::Normal);
                    assert!(matches!(dispatch_reload(&mut world, input), DispatchResult::Continue));
                    world.with_backend(|backend| {
                        let zombie = backend.add_zombie_in_row(ZombieKind::Normal, 0, 0).unwrap().unwrap();
                        backend
                            .set_zombie_x(zombie, I32RepresentableF32::new(-100.0).unwrap())
                            .unwrap();
                    });
                    assert_eq!(world.update_world().unwrap(), PeUpdateOutcome::GameOver);
                    assert!(matches!(
                        dispatch_reload(&mut world, input),
                        DispatchResult::Stop {
                            artifact: None,
                            error: None
                        }
                    ));
                })
            });
            for handle in handles {
                handle.join().unwrap();
            }
        });
    }

    #[test]
    fn unhandled_timing_violation_stops_instead_of_repeating_forever() {
        reset_counts();
        let shard = SessionShard::default();
        let mut world = new_world();
        assert!(matches!(
            dispatch(&mut world, input(shard, 0, false)),
            DispatchResult::Continue
        ));
        assert_eq!(
            world.update_world().expect("opening transition update"),
            PeUpdateOutcome::Normal
        );
        crate::with_timeline(|timeline| {
            timeline.set_timing_violation_policy(TimingViolationPolicy::ReportFailure);
            timeline
                .assume_wavelengths([(1, 601)])
                .expect("wavelength assumption should register");
        });
        let meta = TickMeta {
            phase: TickPhase::Playing,
            game_ui: Some(GameUi::Playing),
            clock: Some(100),
            is_new_frame: true,
        };
        assert_eq!(
            crate::dispatch_timeline_tick(WaveTimingSnapshot::minimal(100, Wave(1)), meta),
            TimelineDispatchResult::Continue
        );
        assert!(matches!(
            crate::dispatch_timeline_tick(
                WaveTimingSnapshot::minimal(800, Wave(2)),
                TickMeta {
                    clock: Some(800),
                    ..meta
                }
            ),
            TimelineDispatchResult::TimingViolation(_)
        ));

        match dispatch(&mut world, input(shard, 0, false)) {
            DispatchResult::Stop {
                artifact: None,
                error: Some(error),
            } => assert!(error.to_string().contains("TimingViolation")),
            _ => panic!("unhandled timing violation must stop"),
        }
        assert_eq!(TICKS.get(), 0);
    }

    #[test]
    fn timing_violation_finalizer_may_request_the_next_trial_reset() {
        fn reset_probe(_backend: &mut crate::CurrentBackend, _config: WorldResetConfig) -> RuntimeResult<()> {
            Ok(())
        }

        reset_counts();
        let shard = SessionShard::default();
        let mut world = new_world();
        assert!(matches!(
            dispatch(&mut world, input(shard, 0, false)),
            DispatchResult::Continue
        ));
        assert_eq!(
            world.update_world().expect("opening transition update"),
            PeUpdateOutcome::Normal
        );
        crate::with_timeline(|timeline| {
            timeline.set_timing_violation_policy(TimingViolationPolicy::ReportFailure);
            timeline
                .assume_wavelengths([(1, 601)])
                .expect("wavelength assumption should register");
        });
        let meta = TickMeta {
            phase: TickPhase::Playing,
            game_ui: Some(GameUi::Playing),
            clock: Some(100),
            is_new_frame: true,
        };
        assert_eq!(
            crate::dispatch_timeline_tick(WaveTimingSnapshot::minimal(100, Wave(1)), meta),
            TimelineDispatchResult::Continue
        );
        assert!(matches!(
            crate::dispatch_timeline_tick(
                WaveTimingSnapshot::minimal(800, Wave(2)),
                TickMeta {
                    clock: Some(800),
                    ..meta
                }
            ),
            TimelineDispatchResult::TimingViolation(_)
        ));
        crate::with_scheduler(|scheduler| {
            scheduler.spawn_runtime_finalizer(TickOptions::any_dispatch(), |_meta| {
                assert_eq!(
                    crate::take_dispatch_outcome(),
                    Some(crate::DispatchOutcome::TimingViolation)
                );
                crate::request_world_reset(WorldResetConfig::default(), reset_probe)?;
                Ok(TickControl::Continue)
            });
        });

        assert!(matches!(
            dispatch(&mut world, input(shard, 0, false)),
            DispatchResult::SkipUpdate
        ));
        assert!(!crate::world_reset_pending());
    }

    #[test]
    fn reset_requested_before_an_unhandled_timing_violation_does_not_hide_it() {
        fn reset_probe(_backend: &mut crate::CurrentBackend, _config: WorldResetConfig) -> RuntimeResult<()> {
            Ok(())
        }

        reset_counts();
        let shard = SessionShard::default();
        let mut world = new_world();
        assert!(matches!(
            dispatch(&mut world, input(shard, 0, false)),
            DispatchResult::Continue
        ));
        assert_eq!(
            world.update_world().expect("opening transition update"),
            PeUpdateOutcome::Normal
        );
        crate::with_state_hooks(|hooks| {
            hooks.register(StateEvent::BeforeTick, 0, || {
                crate::request_world_reset(WorldResetConfig::default(), reset_probe)
            });
        });
        crate::with_timeline(|timeline| {
            timeline.set_timing_violation_policy(TimingViolationPolicy::ReportFailure);
            timeline
                .assume_wavelengths([(1, 601)])
                .expect("wavelength assumption should register");
        });
        let meta = TickMeta {
            phase: TickPhase::Playing,
            game_ui: Some(GameUi::Playing),
            clock: Some(100),
            is_new_frame: true,
        };
        assert_eq!(
            crate::dispatch_timeline_tick(WaveTimingSnapshot::minimal(100, Wave(1)), meta),
            TimelineDispatchResult::Continue
        );
        assert!(matches!(
            crate::dispatch_timeline_tick(
                WaveTimingSnapshot::minimal(800, Wave(2)),
                TickMeta {
                    clock: Some(800),
                    ..meta
                }
            ),
            TimelineDispatchResult::TimingViolation(_)
        ));

        match dispatch(&mut world, input(shard, 0, false)) {
            DispatchResult::Stop {
                artifact: None,
                error: Some(error),
            } => assert!(error.to_string().contains("TimingViolation")),
            _ => panic!("a reset predating the timing failure must not handle it"),
        }
    }

    #[test]
    fn successful_reset_reseeds_the_advancing_opening_stream() {
        fn reset_probe(_backend: &mut crate::CurrentBackend, _config: WorldResetConfig) -> RuntimeResult<()> {
            Ok(())
        }

        crate::reset_session_control();
        let mut world = new_world();
        crate::request_world_reset(
            WorldResetConfig {
                completed_rounds: 500,
                seed: 123,
                ..WorldResetConfig::default()
            },
            reset_probe,
        )
        .expect("request reset");
        world.with_backend(crate::execute_pending_reset).expect("execute reset");

        assert_eq!(rsvz_game::session::take_next_opening_seed(999), 123);
        assert_eq!(
            rsvz_game::session::take_next_opening_seed(999),
            123u64.wrapping_add(0x9e37_79b9_7f4a_7c15)
        );
        crate::request_world_reset(
            WorldResetConfig {
                completed_rounds: 500,
                seed: 123,
                ..WorldResetConfig::default()
            },
            reset_probe,
        )
        .unwrap();
        world.with_backend(crate::execute_pending_reset).unwrap();
        assert_eq!(
            rsvz_game::session::take_next_opening_seed(888),
            123,
            "a fresh life is independent of earlier life length"
        );
    }

    #[test]
    fn pe_workers_receive_independent_runtime_tls_and_installers() {
        let workers: [std::thread::JoinHandle<_>; 2] = std::array::from_fn(|index| {
            std::thread::spawn(move || {
                let index = u32::try_from(index).expect("two test workers fit u32");
                reset_counts();
                let shard = SessionShard {
                    index,
                    count: 2,
                    seed_base: 100,
                };
                let mut world = new_world();
                assert!(matches!(
                    dispatch(&mut world, input(shard, 0, false)),
                    DispatchResult::Continue
                ));
                assert_eq!(
                    world.update_world().expect("opening transition update"),
                    PeUpdateOutcome::Normal
                );
                assert!(matches!(
                    dispatch(&mut world, input(shard, 0, false)),
                    DispatchResult::Continue
                ));
                (
                    INSTALLS.get(),
                    ATTACHES.get(),
                    REGISTRATIONS.get(),
                    TICKS.get(),
                    INSTALLED_SHARD.get(),
                )
            })
        });

        let mut results = workers
            .into_iter()
            .map(|worker| worker.join().expect("worker should not panic"))
            .collect::<Vec<_>>();
        results.sort_unstable_by_key(|result| result.4);
        assert_eq!(results, [(1, 1, 1, 1, 0), (1, 1, 1, 1, 1)]);
    }
}
