use std::cell::RefCell;
use std::ops::RangeInclusive;

const EARLY_WEIGHT: f64 = 1.0 / 20.0;
const LATE_WEIGHT: f64 = 19.0 / 20.0;
const EARLY_DISTANCE: RangeInclusive<i32> = 150..=249;
const LATE_DISTANCE: RangeInclusive<i32> = 450..=749;
const JACK_POP_DURATION: i32 = 110;
const LAND_FIRST_FREEZE: RangeInclusive<i32> = 400..=600;
const POOL_FIRST_FREEZE: i32 = 300;

/// Visible inputs that determine a legal conditional explosion distribution.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JackDistributionInput {
    /// Absolute wave-relative spawn time.
    pub spawned_at: i32,
    /// Current wave-relative observation time. The jack is known not to have
    /// entered the popping phase at the end of this time.
    pub observed_at: i32,
    /// Speed used as the native birth-speed proxy in
    /// `2 * floor(distance / speed)`. Smart-fodder v1 deliberately supplies
    /// the current visible speed. Freeze and thaw do not rewrite `mVelX`; a
    /// prior eat-stop speed reroll is therefore accepted as the proxy.
    pub birth_speed: f64,
    /// First ice effect time, if this jack experienced the first freeze.
    pub first_ice_at: Option<i32>,
    /// Whether the jack was in a pool row for its first freeze.
    pub first_freeze_in_pool: bool,
}

/// Finite absolute explosion-time PMF stored on an inclusive integer axis.
#[derive(Clone, Debug, PartialEq)]
pub struct ExplosionPmf {
    start: i32,
    probabilities: Vec<f64>,
    cumulative: Vec<f64>,
}

impl ExplosionPmf {
    /// Enumerates the exact early/late distance priors and, when applicable,
    /// the uniform 400..=600 first-freeze prior. Outcomes inconsistent with
    /// the visible "not popping through observed_at" fact are conditioned out.
    pub fn conditional_unopened(input: JackDistributionInput) -> Result<Self, JackPmfError> {
        Self::conditional_unopened_with_freezes(input, None)
    }

    /// As [`Self::conditional_unopened`], additionally restricting the hidden
    /// first-freeze duration to values compatible with visible position and
    /// animation state. Every retained duration keeps its original uniform
    /// prior weight before conditioning.
    pub fn conditional_unopened_with_freezes(
        input: JackDistributionInput, compatible_freezes: Option<&[i32]>,
    ) -> Result<Self, JackPmfError> {
        if !input.birth_speed.is_finite() || input.birth_speed <= 0.0 {
            return Err(JackPmfError::InvalidBirthSpeed);
        }

        let legal_freeze = |duration: &i32| {
            if input.first_freeze_in_pool {
                *duration == POOL_FIRST_FREEZE
            } else {
                LAND_FIRST_FREEZE.contains(duration)
            }
        };
        let freeze_durations = match (input.first_ice_at, compatible_freezes) {
            (None, _) => vec![0],
            (Some(_), None) if input.first_freeze_in_pool => vec![POOL_FIRST_FREEZE],
            (Some(_), None) => LAND_FIRST_FREEZE.collect(),
            (Some(_), Some(durations)) if !durations.is_empty() && durations.iter().all(legal_freeze) => {
                let mut durations = durations.to_vec();
                durations.sort_unstable();
                durations.dedup();
                durations
            }
            (Some(_), Some(_)) => return Err(JackPmfError::InvalidCompatibleFreezeDurations),
        };
        let freeze_count = freeze_durations.len() as f64;
        let mut conditional_mass = 0.0;
        let mut start = i32::MAX;
        let mut end = i32::MIN;
        for_each_outcome(input, &freeze_durations, freeze_count, |time, probability| {
            conditional_mass += probability;
            start = start.min(time);
            end = end.max(time);
        });
        if conditional_mass <= 0.0 || !conditional_mass.is_finite() {
            return Err(JackPmfError::VisibleStateHasZeroProbability);
        }
        let len =
            usize::try_from(i64::from(end) - i64::from(start) + 1).map_err(|_error| JackPmfError::TimeRangeTooLarge)?;
        let mut probabilities = vec![0.0; len];
        for_each_outcome(input, &freeze_durations, freeze_count, |time, probability| {
            probabilities[(time - start) as usize] += probability / conditional_mass;
        });
        Ok(Self::with_cumulative(start, probabilities))
    }

