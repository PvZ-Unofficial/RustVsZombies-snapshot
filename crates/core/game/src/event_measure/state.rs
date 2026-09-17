use super::*;

impl EventMeasurementState {
    #[must_use]
    pub fn new(config: EventMeasureConfig) -> Self {
        let imp_enabled = config.mode == EventMode::DamageNarrow && config.imp_leak.enabled();
        let imp_threshold = config.imp_leak.threshold_cs();
        let mut trial = EventMeasurementPartial::default();
        let mut partial = EventMeasurementPartial::default();
        if imp_enabled {
            trial.imp_leak.reserve(false);
            partial.imp_leak.reserve(true);
        }
        Self {
            artifact_config: config.clone(),
            config,
            initialized: false,
            status: TrialStatus::Idle,
            origin_counter: 0,
            board_epoch: None,
            frame: None,
            last_frame_delta: 0,
            pending_completion: None,
            completed_by_level_end: false,
            trial,
            partial,
            imp_tracker: imp_enabled.then(|| ImpLeakTracker::new(imp_threshold)),
            trial_identity: (0, 0, 0),
        }
    }

    #[must_use]
    pub const fn mode(&self) -> MeasureMode {
        self.config.mode.mode()
    }

    #[must_use]
    pub const fn initialized(&self) -> bool {
        self.initialized
    }

    pub fn initialize(&mut self) -> Result<(), ProtectionGridResolveError>
    where
        rsvz_current::CurrentBackend: PlantReadBackend,
    {
        resolve_event_measure_config(&mut self.config)?;
        self.initialized = true;
        Ok(())
    }

    #[must_use]
    pub fn interest(&self) -> EventInterest {
        let base = match self.config.mode {
            EventMode::DamageNarrow => EventInterest::PLANT_EFFECT.union(EventInterest::HOME_ENTRY),
            EventMode::BroadPass | EventMode::Pogo => EventInterest::HOME_ENTRY,
            EventMode::Smash => EventInterest::GARGANTUAR,
        };
        if self.imp_tracker.is_some() {
            base.union(EventInterest::IMP_DIAGNOSTIC)
        } else {
            base
        }
    }

    pub fn start_trial(&mut self, main_counter: i32) {
        self.start_trial_with_identity(main_counter, 0, 0, 0);
    }

    pub(super) fn start_trial_with_identity(
        &mut self, main_counter: i32, trial_sequence: u64, seed: u32, world_epoch: u64,
    ) {
        if self.status == TrialStatus::Active {
            self.commit(EventTrialOutcome::Invalid);
        }
        self.status = TrialStatus::Active;
        self.origin_counter = main_counter;
        self.board_epoch = None;
        self.frame = None;
        self.last_frame_delta = 0;
        self.pending_completion = None;
        self.completed_by_level_end = false;
        self.trial.outcomes.fill(0);
        self.trial.damage_total = 0;
        self.trial.damage_by_source.fill(0);
        self.trial.damage_by_plant.fill(0);
        self.trial.home_entries = 0;
        self.trial.home_by_row.fill(0);
        self.trial.first_home_time = None;
        self.trial.pogo_entries = 0;
        self.trial.pogo_by_row.fill(0);
        self.trial.first_pogo_time = None;
        self.trial.smash_events = 0;
        self.trial.smash_by_grid.fill(0);
        self.trial.imp_leak.clear_trial();
        self.trial.vehicle_crush_sample = None;
        self.trial.imitator_ice_loss_sample = None;
        self.trial_identity = (trial_sequence, seed, world_epoch);
        if let Some(tracker) = &mut self.imp_tracker {
            tracker.start_trial(trial_sequence, seed, world_epoch);
        }
    }

    #[must_use]
    pub const fn trial_active(&self) -> bool {
        matches!(self.status, TrialStatus::Active)
    }

    pub(super) fn completed(&self) -> Option<EventTrialOutcome> {
        match self.status {
            TrialStatus::Complete(outcome) => Some(outcome),
            TrialStatus::Idle | TrialStatus::Active => None,
        }
    }

