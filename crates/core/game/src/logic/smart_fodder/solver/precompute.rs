use super::*;

impl SmartFodderModel {
    pub(super) fn prepare_release_queries_into(
        &self, tables: DamageTables<'_>, release_values: &ReleaseValueWorkspace, contacts: &[Option<(i32, f32)>],
        output: &mut Vec<Option<ReleaseQuery>>,
    ) -> Result<(), SmartFodderSolveError> {
        output.clear();
        for (threat_index, (threat, contact)) in self.threats.iter().zip(contacts).enumerate() {
            if threat.pole_vault || !threat.ordinary_damage_enabled || contact.is_none() {
                output.push(None);
                continue;
            }
            let Some(kind) = threat.release_kind else {
                // Ordinary slow zombies released from C9 cannot reach C8
                // before A in the v1 domain. Their measured tail is 0.
                output.push(Some(ReleaseQuery {
                    left_grid: None,
                    right_grid: None,
                }));
                continue;
            };
            let residue = threat
                .bite_check_residue
                .ok_or(SmartFodderSolveError::UnsupportedReleaseThreat)?;
            let (_contact_at, contact_x) = contact.expect("checked above");
            let (left_x, right_x) = tables.release_x_query(kind, contact_x)?;
            let cutoff_threat = release_values.cutoff_keys[threat_index];
            let find = |x_index| {
                release_values
                    .find(kind, x_index, residue, cutoff_threat)
                    .ok_or(SmartFodderSolveError::UnsupportedReleaseThreat)
            };
            output.push(Some(ReleaseQuery {
                left_grid: Some(find(left_x)?),
                right_grid: right_x
                    .map(|(right_x, weight)| find(right_x).map(|grid| (grid, weight)))
                    .transpose()?,
            }));
        }
        Ok(())
    }

    pub(super) fn direct_x_masks_for(
        &self, tables: DamageTables<'_>, plant_candidates: &[i32],
    ) -> Result<Vec<u64>, SmartFodderSolveError> {
        debug_assert!(tables.jack_x_count() <= u64::BITS as usize);
        let mut masks = vec![0_u64; self.threats.len()];
        for plant_at in plant_candidates.iter().copied() {
            for (index, threat) in self.threats.iter().enumerate() {
                if threat.jack_pmf.is_none() || threat.jack_geometry_counts.iter().all(|count| *count == 0) {
                    continue;
                }
                let Some((_contact_at, contact_x)) = threat.contact(plant_at, self.domain.activation_at) else {
                    continue;
                };
                let (left, right) = self.direct_x_query(tables, contact_x)?;
                masks[index] |= 1_u64 << left;
                if let Some((right, _weight)) = right {
                    masks[index] |= 1_u64 << right;
                }
            }
        }
        Ok(masks)
    }