    #[must_use]
    pub fn from_probabilities(start: i32, probabilities: Vec<f64>) -> Option<Self> {
        let sum: f64 = probabilities.iter().sum();
        let valid = !probabilities.is_empty()
            && probabilities
                .iter()
                .all(|probability| probability.is_finite() && *probability >= 0.0)
            && (sum - 1.0).abs() <= 1.0e-10;
        valid.then(|| Self::with_cumulative(start, probabilities))
    }

    /// Builds a defective event-time distribution. Missing mass means that
    /// the event never occurs in the represented support and therefore stays
    /// in the survival tail. This is used for spatially filtered jack hits.
    #[must_use]
    pub fn from_subprobabilities(start: i32, probabilities: Vec<f64>) -> Option<Self> {
        let sum: f64 = probabilities.iter().sum();
        let valid = !probabilities.is_empty()
            && probabilities
                .iter()
                .all(|probability| probability.is_finite() && *probability >= 0.0)
            && sum <= 1.0 + 1.0e-10;
        valid.then(|| Self::with_cumulative(start, probabilities))
    }

    #[must_use]
    pub const fn start(&self) -> i32 {
        self.start
    }

    #[must_use]
    pub fn end(&self) -> i32 {
        self.start
            .saturating_add(i32::try_from(self.probabilities.len() - 1).unwrap_or(i32::MAX))
    }

    #[must_use]
    pub fn probability_at(&self, time: i32) -> f64 {
        time.checked_sub(self.start)
            .and_then(|index| usize::try_from(index).ok())
            .and_then(|index| self.probabilities.get(index))
            .copied()
            .unwrap_or(0.0)
    }

    /// `P(E < time)` under the half-open endpoint convention.
    #[must_use]
    pub fn probability_before(&self, time: i32) -> f64 {
        if time <= self.start {
            return 0.0;
        }
        let count = usize::try_from(time.saturating_sub(self.start))
            .unwrap_or(usize::MAX)
            .min(self.probabilities.len());
        self.cumulative[count]
    }

    /// `P(E > time)`.
    #[must_use]
    pub fn probability_after(&self, time: i32) -> f64 {
        1.0 - self.probability_before(time.saturating_add(1))
    }

    fn with_cumulative(start: i32, probabilities: Vec<f64>) -> Self {
        let mut cumulative = Vec::with_capacity(probabilities.len() + 1);
        cumulative.push(0.0);
        for probability in &probabilities {
            cumulative.push(cumulative.last().copied().unwrap_or(0.0) + probability);
        }
        Self {
            start,
            probabilities,
            cumulative,
        }
    }
}

fn for_each_outcome(
    input: JackDistributionInput, freeze_durations: &[i32], freeze_count: f64, mut visit: impl FnMut(i32, f64),
) {
    enumerate_distance_branch(
        input,
        EARLY_DISTANCE,
        EARLY_WEIGHT,
        freeze_durations,
        freeze_count,
        &mut visit,
    );
    enumerate_distance_branch(input, LATE_DISTANCE, LATE_WEIGHT, freeze_durations, freeze_count, visit);
}

fn enumerate_distance_branch(
    input: JackDistributionInput, distances: RangeInclusive<i32>, branch_weight: f64, freeze_durations: &[i32],
    freeze_count: f64, mut visit: impl FnMut(i32, f64),
) {
    let distance_count = f64::from(distances.end() - distances.start() + 1);
    let outcome_weight = branch_weight / distance_count / freeze_count;
    for distance in distances {
        let active_until_pop = 2.0 * (f64::from(distance) / input.birth_speed).floor();
        let Ok(active_until_pop) = i32::try_from(active_until_pop as i64) else {
            continue;
        };
        for freeze_duration in freeze_durations.iter().copied() {
            let popping_at =
                absolute_after_active_time(input.spawned_at, active_until_pop, input.first_ice_at, freeze_duration);
            if popping_at > input.observed_at {
                visit(popping_at.saturating_add(JACK_POP_DURATION), outcome_weight);
            }
        }
    }
}

