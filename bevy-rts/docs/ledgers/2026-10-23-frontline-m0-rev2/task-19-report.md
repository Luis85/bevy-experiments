# Task 19 report: fl_observe (logging, profiler layer, journal sink, debug.ron)

Worktree `C:\fl\t19`, branch `m0/t19-observe`, base 8cfc38e. Commit `4df225b feat(fl_observe): logging setup, profiler layer (both threads, phase markers), journal sink, debug.ron`.

## What was implemented

- `config.rs`: `DebugConfig` and `Budgets` as in the brief (`deny_unknown_fields, default`). `load_debug_config(path)` uses the explicit path, else `./debug.ron`, else the defaults. A non-empty `RUST_LOG` replaces `log_filter`. A file that can't be read or parsed is reported on stderr, and the defaults are used.
- `logging.rs`:
  - `RunDir` and `new_run_dir` create `root/<YYYYMMDD-HHMMSS>-<pid>/` (UTC, with `-<n>` added on a collision) and write `root/latest.txt`.
  - `json_file_layer` is a custom JSON-lines layer writing `log.jsonl` with `timestamp` (RFC 3339 UTC), `level`, `target`, `fields`, `spans` (name and fields of each span in scope) and `tick`.
    - `tick` comes from the enclosing `sim_tick` span, or from the `sim_tick` span entered on this thread. A thread-local stack is needed because bevy_ecs `system` spans are `parent: None`, so the stock JSON formatter would lose the tick for every record emitted inside a system.
    - If the file can't be opened, the layer becomes a no-op `Option` and the error goes to stderr.
  - `init_headless` is idempotent (OnceLock) and uses `try_init`. It installs the EnvFilter as a **per-layer** filter over stderr fmt plus the JSON layer. A small wrapper also always enables `sim_tick`, so records keep their tick under `RUST_LOG=warn`. The profiler layer gets its own `filter_fn` (the `system`/`sim_tick`/`frame` spans and `fl_sim::phase` events), so the log filter never hides profiler input (phase markers are TRACE).
  - Also sets the global run dir and the budgets.
  - `set_global_run_dir`, `global_run_dir`.
- `profiler.rs`: `Profiler`, `ProfilerLayer`, `Stat`, `PerfSnapshot`, `SystemStat`, `feature_of`, `global_profiler`.
  - Systems are timed from enter to exit of the `system` span, by `name`, on any thread.
  - A system run on a thread with an open `sim_tick` (of this profiler, thread-local stack keyed by profiler id) goes to that tick's pending set, keyed by span id. Concurrent sims in one process are therefore kept apart. Other runs (client worker threads) are pooled and fold when a `frame` span closes.
  - Phases are timed between `fl_sim::phase` `start`/`end` marker events (`phase`/`edge` read as str or debug) inside a tick.
  - On `sim_tick` close, the profiler folds tick busy time, phases (first-seen order), systems and per-feature sums into 240-sample rolling series. Percentiles are nearest-rank, computed with integer rank arithmetic. `Stat.samples` is the lifetime count, not the window count.
  - Budgets (tick, per phase, per feature) are checked at fold. Overruns are logged with `warn!(target: "fl_observe::budget")` after the lock is released. Each budget warns at most once per 240 folds, to avoid 20 Hz spam.
  - `Instant` sits in one `Stamp` newtype with scoped `#[expect(clippy::disallowed_types, reason = "observe-only wall clock (spec §15)")]`. `Clone` is written by hand because a derived `Clone` re-names the field type outside the expect. `SystemTime` (run ids and timestamps) uses the same kind of expect in `logging.rs`.
- `sink.rs`: `JournalSink::create` creates `journal.jsonl`, and `write` writes one JSON object per entry and flushes.
- `debug.ron`: the defaults plus `per_phase_ms` of 3.0 for all 8 phases. A unit test parses it.
- Extra public items: `JsonFileLayer` (the concrete layer type), and the constants `WINDOW`, `PHASE_TARGET` and `BUDGET_TARGET`.

## Tests (14)

