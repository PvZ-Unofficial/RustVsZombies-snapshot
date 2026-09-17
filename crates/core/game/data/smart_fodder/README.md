# Smart-fodder v1 table

`v1.bin` was packed from a formal 50,000-trial-per-case PE run with 35 release coordinates (`620..=722`, step 3) and `h=0..=1142`. The measurement held the Jack phase countdown outside the sampled horizon through the backend-neutral phase-countdown writer; all 5,300,000 trajectories were accepted (`invalid_attempts=0`).

Source revisions:

- RustVsZombies: `3f0f412ebbebe032d9b9ca6a501f7e32d9cd1a8d`
- PvZ-Emulator: `5148ddf73a926698b7acd02952b5e7d03622e3fd`

Generation command:

```text
$env:RSVZ_SOURCE_COMMIT = "3f0f412ebbebe032d9b9ca6a501f7e32d9cd1a8d"
$env:PVZ_EMULATOR_SOURCE_COMMIT = "5148ddf73a926698b7acd02952b5e7d03622e3fd"
cargo run -q -p rsvz-cli -- run-pe --dev-script smart_fodder_measure --release --threads 8 --base-seed 20515489 --max-wall-secs 10800 --output target/smart-fodder-formal-3f0f412.json --stats
cargo run --manifest-path dev-scripts/smart_fodder_pack/Cargo.toml --release -- target/smart-fodder-formal-3f0f412.json crates/core/game/data/smart_fodder/v1.bin
```

Raw artifact SHA-256: `969B05E4412F7C6C8FF79452F3C6551AD872856A988F28912E1F42E90DEF2301`

Packed binary SHA-256: `172334390873A2BF1C8D96A70C178D319AE3DDC5B5D194DA94CAE4C0F9FAF322`

See [`VALIDATION.md`](VALIDATION.md) for the paired full-trajectory PE validation and the independent PUC/SEML check.