fn absolute_after_active_time(spawned_at: i32, active_time: i32, ice_at: Option<i32>, freeze_duration: i32) -> i32 {
    let Some(ice_at) = ice_at.filter(|ice_at| *ice_at >= spawned_at) else {
        return spawned_at.saturating_add(active_time);
    };
    let active_before_ice = ice_at.saturating_sub(spawned_at);
    if active_time <= active_before_ice {
        spawned_at.saturating_add(active_time)
    } else {
        spawned_at.saturating_add(active_time).saturating_add(freeze_duration)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CapOneDiagnostics {
    pub omitted_probability: f64,
    pub excess_expected_count: f64,
}

/// Weights of the retained `N_F = 0` and identity-bearing `N_F = 1`
/// branches. They intentionally sum to `1 - epsilon`, not one.
#[derive(Clone, Debug, PartialEq)]
pub struct CapOneWeights {
    pub none_before: f64,
    pub only_before: Vec<f64>,
    pub diagnostics: CapOneDiagnostics,
}

#[must_use]
pub fn cap_one_weights(pmfs: &[ExplosionPmf], fodder_at: i32) -> CapOneWeights {
    let before: Vec<f64> = pmfs.iter().map(|pmf| pmf.probability_before(fodder_at)).collect();
    let mut prefix = vec![1.0; before.len() + 1];
    for (index, probability) in before.iter().copied().enumerate() {
        prefix[index + 1] = prefix[index] * (1.0 - probability);
    }
    let mut suffix = vec![1.0; before.len() + 1];
    for index in (0..before.len()).rev() {
        suffix[index] = suffix[index + 1] * (1.0 - before[index]);
    }
    let only_before = before
        .iter()
        .copied()
        .enumerate()
        .map(|(index, probability)| probability * prefix[index] * suffix[index + 1])
        .collect::<Vec<_>>();
    let none_before = prefix[before.len()];
    let exactly_one: f64 = only_before.iter().sum();
    let at_least_one = 1.0 - none_before;
    let expected_count: f64 = before.iter().sum();
    CapOneWeights {
        none_before,
        only_before,
        diagnostics: CapOneDiagnostics {
            omitted_probability: (1.0 - none_before - exactly_one).max(0.0),
            excess_expected_count: (expected_count - at_least_one).max(0.0),
        },
    }
}

/// Conditional first-explosion distribution for the jacks retained in one
/// Cap-1 branch. `first_by_jack` intentionally overlaps on integer-time ties.
#[derive(Clone, Debug, PartialEq)]
pub struct FirstExplosionKernel {
    start: i32,
    survival_before: Vec<f64>,
    first_by_jack: Vec<Vec<f64>>,
}

impl FirstExplosionKernel {
    #[must_use]
    pub fn conditional(pmfs: &[ExplosionPmf], excluded: Option<usize>, fodder_at: i32, end: i32) -> Option<Self> {
        Self::conditional_impl(pmfs, excluded, fodder_at, end, true)
    }

    fn conditional_impl(
        pmfs: &[ExplosionPmf], excluded: Option<usize>, fodder_at: i32, end: i32, include_identity: bool,
    ) -> Option<Self> {
        if end < fodder_at || excluded.is_some_and(|index| index >= pmfs.len()) {
            return None;
        }
        let len = usize::try_from(i64::from(end) - i64::from(fodder_at) + 1).ok()?;
        let retained = pmfs.len().saturating_sub(usize::from(excluded.is_some()));
        let mut conditional_at = vec![vec![0.0; len]; retained];
        let mut out_index = 0;
        for (index, pmf) in pmfs.iter().enumerate() {
            if excluded == Some(index) {
                continue;
            }
            let denominator = 1.0 - pmf.probability_before(fodder_at);
            if denominator <= 0.0 {
                return None;
            }
            for (offset, probability) in conditional_at[out_index].iter_mut().enumerate() {
                let time = fodder_at.saturating_add(i32::try_from(offset).ok()?);
                *probability = pmf.probability_at(time) / denominator;
            }
            out_index += 1;
        }

        let mut survival_before = vec![1.0; len + 1];
        let mut first_by_jack = if include_identity {
            vec![vec![0.0; len]; retained]
        } else {
            Vec::new()
        };
        let mut per_jack_survival = vec![1.0; retained];
        let mut identity_scratch = include_identity.then(|| (vec![1.0; retained + 1], vec![1.0; retained + 1]));
        for offset in 0..len {
            let all_before = per_jack_survival.iter().product();
            survival_before[offset] = all_before;
            if let Some((prefix, suffix)) = &mut identity_scratch {
                for index in 0..retained {
                    prefix[index + 1] = prefix[index] * per_jack_survival[index];
                }
                for index in (0..retained).rev() {
                    suffix[index] = suffix[index + 1] * per_jack_survival[index];
                }
                for index in 0..retained {
                    let others_before = prefix[index] * suffix[index + 1];
                    first_by_jack[index][offset] = conditional_at[index][offset] * others_before;
                }
            }
            for index in 0..retained {
                per_jack_survival[index] = (per_jack_survival[index] - conditional_at[index][offset]).max(0.0);
            }
        }
        survival_before[len] = per_jack_survival.iter().product();
        Some(Self {
            start: fodder_at,
            survival_before,
            first_by_jack,
        })
    }

    #[must_use]
    pub fn first_probability_at(&self, time: i32) -> f64 {
        (self.survival_after(time.saturating_sub(1)) - self.survival_after(time)).max(0.0)
    }

    #[must_use]
    pub const fn start(&self) -> i32 {
        self.start
    }

    #[must_use]
    pub fn end(&self) -> i32 {
        self.start
            .saturating_add(i32::try_from(self.survival_before.len().saturating_sub(2)).unwrap_or(i32::MAX))
    }

    #[must_use]
    pub fn first_probability_for_jack_at(&self, jack: usize, time: i32) -> f64 {
        self.first_by_jack
            .get(jack)
            .map_or(0.0, |probabilities| self.value_at(probabilities, time))
    }

    /// Probability that every retained jack explodes strictly after `time`.
    #[must_use]
    pub fn survival_after(&self, time: i32) -> f64 {
        if time < self.start {
            return 1.0;
        }
        let Some(offset) = time
            .checked_sub(self.start)
            .and_then(|value| usize::try_from(value).ok())
            .and_then(|value| value.checked_add(1))
        else {
            return 0.0;
        };
        self.survival_before.get(offset).copied().unwrap_or(0.0)
    }

    fn value_at(&self, values: &[f64], time: i32) -> f64 {
        time.checked_sub(self.start)
            .and_then(|offset| usize::try_from(offset).ok())
            .and_then(|offset| values.get(offset))
            .copied()
            .unwrap_or(0.0)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct SharedJackExplosionSource<'a> {
    pub always_hits: &'a ExplosionPmf,
    pub blockable_hits: &'a ExplosionPmf,
    pub frame_order: i32,
}

#[derive(Clone, Copy)]
pub(crate) struct SharedExplosionSource<'a> {
    pub pmf: &'a ExplosionPmf,
    pub survival_at_start: f64,
    pub frame_order: i32,
}

#[derive(Clone, Copy)]
pub(crate) struct BranchJackState {
    pub survival_at_start: f64,
    pub blockable_from: i32,
}

#[derive(Clone, Copy)]
pub(crate) struct CapOneBranchInput {
    pub branch_scalar: f64,
    pub excluded_jack: Option<usize>,
}

enum SharedExplosionKind<'a> {
    Jack {
        always_hits: &'a ExplosionPmf,
        blockable_hits: &'a ExplosionPmf,
    },
    Other {
        pmf: &'a ExplosionPmf,
        survival_at_start: f64,
    },
}

struct SharedExplosion<'a> {
    kind: SharedExplosionKind<'a>,
    frame_order: i32,
}

