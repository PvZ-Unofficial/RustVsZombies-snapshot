# Witness acceptance

## Current acceptance status

Status recorded on 2026-09-03:

- The current low-level DSL `pe24` script has completed a real PvZ 1.0.0.1051
  injection smoke test.
- The checked-in 1051 Digest golden was produced by a real English
  PvZ 1.0.0.1051 process and added in commit
  `747d1b9303220ab5e8a3e65d1f399015e0502648`.
- The PE and native PvZ-Portable acceptance gates have both been run against
  that same golden and matched it.

This status is limited to the scenario below. It does not claim complete
cross-backend parity for every script, game scene, or capability.

`golden/pe24-three-rounds-1051.json` is the reviewed PvZ 1.0.0.1051 Digest
reference for `dev-scripts/witness_acceptance`.

The acceptance scenario uses one worker, seed `0x5eed1051`, Locked `7`, an
explicit 20-wave spawn table, the PE24 lineup, and three completed Survival
rounds. UI/chooser transport frames are excluded, so the expected artifact has
47,136 canonical frames.

Run the PE acceptance gate from the repository root:

```powershell
pwsh -File scripts/check-witness.ps1
```

Run the same three-round gate through the native PvZ-Portable host by supplying
an explicit 1051 resource directory containing `main.pak` and `properties/`:

```powershell
pwsh -File scripts/check-witness-portable.ps1 `
  -ResourceDirectory C:\path\to\PlantsVsZombies-1051
```

The Portable command builds the MSVC host/plugin, passes the resource path via
the game's native `-resdir` argument, compares against the same 1051 golden,
and closes that test host afterward. Resources are never copied into the repo.

The sibling `../PvZ-Emulator` or `../PvZ-Portable` checkout is required for its
respective gate. CI must compare each backend against this 1051 reference; it
must never replace the golden with non-1051 output. Regenerate
the golden only after an intentional semantic change, using a real English
1.0.0.1051 process and the same script:

```powershell
cargo run -p rsvz-cli -- inject-1051 --pid <PID> `
  --dev-script witness_acceptance `
  --output tests\witness\golden\pe24-three-rounds-1051.json `
  --report target\witness-acceptance-1051.report.json `
  --wait-timeout-secs 300
```

Review the new 1051 artifact against the previous golden before accepting it.
Full captures remain diagnostic artifacts and are not checked into the
acceptance fixture.
