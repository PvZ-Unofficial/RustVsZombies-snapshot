//! 智能无铲 C9 垫材 API。
//!
//! 本模块只从当前运行时取得波内时刻和 backend，将预测委托给
//! [`crate::logic::smart_fodder`]，再登记选中的种植与可选删除操作。

use std::cell::Cell;
use std::rc::Rc;

use crate::logic::ContactGeometryBackend;
use crate::logic::card_timing::IMITATOR_MORPH_DELAY;
use crate::logic::cards::{CardContext, PlantingComponent};
use crate::logic::smart_fodder::{SmartFodderReplanContext, predict_smart_fodder_with_context_at};
use crate::logic::smart_fodder::{SmartFodderSpec, predict_c9_remove_by_at};
use crate::runtime::{RuntimeError, RuntimeResult};
use rsvz_backend_api::backend::{
    ImitatorMorphBackend, PlantReadBackend, SceneBackend, ZombieRawFactsBackend, ZombieReadBackend,
};
use rsvz_current::CurrentBackend;
use rsvz_model::{CardSelection, PlantId, Wave, ZombieId, ZombieKind, ZombiePhase};
use rsvz_schedule::timeline::TimeHandle;
use rsvz_schedule::{TickControl, TickLane, TickOptions, TickPriority};

const REPLAN_POLL_PERIOD: i32 = 4;

/// 初次求解结果以及已经登记的时间轴操作。
///
/// 垫材生效前若观察到小丑进入开盒阶段，内部观察器可能用新的可见信息替换这份
/// 初始计划；此时旧 handle 对应的操作会安全失效。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScheduledFodder {
    pub plant_at: i32,
    pub remove_at: Option<i32>,
    pub expected_damage: f64,
    pub plant_op: TimeHandle,
    pub remove_op: Option<TimeHandle>,
}

pub fn predict_c9_remove_by(row: i32) -> RuntimeResult<Option<i32>>
where
    CurrentBackend: ContactGeometryBackend + SceneBackend,
{
    let (wave, now) = current_relative_time()?;
    let from_wave = wave
        .0
        .checked_sub(1)
        .ok_or_else(|| RuntimeError::new("current wave cannot be converted to a native spawn-wave index"))?;
    predict_c9_remove_by_at(row, from_wave, now)
}

pub fn try_smart_fodder(spec: SmartFodderSpec) -> RuntimeResult<ScheduledFodder>
where
    CurrentBackend: CardContext
        + ContactGeometryBackend
        + ImitatorMorphBackend
        + SceneBackend
        + 'static
        + rsvz_backend_api::BoardReadinessBackend,
{
    solve_current(spec)
}

pub fn smart_fodder(spec: SmartFodderSpec) -> Option<ScheduledFodder>
where
    CurrentBackend: CardContext
        + ContactGeometryBackend
        + ImitatorMorphBackend
        + SceneBackend
        + 'static
        + rsvz_backend_api::BoardReadinessBackend,
{
    match try_smart_fodder(spec) {
        Ok(scheduled) => Some(scheduled),
        Err(error) => {
            crate::diagnostics::report_operation_error(error);
            None
        }
    }
}

fn solve_current(spec: SmartFodderSpec) -> RuntimeResult<ScheduledFodder>
where
    CurrentBackend: CardContext
        + ContactGeometryBackend
        + ImitatorMorphBackend
        + SceneBackend
        + 'static
        + rsvz_backend_api::BoardReadinessBackend,
{
    let (wave, now) = current_relative_time()?;
    let (choice, replan_context) = predict_smart_fodder_with_context_at(&spec, now, None)?;
    let watched_jacks = running_jacks()?;
    // Keep partially registered work inert until the complete initial plan is
    // ready, matching the atomic generation switch used by later replans.
    let current_generation = Rc::new(Cell::new(u64::MAX));
    let scheduled = schedule(
        spec.card,
        spec.row,
        wave,
        choice.plant_at,
        choice.remove_at,
        choice.expected_damage,
        ScheduleGeneration::new(Rc::clone(&current_generation), 0),
    )?;
    current_generation.set(0);
    if !watched_jacks.is_empty() {
        spawn_replan_observer(AdaptiveFodderState {
            spec,
            wave,
            scheduled,
            current_generation,
            watched_jacks,
            replan_pending: false,
            replan_context,
        });
    }
    Ok(scheduled)
}

