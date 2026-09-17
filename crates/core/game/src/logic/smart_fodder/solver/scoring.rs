use super::*;

impl SmartFodderModel {
    #[allow(
        clippy::too_many_arguments,
        reason = "the exact fused score reuses caller-owned scratch buffers"
    )]
    pub(super) fn fused_branch_score_with_scratch(
        &self, tables: DamageTables<'_>, release_values: &ReleaseValueWorkspace, contacts: &[Option<(i32, f32)>],
        release_queries: &[Option<ReleaseQuery>], direct_corrections: &[Option<DirectCorrectionSeries>],
        jack_blast_axes: &[AbsoluteJackBlastAxes], candidate: &CandidateProbabilityCache<'_, '_>,
        states: &[BranchScoreState], branch_inputs: &[CapOneBranchInput], time_groups: &BranchEndGrouping<i32>,
        ordered_groups: &BranchEndGrouping<DeterministicFodderEnd>, terms: &mut Vec<CapOneBranchInput>,
    ) -> Result<f64, SmartFodderSolveError> {
        let mut damage = 0.0;
        let plant_at = candidate.aggregate(branch_inputs, None).start();
        let mut jack = 0;
        for (threat_index, threat) in self.threats.iter().enumerate() {
            if threat.jack_pmf.is_none() {
                continue;
            }
            let current_jack = jack;
            jack += 1;
            let focused_all = candidate.aggregate(branch_inputs, Some(current_jack));
            damage += 300.0
                * focused_all.survival_after(focused_all.start().saturating_sub(1))
                * jack_blast_axes[current_jack].baseline_at_or_after(focused_all.start());
            let Some(corrections) = direct_corrections[threat_index].as_ref() else {
                continue;
            };
            let fused_ordinary = threat.ordinary_damage_enabled
                && !threat.pole_vault
                && contacts[threat_index].is_some()
                && release_queries[threat_index].is_some();
            for group in &time_groups.groups {
                time_groups.fill_terms(group, states, terms, |state| {
                    state.input.excluded_jack != Some(current_jack)
                });
                if terms.is_empty() {
                    continue;
                }
                let focused = candidate.aggregate(terms, Some(current_jack));
                let end = group.key.clamp(focused.start(), self.domain.activation_at);
                if fused_ordinary {
                    let contact_at = contacts[threat_index].expect("fused ordinary contact").0;
                    let query = release_queries[threat_index].expect("fused ordinary release query");
                    damage += aggregate_ordinary_damage(
                        release_values,
                        &focused,
                        query,
                        threat,
                        release_values.jack_baseline_at_or_after(threat_index, plant_at),
                        contact_at,
                        DeterministicFodderEnd {
                            time: group.key,
                            frame_order: None,
                        },
                        self.domain.activation_at,
                        Some(corrections),
                    )?;
                } else {
                    let correction = aggregate_direct_weighted_before(&focused, corrections, end)
                        + focused.survival_after(end.saturating_sub(1)) * corrections.correction_at(end);
                    damage += 300.0 * correction;
                }
                for ordered_group in ordered_groups
                    .groups
                    .iter()
                    .filter(|ordered| ordered.key.time == group.key)
                {
                    ordered_groups.fill_terms(ordered_group, states, terms, |state| {
                        state.input.excluded_jack != Some(current_jack)
                    });
                    if terms.is_empty() {
                        continue;
                    }
                    let ordered = candidate.aggregate(terms, Some(current_jack));
                    if fused_ordinary {
                        let contact_at = contacts[threat_index].expect("fused ordinary contact").0;
                        let query = release_queries[threat_index].expect("fused ordinary release query");
                        damage += aggregate_ordinary_order_correction(
                            release_values,
                            &ordered,
                            query,
                            threat,
                            release_values.jack_baseline_at_or_after(threat_index, plant_at),
                            contact_at,
                            ordered_group.key,
                            self.domain.activation_at,
                            Some(corrections),
                        )?;
                    } else {
                        damage += 300.0
                            * deterministic_order_correction(
                                |time| ordered.first_probability_at_or_after_focus(time),
                                |time| ordered.survival_after(time),
                                corrections.contact_at,
                                corrections.bite_frame_order,
                                ordered_group.key,
                                |time| corrections.correction_at(time),
                                |time| corrections.delayed_correction_at(time),
                                0.0,
                                self.domain.activation_at,
                            );
                    }
                }
            }
        }
        let mut ordinary_jack = 0;
        for (index, (threat, contact)) in self.threats.iter().zip(contacts).enumerate() {
            let focused_jack = threat.jack_pmf.as_ref().map(|_| {
                let jack = ordinary_jack;
                ordinary_jack += 1;
                jack
            });
            if !threat.ordinary_damage_enabled {
                continue;
            }
            if focused_jack.is_some()
                && !threat.pole_vault
                && contact.is_some()
                && direct_corrections[index].is_some()
                && release_queries[index].is_some()
            {
                continue;
            }
            let baseline_damage = focused_jack.map_or(threat.baseline_damage, |_| {
                release_values.jack_baseline_at_or_after(index, plant_at)
            });
            terms.clear();
            terms.extend(
                states
                    .iter()
                    .filter(|state| state.excluded_threat != Some(index))
                    .map(|state| state.input),
            );
            if terms.is_empty() {
                continue;
            }
            if contact.is_none() {
                let branch = candidate.aggregate(terms, focused_jack);
                damage += branch.branch_mass() * baseline_damage;
                continue;
            }
            let (contact_at, _contact_x) = contact.expect("checked above");
            if threat.pole_vault {
                let landing = contact_at.saturating_add(POLE_VAULT_DURATION);
                let after_landing = self.domain.activation_at.saturating_sub(landing);
                for group in &ordered_groups.groups {
                    ordered_groups.fill_terms(group, states, terms, |state| state.excluded_threat != Some(index));
                    if terms.is_empty() {
                        continue;
                    }
                    let branch = candidate.aggregate(terms, focused_jack);
                    if group.key.time < contact_at {
                        damage += branch.branch_mass() * baseline_damage;
                        continue;
                    }
                    if after_landing <= SLOWED_POLE_MIN_AFTER_LANDING {
                        continue;
                    }
                    damage += aggregate_fodder_presence_at_actor(
                        &branch,
                        group.key,
                        contact_at,
                        zombie_before_check_order(threat.update_rank),
                    ) * f64::from(tables.pole_damage(usize::try_from(after_landing).unwrap_or_default())?);
                }
                continue;
            }
            let release_query = release_queries[index]
                .as_ref()
                .ok_or(SmartFodderSolveError::UnsupportedReleaseThreat)?;
            for group in &time_groups.groups {
                time_groups.fill_terms(group, states, terms, |state| state.excluded_threat != Some(index));
                if terms.is_empty() {
                    continue;
                }
                let branch = candidate.aggregate(terms, focused_jack);
                damage += aggregate_ordinary_damage(
                    release_values,
                    &branch,
                    *release_query,
                    threat,
                    baseline_damage,
                    contact_at,
                    DeterministicFodderEnd {
                        time: group.key,
                        frame_order: None,
                    },
                    self.domain.activation_at,
                    None,
                )?;
                for ordered_group in ordered_groups
                    .groups
                    .iter()
                    .filter(|ordered| ordered.key.time == group.key)
                {
                    ordered_groups.fill_terms(ordered_group, states, terms, |state| {
                        state.excluded_threat != Some(index)
                    });
                    if terms.is_empty() {
                        continue;
                    }
                    let ordered = candidate.aggregate(terms, focused_jack);
                    damage += aggregate_ordinary_order_correction(
                        release_values,
                        &ordered,
                        *release_query,
                        threat,
                        baseline_damage,
                        contact_at,
                        ordered_group.key,
                        self.domain.activation_at,
                        None,
                    )?;
                }
            }
        }
        Ok(damage)
    }

    pub(super) fn ordinary_damage_lower_bound(
        &self, release_values: &ReleaseValueWorkspace, contacts: &[Option<(i32, f32)>],
        candidate: &CandidateProbabilityCache<'_, '_>, states: &[BranchScoreState],
        time_groups: &BranchEndGrouping<i32>, terms: &mut Vec<CapOneBranchInput>,
    ) -> f64 {
        let plant_at = candidate.aggregate(&[], None).start();
        let mut lower_bound = 0.0;
        let mut jack = 0;
        for (index, (threat, contact)) in self.threats.iter().zip(contacts).enumerate() {
            let focused_jack = threat.jack_pmf.as_ref().map(|_| {
                let current = jack;
                jack += 1;
                current
            });
            if !threat.ordinary_damage_enabled {
                continue;
            }
            let baseline = focused_jack.map_or(threat.baseline_damage, |_| {
                release_values.jack_baseline_at_or_after(index, plant_at)
            });
            if baseline <= 0.0 {
                continue;
            }
            let Some((contact_at, _x)) = *contact else {
                terms.clear();
                terms.extend(
                    states
                        .iter()
                        .filter(|state| state.excluded_threat != Some(index))
                        .map(|state| state.input),
                );
                lower_bound += candidate.aggregate(terms, focused_jack).branch_mass() * baseline;
                continue;
            };
            for group in &time_groups.groups {
                time_groups.fill_terms(group, states, terms, |state| state.excluded_threat != Some(index));
                if terms.is_empty() {
                    continue;
                }
                let branch = candidate.aggregate(terms, focused_jack);
                if group.key < contact_at {
                    lower_bound += branch.branch_mass() * baseline;
                } else if !threat.pole_vault {
                    lower_bound += (branch.branch_mass() - branch.survival_after(contact_at.saturating_sub(1)))
                        .max(0.0)
                        * baseline;
                }
            }
        }
        lower_bound
    }

    pub(super) fn excluded_threat(&self, excluded_jack: Option<usize>) -> Option<usize> {
        excluded_jack.and_then(|target| {
            self.threats
                .iter()
                .enumerate()
                .filter(|(_index, threat)| threat.jack_pmf.is_some())
                .nth(target)
                .map(|(index, _threat)| index)
        })
    }

    pub(super) fn direct_x_query(
        &self, tables: DamageTables<'_>, x: f32,
    ) -> Result<(usize, Option<(usize, f64)>), SmartFodderTableError> {
        let (left, right) = tables.jack_x_query(x)?;
        let coarse = self.fodder_behavior == FodderBehavior::Biteable
            && self.fodder_morph.is_none()
            && self.fodder_invulnerable_for == 0
            && self.domain.plant_candidates.clone().count() > usize::try_from(BITE_PERIOD).unwrap_or_default();
        if !coarse {
            return Ok((left, right));
        }
        let position = right.map_or(left as f64, |(_right, weight)| left as f64 + weight);
        let lower = ((position / 2.0).floor() as usize)
            .saturating_mul(2)
            .min(tables.jack_x_count().saturating_sub(1));
        let upper = lower.saturating_add(2).min(tables.jack_x_count().saturating_sub(1));
        if lower == upper {
            Ok((lower, None))
        } else {
            Ok((lower, Some((upper, (position - lower as f64) / (upper - lower) as f64))))
        }
    }

    pub(super) fn branch_fodder_end(
        &self, plant_at: i32, remove_at: Option<i32>, contacts: &[Option<(i32, f32)>], excluded_threat: Option<usize>,
    ) -> DeterministicFodderEnd {
        deterministic_fodder_end(
            self.fodder_hp,
            self.fodder_behavior,
            plant_at.saturating_add(self.fodder_invulnerable_for),
            self.fodder_morph.map(|morph| {
                (
                    plant_at.saturating_add(morph.after_plant),
                    morph.hp,
                    morph.behavior,
                    morph.invulnerable_for,
                )
            }),
            remove_at,
            self.domain.activation_at,
            contacts
                .iter()
                .enumerate()
                .filter(|(index, _contact)| Some(*index) != excluded_threat)
                .filter(|(index, _contact)| !self.threats[*index].pole_vault)
                .filter_map(|(index, contact)| {
                    contact.map(|contact| {
                        (
                            contact.0,
                            self.threats[index].fodder_contact_kind,
                            self.threats[index].bite_check_residue,
                            self.threats[index].update_rank,
                        )
                    })
                }),
        )
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the exact sparse score keeps branch state allocation-free"
)]
pub(super) fn aggregate_ordinary_damage(
    release_values: &ReleaseValueWorkspace, branch: &AggregateBranchProbabilityView<'_, '_, '_, '_>,
    query: ReleaseQuery, threat: &SmartFodderThreat, baseline_damage: f64, contact_at: i32,
    deterministic_end: DeterministicFodderEnd, activation_at: i32, direct_corrections: Option<&DirectCorrectionSeries>,
) -> Result<f64, SmartFodderSolveError> {
    let residue = threat
        .bite_check_residue
        .ok_or(SmartFodderSolveError::UnsupportedReleaseThreat)?;
    let bite_order = zombie_bite_check_order(threat.update_rank);
    debug_assert!(direct_corrections.is_none_or(|corrections| {
        corrections.contact_at == contact_at && corrections.bite_check_residue == residue
    }));
    let release_at = |death_at| {
        interpolated_release_value(release_values, query, residue, death_at, activation_at)
            + direct_corrections.map_or(0.0, |corrections| 300.0 * corrections.correction_at(death_at))
    };
    Ok(aggregate_weighted_release_damage(
        branch,
        baseline_damage,
        contact_at,
        residue,
        bite_order,
        deterministic_end.time,
        release_at,
    ))
}