/// Absolute event axes and object order shared by the entire planting window.
/// A branch view adds only its planting-time survivals and Cap-1 identity.
pub(crate) struct BranchExplosionWorkspace<'a> {
    sources: Vec<SharedExplosion<'a>>,
    ordered_source_indices: Vec<usize>,
    ordered_source_orders: Vec<i32>,
    source_order_positions: Vec<usize>,
    jack_count: usize,
}

pub(crate) struct CandidateProbabilityCache<'workspace, 'source> {
    workspace: &'workspace BranchExplosionWorkspace<'source>,
    jack_states: Vec<BranchJackState>,
    start: i32,
    end: i32,
    cache: Option<RefCell<BoundaryCache>>,
}

#[derive(Default)]
pub(crate) struct CandidateProbabilityStorage {
    pub jack_states: Vec<BranchJackState>,
    cache: Option<BoundaryCache>,
}

#[derive(Clone, Copy)]
pub(crate) struct AggregateBranchProbabilityView<'terms, 'candidate, 'workspace, 'source> {
    candidate: &'candidate CandidateProbabilityCache<'workspace, 'source>,
    terms: &'terms [CapOneBranchInput],
    focused_jack: Option<usize>,
}

struct BoundaryCache {
    positions: Vec<usize>,
    values: Vec<f64>,
    prefix: Vec<ProductStat>,
    suffix: Vec<ProductStat>,
    source_count: usize,
}

impl BoundaryCache {
    fn reset(&mut self, boundary_count: usize, source_count: usize) {
        self.positions.clear();
        self.positions.resize(boundary_count, usize::MAX);
        self.values.clear();
        self.prefix.clear();
        self.suffix.clear();
        self.source_count = source_count;
    }
}