struct AdaptiveFodderState {
    spec: SmartFodderSpec,
    wave: Wave,
    scheduled: ScheduledFodder,
    current_generation: Rc<Cell<u64>>,
    watched_jacks: Vec<ZombieId>,
    replan_pending: bool,
    replan_context: SmartFodderReplanContext,
}

#[derive(Clone)]
struct ScheduleGeneration {
    current: Rc<Cell<u64>>,
    expected: u64,
}

impl ScheduleGeneration {
    fn new(current: Rc<Cell<u64>>, expected: u64) -> Self {
        Self { current, expected }
    }

    fn is_current(&self) -> bool {
        self.current.get() == self.expected
    }
}

fn running_jacks() -> RuntimeResult<Vec<ZombieId>>
where
    CurrentBackend: ContactGeometryBackend,
{
    crate::access::with_backend(|backend| {
        let mut jacks = Vec::new();
        for zombie in crate::live_value::read_or_abort(backend.zombies(), "zombies") {
            if backend.zombie_is_alive(zombie)
                && !backend.zombie_is_disappeared(zombie)
                && !backend.zombie_is_mind_controlled(zombie)
                && crate::live_value::read_or_abort(backend.zombie_kind(zombie), "zombie_kind")
                    == ZombieKind::JackInTheBox
                && crate::live_value::read_or_abort(backend.zombie_phase(zombie), "zombie_phase")
                    == ZombiePhase::JackInTheBoxRunning
            {
                jacks.push(backend.zombie_id(zombie));
            }
        }
        Ok(jacks)
    })
}

fn spawn_replan_observer(mut state: AdaptiveFodderState)
where
    CurrentBackend: CardContext
        + ContactGeometryBackend
        + ImitatorMorphBackend
        + SceneBackend
        + 'static
        + rsvz_backend_api::BoardReadinessBackend,
{
    rsvz_schedule::tick::with_scheduler(|scheduler| {
        scheduler.spawn(
            TickOptions::playing_frame()
                .lane(TickLane::Observe)
                .priority(TickPriority::HIGH)
                .idle_neutral()
                .name("smart_fodder_replan"),
            move |_meta| tick_replan(&mut state),
        )
    });
}

fn tick_replan(state: &mut AdaptiveFodderState) -> RuntimeResult<TickControl>
where
    CurrentBackend: CardContext
        + ContactGeometryBackend
        + ImitatorMorphBackend
        + SceneBackend
        + 'static
        + rsvz_backend_api::BoardReadinessBackend,
{
    let (wave, now) = current_relative_time()?;
    if wave != state.wave || now >= state.scheduled.plant_at {
        return Ok(TickControl::Stop);
    }
    if !should_poll_replan(now) {
        return Ok(TickControl::Continue);
    }

    update_watched_jacks(&mut state.watched_jacks, &mut state.replan_pending)?;
    if !state.replan_pending {
        return Ok(if state.watched_jacks.is_empty() {
            TickControl::Stop
        } else {
            TickControl::Continue
        });
    }

    let start = (*state.spec.plant_window.start()).max(now.saturating_add(1));
    let end = *state.spec.plant_window.end();
    if start > end {
        return Ok(TickControl::Stop);
    }
    let mut next_spec = state.spec.clone();
    next_spec.plant_window = start..=end;
    let prediction = predict_smart_fodder_with_context_at(&next_spec, now, Some(state.replan_context));
    state.replan_pending = false;
    let Ok((choice, replan_context)) = prediction else {
        return Ok(if state.watched_jacks.is_empty() {
            TickControl::Stop
        } else {
            TickControl::Continue
        });
    };

    state.spec = next_spec;
    state.replan_context = replan_context;
    if choice.plant_at != state.scheduled.plant_at || choice.remove_at != state.scheduled.remove_at {
        let next_generation = state.current_generation.get().saturating_add(1);
        let Ok(scheduled) = schedule(
            state.spec.card,
            state.spec.row,
            state.wave,
            choice.plant_at,
            choice.remove_at,
            choice.expected_damage,
            ScheduleGeneration::new(Rc::clone(&state.current_generation), next_generation),
        ) else {
            return Ok(if state.watched_jacks.is_empty() {
                TickControl::Stop
            } else {
                TickControl::Continue
            });
        };
        state.current_generation.set(next_generation);
        state.scheduled = scheduled;
    } else {
        state.scheduled.expected_damage = choice.expected_damage;
    }

    Ok(if state.watched_jacks.is_empty() {
        TickControl::Stop
    } else {
        TickControl::Continue
    })
}

