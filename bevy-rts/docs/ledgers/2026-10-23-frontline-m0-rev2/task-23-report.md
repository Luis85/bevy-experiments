# Task 23 report: bench harness

Implemented `crates/fl_cli/src/bench.rs` (BenchConfig/BenchRow/run_bench/run_bench_full/median/profile_json/render_markdown/compare/rows_from_json/exclusive), `fl_cli bench` CLI (--units --ticks --warmup --seed --repeat --out --json --profile --baseline --max-regression; exit 1 on regression), criterion `sim_tick` (tick/500, /3000, /5000 on core_mod, 40-tick warmup), tests in crates/fl_cli/tests/bench.rs (5 brief tests), and the carried fl_observe `profiler_sim` change (phase_markers on, asserts 8 phases).

Notes: runs interleave sizes x repeat, median by mean_ms. Wall clock via `time_ms` helper behind scoped expect. Scenario built in one fn `populate` (switch point for Task 29 entity-mix). Profiler is global, so `exclusive()` mutex serializes runs; per-phase means come from the profiler's 240-tick rolling window. CLI uses base mod + fl_catalog features (criterion uses core_mod, R3). profile_json: top-level `rows`, `sizes`, plus `phases/features/systems` of the last size; `--profile` needed to include the breakdown in the file (baseline reads `rows`).

Tests: nextest fl_cli + fl_observe 23/23 pass. `cargo bench -p fl_sim --bench sim_tick -- --quick` runs. Gate: `gate task --track tools --track sim-core` => PASS in 238s (all rows PASS, test-client SKIP).
