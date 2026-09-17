//! Event-backed measurement policy, aggregation, and reports.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::rc::Rc;
#[cfg(test)]
use std::time::Instant;

use crate::SessionArtifact;
use crate::measure::{
    MeasurementRunConfig, MeasurementTrialCountError, MeasurementWindowEndReport, TrialRun, measurement_window_reached,
};
use crate::runtime::{RuntimeError, RuntimeResult};
use rsvz_backend_api::backend::{PlantReadBackend, ZombieRawFactsBackend, ZombieStateBackend};
use rsvz_model::model::{
    BattleStatus, CardSelection, EffectOutcomeFact, EventDecision, EventFrameStatus, EventInterest,
    GargantuarAshHitFact, GargantuarSpawnedFact, Grid, HomeEntryFact, ImpLeakDiagnosticConfig, ImpThrownFact,
    MeasureLimit, MeasureMode, MeasurementEnd, MeasurementTrialCounts, PlantEffect, PlantEffectAttemptFact,
    PlantEffectOutcome, PlantEffectSource, PlantKind, ProtectTarget, ProtectionEdit, ProtectionPolicy, RelativeTime,
    SessionShard, WaveClockState, WorldResetConfig, ZombieKind,
};
use rsvz_model::plant_occupies_grid;
use rsvz_schedule::event::InternalEventInterceptor;

mod imp_leak;
mod plant_loss;
use plant_loss::PlantLossSample;
mod report;
use imp_leak::SampleOutcome;
use imp_leak::{ImpLeakAggregate, ImpLeakTracker, TrialEnd, bite_target};
pub use report::*;

const DAMAGE_SOURCE_COUNT: usize = 3;
const PLANT_KIND_COUNT: usize = PlantKind::COUNT;
const BOARD_ROWS: usize = 6;
const BOARD_COLS: usize = 9;
const BOARD_GRIDS: usize = BOARD_ROWS * BOARD_COLS;
const OUTCOME_COUNT: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum EventTrialOutcome {
    ObjectiveReached,
    Home,
    PogoHome,
    GargSmash,
    RefreshFailure,
    GameOver,
    Invalid,
    ImpLeak,
    VehicleCrush,
    ImitatorIceLoss,
}

impl EventTrialOutcome {
    const fn index(self) -> usize {
        match self {
            Self::ObjectiveReached => 0,
            Self::Home => 1,
            Self::PogoHome => 2,
            Self::GargSmash => 3,
            Self::RefreshFailure => 4,
            Self::GameOver => 5,
            Self::Invalid => 6,
            Self::ImpLeak => 7,
            Self::VehicleCrush => 8,
            Self::ImitatorIceLoss => 9,
        }
    }