    pub(super) fn completed_by_level_end(&self) -> bool {
        self.completed_by_level_end && self.completed().is_some()
    }

    pub fn seal_invalid(&mut self) -> bool {
        if !self.trial_active() {
            return false;
        }
        self.commit(EventTrialOutcome::Invalid);
        true
    }

    pub(super) fn finish(&mut self, outcome: EventTrialOutcome) {
        if self.trial_active() {
            self.completed_by_level_end = false;
            self.commit(outcome);
        }
    }

    pub(super) fn finish_pending(&mut self, clocks: &WaveClockState) {
        if self.pending_completion.is_some() {
            self.resolve_pending_imp_cohorts(clocks);
            if self
                .imp_tracker
                .as_ref()
                .is_some_and(ImpLeakTracker::has_critical_overflow)
            {
                self.pending_completion = None;
                self.commit(EventTrialOutcome::Invalid);
                return;
            }
        }
        if let Some(pending) = self.pending_completion.take() {
            for sample in [
                &mut self.trial.vehicle_crush_sample,
                &mut self.trial.imitator_ice_loss_sample,
            ]
            .into_iter()
            .flatten()
            {
                sample.resolve_time(clocks);
            }
            self.completed_by_level_end = pending.level_ended;
            self.commit(pending.outcome);
        }
    }

    pub(super) fn resolve_pending_imp_cohorts(&mut self, clocks: &WaveClockState) {
        if let Some(tracker) = &mut self.imp_tracker {
            tracker.resolve_pending_cohorts(clocks, &mut self.trial.imp_leak);
        }
    }

    pub(super) fn begin_frame(&mut self, board_epoch: u64, main_counter: i32) {
        if !self.trial_active() {
            return;
        }
        if self.frame.is_some() || self.pending_completion.is_some() {
            self.commit(EventTrialOutcome::Invalid);
            return;
        }
        if self.board_epoch.is_none() {
            self.board_epoch = Some(board_epoch);
        }
        let delta = relative_elapsed(self.origin_counter, main_counter);
        if self.board_epoch != Some(board_epoch) || delta.is_none_or(|delta| delta < self.last_frame_delta) {
            self.commit(EventTrialOutcome::Invalid);
            return;
        }
        self.last_frame_delta = delta.unwrap_or_default();
        self.frame = Some(ActiveFrame {
            counter: main_counter,
            terminal: None,
        });
    }

    pub(super) fn begin_effect(&mut self, fact: PlantEffectAttemptFact) -> EventDecision {
        if !self.initialized
            || !self.trial_active()
            || self.frame.is_none_or(|frame| frame.counter != fact.main_counter)
        {
            return EventDecision::Apply;
        }
        if self.config.mode == EventMode::DamageNarrow
            && fact.raw_kind == PlantKind::Imitator
            && fact.effective_kind == PlantKind::IceShroom
        {
            return EventDecision::Apply;
        }
        let protected = protects(&self.config, fact.grid, fact.raw_kind, fact.effective_kind);
        if let PlantEffectSource::Bite(zombie_id) = fact.source
            && let Some(tracker) = &mut self.imp_tracker
        {
            tracker.note_bite(
                zombie_id,
                bite_target(fact.plant_id, fact.raw_kind, fact.effective_kind, fact.grid),
                protected,
                fact.main_counter,
            );
        }
        if !matches!(self.config.mode, EventMode::DamageNarrow | EventMode::Smash) || !protected {
            return EventDecision::Apply;
        }
        match fact.source {
            PlantEffectSource::Gargantuar(_) => self.record_garg_smash(fact.grid),
            PlantEffectSource::Jack(_) | PlantEffectSource::Bite(_) | PlantEffectSource::Basketball(_)
                if self.config.mode == EventMode::DamageNarrow =>
            {
                self.record_legacy_damage(fact);
            }
            PlantEffectSource::Jack(_) | PlantEffectSource::Bite(_) | PlantEffectSource::Basketball(_) => {
                return EventDecision::Apply;
            }
            PlantEffectSource::ZomboniCrush(_) | PlantEffectSource::CatapultCrush(_) | PlantEffectSource::Bungee(_) => {
                return EventDecision::Apply;
            }
        }
        EventDecision::SuppressByMeasurement
    }