#[derive(Clone, Copy)]
struct ProductStat {
    nonzero: f64,
    zeros: usize,
}

fn valid_survival(value: f64) -> bool {
    value.is_finite() && (0.0..=1.0 + 1.0e-10).contains(&value)
}

fn probability_between(pmf: &ExplosionPmf, start: i32, end: i32) -> f64 {
    if end <= start {
        0.0
    } else {
        (pmf.probability_before(end) - pmf.probability_before(start)).max(0.0)
    }
}

impl<'a> BranchExplosionWorkspace<'a> {
    pub fn new(
        jack_sources: &[SharedJackExplosionSource<'a>], other_sources: &[SharedExplosionSource<'a>],
    ) -> Option<Self> {
        if other_sources
            .iter()
            .any(|source| !valid_survival(source.survival_at_start))
        {
            return None;
        }
        let sources = jack_sources
            .iter()
            .map(|source| SharedExplosion {
                kind: SharedExplosionKind::Jack {
                    always_hits: source.always_hits,
                    blockable_hits: source.blockable_hits,
                },
                frame_order: source.frame_order,
            })
            .chain(other_sources.iter().map(|source| SharedExplosion {
                kind: SharedExplosionKind::Other {
                    pmf: source.pmf,
                    survival_at_start: source.survival_at_start.max(0.0),
                },
                frame_order: source.frame_order,
            }))
            .collect::<Vec<_>>();
        let mut ordered_source_indices = (0..sources.len()).collect::<Vec<_>>();
        ordered_source_indices.sort_unstable_by_key(|index| sources[*index].frame_order);
        let ordered_source_orders = ordered_source_indices
            .iter()
            .map(|index| sources[*index].frame_order)
            .collect::<Vec<_>>();
        if ordered_source_orders.windows(2).any(|orders| orders[0] == orders[1]) {
            return None;
        }
        let mut source_order_positions = vec![0; sources.len()];
        for (position, source) in ordered_source_indices.iter().copied().enumerate() {
            source_order_positions[source] = position;
        }
        Some(Self {
            sources,
            ordered_source_indices,
            ordered_source_orders,
            source_order_positions,
            jack_count: jack_sources.len(),
        })
    }

    pub fn candidate_with_storage<'workspace>(
        &'workspace self, start: i32, end: i32, mut storage: CandidateProbabilityStorage,
    ) -> Option<CandidateProbabilityCache<'workspace, 'a>> {
        if end < start
            || storage.jack_states.len() != self.jack_count
            || storage
                .jack_states
                .iter()
                .any(|state| !valid_survival(state.survival_at_start))
        {
            return None;
        }
        let boundary_count = usize::try_from(end.saturating_sub(start).saturating_add(1)).ok()?;
        let source_count = self.sources.len();
        let cache = (source_count > 2).then(|| {
            let mut cache = storage.cache.take().unwrap_or_else(|| BoundaryCache {
                positions: Vec::new(),
                values: Vec::new(),
                prefix: Vec::new(),
                suffix: Vec::new(),
                source_count,
            });
            cache.reset(boundary_count, source_count);
            RefCell::new(cache)
        });
        Some(CandidateProbabilityCache {
            workspace: self,
            jack_states: storage.jack_states,
            start,
            end,
            cache,
        })
    }

    fn remaining(&self, start: i32, jack_states: &[BranchJackState], source: usize, boundary_at: i32) -> f64 {
        match &self.sources[source].kind {
            SharedExplosionKind::Jack {
                always_hits,
                blockable_hits,
            } => {
                let state = jack_states[source];
                let always = probability_between(always_hits, start, boundary_at);
                let blockable = probability_between(blockable_hits, start.max(state.blockable_from), boundary_at);
                (state.survival_at_start - always - blockable).max(0.0)
            }
            SharedExplosionKind::Other { pmf, survival_at_start } => {
                (survival_at_start - probability_between(pmf, start, boundary_at)).max(0.0)
            }
        }
    }
}

impl<'workspace, 'source> CandidateProbabilityCache<'workspace, 'source> {
    pub fn into_storage(self) -> CandidateProbabilityStorage {
        CandidateProbabilityStorage {
            jack_states: self.jack_states,
            cache: self.cache.map(RefCell::into_inner),
        }
    }

