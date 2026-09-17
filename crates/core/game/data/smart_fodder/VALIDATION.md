# Smart-fodder v1 validation

## Paired PE validation

The user-space `smart_fodder_measure` script also has a validation-only mode. It builds the same visible Day-scene state for two candidates, uses the analytical online path at `T=650`, and then advances real PvZ-Emulator worlds to `A=1800`. Each pair uses the same seed for `F=1150` and `F=1200`.

The fixed scene contains one ladder zombie, five unopened Jack-in-the-box zombies, a land ice effect, a C9 sunflower, and one C7-C8 cannon. The cannon is given excess validation HP so cumulative ordinary damage is not truncated at its native 300 HP; the tested geometry keeps Jack explosions away from the cannon. The fodder retains native HP and damage behavior.

Source revisions:

- RustVsZombies: `fe86da8d629b6fc5c1b76a41c98273859c17cfc2`
- PvZ-Emulator: `5148ddf73a926698b7acd02952b5e7d03622e3fd`

Command:

```text
$env:SMART_FODDER_VALIDATE_ONLY = "1"
$env:RSVZ_SOURCE_COMMIT = "fe86da8d629b6fc5c1b76a41c98273859c17cfc2"
$env:PVZ_EMULATOR_SOURCE_COMMIT = "5148ddf73a926698b7acd02952b5e7d03622e3fd"
cargo run -q -p rsvz-cli -- run-pe --dev-script smart_fodder_measure --release --threads 8 --base-seed 30515489 --max-wall-secs 1800 --output target/smart-fodder-paired-fe86da8.json --stats
```

Raw artifact SHA-256: `D52DD021E9F80A0705F59FCFD35294F551167BF3FB9A5C0B4D8E209818C76161`

Results (`actual - predicted`; 95% confidence interval half-width):

| F | Predicted mean | Full PE mean | Paired difference |
|---:|---:|---:|---:|
| 1150 | 39.7171 | 35.7764 | -3.9407 ± 0.2634 |
| 1200 | 54.4823 | 54.0662 | -0.4162 ± 0.1962 |

Both aggregate means choose `F=1150`; the accepted v1 approximations do not change the winner in this case. Individual noisy outcomes disagree with the conditional-mean ordering in 8,507 of 50,000 pairs; this is retained as a sample-level diagnostic, not reported as an aggregate ranking reversal. The batch had zero invalid attempts.

A diagnostic-only rerun on the same 50,000 seeds additionally counted the
realized Cap-1 omitted samples immediately before each half-open `F` boundary.
It left every predicted/actual damage statistic above unchanged:

| F | `N_F>=2` samples | Omitted frequency | Excess-count mean |
|---:|---:|---:|---:|
| 1150 | 129 | 0.2580% +/- 0.0445 percentage points | 0.2620% +/- 0.0455 percentage points |
| 1200 | 260 | 0.5200% +/- 0.0630 percentage points | 0.5300% +/- 0.0648 percentage points |

The diagnostic artifact SHA-256 is
`449F5E4F794F918C885B854CE3944B2D2A8197EDF5A7960A083195AD40C11319`.
These are empirical checks of the omitted probability and
`E[(N_F-1)_+]`, not damage-error bounds; in particular, the measured damage
bias at `F=1150` is much larger than the omitted frequency alone.

## Event-stratified follow-up

A diagnostic-only follow-up retained the same seeds and worlds while recording
whether a post-`F` Jack explosion killed the fodder. It also repeated the scene
with zero and one Jack; the production predictor and public API were unchanged.
The default validation uses five Jacks; setting the compile-time environment
flag `SMART_FODDER_VALIDATION_ONE_JACK` or
`SMART_FODDER_VALIDATION_ZERO_JACKS` selects the corresponding control.

| Jacks | F | Predicted mean | Full PE mean | `actual - predicted` |
|---:|---:|---:|---:|---:|
| 0 | 1150 | 0.0000 | 0.0000 | 0.0000 |
| 0 | 1200 | 0.0000 | 0.0000 | 0.0000 |
| 1 | 1150 | 3.2022 | 2.9983 | -0.2039 +/- 0.1308 |
| 1 | 1200 | 8.1057 | 8.7921 | 0.6864 +/- 0.1353 |
| 5 | 1150 | 39.7171 | 35.7764 | -3.9407 +/- 0.2634 |
| 5 | 1200 | 54.4823 | 54.0662 | -0.4162 +/- 0.1962 |

With five Jacks, a post-`F` explosion killed the fodder in 7,144/50,000
`F=1150` trials and 5,471/50,000 `F=1200` trials. An explosion occurred in
the interval `[1150,1200)` in 2,079 paired seeds. In those pairs, the actual
`F=1200 - F=1150` damage difference averaged `-99.95`, compared with `23.42`
when no explosion occurred in that interval. These conditional strata use the
same unconditional prediction made at `T=650`, so their individual residuals
are attribution diagnostics rather than conditional calibration errors.