    pub(super) fn finish_effect(&mut self, fact: EffectOutcomeFact) {
        if fact.decision_origin == rsvz_model::EventDecisionOrigin::Measurement
            && fact.outcome != PlantEffectOutcome::SuppressedByMeasurement
        {
            self.commit(EventTrialOutcome::Invalid);
            return;
        }
        if self.config.mode != EventMode::DamageNarrow
            || !self.trial_active()
            || !matches!(
                fact.outcome,
                PlantEffectOutcome::Killed | PlantEffectOutcome::Squished | PlantEffectOutcome::Stolen
            )
        {
            return;
        }
        let key = fact.key;
        let terminal = if key.raw_kind == PlantKind::Imitator && key.effective_kind == PlantKind::IceShroom {
            Some(EventTrialOutcome::ImitatorIceLoss)
        } else if matches!(
            key.source,
            PlantEffectSource::ZomboniCrush(_) | PlantEffectSource::CatapultCrush(_)
        ) && protects(&self.config, key.grid, key.raw_kind, key.effective_kind)
        {
            Some(EventTrialOutcome::VehicleCrush)
        } else {
            None
        };
        if let Some(frame) = &mut self.frame
            && frame.terminal.is_none()
        {
            frame.terminal = terminal;
            if let Some(terminal) = terminal {
                let sample = Some(PlantLossSample {
                    trial_sequence: self.trial_identity.0,
                    seed: self.trial_identity.1,
                    world_epoch: self.trial_identity.2,
                    main_counter: frame.counter,
                    wave_time: None,
                    fact,
                });
                match terminal {
                    EventTrialOutcome::VehicleCrush => self.trial.vehicle_crush_sample = sample,
                    EventTrialOutcome::ImitatorIceLoss => self.trial.imitator_ice_loss_sample = sample,
                    _ => unreachable!(),
                }
            }
        }
    }

    pub(super) fn home_entry(&mut self, fact: HomeEntryFact) {
        if !self.trial_active() || self.frame.is_none_or(|frame| frame.counter != fact.main_counter) {
            return;
        }
        let Ok(row) = usize::try_from(fact.row) else {
            self.commit(EventTrialOutcome::Invalid);
            return;
        };
        if row >= BOARD_ROWS {
            self.commit(EventTrialOutcome::Invalid);
            return;
        }
        let Some(elapsed) = relative_elapsed(self.origin_counter, fact.main_counter).map(u64::from) else {
            self.commit(EventTrialOutcome::Invalid);
            return;
        };
        self.trial.home_entries = self.trial.home_entries.saturating_add(1);
        self.trial.home_by_row[row] = self.trial.home_by_row[row].saturating_add(1);
        self.trial.first_home_time = min_option(self.trial.first_home_time, Some(elapsed));
        if fact.zombie_kind == ZombieKind::Pogo {
            self.trial.pogo_entries = self.trial.pogo_entries.saturating_add(1);
            self.trial.pogo_by_row[row] = self.trial.pogo_by_row[row].saturating_add(1);
            self.trial.first_pogo_time = min_option(self.trial.first_pogo_time, Some(elapsed));
        }
        let terminal = match self.config.mode {
            EventMode::DamageNarrow | EventMode::BroadPass => Some(if fact.zombie_kind == ZombieKind::Pogo {
                EventTrialOutcome::PogoHome
            } else {
                EventTrialOutcome::Home
            }),
            EventMode::Pogo if fact.zombie_kind == ZombieKind::Pogo => Some(EventTrialOutcome::PogoHome),
            EventMode::Smash | EventMode::Pogo => None,
        };
        if let Some(frame) = &mut self.frame
            && frame.terminal.is_none()
        {
            frame.terminal = terminal;
        }
    }

    pub(super) fn gargantuar_spawned(&mut self, fact: GargantuarSpawnedFact) {
        if self.trial_active()
            && self.frame.is_some_and(|frame| frame.counter == fact.main_counter)
            && let Some(tracker) = &mut self.imp_tracker
        {
            tracker.note_gargantuar_spawned(fact);
        }
    }