    pub fn aggregate<'terms>(
        &self, terms: &'terms [CapOneBranchInput], focused_jack: Option<usize>,
    ) -> AggregateBranchProbabilityView<'terms, '_, 'workspace, 'source> {
        debug_assert!(focused_jack.is_none_or(|jack| jack < self.workspace.jack_count));
        debug_assert!(terms.iter().all(|term| {
            term.branch_scalar.is_finite()
                && term.branch_scalar >= 0.0
                && term.excluded_jack.is_none_or(|jack| jack < self.workspace.jack_count)
        }));
        AggregateBranchProbabilityView {
            candidate: self,
            terms,
            focused_jack,
        }
    }

    fn ensure(&self, time: i32) -> Option<usize> {
        let cache = self.cache.as_ref()?;
        let offset = time
            .checked_sub(self.start)
            .and_then(|offset| usize::try_from(offset).ok())?;
        if time > self.end {
            return None;
        }
        let mut cache = cache.borrow_mut();
        let position = *cache.positions.get(offset)?;
        if position != usize::MAX {
            return Some(position);
        }
        let source_count = cache.source_count;
        let entry = cache.values.len() / source_count;
        let value_start = cache.values.len();
        for source in 0..source_count {
            let value = self.workspace.remaining(self.start, &self.jack_states, source, time);
            cache.values.push(value);
        }
        let mut product = ProductStat::identity();
        cache.prefix.push(product);
        for source in self.workspace.ordered_source_indices.iter().copied() {
            product = product.include(cache.values[value_start + source]);
            cache.prefix.push(product);
        }
        let suffix_start = cache.suffix.len();
        cache
            .suffix
            .extend(std::iter::repeat_n(ProductStat::identity(), source_count + 1));
        product = ProductStat::identity();
        cache.suffix[suffix_start + source_count] = product;
        for position in (0..source_count).rev() {
            let source = self.workspace.ordered_source_indices[position];
            product = product.include(cache.values[value_start + source]);
            cache.suffix[suffix_start + position] = product;
        }
        cache.positions[offset] = entry;
        Some(entry)
    }

    fn product_without(&self, time: i32, first: Option<usize>, second: Option<usize>) -> f64 {
        if self.cache.is_none() {
            return self
                .workspace
                .sources
                .iter()
                .enumerate()
                .filter(|(source, _)| Some(*source) != first && Some(*source) != second)
                .map(|(source, _)| self.workspace.remaining(self.start, &self.jack_states, source, time))
                .product();
        }
        let Some(offset) = self.ensure(time) else {
            return 0.0;
        };
        let cache = self.cache.as_ref().expect("checked above").borrow();
        Self::cached_product_without(&cache, offset, first, second)
    }

    fn cached_product_without(
        cache: &BoundaryCache, offset: usize, first: Option<usize>, second: Option<usize>,
    ) -> f64 {
        let value_start = offset * cache.source_count;
        let product_start = offset * (cache.source_count + 1);
        cache.prefix[product_start + cache.source_count].without_sources(
            &cache.values[value_start..value_start + cache.source_count],
            first,
            second,
        )
    }

    fn probability_at_and_ordered(
        &self, position: usize, time: i32, first: Option<usize>, second: Option<usize>,
    ) -> (f64, f64, f64) {
        if self.cache.is_none() {
            let after_survival = self.product_without(time.saturating_add(1), first, second);
            let total = (self.product_without(time, first, second) - after_survival).max(0.0);
            let retained = |source: usize| Some(source) != first && Some(source) != second;
            let mut earlier_after = 1.0;
            for source in self.workspace.ordered_source_indices[..position].iter().copied() {
                if retained(source) {
                    earlier_after *=
                        self.workspace
                            .remaining(self.start, &self.jack_states, source, time.saturating_add(1));
                }
            }
            let mut suffix_before = 1.0;
            let mut suffix_after = 1.0;
            for source in self.workspace.ordered_source_indices[position..].iter().copied() {
                if retained(source) {
                    suffix_before *= self.workspace.remaining(self.start, &self.jack_states, source, time);
                    suffix_after *=
                        self.workspace
                            .remaining(self.start, &self.jack_states, source, time.saturating_add(1));
                }
            }
            return (
                total,
                earlier_after * (suffix_before - suffix_after).max(0.0),
                after_survival,
            );
        }
        let (Some(before), Some(after)) = (self.ensure(time), self.ensure(time.saturating_add(1))) else {
            return (0.0, 0.0, 0.0);
        };
        let cache = self.cache.as_ref().expect("checked above").borrow();
        self.cached_probability_at_and_ordered(&cache, position, before, after, first, second)
    }

    fn cached_probability_at_and_ordered(
        &self, cache: &BoundaryCache, position: usize, before: usize, after: usize, first: Option<usize>,
        second: Option<usize>,
    ) -> (f64, f64, f64) {
        let count = cache.source_count;
        let stride = count + 1;
        let before_values = &cache.values[before * count..before * count + count];
        let after_values = &cache.values[after * count..after * count + count];
        let first_position = first.map(|source| self.workspace.source_order_positions[source]);
        let second_position = second.map(|source| self.workspace.source_order_positions[source]);
        let total_before = cache.prefix[before * stride + count].without_sources(before_values, first, second);
        let total_after = cache.prefix[after * stride + count].without_sources(after_values, first, second);
        let total = (total_before - total_after).max(0.0);
        let earlier_after = cache.prefix[after * stride + position].without_sources(
            after_values,
            first.filter(|_| first_position.is_some_and(|source| source < position)),
            second.filter(|_| second_position.is_some_and(|source| source < position)),
        );
        let suffix_before = cache.suffix[before * stride + position].without_sources(
            before_values,
            first.filter(|_| first_position.is_some_and(|source| source >= position)),
            second.filter(|_| second_position.is_some_and(|source| source >= position)),
        );
        let suffix_after = cache.suffix[after * stride + position].without_sources(
            after_values,
            first.filter(|_| first_position.is_some_and(|source| source >= position)),
            second.filter(|_| second_position.is_some_and(|source| source >= position)),
        );
        (
            total,
            earlier_after * (suffix_before - suffix_after).max(0.0),
            total_after,
        )
    }
}

