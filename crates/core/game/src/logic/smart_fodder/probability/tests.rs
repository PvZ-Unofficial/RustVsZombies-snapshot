use super::*;

fn point(time: i32) -> ExplosionPmf {
    ExplosionPmf::from_probabilities(time, vec![1.0]).expect("point PMF")
}

#[test]
fn unopened_distribution_matches_documented_no_ice_bounds() {
    let pmf = ExplosionPmf::conditional_unopened(JackDistributionInput {
        spawned_at: 0,
        observed_at: -1,
        birth_speed: 0.5,
        first_ice_at: None,
        first_freeze_in_pool: false,
    })
    .expect("PMF");

    assert_eq!(pmf.start(), 710);
    assert_eq!(pmf.end(), 3_106);
    assert!((pmf.probability_before(i32::MAX) - 1.0).abs() < 1.0e-12);
}

#[test]
fn first_ice_delays_only_active_time_after_ice() {
    assert_eq!(absolute_after_active_time(0, 100, Some(80), 500), 600);
    assert_eq!(absolute_after_active_time(0, 80, Some(80), 500), 80);
    assert_eq!(absolute_after_active_time(100, 80, Some(0), 500), 180);
}

#[test]
fn visible_motion_can_condition_the_hidden_freeze_prior_without_reading_countdown() {
    let input = JackDistributionInput {
        spawned_at: 0,
        observed_at: 700,
        birth_speed: 0.5,
        first_ice_at: Some(80),
        first_freeze_in_pool: false,
    };
    let all = ExplosionPmf::conditional_unopened(input).expect("all compatible freezes");
    let narrowed = ExplosionPmf::conditional_unopened_with_freezes(input, Some(&[500])).expect("narrow PMF");

    assert!(all.start() < narrowed.start());
    assert!(matches!(
        ExplosionPmf::conditional_unopened_with_freezes(input, Some(&[399])),
        Err(JackPmfError::InvalidCompatibleFreezeDurations)
    ));
}

#[test]
fn pool_first_freeze_is_fixed_at_300() {
    let land = ExplosionPmf::conditional_unopened(JackDistributionInput {
        spawned_at: 0,
        observed_at: 700,
        birth_speed: 0.5,
        first_ice_at: Some(80),
        first_freeze_in_pool: false,
    })
    .expect("land PMF");
    let pool = ExplosionPmf::conditional_unopened(JackDistributionInput {
        spawned_at: 0,
        observed_at: 700,
        birth_speed: 0.5,
        first_ice_at: Some(80),
        first_freeze_in_pool: true,
    })
    .expect("pool PMF");

    assert_eq!(pool.start() + 100, land.start());
    assert!(matches!(
        ExplosionPmf::conditional_unopened_with_freezes(
            JackDistributionInput {
                spawned_at: 0,
                observed_at: 700,
                birth_speed: 0.5,
                first_ice_at: Some(80),
                first_freeze_in_pool: true,
            },
            Some(&[400]),
        ),
        Err(JackPmfError::InvalidCompatibleFreezeDurations)
    ));
}

#[test]
fn visible_unopened_fact_conditions_out_earlier_outcomes() {
    let pmf = ExplosionPmf::conditional_unopened(JackDistributionInput {
        spawned_at: 0,
        observed_at: 1_000,
        birth_speed: 0.5,
        first_ice_at: None,
        first_freeze_in_pool: false,
    })
    .expect("conditional PMF");

    assert!(pmf.start() >= 1_111);
    assert!((pmf.probability_before(i32::MAX) - 1.0).abs() < 1.0e-12);
}

#[test]
fn spatially_filtered_event_keeps_missing_mass_in_survival() {
    let pmf = ExplosionPmf::from_subprobabilities(10, vec![0.25]).expect("defective PMF");
    let kernel = FirstExplosionKernel::conditional(&[pmf], None, 10, 11).expect("kernel");
    assert_eq!(kernel.first_probability_at(10), 0.25);
    assert_eq!(kernel.survival_after(10), 0.75);
}

