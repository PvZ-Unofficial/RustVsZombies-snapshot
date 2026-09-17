#![cfg(any(feature = "pvz-1-0-0-1051", feature = "pvz-emulator"))]

use std::cell::RefCell;
use std::io::{self, Write};

use rsvz::core::runtime::{RuntimeError, RuntimeResult};
use rsvz::prelude::RefreshDance;
use rsvz::tick::{TickControl, TickLifetime, TickOptions};
use rsvz::{SessionArtifact, SessionJobKey, SessionShard, WorldResetConfig};

static CUSTOM_MEASURE_JOB: u8 = 0;

thread_local! {
    static REPORTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn capture_report(record: &rsvz::LogRecord<'_>) {
    REPORTS.with_borrow_mut(|reports| reports.push(record.message.to_owned()));
}

#[derive(Clone, Copy)]
struct CustomCounts {
    samples: u64,
}

fn merge_counts(target: &mut CustomCounts, source: CustomCounts) -> RuntimeResult<()> {
    target.samples = target.samples.saturating_add(source.samples);
    Ok(())
}

fn write_counts(counts: &CustomCounts, writer: &mut dyn Write) -> io::Result<()> {
    write!(writer, "{{\"kind\":\"custom\",\"samples\":{}}}", counts.samples)
}

fn artifact(samples: u64) -> SessionArtifact {
    SessionArtifact::new(CustomCounts { samples }, merge_counts, write_counts)
}

#[rsvz::script]
fn script() {
    if rsvz::claim_session_job(SessionJobKey::new(&CUSTOM_MEASURE_JOB))? {
        rsvz::request_world_reset(WorldResetConfig::default())?;
        let _task = rsvz::tick::spawn(
            TickOptions::any_dispatch().lifetime(TickLifetime::Session),
            move |_meta| {
                rsvz::publish_artifact(artifact(1))?;
                rsvz::stop_script();
                Ok(TickControl::Stop)
            },
        );
    }
}

#[test]
fn custom_measure_artifact_uses_only_the_public_surface() {
    let mut output = Vec::new();
    artifact(1).write_json(&mut output).expect("artifact JSON");
    assert_eq!(output, br#"{"kind":"custom","samples":1}"#);
}

#[cfg(feature = "pvz-emulator")]
#[test]
fn refresh_freezes_setup_after_the_whole_script() {
    REPORTS.with_borrow_mut(Vec::clear);
    rsvz_game::frame::reset_runtime_state_preserving_backend();
    rsvz_game::state_hook::clear_state_hooks();
    rsvz_game::lifecycle::reset_session_resources();
    rsvz_game::session::reset_session_control();
    let original_reporter = rsvz::replace_logger(capture_report);
    rsvz_game::session::install_session_shard(SessionShard {
        index: 1,
        count: 2,
        seed_base: 100,
    })
    .expect("valid empty shard");
    rsvz_game::lifecycle::finish_hook_installation().expect("finish hook installation");

    let generation = rsvz_game::registration::begin_script_generation().expect("begin script");
    rsvz::__run_script(|| {
        rsvz::measure::refresh_trials(1);
        rsvz::measure::refresh_activate(true);
        rsvz::measure::refresh_dance(RefreshDance::Fast);
        rsvz::measure::end_at((4, 0));
        Ok(())
    })
    .expect("register Refresh");
    rsvz_game::registration::finish_script_generation(generation).expect("finalize setup");
    rsvz_game::diagnostics::report_message("hidden during measurement");
    REPORTS.with_borrow(|reports| assert!(reports.is_empty()));

    let artifact = rsvz_game::session::take_session_artifact().expect("empty shard artifact");
    let mut output = Vec::new();
    artifact.write_json(&mut output).expect("artifact JSON");
    let output = String::from_utf8(output).expect("UTF-8 JSON");
    assert!(output.contains(r#""activate":true"#));
    assert!(output.contains(r#""dance":"fast""#));
    assert!(output.contains(r#""window_end":{"wave":4,"time":0}"#), "{output}");
    rsvz_game::lifecycle::finalize_session().expect("finalize session");
    rsvz_game::diagnostics::report_message("restored after measurement");
    REPORTS.with_borrow(|reports| assert_eq!(reports.as_slice(), ["restored after measurement"]));
    rsvz::restore_logger(original_reporter);
}

#[cfg(feature = "pvz-emulator")]
#[test]
fn measurement_reporter_is_restored_when_after_script_rolls_back() {
    REPORTS.with_borrow_mut(Vec::clear);
    rsvz_game::frame::reset_runtime_state_preserving_backend();
    rsvz_game::state_hook::clear_state_hooks();
    rsvz_game::lifecycle::reset_session_resources();
    rsvz_game::session::reset_session_control();
    let original_reporter = rsvz::replace_logger(capture_report);
    rsvz_game::lifecycle::finish_hook_installation().expect("finish hook installation");

    let generation = rsvz_game::registration::begin_script_generation().expect("begin script");
    rsvz::__run_script(|| {
        rsvz::measure::refresh_trials(1);
        rsvz::state_hook::register_fallible(rsvz::state_hook::StateEvent::AfterScript, i32::MAX, || {
            Err(RuntimeError::new("rollback probe"))
        });
        Ok(())
    })
    .expect("register Refresh and rollback probe");
    assert!(rsvz_game::registration::finish_script_generation(generation).is_err());

    rsvz_game::diagnostics::report_message("restored after rollback");
    REPORTS.with_borrow(|reports| assert_eq!(reports.as_slice(), ["restored after rollback"]));
    rsvz_game::lifecycle::finalize_session().expect("finalize session");
    rsvz::restore_logger(original_reporter);
}

#[cfg(feature = "pvz-emulator")]
#[test]
fn refresh_and_event_measure_jobs_conflict_during_registration() {
    rsvz_game::frame::reset_runtime_state_preserving_backend();
    rsvz_game::state_hook::clear_state_hooks();
    rsvz_game::lifecycle::reset_session_resources();
    rsvz_game::session::reset_session_control();
    rsvz_game::lifecycle::finish_hook_installation().expect("finish hook installation");

    let generation = rsvz_game::registration::begin_script_generation().expect("begin script");
    let error = rsvz::__run_script(|| {
        rsvz::measure::refresh_trials(1);
        rsvz::measure::damage_narrow_trials(1);
        Ok(())
    })
    .expect_err("one session cannot run two measurement jobs");
    assert_eq!(error.to_string(), "a different session job is already installed");
    rsvz_game::registration::abort_script_generation(generation).expect("rollback registration");
    rsvz_game::lifecycle::finalize_session().expect("finalize session");
}

#[test]
fn different_event_measure_jobs_conflict_during_registration() {
    rsvz_game::frame::reset_runtime_state_preserving_backend();
    rsvz_game::state_hook::clear_state_hooks();
    rsvz_game::lifecycle::reset_session_resources();
    rsvz_game::session::reset_session_control();
    rsvz_game::lifecycle::finish_hook_installation().expect("finish hook installation");

    let generation = rsvz_game::registration::begin_script_generation().expect("begin script");
    let error = rsvz::__run_script(|| {
        rsvz::measure::damage_narrow_trials(1);
        rsvz::measure::smash_trials(1);
        Ok(())
    })
    .expect_err("one session cannot run two event measurement modes");
    assert_eq!(error.to_string(), "a different session job is already installed");
    rsvz_game::registration::abort_script_generation(generation).expect("rollback registration");
    rsvz_game::lifecycle::finalize_session().expect("finalize session");
}

#[cfg(feature = "pvz-emulator")]
thread_local! {
    static SHORT_MEASURE_REGISTRATIONS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static SHORT_MEASURE_ENDPOINT_TICKS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

#[cfg(feature = "pvz-emulator")]
fn short_measure_script() -> rsvz::core::runtime::RuntimeResult<()> {
    SHORT_MEASURE_REGISTRATIONS.set(SHORT_MEASURE_REGISTRATIONS.get() + 1);
    rsvz::setup::set_zombies([rsvz::prelude::ZombieKind::Gargantuar]);
    rsvz::measure::pogo_trials(3);
    rsvz::measure::end_at((2, 0));
    let _endpoint_task = rsvz::tick::spawn(TickOptions::playing_frame().lane(rsvz::tick::TickLane::After), |meta| {
        let reached = meta.clock.is_some_and(|clock| {
            rsvz::runtime_wave_clocks()
                .refresh_clock(rsvz::prelude::Wave(2))
                .is_some_and(|refresh| clock >= refresh)
        });
        if reached {
            SHORT_MEASURE_ENDPOINT_TICKS.set(SHORT_MEASURE_ENDPOINT_TICKS.get() + 1);
            rsvz_game::session::record_dispatch_outcome(rsvz_game::session::DispatchOutcome::RecoverableError);
        }
        Ok(TickControl::Continue)
    });
    Ok(())
}

#[cfg(feature = "pvz-emulator")]
fn no_hooks() -> rsvz::core::runtime::RuntimeResult<()> {
    Ok(())
}

#[cfg(feature = "pvz-emulator")]
fn continuous_script() -> RuntimeResult<()> {
    rsvz::setup::reload(rsvz::prelude::ReloadMode::MainUiOrFightUi);
    rsvz::measure::expected_passes_trials(
        2,
        WorldResetConfig {
            completed_rounds: 63,
            ..WorldResetConfig::default()
        },
    )
}

#[cfg(feature = "pvz-emulator")]
fn drain_script() -> RuntimeResult<()> {
    rsvz::setup::reload(rsvz::prelude::ReloadMode::MainUiOrFightUi);
    rsvz::measure::expected_passes_for_with_end(
        std::time::Duration::from_millis(100),
        WorldResetConfig {
            completed_rounds: 63,
            ..Default::default()
        },
        rsvz::measure::ExpectedPassesEnd::FinishActive,
    )
}

#[cfg(feature = "pvz-emulator")]
#[test]
fn time_budget_drains_the_live_sample_and_never_resets_after_deadline() {
    use rsvz::__private::{DispatchInput, DispatchResult, runtime_dispatch};
    use rsvz::core::backend::{ZombieCreateBackend, ZombieXWriteBackend};
    use rsvz::core::model::{I32RepresentableF32, ZombieKind};
    use rsvz_pvz_emulator_backend::{PeUpdateOutcome, PeWorldConfig, runner_internal::PeWorldOwner};
    let mut world = PeWorldOwner::new_reset_deferred_spawn(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    let input = DispatchInput {
        shard: SessionShard::default(),
        stop_requested: false,
        completed_rounds: 0,
    };
    let mut started = false;
    for _ in 0..20 {
        match world.with_backend(|b| runtime_dispatch(b, input, drain_script, no_hooks)) {
            DispatchResult::Continue => {
                world.update_world().unwrap();
                started = true;
                break;
            }
            DispatchResult::SkipUpdate => {}
            DispatchResult::Stop { error, .. } => panic!("early stop: {error:?}"),
        }
    }
    assert!(started);
    let epoch = rsvz_game::session::world_epoch();
    std::thread::sleep(std::time::Duration::from_millis(120));
    assert!(matches!(
        world.with_backend(|b| runtime_dispatch(b, input, drain_script, no_hooks)),
        DispatchResult::Continue
    ));
    assert_eq!(rsvz_game::session::world_epoch(), epoch);
    world.with_backend(|b| {
        let zombie = b.add_zombie_in_row(ZombieKind::Normal, 0, 0).unwrap().unwrap();
        b.set_zombie_x(zombie, I32RepresentableF32::new(-100.0).unwrap())
            .unwrap();
    });
    assert_eq!(world.update_world().unwrap(), PeUpdateOutcome::GameOver);
    let artifact = match world.with_backend(|b| runtime_dispatch(b, input, drain_script, no_hooks)) {
        DispatchResult::Stop {
            artifact: Some(artifact),
            error: None,
        } => artifact,
        _ => panic!("drained sample must end without admitting another"),
    };
    assert_eq!(rsvz_game::session::world_epoch(), epoch);
    let mut bytes = Vec::new();
    artifact.write_json(&mut bytes).unwrap();
    let report = String::from_utf8(bytes).unwrap();
    assert!(report.contains("\"failed_samples\":1,"), "{report}");
    assert!(report.contains("\"censored_samples\":0,"), "{report}");
}

#[cfg(feature = "pvz-emulator")]
#[test]
fn continuous_measure_preserves_passed_world_and_restarts_only_after_game_over() {
    use rsvz::__private::{DispatchInput, DispatchResult, runtime_dispatch};
    use rsvz::core::backend::{PlantCreateBackend, PlantReadBackend, ZombieCreateBackend, ZombieXWriteBackend};
    use rsvz::core::model::{CardSelection, Grid, I32RepresentableF32, PlantKind, ZombieKind};
    use rsvz_pvz_emulator_backend::{PeUpdateOutcome, PeWorldConfig, runner_internal::PeWorldOwner};
    let mut world = PeWorldOwner::new_reset_deferred_spawn(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    let initial_epoch = rsvz_game::session::world_epoch();
    let mut observed_epoch = initial_epoch;
    let mut stage = 0;
    let mut completed = 0;
    let mut retained = None;
    let mut passes = 0;
    let artifact = (0..100)
        .find_map(|_| {
            let input = DispatchInput {
                shard: SessionShard {
                    index: 0,
                    count: 1,
                    seed_base: 31,
                },
                stop_requested: false,
                completed_rounds: std::mem::take(&mut completed),
            };
            let result = world.with_backend(|backend| runtime_dispatch(backend, input, continuous_script, no_hooks));
            let epoch = rsvz_game::session::world_epoch();
            if epoch != observed_epoch {
                observed_epoch = epoch;
                stage = 0;
            }
            match result {
                DispatchResult::Stop { artifact, error: None } => return artifact,
                DispatchResult::Stop { error, .. } => panic!("unexpected stop: {error:?}"),
                DispatchResult::SkipUpdate => return None,
                DispatchResult::Continue => {}
            }
            if stage == 1 {
                let id = world.with_backend(|backend| {
                    let plant = backend
                        .add_plant(
                            CardSelection::Plant(PlantKind::Sunflower).checked().unwrap(),
                            Grid { row: 0, col: 0 },
                        )
                        .unwrap();
                    backend.plant_id(plant)
                });
                retained = Some(id);
                completed = 1; // Exercise the actual host completion input and deferred reload.
                passes += 1;
            } else if stage == 2 {
                world.with_backend(|backend| {
                    assert!(
                        backend.plant(retained.unwrap()).unwrap().is_some(),
                        "ordinary completion retained the native plant"
                    );
                    let z = backend.add_zombie_in_row(ZombieKind::Normal, 0, 0).unwrap().unwrap();
                    backend
                        .set_zombie_x(z, I32RepresentableF32::new(-100.0).unwrap())
                        .unwrap();
                });
            }
            let outcome = world.update_world().unwrap();
            if stage == 2 {
                assert_eq!(outcome, PeUpdateOutcome::GameOver);
            }
            stage += 1;
            None
        })
        .expect("two completed lives");
    assert_eq!(passes, 2);
    assert_eq!(
        observed_epoch - initial_epoch,
        2,
        "only initial reset and one post-loss reset"
    );
    let mut bytes = Vec::new();
    artifact.write_json(&mut bytes).unwrap();
    let report = String::from_utf8(bytes).unwrap();
    assert!(report.contains("\"failed_samples\":2,"), "{report}");
    assert!(report.contains("\"completed_rounds\":2,"), "{report}");
    assert!(report.contains("\"censored_samples\":0,"), "{report}");
}

#[cfg(feature = "pvz-emulator")]
#[test]
fn pe_short_measure_resets_each_trial_at_the_second_wave() {
    use rsvz::__private::{DispatchInput, DispatchResult, runtime_dispatch};
    use rsvz_pvz_emulator_backend::PeWorldConfig;
    use rsvz_pvz_emulator_backend::runner_internal::PeWorldOwner;
    use rsvz_schedule::timeline::current_wave_refresh_clock;

    SHORT_MEASURE_REGISTRATIONS.set(0);
    SHORT_MEASURE_ENDPOINT_TICKS.set(0);
    let owner = PeWorldOwner::new_reset_deferred_spawn(PeWorldConfig::default()).expect("PE owner");
    let mut world = owner.install_current().expect("PE world");
    let input = DispatchInput {
        shard: SessionShard {
            index: 0,
            count: 1,
            seed_base: 17,
        },
        stop_requested: false,
        completed_rounds: 0,
    };
    let initial_epoch = rsvz_game::session::world_epoch();
    let mut updates = 0;
    let artifact = loop {
        let result = world.with_backend(|backend| runtime_dispatch(backend, input, short_measure_script, no_hooks));
        match result {
            DispatchResult::Continue => {
                world.update_world().expect("PE update");
                updates += 1;
            }
            DispatchResult::SkipUpdate => {}
            DispatchResult::Stop {
                artifact: Some(artifact),
                error: None,
            } => break artifact,
            DispatchResult::Stop { error, .. } => panic!("PE measurement stopped without an artifact: {error:?}"),
        }
        if updates >= 30_000 {
            let snapshot = world
                .with_backend(|backend| {
                    rsvz_pvz_emulator_backend::scope_backend(backend, rsvz_game::timing::wave_timing)
                })
                .expect("timing snapshot");
            panic!(
                "short measurement did not stop: updates={updates}, epoch={}, registrations={}, snapshot={snapshot:?}, clocks={:?}",
                rsvz_game::session::world_epoch(),
                SHORT_MEASURE_REGISTRATIONS.get(),
                rsvz::runtime_wave_clocks()
            );
        }
    };

    assert_eq!(rsvz_game::session::world_epoch().saturating_sub(initial_epoch), 3);
    assert_eq!(SHORT_MEASURE_REGISTRATIONS.get(), 4);
    assert_eq!(SHORT_MEASURE_ENDPOINT_TICKS.get(), 3);
    let snapshot = world
        .with_backend(|backend| rsvz_pvz_emulator_backend::scope_backend(backend, rsvz_game::timing::wave_timing))
        .expect("final PE timing snapshot");
    assert_eq!(snapshot.current_wave, rsvz::prelude::Wave(2));
    assert_eq!(Some(snapshot.clock), current_wave_refresh_clock(snapshot));

    let mut json = Vec::new();
    artifact.write_json(&mut json).expect("measurement report");
    let report = String::from_utf8(json).expect("UTF-8 measurement JSON");
    assert!(report.contains(r#""attempted_trials":3"#), "{report}");
    assert!(report.contains(r#""valid_trials":0"#), "{report}");
    assert!(report.contains(r#""invalid_trials":3"#), "{report}");
    assert!(report.contains(r#""window_end":{"wave":2,"time":0}"#), "{report}");
}
