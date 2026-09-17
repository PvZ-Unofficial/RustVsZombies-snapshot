use super::*;

fn constant_tables(max_h: u16, release: f32, pole: f32, danger: f32) -> Vec<u8> {
    let h_count = usize::from(max_h) + 1;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RSVZSF01");
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&max_h.to_le_bytes());
    for _ in 0..4 {
        bytes.extend_from_slice(&620_f32.to_le_bytes());
        bytes.extend_from_slice(&3_f32.to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
    }
    bytes.extend(std::iter::repeat_n(release, 3 * 2 * h_count).flat_map(f32::to_le_bytes));
    bytes.extend(std::iter::repeat_n(pole, h_count).flat_map(f32::to_le_bytes));
    bytes.extend(std::iter::repeat_n(danger, 5 * 2 * h_count).flat_map(f32::to_le_bytes));
    bytes
}

fn linear_release_tables(max_h: u16) -> Vec<u8> {
    let h_count = usize::from(max_h) + 1;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RSVZSF01");
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&max_h.to_le_bytes());
    for _ in 0..4 {
        bytes.extend_from_slice(&620_f32.to_le_bytes());
        bytes.extend_from_slice(&3_f32.to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
    }
    for kind in 0..3 {
        for x_index in 0..2 {
            bytes.extend(
                (0..h_count)
                    .map(|horizon| horizon as f32 + (kind * 1_000 + x_index * 100) as f32)
                    .flat_map(f32::to_le_bytes),
            );
        }
    }
    bytes.extend(std::iter::repeat_n(0.0_f32, h_count).flat_map(f32::to_le_bytes));
    bytes.extend(std::iter::repeat_n(0.0_f32, 5 * 2 * h_count).flat_map(f32::to_le_bytes));
    bytes
}

fn test_release_values(tables: DamageTables<'_>, axis_start: i32, activation_at: i32) -> ReleaseValueWorkspace {
    let mut grids = Vec::new();
    for kind in [
        ReleaseTailKind::Ladder,
        ReleaseTailKind::Football,
        ReleaseTailKind::JackNoPop,
    ] {
        for x_index in 0..2 {
            for bite_check_residue in 0..BITE_PERIOD {
                let first_release = align_bite_check(axis_start, bite_check_residue);
                let released = (first_release..activation_at)
                    .step_by(usize::try_from(BITE_PERIOD).unwrap_or(1))
                    .map(|release_at| {
                        tables
                            .release_damage_grid(
                                kind,
                                x_index,
                                usize::try_from(activation_at - release_at).unwrap_or_default(),
                            )
                            .map(f64::from)
                            .expect("release value")
                    })
                    .collect();
                grids.push(AbsoluteReleaseGrid {
                    kind,
                    x_index,
                    bite_check_residue,
                    cutoff_threat: None,
                    first_release,
                    released,
                });
            }
        }
    }
    ReleaseValueWorkspace {
        grids,
        jack_baselines: Vec::new(),
        cutoff_keys: Vec::new(),
    }
}

#[test]
fn mod_eight_alignment_covers_every_residue_and_contact_endpoint() {
    for residue in 0..BITE_PERIOD {
        let aligned = align_bite_check(100, residue);
        assert!((100..=107).contains(&aligned));
        assert_eq!(aligned.rem_euclid(BITE_PERIOD), residue);
        let threat = SmartFodderThreat {
            update_rank: 0,
            trace_start: 90,
            x_trace: (90..=120).map(|time| time as f32).collect(),
            baseline_damage: 0.0,
            baseline_contact_at: None,
            bite_check_residue: Some(residue),
            ordinary_damage_enabled: true,
            release_kind: Some(ReleaseTailKind::Ladder),
            jack_pmf: None,
            pole_vault: false,
            fodder_contact_kind: FodderContactKind::Bite,
            can_contact_fodder: true,
            first_contact_at: Some(100),
            last_contact_at: Some(107),
            jack_blast_events: Vec::new(),
            jack_geometry_counts: [0; 5],
        };
        assert_eq!(threat.contact(99, 120), Some((aligned, aligned as f32)));
        let mut too_late = threat;
        too_late.last_contact_at = Some(aligned - 1);
        assert_eq!(too_late.contact(99, 120), None);
    }
}

#[test]
fn planting_cannot_retroactively_join_the_current_contact_check() {
    let bite = SmartFodderThreat {
        update_rank: 0,
        trace_start: 90,
        x_trace: (90..=120).map(|time| time as f32).collect(),
        baseline_damage: 0.0,
        baseline_contact_at: None,
        bite_check_residue: Some(4),
        ordinary_damage_enabled: true,
        release_kind: Some(ReleaseTailKind::Ladder),
        jack_pmf: None,
        pole_vault: false,
        fodder_contact_kind: FodderContactKind::Bite,
        can_contact_fodder: true,
        first_contact_at: Some(90),
        last_contact_at: Some(108),
        jack_blast_events: Vec::new(),
        jack_geometry_counts: [0; 5],
    };
    assert_eq!(bite.contact(100, 120), Some((108, 108.0)));
}

#[test]
fn mod_eight_invulnerability_and_release_endpoints_use_real_checks() {
    for residue in 0..BITE_PERIOD {
        assert_eq!(
            deterministic_fodder_end(
                4,
                FodderBehavior::Biteable,
                103,
                None,
                None,
                130,
                [(100, FodderContactKind::Bite, Some(residue), 0)].into_iter(),
            )
            .time,
            align_bite_check(103, residue)
        );
    }

    let bytes = linear_release_tables(40);
    let tables = DamageTables::parse(&bytes).expect("tables");
    let release_values = test_release_values(tables, 0, 32);
    let query = |residue| ReleaseQuery {
        left_grid: release_values.find(ReleaseTailKind::Ladder, 0, residue, None),
        right_grid: None,
    };
    assert_eq!(interpolated_release_value(&release_values, query(0), 0, 24, 32), 8.0);
    assert_eq!(interpolated_release_value(&release_values, query(0), 0, 31, 32), 0.0);
    assert_eq!(interpolated_release_value(&release_values, query(0), 0, 32, 32), 0.0);
    assert_eq!(interpolated_release_value(&release_values, query(7), 7, 31, 32), 1.0);
}

#[test]
fn truncated_ordinary_grid_keeps_the_delayed_same_frame_release() {
    let bytes = linear_release_tables(64);
    let tables = DamageTables::parse(&bytes).expect("tables");
    let release_values = test_release_values(tables, 0, 64);
    let query = ReleaseQuery {
        left_grid: release_values.find(ReleaseTailKind::Ladder, 0, 0, None),
        right_grid: None,
    };

    // A death at check 32 can be ordered before this eater's check. The
    // delayed case queries death 33, whose next release is check 40.
    assert_eq!(interpolated_release_value(&release_values, query, 0, 33, 64), 24.0);
}

