# Integration fix report

## 1. Bench entity mix (4958312)
`populate` in `crates/fl_cli/src/bench.rs` now builds `ScaleScenario { seed, ..ScaleScenario::with_units(units) }`.
`BenchRow` already carried `units` and `entities` (markdown and JSON); unchanged. `bench_reports_each_size`
asserts `units == requested` and `entities > units`.

## 2. R63 replay_app (6bf03ad)
New `pub fn fl_sim::replay_app(r, features, content, run) -> Result<(App, Vec<String>), ReplayError>`
(build/load, `check_replay`, `ReplayStart` setup). `verify_replay` and fl_cli `Session::open` (hence inspect and
bisect) use it; fl_cli logs the warnings. New test `replay_app_checks_header_and_sets_up` (build_id warns, rules hash
-> `ReplayError::Header`).

## Gate
`cargo xtask gate pr --base main`: PASS (clippy, arch-check, test-headless, test-client, deny, data-check).
The first run failed clippy on `len() > 0` in my new test; fixed and amended into the refactor commit.

## Concern
fl_cli's `replay_app` used to report a sim build failure as `CliError::Build`; `ReplayError` has no Build variant,
so it is now `CliError::Setup` (message unchanged).
