use std::cell::RefCell;
use std::fmt::Display;
use std::rc::Rc;

use rsvz::core::runtime::{RuntimeError, RuntimeResult};
use rsvz::core::timeline::current_wave_refresh_clock;
use rsvz::prelude::{WaveTimingBackend, ZombieRuleEditBackend};

const ENABLE_TIME: i32 = -599;
const FIRST_SAMPLE_TIME: i32 = -590;
const SECOND_SAMPLE_TIME: i32 = -580;
const REFRESH_BLOCK_SAMPLE_TIME: i32 = 100;

#[derive(Clone, Copy, Debug)]
struct TimingSample {
    clock: i32,
    current_wave: i32,
    refresh_countdown: i32,
    current_wave_refresh_clock: Option<i32>,
}

#[rsvz::script]
fn script() {
    reload(MainUiOrFightUi);
    rsvz::setup::set_game_speed(1.0);
    set_zombies("普");
    select_cards("IIKAWPCCCC");

    let initial = Rc::new(RefCell::new(std::option::Option::None::<TimingSample>));

    let initial_for_enable = Rc::clone(&initial);
    rsvz::try_at(1, ENABLE_TIME, move || {
        rsvz::__private::with_board_access(|access| {
            let backend = access.backend();
            enable_and_capture_initial(backend, &initial_for_enable)
                .map_err(|error| RuntimeError::new(format!("enable failed: {error}")))
        }).and_then(|result| result)
    })?;

    let initial_for_first = Rc::clone(&initial);
    rsvz::try_at(1, FIRST_SAMPLE_TIME, move || {
        rsvz::__private::with_board_access(|access| {
            let backend = access.backend();
            verify_stopped_sample(backend, &initial_for_first, "first")
                .map_err(|error| RuntimeError::new(format!("first stopped sample failed: {error}")))
        }).and_then(|result| result)
    })?;

    let initial_for_second = Rc::clone(&initial);
    rsvz::try_at(1, SECOND_SAMPLE_TIME, move || {
        rsvz::__private::with_board_access(|access| {
            let backend = access.backend();
            verify_stopped_sample(backend, &initial_for_second, "second")
                .map_err(|error| RuntimeError::new(format!("second stopped sample failed: {error}")))
        }).and_then(|result| result)
    })?;

    let initial_for_refresh_block = Rc::clone(&initial);
    rsvz::try_at(1, REFRESH_BLOCK_SAMPLE_TIME, move || {
        rsvz::__private::with_board_access(|access| {
            let backend = access.backend();
            verify_stopped_sample(backend, &initial_for_refresh_block, "refresh-block")
                .map_err(|error| RuntimeError::new(format!("refresh-block sample failed: {error}")))
        }).and_then(|result| result)
    })?;

    let _keepalive = rsvz::tick::spawn(rsvz::tick::TickOptions::playing_frame(), |_meta| {
        Ok(rsvz::tick::TickControl::Continue)
    });
}

fn enable_and_capture_initial<B>(backend: &B, initial: &Rc<RefCell<Option<TimingSample>>>) -> RuntimeResult<()>
where
    B: WaveTimingBackend + ZombieRuleEditBackend,
    B::Error: Display,
{
    backend.set_zombie_spawn_stopped(true).map_err(runtime_error)?;
    ensure(
        backend.zombie_spawn_stopped().map_err(runtime_error)?,
        "zombie_spawn_stopped should read back as enabled",
    )?;
    let sample = timing_sample(backend, "initial")?;
    ensure(
        sample.refresh_countdown >= 590,
        format!("initial refresh_countdown should still be near the opening countdown, got {sample:?}"),
    )?;
    *initial.borrow_mut() = Some(sample);
    Ok(())
}

fn verify_stopped_sample<B>(
    backend: &B, initial: &Rc<RefCell<Option<TimingSample>>>, label: &'static str,
) -> RuntimeResult<()>
where
    B: WaveTimingBackend + ZombieRuleEditBackend,
    B::Error: Display,
{
    ensure(
        backend.zombie_spawn_stopped().map_err(runtime_error)?,
        "zombie_spawn_stopped should remain enabled during hold window",
    )?;
    let initial = require_initial(initial)?;
    let sample = timing_sample(backend, label)?;
    ensure(
        sample.clock > initial.clock,
        format!("board clock did not advance: initial={initial:?} sample={sample:?}"),
    )?;
    ensure(
        sample.current_wave == initial.current_wave,
        format!("current_wave advanced while stop-spawning was enabled: initial={initial:?} sample={sample:?}"),
    )?;
    ensure(
        sample.refresh_countdown == initial.refresh_countdown,
        format!("refresh_countdown changed while stop-spawning was enabled: initial={initial:?} sample={sample:?}"),
    )?;
    if let (Some(initial_refresh_clock), Some(sample_refresh_clock)) =
        (initial.current_wave_refresh_clock, sample.current_wave_refresh_clock)
    {
        ensure(
            sample_refresh_clock > initial_refresh_clock,
            format!(
                "inferred refresh clock did not move forward while countdown was held: initial={initial:?} sample={sample:?}"
            ),
        )?;
    }
    Ok(())
}

fn timing_sample<B>(backend: &B, label: &'static str) -> RuntimeResult<TimingSample>
where
    B: WaveTimingBackend,
    B::Error: Display,
{
    let snapshot = rsvz::core::model::WaveTimingSnapshot {
        clock: backend.clock().map_err(runtime_error)?,
        current_wave: backend.current_wave().map_err(runtime_error)?,
        total_waves: Some(backend.total_waves()),
        refresh_countdown: Some(backend.refresh_countdown().map_err(runtime_error)?),
        initial_countdown: Some(backend.initial_countdown().map_err(runtime_error)?),
        huge_wave_countdown: Some(backend.huge_wave_countdown().map_err(runtime_error)?),
        level_end_countdown: Some(backend.level_end_countdown().map_err(runtime_error)?),
    };
    let refresh_countdown = snapshot
        .refresh_countdown
        .ok_or_else(|| RuntimeError::new(format!("{label} sample missing refresh_countdown: {snapshot:?}")))?;
    Ok(TimingSample {
        clock: snapshot.clock,
        current_wave: snapshot.current_wave.0,
        refresh_countdown,
        current_wave_refresh_clock: current_wave_refresh_clock(snapshot),
    })
}

fn require_initial(initial: &Rc<RefCell<Option<TimingSample>>>) -> RuntimeResult<TimingSample> {
    initial
        .borrow()
        .as_ref()
        .copied()
        .ok_or_else(|| RuntimeError::new("initial stop-spawning sample was not recorded"))
}

fn ensure(condition: bool, message: impl Into<RuntimeError>) -> RuntimeResult<()> {
    if condition { Ok(()) } else { Err(message.into()) }
}

fn runtime_error(error: impl Display) -> RuntimeError {
    RuntimeError::new(error.to_string())
}