#[derive(Clone, Copy)]
enum WatchedJack {
    Running,
    Popping,
    Gone,
}

fn update_watched_jacks(watched: &mut Vec<ZombieId>, pending: &mut bool) -> RuntimeResult<()>
where
    CurrentBackend: ContactGeometryBackend,
{
    crate::access::with_backend(|backend| {
        update_watched_jacks_with(watched, pending, |id| {
            let Some(zombie) = crate::live_value::read_or_abort(backend.zombie(id), "zombie") else {
                return Ok(WatchedJack::Gone);
            };
            let phase = crate::live_value::read_or_abort(backend.zombie_phase(zombie), "zombie_phase");
            let running = backend.zombie_is_alive(zombie)
                && !backend.zombie_is_disappeared(zombie)
                && !backend.zombie_is_mind_controlled(zombie)
                && crate::live_value::read_or_abort(backend.zombie_kind(zombie), "zombie_kind")
                    == ZombieKind::JackInTheBox
                && phase == ZombiePhase::JackInTheBoxRunning;
            Ok(if running {
                WatchedJack::Running
            } else if phase == ZombiePhase::JackInTheBoxPopping {
                WatchedJack::Popping
            } else {
                WatchedJack::Gone
            })
        })
    })
}

fn update_watched_jacks_with(
    watched: &mut Vec<ZombieId>, pending: &mut bool, mut read: impl FnMut(ZombieId) -> RuntimeResult<WatchedJack>,
) -> RuntimeResult<()> {
    let mut index = 0;
    while index < watched.len() {
        match read(watched[index])? {
            WatchedJack::Running => index += 1,
            state => {
                // Keep already observed events if a later read aborts this callback.
                *pending |= matches!(state, WatchedJack::Popping);
                watched.swap_remove(index);
            }
        }
    }
    Ok(())
}

fn should_poll_replan(now: i32) -> bool {
    now.rem_euclid(REPLAN_POLL_PERIOD) == 0
}

fn current_relative_time() -> RuntimeResult<(Wave, i32)> {
    rsvz_schedule::timeline::with_timeline(|timeline| {
        let diagnostics = timeline.diagnostics();
        let wave = diagnostics
            .current_wave
            .ok_or_else(|| RuntimeError::new("smart fodder must be called from a running wave callback"))?;
        let clock = diagnostics
            .current_clock
            .ok_or_else(|| RuntimeError::new("current game clock is unavailable"))?;
        let refresh = timeline
            .wave_clocks()
            .refresh_clock(wave)
            .ok_or_else(|| RuntimeError::new("current wave refresh clock is unavailable"))?;
        Ok((wave, clock.saturating_sub(refresh)))
    })
}