impl ProductStat {
    const fn identity() -> Self {
        Self { nonzero: 1.0, zeros: 0 }
    }

    fn include(mut self, value: f64) -> Self {
        if value == 0.0 {
            self.zeros += 1;
        } else {
            self.nonzero *= value;
        }
        self
    }

    fn exclude(mut self, value: f64) -> Self {
        if value == 0.0 {
            self.zeros = self.zeros.saturating_sub(1);
        } else {
            self.nonzero /= value;
        }
        self
    }

    fn without_sources(self, values: &[f64], first: Option<usize>, second: Option<usize>) -> f64 {
        let mut product = self;
        for source in [first, second.filter(|source| Some(*source) != first)]
            .into_iter()
            .flatten()
        {
            product = product.exclude(values[source]);
        }
        if product.zeros == 0 { product.nonzero } else { 0.0 }
    }
}

impl AggregateBranchProbabilityView<'_, '_, '_, '_> {
    pub const fn start(&self) -> i32 {
        self.candidate.start
    }

    pub fn branch_mass(&self) -> f64 {
        self.survival_before(self.start())
    }

    pub fn survival_after(&self, time: i32) -> f64 {
        if time < self.start() {
            return self.branch_mass();
        }
        self.survival_before(time.saturating_add(1))
    }

    pub fn first_probability_at_or_after_order(&self, frame_order: i32, time: i32) -> f64 {
        self.probability_at_ordered_and_survival_after(frame_order, time).1
    }

    pub fn probability_at_ordered_and_survival_after(&self, frame_order: i32, time: i32) -> (f64, f64, f64) {
        let position = self
            .candidate
            .workspace
            .ordered_source_orders
            .partition_point(|source_order| *source_order < frame_order);
        self.probability_at_and_ordered_at(position, time)
    }

    pub fn first_probability_at_or_after_focus(&self, time: i32) -> f64 {
        self.probability_at_ordered_and_survival_after_focus(time).1
    }

    pub fn probability_at_ordered_and_survival_after_focus(&self, time: i32) -> (f64, f64, f64) {
        let jack = self.focused_jack.expect("focused aggregate required");
        let position = self.candidate.workspace.source_order_positions[jack].saturating_add(1);
        self.probability_at_and_ordered_at(position, time)
    }