The zero-Jack baseline is exact, the one-Jack bias is small, and the five-Jack
bias contains a large nonlinear upward-prediction component. Cap-1 cannot
produce that sign because discarding nonnegative future-damage branches lowers
the prediction. The observed direction, multiplicity dependence, and earlier-
`F` sensitivity instead match the accepted v1 `G_jack_no_pop` approximation:
the exploding Jack and Jacks released by another explosion retain ordinary
tails that real explosions truncate. This diagnoses an algorithmic
approximation, not a simple implementation defect; production behavior was not
changed.

Artifacts:

- Five Jacks: `F18804FFA315F9A235F030D2FD796E6D584BD1787A9D5CCADEAA1E8DAA256351`
- One Jack: `BF9D83518F111F5FD67BA8B9600B9D8C04ADA573BBB28D910D9C7EAD0BFBB90E`
- Zero Jacks: `23F7CD04EB3E51F63A6B26BE5E6B2B26C28207D9831CDDBCE00E7D042BA0D2F4`

## Jack post-explosion ordinary-bite cutoff follow-up

The solver was then changed to stop each unopened Jack's ordinary cannon-damage
curve at that Jack's sampled explosion. The same identity-specific calculation
makes a Jack that kills the fodder contribute no counterfactual post-explosion
ordinary tail, and baseline cannon bites stop at the same explosion boundary.
Both the decompiled game and PvZ-Emulator continue ordinary plant bites during
the 110 cs popping phase, so an earlier diagnostic build that stopped at the
start of popping was mechanically wrong. This does not change the accepted v1
approximation for Jack bites against the fodder itself.

The five-Jack and one-Jack validations reused the exact 50,000 paired seeds,
world setup, `T=650`, `F=1150/1200`, and `A=1800` above. The unchanged full-PE
means confirm that the experimental worlds did not drift.

| Jacks | F | Old prediction | Wrong pop-start cutoff | Explosion cutoff | Full PE mean | New `actual - predicted` |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 1150 | 3.2022 | 3.0962 | 3.0962 | 2.9983 | -0.0978 +/- 0.1308 |
| 1 | 1200 | 8.1057 | 8.0633 | 8.0633 | 8.7921 | 0.7288 +/- 0.1352 |
| 5 | 1150 | 39.7171 | 39.1278 | 39.1423 | 35.7764 | -3.3659 +/- 0.2634 |
| 5 | 1200 | 54.4823 | 54.2464 | 54.2541 | 54.0662 | -0.1880 +/- 0.1962 |

Both corrected batches completed with zero invalid attempts. The explosion
cutoff removes a real upward component and makes the one-Jack `F=1150`
residual statistically compatible with zero. It does not explain most of the
five-Jack `F=1150` residual, which remains significant. The wrong pop-start
cutoff happened to reduce that residual by a further `0.0145`, but that numerical
movement is not valid mechanism evidence. The remaining multi-Jack discrepancy
is therefore not attributed to the removed future-bite tail without a separate
probe.

The runs used a worktree based on RustVsZombies commit
`9f5e6506ba7ce19de686b7a5681318a4ed8e134a` and PvZ-Emulator commit
`5148ddf73a926698b7acd02952b5e7d03622e3fd`.

Artifacts:

- Five Jacks: `0701B89A24B3E1849876C66C70A26C2C615B9AEFB8129CE8CB043320D6CCEAFB`
- One Jack: `3B766BB6E748AA395E1CF60DA61B3DEBF2D98FED35FB2F246EDD4C805967D1BE`

### Production disposition and solver microbenchmark

The following performance batch retained the diagnostic artifact and this
record but restored the accepted v1 `G_jack_no_pop` production approximation.
The explosion cutoff removed only about 14.6% of the five-Jack `F=1150`
residual while making the measured solver substantially slower, so it is not
part of the production score.

The dependency-free ignored test `release_solver_microbenchmark` uses the
embedded `v1.bin`, a fixed synthetic slowed C9 scene, 0/1/5 Jacks, and either
the point window `F=1150` or the full `658..=1290` window. The values below are
same-machine debug-test timings and are only comparable within this table.