    pub(super) fn prepare_release_values_for(
        &self, tables: DamageTables<'_>, plant_candidates: &[i32],
    ) -> Result<ReleaseValueWorkspace, SmartFodderSolveError> {
        let cutoff_keys = self
            .threats
            .iter()
            .enumerate()
            .map(|(threat_index, threat)| {
                threat.jack_pmf.as_ref().map(|pmf| {
                    self.threats[..=threat_index]
                        .iter()
                        .position(|candidate| candidate.jack_pmf.as_ref() == Some(pmf))
                        .unwrap_or(threat_index)
                })
            })
            .collect::<Vec<_>>();
        let mut keys = Vec::new();
        for plant_at in plant_candidates.iter().copied() {
            for (threat_index, threat) in self.threats.iter().enumerate() {
                let (Some(kind), Some(residue), Some((_contact_at, contact_x))) = (
                    threat.release_kind,
                    threat.bite_check_residue,
                    threat.contact(plant_at, self.domain.activation_at),
                ) else {
                    continue;
                };
                let (left, right) = tables.release_x_query(kind, contact_x)?;
                let cutoff_threat = cutoff_keys[threat_index];
                for x_index in std::iter::once(left).chain(right.map(|(right, _weight)| right)) {
                    if !keys.contains(&(kind, x_index, residue, cutoff_threat)) {
                        keys.push((kind, x_index, residue, cutoff_threat));
                    }
                }
            }
        }
        let axis_start = *self.domain.plant_candidates.start();
        let time_count = usize::try_from(self.domain.activation_at.saturating_sub(axis_start))
            .unwrap_or_default()
            .saturating_add(1);
        let mut jack_probabilities = std::iter::repeat_with(|| None)
            .take(self.threats.len())
            .collect::<Vec<_>>();
        let mut jack_baselines = std::iter::repeat_with(|| None)
            .take(self.threats.len())
            .collect::<Vec<_>>();
        for (threat_index, threat) in self.threats.iter().enumerate() {
            let Some(pmf) = &threat.jack_pmf else {
                continue;
            };
            let mut explosion_probability = vec![0.0; time_count];
            for explosion_at in pmf.start()..=pmf.end() {
                let probability = pmf.probability_at(explosion_at);
                if probability <= 0.0 || explosion_at < axis_start {
                    continue;
                }
                let index = usize::try_from(explosion_at.min(self.domain.activation_at).saturating_sub(axis_start))
                    .unwrap_or(usize::MAX)
                    .min(time_count.saturating_sub(1));
                explosion_probability[index] += probability;
            }
            if cutoff_keys[threat_index] == Some(threat_index) {
                jack_probabilities[threat_index] = Some(explosion_probability);
            }

            let baseline_start = pmf.start().min(axis_start);
            let baseline_count = usize::try_from(self.domain.activation_at.saturating_sub(baseline_start))
                .unwrap_or_default()
                .saturating_add(1);
            let mut baseline = vec![0.0; baseline_count];
            for explosion_at in pmf.start()..=pmf.end() {
                let probability = pmf.probability_at(explosion_at);
                if probability <= 0.0 {
                    continue;
                }
                let end = explosion_at.min(self.domain.activation_at);
                let index = usize::try_from(end.saturating_sub(baseline_start))
                    .unwrap_or(usize::MAX)
                    .min(baseline.len().saturating_sub(1));
                baseline[index] += probability * bite_damage_until(threat.baseline_contact_at, end);
            }
            let mut prefix = Vec::with_capacity(baseline.len() + 1);
            prefix.push(0.0);
            for value in baseline {
                prefix.push(prefix.last().copied().unwrap_or(0.0) + value);
            }
            jack_baselines[threat_index] = Some(AbsoluteJackOrdinaryBaseline {
                start: baseline_start,
                prefix,
            });
        }
        let mut damage_axes = Vec::<((ReleaseTailKind, usize), Vec<f64>)>::new();
        for (kind, x_index, _residue, cutoff_threat) in &keys {
            if cutoff_threat.is_none()
                || damage_axes
                    .iter()
                    .any(|((cached_kind, cached_x), _)| cached_kind == kind && cached_x == x_index)
            {
                continue;
            }
            let damage = (0..time_count)
                .map(|horizon| tables.release_damage_grid(*kind, *x_index, horizon).map(f64::from))
                .collect::<Result<Vec<_>, _>>()?;
            damage_axes.push(((*kind, *x_index), damage));
        }
        let grids = keys
            .into_iter()
            .map(|(kind, x_index, bite_check_residue, cutoff_threat)| {
                let first_release = align_bite_check(axis_start, bite_check_residue);
                let released = if let Some(threat_index) = cutoff_threat {
                    let damage = damage_axes
                        .iter()
                        .find(|((cached_kind, cached_x), _)| *cached_kind == kind && *cached_x == x_index)
                        .map(|(_key, damage)| damage)
                        .ok_or(SmartFodderSolveError::UnsupportedReleaseThreat)?;
                    let explosion = jack_probabilities[threat_index]
                        .as_ref()
                        .ok_or(SmartFodderSolveError::InvalidJackBranch)?;
                    (first_release..self.domain.activation_at)
                        .step_by(usize::try_from(BITE_PERIOD).unwrap_or(1))
                        .map(|release_at| {
                            let index = usize::try_from(release_at.saturating_sub(axis_start)).unwrap_or(usize::MAX);
                            explosion.get(index..).map_or(0.0, |future| {
                                future
                                    .iter()
                                    .zip(damage)
                                    .map(|(probability, damage)| probability * damage)
                                    .sum()
                            })
                        })
                        .collect()
                } else {
                    (first_release..self.domain.activation_at)
                        .step_by(usize::try_from(BITE_PERIOD).unwrap_or(1))
                        .map(|release_at| {
                            let horizon = usize::try_from(self.domain.activation_at.saturating_sub(release_at))
                                .unwrap_or_default();
                            tables.release_damage_grid(kind, x_index, horizon).map(f64::from)
                        })
                        .collect::<Result<Vec<_>, _>>()?
                };
                Ok(AbsoluteReleaseGrid {
                    kind,
                    x_index,
                    bite_check_residue,
                    cutoff_threat,
                    first_release,
                    released,
                })
            })
            .collect::<Result<Vec<_>, SmartFodderSolveError>>()?;
        Ok(ReleaseValueWorkspace {
            grids,
            jack_baselines,
            cutoff_keys,
        })
    }