    pub(super) fn imp_thrown(&mut self, fact: ImpThrownFact) {
        if self.trial_active()
            && self.frame.is_some_and(|frame| frame.counter == fact.main_counter)
            && let Some(tracker) = &mut self.imp_tracker
        {
            tracker.note_imp_thrown(fact);
        }
    }

    pub(super) fn gargantuar_ash_hit(&mut self, fact: GargantuarAshHitFact) {
        if self.trial_active()
            && self.frame.is_some_and(|frame| frame.counter == fact.main_counter)
            && let Some(tracker) = &mut self.imp_tracker
        {
            tracker.note_ash_hit(fact);
        }
    }

    pub(super) fn sample_imp_leak(&mut self, main_counter: i32, clocks: &WaveClockState) -> RuntimeResult<()>
    where
        rsvz_current::CurrentBackend: PlantReadBackend + ZombieRawFactsBackend + ZombieStateBackend,
    {
        if !self.trial_active() || self.pending_completion.is_some() {
            return Ok(());
        }
        let Some(tracker) = &mut self.imp_tracker else {
            return Ok(());
        };
        match tracker.sample(
            main_counter,
            clocks,
            &mut self.trial.imp_leak,
            &mut self.partial.imp_leak,
        )? {
            SampleOutcome::Continue => {}
            SampleOutcome::Invalid => self.commit(EventTrialOutcome::Invalid),
            SampleOutcome::ImpLeak => self.finish(EventTrialOutcome::ImpLeak),
        }
        Ok(())
    }

    pub(super) fn end_frame(&mut self, status: EventFrameStatus) {
        if !self.trial_active() {
            return;
        }
        let Some(frame) = self.frame.take() else {
            self.commit(EventTrialOutcome::Invalid);
            return;
        };
        let terminal = frame.terminal;
        let outcome = terminal.or(match status {
            BattleStatus::ObjectiveReached => Some(EventTrialOutcome::ObjectiveReached),
            BattleStatus::Lost | BattleStatus::Ended
                if matches!(self.config.mode, EventMode::Smash | EventMode::Pogo) =>
            {
                Some(EventTrialOutcome::GameOver)
            }
            BattleStatus::Lost | BattleStatus::Ended => Some(EventTrialOutcome::Invalid),
            BattleStatus::Running | BattleStatus::Unknown => None,
        });
        self.pending_completion = outcome.map(|outcome| PendingCompletion {
            outcome,
            level_ended: terminal.is_none() && matches!(status, BattleStatus::ObjectiveReached | BattleStatus::Ended),
        });
    }

    pub(super) fn record_legacy_damage(&mut self, fact: PlantEffectAttemptFact) {
        let (source, loss) = match (fact.source, fact.effect) {
            (PlantEffectSource::Jack(_), PlantEffect::InstantKill) => (0, fact.max_hp.max(0)),
            (PlantEffectSource::Bite(_), PlantEffect::HpDamage { native_requested }) => (1, native_requested.max(0)),
            (PlantEffectSource::Basketball(_), PlantEffect::HpDamage { .. }) => (2, fact.max_hp.max(0)),
            _ => return,
        };
        let loss = i64::from(loss);
        self.trial.damage_total = self.trial.damage_total.saturating_add(loss);
        self.trial.damage_by_source[source] = self.trial.damage_by_source[source].saturating_add(loss);
        let plant = fact.raw_kind as usize;
        if plant < PLANT_KIND_COUNT {
            self.trial.damage_by_plant[plant] = self.trial.damage_by_plant[plant].saturating_add(loss);
        }
    }