pub(super) fn aggregate_weighted_release_damage(
    branch: &AggregateBranchProbabilityView<'_, '_, '_, '_>, baseline_damage: f64, contact_at: i32, residue: i32,
    bite_order: i32, end: i32, release_at: impl Fn(i32) -> f64,
) -> f64 {
    if end < contact_at {
        return branch.branch_mass() * baseline_damage;
    }
    let mut surviving = branch.survival_after(contact_at.saturating_sub(1));
    let mut damage = (branch.branch_mass() - surviving).max(0.0) * baseline_damage;
    let mut bucket_start = contact_at;
    while bucket_start < end {
        let check_at = align_bite_check(bucket_start, residue);
        let bucket_end = end.min(check_at.saturating_add(1));
        let base = release_at(bucket_start);
        let check = (bucket_end == check_at.saturating_add(1))
            .then(|| branch.probability_at_ordered_and_survival_after(bite_order, check_at));
        let surviving_at_end = check.map_or_else(
            || branch.survival_after(bucket_end.saturating_sub(1)),
            |(_check_mass, _after_mass, after_survival)| after_survival,
        );
        let total_mass = (surviving - surviving_at_end).max(0.0);
        damage += total_mass * base;
        if let Some((check_mass, after_mass, _after_survival)) = check.filter(|_| total_mass > 0.0) {
            damage += after_mass * (release_at(check_at.saturating_add(1)) - base);
            if check_at == contact_at {
                damage += (check_mass - after_mass) * (baseline_damage - base);
            }
        }
        surviving = surviving_at_end;
        bucket_start = bucket_end;
    }
    damage += surviving * release_at(end);
    damage
}

