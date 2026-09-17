//! Continuous survival samples. Ordinary level completion never reconstructs the board.
use crate::SessionArtifact;
use crate::diagnostics::{LogContext, LogLevel, LogRecord, emit_log};
use crate::runtime::{RuntimeError, RuntimeResult};
use crate::session::{self, SessionJobKey};
use rsvz_backend_api::WorldResetBackend;
use rsvz_current::CurrentBackend;
use rsvz_model::{GameUi, PlantEffectOutcome, PlantKind, WorldResetConfig};
use rsvz_schedule::event::EventOptions;
use rsvz_schedule::state_hook::StateEvent;
use rsvz_schedule::tick::{TickControl, TickLane, TickLifetime, TickOptions};
use std::cell::RefCell;
use std::io::{self, Write};
use std::rc::Rc;
use std::time::{Duration, Instant};

static JOB: u8 = 0;

/// What a duration budget means for an already-running survival sample.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExpectedPassesEnd {
    #[default]
    AtDeadline,
    /// Stop admitting new samples at the deadline; keep the current world until loss.
    FinishActive,
}

/// Logs combat losses of the selected kinds without changing any game outcome.
/// Register once per script generation; this observer has script lifetime.
pub fn trace_plant_losses(kinds: impl IntoIterator<Item = PlantKind>) -> RuntimeResult<()> {
    let kinds: Vec<_> = kinds.into_iter().collect();
    crate::event::on_plant_effect(EventOptions::new(), move |event| {
        if kinds.contains(&event.raw_kind)
            && matches!(event.outcome, PlantEffectOutcome::Killed | PlantEffectOutcome::Squished | PlantEffectOutcome::Stolen)
        {
            let time = rsvz_schedule::timeline::with_timeline_ref(|timeline| {
                (1..=20).filter_map(|wave| {
                    let refresh = timeline.wave_clocks().refresh_clock(rsvz_model::Wave(wave))?;
                    (refresh <= event.main_counter).then_some((wave, event.main_counter - refresh))
                }).last()
            });
            trace(&format!(
                "plant_loss worker={} epoch={} rounds={} clock={} kind={:?} grid=({}, {}) id={} source={:?} outcome={:?} hp_before={} wave_time={:?}",
                session::session_shard().index, session::world_epoch(), session::completed_rounds(),
                event.main_counter, event.raw_kind, event.grid.row + 1, event.grid.col + 1,
                event.plant_id.raw(), event.source, event.outcome, event.hp_before, time,
            ));
        }
    }).map_err(|error| RuntimeError::new(error.to_string()))?;
    Ok(())
}

fn trace(message: &str) {
    emit_log(&LogRecord::new(LogLevel::Debug, message, LogContext::Unscoped));
}

#[derive(Clone, Copy, Debug, Default)]
struct Counts {
    failed_samples: u64,
    completed_rounds: u64,
    squared_rounds: f64,
    min_rounds: Option<u64>,
    max_rounds: u64,
    censored_samples: u64,
    censored_rounds: u64,
    invalid_samples: u64,
}

impl Counts {
    fn failure(&mut self, rounds: u64) {
        self.failed_samples += 1;
        self.completed_rounds += rounds;
        self.squared_rounds += (rounds as f64).powi(2);
        self.min_rounds = Some(self.min_rounds.map_or(rounds, |old| old.min(rounds)));
        self.max_rounds = self.max_rounds.max(rounds);
    }
}

fn merge(target: &mut Counts, source: Counts) -> RuntimeResult<()> {
    target.failed_samples += source.failed_samples;
    target.completed_rounds += source.completed_rounds;
    target.squared_rounds += source.squared_rounds;
    target.min_rounds = match (target.min_rounds, source.min_rounds) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    };
    target.max_rounds = target.max_rounds.max(source.max_rounds);
    target.censored_samples += source.censored_samples;
    target.censored_rounds += source.censored_rounds;
    target.invalid_samples += source.invalid_samples;
    Ok(())
}