    fn retained_terms(&self) -> impl Iterator<Item = CapOneBranchInput> + '_ {
        self.terms
            .iter()
            .copied()
            .filter(|term| self.focused_jack.is_none_or(|jack| term.excluded_jack != Some(jack)))
    }

    fn survival_before(&self, time: i32) -> f64 {
        if self.candidate.cache.is_none() {
            return self
                .retained_terms()
                .map(|term| {
                    term.branch_scalar
                        * self
                            .candidate
                            .product_without(time, term.excluded_jack, self.focused_jack)
                })
                .sum();
        }
        let Some(offset) = self.candidate.ensure(time) else {
            return 0.0;
        };
        let cache = self.candidate.cache.as_ref().expect("checked above").borrow();
        self.cached_survival_before(&cache, offset)
    }

    fn probability_at_and_ordered_at(&self, position: usize, time: i32) -> (f64, f64, f64) {
        let mut result = (0.0, 0.0, 0.0);
        if self.candidate.cache.is_none() {
            for term in self.retained_terms() {
                let (total, ordered, after_survival) =
                    self.candidate
                        .probability_at_and_ordered(position, time, term.excluded_jack, self.focused_jack);
                result.0 += term.branch_scalar * total;
                result.1 += term.branch_scalar * ordered.clamp(0.0, total);
                result.2 += term.branch_scalar * after_survival;
            }
            return result;
        }
        let (Some(before), Some(after)) = (
            self.candidate.ensure(time),
            self.candidate.ensure(time.saturating_add(1)),
        ) else {
            return result;
        };
        let cache = self.candidate.cache.as_ref().expect("checked above").borrow();
        let before_survival = self.cached_survival_before(&cache, before);
        let after_survival = self.cached_survival_before(&cache, after);
        let total = (before_survival - after_survival).max(0.0);
        let ordered = self.cached_ordered_probability(&cache, position, before, after);
        (total, ordered.clamp(0.0, total), after_survival)
    }

    fn cached_survival_before(&self, cache: &BoundaryCache, offset: usize) -> f64 {
        let count = cache.source_count;
        let value_start = offset * count;
        let values = &cache.values[value_start..value_start + count];
        let full = cache.prefix[offset * (count + 1) + count];
        if full.zeros == 0 {
            let mut base = full.nonzero;
            if let Some(focus) = self.focused_jack {
                base /= values[focus];
            }
            return self
                .retained_terms()
                .map(|term| term.branch_scalar * term.excluded_jack.map_or(base, |excluded| base / values[excluded]))
                .sum();
        }
        self.retained_terms()
            .map(|term| {
                term.branch_scalar
                    * CandidateProbabilityCache::cached_product_without(
                        cache,
                        offset,
                        term.excluded_jack,
                        self.focused_jack,
                    )
            })
            .sum()
    }

    fn cached_ordered_probability(&self, cache: &BoundaryCache, position: usize, before: usize, after: usize) -> f64 {
        let count = cache.source_count;
        let stride = count + 1;
        let before_values = &cache.values[before * count..before * count + count];
        let after_values = &cache.values[after * count..after * count + count];
        let mut earlier_after = cache.prefix[after * stride + position];
        let mut suffix_before = cache.suffix[before * stride + position];
        let mut suffix_after = cache.suffix[after * stride + position];
        let focus_position = self
            .focused_jack
            .map(|jack| self.candidate.workspace.source_order_positions[jack]);
        if let Some(focus) = self.focused_jack {
            if focus_position.is_some_and(|focus_position| focus_position < position) {
                earlier_after = earlier_after.exclude(after_values[focus]);
            } else {
                suffix_before = suffix_before.exclude(before_values[focus]);
                suffix_after = suffix_after.exclude(after_values[focus]);
            }
        }
        if earlier_after.zeros == 0 && suffix_before.zeros == 0 && suffix_after.zeros == 0 {
            let common = earlier_after.nonzero * (suffix_before.nonzero - suffix_after.nonzero).max(0.0);
            let mut result = 0.0;
            for term in self.retained_terms() {
                let event = match term.excluded_jack {
                    None => common,
                    Some(excluded) if self.candidate.workspace.source_order_positions[excluded] < position => {
                        common / after_values[excluded]
                    }
                    Some(excluded) => {
                        earlier_after.nonzero
                            * (suffix_before.nonzero / before_values[excluded]
                                - suffix_after.nonzero / after_values[excluded])
                                .max(0.0)
                    }
                };
                result += term.branch_scalar * event;
            }
            return result;
        }
        self.retained_terms()
            .map(|term| {
                let (_total, ordered, _after_survival) = self.candidate.cached_probability_at_and_ordered(
                    cache,
                    position,
                    before,
                    after,
                    term.excluded_jack,
                    self.focused_jack,
                );
                term.branch_scalar * ordered
            })
            .sum()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum JackPmfError {
    #[error("jack birth speed must be finite and positive")]
    InvalidBirthSpeed,
    #[error("visible unopened state has zero prior probability")]
    VisibleStateHasZeroProbability,
    #[error("jack explosion time range is too large")]
    TimeRangeTooLarge,
    #[error("compatible first-freeze durations must be a non-empty subset of 400..=600")]
    InvalidCompatibleFreezeDurations,
}

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use tests::{BranchProbabilityView, FocusedBranchProbabilityView};