#[allow(
    clippy::too_many_arguments,
    reason = "the terminal tie correction consumes one exact aggregate branch group"
)]
pub(super) fn aggregate_ordinary_order_correction(
    release_values: &ReleaseValueWorkspace, branch: &AggregateBranchProbabilityView<'_, '_, '_, '_>,
    query: ReleaseQuery, threat: &SmartFodderThreat, baseline_damage: f64, contact_at: i32,
    deterministic_end: DeterministicFodderEnd, activation_at: i32, direct_corrections: Option<&DirectCorrectionSeries>,
) -> Result<f64, SmartFodderSolveError> {
    let residue = threat
        .bite_check_residue
        .ok_or(SmartFodderSolveError::UnsupportedReleaseThreat)?;
    let bite_order = zombie_bite_check_order(threat.update_rank);
    debug_assert!(direct_corrections.is_none_or(|corrections| {
        corrections.contact_at == contact_at && corrections.bite_check_residue == residue
    }));
    let release_at = |death_at| {
        interpolated_release_value(release_values, query, residue, death_at, activation_at)
            + direct_corrections.map_or(0.0, |corrections| 300.0 * corrections.correction_at(death_at))
    };
    let delayed_release_at = |death_at: i32| {
        interpolated_release_value(
            release_values,
            query,
            residue,
            death_at.saturating_add(1),
            activation_at,
        ) + direct_corrections.map_or(0.0, |corrections| 300.0 * corrections.delayed_correction_at(death_at))
    };
    Ok(deterministic_order_correction(
        |time| branch.first_probability_at_or_after_order(bite_order, time),
        |time| branch.survival_after(time),
        contact_at,
        bite_order,
        deterministic_end,
        release_at,
        delayed_release_at,
        baseline_damage,
        activation_at,
    ))
}