- `tests/profiler.rs`:
  - `feature_of_maps_paths`
  - `phase_times_from_marker_events`: 8 phases × 30 ticks, samples == 30 each, first-seen order.
  - `system_spans_on_another_thread`: sim systems on a spawned thread inside ticks, plus a client system on another thread folded by a `frame` span.
  - `budget_overrun_warns`: a capture layer on `fl_observe::budget`.
  - `json_layer_writes_tick_field`: the event is inside a parentless `system` span within `sim_tick{tick=3}`, which yields `"tick":3`.
  - `journal_sink_writes_jsonl`
- Unit tests:
  - `percentiles_nearest_rank`
  - `window_keeps_the_last_240`
  - `feature_of_edge_cases`
  - `debug_config_defaults_and_rust_log_override`
  - `repo_debug_ron_parses`
  - `utc_calendar`
  - `run_dirs_never_collide`
- `tests/profiler_sim.rs`, `profiler_times_real_sim_systems`: real fl_sim with the core_mod duel, `init_headless` with a run dir, 30 ticks stepped on a spawned thread. Asserts:
  - `fl_sim::movement::move_system` is present with feature `core.movement` and samples ≥ 30;
  - ticks == 30;
  - `log.jsonl` has the `fl_sim::step` "state hash" records at ticks [0, 20], each with `tick`.

## TDD evidence

RED: `cargo nextest run --profile agent -p fl_observe` (with the tests written and the crate still a stub):
```
error[E0432]: unresolved imports `fl_observe::Budgets`, `fl_observe::JournalSink`, `fl_observe::Profiler`, `fl_observe::feature_of`, `fl_observe::json_file_layer`, `fl_observe::new_run_dir`
error[E0432]: unresolved imports `fl_observe::DebugConfig`, `fl_observe::init_headless`
error[E0425]: cannot find function `global_profiler` in crate `fl_observe`
```
This was expected: the API did not exist yet.

GREEN: same command:
```
    Starting 14 tests across 3 binaries
     Summary [   0.696s] 14 tests run: 14 passed, 0 skipped
```
`cargo clippy -p fl_observe --all-targets -- -D warnings` is clean.

## Gate

`cargo xtask gate task --track tools` (`.gate-base` 8cfc38e):
```
PASS fmt (4.1s)
PASS clippy-drift (5.4s)
PASS claude-md (0.0s)
PASS check-scope (0.5s)
PASS protected (0.5s)
PASS test-guard (0.8s)
PASS clippy (54.1s)
PASS arch-check (5.1s)
PASS test-headless (88.2s)
SKIP test-client: no client package touched since 8cfc38e (.gate-base)
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: PASS in 160s
```

## Files changed

- `crates/fl_observe/src/{lib,config,logging,profiler,sink}.rs`
- `crates/fl_observe/tests/{profiler,profiler_sim}.rs`
- `debug.ron`
- No `Cargo.toml` or `Cargo.lock` change was needed: the crate manifest and the workspace deps (`tracing-subscriber` with `env-filter`/`json`/`fmt`/`registry`) were already in place.

## Concerns and deviations

1. **No `RunConfig::phase_markers` and no threaded runner at 8cfc38e.** Task 12's runner is not in this base. `profiler_sim` therefore uses `RunConfig::default()` and steps the sim with `step_tick` on a spawned thread, as the runner will. Phase timing against real markers is untested until Task 18 lands; the synthetic test uses the exact target and fields. Once T12/T18 merge, the test can switch to `RunConfig { phase_markers: true, .. }` and the runner, and also assert `phases.len() == 8`.
2. **`Budgets` has a manual `Default`, not `derive(Default)`.** The brief's derive would give `tick_ms: 0.0`. With `#[serde(default)]`, a `debug.ron` that omits `tick_ms` would then warn on every tick. The manual default gives the documented 15.0.
3. **Budget warnings are rate-limited** to once per budget per 240 folds. This is not in the brief; it was added to avoid a 20 Hz warning flood. `set_budgets` resets it.
4. **`Stat.samples` is the lifetime count**, while mean, p50, p95 and max cover the 240-sample window.
5. The `frame` span that folds client systems is not emitted anywhere yet. fl_client or fl_game must open `info_span!("frame")` per frame for client system stats to appear.
6. The rev-1 brief's `scripts/check_deps.sh` edit does not apply: the script no longer exists, and `xtask arch-check` already lists fl_observe as a tool crate.
