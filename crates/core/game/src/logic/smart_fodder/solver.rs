#[cfg(test)]
use super::critical_remove_candidates;
use super::domain::critical_remove_candidates_into;
use super::probability::{
    AggregateBranchProbabilityView, BranchExplosionWorkspace, BranchJackState, CandidateProbabilityCache,
    CandidateProbabilityStorage, CapOneBranchInput, SharedExplosionSource, SharedJackExplosionSource,
};
#[cfg(test)]
use super::probability::{BranchProbabilityView, FocusedBranchProbabilityView};
use super::{DamageTables, ExplosionPmf, JackGeometry, ReleaseTailKind, SmartFodderDomain, SmartFodderTableError};

const BITE_DAMAGE: i32 = 4;
const BITE_PERIOD: i32 = 8;
const BLOVER_LIFETIME: i32 = 250;
const POLE_VAULT_DURATION: i32 = 181;
const SLOWED_POLE_MIN_AFTER_LANDING: i32 = 212;
const BEFORE_ZOMBIE_UPDATES: i32 = -1;
const SCORE_EPSILON: f64 = 1.0e-9;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FodderBehavior {
    Biteable,
    Blover,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FodderContactKind {
    Bite,
    Crush,
    Smash { impact_after: i32 },
}

#[derive(Clone, Debug)]
pub struct SmartFodderThreat {
    pub update_rank: usize,
    pub trace_start: i32,
    pub x_trace: Vec<f32>,
    pub baseline_damage: f64,
    pub baseline_contact_at: Option<i32>,
    pub bite_check_residue: Option<i32>,
    pub ordinary_damage_enabled: bool,
    pub release_kind: Option<ReleaseTailKind>,
    pub jack_pmf: Option<ExplosionPmf>,
    pub pole_vault: bool,
    pub fodder_contact_kind: FodderContactKind,
    pub can_contact_fodder: bool,
    pub first_contact_at: Option<i32>,
    pub last_contact_at: Option<i32>,
    pub jack_blast_events: Vec<JackBlastEvent>,
    pub jack_geometry_counts: [u16; 5],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JackBlastEvent {
    pub explosion_at: i32,
    pub probability: f64,
    pub baseline_hits: f64,
    pub hits_fodder_without_block: bool,
}

impl SmartFodderThreat {
    fn x_at(&self, time: i32) -> Option<f32> {
        time.checked_sub(self.trace_start)
            .and_then(|offset| usize::try_from(offset).ok())
            .and_then(|offset| self.x_trace.get(offset))
            .copied()
    }

    fn contact(&self, plant_at: i32, activation_at: i32) -> Option<(i32, f32)> {
        if !self.can_contact_fodder {
            return None;
        }
        let first = self.first_contact_at?;
        let last = self.last_contact_at?;
        if plant_at > last {
            return None;
        }
        let contact_at = self.bite_check_residue.map_or_else(
            || plant_at.max(first),
            |residue| align_bite_check(plant_at.saturating_add(1).max(first), residue),
        );
        if contact_at > last {
            return None;
        }
        if contact_at >= activation_at {
            return None;
        }
        let x = self.x_at(contact_at)?;
        Some((contact_at, x))
    }
}

#[derive(Clone, Debug)]
pub struct SmartFodderModel {
    pub domain: SmartFodderDomain,
    pub fodder_hp: i32,
    pub fodder_behavior: FodderBehavior,
    pub fodder_morph: Option<FodderMorph>,
    pub fodder_invulnerable_for: i32,
    pub threats: Vec<SmartFodderThreat>,
    pub deterministic_explosions: Vec<OrderedExplosionPmf>,
    pub deterministic_jack_blast_damage: f64,
}

#[derive(Clone, Debug)]
pub struct OrderedExplosionPmf {
    pub pmf: ExplosionPmf,
    pub update_rank: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FodderMorph {
    pub after_plant: i32,
    pub hp: i32,
    pub behavior: FodderBehavior,
    pub invulnerable_for: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SmartFodderChoice {
    pub plant_at: i32,
    pub remove_at: Option<i32>,
    pub expected_damage: f64,
}

#[derive(Clone, Copy)]
struct ReleaseQuery {
    left_grid: Option<usize>,
    right_grid: Option<(usize, f64)>,
}

struct AbsoluteReleaseGrid {
    kind: ReleaseTailKind,
    x_index: usize,
    bite_check_residue: i32,
    cutoff_threat: Option<usize>,
    first_release: i32,
    released: Vec<f64>,
}

struct AbsoluteJackOrdinaryBaseline {
    start: i32,
    prefix: Vec<f64>,
}

struct AbsoluteJackBlastAxes {
    always_hits: ExplosionPmf,
    blockable_hits: ExplosionPmf,
    baseline_start: i32,
    baseline_prefix: Vec<f64>,
}

struct ReleaseValueWorkspace {
    grids: Vec<AbsoluteReleaseGrid>,
    jack_baselines: Vec<Option<AbsoluteJackOrdinaryBaseline>>,
    cutoff_keys: Vec<Option<usize>>,
}

struct DirectCorrectionSeries {
    contact_at: i32,
    bite_frame_order: i32,
    bite_check_residue: i32,
    x_query: (usize, Option<(usize, f64)>),
    first_release: i32,
    correction_by_release: Vec<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DeterministicFodderEnd {
    time: i32,
    frame_order: Option<i32>,
}

#[derive(Clone, Copy)]
struct BranchScoreState {
    input: CapOneBranchInput,
    excluded_threat: Option<usize>,
    end: DeterministicFodderEnd,
}

struct JackDirectConvolution {
    axis_start: i32,
    time_count: usize,
    danger_at_zero: Vec<f64>,
    future_hits: Vec<Option<Vec<f64>>>,
    pop_prefix: Vec<f64>,
    baseline_suffix: Vec<f64>,
}

struct FftSpectrum {
    real: Vec<f32>,
    imag: Vec<f32>,
}

struct FftPlan {
    len: usize,
    bit_reversed: Vec<u16>,
    twiddles: Vec<(f32, f32)>,
}

impl FftPlan {
    fn new(len: usize) -> Self {
        debug_assert!(len.is_power_of_two());
        debug_assert!(u16::try_from(len).is_ok());
        let mut bit_reversed = Vec::with_capacity(len);
        let mut reversed = 0;
        bit_reversed.push(0);
        for _index in 1..len {
            let mut bit = len >> 1;
            while reversed & bit != 0 {
                reversed ^= bit;
                bit >>= 1;
            }
            reversed ^= bit;
            bit_reversed.push(u16::try_from(reversed).unwrap_or_default());
        }
        let mut twiddles = Vec::with_capacity(len - 1);
        let mut width = 2;
        while width <= len {
            let angle = -2.0 * std::f32::consts::PI / width as f32;
            let (step_imag, step_real) = angle.sin_cos();
            let (mut real, mut imag) = (1.0, 0.0);
            for _ in 0..width / 2 {
                twiddles.push((real, imag));
                (real, imag) = (real * step_real - imag * step_imag, real * step_imag + imag * step_real);
            }
            width *= 2;
        }
        Self {
            len,
            bit_reversed,
            twiddles,
        }
    }

    fn len(&self) -> usize {
        self.len
    }
}

struct BranchEndGroup<K> {
    key: K,
    members_start: usize,
    members_end: usize,
}

struct BranchEndGrouping<K> {
    groups: Vec<BranchEndGroup<K>>,
    members: Vec<usize>,
}

impl<K> Default for BranchEndGrouping<K> {
    fn default() -> Self {
        Self {
            groups: Vec::new(),
            members: Vec::new(),
        }
    }
}

impl<K: Copy + Eq> BranchEndGrouping<K> {
    fn reserve(&mut self, state_count: usize) {
        self.groups.reserve(state_count);
        self.members.reserve(state_count);
    }

    fn rebuild(&mut self, states: &[BranchScoreState], key: impl Fn(&BranchScoreState) -> K) {
        self.groups.clear();
        self.members.clear();
        for state in states {
            let key = key(state);
            if self.groups.iter().all(|group| group.key != key) {
                self.groups.push(BranchEndGroup {
                    key,
                    members_start: 0,
                    members_end: 0,
                });
            }
        }
        for group in &mut self.groups {
            group.members_start = self.members.len();
            self.members.extend(
                states
                    .iter()
                    .enumerate()
                    .filter_map(|(index, state)| (key(state) == group.key).then_some(index)),
            );
            group.members_end = self.members.len();
        }
    }

    fn fill_terms(
        &self, group: &BranchEndGroup<K>, states: &[BranchScoreState], terms: &mut Vec<CapOneBranchInput>,
        include: impl Fn(&BranchScoreState) -> bool,
    ) {
        terms.clear();
        terms.extend(
            self.members[group.members_start..group.members_end]
                .iter()
                .filter_map(|index| include(&states[*index]).then_some(states[*index].input)),
        );
    }
}

#[derive(Default)]
struct CandidateScratch {
    contacts: Vec<Option<(i32, f32)>>,
    pole_boundaries: Vec<i32>,
    branch_latest_ends: Vec<i32>,
    direct_corrections: Vec<Option<DirectCorrectionSeries>>,
    release_queries: Vec<Option<ReleaseQuery>>,
    remove_candidates: Vec<Option<i32>>,
    scores: Vec<f64>,
    before: Vec<f64>,
    survivals: Vec<f64>,
    probability: CandidateProbabilityStorage,
    branch_inputs: Vec<CapOneBranchInput>,
    states: Vec<BranchScoreState>,
    time_groups: BranchEndGrouping<i32>,
    ordered_groups: BranchEndGrouping<DeterministicFodderEnd>,
    terms: Vec<CapOneBranchInput>,
}

impl SmartFodderModel {
    pub fn solve(&self, tables: DamageTables<'_>) -> Result<SmartFodderChoice, SmartFodderSolveError> {
        if (self.fodder_behavior == FodderBehavior::Biteable && self.fodder_hp <= 0)
            || self.fodder_morph.is_some_and(|morph| {
                morph.after_plant < 0 || (morph.behavior == FodderBehavior::Biteable && morph.hp <= 0)
            })
        {
            return Err(SmartFodderSolveError::InvalidFodderHp);
        }
        let plant_candidates = self.equivalent_plant_candidates();
        let direct_x_masks = self.direct_x_masks_for(tables, &plant_candidates)?;
        self.solve_with_direct_x_masks_and_candidates(tables, &direct_x_masks, &plant_candidates, true)
    }

    fn solve_with_direct_x_masks_and_candidates(
        &self, tables: DamageTables<'_>, direct_x_masks: &[u64], plant_candidates: &[i32], prune: bool,
    ) -> Result<SmartFodderChoice, SmartFodderSolveError> {
        // Intentional bounded approximation for ordinary biteable fodder:
        // sample one native bite period, then fully refine the winning basin.
        // Morph, invulnerability, and special lifetimes stay exhaustive.
        let coarse_refine = prune
            && self.fodder_behavior == FodderBehavior::Biteable
            && self.fodder_morph.is_none()
            && self.fodder_invulnerable_for == 0
            && plant_candidates.len() > usize::try_from(BITE_PERIOD).unwrap_or_default();
        let mut search_candidates = if coarse_refine {
            plant_candidates
                .iter()
                .copied()
                .step_by(usize::try_from(BITE_PERIOD).unwrap_or(1))
                .collect::<Vec<_>>()
        } else {
            plant_candidates.to_vec()
        };
        if coarse_refine
            && let Some(last) = plant_candidates
                .last()
                .copied()
                .filter(|last| !search_candidates.contains(last))
        {
            search_candidates.push(last);
        }
        let coarse_count = search_candidates.len();
        let mut best: Option<SmartFodderChoice> = None;
        let lifecycle_pmfs = self
            .threats
            .iter()
            .filter_map(|threat| threat.jack_pmf.as_ref())
            .collect::<Vec<_>>();
        let direct_convolutions = self.prepare_direct_convolutions(tables, direct_x_masks)?;
        let release_values = self.prepare_release_values_for(tables, plant_candidates)?;
        let jack_blast_axes = self.prepare_jack_blast_axes()?;
        let shared_jack_sources = jack_blast_axes
            .iter()
            .zip(self.threats.iter().filter(|threat| threat.jack_pmf.is_some()))
            .map(|(axes, threat)| SharedJackExplosionSource {
                always_hits: &axes.always_hits,
                blockable_hits: &axes.blockable_hits,
                frame_order: zombie_before_check_order(threat.update_rank),
            })
            .collect::<Vec<_>>();
        let shared_deterministic_sources = self
            .deterministic_explosions
            .iter()
            .map(|source| SharedExplosionSource {
                pmf: &source.pmf,
                survival_at_start: 1.0,
                frame_order: zombie_before_check_order(source.update_rank),
            })
            .collect::<Vec<_>>();
        let probability_workspace = BranchExplosionWorkspace::new(&shared_jack_sources, &shared_deterministic_sources)
            .ok_or(SmartFodderSolveError::InvalidJackBranch)?;
        let mut scratch = CandidateScratch::default();
        scratch.contacts.reserve(self.threats.len());
        scratch.pole_boundaries.reserve(self.threats.len());
        scratch.branch_latest_ends.reserve(lifecycle_pmfs.len() + 1);
        scratch.direct_corrections.reserve(self.threats.len());
        scratch.release_queries.reserve(self.threats.len());
        scratch.before.reserve(lifecycle_pmfs.len());
        scratch.survivals.reserve(lifecycle_pmfs.len());
        scratch.probability.jack_states.reserve(lifecycle_pmfs.len());
        scratch.branch_inputs.reserve(lifecycle_pmfs.len() + 1);
        scratch.states.reserve(lifecycle_pmfs.len() + 1);
        scratch.time_groups.reserve(lifecycle_pmfs.len() + 1);
        scratch.ordered_groups.reserve(lifecycle_pmfs.len() + 1);
        scratch.terms.reserve(lifecycle_pmfs.len() + 1);
        let mut search_index = 0;
        loop {
            if coarse_refine && search_index == coarse_count {
                let coarse_best = best.expect("coarse candidates are non-empty").plant_at;
                let radius = BITE_PERIOD.saturating_sub(1);
                let mut refinement = plant_candidates
                    .iter()
                    .copied()
                    .filter(|plant_at| (coarse_best - radius..=coarse_best + radius).contains(plant_at))
                    .filter(|plant_at| !search_candidates[..coarse_count].contains(plant_at))
                    .collect::<Vec<_>>();
                refinement.sort_unstable();
                search_candidates.extend(refinement);
            }
            if search_index >= search_candidates.len() {
                break;
            }
            let plant_at = search_candidates[search_index];
            search_index += 1;
            let direct_before_fodder = 300.0
                * jack_blast_axes
                    .iter()
                    .map(|axes| axes.baseline_before(plant_at))
                    .sum::<f64>();
            let ordinary_before_fodder = self
                .threats
                .iter()
                .enumerate()
                .filter(|(_index, threat)| threat.ordinary_damage_enabled && threat.jack_pmf.is_some())
                .map(|(index, _threat)| release_values.jack_baseline_before(index, plant_at))
                .sum::<f64>();
            let damage_before_fodder =
                self.deterministic_jack_blast_damage + direct_before_fodder + ordinary_before_fodder;
            if prune
                && !coarse_refine
                && best
                    .is_some_and(|best: SmartFodderChoice| damage_before_fodder > best.expected_damage + SCORE_EPSILON)
            {
                break;
            }
            scratch.contacts.clear();
            scratch.contacts.extend(
                self.threats
                    .iter()
                    .map(|threat| threat.contact(plant_at, self.domain.activation_at)),
            );
            scratch.pole_boundaries.clear();
            scratch
                .pole_boundaries
                .extend(
                    self.threats
                        .iter()
                        .zip(&scratch.contacts)
                        .filter_map(|(threat, contact)| {
                            let contact_at = contact.as_ref()?.0;
                            let landing_at = contact_at.saturating_add(POLE_VAULT_DURATION);
                            (threat.pole_vault
                                && threat.ordinary_damage_enabled
                                && self.domain.activation_at.saturating_sub(landing_at) > SLOWED_POLE_MIN_AFTER_LANDING)
                                .then(|| contact_at.saturating_sub(1))
                        }),
                );
            // Removing the fodder can only move its end earlier. The no-remove
            // end is therefore the exact latest time any candidate can query.
            scratch.branch_latest_ends.clear();
            scratch
                .branch_latest_ends
                .push(self.branch_fodder_end(plant_at, None, &scratch.contacts, None).time);
            for jack in 0..lifecycle_pmfs.len() {
                scratch.branch_latest_ends.push(
                    self.branch_fodder_end(plant_at, None, &scratch.contacts, self.excluded_threat(Some(jack)))
                        .time,
                );
            }
            let latest_fodder_end = scratch.branch_latest_ends.iter().copied().max().unwrap_or(plant_at);
            self.prepare_direct_corrections_into(
                tables,
                plant_at,
                latest_fodder_end,
                &scratch.contacts,
                &direct_convolutions,
                &mut scratch.direct_corrections,
            )?;
            self.prepare_release_queries_into(
                tables,
                &release_values,
                &scratch.contacts,
                &mut scratch.release_queries,
            )?;
            critical_remove_candidates_into(
                &mut scratch.remove_candidates,
                plant_at,
                self.domain.remove_by,
                scratch.pole_boundaries.iter().copied(),
            );
            scratch.scores.clear();
            scratch
                .scores
                .resize(scratch.remove_candidates.len(), damage_before_fodder);
            scratch.before.clear();
            scratch
                .before
                .extend(lifecycle_pmfs.iter().map(|pmf| pmf.probability_before(plant_at)));
            scratch.survivals.clear();
            scratch
                .survivals
                .extend(scratch.before.iter().map(|probability| 1.0 - probability));
            scratch.probability.jack_states.clear();
            scratch.probability.jack_states.extend(
                scratch
                    .survivals
                    .iter()
                    .zip(
                        self.threats
                            .iter()
                            .zip(&scratch.contacts)
                            .filter(|(threat, _contact)| threat.jack_pmf.is_some()),
                    )
                    .map(|(survival_at_start, (_threat, contact))| BranchJackState {
                        survival_at_start: *survival_at_start,
                        blockable_from: contact.map_or(i32::MAX, |(contact_at, _x)| contact_at.saturating_add(110)),
                    }),
            );
            scratch.branch_inputs.clear();
            if scratch.survivals.iter().product::<f64>() > 0.0 {
                scratch.branch_inputs.push(CapOneBranchInput {
                    branch_scalar: 1.0,
                    excluded_jack: None,
                });
            }
            for (jack, before_jack) in scratch.before.iter().copied().enumerate() {
                let branch_mass = before_jack
                    * scratch
                        .survivals
                        .iter()
                        .enumerate()
                        .filter(|(index, _survival)| *index != jack)
                        .map(|(_index, survival)| survival)
                        .product::<f64>();
                if branch_mass <= 0.0 {
                    continue;
                }
                scratch.branch_inputs.push(CapOneBranchInput {
                    branch_scalar: before_jack,
                    excluded_jack: Some(jack),
                });
            }
            let probability_storage = std::mem::take(&mut scratch.probability);
            let candidate_probability = probability_workspace
                .candidate_with_storage(plant_at, self.domain.activation_at, probability_storage)
                .ok_or(SmartFodderSolveError::InvalidJackBranch)?;
            for candidate_index in 0..scratch.remove_candidates.len() {
                let remove_at = scratch.remove_candidates[candidate_index];
                scratch.states.clear();
                scratch
                    .states
                    .extend(scratch.branch_inputs.iter().copied().map(|input| {
                        let excluded_threat = self.excluded_threat(input.excluded_jack);
                        BranchScoreState {
                            input,
                            excluded_threat,
                            end: self.branch_fodder_end(plant_at, remove_at, &scratch.contacts, excluded_threat),
                        }
                    }));
                scratch.time_groups.rebuild(&scratch.states, |state| state.end.time);
                scratch.ordered_groups.rebuild(&scratch.states, |state| state.end);
                if prune
                    && best.is_some_and(|best: SmartFodderChoice| {
                        let unavoidable = self.ordinary_damage_lower_bound(
                            &release_values,
                            &scratch.contacts,
                            &candidate_probability,
                            &scratch.states,
                            &scratch.time_groups,
                            &mut scratch.terms,
                        );
                        scratch.scores[candidate_index] + unavoidable > best.expected_damage + SCORE_EPSILON
                    })
                {
                    scratch.scores[candidate_index] = f64::INFINITY;
                    continue;
                }
                scratch.scores[candidate_index] += self.fused_branch_score_with_scratch(
                    tables,
                    &release_values,
                    &scratch.contacts,
                    &scratch.release_queries,
                    &scratch.direct_corrections,
                    &jack_blast_axes,
                    &candidate_probability,
                    &scratch.states,
                    &scratch.branch_inputs,
                    &scratch.time_groups,
                    &scratch.ordered_groups,
                    &mut scratch.terms,
                )?;
            }
            for (remove_at, expected_damage) in scratch.remove_candidates.iter().copied().zip(&scratch.scores) {
                let choice = SmartFodderChoice {
                    plant_at,
                    remove_at,
                    expected_damage: *expected_damage,
                };
                if better(choice, best) {
                    best = Some(choice);
                }
            }
            scratch.probability = candidate_probability.into_storage();
        }
        best.ok_or(SmartFodderSolveError::NoCandidate)
    }

    fn equivalent_plant_candidates(&self) -> Vec<i32> {
        let sorted_blast_events = self
            .threats
            .iter()
            .map(|threat| {
                threat
                    .jack_blast_events
                    .windows(2)
                    .all(|events| events[0].explosion_at <= events[1].explosion_at)
            })
            .collect::<Vec<_>>();
        let mut candidates = Vec::with_capacity(self.domain.plant_candidates.clone().count());
        for plant_at in self.domain.plant_candidates.clone() {
            if candidates.last().copied().is_some_and(|previous| {
                self.adjacent_plant_candidates_are_equivalent_with_order(previous, plant_at, &sorted_blast_events)
            }) {
                *candidates.last_mut().expect("candidate exists") = plant_at;
            } else {
                candidates.push(plant_at);
            }
        }
        candidates
    }

    fn adjacent_plant_candidates_are_equivalent_with_order(
        &self, earlier: i32, later: i32, sorted_blast_events: &[bool],
    ) -> bool {
        if later != earlier.saturating_add(1)
            || self.fodder_behavior != FodderBehavior::Biteable
            || self.fodder_morph.is_some()
            || self.fodder_invulnerable_for != 0
        {
            return false;
        }
        let same_contacts = self.threats.iter().all(|threat| {
            threat.contact(earlier, self.domain.activation_at) == threat.contact(later, self.domain.activation_at)
        });
        let no_jack_event = self.threats.iter().enumerate().all(|(index, threat)| {
            let events_at_time = if sorted_blast_events.get(index) == Some(&true) {
                let start = threat
                    .jack_blast_events
                    .partition_point(|event| event.explosion_at < earlier);
                threat.jack_blast_events[start..]
                    .iter()
                    .take_while(|event| event.explosion_at == earlier)
                    .any(|event| event.probability > 0.0)
            } else {
                threat
                    .jack_blast_events
                    .iter()
                    .any(|event| event.explosion_at == earlier && event.probability > 0.0)
            };
            threat
                .jack_pmf
                .as_ref()
                .is_none_or(|pmf| pmf.probability_at(earlier) == 0.0)
                && !events_at_time
        });
        let no_deterministic_event = self
            .deterministic_explosions
            .iter()
            .all(|source| source.pmf.probability_at(earlier) == 0.0);
        let same_remove_candidates = self.domain.remove_by.is_none_or(|_deadline| {
            self.threats.iter().all(|threat| {
                !threat.pole_vault
                    || !threat.ordinary_damage_enabled
                    || threat
                        .contact(earlier, self.domain.activation_at)
                        .is_none_or(|(contact_at, _x)| contact_at.saturating_sub(1) != earlier)
            })
        });
        same_contacts && no_jack_event && no_deterministic_event && same_remove_candidates
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SmartFodderSolveError {
    #[error("fodder HP must be positive")]
    InvalidFodderHp,
    #[error("smart-fodder domain has no candidate")]
    NoCandidate,
    #[error("a retained jack branch has zero or invalid conditional probability")]
    InvalidJackBranch,
    #[error("a zombie that can be released has no supported release-tail table")]
    UnsupportedReleaseThreat,
    #[error(transparent)]
    Table(#[from] SmartFodderTableError),
}

#[cfg(test)]
mod tests;

mod precompute;
mod scoring;

mod convolution;
use convolution::*;
use scoring::*;

pub(super) use scoring::align_bite_check;