#[cfg(test)]
#[allow(
    clippy::too_many_arguments,
    reason = "the exact sparse score keeps branch state allocation-free"
)]
pub(super) fn ordinary_damage(
    release_values: &ReleaseValueWorkspace, branch: &BranchProbabilityView<'_, '_, '_>, query: ReleaseQuery,
    threat: &SmartFodderThreat, contact_at: i32, deterministic_end: DeterministicFodderEnd, activation_at: i32,
) -> Result<f64, SmartFodderSolveError> {
    if deterministic_end.time < contact_at {
        return Ok(branch.branch_mass() * threat.baseline_damage);
    }
    let residue = threat
        .bite_check_residue
        .ok_or(SmartFodderSolveError::UnsupportedReleaseThreat)?;
    let bite_order = zombie_bite_check_order(threat.update_rank);
    let release_at = |death_at| interpolated_release_value(release_values, query, residue, death_at, activation_at);
    let mut surviving = branch.survival_after(contact_at.saturating_sub(1));
    let mut damage = (branch.branch_mass() - surviving).max(0.0) * threat.baseline_damage;
    let mut bucket_start = contact_at;
    while bucket_start < deterministic_end.time {
        let check_at = align_bite_check(bucket_start, residue);
        let bucket_end = deterministic_end.time.min(check_at.saturating_add(1));
        let base = release_at(bucket_start);
        let check = (bucket_end == check_at.saturating_add(1))
            .then(|| branch.probability_at_ordered_and_survival_after(bite_order, check_at));
        let surviving_at_end = check.map_or_else(
            || branch.survival_after(bucket_end.saturating_sub(1)),
            |(_check_mass, _after_mass, after_survival)| after_survival,
        );
        let total_mass = (surviving - surviving_at_end).max(0.0);
        damage += total_mass * base;
        if let Some((check_mass, after_mass, _after_survival)) = check.filter(|_| total_mass > 0.0) {
            damage += after_mass * (release_at(check_at.saturating_add(1)) - base);
            if check_at == contact_at {
                damage += (check_mass - after_mass) * (threat.baseline_damage - base);
            }
        }
        surviving = surviving_at_end;
        bucket_start = bucket_end;
    }
    let end = deterministic_end.time;
    damage += surviving * release_at(end);
    damage += deterministic_order_correction(
        |time| branch.first_probability_at_or_after_order(bite_order, time),
        |time| branch.survival_after(time),
        contact_at,
        bite_order,
        deterministic_end,
        release_at,
        |time| release_at(time.saturating_add(1)),
        threat.baseline_damage,
        activation_at,
    );
    Ok(damage)
}