    const fn is_valid(self) -> bool {
        !matches!(self, Self::Invalid)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EventMode {
    DamageNarrow,
    BroadPass,
    Smash,
    Pogo,
}

impl EventMode {
    fn from_mode(mode: MeasureMode) -> Result<Self, EventMeasureConfigError> {
        Ok(match mode {
            MeasureMode::DamageNarrow => Self::DamageNarrow,
            MeasureMode::BroadPass => Self::BroadPass,
            MeasureMode::Smash => Self::Smash,
            MeasureMode::Pogo => Self::Pogo,
            MeasureMode::Refresh => return Err(EventMeasureConfigError::RefreshMode),
        })
    }

    const fn mode(self) -> MeasureMode {
        match self {
            Self::DamageNarrow => MeasureMode::DamageNarrow,
            Self::BroadPass => MeasureMode::BroadPass,
            Self::Smash => MeasureMode::Smash,
            Self::Pogo => MeasureMode::Pogo,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventMeasureConfig {
    mode: EventMode,
    protection: ProtectionPolicy,
    declared_cards: Option<Vec<CardSelection>>,
    window_end: Option<RelativeTime>,
    imp_leak: ImpLeakDiagnosticConfig,
}

impl EventMeasureConfig {
    pub fn new(
        mode: MeasureMode, protection: ProtectionPolicy, declared_cards: Option<Vec<CardSelection>>,
    ) -> Result<Self, EventMeasureConfigError> {
        let mode = EventMode::from_mode(mode)?;
        validate_protection(&protection, declared_cards.as_deref(), false)?;
        Ok(Self {
            mode,
            protection,
            declared_cards,
            window_end: None,
            imp_leak: ImpLeakDiagnosticConfig::default(),
        })
    }

    #[must_use]
    pub const fn mode(&self) -> MeasureMode {
        self.mode.mode()
    }

    #[must_use]
    pub const fn protection(&self) -> &ProtectionPolicy {
        &self.protection
    }

    #[must_use]
    pub fn declared_cards(&self) -> Option<&[CardSelection]> {
        self.declared_cards.as_deref()
    }

    pub fn set_window_end(&mut self, window_end: Option<RelativeTime>) {
        self.window_end = window_end;
    }

    #[must_use]
    pub const fn window_end(&self) -> Option<RelativeTime> {
        self.window_end
    }

    pub fn set_imp_leak(&mut self, config: ImpLeakDiagnosticConfig) {
        self.imp_leak = config;
    }

    #[must_use]
    pub const fn imp_leak(&self) -> ImpLeakDiagnosticConfig {
        self.imp_leak
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EventMeasureConfigError {
    #[error("Refresh does not use the native event measurement task")]
    RefreshMode,
    #[error("protect_unrepairable_from_cards() requires selected cards")]
    SelectedCardsRequired,
    #[error("protect_unrepairable_from_cards() requires at least one selected card")]
    EmptySelectedCards,
    #[error("measurement Add/Only grid selector must be resolved after lineup setup")]
    UnresolvedGridTarget,
}

#[derive(Debug, thiserror::Error)]
pub enum ProtectionGridResolveError {
    #[error("measurement protection backend operation failed: {0}")]
    Backend(RuntimeError),
    #[error("measurement protect::grid({row}, {col}) did not match a live plant")]
    EmptyGrid { row: i32, col: i32 },
    #[error(transparent)]
    Policy(#[from] EventMeasureConfigError),
}

pub fn resolve_event_measure_config(config: &mut EventMeasureConfig) -> Result<(), ProtectionGridResolveError>
where
    rsvz_current::CurrentBackend: PlantReadBackend,
{
    for edit in config.protection.edits_mut() {
        let targets = match edit {
            ProtectionEdit::Add(targets) | ProtectionEdit::Only(targets) => targets,
            ProtectionEdit::Remove(_) => continue,
        };
        for target in targets {
            if let ProtectTarget::Grid(grid) = *target {
                *target = key_plant_at(grid)?;
            }
        }
    }
    validate_protection(&config.protection, config.declared_cards.as_deref(), true)?;
    Ok(())
}

fn validate_protection(
    policy: &ProtectionPolicy, declared_cards: Option<&[CardSelection]>, resolved: bool,
) -> Result<(), EventMeasureConfigError> {
    if policy.protect_unrepairable_from_cards_enabled() {
        match declared_cards {
            None => return Err(EventMeasureConfigError::SelectedCardsRequired),
            Some([]) => return Err(EventMeasureConfigError::EmptySelectedCards),
            Some(_) => {}
        }
    }
    if resolved
        && policy.edits().iter().any(|edit| {
            matches!(edit, ProtectionEdit::Add(targets) | ProtectionEdit::Only(targets)
                if targets.iter().any(|target| matches!(target, ProtectTarget::Grid(_))))
        })
    {
        return Err(EventMeasureConfigError::UnresolvedGridTarget);
    }
    Ok(())
}

fn protects(config: &EventMeasureConfig, grid: Grid, raw_kind: PlantKind, effective_kind: PlantKind) -> bool {
    let mut protected = config.protection.protect_unrepairable_from_cards_enabled()
        && !config
            .declared_cards
            .as_deref()
            .unwrap_or_default()
            .iter()
            .any(|selection| {
                matches!(selection, CardSelection::Plant(kind) | CardSelection::Imitator(kind) if *kind == effective_kind)
            });
    for edit in config.protection.edits() {
        match edit {
            ProtectionEdit::Add(targets) => {
                if targets
                    .iter()
                    .copied()
                    .any(|target| target_matches(target, grid, raw_kind, effective_kind))
                {
                    protected = true;
                }
            }
            ProtectionEdit::Remove(targets) => {
                if targets
                    .iter()
                    .copied()
                    .any(|target| target_matches(target, grid, raw_kind, effective_kind))
                {
                    protected = false;
                }
            }
            ProtectionEdit::Only(targets) => {
                protected = targets
                    .iter()
                    .copied()
                    .any(|target| target_matches(target, grid, raw_kind, effective_kind));
            }
        }
    }
    protected
}

fn target_matches(target: ProtectTarget, grid: Grid, raw_kind: PlantKind, effective_kind: PlantKind) -> bool {
    match target {
        ProtectTarget::Plant {
            grid: target_grid,
            kind,
        } => target_grid == grid && kind == raw_kind,
        ProtectTarget::Grid(target_grid) => plant_occupies_grid(grid, effective_kind, target_grid),
    }
}

fn key_plant_at(grid: Grid) -> Result<ProtectTarget, ProtectionGridResolveError>
where
    rsvz_current::CurrentBackend: PlantReadBackend,
{
    rsvz_current::with_backend_shared(|backend| {
        let mut best = None;
        for plant in crate::live_value::read_or_abort(backend.plants(), "plants") {
            let anchor = crate::plant::grid_from_handle(backend, plant);
            let effective_kind = backend
                .plant_kind(plant)
                .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
            let raw_kind = backend
                .plant_raw_kind(plant)
                .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
            if !plant_occupies_grid(anchor, effective_kind, grid) {
                continue;
            }
            let priority = match raw_kind {
                PlantKind::Pumpkin => 2,
                PlantKind::LilyPad | PlantKind::FlowerPot => 3,
                _ => 0,
            };
            if best.is_none_or(|(best_priority, _)| priority < best_priority) {
                best = Some((
                    priority,
                    ProtectTarget::Plant {
                        grid: anchor,
                        kind: raw_kind,
                    },
                ));
            }
        }
        best.map(|(_, target)| target).ok_or_else(|| {
            let (row, col) = grid.to_one_based();
            ProtectionGridResolveError::EmptyGrid { row, col }
        })
    })
    .map_err(|error| ProtectionGridResolveError::Backend(RuntimeError::new(error.to_string())))?
}

#[derive(Clone, Debug, PartialEq)]
pub struct EventMeasurementPartial {
    outcomes: [u64; OUTCOME_COUNT],
    damage_total: i64,
    damage_by_source: [i64; DAMAGE_SOURCE_COUNT],
    damage_by_plant: [i64; PLANT_KIND_COUNT],
    home_entries: u64,
    home_by_row: [u64; BOARD_ROWS],
    first_home_time: Option<u64>,
    pogo_entries: u64,
    pogo_by_row: [u64; BOARD_ROWS],
    first_pogo_time: Option<u64>,
    smash_events: u64,
    smash_by_grid: [u64; BOARD_GRIDS],
    imp_leak: ImpLeakAggregate,
    vehicle_crush_sample: Option<PlantLossSample>,
    imitator_ice_loss_sample: Option<PlantLossSample>,
}

impl Default for EventMeasurementPartial {
    fn default() -> Self {
        Self {
            outcomes: [0; OUTCOME_COUNT],
            damage_total: 0,
            damage_by_source: [0; DAMAGE_SOURCE_COUNT],
            damage_by_plant: [0; PLANT_KIND_COUNT],
            home_entries: 0,
            home_by_row: [0; BOARD_ROWS],
            first_home_time: None,
            pogo_entries: 0,
            pogo_by_row: [0; BOARD_ROWS],
            first_pogo_time: None,
            smash_events: 0,
            smash_by_grid: [0; BOARD_GRIDS],
            imp_leak: ImpLeakAggregate::default(),
            vehicle_crush_sample: None,
            imitator_ice_loss_sample: None,
        }
    }
}

impl EventMeasurementPartial {
    pub fn merge_from(&mut self, other: &Self) {
        add_u64(&mut self.outcomes, &other.outcomes);
        self.damage_total = self.damage_total.saturating_add(other.damage_total);
        add_i64(&mut self.damage_by_source, &other.damage_by_source);
        add_i64(&mut self.damage_by_plant, &other.damage_by_plant);
        self.home_entries = self.home_entries.saturating_add(other.home_entries);
        add_u64(&mut self.home_by_row, &other.home_by_row);
        self.first_home_time = min_option(self.first_home_time, other.first_home_time);
        self.pogo_entries = self.pogo_entries.saturating_add(other.pogo_entries);
        add_u64(&mut self.pogo_by_row, &other.pogo_by_row);
        self.first_pogo_time = min_option(self.first_pogo_time, other.first_pogo_time);
        self.smash_events = self.smash_events.saturating_add(other.smash_events);
        add_u64(&mut self.smash_by_grid, &other.smash_by_grid);
        self.imp_leak.merge_from(&other.imp_leak);
        PlantLossSample::merge(&mut self.vehicle_crush_sample, other.vehicle_crush_sample);
        PlantLossSample::merge(&mut self.imitator_ice_loss_sample, other.imitator_ice_loss_sample);
    }

    const fn outcome(&self, outcome: EventTrialOutcome) -> u64 {
        self.outcomes[outcome.index()]
    }

    #[must_use]
    pub fn attempted_trials(&self) -> u64 {
        self.outcomes.iter().copied().fold(0, u64::saturating_add)
    }

    #[must_use]
    pub const fn invalid_trials(&self) -> u64 {
        self.outcome(EventTrialOutcome::Invalid)
    }

    #[cfg(test)]
    fn report(&self, mode: MeasureMode, counts: MeasurementTrialCounts) -> EventMeasureReport {
        self.report_with_window(
            EventMode::from_mode(mode).expect("event mode"),
            counts,
            None,
            ImpLeakDiagnosticConfig::default(),
        )
    }

    fn report_with_window(
        &self, mode: EventMode, counts: MeasurementTrialCounts, window_end: Option<RelativeTime>,
        imp_leak_config: ImpLeakDiagnosticConfig,
    ) -> EventMeasureReport {
        let valid_trials = counts.attempted_trials.saturating_sub(counts.invalid_trials);
        let common = CommonCounts {
            mode: mode.mode().as_str().to_owned(),
            requested_trials: counts.requested_trials,
            attempted_trials: counts.attempted_trials,
            valid_trials,
            invalid_trials: counts.invalid_trials,
            aborted_unrun_trials: counts.aborted_unrun_trials,
            window_end: window_end.map(MeasurementWindowEndReport::from),
        };
        match mode {
            EventMode::DamageNarrow => {
                let success = self.outcome(EventTrialOutcome::ObjectiveReached);
                let garg_smash = self.outcome(EventTrialOutcome::GargSmash);
                let pogo_home = self.outcome(EventTrialOutcome::PogoHome);
                let home_total = self.outcome(EventTrialOutcome::Home).saturating_add(pogo_home);
                let refresh = self.outcome(EventTrialOutcome::RefreshFailure);
                let imp_leak = self.outcome(EventTrialOutcome::ImpLeak);
                let vehicle_crush = self.outcome(EventTrialOutcome::VehicleCrush);
                let imitator_ice_loss = self.outcome(EventTrialOutcome::ImitatorIceLoss);
                let failure = home_total
                    .saturating_add(garg_smash)
                    .saturating_add(refresh)
                    .saturating_add(imp_leak)
                    .saturating_add(vehicle_crush)
                    .saturating_add(imitator_ice_loss);
                EventMeasureReport::DamageNarrow(DamageNarrowReport {
                    common,
                    narrow_pass: PassStats::new(success, failure),
                    damage: DamageStats {
                        total: self.damage_total,
                        by_source: source_map(self.damage_by_source),
                        by_plant: plant_map(self.damage_by_plant),
                    },
                    failures: NarrowFailures {
                        garg_smash,
                        home_total,
                        pogo_home,
                        refresh,
                        imp_leak,
                        vehicle_crush,
                        imitator_ice_loss,
                    },
                    imp_leak: Some(self.imp_leak.report(
                        imp_leak_config.enabled(),
                        imp_leak_config.threshold_cs(),
                        imp_leak,
                    )),
                    vehicle_crush_sample: self.vehicle_crush_sample.map(PlantLossSample::report),
                    imitator_ice_loss_sample: self.imitator_ice_loss_sample.map(PlantLossSample::report),
                })
            }
            EventMode::BroadPass => {
                let success = self.outcome(EventTrialOutcome::ObjectiveReached);
                let failure = self
                    .outcome(EventTrialOutcome::Home)
                    .saturating_add(self.outcome(EventTrialOutcome::PogoHome));
                EventMeasureReport::BroadPass(BroadPassReport {
                    common,
                    broad_pass: PassStats::new(success, failure),
                    home: HomeStats {
                        entries: self.home_entries,
                        by_row: row_map(self.home_by_row),
                        first_entry_time: self.first_home_time,
                    },
                })
            }
            EventMode::Smash => EventMeasureReport::Smash(SmashReport {
                common,
                smash: EventDistribution {
                    trials: valid_trials,
                    events: self.smash_events,
                    probability: report::ratio_or_zero(self.outcome(EventTrialOutcome::GargSmash), valid_trials),
                    by_grid: grid_map(self.smash_by_grid),
                },
            }),
            EventMode::Pogo => EventMeasureReport::Pogo(PogoReport {
                common,
                pogo_home: PogoStats {
                    trials: valid_trials,
                    entries: self.pogo_entries,
                    probability: report::ratio_or_zero(self.outcome(EventTrialOutcome::PogoHome), valid_trials),
                    by_row: row_map(self.pogo_by_row),
                    first_entry_time: self.first_pogo_time,
                },
            }),
        }
    }
}

fn add_u64<const N: usize>(target: &mut [u64; N], source: &[u64; N]) {
    for (target, source) in target.iter_mut().zip(source) {
        *target = target.saturating_add(*source);
    }
}

fn add_i64<const N: usize>(target: &mut [i64; N], source: &[i64; N]) {
    for (target, source) in target.iter_mut().zip(source) {
        *target = target.saturating_add(*source);
    }
}

const fn min_option(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(if left < right { left } else { right }),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum TrialStatus {
    #[default]
    Idle,
    Active,
    Complete(EventTrialOutcome),
}

#[derive(Clone, Copy, Debug)]
struct ActiveFrame {
    counter: i32,
    terminal: Option<EventTrialOutcome>,
}

#[derive(Clone, Copy, Debug)]
struct PendingCompletion {
    outcome: EventTrialOutcome,
    level_ended: bool,
}

pub struct EventMeasurementState {
    config: EventMeasureConfig,
    artifact_config: EventMeasureConfig,
    initialized: bool,
    status: TrialStatus,
    origin_counter: i32,
    board_epoch: Option<u64>,
    frame: Option<ActiveFrame>,
    last_frame_delta: u32,
    pending_completion: Option<PendingCompletion>,
    completed_by_level_end: bool,
    trial: EventMeasurementPartial,
    partial: EventMeasurementPartial,
    imp_tracker: Option<ImpLeakTracker>,
    trial_identity: (u64, u32, u64),
}

const fn relative_elapsed(origin: i32, current: i32) -> Option<u32> {
    let delta = (current as u32).wrapping_sub(origin as u32);
    if delta < 0x8000_0000 { Some(delta) } else { None }
}

pub struct SharedEventMeasure {
    task: Rc<RefCell<EventMeasureTask>>,
}

impl SharedEventMeasure {
    #[must_use]
    pub fn new(task: Rc<RefCell<EventMeasureTask>>) -> Self {
        Self { task }
    }
}

impl InternalEventInterceptor for SharedEventMeasure {
    fn interest(&self) -> EventInterest {
        self.task.borrow().state.interest()
    }

    fn begin_logic_frame(&mut self, board_epoch: u64, main_counter: i32) {
        self.task.borrow_mut().state.begin_frame(board_epoch, main_counter);
    }

    fn begin_plant_effect(&mut self, fact: PlantEffectAttemptFact) -> EventDecision {
        self.task.borrow_mut().state.begin_effect(fact)
    }

    fn finish_plant_effect(&mut self, fact: EffectOutcomeFact) {
        self.task.borrow_mut().state.finish_effect(fact);
    }

    fn emit_home_entry(&mut self, fact: HomeEntryFact) {
        self.task.borrow_mut().state.home_entry(fact);
    }

    fn emit_gargantuar_spawned(&mut self, fact: GargantuarSpawnedFact) {
        self.task.borrow_mut().state.gargantuar_spawned(fact);
    }

    fn emit_imp_thrown(&mut self, fact: ImpThrownFact) {
        self.task.borrow_mut().state.imp_thrown(fact);
    }

    fn emit_gargantuar_ash_hit(&mut self, fact: GargantuarAshHitFact) {
        self.task.borrow_mut().state.gargantuar_ash_hit(fact);
    }

    fn end_logic_frame(&mut self, status: EventFrameStatus) {
        self.task.borrow_mut().state.end_frame(status);
    }
}

pub struct EventMeasureTask {
    state: EventMeasurementState,
    run: TrialRun,
    world_epoch: u64,
    rounds_seen: u64,
    finished: bool,
}

pub enum EventMeasureTaskControl {
    Continue,
    Reset(WorldResetConfig),
    Complete(SessionArtifact),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EventMeasureTaskError {
    #[error("completed-round counter moved backwards")]
    CompletedRoundsMovedBackwards,
    #[error("completed-round boundary does not match one active event trial")]
    CompletedRoundBoundaryMismatch,
    #[error("event measurement reached a trial boundary without a completed trial")]
    MissingCompletedTrial,
    #[error("level ended before the configured measurement window endpoint was reached")]
    WindowEndUnreached,
    #[error(transparent)]
    TrialCount(#[from] MeasurementTrialCountError),
}

impl EventMeasureTask {
    #[must_use]
    pub fn new(
        limit: MeasureLimit, config: EventMeasureConfig, completed_rounds: u32, shard: SessionShard, rounds_seen: u64,
    ) -> Self {
        Self {
            state: EventMeasurementState::new(config),
            run: TrialRun::new(limit, completed_rounds, shard),
            world_epoch: u64::MAX,
            rounds_seen,
            finished: false,
        }
    }

    pub fn set_artifact_limit(&mut self, limit: MeasureLimit) {
        self.run.artifact_limit = limit;
    }

    #[must_use]
    pub const fn initialized(&self) -> bool {
        self.state.initialized()
    }

    pub fn initialize(&mut self) -> Result<(), ProtectionGridResolveError>
    where
        rsvz_current::CurrentBackend: PlantReadBackend,
    {
        self.state.initialize()
    }

    #[must_use]
    pub fn next_reset_config(&self) -> WorldResetConfig {
        self.run.next_reset_config()
    }

    pub fn tick(
        &mut self, world_epoch: u64, main_counter: i32, completed_rounds: u64,
        interrupted: Option<EventMeasureInterruption>,
    ) -> Result<EventMeasureTaskControl, EventMeasureTaskError> {
        self.tick_at(
            world_epoch,
            Some(main_counter),
            &WaveClockState::new(),
            completed_rounds,
            interrupted,
        )
    }

    pub fn tick_at(
        &mut self, world_epoch: u64, current_clock: Option<i32>, clocks: &WaveClockState, completed_rounds: u64,
        interrupted: Option<EventMeasureInterruption>,
    ) -> Result<EventMeasureTaskControl, EventMeasureTaskError> {
        let completed_round_boundary = if completed_rounds != self.rounds_seen {
            let Some(delta) = completed_rounds.checked_sub(self.rounds_seen) else {
                return Err(EventMeasureTaskError::CompletedRoundsMovedBackwards);
            };
            self.rounds_seen = completed_rounds;
            if delta != 1 || self.state.status == TrialStatus::Idle {
                return Err(EventMeasureTaskError::CompletedRoundBoundaryMismatch);
            }
            true
        } else {
            false
        };
        self.ensure_trial(world_epoch, current_clock);
        if let Some(interrupted) = interrupted {
            let mut outcome = match interrupted {
                EventMeasureInterruption::RecoverableError => EventTrialOutcome::Invalid,
                EventMeasureInterruption::TimingViolation if self.state.mode() == MeasureMode::DamageNarrow => {
                    EventTrialOutcome::RefreshFailure
                }
                EventMeasureInterruption::TimingViolation => EventTrialOutcome::Invalid,
            };
            if outcome == EventTrialOutcome::RefreshFailure {
                self.state.resolve_pending_imp_cohorts(clocks);
                if self
                    .state
                    .imp_tracker
                    .as_ref()
                    .is_some_and(ImpLeakTracker::has_critical_overflow)
                {
                    outcome = EventTrialOutcome::Invalid;
                }
            }
            self.state.finish(outcome);
        }
        self.state.finish_pending(clocks);
        let window_reached = measurement_window_reached(self.state.config.window_end, current_clock, clocks);
        if self.state.completed().is_some() {
            if self.state.completed_by_level_end() && window_reached == Some(false) {
                return Err(EventMeasureTaskError::WindowEndUnreached);
            }
            return self.finish_boundary();
        }
        if window_reached == Some(true) {
            self.state.finish(EventTrialOutcome::ObjectiveReached);
            return self.finish_boundary();
        }
        if completed_round_boundary {
            if window_reached == Some(false) {
                return Err(EventMeasureTaskError::WindowEndUnreached);
            }
            self.state.finish(EventTrialOutcome::ObjectiveReached);
            return self.finish_boundary();
        }
        Ok(EventMeasureTaskControl::Continue)
    }

    pub fn sample_imp_leak(
        &mut self, world_epoch: u64, current_clock: Option<i32>, clocks: &WaveClockState,
    ) -> RuntimeResult<()>
    where
        rsvz_current::CurrentBackend: PlantReadBackend + ZombieRawFactsBackend + ZombieStateBackend,
    {
        self.ensure_trial(world_epoch, current_clock);
        self.state.sample_imp_leak(current_clock.unwrap_or_default(), clocks)
    }

    fn ensure_trial(&mut self, world_epoch: u64, current_clock: Option<i32>) {
        if world_epoch == self.world_epoch {
            return;
        }
        self.world_epoch = world_epoch;
        let sequence = self.run.sequence();
        let seed = self.run.next_reset_config().seed;
        self.state
            .start_trial_with_identity(current_clock.unwrap_or_default(), sequence, seed, world_epoch);
    }

    fn finish_boundary(&mut self) -> Result<EventMeasureTaskControl, EventMeasureTaskError> {
        if self.state.completed().is_none() {
            return Err(EventMeasureTaskError::MissingCompletedTrial);
        }
        if self.run.finish_trial(self.state.partial.attempted_trials()) {
            let artifact = self.state.take_artifact_with_run(
                self.run.limit,
                MeasurementEnd::Completed,
                self.run.artifact_config(),
            )?;
            self.finished = true;
            return Ok(EventMeasureTaskControl::Complete(artifact));
        }
        Ok(EventMeasureTaskControl::Reset(self.next_reset_config()))
    }

    pub fn seal_active_invalid(&mut self) -> bool {
        self.state.seal_invalid()
    }

    #[must_use]
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn aborted_artifact(&mut self) -> Result<SessionArtifact, MeasurementTrialCountError> {
        self.state
            .take_artifact_with_run(self.run.limit, MeasurementEnd::Aborted, self.run.artifact_config())
    }

    #[must_use]
    pub fn empty_artifact(requested_trials: Option<u64>, config: EventMeasureConfig) -> SessionArtifact {
        Self::empty_artifact_with_run(
            requested_trials,
            config,
            MeasureLimit::trials(requested_trials.unwrap_or(1).max(1))
                .expect("the normalized empty-artifact trial count is nonzero"),
            0,
            SessionShard::default(),
        )
    }

    #[must_use]
    pub fn empty_artifact_with_run(
        requested_trials: Option<u64>, config: EventMeasureConfig, artifact_limit: MeasureLimit, completed_rounds: u32,
        shard: SessionShard,
    ) -> SessionArtifact {
        EventMeasureArtifact {
            config,
            partial: EventMeasurementPartial::default(),
            counts: MeasurementTrialCounts {
                requested_trials,
                attempted_trials: 0,
                invalid_trials: 0,
                aborted_unrun_trials: requested_trials.unwrap_or(0),
            },
            run: MeasurementRunConfig::new(artifact_limit, completed_rounds, shard),
        }
        .into_session_artifact()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventMeasureInterruption {
    RecoverableError,
    TimingViolation,
}

struct EventMeasureArtifact {
    config: EventMeasureConfig,
    partial: EventMeasurementPartial,
    counts: MeasurementTrialCounts,
    run: MeasurementRunConfig,
}

impl EventMeasureArtifact {
    fn into_session_artifact(self) -> SessionArtifact {
        SessionArtifact::new(self, merge_event_measure_artifacts, write_event_measure_artifact)
    }
}

fn merge_event_measure_artifacts(target: &mut EventMeasureArtifact, source: EventMeasureArtifact) -> RuntimeResult<()> {
    if target.config != source.config || target.run != source.run {
        return Err(RuntimeError::new(
            "cannot merge event measurement artifacts with different mode or configuration",
        ));
    }
    target.partial.merge_from(&source.partial);
    target.counts.requested_trials = match (target.counts.requested_trials, source.counts.requested_trials) {
        (Some(target), Some(source)) => Some(target.saturating_add(source)),
        (None, None) => None,
        _ => {
            return Err(RuntimeError::new(
                "cannot merge trial- and duration-limited event measurement artifacts",
            ));
        }
    };
    target.counts.attempted_trials = target
        .counts
        .attempted_trials
        .saturating_add(source.counts.attempted_trials);
    target.counts.invalid_trials = target
        .counts
        .invalid_trials
        .saturating_add(source.counts.invalid_trials);
    target.counts.aborted_unrun_trials = target
        .counts
        .aborted_unrun_trials
        .saturating_add(source.counts.aborted_unrun_trials);
    Ok(())
}

fn write_event_measure_artifact(artifact: &EventMeasureArtifact, writer: &mut dyn Write) -> io::Result<()> {
    let report = artifact.partial.report_with_window(
        artifact.config.mode,
        artifact.counts,
        artifact.config.window_end,
        artifact.config.imp_leak,
    );
    serde_json::to_writer(writer, &report).map_err(io::Error::other)
}

fn source_map(values: [i64; DAMAGE_SOURCE_COUNT]) -> BTreeMap<String, i64> {
    ["jack_explosion", "zombie_chew", "catapult_basket"]
        .into_iter()
        .zip(values)
        .filter(|(_, value)| *value != 0)
        .map(|(name, value)| (name.to_owned(), value))
        .collect()
}

fn plant_map(values: [i64; PLANT_KIND_COUNT]) -> BTreeMap<String, i64> {
    PlantKind::ALL
        .into_iter()
        .zip(values)
        .filter(|(_, value)| *value != 0)
        .map(|(kind, value)| (kind.report_name().to_owned(), value))
        .collect()
}

fn row_map(values: [u64; BOARD_ROWS]) -> BTreeMap<String, u64> {
    values
        .into_iter()
        .enumerate()
        .filter(|(_, value)| *value != 0)
        .map(|(row, value)| ((row + 1).to_string(), value))
        .collect()
}

fn grid_map(values: [u64; BOARD_GRIDS]) -> BTreeMap<String, u64> {
    values
        .into_iter()
        .enumerate()
        .filter(|(_, value)| *value != 0)
        .map(|(index, value)| {
            let row = index / BOARD_COLS + 1;
            let col = index % BOARD_COLS + 1;
            (format!("r{row}c{col}"), value)
        })
        .collect()
}

#[cfg(test)]
mod tests;

mod state;