#[test]
fn cap_one_keeps_real_zero_and_single_weights_without_projection() {
    let synthetic = ExplosionPmf::from_probabilities(0, vec![0.025, 0.975]).expect("PMF");
    let pmfs = vec![synthetic; 5];
    let weights = cap_one_weights(&pmfs, 1);
    let expected_epsilon = 0.005_943_320_312_5;
    let expected_rho = 0.006_095_693_359_375;

    assert!((weights.diagnostics.omitted_probability - expected_epsilon).abs() < 1.0e-12);
    assert!((weights.diagnostics.excess_expected_count - expected_rho).abs() < 1.0e-12);
    let retained = weights.none_before + weights.only_before.iter().sum::<f64>();
    assert!((retained - (1.0 - expected_epsilon)).abs() < 1.0e-12);
}

#[test]
fn first_explosion_ties_kill_fodder_once_but_keep_each_identity() {
    let kernel = FirstExplosionKernel::conditional(&[point(10), point(10)], None, 0, 10).expect("kernel");

    assert_eq!(kernel.first_probability_at(10), 1.0);
    assert_eq!(kernel.first_probability_for_jack_at(0, 10), 1.0);
    assert_eq!(kernel.first_probability_for_jack_at(1, 10), 1.0);
    assert_eq!(kernel.survival_after(9), 1.0);
    assert_eq!(kernel.survival_after(10), 0.0);
}

#[test]
fn sparse_branch_view_resolves_same_frame_ties_by_native_order() {
    let points = [point(10), point(10)];
    let never = ExplosionPmf::from_subprobabilities(10, vec![0.0]).expect("never PMF");
    let sources = [
        SharedJackExplosionSource {
            always_hits: &points[0],
            blockable_hits: &never,
            frame_order: 4,
        },
        SharedJackExplosionSource {
            always_hits: &points[1],
            blockable_hits: &never,
            frame_order: 0,
        },
    ];
    let states = [
        BranchJackState {
            survival_at_start: 1.0,
            blockable_from: i32::MAX,
        },
        BranchJackState {
            survival_at_start: 1.0,
            blockable_from: i32::MAX,
        },
    ];
    let workspace = BranchExplosionWorkspace::new(&sources, &[]).expect("workspace");
    let candidate = workspace.candidate(10, 11, states.to_vec()).expect("candidate");
    let branch = candidate
        .branch(CapOneBranchInput {
            branch_scalar: 1.0,
            excluded_jack: None,
        })
        .expect("branch");

    assert_eq!(branch.first_probability_at(10), 1.0);
    assert_eq!(branch.first_probability_at_or_after_order(0, 10), 1.0);
    assert_eq!(branch.first_probability_at_or_after_order(1, 10), 0.0);
    assert_eq!(branch.first_probability_at_or_after_order(4, 10), 0.0);
    assert_eq!(
        branch
            .focused(0)
            .expect("later focus")
            .first_probability_at_or_after_focus(10),
        0.0
    );
    assert_eq!(
        branch
            .focused(1)
            .expect("earlier focus")
            .first_probability_at_or_after_focus(10),
        1.0
    );
}