pub(super) fn interpolated_release_value(
    release_values: &ReleaseValueWorkspace, query: ReleaseQuery, residue: i32, death_at: i32, activation_at: i32,
) -> f64 {
    let release_at = align_bite_check(death_at, residue);
    if release_at >= activation_at {
        return 0.0;
    }
    let Some(left) = query.left_grid else {
        return 0.0;
    };
    let value = release_values.value_at(left, release_at);
    query.right_grid.map_or(value, |(right, weight)| {
        (release_values.value_at(right, release_at) - value).mul_add(weight, value)
    })
}

pub(super) fn aggregate_fodder_presence_at_actor(
    branch: &AggregateBranchProbabilityView<'_, '_, '_, '_>, deterministic_end: DeterministicFodderEnd, time: i32,
    frame_order: i32,
) -> f64 {
    if deterministic_end.time < time
        || (deterministic_end.time == time
            && deterministic_end
                .frame_order
                .is_some_and(|end_order| end_order < frame_order))
    {
        return 0.0;
    }
    (branch.first_probability_at_or_after_order(frame_order, time) + branch.survival_after(time))
        .clamp(0.0, branch.survival_after(time.saturating_sub(1)))
}

impl ReleaseValueWorkspace {
    pub(super) fn find(
        &self, kind: ReleaseTailKind, x_index: usize, bite_check_residue: i32, cutoff_threat: Option<usize>,
    ) -> Option<usize> {
        self.grids.iter().position(|grid| {
            grid.kind == kind
                && grid.x_index == x_index
                && grid.bite_check_residue == bite_check_residue
                && grid.cutoff_threat == cutoff_threat
        })
    }

    pub(super) fn jack_baseline_before(&self, threat: usize, time: i32) -> f64 {
        self.jack_baselines
            .get(threat)
            .and_then(Option::as_ref)
            .map_or(0.0, |baseline| baseline.before(time))
    }

    pub(super) fn jack_baseline_at_or_after(&self, threat: usize, time: i32) -> f64 {
        self.jack_baselines
            .get(threat)
            .and_then(Option::as_ref)
            .map_or(0.0, |baseline| baseline.at_or_after(time))
    }

    pub(super) fn value_at(&self, grid: usize, release_at: i32) -> f64 {
        let Some(grid) = self.grids.get(grid) else {
            return 0.0;
        };
        release_at
            .checked_sub(grid.first_release)
            .filter(|offset| *offset >= 0 && offset.rem_euclid(BITE_PERIOD) == 0)
            .and_then(|offset| usize::try_from(offset / BITE_PERIOD).ok())
            .and_then(|index| grid.released.get(index))
            .copied()
            .unwrap_or(0.0)
    }
}

impl AbsoluteJackOrdinaryBaseline {
    pub(super) fn before(&self, time: i32) -> f64 {
        let count = usize::try_from(time.saturating_sub(self.start))
            .unwrap_or(usize::MAX)
            .min(self.prefix.len().saturating_sub(1));
        self.prefix.get(count).copied().unwrap_or(0.0)
    }

    pub(super) fn at_or_after(&self, time: i32) -> f64 {
        self.prefix.last().copied().unwrap_or(0.0) - self.before(time)
    }
}

impl AbsoluteJackBlastAxes {
    pub(super) fn baseline_before(&self, time: i32) -> f64 {
        let count = usize::try_from(time.saturating_sub(self.baseline_start))
            .unwrap_or(usize::MAX)
            .min(self.baseline_prefix.len().saturating_sub(1));
        self.baseline_prefix.get(count).copied().unwrap_or(0.0)
    }

    pub(super) fn baseline_at_or_after(&self, time: i32) -> f64 {
        self.baseline_prefix.last().copied().unwrap_or(0.0) - self.baseline_before(time)
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the deterministic tie correction keeps value access allocation-free"
)]
pub(super) fn deterministic_order_correction(
    first_probability_at_or_after_check: impl Fn(i32) -> f64, survival_after: impl Fn(i32) -> f64, contact_at: i32,
    bite_frame_order: i32, deterministic_end: DeterministicFodderEnd, base_value_at: impl Fn(i32) -> f64,
    delayed_value_at: impl Fn(i32) -> f64, baseline_value: f64, activation_at: i32,
) -> f64 {
    let end = deterministic_end.time;
    let Some(end_order) = deterministic_end.frame_order else {
        return 0.0;
    };
    let residue = contact_at.rem_euclid(BITE_PERIOD);
    if end < contact_at || end >= activation_at || end.rem_euclid(BITE_PERIOD) != residue {
        return 0.0;
    }
    let reached_end = survival_after(end.saturating_sub(1));
    let after_mass = if end_order >= bite_frame_order {
        first_probability_at_or_after_check(end) + survival_after(end)
    } else {
        0.0
    }
    .clamp(0.0, reached_end);
    let base = base_value_at(end);
    let mut correction = after_mass * (delayed_value_at(end) - base);
    if end == contact_at {
        correction += (reached_end - after_mass) * (baseline_value - base);
    }
    correction
}