fn schedule(
    selection: CardSelection, row: i32, wave: Wave, plant_at: i32, remove_at: Option<i32>, expected_damage: f64,
    generation: ScheduleGeneration,
) -> RuntimeResult<ScheduledFodder>
where
    CurrentBackend: CardContext + ImitatorMorphBackend + 'static + rsvz_backend_api::BoardReadinessBackend,
{
    let retained = remove_at.map(|_time| Rc::new(Cell::new(None::<PlantId>)));
    let plant_state = retained.clone();
    let plant_generation = generation.clone();
    let plant_op = crate::timeline::try_at(wave.0, plant_at, move || {
        if !plant_generation.is_current() {
            return Ok(());
        }
        {
            crate::logic::cards::card_recording(selection, row, 9, |component, id| {
                if component == PlantingComponent::Main
                    && let Some(state) = &plant_state
                {
                    state.set(Some(id));
                }
            })
        }
        .map(|_receipt| ())
        .map_err(runtime_error)
    })?;

    let remove_op = if let (Some(remove_at), Some(state)) = (remove_at, retained) {
        if matches!(selection, CardSelection::Imitator(_)) && remove_at >= plant_at.saturating_add(IMITATOR_MORPH_DELAY)
        {
            let morph_state = Rc::clone(&state);
            let morph_generation = generation.clone();
            let morph_at = plant_at.saturating_add(IMITATOR_MORPH_DELAY);
            let _morph_op = crate::timeline::try_at(wave.0, morph_at, move || {
                if !morph_generation.is_current() {
                    return Ok(());
                }
                resolve_imitator(&morph_state)
            })?;
        }
        let remove_generation = generation;
        Some(crate::timeline::try_at(wave.0, remove_at, move || {
            if !remove_generation.is_current() {
                return Ok(());
            }
            if let Some(id) = state.take() {
                crate::modifier::remove_plant_by_id(id).map_err(runtime_error)?;
            }
            Ok(())
        })?)
    } else {
        None
    };

    Ok(ScheduledFodder {
        plant_at,
        remove_at,
        expected_damage,
        plant_op,
        remove_op,
    })
}

fn resolve_imitator(retained: &Cell<Option<PlantId>>) -> RuntimeResult<()>
where
    CurrentBackend: ImitatorMorphBackend,
{
    crate::access::with_backend(|backend| {
        let Some(placeholder) = retained.get() else {
            return Ok(());
        };
        if let Some(successor) = crate::live_value::read_or_abort(
            backend.imitator_morph_successor(placeholder),
            "imitator_morph_successor",
        ) {
            retained.set(Some(successor));
            return Ok(());
        }
        if crate::live_value::read_or_abort(backend.plant(placeholder), "plant").is_some() {
            Err(RuntimeError::new(
                "imitator placeholder did not morph at the native +320 boundary",
            ))
        } else {
            retained.set(None);
            Ok(())
        }
    })
}

fn runtime_error(error: impl std::fmt::Display) -> RuntimeError {
    RuntimeError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_generation_switch_is_atomic() {
        let current = Rc::new(Cell::new(u64::MAX));
        let initial = ScheduleGeneration::new(Rc::clone(&current), 0);
        assert!(!initial.is_current());

        current.set(0);
        assert!(initial.is_current());

        let replacement = ScheduleGeneration::new(Rc::clone(&current), 1);
        assert!(!replacement.is_current());
        current.set(1);
        assert!(!initial.is_current());
        assert!(replacement.is_current());
    }

    #[test]
    fn observed_pop_survives_a_later_read_failure() {
        let first = ZombieId::from_raw(1);
        let second = ZombieId::from_raw(2);
        let mut watched = vec![first, second];
        let mut pending = false;
        assert!(
            update_watched_jacks_with(&mut watched, &mut pending, |id| {
                if id == first {
                    Ok(WatchedJack::Popping)
                } else {
                    Err(RuntimeError::new("read"))
                }
            })
            .is_err()
        );
        assert_eq!(watched, [second]);
        assert!(pending);
        update_watched_jacks_with(&mut watched, &mut pending, |_| Ok(WatchedJack::Running)).unwrap();
        assert!(pending);
        assert_eq!(watched, [second]);
    }

    #[test]
    fn replan_polling_uses_absolute_four_centisecond_boundaries() {
        assert!(!should_poll_replan(691));
        assert!(should_poll_replan(692));
        assert!(!should_poll_replan(695));
        assert!(should_poll_replan(696));
    }
}