#[test]
fn sparse_eight_cs_score_matches_per_cs_oracle_and_interpolation() {
    let bytes = linear_release_tables(40);
    let tables = DamageTables::parse(&bytes).expect("tables");
    let release_values = test_release_values(tables, 0, 32);
    let mut probabilities = vec![0.0; 32];
    for (time, probability) in [(0, 0.05), (3, 0.1), (7, 0.15), (8, 0.2), (15, 0.1), (30, 0.1)] {
        probabilities[time] = probability;
    }
    let pmf = ExplosionPmf::from_subprobabilities(0, probabilities).expect("event PMF");
    let never = ExplosionPmf::from_subprobabilities(0, vec![0.0]).expect("never PMF");
    let source = [SharedJackExplosionSource {
        always_hits: &pmf,
        blockable_hits: &never,
        frame_order: zombie_before_check_order(0),
    }];
    let states = [BranchJackState {
        survival_at_start: 1.0,
        blockable_from: i32::MAX,
    }];
    let workspace = BranchExplosionWorkspace::new(&source, &[]).expect("workspace");
    let candidate = workspace.candidate(0, 32, states.to_vec()).expect("candidate");
    let branch = candidate
        .branch(CapOneBranchInput {
            branch_scalar: 1.0,
            excluded_jack: None,
        })
        .expect("branch");
    let threat = SmartFodderThreat {
        update_rank: 0,
        trace_start: 0,
        x_trace: Vec::new(),
        baseline_damage: 0.0,
        baseline_contact_at: None,
        bite_check_residue: Some(0),
        ordinary_damage_enabled: true,
        release_kind: Some(ReleaseTailKind::Ladder),
        jack_pmf: None,
        pole_vault: false,
        fodder_contact_kind: FodderContactKind::Bite,
        can_contact_fodder: true,
        first_contact_at: Some(0),
        last_contact_at: Some(31),
        jack_blast_events: Vec::new(),
        jack_geometry_counts: [0; 5],
    };
    let expected_at = |x_bonus: f64| {
        let before = (0..19)
            .map(|death_at| {
                let release_at = align_bite_check(death_at, 0);
                let damage = if death_at == 0 {
                    0.0
                } else {
                    f64::from(32 - release_at) + x_bonus
                };
                branch.first_probability_at(death_at) * damage
            })
            .sum::<f64>();
        let release_at = align_bite_check(19, 0);
        before + branch.survival_after(18) * (f64::from(32 - release_at) + x_bonus)
    };
    let expected = f64::midpoint(expected_at(0.0), expected_at(100.0));
    let actual = ordinary_damage(
        &release_values,
        &branch,
        ReleaseQuery {
            left_grid: release_values.find(ReleaseTailKind::Ladder, 0, 0, None),
            right_grid: release_values
                .find(ReleaseTailKind::Ladder, 1, 0, None)
                .map(|right| (right, 0.5)),
        },
        &threat,
        0,
        DeterministicFodderEnd {
            time: 19,
            frame_order: None,
        },
        32,
    )
    .expect("ordinary score");
    assert!((actual - expected).abs() < 1.0e-10);
}