impl JackDirectConvolution {
    pub(super) fn correction(
        &self, contact_at: i32, death_at: i32, release_at: i32, x_query: (usize, Option<(usize, f64)>),
    ) -> f64 {
        if death_at < contact_at {
            return 0.0;
        }
        let index = |time: i32| {
            usize::try_from(time.saturating_sub(self.axis_start))
                .unwrap_or(usize::MAX)
                .min(self.time_count.saturating_sub(1))
        };
        let contact = index(contact_at);
        let release = index(release_at);
        let at_grid = |x_index: usize| {
            let danger_at_zero = self.danger_at_zero.get(x_index).copied().unwrap_or(0.0);
            let trapped_probability = self.pop_prefix[release] - self.pop_prefix[contact];
            let future = self
                .future_hits
                .get(x_index)
                .and_then(Option::as_ref)
                .and_then(|row| row.get(release))
                .copied()
                .unwrap_or(0.0);
            danger_at_zero * trapped_probability + future - self.baseline_suffix[contact]
        };
        let left = at_grid(x_query.0);
        x_query
            .1
            .map_or(left, |(right, weight)| (at_grid(right) - left).mul_add(weight, left))
    }
}

impl DirectCorrectionSeries {
    pub(super) fn release_index(&self, release_at: i32) -> Option<usize> {
        let offset = release_at.checked_sub(self.first_release)?;
        (offset >= 0 && offset.rem_euclid(BITE_PERIOD) == 0)
            .then(|| usize::try_from(offset / BITE_PERIOD).ok())
            .flatten()
    }

    pub(super) fn correction_for_release(&self, release_at: i32) -> f64 {
        self.release_index(release_at)
            .and_then(|index| self.correction_by_release.get(index))
            .copied()
            .unwrap_or(0.0)
    }

    pub(super) fn correction_at(&self, death_at: i32) -> f64 {
        if death_at < self.contact_at {
            return 0.0;
        }
        self.correction_for_release(align_bite_check(death_at, self.bite_check_residue))
    }

    pub(super) fn delayed_correction_at(&self, death_at: i32) -> f64 {
        if death_at < self.contact_at {
            return 0.0;
        }
        self.correction_for_release(align_bite_check(death_at.saturating_add(1), self.bite_check_residue))
    }
}

pub(super) fn direct_bucket_contribution(
    corrections: &DirectCorrectionSeries, release_at: i32, start: i32, end: i32, total_mass: f64,
    check: Option<(f64, f64)>,
) -> f64 {
    if end <= start {
        return 0.0;
    }
    let base = corrections.correction_for_release(release_at);
    let mut weighted = total_mass * base;
    if let Some((check_mass, after_mass)) = check {
        let delayed = corrections.delayed_correction_at(release_at);
        weighted += after_mass * (delayed - base);
        if release_at == corrections.contact_at {
            weighted -= (check_mass - after_mass) * base;
        }
    }
    weighted
}

pub(super) fn aggregate_direct_weighted_before(
    branch: &AggregateBranchProbabilityView<'_, '_, '_, '_>, corrections: &DirectCorrectionSeries, end: i32,
) -> f64 {
    let mut result = 0.0;
    let mut bucket_start = branch.start().max(corrections.contact_at);
    let mut surviving = branch.survival_after(bucket_start.saturating_sub(1));
    while bucket_start < end {
        let release_at = align_bite_check(bucket_start, corrections.bite_check_residue);
        let bucket_end = end.min(release_at.saturating_add(1));
        let check = (bucket_end == release_at.saturating_add(1))
            .then(|| branch.probability_at_ordered_and_survival_after_focus(release_at));
        let surviving_at_end = check.map_or_else(
            || branch.survival_after(bucket_end.saturating_sub(1)),
            |(_check_mass, _after_mass, after_survival)| after_survival,
        );
        let total_mass = (surviving - surviving_at_end).max(0.0);
        result += direct_bucket_contribution(
            corrections,
            release_at,
            bucket_start,
            bucket_end,
            total_mass,
            check.map(|(check_mass, after_mass, _after_survival)| (check_mass, after_mass)),
        );
        surviving = surviving_at_end;
        bucket_start = bucket_end;
    }
    result
}

pub(in crate::logic::smart_fodder) fn align_bite_check(time: i32, residue: i32) -> i32 {
    let offset = (i64::from(residue.rem_euclid(BITE_PERIOD)) - i64::from(time)).rem_euclid(i64::from(BITE_PERIOD));
    time.saturating_add(i32::try_from(offset).unwrap_or_default())
}