    pub(super) fn prepare_jack_blast_axes(&self) -> Result<Vec<AbsoluteJackBlastAxes>, SmartFodderSolveError> {
        let axis_start = *self.domain.plant_candidates.start();
        let event_count = usize::try_from(self.domain.activation_at.saturating_sub(axis_start)).unwrap_or_default();
        self.threats
            .iter()
            .filter(|threat| threat.jack_pmf.is_some())
            .map(|threat| {
                let mut always_hits = vec![0.0; event_count.max(1)];
                let mut blockable_hits = vec![0.0; event_count.max(1)];
                let baseline_start = threat
                    .jack_blast_events
                    .iter()
                    .map(|event| event.explosion_at)
                    .min()
                    .unwrap_or(axis_start)
                    .min(axis_start);
                let baseline_end = threat
                    .jack_blast_events
                    .iter()
                    .map(|event| event.explosion_at)
                    .max()
                    .unwrap_or(baseline_start);
                let baseline_count = usize::try_from(baseline_end.saturating_sub(baseline_start).saturating_add(1))
                    .unwrap_or_default()
                    .max(1);
                let mut baseline = vec![0.0; baseline_count];
                for event in &threat.jack_blast_events {
                    if let Some(index) = event
                        .explosion_at
                        .checked_sub(baseline_start)
                        .and_then(|offset| usize::try_from(offset).ok())
                        .filter(|index| *index < baseline.len())
                    {
                        baseline[index] += event.probability * event.baseline_hits;
                    }
                    let Some(index) = event
                        .explosion_at
                        .checked_sub(axis_start)
                        .and_then(|offset| usize::try_from(offset).ok())
                        .filter(|index| *index < event_count)
                    else {
                        continue;
                    };
                    if event.hits_fodder_without_block {
                        always_hits[index] += event.probability;
                    } else {
                        blockable_hits[index] += event.probability;
                    }
                }
                let mut baseline_prefix = Vec::with_capacity(baseline.len() + 1);
                baseline_prefix.push(0.0);
                for value in baseline {
                    baseline_prefix.push(baseline_prefix.last().copied().unwrap_or(0.0) + value);
                }
                Ok(AbsoluteJackBlastAxes {
                    always_hits: ExplosionPmf::from_subprobabilities(axis_start, always_hits)
                        .ok_or(SmartFodderSolveError::InvalidJackBranch)?,
                    blockable_hits: ExplosionPmf::from_subprobabilities(axis_start, blockable_hits)
                        .ok_or(SmartFodderSolveError::InvalidJackBranch)?,
                    baseline_start,
                    baseline_prefix,
                })
            })
            .collect()
    }