#[test]
fn full_branch_scores_match_joint_outcome_oracle_for_every_f_r_b() {
    let bytes = linear_release_tables(20);
    let tables = DamageTables::parse(&bytes).expect("tables");
    let jack_pmfs = [
        ExplosionPmf::from_probabilities(0, vec![0.2, 0.0, 0.3, 0.0, 0.5]).expect("jack 0"),
        ExplosionPmf::from_probabilities(1, vec![0.25, 0.0, 0.25, 0.0, 0.5]).expect("jack 1"),
    ];
    let jack = |update_rank, pmf: ExplosionPmf| SmartFodderThreat {
        update_rank,
        trace_start: 0,
        x_trace: vec![800.0; 13],
        baseline_damage: 0.0,
        baseline_contact_at: None,
        bite_check_residue: None,
        ordinary_damage_enabled: false,
        release_kind: None,
        jack_pmf: Some(pmf.clone()),
        pole_vault: false,
        fodder_contact_kind: FodderContactKind::Bite,
        can_contact_fodder: false,
        first_contact_at: None,
        last_contact_at: None,
        jack_blast_events: (pmf.start()..=pmf.end())
            .filter_map(|explosion_at| {
                let probability = pmf.probability_at(explosion_at);
                (probability > 0.0).then_some(JackBlastEvent {
                    explosion_at,
                    probability,
                    baseline_hits: 0.0,
                    hits_fodder_without_block: true,
                })
            })
            .collect(),
        jack_geometry_counts: [0; 5],
    };
    let model = SmartFodderModel {
        domain: SmartFodderDomain {
            now: -1,
            plant_candidates: 2..=4,
            remove_by: None,
            activation_at: 12,
        },
        fodder_hp: 300,
        fodder_behavior: FodderBehavior::Biteable,
        fodder_morph: None,
        fodder_invulnerable_for: 0,
        threats: vec![
            SmartFodderThreat {
                update_rank: 0,
                trace_start: 0,
                x_trace: vec![620.0; 13],
                baseline_damage: 300.0,
                baseline_contact_at: Some(0),
                bite_check_residue: Some(3),
                ordinary_damage_enabled: true,
                release_kind: Some(ReleaseTailKind::Ladder),
                jack_pmf: None,
                pole_vault: false,
                fodder_contact_kind: FodderContactKind::Bite,
                can_contact_fodder: true,
                first_contact_at: Some(2),
                last_contact_at: Some(11),
                jack_blast_events: Vec::new(),
                jack_geometry_counts: [0; 5],
            },
            jack(1, jack_pmfs[0].clone()),
            jack(2, jack_pmfs[1].clone()),
        ],
        deterministic_explosions: Vec::new(),
        deterministic_jack_blast_damage: 0.0,
    };
    let release_values = model.prepare_release_values(tables).expect("release workspace");
    let jack_blast_axes = model.prepare_jack_blast_axes().expect("blast axes");
    let shared_sources = jack_blast_axes
        .iter()
        .enumerate()
        .map(|(index, axes)| SharedJackExplosionSource {
            always_hits: &axes.always_hits,
            blockable_hits: &axes.blockable_hits,
            frame_order: zombie_before_check_order(index + 1),
        })
        .collect::<Vec<_>>();
    let workspace = BranchExplosionWorkspace::new(&shared_sources, &[]).expect("workspace");
    let outcomes = jack_pmfs
        .iter()
        .map(|pmf| {
            (pmf.start()..=pmf.end())
                .filter_map(|time| {
                    let probability = pmf.probability_at(time);
                    (probability > 0.0).then_some((time, probability))
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    for plant_at in model.domain.plant_candidates.clone() {
        let contacts = model
            .threats
            .iter()
            .map(|threat| threat.contact(plant_at, model.domain.activation_at))
            .collect::<Vec<_>>();
        let lifecycle = jack_pmfs.to_vec();
        let survivals = lifecycle
            .iter()
            .map(|pmf| 1.0 - pmf.probability_before(plant_at))
            .collect::<Vec<_>>();
        let states = survivals
            .iter()
            .map(|survival_at_start| BranchJackState {
                survival_at_start: *survival_at_start,
                blockable_from: i32::MAX,
            })
            .collect::<Vec<_>>();
        let candidate = workspace
            .candidate(plant_at, model.domain.activation_at, states)
            .expect("candidate");
        let release_queries = model
            .prepare_release_queries(tables, &release_values, &contacts)
            .expect("release queries");
        for excluded in [None, Some(0), Some(1)] {
            let scalar = excluded.map_or(1.0, |index| lifecycle[index].probability_before(plant_at));
            let branch_mass = scalar
                * survivals
                    .iter()
                    .enumerate()
                    .filter(|(index, _survival)| Some(*index) != excluded)
                    .map(|(_index, survival)| survival)
                    .product::<f64>();
            if branch_mass <= 0.0 {
                continue;
            }
            let branch = candidate
                .branch(CapOneBranchInput {
                    branch_scalar: scalar,
                    excluded_jack: excluded,
                })
                .expect("branch");
            for remove_at in [None, Some(plant_at), Some(plant_at + 2), Some(10)] {
                let mut score = [0.0];
                model
                    .accumulate_branch_scores(
                        tables,
                        &release_values,
                        &contacts,
                        &release_queries,
                        &[None, None, None],
                        &jack_blast_axes,
                        excluded,
                        &[remove_at],
                        &branch,
                        &mut score,
                    )
                    .expect("branch score");
                let actual = score[0];
                let contact_at = contacts[0].expect("ladder contact").0;
                let deterministic_end = remove_at.unwrap_or(model.domain.activation_at);
                let mut expected = 0.0;
                for (time_0, probability_0) in &outcomes[0] {
                    for (time_1, probability_1) in &outcomes[1] {
                        let times = [*time_0, *time_1];
                        let retained = match excluded {
                            None => times.iter().all(|time| *time >= plant_at),
                            Some(excluded) => {
                                times[excluded] < plant_at
                                    && times
                                        .iter()
                                        .enumerate()
                                        .all(|(index, time)| index == excluded || *time >= plant_at)
                            }
                        };
                        if !retained {
                            continue;
                        }
                        let first_blast = times
                            .iter()
                            .enumerate()
                            .filter(|(index, _time)| Some(*index) != excluded)
                            .map(|(index, time)| (*time, zombie_before_check_order(index + 1)))
                            .min();
                        let deterministic_event =
                            (deterministic_end, remove_at.map_or(i32::MAX, |_| BEFORE_ZOMBIE_UPDATES));
                        let (death_at, death_order) =
                            first_blast.map_or(deterministic_event, |blast| blast.min(deterministic_event));
                        let bite_order = zombie_bite_check_order(0);
                        let damage = if death_at < contact_at || (death_at == contact_at && death_order < bite_order) {
                            300.0
                        } else {
                            let release_at = align_bite_check(
                                death_at.saturating_add(i32::from(
                                    death_at.rem_euclid(BITE_PERIOD) == 3 && death_order >= bite_order,
                                )),
                                3,
                            );
                            if release_at < model.domain.activation_at {
                                f64::from(model.domain.activation_at - release_at)
                            } else {
                                0.0
                            }
                        };
                        expected += probability_0 * probability_1 * damage;
                    }
                }
                assert!(
                    (actual - expected).abs() < 1.0e-10,
                    "F={plant_at} R={remove_at:?} b={excluded:?}: {actual} != {expected}"
                );
            }
        }
    }
}

#[test]
fn natural_death_uses_joint_bite_rate_without_frame_scan() {
    assert_eq!(
        deterministic_fodder_end(
            12,
            FodderBehavior::Biteable,
            0,
            None,
            None,
            100,
            [
                (10, FodderContactKind::Bite, None, 0),
                (18, FodderContactKind::Bite, None, 1),
            ]
            .into_iter(),
        )
        .time,
        18
    );
    assert_eq!(
        deterministic_fodder_end(
            300,
            FodderBehavior::Biteable,
            0,
            None,
            Some(20),
            100,
            [(10, FodderContactKind::Bite, None, 0)].into_iter(),
        )
        .time,
        20
    );
}

#[test]
fn fatal_bite_releases_only_eaters_that_already_updated_that_frame() {
    let end = deterministic_fodder_end(
        4,
        FodderBehavior::Biteable,
        0,
        None,
        None,
        16,
        [
            (8, FodderContactKind::Bite, None, 0),
            (8, FodderContactKind::Bite, None, 1),
        ]
        .into_iter(),
    );
    assert_eq!(
        end,
        DeterministicFodderEnd {
            time: 8,
            frame_order: Some(zombie_bite_check_order(0)),
        }
    );

    let correction_for = |update_rank| {
        deterministic_order_correction(
            |_time| 0.0,
            |_time| 1.0,
            0,
            zombie_bite_check_order(update_rank),
            end,
            |_time| 8.0,
            |_time| 0.0,
            0.0,
            16,
        )
    };
    assert_eq!(correction_for(0), -8.0);
    assert_eq!(correction_for(1), 0.0);
}

#[test]
fn blover_ignores_bites_and_vehicle_crush_but_not_gargantuar_smash() {
    let bites_and_crush = [
        (10, FodderContactKind::Bite, None, 0),
        (20, FodderContactKind::Crush, None, 1),
    ];
    assert_eq!(
        deterministic_fodder_end(
            300,
            FodderBehavior::Blover,
            0,
            None,
            None,
            1_000,
            bites_and_crush.into_iter(),
        )
        .time,
        BLOVER_LIFETIME
    );
    assert_eq!(
        deterministic_fodder_end(
            300,
            FodderBehavior::Blover,
            0,
            None,
            None,
            1_000,
            [(10, FodderContactKind::Smash { impact_after: 100 }, None, 0)].into_iter(),
        )
        .time,
        110
    );
}

#[test]
fn imitator_blover_is_crushable_only_before_morph() {
    let morph = Some((320, 300, FodderBehavior::Blover, 0));
    assert_eq!(
        deterministic_fodder_end(
            300,
            FodderBehavior::Biteable,
            0,
            morph,
            None,
            1_000,
            [(100, FodderContactKind::Crush, None, 0)].into_iter(),
        )
        .time,
        100
    );
    assert_eq!(
        deterministic_fodder_end(
            300,
            FodderBehavior::Biteable,
            0,
            morph,
            None,
            1_000,
            [(400, FodderContactKind::Crush, None, 0)].into_iter(),
        )
        .time,
        570
    );
}

#[test]
fn one_shot_contact_preempts_an_earlier_computed_bite_death() {
    assert_eq!(
        deterministic_fodder_end(
            8,
            FodderBehavior::Biteable,
            0,
            Some((20, 300, FodderBehavior::Biteable, 0)),
            None,
            100,
            [
                (5, FodderContactKind::Crush, None, 0),
                (10, FodderContactKind::Bite, None, 1),
            ]
            .into_iter(),
        )
        .time,
        5
    );
}

#[test]
fn fft_suffix_correlation_matches_explicit_dot_products() {
    let left = [0.1, 0.2, 0.3, 0.4, 0.0];
    let right = [1.0, 0.5, 0.25, 0.0, 0.75];
    let actual = suffix_correlation(&left, &right);
    let expected = (0..left.len())
        .map(|death| {
            left[death..]
                .iter()
                .zip(right)
                .map(|(left, right)| left * right)
                .sum::<f64>()
        })
        .collect::<Vec<_>>();
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 1.0e-5, "{actual} != {expected}");
    }
}

#[test]
fn imitator_morph_resets_damage_only_if_placeholder_survives() {
    assert_eq!(
        deterministic_fodder_end(
            8,
            FodderBehavior::Biteable,
            0,
            Some((20, 100, FodderBehavior::Biteable, 0)),
            None,
            50,
            [(10, FodderContactKind::Bite, None, 0)].into_iter(),
        )
        .time,
        18
    );
    assert_eq!(
        deterministic_fodder_end(
            12,
            FodderBehavior::Biteable,
            0,
            Some((20, 12, FodderBehavior::Biteable, 0)),
            None,
            50,
            [(10, FodderContactKind::Bite, None, 0)].into_iter(),
        )
        .time,
        36
    );
    assert_eq!(
        deterministic_fodder_end(
            300,
            FodderBehavior::Biteable,
            0,
            Some((20, 4, FodderBehavior::Biteable, 100)),
            None,
            150,
            [(10, FodderContactKind::Bite, None, 0)].into_iter(),
        )
        .time,
        120
    );
}

#[test]
fn initial_invulnerability_delays_the_first_effective_bite() {
    assert_eq!(
        deterministic_fodder_end(
            4,
            FodderBehavior::Biteable,
            110,
            None,
            None,
            150,
            [(10, FodderContactKind::Bite, None, 0)].into_iter(),
        )
        .time,
        110
    );
}

#[test]
fn direct_feedback_is_conditioned_without_deduplicating_the_baseline_hit() {
    let bytes = constant_tables(130, 0.0, 0.0, 1.0);
    let tables = DamageTables::parse(&bytes).expect("tables");
    let point = ExplosionPmf::from_probabilities(125, vec![1.0]).expect("point PMF");
    let model = SmartFodderModel {
        domain: SmartFodderDomain {
            now: -1,
            plant_candidates: 0..=0,
            remove_by: None,
            activation_at: 130,
        },
        fodder_hp: 300,
        fodder_behavior: FodderBehavior::Biteable,
        fodder_morph: None,
        fodder_invulnerable_for: 0,
        threats: vec![SmartFodderThreat {
            update_rank: 0,
            trace_start: 0,
            x_trace: vec![620.0; 131],
            baseline_damage: 0.0,
            baseline_contact_at: None,
            bite_check_residue: Some(0),
            ordinary_damage_enabled: true,
            release_kind: Some(ReleaseTailKind::JackNoPop),
            jack_pmf: Some(point),
            pole_vault: false,
            fodder_contact_kind: FodderContactKind::Bite,
            can_contact_fodder: true,
            first_contact_at: Some(0),
            last_contact_at: Some(130),
            jack_blast_events: vec![JackBlastEvent {
                explosion_at: 125,
                probability: 1.0,
                baseline_hits: 0.0,
                hits_fodder_without_block: true,
            }],
            jack_geometry_counts: [1, 0, 0, 0, 0],
        }],
        deterministic_explosions: Vec::new(),
        deterministic_jack_blast_damage: 0.0,
    };

    let choice = model.solve(tables).expect("choice");
    assert!((choice.expected_damage - 300.0).abs() < 1.0e-9);
}

#[test]
fn exploding_jack_contributes_no_post_explosion_ordinary_tail() {
    let bytes = linear_release_tables(40);
    let tables = DamageTables::parse(&bytes).expect("tables");
    let point = ExplosionPmf::from_probabilities(20, vec![1.0]).expect("point PMF");
    let model = SmartFodderModel {
        domain: SmartFodderDomain {
            now: -1,
            plant_candidates: 0..=0,
            remove_by: None,
            activation_at: 32,
        },
        fodder_hp: 300,
        fodder_behavior: FodderBehavior::Biteable,
        fodder_morph: None,
        fodder_invulnerable_for: 0,
        threats: vec![SmartFodderThreat {
            update_rank: 0,
            trace_start: 0,
            x_trace: vec![620.0; 33],
            baseline_damage: 0.0,
            baseline_contact_at: None,
            bite_check_residue: Some(0),
            ordinary_damage_enabled: true,
            release_kind: Some(ReleaseTailKind::JackNoPop),
            jack_pmf: Some(point),
            pole_vault: false,
            fodder_contact_kind: FodderContactKind::Bite,
            can_contact_fodder: true,
            first_contact_at: Some(0),
            last_contact_at: Some(31),
            jack_blast_events: vec![JackBlastEvent {
                explosion_at: 20,
                probability: 1.0,
                baseline_hits: 0.0,
                hits_fodder_without_block: true,
            }],
            jack_geometry_counts: [0; 5],
        }],
        deterministic_explosions: Vec::new(),
        deterministic_jack_blast_damage: 0.0,
    };

    assert_eq!(model.solve(tables).expect("choice").expected_damage, 0.0);
}

#[test]
fn jack_released_by_another_explosion_stops_its_tail_at_its_own_explosion() {
    let bytes = linear_release_tables(200);
    let tables = DamageTables::parse(&bytes).expect("tables");
    let model = SmartFodderModel {
        domain: SmartFodderDomain {
            now: -1,
            plant_candidates: 0..=0,
            remove_by: None,
            activation_at: 200,
        },
        fodder_hp: 300,
        fodder_behavior: FodderBehavior::Biteable,
        fodder_morph: None,
        fodder_invulnerable_for: 0,
        threats: vec![SmartFodderThreat {
            update_rank: 1,
            trace_start: 0,
            x_trace: vec![620.0; 201],
            baseline_damage: 0.0,
            baseline_contact_at: None,
            bite_check_residue: Some(4),
            ordinary_damage_enabled: true,
            release_kind: Some(ReleaseTailKind::JackNoPop),
            jack_pmf: Some(ExplosionPmf::from_probabilities(160, vec![1.0]).expect("point PMF")),
            pole_vault: false,
            fodder_contact_kind: FodderContactKind::Bite,
            can_contact_fodder: true,
            first_contact_at: Some(5),
            last_contact_at: Some(199),
            jack_blast_events: vec![JackBlastEvent {
                explosion_at: 160,
                probability: 1.0,
                baseline_hits: 0.0,
                hits_fodder_without_block: true,
            }],
            jack_geometry_counts: [0; 5],
        }],
        deterministic_explosions: vec![OrderedExplosionPmf {
            pmf: ExplosionPmf::from_probabilities(20, vec![1.0]).expect("point PMF"),
            update_rank: 0,
        }],
        deterministic_jack_blast_damage: 0.0,
    };

    // JackNoPop x=620 is 2000+h in this fixture. Release at 20 and
    // own explosion at 160 therefore retain exactly h=140.
    assert!((model.solve(tables).expect("choice").expected_damage - 2_140.0).abs() < 1.0e-8);
}

#[test]
fn off_row_jack_that_misses_fodder_does_not_release_eaters() {
    let bytes = constant_tables(130, 100.0, 0.0, 0.0);
    let tables = DamageTables::parse(&bytes).expect("tables");
    let point = ExplosionPmf::from_probabilities(125, vec![1.0]).expect("point PMF");
    let model = SmartFodderModel {
        domain: SmartFodderDomain {
            now: -1,
            plant_candidates: 0..=0,
            remove_by: None,
            activation_at: 130,
        },
        fodder_hp: 300,
        fodder_behavior: FodderBehavior::Biteable,
        fodder_morph: None,
        fodder_invulnerable_for: 0,
        threats: vec![SmartFodderThreat {
            update_rank: 0,
            trace_start: 0,
            x_trace: vec![800.0; 131],
            baseline_damage: 0.0,
            baseline_contact_at: None,
            bite_check_residue: None,
            ordinary_damage_enabled: false,
            release_kind: None,
            jack_pmf: Some(point),
            pole_vault: false,
            fodder_contact_kind: FodderContactKind::Bite,
            can_contact_fodder: false,
            first_contact_at: None,
            last_contact_at: None,
            jack_blast_events: vec![JackBlastEvent {
                explosion_at: 125,
                probability: 1.0,
                baseline_hits: 0.0,
                hits_fodder_without_block: false,
            }],
            jack_geometry_counts: [0; 5],
        }],
        deterministic_explosions: Vec::new(),
        deterministic_jack_blast_damage: 0.0,
    };

    assert_eq!(model.solve(tables).expect("choice").expected_damage, 0.0);
}

#[test]
fn direct_convolution_matches_explicit_event_sum() {
    let bytes = constant_tables(130, 0.0, 0.0, 1.0);
    let tables = DamageTables::parse(&bytes).expect("tables");
    let point = ExplosionPmf::from_probabilities(125, vec![1.0]).expect("point PMF");
    let model = SmartFodderModel {
        domain: SmartFodderDomain {
            now: -1,
            plant_candidates: 0..=0,
            remove_by: None,
            activation_at: 130,
        },
        fodder_hp: 300,
        fodder_behavior: FodderBehavior::Biteable,
        fodder_morph: None,
        fodder_invulnerable_for: 0,
        threats: vec![SmartFodderThreat {
            update_rank: 0,
            trace_start: 0,
            x_trace: vec![620.0; 131],
            baseline_damage: 0.0,
            baseline_contact_at: None,
            bite_check_residue: Some(0),
            ordinary_damage_enabled: true,
            release_kind: Some(ReleaseTailKind::JackNoPop),
            jack_pmf: Some(point),
            pole_vault: false,
            fodder_contact_kind: FodderContactKind::Bite,
            can_contact_fodder: true,
            first_contact_at: Some(5),
            last_contact_at: Some(130),
            jack_blast_events: vec![
                JackBlastEvent {
                    explosion_at: 113,
                    probability: 0.2,
                    baseline_hits: 0.25,
                    hits_fodder_without_block: false,
                },
                JackBlastEvent {
                    explosion_at: 120,
                    probability: 0.3,
                    baseline_hits: 0.5,
                    hits_fodder_without_block: true,
                },
                JackBlastEvent {
                    explosion_at: 125,
                    probability: 0.5,
                    baseline_hits: 0.2,
                    hits_fodder_without_block: true,
                },
            ],
            jack_geometry_counts: [1, 0, 0, 0, 0],
        }],
        deterministic_explosions: Vec::new(),
        deterministic_jack_blast_damage: 0.0,
    };
    let masks = model.direct_x_masks(tables).expect("x masks");
    let convolution = model
        .prepare_direct_convolutions(tables, &masks)
        .expect("convolutions")
        .into_iter()
        .next()
        .flatten()
        .expect("jack convolution");
    assert!(convolution.future_hits[0].is_some());
    assert!(convolution.future_hits[1].is_none());
    let full = model
        .prepare_direct_convolutions(tables, &[0b11])
        .expect("full-grid convolutions")
        .into_iter()
        .next()
        .flatten()
        .expect("full-grid jack convolution");
    assert!(full.future_hits[1].is_some());
    let expected = 0.3 * (1.0 - 0.5) + 0.5 * (1.0 - 0.2);
    assert!((convolution.correction(5, 12, 12, (0, None)) - expected).abs() < 1.0e-5);
    assert!((convolution.correction(5, 12, 12, (0, None)) - full.correction(5, 12, 12, (0, None))).abs() < 1.0e-12);

    let demand_choice = model.solve(tables).expect("demand-grid choice");
    let full_choice = model
        .solve_with_direct_x_masks(tables, &[0b11])
        .expect("full-grid choice");
    assert_eq!(demand_choice.plant_at, full_choice.plant_at);
    assert_eq!(demand_choice.remove_at, full_choice.remove_at);
    assert!((demand_choice.expected_damage - full_choice.expected_damage).abs() < 1.0e-12);
}

#[test]
fn direct_feedback_release_uses_the_jacks_next_bite_check() {
    let bytes = constant_tables(16, 0.0, 0.0, 0.0);
    let tables = DamageTables::parse(&bytes).expect("tables");
    let point = ExplosionPmf::from_probabilities(16, vec![1.0]).expect("point PMF");
    let model = SmartFodderModel {
        domain: SmartFodderDomain {
            now: -1,
            plant_candidates: 0..=0,
            remove_by: None,
            activation_at: 16,
        },
        fodder_hp: 300,
        fodder_behavior: FodderBehavior::Biteable,
        fodder_morph: None,
        fodder_invulnerable_for: 0,
        threats: vec![SmartFodderThreat {
            update_rank: 0,
            trace_start: 0,
            x_trace: vec![620.0; 17],
            baseline_damage: 0.0,
            baseline_contact_at: None,
            bite_check_residue: Some(7),
            ordinary_damage_enabled: true,
            release_kind: Some(ReleaseTailKind::JackNoPop),
            jack_pmf: Some(point),
            pole_vault: false,
            fodder_contact_kind: FodderContactKind::Bite,
            can_contact_fodder: true,
            first_contact_at: Some(0),
            last_contact_at: Some(16),
            jack_blast_events: Vec::new(),
            jack_geometry_counts: [1, 0, 0, 0, 0],
        }],
        deterministic_explosions: Vec::new(),
        deterministic_jack_blast_damage: 0.0,
    };
    let convolution = JackDirectConvolution {
        axis_start: 0,
        time_count: 17,
        danger_at_zero: vec![0.0; 2],
        future_hits: vec![Some((0..17).map(f64::from).collect()), None],
        pop_prefix: vec![0.0; 18],
        baseline_suffix: vec![0.0; 18],
    };

    let corrections = model
        .prepare_direct_corrections(tables, 0, 16, &[Some((0, 620.0))], &[Some(convolution)])
        .expect("direct corrections");
    let values = corrections[0].as_ref().expect("jack corrections");

    // A fodder death at t=1 is not observed by this Jack until its t=7
    // bite check, so the direct-position kernel must start at index 7.
    assert_eq!(values.correction_at(1), 7.0);
}

#[test]
fn direct_eight_cs_buckets_match_the_per_cs_formula() {
    let pmf = |events: &[(usize, f64)]| {
        let mut probabilities = vec![0.0; 32];
        for (time, probability) in events {
            probabilities[*time] = *probability;
        }
        ExplosionPmf::from_subprobabilities(0, probabilities).expect("event PMF")
    };
    let pmfs = [
        pmf(&[(8, 0.1), (16, 0.1)]),
        pmf(&[(5, 0.1), (8, 0.1), (17, 0.1)]),
        pmf(&[(8, 0.15), (14, 0.1), (24, 0.1)]),
    ];
    let never = ExplosionPmf::from_subprobabilities(0, vec![0.0]).expect("never PMF");
    let sources = [
        SharedJackExplosionSource {
            always_hits: &pmfs[0],
            blockable_hits: &never,
            frame_order: 1,
        },
        SharedJackExplosionSource {
            always_hits: &pmfs[1],
            blockable_hits: &never,
            frame_order: 0,
        },
        SharedJackExplosionSource {
            always_hits: &pmfs[2],
            blockable_hits: &never,
            frame_order: 2,
        },
    ];
    let states = [BranchJackState {
        survival_at_start: 1.0,
        blockable_from: i32::MAX,
    }; 3];
    let workspace = BranchExplosionWorkspace::new(&sources, &[]).expect("workspace");
    let candidate = workspace.candidate(0, 32, states.to_vec()).expect("candidate");
    let branch = candidate
        .branch(CapOneBranchInput {
            branch_scalar: 1.0,
            excluded_jack: None,
        })
        .expect("branch");
    let focused = branch.focused(0).expect("focused branch");
    let corrections = DirectCorrectionSeries {
        contact_at: 8,
        bite_frame_order: 0,
        bite_check_residue: 0,
        x_query: (0, None),
        first_release: 0,
        correction_by_release: vec![1.0, 2.0, 4.0, 8.0, 16.0, 32.0],
    };
    for end in 0..=32 {
        let expected = (0..end)
            .map(|time| {
                let value = corrections.correction_at(time);
                let total_mass = focused.first_probability_at(time);
                let mut weighted = total_mass * value;
                if time >= corrections.contact_at && time.rem_euclid(BITE_PERIOD) == corrections.bite_check_residue {
                    let after_mass = focused.first_probability_at_or_after_focus(time).clamp(0.0, total_mass);
                    weighted += after_mass * (corrections.delayed_correction_at(time) - value);
                    if time == corrections.contact_at {
                        weighted -= (total_mass - after_mass) * value;
                    }
                }
                weighted
            })
            .sum::<f64>();
        let actual = direct_weighted_before(&focused, &corrections, end);
        assert!((actual - expected).abs() < 1.0e-12, "end={end}: {actual} != {expected}");
    }
}

#[test]
fn target_row_without_cannon_keeps_ordinary_damage_at_zero() {
    let bytes = constant_tables(20, 100.0, 100.0, 0.0);
    let tables = DamageTables::parse(&bytes).expect("tables");
    let model = SmartFodderModel {
        domain: SmartFodderDomain {
            now: -1,
            plant_candidates: 0..=0,
            remove_by: None,
            activation_at: 20,
        },
        fodder_hp: 300,
        fodder_behavior: FodderBehavior::Biteable,
        fodder_morph: None,
        fodder_invulnerable_for: 0,
        threats: vec![SmartFodderThreat {
            update_rank: 0,
            trace_start: 0,
            x_trace: vec![620.0; 21],
            baseline_damage: 0.0,
            baseline_contact_at: None,
            bite_check_residue: Some(0),
            ordinary_damage_enabled: false,
            release_kind: Some(ReleaseTailKind::Ladder),
            jack_pmf: None,
            pole_vault: false,
            fodder_contact_kind: FodderContactKind::Bite,
            can_contact_fodder: true,
            first_contact_at: Some(0),
            last_contact_at: Some(20),
            jack_blast_events: Vec::new(),
            jack_geometry_counts: [0; 5],
        }],
        deterministic_explosions: Vec::new(),
        deterministic_jack_blast_damage: 0.0,
    };

    assert_eq!(model.solve(tables).expect("choice").expected_damage, 0.0);
}

#[test]
fn ordinary_zero_release_tail_needs_no_monte_carlo_table() {
    let bytes = constant_tables(20, 100.0, 100.0, 0.0);
    let tables = DamageTables::parse(&bytes).expect("tables");
    let model = SmartFodderModel {
        domain: SmartFodderDomain {
            now: -1,
            plant_candidates: 0..=0,
            remove_by: None,
            activation_at: 20,
        },
        fodder_hp: 300,
        fodder_behavior: FodderBehavior::Biteable,
        fodder_morph: None,
        fodder_invulnerable_for: 0,
        threats: vec![SmartFodderThreat {
            update_rank: 0,
            trace_start: 0,
            x_trace: vec![680.0; 21],
            baseline_damage: 100.0,
            baseline_contact_at: Some(0),
            bite_check_residue: Some(0),
            ordinary_damage_enabled: true,
            release_kind: None,
            jack_pmf: None,
            pole_vault: false,
            fodder_contact_kind: FodderContactKind::Bite,
            can_contact_fodder: true,
            first_contact_at: Some(0),
            last_contact_at: Some(20),
            jack_blast_events: Vec::new(),
            jack_geometry_counts: [0; 5],
        }],
        deterministic_explosions: Vec::new(),
        deterministic_jack_blast_damage: 0.0,
    };

    assert_eq!(model.solve(tables).expect("choice").expected_damage, 0.0);
}

#[test]
fn pole_exact_212_endpoint_is_outside_the_damage_window() {
    let bytes = constant_tables(400, 0.0, 100.0, 0.0);
    let tables = DamageTables::parse(&bytes).expect("tables");
    let mut model = SmartFodderModel {
        domain: SmartFodderDomain {
            now: -1,
            plant_candidates: 0..=0,
            remove_by: None,
            activation_at: 393,
        },
        fodder_hp: 300,
        fodder_behavior: FodderBehavior::Biteable,
        fodder_morph: None,
        fodder_invulnerable_for: 0,
        threats: vec![SmartFodderThreat {
            update_rank: 0,
            trace_start: 0,
            x_trace: vec![620.0; 394],
            baseline_damage: 0.0,
            baseline_contact_at: None,
            bite_check_residue: None,
            ordinary_damage_enabled: true,
            release_kind: None,
            jack_pmf: None,
            pole_vault: true,
            fodder_contact_kind: FodderContactKind::Bite,
            can_contact_fodder: true,
            first_contact_at: Some(0),
            last_contact_at: Some(393),
            jack_blast_events: Vec::new(),
            jack_geometry_counts: [0; 5],
        }],
        deterministic_explosions: Vec::new(),
        deterministic_jack_blast_damage: 0.0,
    };

    assert_eq!(model.solve(tables).expect("choice").expected_damage, 0.0);
    model.domain.activation_at = 394;
    assert_eq!(model.solve(tables).expect("choice").expected_damage, 100.0);

    model.domain.remove_by = Some(20);
    model.threats[0].first_contact_at = Some(10);
    model.domain.activation_at = 403;
    assert_eq!(model.solve(tables).expect("choice").remove_at, Some(20));
    model.domain.activation_at = 404;
    assert_eq!(model.solve(tables).expect("choice").remove_at, Some(9));
}

#[test]
fn pole_vault_requires_fodder_to_survive_until_its_object_update() {
    let bytes = constant_tables(400, 0.0, 100.0, 0.0);
    let tables = DamageTables::parse(&bytes).expect("tables");
    let mut model = SmartFodderModel {
        domain: SmartFodderDomain {
            now: -1,
            plant_candidates: 0..=0,
            remove_by: None,
            activation_at: 400,
        },
        fodder_hp: 300,
        fodder_behavior: FodderBehavior::Biteable,
        fodder_morph: None,
        fodder_invulnerable_for: 0,
        threats: vec![SmartFodderThreat {
            update_rank: 1,
            trace_start: 0,
            x_trace: vec![620.0; 401],
            baseline_damage: 0.0,
            baseline_contact_at: None,
            bite_check_residue: None,
            ordinary_damage_enabled: true,
            release_kind: None,
            jack_pmf: None,
            pole_vault: true,
            fodder_contact_kind: FodderContactKind::Bite,
            can_contact_fodder: true,
            first_contact_at: Some(0),
            last_contact_at: Some(0),
            jack_blast_events: Vec::new(),
            jack_geometry_counts: [0; 5],
        }],
        deterministic_explosions: vec![OrderedExplosionPmf {
            pmf: ExplosionPmf::from_probabilities(0, vec![1.0]).expect("point PMF"),
            update_rank: 0,
        }],
        deterministic_jack_blast_damage: 0.0,
    };

    assert_eq!(model.solve(tables).expect("early blast").expected_damage, 0.0);
    model.deterministic_explosions[0].update_rank = 2;
    assert_eq!(model.solve(tables).expect("late blast").expected_damage, 100.0);
}

#[test]
fn cap_two_omits_future_random_direct_hits_but_keeps_prior_hits() {
    let bytes = constant_tables(30, 0.0, 0.0, 0.0);
    let tables = DamageTables::parse(&bytes).expect("tables");
    let jack = |update_rank, explosion_at| SmartFodderThreat {
        update_rank,
        trace_start: 0,
        x_trace: vec![800.0; 31],
        baseline_damage: 0.0,
        baseline_contact_at: None,
        bite_check_residue: None,
        ordinary_damage_enabled: false,
        release_kind: None,
        jack_pmf: Some(ExplosionPmf::from_probabilities(explosion_at, vec![1.0]).expect("point PMF")),
        pole_vault: false,
        fodder_contact_kind: FodderContactKind::Bite,
        can_contact_fodder: false,
        first_contact_at: None,
        last_contact_at: None,
        jack_blast_events: vec![JackBlastEvent {
            explosion_at,
            probability: 1.0,
            baseline_hits: 1.0,
            hits_fodder_without_block: false,
        }],
        jack_geometry_counts: [0; 5],
    };
    let model = SmartFodderModel {
        domain: SmartFodderDomain {
            now: -1,
            plant_candidates: 10..=10,
            remove_by: None,
            activation_at: 30,
        },
        fodder_hp: 300,
        fodder_behavior: FodderBehavior::Biteable,
        fodder_morph: None,
        fodder_invulnerable_for: 0,
        threats: vec![jack(0, 5), jack(1, 6), jack(2, 20)],
        deterministic_explosions: Vec::new(),
        deterministic_jack_blast_damage: 0.0,
    };

    assert_eq!(model.solve(tables).expect("choice").expected_damage, 600.0);
}

fn benchmark_model(jack_count: usize, full_window: bool) -> SmartFodderModel {
    let pmf = ExplosionPmf::conditional_unopened(super::super::JackDistributionInput {
        spawned_at: 0,
        observed_at: 650,
        birth_speed: 0.5,
        first_ice_at: None,
        first_freeze_in_pool: false,
    })
    .expect("benchmark PMF");
    let mut threats = vec![SmartFodderThreat {
        update_rank: 0,
        trace_start: 650,
        x_trace: vec![650.0; 1_151],
        baseline_damage: 300.0,
        baseline_contact_at: Some(0),
        bite_check_residue: Some(2),
        ordinary_damage_enabled: true,
        release_kind: Some(ReleaseTailKind::Ladder),
        jack_pmf: None,
        pole_vault: false,
        fodder_contact_kind: FodderContactKind::Bite,
        can_contact_fodder: true,
        first_contact_at: Some(800),
        last_contact_at: Some(1_500),
        jack_blast_events: Vec::new(),
        jack_geometry_counts: [0; 5],
    }];
    for index in 0..jack_count {
        let jack_blast_events = (pmf.start()..=pmf.end().min(1_799))
            .filter_map(|explosion_at| {
                let probability = pmf.probability_at(explosion_at);
                (probability > 0.0).then_some(JackBlastEvent {
                    explosion_at,
                    probability,
                    baseline_hits: 1.0,
                    hits_fodder_without_block: true,
                })
            })
            .collect();
        threats.push(SmartFodderThreat {
            update_rank: index + 1,
            trace_start: 650,
            x_trace: vec![650.0; 1_151],
            baseline_damage: 300.0,
            baseline_contact_at: Some(0),
            bite_check_residue: Some(2),
            ordinary_damage_enabled: true,
            release_kind: Some(ReleaseTailKind::JackNoPop),
            jack_pmf: Some(pmf.clone()),
            pole_vault: false,
            fodder_contact_kind: FodderContactKind::Bite,
            can_contact_fodder: true,
            first_contact_at: Some(760),
            last_contact_at: Some(1_500),
            jack_blast_events,
            jack_geometry_counts: [2, 0, 0, 0, 0],
        });
    }
    SmartFodderModel {
        domain: SmartFodderDomain {
            now: 650,
            plant_candidates: if full_window { 658..=1_290 } else { 1_150..=1_150 },
            remove_by: Some(1_290),
            activation_at: 1_800,
        },
        fodder_hp: 300,
        fodder_behavior: FodderBehavior::Biteable,
        fodder_morph: None,
        fodder_invulnerable_for: 0,
        threats,
        deterministic_explosions: Vec::new(),
        deterministic_jack_blast_damage: 0.0,
    }
}

#[test]
fn fused_score_matches_individual_branch_reference() {
    let tables = super::super::smart_fodder_tables().expect("embedded tables");
    let model = benchmark_model(3, false);
    let plant_at = *model.domain.plant_candidates.start();
    let lifecycle_pmfs = model
        .threats
        .iter()
        .filter_map(|threat| threat.jack_pmf.as_ref())
        .collect::<Vec<_>>();
    let contacts = model
        .threats
        .iter()
        .map(|threat| threat.contact(plant_at, model.domain.activation_at))
        .collect::<Vec<_>>();
    let release_values = model.prepare_release_values(tables).expect("release workspace");
    let release_queries = model
        .prepare_release_queries(tables, &release_values, &contacts)
        .expect("release queries");
    let jack_blast_axes = model.prepare_jack_blast_axes().expect("jack blast axes");
    let shared_sources = jack_blast_axes
        .iter()
        .zip(model.threats.iter().filter(|threat| threat.jack_pmf.is_some()))
        .map(|(axes, threat)| SharedJackExplosionSource {
            always_hits: &axes.always_hits,
            blockable_hits: &axes.blockable_hits,
            frame_order: zombie_before_check_order(threat.update_rank),
        })
        .collect::<Vec<_>>();
    let workspace = BranchExplosionWorkspace::new(&shared_sources, &[]).expect("probability workspace");
    let before = lifecycle_pmfs
        .iter()
        .map(|pmf| pmf.probability_before(plant_at))
        .collect::<Vec<_>>();
    let survivals = before.iter().map(|probability| 1.0 - probability).collect::<Vec<_>>();
    let jack_states = survivals
        .iter()
        .zip(
            model
                .threats
                .iter()
                .zip(&contacts)
                .filter(|(threat, _contact)| threat.jack_pmf.is_some()),
        )
        .map(|(survival_at_start, (_threat, contact))| BranchJackState {
            survival_at_start: *survival_at_start,
            blockable_from: contact.map_or(i32::MAX, |(contact_at, _x)| contact_at.saturating_add(110)),
        })
        .collect::<Vec<_>>();
    let mut inputs = vec![CapOneBranchInput {
        branch_scalar: 1.0,
        excluded_jack: None,
    }];
    inputs.extend(
        before
            .iter()
            .copied()
            .enumerate()
            .map(|(jack, branch_scalar)| CapOneBranchInput {
                branch_scalar,
                excluded_jack: Some(jack),
            }),
    );
    let candidate = workspace
        .candidate(plant_at, model.domain.activation_at, jack_states)
        .expect("candidate");
    let latest_end = inputs
        .iter()
        .map(|input| {
            model
                .branch_fodder_end(plant_at, None, &contacts, model.excluded_threat(input.excluded_jack))
                .time
        })
        .max()
        .expect("branch end");
    let x_masks = model.direct_x_masks(tables).expect("direct x masks");
    let convolutions = model
        .prepare_direct_convolutions(tables, &x_masks)
        .expect("direct convolutions");
    let direct_corrections = model
        .prepare_direct_corrections(tables, plant_at, latest_end, &contacts, &convolutions)
        .expect("direct corrections");

    for remove_at in critical_remove_candidates(plant_at, model.domain.remove_by, Vec::new()) {
        let states = inputs
            .iter()
            .copied()
            .map(|input| {
                let excluded_threat = model.excluded_threat(input.excluded_jack);
                BranchScoreState {
                    input,
                    excluded_threat,
                    end: model.branch_fodder_end(plant_at, remove_at, &contacts, excluded_threat),
                }
            })
            .collect::<Vec<_>>();
        let fused = model
            .fused_branch_score(
                tables,
                &release_values,
                &contacts,
                &release_queries,
                &direct_corrections,
                &jack_blast_axes,
                &candidate,
                &states,
            )
            .expect("fused score");
        let mut reference = 0.0;
        for state in &states {
            reference += model
                .fused_branch_score(
                    tables,
                    &release_values,
                    &contacts,
                    &release_queries,
                    &direct_corrections,
                    &jack_blast_axes,
                    &candidate,
                    std::slice::from_ref(state),
                )
                .expect("individual score");
        }
        assert!(
            (fused - reference).abs() < 1.0e-9,
            "R={remove_at:?}: fused={fused}, reference={reference}"
        );
    }
}

#[test]
fn demand_x_masks_match_full_35_grid_solver() {
    let tables = super::super::smart_fodder_tables().expect("embedded tables");
    assert_eq!(tables.jack_x_count(), 35);
    let mut model = benchmark_model(1, false);
    model.domain.plant_candidates = 1_149..=1_150;

    let demand_choice = model.solve(tables).expect("demand-grid choice");
    let full_mask = (1_u64 << tables.jack_x_count()) - 1;
    let full_choice = model
        .solve_with_direct_x_masks(tables, &vec![full_mask; model.threats.len()])
        .expect("full-grid choice");

    assert_eq!(demand_choice.plant_at, full_choice.plant_at);
    assert_eq!(demand_choice.remove_at, full_choice.remove_at);
    assert!((demand_choice.expected_damage - full_choice.expected_damage).abs() < 1.0e-9);
}

#[test]
fn equivalent_plant_candidates_match_exhaustive_full_window() {
    let tables = super::super::smart_fodder_tables().expect("embedded tables");
    for jack_count in [0, 1, 5] {
        let model = benchmark_model(jack_count, true);
        let sparse_candidates = model.equivalent_plant_candidates();
        let masks = model.direct_x_masks(tables).expect("direct x masks");
        let sparse = model
            .solve_with_direct_x_masks_and_candidates(tables, &masks, &sparse_candidates, false)
            .expect("equivalent candidates");
        let exhaustive = model.solve_exhaustive(tables).expect("all candidates");

        assert!(sparse_candidates.len() < model.domain.plant_candidates.clone().count());
        assert_eq!(sparse.plant_at, exhaustive.plant_at);
        assert_eq!(sparse.remove_at, exhaustive.remove_at);
        assert!((sparse.expected_damage - exhaustive.expected_damage).abs() < 1.0e-9);
    }
}

#[test]
fn equivalent_plant_candidates_keep_unsafe_and_event_boundaries() {
    let mut model = benchmark_model(0, true);
    let full_count = model.domain.plant_candidates.clone().count();
    model.fodder_behavior = FodderBehavior::Blover;
    assert_eq!(model.equivalent_plant_candidates().len(), full_count);
    model.fodder_behavior = FodderBehavior::Biteable;
    model.fodder_invulnerable_for = 1;
    assert_eq!(model.equivalent_plant_candidates().len(), full_count);

    model.fodder_invulnerable_for = 0;
    model.deterministic_explosions.push(OrderedExplosionPmf {
        pmf: ExplosionPmf::from_probabilities(900, vec![1.0]).expect("point PMF"),
        update_rank: 10,
    });
    assert!(!model.adjacent_plant_candidates_are_equivalent(900, 901));

    let mut pole = model.threats[0].clone();
    pole.update_rank = 11;
    pole.bite_check_residue = None;
    pole.release_kind = None;
    pole.pole_vault = true;
    pole.first_contact_at = Some(800);
    pole.last_contact_at = Some(1_000);
    model.threats.push(pole);
    assert!(!model.adjacent_plant_candidates_are_equivalent(799, 800));
}

#[test]
fn coarse_refinement_keeps_special_fodder_exhaustive() {
    let tables = super::super::smart_fodder_tables().expect("embedded tables");
    let mut model = benchmark_model(1, true);
    let configurations: [fn(&mut SmartFodderModel); 2] = [
        |model: &mut SmartFodderModel| model.fodder_behavior = FodderBehavior::Blover,
        |model: &mut SmartFodderModel| model.fodder_invulnerable_for = 1,
    ];
    for configure in configurations {
        configure(&mut model);
        let actual = model.solve(tables).expect("production solve");
        let exhaustive = model.solve_exhaustive(tables).expect("exhaustive solve");
        assert_eq!(actual.plant_at, exhaustive.plant_at);
        assert_eq!(actual.remove_at, exhaustive.remove_at);
        assert!((actual.expected_damage - exhaustive.expected_damage).abs() < SCORE_EPSILON);
        model.fodder_behavior = FodderBehavior::Biteable;
        model.fodder_invulnerable_for = 0;
    }
}

#[test]
#[ignore = "performance probe; run explicitly with --ignored --nocapture"]
fn release_solver_microbenchmark() {
    let tables = super::super::smart_fodder_tables().expect("embedded tables");
    for jack_count in [0, 1, 5] {
        for full_window in [false, true] {
            let model = benchmark_model(jack_count, full_window);
            let iterations = if full_window { 1 } else { 10 };
            let started = std::time::Instant::now();
            for _ in 0..iterations {
                std::hint::black_box(model.solve(tables).expect("benchmark solve"));
            }
            let elapsed = started.elapsed();
            eprintln!(
                "release_solver jacks={jack_count} window={} iterations={iterations} elapsed_ms={:.3} mean_ms={:.3}",
                if full_window { "658..=1290" } else { "1150" },
                elapsed.as_secs_f64() * 1_000.0,
                elapsed.as_secs_f64() * 1_000.0 / f64::from(iterations),
            );
        }
    }
}

fn suffix_correlation(left: &[f64], right: &[f64]) -> Vec<f64> {
    debug_assert_eq!(left.len(), right.len());
    if left.is_empty() {
        return Vec::new();
    }
    let result_len = left.len();
    let fft_len = (left.len() + right.len() - 1).next_power_of_two();
    let plan = FftPlan::new(fft_len);
    let left = fft_spectrum(left, true, &plan);
    let right = fft_spectrum(right, false, &plan);
    let mut work_real = vec![0.0_f32; fft_len];
    let mut work_imag = vec![0.0_f32; fft_len];
    correlate_spectra(&left, &right, result_len, &mut work_real, &mut work_imag, &plan)
}

fn direct_weighted_before(
    branch: &FocusedBranchProbabilityView<'_, '_, '_>, corrections: &DirectCorrectionSeries, end: i32,
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

fn fodder_presence_at_actor(
    branch: &BranchProbabilityView<'_, '_, '_>, deterministic_end: DeterministicFodderEnd, time: i32, frame_order: i32,
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

impl SmartFodderModel {
    pub(super) fn accumulate_branch_scores(
        &self, tables: DamageTables<'_>, release_values: &ReleaseValueWorkspace, contacts: &[Option<(i32, f32)>],
        release_queries: &[Option<ReleaseQuery>], direct_corrections: &[Option<DirectCorrectionSeries>],
        jack_blast_axes: &[AbsoluteJackBlastAxes], excluded_jack: Option<usize>, remove_candidates: &[Option<i32>],
        branch: &BranchProbabilityView<'_, '_, '_>, scores: &mut [f64],
    ) -> Result<(), SmartFodderSolveError> {
        if branch.branch_mass() <= 0.0 {
            return Ok(());
        }
        let excluded_threat = self.excluded_threat(excluded_jack);
        let direct_baseline_after_fodder = jack_blast_axes
            .iter()
            .enumerate()
            .filter_map(|(jack, axes)| {
                branch.focused(jack).map(|focused| {
                    focused.survival_after(branch.start().saturating_sub(1)) * axes.baseline_at_or_after(branch.start())
                })
            })
            .sum::<f64>();
        for (score, remove_at) in scores.iter_mut().zip(remove_candidates) {
            let deterministic_end = self.branch_fodder_end(branch.start(), *remove_at, contacts, excluded_threat);
            let mut damage = 300.0 * direct_baseline_after_fodder;
            let mut jack = 0;
            for (threat_index, threat) in self.threats.iter().enumerate() {
                if threat.jack_pmf.is_none() {
                    continue;
                }
                let current_jack = jack;
                jack += 1;
                let (Some(corrections), Some(focused)) =
                    (direct_corrections[threat_index].as_ref(), branch.focused(current_jack))
                else {
                    continue;
                };
                let end = deterministic_end.time.clamp(focused.start(), self.domain.activation_at);
                let correction = direct_weighted_before(&focused, corrections, end)
                    + focused.survival_after(end.saturating_sub(1)) * corrections.correction_at(end);
                let order_correction = deterministic_order_correction(
                    |time| focused.first_probability_at_or_after_focus(time),
                    |time| focused.survival_after(time),
                    corrections.contact_at,
                    corrections.bite_frame_order,
                    deterministic_end,
                    |time| corrections.correction_at(time),
                    |time| corrections.delayed_correction_at(time),
                    0.0,
                    self.domain.activation_at,
                );
                damage += 300.0 * (correction + order_correction);
            }
            for (index, (threat, contact)) in self.threats.iter().zip(contacts).enumerate() {
                if Some(index) == excluded_threat || !threat.ordinary_damage_enabled {
                    continue;
                }
                let Some((contact_at, _contact_x)) = *contact else {
                    damage += branch.branch_mass() * threat.baseline_damage;
                    continue;
                };
                if threat.pole_vault {
                    if deterministic_end.time < contact_at {
                        damage += branch.branch_mass() * threat.baseline_damage;
                        continue;
                    }
                    let landing = contact_at.saturating_add(POLE_VAULT_DURATION);
                    let after_landing = self.domain.activation_at.saturating_sub(landing);
                    if after_landing <= SLOWED_POLE_MIN_AFTER_LANDING {
                        continue;
                    }
                    damage += fodder_presence_at_actor(
                        branch,
                        deterministic_end,
                        contact_at,
                        zombie_before_check_order(threat.update_rank),
                    ) * f64::from(tables.pole_damage(usize::try_from(after_landing).unwrap_or_default())?);
                    continue;
                }
                let release_query = release_queries[index]
                    .as_ref()
                    .ok_or(SmartFodderSolveError::UnsupportedReleaseThreat)?;
                damage += ordinary_damage(
                    release_values,
                    branch,
                    *release_query,
                    threat,
                    contact_at,
                    deterministic_end,
                    self.domain.activation_at,
                )?;
            }
            *score += damage;
        }
        Ok(())
    }
    pub(super) fn fused_branch_score(
        &self, tables: DamageTables<'_>, release_values: &ReleaseValueWorkspace, contacts: &[Option<(i32, f32)>],
        release_queries: &[Option<ReleaseQuery>], direct_corrections: &[Option<DirectCorrectionSeries>],
        jack_blast_axes: &[AbsoluteJackBlastAxes], candidate: &CandidateProbabilityCache<'_, '_>,
        states: &[BranchScoreState],
    ) -> Result<f64, SmartFodderSolveError> {
        let mut terms = Vec::new();
        let branch_inputs = states.iter().map(|state| state.input).collect::<Vec<_>>();
        let mut time_groups = BranchEndGrouping::default();
        time_groups.rebuild(states, |state| state.end.time);
        let mut ordered_groups = BranchEndGrouping::default();
        ordered_groups.rebuild(states, |state| state.end);
        self.fused_branch_score_with_scratch(
            tables,
            release_values,
            contacts,
            release_queries,
            direct_corrections,
            jack_blast_axes,
            candidate,
            states,
            &branch_inputs,
            &time_groups,
            &ordered_groups,
            &mut terms,
        )
    }
    pub(super) fn prepare_direct_corrections(
        &self, tables: DamageTables<'_>, plant_at: i32, end: i32, contacts: &[Option<(i32, f32)>],
        convolutions: &[Option<JackDirectConvolution>],
    ) -> Result<Vec<Option<DirectCorrectionSeries>>, SmartFodderSolveError> {
        let mut output = Vec::new();
        self.prepare_direct_corrections_into(tables, plant_at, end, contacts, convolutions, &mut output)?;
        Ok(output)
    }
    pub(super) fn prepare_release_values(
        &self, tables: DamageTables<'_>,
    ) -> Result<ReleaseValueWorkspace, SmartFodderSolveError> {
        self.prepare_release_values_for(tables, &self.domain.plant_candidates.clone().collect::<Vec<_>>())
    }
    pub(super) fn direct_x_masks(&self, tables: DamageTables<'_>) -> Result<Vec<u64>, SmartFodderSolveError> {
        self.direct_x_masks_for(tables, &self.domain.plant_candidates.clone().collect::<Vec<_>>())
    }
    pub(super) fn prepare_release_queries(
        &self, tables: DamageTables<'_>, release_values: &ReleaseValueWorkspace, contacts: &[Option<(i32, f32)>],
    ) -> Result<Vec<Option<ReleaseQuery>>, SmartFodderSolveError> {
        let mut output = Vec::new();
        self.prepare_release_queries_into(tables, release_values, contacts, &mut output)?;
        Ok(output)
    }
    pub(super) fn adjacent_plant_candidates_are_equivalent(&self, earlier: i32, later: i32) -> bool {
        self.adjacent_plant_candidates_are_equivalent_with_order(earlier, later, &[])
    }
    pub(super) fn solve_exhaustive(
        &self, tables: DamageTables<'_>,
    ) -> Result<SmartFodderChoice, SmartFodderSolveError> {
        let direct_x_masks = self.direct_x_masks(tables)?;
        let plant_candidates = self.domain.plant_candidates.clone().collect::<Vec<_>>();
        self.solve_with_direct_x_masks_and_candidates(tables, &direct_x_masks, &plant_candidates, false)
    }
    pub(super) fn solve_with_direct_x_masks(
        &self, tables: DamageTables<'_>, direct_x_masks: &[u64],
    ) -> Result<SmartFodderChoice, SmartFodderSolveError> {
        let plant_candidates = self.equivalent_plant_candidates();
        self.solve_with_direct_x_masks_and_candidates(tables, direct_x_masks, &plant_candidates, true)
    }
}