#[test]
fn sparse_branch_view_matches_conditional_oracle_and_zero_safe_exclusions() {
    let pmfs = [
        ExplosionPmf::from_subprobabilities(10, vec![0.05, 0.1, 0.0]).expect("first PMF"),
        ExplosionPmf::from_subprobabilities(10, vec![0.0, 0.08, 0.04]).expect("second PMF"),
        ExplosionPmf::from_subprobabilities(10, vec![0.0, 0.0, 0.0]).expect("never PMF"),
    ];
    let never = ExplosionPmf::from_subprobabilities(10, vec![0.0]).expect("never blockable PMF");
    let sources = pmfs
        .iter()
        .enumerate()
        .map(|(index, pmf)| SharedJackExplosionSource {
            always_hits: pmf,
            blockable_hits: &never,
            frame_order: [4, 0, 2][index],
        })
        .collect::<Vec<_>>();
    let survivals = [0.8, 0.75, 0.0];
    let states = survivals.map(|survival_at_start| BranchJackState {
        survival_at_start,
        blockable_from: i32::MAX,
    });
    let workspace = BranchExplosionWorkspace::new(&sources, &[]).expect("workspace");
    let candidate = workspace.candidate(10, 13, states.to_vec()).expect("candidate");
    for excluded in [None, Some(0), Some(1), Some(2)] {
        let scalar = excluded.map_or(1.0, |_| 0.125);
        let branch = candidate
            .branch(CapOneBranchInput {
                branch_scalar: scalar,
                excluded_jack: excluded,
            })
            .expect("branch");
        let conditional_pmfs = pmfs
            .iter()
            .zip(survivals)
            .enumerate()
            .filter(|(index, _source)| Some(*index) != excluded)
            .filter(|(_index, (_pmf, survival))| *survival > 0.0)
            .map(|(_index, (pmf, survival))| {
                ExplosionPmf::from_subprobabilities(
                    10,
                    (10..=12).map(|time| pmf.probability_at(time) / survival).collect(),
                )
                .expect("conditional PMF")
            })
            .collect::<Vec<_>>();
        let conditional = FirstExplosionKernel::conditional(&conditional_pmfs, None, 10, 12).expect("conditional");
        let branch_mass = scalar
            * survivals
                .iter()
                .enumerate()
                .filter(|(index, _survival)| Some(*index) != excluded)
                .map(|(_index, survival)| survival)
                .product::<f64>();
        for time in 9..=12 {
            assert!(
                (branch.first_probability_at(time) - branch_mass * conditional.first_probability_at(time)).abs()
                    < 1.0e-12
            );
            assert!((branch.survival_after(time) - branch_mass * conditional.survival_after(time)).abs() < 1.0e-12);
        }
    }
}

#[test]
fn aggregate_branch_view_matches_sum_of_individual_branches() {
    let pmfs = [
        ExplosionPmf::from_subprobabilities(10, vec![0.05, 0.1, 0.0]).expect("first PMF"),
        ExplosionPmf::from_subprobabilities(10, vec![0.0, 0.08, 0.04]).expect("second PMF"),
        ExplosionPmf::from_subprobabilities(10, vec![0.0, 0.0, 0.0]).expect("never PMF"),
    ];
    let never = ExplosionPmf::from_subprobabilities(10, vec![0.0]).expect("never blockable PMF");
    let sources = pmfs
        .iter()
        .enumerate()
        .map(|(index, pmf)| SharedJackExplosionSource {
            always_hits: pmf,
            blockable_hits: &never,
            frame_order: [4, 0, 2][index],
        })
        .collect::<Vec<_>>();
    let terms = [
        CapOneBranchInput {
            branch_scalar: 0.7,
            excluded_jack: None,
        },
        CapOneBranchInput {
            branch_scalar: 0.2,
            excluded_jack: Some(0),
        },
        CapOneBranchInput {
            branch_scalar: 0.3,
            excluded_jack: Some(1),
        },
        CapOneBranchInput {
            branch_scalar: 0.4,
            excluded_jack: Some(2),
        },
    ];
    let close = |actual: f64, expected: f64| assert!((actual - expected).abs() < 1.0e-12);
    let close_tuple = |actual: (f64, f64, f64), expected: (f64, f64, f64)| {
        close(actual.0, expected.0);
        close(actual.1, expected.1);
        close(actual.2, expected.2);
    };

    for survivals in [[0.8, 0.75, 0.6], [0.8, 0.75, 0.0]] {
        let states = survivals.map(|survival_at_start| BranchJackState {
            survival_at_start,
            blockable_from: i32::MAX,
        });
        let workspace = BranchExplosionWorkspace::new(&sources, &[]).expect("workspace");
        let candidate = workspace.candidate(10, 13, states.to_vec()).expect("candidate");
        let branches = terms
            .iter()
            .copied()
            .map(|term| candidate.branch(term).expect("branch"))
            .collect::<Vec<_>>();
        let aggregate = candidate.aggregate(&terms, None);
        close(
            aggregate.branch_mass(),
            branches.iter().map(BranchProbabilityView::branch_mass).sum(),
        );
        for time in 9..=12 {
            close(
                aggregate.survival_after(time),
                branches.iter().map(|branch| branch.survival_after(time)).sum(),
            );
            for order in -1..=5 {
                let expected = branches.iter().fold((0.0, 0.0, 0.0), |mut sum, branch| {
                    let value = branch.probability_at_ordered_and_survival_after(order, time);
                    sum.0 += value.0;
                    sum.1 += value.1;
                    sum.2 += value.2;
                    sum
                });
                close_tuple(
                    aggregate.probability_at_ordered_and_survival_after(order, time),
                    expected,
                );
            }
        }
        for focus in 0..sources.len() {
            let aggregate = candidate.aggregate(&terms, Some(focus));
            for time in 9..=12 {
                let expected_survival = branches
                    .iter()
                    .filter_map(|branch| branch.focused(focus))
                    .map(|branch| branch.survival_after(time))
                    .sum();
                close(aggregate.survival_after(time), expected_survival);
                let expected = branches.iter().filter_map(|branch| branch.focused(focus)).fold(
                    (0.0, 0.0, 0.0),
                    |mut sum, branch| {
                        let value = branch.probability_at_ordered_and_survival_after_focus(time);
                        sum.0 += value.0;
                        sum.1 += value.1;
                        sum.2 += value.2;
                        sum
                    },
                );
                close_tuple(
                    aggregate.probability_at_ordered_and_survival_after_focus(time),
                    expected,
                );
            }
        }
    }
}