pub(super) fn bite_damage_until(contact_at: Option<i32>, end_at: i32) -> f64 {
    let Some(contact_at) = contact_at.filter(|contact_at| *contact_at < end_at) else {
        return 0.0;
    };
    let checks = (end_at - contact_at + BITE_PERIOD - 1) / BITE_PERIOD;
    f64::from(checks.saturating_mul(BITE_DAMAGE).min(300))
}

pub(super) fn mask_indices(mask: u64) -> impl Iterator<Item = usize> {
    (0..u64::BITS as usize).filter(move |index| mask & (1_u64 << index) != 0)
}

/// Returns `result[d] = sum_k>=d left[k] * right[k-d]` using one
/// zero-padded convolution. This avoids a per-death suffix dot-product and
/// keeps preprocessing at O(H log H) instead of O(H²).

pub(super) fn deterministic_fodder_end(
    hp: i32, behavior: FodderBehavior, initial_vulnerable_at: i32, morph: Option<(i32, i32, FodderBehavior, i32)>,
    remove_at: Option<i32>, activation_at: i32,
    contacts: impl Iterator<Item = (i32, FodderContactKind, Option<i32>, usize)> + Clone,
) -> DeterministicFodderEnd {
    let plant_at = initial_vulnerable_at;
    let deadline = remove_at.unwrap_or(activation_at).min(match behavior {
        FodderBehavior::Biteable => i32::MAX,
        FodderBehavior::Blover => plant_at.saturating_add(BLOVER_LIFETIME),
    });
    let first_reaching = |vulnerable_at: i32, start: i32, end: i32, required: i32| {
        let mut low = start;
        let mut high = end;
        while low < high {
            let middle = low + (high - low) / 2;
            if bite_damage_at(contacts.clone(), vulnerable_at, middle) >= required {
                high = middle;
            } else {
                low = middle + 1;
            }
        }
        low
    };
    let health_end = |vulnerable_at: i32, deadline: i32, hp: i32| {
        let Some(first_contact) = contacts
            .clone()
            .filter(|(_contact, kind, _residue, _rank)| *kind == FodderContactKind::Bite)
            .map(|(contact, _kind, residue, _rank)| bite_contact_at(contact, residue, vulnerable_at))
            .min()
        else {
            return deadline;
        };
        if bite_damage_at(contacts.clone(), vulnerable_at, deadline) < hp {
            deadline
        } else {
            first_reaching(vulnerable_at, first_contact, deadline, hp)
        }
    };
    let mut end = if let Some((morph_at, morphed_hp, morphed_behavior, morph_invulnerable_for)) =
        morph.filter(|(morph_at, _hp, _behavior, _invulnerable_for)| *morph_at <= deadline)
    {
        let before_morph_death = if behavior == FodderBehavior::Biteable {
            let before_morph = bite_damage_at(contacts.clone(), initial_vulnerable_at, morph_at.saturating_sub(1));
            (before_morph >= hp).then(|| {
                first_reaching(
                    initial_vulnerable_at,
                    initial_vulnerable_at,
                    morph_at.saturating_sub(1),
                    hp,
                )
            })
        } else {
            None
        };
        if let Some(death) = before_morph_death {
            death
        } else {
            match morphed_behavior {
                FodderBehavior::Biteable => {
                    let morph_vulnerable_at = morph_at.saturating_add(morph_invulnerable_for);
                    health_end(morph_vulnerable_at, deadline, morphed_hp)
                }
                FodderBehavior::Blover => deadline.min(morph_at.saturating_add(BLOVER_LIFETIME)),
            }
        }
    } else {
        match behavior {
            FodderBehavior::Biteable => health_end(initial_vulnerable_at, deadline, hp),
            FodderBehavior::Blover => deadline,
        }
    };
    for (contact, kind, _residue, _rank) in contacts.clone() {
        if contact > end {
            continue;
        }
        match kind {
            FodderContactKind::Bite => {}
            FodderContactKind::Crush => {
                let current_behavior = morph
                    .filter(|(morph_at, _hp, _behavior, _invulnerable_for)| *morph_at <= contact)
                    .map_or(behavior, |(_morph_at, _hp, behavior, _invulnerable_for)| behavior);
                if current_behavior == FodderBehavior::Biteable {
                    end = contact;
                }
            }
            FodderContactKind::Smash { impact_after } => {
                end = end.min(contact.saturating_add(impact_after));
            }
        }
    }
    let frame_order = deterministic_death_frame_order(
        end,
        hp,
        behavior,
        initial_vulnerable_at,
        morph,
        remove_at,
        activation_at,
        contacts,
    );
    DeterministicFodderEnd { time: end, frame_order }
}