fn encode(c: &Counts, w: &mut dyn Write) -> io::Result<()> {
    let mean = if c.failed_samples == 0 {
        "null".to_owned()
    } else {
        (c.completed_rounds as f64 / c.failed_samples as f64).to_string()
    };
    let min = c.min_rounds.map_or_else(|| "null".to_owned(), |n| n.to_string());
    write!(
        w,
        "{{\"kind\":\"expected_passes\",\"round_unit\":\"20_waves_2_flags\",\"failed_samples\":{},\"completed_rounds\":{},\"squared_rounds\":{},\"mean_completed_failure_samples\":{},\"min_rounds\":{},\"max_rounds\":{},\"censored_samples\":{},\"censored_rounds\":{},\"invalid_samples\":{}}}",
        c.failed_samples,
        c.completed_rounds,
        c.squared_rounds,
        mean,
        min,
        c.max_rounds,
        c.censored_samples,
        c.censored_rounds,
        c.invalid_samples
    )
}

struct Run {
    counts: Counts,
    started: Instant,
    epoch: u64,
    rounds_at_start: u64,
    active: bool,
    published: bool,
    invalid: bool,
    deadline_observed: bool,
}

impl Run {
    fn publish(&mut self) -> RuntimeResult<()> {
        if self.published {
            return Ok(());
        }
        if self.active {
            if self.invalid {
                self.counts.invalid_samples += 1;
            } else {
                self.counts.censored_samples += 1;
                self.counts.censored_rounds += session::completed_rounds().saturating_sub(self.rounds_at_start);
            }
        }
        session::set_session_artifact(SessionArtifact::new(self.counts, merge, encode))?;
        self.published = true;
        Ok(())
    }
}

/// Measures independent continuous lives for a wall-time budget. A surviving final
/// life is censored, so the mean of failed lives alone is not an uncensored expectation.
/// Each fresh life uses `reset` with a seed derived from the session shard.
pub fn expected_passes_for(duration: Duration, reset: WorldResetConfig) -> RuntimeResult<()>
where
    CurrentBackend: WorldResetBackend,
{
    expected_passes_for_with_end(duration, reset, ExpectedPassesEnd::AtDeadline)
}

/// Measures for a duration with an explicit policy for the active sample.
/// External host limits remain hard limits, including while draining.
pub fn expected_passes_for_with_end(
    duration: Duration, reset: WorldResetConfig, end: ExpectedPassesEnd,
) -> RuntimeResult<()>
where
    CurrentBackend: WorldResetBackend,
{
    install(Some(duration), None, reset, end)
}

/// Measures this many independent lives across all workers, resetting only after loss.
pub fn expected_passes_trials(trials: u64, reset: WorldResetConfig) -> RuntimeResult<()>
where
    CurrentBackend: WorldResetBackend,
{
    if trials == 0 {
        return Err(RuntimeError::new("expected passes requires at least one sample"));
    }
    install(None, Some(trials), reset, ExpectedPassesEnd::AtDeadline)
}