#[test]
fn excluded_cap_one_identity_is_removed_from_future_kernel() {
    let kernel = FirstExplosionKernel::conditional(&[point(4), point(7)], Some(0), 5, 8).expect("kernel");
    assert_eq!(kernel.first_probability_at(7), 1.0);
    assert_eq!(kernel.first_probability_for_jack_at(0, 7), 1.0);
    assert_eq!(kernel.first_probability_for_jack_at(1, 7), 0.0);
}

#[test]
fn branch_mass_retains_exactly_cap_one_probability() {
    let survival = 0.975;
    let before = 0.025;
    let never = ExplosionPmf::from_subprobabilities(10, vec![0.0]).expect("never PMF");
    let sources = (0..5)
        .map(|frame_order| SharedJackExplosionSource {
            always_hits: &never,
            blockable_hits: &never,
            frame_order,
        })
        .collect::<Vec<_>>();
    let states = vec![
        BranchJackState {
            survival_at_start: survival,
            blockable_from: i32::MAX,
        };
        5
    ];
    let workspace = BranchExplosionWorkspace::new(&sources, &[]).expect("workspace");
    let candidate = workspace.candidate(10, 11, states).expect("candidate");
    let mut retained = candidate
        .branch(CapOneBranchInput {
            branch_scalar: 1.0,
            excluded_jack: None,
        })
        .expect("none branch")
        .branch_mass();
    for excluded in 0..sources.len() {
        retained += candidate
            .branch(CapOneBranchInput {
                branch_scalar: before,
                excluded_jack: Some(excluded),
            })
            .expect("identity branch")
            .branch_mass();
    }
    let weights = cap_one_weights(
        &vec![ExplosionPmf::from_probabilities(0, vec![before, survival]).expect("PMF"); 5],
        1,
    );
    assert!((retained - weights.none_before - weights.only_before.iter().sum::<f64>()).abs() < 1.0e-12);
    assert!(weights.diagnostics.omitted_probability > 0.0);
}

impl FocusedBranchProbabilityView<'_, '_, '_> {
    pub fn first_probability_between(&self, start: i32, end: i32) -> f64 {
        if end <= start {
            return 0.0;
        }
        (self.branch.survival_before(start, Some(self.jack)) - self.branch.survival_before(end, Some(self.jack)))
            .max(0.0)
    }
    pub fn first_probability_at(&self, time: i32) -> f64 {
        self.first_probability_between(time, time.saturating_add(1))
    }
    pub const fn start(&self) -> i32 {
        self.branch.start()
    }

    pub fn first_probability_at_or_after_focus(&self, time: i32) -> f64 {
        self.probability_at_ordered_and_survival_after_focus(time).1
    }

    pub fn probability_at_ordered_and_survival_after_focus(&self, time: i32) -> (f64, f64, f64) {
        let position = self.branch.candidate.workspace.source_order_positions[self.jack].saturating_add(1);
        let (total, ordered, after_survival) =
            self.branch
                .probability_at_and_ordered_at(position, time, Some(self.jack));
        (total, ordered.clamp(0.0, total), after_survival)
    }

    pub fn survival_after(&self, time: i32) -> f64 {
        if time < self.start() {
            return self.branch.survival_before(self.start(), Some(self.jack));
        }
        self.branch.survival_before(time.saturating_add(1), Some(self.jack))
    }
}