| Stage | Jacks | Point mean (ms) | Full window (ms) |
|---|---:|---:|---:|
| Explosion cutoff | 0 | 0.243 | 164.015 |
| Explosion cutoff | 1 | 55.443 | 567.140 |
| Explosion cutoff | 5 | 276.715 | 5585.559 |
| Restored no-pop baseline | 0 | 0.230 | 159.644 |
| Restored no-pop baseline | 1 | 31.319 | 595.108 |
| Restored no-pop baseline | 5 | 151.190 | 4981.568 |
| Exact mod-8 buckets + demand FFT | 0 | 0.111 | 52.008 |
| Exact mod-8 buckets + demand FFT | 1 | 1.354 | 244.721 |
| Exact mod-8 buckets + demand FFT | 5 | 7.953 | 3500.421 |
| Branch-weighted Cap-1 workspace | 0 | 0.113 | 61.752 |
| Branch-weighted Cap-1 workspace | 1 | 1.337 | 255.540 |
| Branch-weighted Cap-1 workspace | 5 | 5.188 | 1773.854 |

The final artifact also records elapsed milliseconds around each real
`predict_smart_fodder_at` call, so bounded PE validation reports solver time
separately from whole-batch wall time.

The final implementation was also run through the ignored benchmark in an
optimized build:

```text
cargo test -p rsvz-game release_solver_microbenchmark --release -- --ignored --nocapture
```

| Jacks | Point `F=1150` mean (ms, 10 iterations) | Full `658..=1290` window (ms) |
|---:|---:|---:|
| 0 | 0.015 | 3.322 |
| 1 | 0.257 | 40.920 |
| 5 | 0.647 | 132.481 |

### Final optimized PE validation

The optimized solver was checked first with an eight-pair, one-worker,
fixed-seed smoke run, then with the same 50,000 paired seeds used above for
zero, one, and five Jacks. All formal runs used a release runner, eight PE
workers, `base-seed=30515489`, `T=650`, `F=1150/1200`, and `A=1800`. The
artifacts identify the source as `cde0903+completion-audit`: the post-commit
source differences are the clippy-equivalent lazy default branch and the
internal full-grid comparison entry used by the completion test. PvZ-Emulator remained at
`5148ddf73a926698b7acd02952b5e7d03622e3fd`.

Results below use `actual - predicted`; the reported solver time surrounds only
`predict_smart_fodder_at`, while PE wall time is the whole eight-worker runtime
reported by the host. Each batch had zero invalid attempts.

| Jacks | F | Predicted mean | Full PE mean | Residual (95% CI half-width) | Solver mean (ms) |
|---:|---:|---:|---:|---:|---:|
| 0 | 1150 | 0.0000 | 0.0000 | 0.0000 +/- 0.0000 | 0.0426 |
| 0 | 1200 | 0.0000 | 0.0000 | 0.0000 +/- 0.0000 | 0.0423 |
| 1 | 1150 | 3.3398 | 2.9983 | -0.3414 +/- 0.1310 | 0.2792 |
| 1 | 1200 | 8.1051 | 8.7921 | 0.6870 +/- 0.1353 | 0.2765 |
| 5 | 1150 | 40.2504 | 35.7764 | -4.4740 +/- 0.2632 | 1.1483 |
| 5 | 1200 | 54.4647 | 54.0662 | -0.3985 +/- 0.1962 | 1.1374 |

The zero-Jack candidates tied in both prediction and PE. For one and five
Jacks, both aggregate means selected `F=1150`. Sample-level ranking
disagreements were respectively 0, 5,133, and 8,846 out of 50,000 pairs; they
are not aggregate ranking reversals. Whole PE wall times for zero, one, and
five Jacks were 15.598 s, 20.592 s, and 34.341 s. End-to-end command times,
including the incremental runner build, were 17.383 s, 22.373 s, and 36.096 s.

Artifacts:

- Zero Jacks: `0885350744F09814DE36E893BD9F6D5E566E2F6DE1BFB245F6A23332607E3716`
- One Jack: `5086144A56B44CA23CB0D218D4E35FF445223AE76B6DC3D461C315094D520A39`
- Five Jacks: `78DC0EF38D7CF8D95E12DBD0ED2291F1CD4D22739CF86CAA74947238A552AB50`

## Independent PUC/SEML check

PUC commit `c8b2b75a1b668e6d4fb48628a5924463ed46479c` independently ran the following fixed mixed-zombie C9 scenario:

```seml
scene:PE
protect:18
repeat:50000
std:true
avzTime:false

w 0 1200~1800
C 700 1 9
```

Command:

```text
puc seml --strict --compact --csv target/smart-fodder-puc-validation.csv explode target/smart-fodder-puc-validation.seml
```

At 1800, PUC reports total damage `216.84 ± 0.60`, comprising instant damage `4.38 ± 0.16` and other damage `212.46 ± 0.58`. CSV SHA-256: `78E7615925C9B9FDA631B7612389BD4A63BAB35459C28D536D328F1F1222EEDC`.

PUC's `explode` mode uses its own fixed five-each Jack/ladder/football/catapult population and cannot invoke Rust's Cap-1 candidate solver. It is therefore an independent representative mechanics check, while the paired artifact above is the solver-specific validation.