fn install(
    duration: Option<Duration>, trials: Option<u64>, reset: WorldResetConfig, end: ExpectedPassesEnd,
) -> RuntimeResult<()>
where
    CurrentBackend: WorldResetBackend,
{
    if duration == Some(Duration::ZERO) {
        return Err(RuntimeError::new("expected passes duration must be positive"));
    }
    if !session::claim_session_job(SessionJobKey::new(&JOB))? {
        return Ok(());
    }
    let shard = session::session_shard();
    let quota = trials.map(|total| super::local_trial_quota(total, shard));
    let run = Rc::new(RefCell::new(Run {
        counts: Counts::default(),
        started: Instant::now(),
        epoch: session::world_epoch(),
        rounds_at_start: session::completed_rounds(),
        active: false,
        published: false,
        invalid: false,
        deadline_observed: false,
    }));
    let final_run = Rc::clone(&run);
    crate::state_hook::register_fallible(StateEvent::BeforeExit, 0, move || final_run.borrow_mut().publish());
    if quota == Some(0) {
        run.borrow_mut().publish()?;
        session::stop_script();
        return Ok(());
    }
    session::request_world_reset(WorldResetConfig {
        seed: shard.seed_base.wrapping_add(shard.index),
        ..reset
    })?;
    crate::event::on_home_entry(
        EventOptions::new().lifetime(rsvz_schedule::event::EventLifetime::Session),
        |event| {
            trace(&format!(
                "home_entry worker={} epoch={} clock={} kind={:?} row={} id={}",
                session::session_shard().index,
                session::world_epoch(),
                event.main_counter,
                event.zombie_kind,
                event.row + 1,
                event.zombie_id.raw()
            ));
        },
    )
    .map_err(|error| RuntimeError::new(error.to_string()))?;
    crate::tick::spawn_finalizer(
        TickOptions::any_dispatch()
            .lifetime(TickLifetime::Session)
            .lane(TickLane::After)
            .idle_neutral()
            .name("expected_passes"),
        move |meta| {
            let mut run = run.borrow_mut();
            let mut expired = duration.is_some_and(|limit| run.started.elapsed() >= limit);
            if run.epoch != session::world_epoch() {
                run.epoch = session::world_epoch();
                run.rounds_at_start = session::completed_rounds();
                run.active = true;
                let sequence = super::global_trial_sequence(shard, run.counts.failed_samples);
                trace(&format!(
                    "life_begin worker={} sample={} seed={} epoch={}",
                    shard.index,
                    sequence,
                    shard.seed_base.wrapping_add(sequence as u32),
                    run.epoch
                ));
            }
            if let Some(error) = session::take_dispatch_outcome() {
                run.invalid = true;
                run.publish()?;
                session::fail_script(RuntimeError::new(format!("continuous survival interrupted: {error:?}")));
                return Ok(TickControl::Stop);
            }
            if run.active && meta.game_ui == Some(GameUi::ZombiesWon) {
                let rounds = session::completed_rounds().saturating_sub(run.rounds_at_start);
                run.counts.failure(rounds);
                run.active = false;
                trace(&format!(
                    "life_end worker={} epoch={} completed_rounds={} clock={:?}",
                    shard.index, run.epoch, rounds, meta.clock
                ));
                expired = duration.is_some_and(|limit| run.started.elapsed() >= limit);
                if quota.is_none_or(|n| run.counts.failed_samples < n) && !expired {
                    let sequence = super::global_trial_sequence(shard, run.counts.failed_samples);
                    session::request_world_reset(WorldResetConfig {
                        seed: shard.seed_base.wrapping_add(sequence as u32),
                        ..reset
                    })?;
                    return Ok(TickControl::Continue);
                }
            }
            if expired && !run.deadline_observed {
                run.deadline_observed = true;
                trace(&format!(
                    "expected_passes_deadline worker={} epoch={} end={end:?} completed_samples={} current_rounds={}",
                    shard.index,
                    run.epoch,
                    run.counts.failed_samples,
                    session::completed_rounds().saturating_sub(run.rounds_at_start)
                ));
            }
            if quota.is_some_and(|n| run.counts.failed_samples >= n)
                || (expired && (end == ExpectedPassesEnd::AtDeadline || !run.active))
            {
                run.publish()?;
                session::stop_script();
                return Ok(TickControl::Stop);
            }
            Ok(TickControl::Continue)
        },
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aggregate_keeps_censored_lives_out_of_failure_mean() {
        let mut a = Counts::default();
        a.failure(0);
        a.failure(4);
        let mut b = Counts {
            censored_samples: 1,
            censored_rounds: 100,
            ..Counts::default()
        };
        b.failure(2);
        merge(&mut a, b).unwrap();
        assert_eq!(
            (a.failed_samples, a.completed_rounds, a.min_rounds, a.max_rounds),
            (3, 6, Some(0), 4)
        );
        let mut bytes = Vec::new();
        encode(&a, &mut bytes).unwrap();
        let json = String::from_utf8(bytes).unwrap();
        assert!(json.contains("\"mean_completed_failure_samples\":2,"));
        assert!(json.contains("\"censored_rounds\":100"));
    }
}