impl BranchProbabilityView<'_, '_, '_> {
    pub fn first_probability_between(&self, start: i32, end: i32) -> f64 {
        if end <= start {
            return 0.0;
        }
        (self.survival_before(start, None) - self.survival_before(end, None)).max(0.0)
    }
    pub fn first_probability_at(&self, time: i32) -> f64 {
        self.first_probability_between(time, time.saturating_add(1))
    }
    pub const fn start(&self) -> i32 {
        self.candidate.start
    }

    pub fn branch_mass(&self) -> f64 {
        self.survival_before(self.start(), None)
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
        let (total, ordered, after_survival) = self.probability_at_and_ordered_at(position, time, None);
        (total, ordered.clamp(0.0, total), after_survival)
    }

    pub fn survival_after(&self, time: i32) -> f64 {
        if time < self.start() {
            return self.branch_mass();
        }
        self.survival_before(time.saturating_add(1), None)
    }

    pub fn focused(&self, jack: usize) -> Option<FocusedBranchProbabilityView<'_, '_, '_>> {
        (jack < self.candidate.workspace.jack_count && Some(jack) != self.excluded_jack)
            .then_some(FocusedBranchProbabilityView { branch: *self, jack })
    }

    pub(super) fn survival_before(&self, time: i32, focused_jack: Option<usize>) -> f64 {
        self.branch_scalar * self.candidate.product_without(time, self.excluded_jack, focused_jack)
    }

    pub(super) fn probability_at_and_ordered_at(
        &self, position: usize, time: i32, focused_jack: Option<usize>,
    ) -> (f64, f64, f64) {
        let (total, ordered, after_survival) =
            self.candidate
                .probability_at_and_ordered(position, time, self.excluded_jack, focused_jack);
        (
            self.branch_scalar * total,
            self.branch_scalar * ordered,
            self.branch_scalar * after_survival,
        )
    }
}

impl<'workspace, 'source> CandidateProbabilityCache<'workspace, 'source> {
    pub fn branch(&self, input: CapOneBranchInput) -> Option<BranchProbabilityView<'_, 'workspace, 'source>> {
        if !input.branch_scalar.is_finite()
            || input.branch_scalar < 0.0
            || input
                .excluded_jack
                .is_some_and(|jack| jack >= self.workspace.jack_count)
        {
            return None;
        }
        Some(BranchProbabilityView {
            candidate: self,
            branch_scalar: input.branch_scalar,
            excluded_jack: input.excluded_jack,
        })
    }
}

impl<'a> BranchExplosionWorkspace<'a> {
    pub fn candidate<'workspace>(
        &'workspace self, start: i32, end: i32, jack_states: Vec<BranchJackState>,
    ) -> Option<CandidateProbabilityCache<'workspace, 'a>> {
        self.candidate_with_storage(
            start,
            end,
            CandidateProbabilityStorage {
                jack_states,
                cache: None,
            },
        )
    }
}

#[derive(Clone, Copy)]
pub(crate) struct BranchProbabilityView<'candidate, 'workspace, 'source> {
    candidate: &'candidate CandidateProbabilityCache<'workspace, 'source>,
    branch_scalar: f64,
    excluded_jack: Option<usize>,
}
#[derive(Clone, Copy)]
pub(crate) struct FocusedBranchProbabilityView<'candidate, 'workspace, 'source> {
    branch: BranchProbabilityView<'candidate, 'workspace, 'source>,
    jack: usize,
}