pub(super) fn deterministic_death_frame_order(
    end: i32, hp: i32, behavior: FodderBehavior, initial_vulnerable_at: i32,
    morph: Option<(i32, i32, FodderBehavior, i32)>, remove_at: Option<i32>, activation_at: i32,
    contacts: impl Iterator<Item = (i32, FodderContactKind, Option<i32>, usize)> + Clone,
) -> Option<i32> {
    if end >= activation_at {
        return None;
    }
    if remove_at == Some(end) {
        return Some(BEFORE_ZOMBIE_UPDATES);
    }
    debug_assert!({
        let mut previous = None;
        contacts.clone().all(|(_contact, _kind, _residue, rank)| {
            let ordered = previous.is_none_or(|previous| previous < rank);
            previous = Some(rank);
            ordered
        })
    });
    let (stage_hp, stage_behavior, vulnerable_at, blover_expires_at) = morph
        .filter(|(morph_at, _hp, _behavior, _invulnerable_for)| *morph_at <= end)
        .map_or_else(
            || {
                (
                    hp,
                    behavior,
                    initial_vulnerable_at,
                    (behavior == FodderBehavior::Blover).then(|| initial_vulnerable_at.saturating_add(BLOVER_LIFETIME)),
                )
            },
            |(morph_at, hp, behavior, invulnerable_for)| {
                (
                    hp,
                    behavior,
                    morph_at.saturating_add(invulnerable_for),
                    (behavior == FodderBehavior::Blover).then(|| morph_at.saturating_add(BLOVER_LIFETIME)),
                )
            },
        );
    if blover_expires_at == Some(end) {
        return Some(BEFORE_ZOMBIE_UPDATES);
    }
    let damage_before = contacts
        .clone()
        .filter(|(_contact, kind, _residue, _rank)| *kind == FodderContactKind::Bite)
        .map(|(contact, _kind, residue, _rank)| bite_contact_at(contact, residue, vulnerable_at))
        .filter(|contact| *contact < end)
        .map(|contact| (end.saturating_sub(1) - contact) / BITE_PERIOD + 1)
        .sum::<i32>()
        .saturating_mul(BITE_DAMAGE);
    let mut remaining_hp = stage_hp.saturating_sub(damage_before);
    for (contact, kind, residue, rank) in contacts {
        match kind {
            FodderContactKind::Bite if stage_behavior == FodderBehavior::Biteable => {
                let first = bite_contact_at(contact, residue, vulnerable_at);
                if first <= end && (end - first).rem_euclid(BITE_PERIOD) == 0 {
                    remaining_hp = remaining_hp.saturating_sub(BITE_DAMAGE);
                    if remaining_hp <= 0 {
                        return Some(zombie_bite_check_order(rank));
                    }
                }
            }
            FodderContactKind::Crush if stage_behavior == FodderBehavior::Biteable && contact == end => {
                return Some(zombie_before_check_order(rank));
            }
            FodderContactKind::Smash { impact_after } if contact.saturating_add(impact_after) == end => {
                return Some(zombie_before_check_order(rank));
            }
            _ => {}
        }
    }
    None
}

pub(super) fn bite_contact_at(contact: i32, residue: Option<i32>, vulnerable_at: i32) -> i32 {
    residue.map_or_else(
        || contact.max(vulnerable_at),
        |residue| align_bite_check(contact.max(vulnerable_at), residue),
    )
}

pub(super) fn bite_damage_at(
    contacts: impl Iterator<Item = (i32, FodderContactKind, Option<i32>, usize)>, vulnerable_at: i32, time: i32,
) -> i32 {
    contacts
        .filter(|(_contact, kind, _residue, _rank)| *kind == FodderContactKind::Bite)
        .map(|(contact, _kind, residue, _rank)| bite_contact_at(contact, residue, vulnerable_at))
        .filter(|contact| *contact <= time)
        .map(|contact| (time - contact) / BITE_PERIOD + 1)
        .sum::<i32>()
        .saturating_mul(BITE_DAMAGE)
}

pub(super) fn zombie_before_check_order(update_rank: usize) -> i32 {
    // Explosion/crush/smash happens during this zombie's update before its
    // ordinary eating check. Consecutive pairs preserve object-pool order.
    i32::try_from(update_rank).unwrap_or(i32::MAX / 2).saturating_mul(2)
}

pub(super) fn zombie_bite_check_order(update_rank: usize) -> i32 {
    zombie_before_check_order(update_rank).saturating_add(1)
}

pub(super) fn better(choice: SmartFodderChoice, best: Option<SmartFodderChoice>) -> bool {
    let Some(best) = best else {
        return true;
    };
    if choice.expected_damage != best.expected_damage {
        return choice.expected_damage < best.expected_damage;
    }
    choice.plant_at > best.plant_at
        || (choice.plant_at == best.plant_at
            && choice.remove_at.unwrap_or(i32::MAX) > best.remove_at.unwrap_or(i32::MAX))
}