    pub(super) fn record_garg_smash(&mut self, grid: Grid) {
        let (Ok(row), Ok(col)) = (usize::try_from(grid.row), usize::try_from(grid.col)) else {
            self.commit(EventTrialOutcome::Invalid);
            return;
        };
        if row >= BOARD_ROWS || col >= BOARD_COLS {
            self.commit(EventTrialOutcome::Invalid);
            return;
        }
        self.trial.smash_events = self.trial.smash_events.saturating_add(1);
        let index = row * BOARD_COLS + col;
        self.trial.smash_by_grid[index] = self.trial.smash_by_grid[index].saturating_add(1);
        if let Some(frame) = &mut self.frame
            && frame.terminal.is_none()
        {
            frame.terminal = Some(EventTrialOutcome::GargSmash);
        }
    }

    pub(super) fn commit(&mut self, outcome: EventTrialOutcome) {
        if !self.trial_active() {
            return;
        }
        if let Some(tracker) = &mut self.imp_tracker {
            let end = match outcome {
                EventTrialOutcome::ObjectiveReached => TrialEnd::ObjectiveReached,
                EventTrialOutcome::ImpLeak => TrialEnd::ImpLeak,
                EventTrialOutcome::Invalid => TrialEnd::Invalid,
                _ => TrialEnd::OtherFailure,
            };
            tracker.finish_trial(end, &mut self.trial.imp_leak);
        }
        self.partial.outcomes[outcome.index()] = self.partial.outcomes[outcome.index()].saturating_add(1);
        if outcome.is_valid() {
            self.partial.merge_from(&self.trial);
        } else {
            self.partial.imp_leak.merge_drops_from(&self.trial.imp_leak);
        }
        self.status = TrialStatus::Complete(outcome);
        self.frame = None;
        self.pending_completion = None;
    }

    pub(super) fn trial_counts(
        &self, limit: MeasureLimit, end: MeasurementEnd,
    ) -> Result<MeasurementTrialCounts, MeasurementTrialCountError> {
        if self.trial_active() {
            return Err(MeasurementTrialCountError::ActiveTrial);
        }
        let attempted_trials = self.partial.attempted_trials();
        let invalid_trials = self.partial.invalid_trials();
        let requested_trials = limit.trial_target();
        let aborted_unrun_trials = match (requested_trials, end) {
            (Some(requested), MeasurementEnd::Completed) if requested != attempted_trials => {
                return Err(MeasurementTrialCountError::CompletedTrialCountMismatch {
                    requested_trials: requested,
                    attempted_trials,
                });
            }
            (Some(requested), MeasurementEnd::Aborted) if attempted_trials > requested => {
                return Err(MeasurementTrialCountError::AttemptedExceedsRequested {
                    requested_trials: requested,
                    attempted_trials,
                });
            }
            (Some(requested), MeasurementEnd::Aborted) => requested - attempted_trials,
            (Some(_), MeasurementEnd::Completed) | (None, _) => 0,
        };
        Ok(MeasurementTrialCounts {
            requested_trials,
            attempted_trials,
            invalid_trials,
            aborted_unrun_trials,
        })
    }

    #[cfg(test)]
    pub(super) fn artifact(
        &self, limit: MeasureLimit, end: MeasurementEnd,
    ) -> Result<SessionArtifact, MeasurementTrialCountError> {
        self.artifact_with_run(limit, end, MeasurementRunConfig::new(limit, 0, SessionShard::default()))
    }

    #[cfg(test)]
    pub(super) fn artifact_with_run(
        &self, limit: MeasureLimit, end: MeasurementEnd, run: MeasurementRunConfig,
    ) -> Result<SessionArtifact, MeasurementTrialCountError> {
        let counts = self.trial_counts(limit, end)?;
        Ok(EventMeasureArtifact {
            config: self.artifact_config.clone(),
            partial: self.partial.clone(),
            counts,
            run,
        }
        .into_session_artifact())
    }

    pub(super) fn take_artifact_with_run(
        &mut self, limit: MeasureLimit, end: MeasurementEnd, run: MeasurementRunConfig,
    ) -> Result<SessionArtifact, MeasurementTrialCountError> {
        let counts = self.trial_counts(limit, end)?;
        Ok(EventMeasureArtifact {
            config: self.artifact_config.clone(),
            partial: std::mem::take(&mut self.partial),
            counts,
            run,
        }
        .into_session_artifact())
    }
}
