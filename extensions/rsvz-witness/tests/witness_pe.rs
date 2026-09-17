#![cfg(feature = "capture")]

use rsvz::core::backend::{RandomControlBackend as _, ZombieRuleEditBackend as _};
use rsvz::core::model::{CardSelection, PlantKind, RandomMode, RandomStreamKind, SessionShard, ZombieKind};
use rsvz::core::runtime::RuntimeResult;
use rsvz_pvz_emulator_backend::runner_internal::{PeWorldOwner, PeWorldRun};
use rsvz_pvz_emulator_backend::{DispatchInput, DispatchResult, PeWorldConfig};
use rsvz_witness::{WitnessCapture, WitnessLimit, WitnessOptions};

fn locked_script() -> RuntimeResult<()> {
    rsvz::setup::with_script_setup(|setup| {
        setup.spawn_list = Some(rsvz::core::logic::average_spawn_list(20, [ZombieKind::Normal]));
        setup.desired_cards = Some(
            [
                PlantKind::Peashooter,
                PlantKind::Sunflower,
                PlantKind::CherryBomb,
                PlantKind::WallNut,
                PlantKind::PotatoMine,
                PlantKind::SnowPea,
                PlantKind::Chomper,
                PlantKind::Repeater,
                PlantKind::PuffShroom,
                PlantKind::SunShroom,
            ]
            .into_iter()
            .map(CardSelection::Plant)
            .collect(),
        );
    });
    rsvz_witness::start(WitnessOptions {
        locked_random: 7,
        capture: WitnessCapture::Digest,
        limit: WitnessLimit::CompletedRounds(2),
        ..WitnessOptions::default()
    })?;
    Ok(())
}

#[allow(clippy::unnecessary_wraps, reason = "runtime_dispatch installer ABI")]
fn no_hooks() -> RuntimeResult<()> {
    Ok(())
}

fn drive_to_locked(world: &mut PeWorldRun, input: DispatchInput) -> u32 {
    for _attempt in 0..6 {
        let result =
            world.with_backend(|backend| rsvz::__private::runtime_dispatch(backend, input, locked_script, no_hooks));
        let fixed = world.with_backend(|backend| {
            rsvz_pvz_emulator_backend::scope_backend(backend, || {
                rsvz_pvz_emulator_backend::with_backend_shared(|access| {
                    let backend = access;
                    backend
                        .random_locked(RandomStreamKind::Battle)
                        .then(|| backend.random_fixed(RandomStreamKind::Battle))
                })
                .expect("Board scope")
            })
        });
        if let Some(fixed) = fixed {
            return fixed;
        }
        match result {
            DispatchResult::Continue => {
                world.update_world().expect("PE host transition update");
            }
            DispatchResult::SkipUpdate => {}
            DispatchResult::Stop { .. } => panic!("runtime stopped before locked mode was installed"),
        }
    }
    panic!("locked mode must be installed before the first gameplay update");
}

#[test]
fn locked_random_is_reapplied_at_each_fight_origin_without_stopping_spawns() {
    let owner = PeWorldOwner::new_reset_deferred_spawn(PeWorldConfig::default()).expect("PE owner");
    let mut world = owner.install_current().expect("PE world");
    let input = DispatchInput {
        shard: SessionShard {
            index: 0,
            count: 1,
            seed_base: 0,
        },
        stop_requested: false,
        completed_rounds: 0,
    };
    let fixed = drive_to_locked(&mut world, input);
    assert_eq!(fixed, 7);
    assert!(!world.with_backend(|backend| backend.zombie_spawn_stopped().expect("spawn rule")));

    world
        .with_backend(|backend| backend.set_random_mode(RandomMode::Seeded(99)))
        .expect("seed replacement world");
    rsvz::request_world_reset(WitnessOptions::default().reset).expect("request replacement PE world");
    let fixed = drive_to_locked(&mut world, input);
    assert_eq!(fixed, 7);
}

thread_local! { static CAPTURE_OBSERVED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }

fn expected_dancer_clock() -> u32 {
    rsvz::core::setup::mix_dancer_clock(WitnessOptions::default().reset.seed)
}

fn normalization_observers() -> RuntimeResult<()> {
    use rsvz::core::backend::{BoardStateBackend as _, DancerClockWriteBackend as _};
    use rsvz::state_hook::{StateEvent, register_fallible};
    register_fallible(StateEvent::BeforeTick, i32::MIN + 1, || {
        let clock = rsvz::__private::with_backend_shared(|backend| backend.dancer_clock()).unwrap()?;
        assert_eq!(
            clock,
            expected_dancer_clock(),
            "earliest normalization precedes user hooks"
        );
        Ok(())
    });
    register_fallible(StateEvent::BeforeTick, 0, || {
        rsvz::with_backend(|backend| backend.set_dancer_clock(expected_dancer_clock().wrapping_add(1)))?;
        Ok(())
    });
    register_fallible(StateEvent::AfterTick, 0, || {
        let clock = rsvz::__private::with_backend_shared(|backend| backend.dancer_clock()).unwrap()?;
        assert_eq!(
            clock,
            expected_dancer_clock(),
            "latest compensation precedes this frame's capture"
        );
        CAPTURE_OBSERVED.set(true);
        Ok(())
    });
    Ok(())
}

#[test]
fn both_normalizations_run_before_the_same_frame_is_captured() {
    CAPTURE_OBSERVED.set(false);
    let owner = PeWorldOwner::new_reset_deferred_spawn(PeWorldConfig::default()).unwrap();
    let mut world = owner.install_current().unwrap();
    let input = DispatchInput {
        shard: SessionShard::default(),
        stop_requested: false,
        completed_rounds: 0,
    };
    for _ in 0..16 {
        let result = world.with_backend(|backend| {
            rsvz::__private::runtime_dispatch(backend, input, locked_script, normalization_observers)
        });
        if CAPTURE_OBSERVED.get() {
            break;
        }
        match result {
            DispatchResult::Continue => {
                world.update_world().unwrap();
            }
            DispatchResult::SkipUpdate => {}
            DispatchResult::Stop { error, .. } => panic!("stopped before capture: {error:?}"),
        }
    }
    assert!(
        CAPTURE_OBSERVED.get(),
        "a baseline capture must occur within the opening bound"
    );
    let result = world.with_backend(|backend| {
        rsvz::__private::runtime_dispatch(
            backend,
            DispatchInput {
                stop_requested: true,
                ..input
            },
            locked_script,
            normalization_observers,
        )
    });
    assert!(matches!(result, DispatchResult::Stop { error: None, .. }));
}