    pub(super) fn prepare_direct_convolutions(
        &self, tables: DamageTables<'_>, x_masks: &[u64],
    ) -> Result<Vec<Option<JackDirectConvolution>>, SmartFodderSolveError> {
        let geometries = [
            JackGeometry::SameRow,
            JackGeometry::UpperRow100,
            JackGeometry::LowerRow100,
            JackGeometry::UpperRow85,
            JackGeometry::LowerRow85,
        ];
        let axis_start = *self.domain.plant_candidates.start();
        let time_count = usize::try_from(self.domain.activation_at.saturating_sub(axis_start))
            .unwrap_or_default()
            .saturating_add(1);
        let mut result = std::iter::repeat_with(|| None)
            .take(self.threats.len())
            .collect::<Vec<_>>();
        if self.threats.iter().enumerate().all(|(index, threat)| {
            x_masks.get(index).copied().unwrap_or(0) == 0
                || threat.jack_pmf.is_none()
                || threat.jack_geometry_counts.iter().all(|count| *count == 0)
        }) {
            return Ok(result);
        }
        let fft_len = (time_count.saturating_mul(2).saturating_sub(1)).next_power_of_two();
        let fft_plan = FftPlan::new(fft_len);
        let x_count = tables.jack_x_count();
        let mut grouped = vec![false; self.threats.len()];
        let mut work_real = vec![0.0; fft_len];
        let mut work_imag = vec![0.0; fft_len];
        for root in 0..self.threats.len() {
            let root_threat = &self.threats[root];
            if grouped[root]
                || x_masks.get(root).copied().unwrap_or(0) == 0
                || root_threat.jack_pmf.is_none()
                || root_threat.jack_geometry_counts.iter().all(|count| *count == 0)
            {
                continue;
            }
            let in_group = |index: usize, grouped: &[bool]| {
                !grouped[index]
                    && x_masks.get(index).copied().unwrap_or(0) != 0
                    && self.threats[index].jack_pmf.is_some()
                    && self.threats[index].jack_geometry_counts == root_threat.jack_geometry_counts
            };
            let union_mask = (root..self.threats.len())
                .filter(|index| in_group(*index, &grouped))
                .fold(0_u64, |mask, index| mask | x_masks.get(index).copied().unwrap_or(0));
            let mut danger_at_zero = vec![0.0; x_count];
            let mut danger_spectra = std::iter::repeat_with(|| None).take(x_count).collect::<Vec<_>>();
            for x_index in mask_indices(union_mask) {
                let danger = (0..time_count)
                    .map(|after_release| {
                        geometries
                            .iter()
                            .zip(root_threat.jack_geometry_counts)
                            .filter(|(_geometry, count)| *count != 0)
                            .map(|(geometry, count)| {
                                tables
                                    .jack_danger_grid(*geometry, x_index, after_release)
                                    .map(|probability| f64::from(count) * f64::from(probability))
                            })
                            .sum::<Result<f64, _>>()
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                danger_at_zero[x_index] = danger[0];
                danger_spectra[x_index] = Some(fft_spectrum(&danger, false, &fft_plan));
            }
            for threat_index in root..self.threats.len() {
                if !in_group(threat_index, &grouped) {
                    continue;
                }
                grouped[threat_index] = true;
                let threat = &self.threats[threat_index];
                let mut pop_probability = vec![0.0; time_count];
                let mut baseline_suffix = vec![0.0; time_count + 1];
                for event in &threat.jack_blast_events {
                    let popping_at = event.explosion_at.saturating_sub(110);
                    let Some(index) = popping_at
                        .checked_sub(axis_start)
                        .and_then(|offset| usize::try_from(offset).ok())
                        .filter(|index| *index < time_count)
                    else {
                        continue;
                    };
                    pop_probability[index] += event.probability;
                    baseline_suffix[index] += event.probability * event.baseline_hits;
                }
                let mut pop_prefix = vec![0.0; time_count + 1];
                for (index, probability) in pop_probability.iter().copied().enumerate() {
                    pop_prefix[index + 1] = pop_prefix[index] + probability;
                }
                for index in (0..time_count).rev() {
                    baseline_suffix[index] += baseline_suffix[index + 1];
                }
                let pop_spectrum = fft_spectrum(&pop_probability, true, &fft_plan);
                let mut future_hits = std::iter::repeat_with(|| None).take(x_count).collect::<Vec<_>>();
                for x_index in mask_indices(x_masks[threat_index]) {
                    let danger = danger_spectra[x_index]
                        .as_ref()
                        .ok_or(SmartFodderSolveError::InvalidJackBranch)?;
                    future_hits[x_index] = Some(correlate_spectra(
                        &pop_spectrum,
                        danger,
                        time_count,
                        &mut work_real,
                        &mut work_imag,
                        &fft_plan,
                    ));
                }
                result[threat_index] = Some(JackDirectConvolution {
                    axis_start,
                    time_count,
                    danger_at_zero: danger_at_zero.clone(),
                    future_hits,
                    pop_prefix,
                    baseline_suffix,
                });
            }
        }
        Ok(result)
    }

    pub(super) fn prepare_direct_corrections_into(
        &self, tables: DamageTables<'_>, plant_at: i32, end: i32, contacts: &[Option<(i32, f32)>],
        convolutions: &[Option<JackDirectConvolution>], output: &mut Vec<Option<DirectCorrectionSeries>>,
    ) -> Result<(), SmartFodderSolveError> {
        output.resize_with(self.threats.len(), || None);
        output.truncate(self.threats.len());
        for (index, ((threat, contact), convolution)) in self.threats.iter().zip(contacts).zip(convolutions).enumerate()
        {
            let previous = output[index].take();
            let (Some((contact_at, contact_x)), Some(convolution)) = (*contact, convolution) else {
                continue;
            };
            let bite_check_residue = threat
                .bite_check_residue
                .ok_or(SmartFodderSolveError::UnsupportedReleaseThreat)?;
            let x_query = self.direct_x_query(tables, contact_x)?;
            let first_release = align_bite_check(plant_at, bite_check_residue);
            let last_release = align_bite_check(end.saturating_add(1), bite_check_residue);
            let release_count = usize::try_from((last_release - first_release) / BITE_PERIOD)
                .unwrap_or_default()
                .saturating_add(1);
            if previous.as_ref().is_some_and(|series| {
                series.contact_at == contact_at
                    && series.bite_check_residue == bite_check_residue
                    && series.x_query == x_query
                    && series.first_release == first_release
                    && series.correction_by_release.len() == release_count
            }) {
                output[index] = previous;
                continue;
            }
            let mut correction_by_release = previous.map_or_else(Vec::new, |series| series.correction_by_release);
            correction_by_release.clear();
            correction_by_release.extend(
                (first_release..=last_release)
                    .step_by(usize::try_from(BITE_PERIOD).unwrap_or(1))
                    .map(|release_at| convolution.correction(contact_at, contact_at, release_at, x_query)),
            );
            output[index] = Some(DirectCorrectionSeries {
                contact_at,
                bite_frame_order: zombie_bite_check_order(threat.update_rank),
                bite_check_residue,
                x_query,
                first_release,
                correction_by_release,
            });
        }
        Ok(())
    }
}
