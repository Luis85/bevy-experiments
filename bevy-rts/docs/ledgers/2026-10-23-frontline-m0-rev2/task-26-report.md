# Task 26 report: dynamic_linking probe

Verdict: dynamic linking WORKS with bevy_ecs/bevy_app-only sim crates.

- `cargo run -p probe_dynlink` (static): `dynlink probe: OK sim_ticks=10 app_updates=10` (cold build 26m43s)
- `cargo run -p probe_dynlink --features dynamic`: same OK line, exit 0. No duplicate-crate, link or type-mismatch error.
- Added `dynamic = ["bevy/dynamic_linking"]` to crates/fl_game/Cargo.toml; recorded section "Probe: dynamic_linking" in docs/research/bevy020-api-reference.md.
- Interim gate: fmt OK, clippy -D warnings OK, cargo test --workspace OK (34 test binaries ok, 0 failed), scripts/check_deps.sh OK. `cargo xtask gate` not available yet.
- Caveat: only the headless path was exercised; windowed `fl_game --features dynamic` was not run.
Files: probes/dynlink/src/main.rs, crates/fl_game/Cargo.toml, docs/research/bevy020-api-reference.md

## Fix round 1
Changes: probe now crosses the bevy_ecs boundary three ways (fl_sim `SimClock` into a bevy app read by a bevy::prelude system; bevy::prelude `Schedule` run on the sim world; sim `World` moved into a bevy resource and stepped from a bevy system), asserting `Tick(10)`; sim clock asserted in the baseline; typed counters; dropped fl_content dep, added fl_protocol (for `Tick`). Doc verdict updated to match what was tested.
Commands/outputs:
- `cargo run -p probe_dynlink` -> `dynlink probe: OK sim_ticks=10 app_updates=10`
- `cargo run -p probe_dynlink --features dynamic` -> same OK line
- `cargo build -p fl_game --features dynamic` -> Finished in 17m38s
- fmt/clippy: see below
- fmt --check OK; clippy -p probe_dynlink --all-targets -D warnings OK
