# Task 24 report: `fl_cli` agent commands with `--json` (rev-1 T20)

Worktree `C:\fl\t24`, branch `m0/t24-agentcli`, base `3c856ff` (Task 23 bench).

## Commits

| SHA | Subject |
|---|---|
| `b7fbac3` | `fix(fl_cli): bench output states the profiler window` (controller ruling R58, Task 23 review fixes) |
| `d9f563c` | `feat(fl_cli): agent debugging commands (run, inspect, journal, replay verify/dump, save/load, bisect) with --json` |

## R58 bench fixes (`b7fbac3`)

1. The markdown meta line now says `profiler window: last N ticks; wall times include profiler and phase-marker overhead`, and the phase section is titled `Mean ms per phase (profiler window: last N ticks)`. `N = BenchConfig::profiler_window_ticks() = min(ticks, fl_observe::WINDOW)`. The JSON `meta` gains `profiler_window_ticks` and a `note`. Every profiler stat gains `window_samples` (`min(samples, 240)`) next to `samples`.
2. `instrumented_hash_unchanged` takes the profiler-off hash before `init_headless` and the profiler-on hash after it. The comment notes that nextest runs each test in its own process.
3. `--repeat 0` is treated as 1, both in the run and in the meta text (`BenchConfig::runs()`). Docs on `BenchConfig`, `run_bench_full` and the CLI help state that an even count takes the upper-middle run.
4. `compare` fails when the run and the baseline share no size. A baseline from another build type prints a warning on stderr. `--profile` without `--json` is a clap usage error, exit 2.
   - After Task 24 this becomes "needs `--json` or `--json-out`", also exit 2.

New tests: `compare_without_common_sizes_fails`, `markdown_states_the_profiler_window`, `repeat_zero_runs_once`, and `window_samples` asserts in `profile_json_has_features_and_phases`. Gate for this commit: `gate task: PASS in 210s`. The only finding was the test-guard WARN below.

## What Task 24 implemented (`d9f563c`)

**Global `--json`** (`#[arg(long, global = true)]`, so it works before or after the subcommand). With it, stdout carries exactly one JSON document. Errors are JSON too (`{"error": "..."}`), including clap usage errors (`try_parse`, then `err.exit()`). Logs go to stderr.

**Exit codes:**

| code | meaning |
|---|---|
| 0 | ok |
| 1 | failure: an error, a failed check, a divergence, a bench regression, an entity not found |
| 2 | usage (clap's errors and `CliError::Usage`) |
| 3 | non-reproducible replay |

**Files:**
- `src/agent.rs`:
  - `Report {json, text, code}`, `hash_hex`, `Game::load` (catalog features plus `--mods`, default the base mod), `init_logs`.
  - `Source::parse`: a directory is a run dir and uses its `run.replay`; `.replay`; `.save`; `.ron` is a scenario file; anything else is a scenario id.
  - `Session`, built on `SimRunner` plus a boxed `TickDriver` and a `ReplayRecorder`. It provides `open`, `step`, `run_to(T)`, `pending`, `replay`.
  - The commands `run_scenario`, `replay_verify`, `replay_dump`.
- `src/inspect.rs`: `inspect` and `bisect_desync`.
- `src/journal_cmd.rs`: `JournalFilter`, `read_journal` (reads line by line and keeps only matches), `journal`.
- `src/save_cmd.rs`: `save` and `load`.
- `data_check.rs`, `hash_dump.rs` and `bench.rs` gain `*_report` builders, so `main.rs` only parses arguments, dispatches and prints. `CliError` gains `Check` and `exit_code()`.

**Commands:**
- `run-scenario <id|*.scenario.ron> --ticks N [--seed S] [--wander R] [--trace] [--dump-state-every K] [--out <root>] [--mods ...]`
  - Creates `<root>/<run id>/` with `new_run_dir`; `--out` defaults to `logs`.
  - Writes `log.jsonl` (the `init_headless` JSON log; filter `info,fl_sim=debug`, or `trace` with `--trace`), `journal.jsonl` (`JournalSink`; the journal is forced on), `run.replay` and `states.jsonl`. Each states line is `{"world": inspect_world, "entities": [inspect_entity...]}`, written whenever the clock is a multiple of K, tick 0 included.
  - JSON output: `{run_dir, ticks, final_hash, rules_hash, journal_entries, rejected_commands}`. Hashes are printed as `0x%016x` strings.
- `inspect <source> --tick T [--entity N]`: re-simulates until the clock reads T, then prints `inspect_world` or `inspect_entity`. A missing entity exits 1. A tick past a replay's end, or before the source's start, exits 2.
- `journal <run dir|journal.jsonl> [--kind] [--sim-id] [--player] [--from] [--to] [--limit]`:
  - JSON: `{"entries": [...], "matched": n}`, where `matched` counts matches before `--limit`. The result is stable-sorted by tick.
  - Text: one JSON line per entry.
- `replay-verify <file>`:
  - exit 0: `{"status": "verified", ticks, warnings}`
  - exit 1: `{"status": "diverged", tick, expected, actual}`
  - exit 3: `{"status": "non-reproducible"}`
- `replay dump <file>`: `Replay::to_json()` with `--json`, otherwise a text summary.
- `save <source> --tick T --out f.save`: `save_world` with the driver's pending commands. For a replay source those are the replay's remaining inputs, so `load` plays the rest of the replay; I checked this by hand: run 60 → save the run dir at 20 → load 40 gives the same final hash. JSON: `{out, tick, hash, entities, pending_commands}`.
- `load f.save --ticks N`: JSON `{start_tick, ticks, tick, final_hash}`.
- `bisect-desync <a> <b>` (replays or run dirs):
  - Both runs are re-simulated side by side with hash interval 1. It first compares the setup state hash, then each tick's hash.
  - It reports the first tick whose hash differs. The hash is taken after the tick runs, so this is the tick whose inputs differ: the test with commands differing at tick 37 reports 37.
  - It also reports the ids whose `inspect_entity` differs right after that tick. Fields: `{diverged, tick, setup, hash_a, hash_b, entities, ticks_compared}`.
  - Exits 1 when the runs diverge, 0 when they are identical (`diff` semantics).
- `data-check`, `hash-dump`, `bench` use the global `--json`:
  - `data-check` failure gives `{"errors": [...]}`, exit 1.
  - `bench --json` prints the document on stdout (`--profile` adds the breakdown). With a baseline it adds `"baseline": {"ok": ...}`, and a regression exits 1.

**Design decisions:**
- **Scenario options (`--seed`, `--wander`) apply to scenario sources only**; giving them for another source is a usage error. `--wander R` is an addition to the brief's table. It mirrors `ScenarioBuilder::wander_all` and the golden `testbed_wander`: before tick 0 every player issues `debug.wander` over all its units, and cheats are turned on for that match. Without it, a testbed run has no commands, so it has no journal entries and no motion. `run_scenario_json_outputs_paths` (journal_entries ≥ 1) and a non-vacuous save/load check need it.
- **A scenario file's replay starts from `ReplayStart::Save`** of the world as set up, the same approach as `ScenarioBuilder`, because the scenario is not in the content. Its `run.replay` verifies: tested.
- **"At tick T"** means the clock reads T, i.e. ticks 0..T-1 have run. `inspect --tick T`, `save --tick T` and `inspect_world.tick` all use this.

## Tests

`tests/agent.rs`, 10 tests. Each runs the binary with `--json` and parses stdout with `serde_json::from_str`, which rejects anything after the first document.
- `run_scenario_json_outputs_paths`: the 4 files exist, and `journal_entries` equals the line count of `journal.jsonl` and is ≥ 1. `log.jsonl` is not empty. `states.jsonl` holds ticks [0, 10, 20, 30] with 10 entities and `core.pose`. `run.replay` decodes and passes `replay-verify`.
- `inspect_entity_at_tick`: base.testbed, tick 10, entity 1 has `components["core.pose"]`. The world at tick 10 has `tick == 10`. Entity 99 exits 1 with an error that names it.
- `journal_filter_by_kind`:
  - Uses a real journal from `ScenarioBuilder` (move commands plus `debug.spawn`; at least 3 kinds). `--kind core.unit_spawned` returns exactly 1 entry, at tick 5.
  - Combined player, from, to and limit filters cut at the limit while `matched` stays the full count.
  - `--sim-id` works, and a run directory works as the input.
- `replay_verify_ok_and_tampered`: the untouched replay is `verified` (60 ticks). Re-encoding the tick-37 move target gives exit 1, `tick == 37`, and expected ≠ actual.
- `replay_dump_json_lists_inputs`: the output equals `Replay::to_json()`, with input ticks [0, 10, 37]. This test uses the spec's `replay dump f --json` form.
- `save_then_load_continues_identically`: the final hash of a 100-tick run equals save at 40 plus load of 60 ticks. The hash at 40 differs from the final one.
- `bisect_desync_finds_tick`: the reported tick is 37 and `entities == [1]`. A run dir against its own `run.replay` is identical over 20 ticks.
- `scenario_file_path_accepted`: a temporary copy of the testbed under another id gives the same final hash as `base.testbed`. Its `run.replay` (Save start) verifies, and `inspect <file>` works.
- `non_reproducible_replay_reported`: a `ContentReload` event gives exit 3 and `non-reproducible`.
- `errors_are_json_too`: a missing replay exits 1 with `{"error"}`. Clap usage errors and `--dump-state-every 0` exit 2 with JSON.

Carried T16 minors, in `tests/data_check.rs`:
- `data_check_base_mod_succeeds`: the binary exits 0 and prints the exact `rules hash: 0x…` line; the `--json` form works too.
- `data_check_kinds_sorted`: the kinds are explicitly `sort_unstable`d.
- `hash_dump_text_output`: the binary's text lines equal the `hash_dump()` result.

In `tests/bench.rs`: `bench_json_on_stdout`.

### TDD evidence

**RED.** `cargo nextest run --profile agent -p fl_cli`, after writing the tests and before the implementation. This was expected: the binary had no global `--json` and none of the new subcommands.
```
     Summary [   1.636s] 26 tests run: 14 passed, 12 failed, 0 skipped
    thread 'inspect_entity_at_tick' (28908) panicked at crates\fl_cli\tests\agent.rs:30:9:
    error: unexpected argument '--json' found
```
The 12 failures were all 10 agent tests, `bench_json_on_stdout` and `data_check_base_mod_succeeds`, all on `--json`/subcommand parsing. `hash_dump_text_output` and `data_check_kinds_sorted` passed straight away: the behaviour already existed, and these tests are regression coverage.

**GREEN.**
```
cargo nextest run --profile agent -p fl_cli --test agent
     Summary [   1.748s] 10 tests run: 10 passed, 0 skipped
cargo nextest run --profile agent -p fl_cli
     Summary [   2.440s] 26 tests run: 26 passed, 0 skipped
```

### Gate (`d9f563c`, `.gate-base` = 3c856ff)
```
PASS fmt (3.3s)
PASS clippy-drift (5.7s)
PASS claude-md (0.0s)
PASS check-scope (0.5s)
PASS protected (0.5s)
WARN test-guard (2.0s): test-guard: 1 findings (reviewer must accept each)
    TEST-GUARD crates/fl_cli/tests/bench.rs:109 RemovedAssert: assert_eq!(final_hash(true), final_hash(false));
PASS clippy (16.6s)
PASS arch-check (7.8s)
PASS test-headless (42.9s)
SKIP test-client: no client package touched since 3c856ff (.gate-base)
PASS data-check (9.1s)
gate task: PASS in 89s
```
The test-guard finding is R58 item 2. The assert was not removed but reordered: `let plain = final_hash(false)` now runs before `init_headless`, followed by `assert_eq!(final_hash(true), plain)`.

## Files changed
- Created: `crates/fl_cli/src/{agent,inspect,journal_cmd,save_cmd}.rs`, `crates/fl_cli/tests/agent.rs`
- Modified: `crates/fl_cli/src/{lib,main,bench,data_check,hash_dump}.rs`, `crates/fl_cli/tests/{bench,data_check}.rs`
- No change to `Cargo.toml` or `Cargo.lock`: every dependency was already present.

## Concerns
1. **fl_observe bug, worked around here and not fixed.** `observe_layers` with `DebugConfig.profiler = false` silences all logging, both stderr and `log.jsonl`.
   - Cause: the profiler layer is an `Option<Filtered<…>>`, and tracing-subscriber 0.3.23's `impl Layer for Option<L>` returns `max_level_hint() = Some(LevelFilter::OFF)` for `None`. I verified this in the crate source and by experiment: with `profiler: false`, `run-scenario --trace` logged nothing.
   - Workaround: `fl_cli::init_logs` always sets `profiler: true`, with a comment explaining why.
   - Proper fix, for the owner of fl_observe (tools track; I did not touch it because it is not in this task's Files): always install the layer, gated by a filter such as `filter_fn(move |m| enabled && is_profiled(m))`, or return a `None` hint.
   - Any other user that runs `init_headless` with `profiler: false` currently gets no logs.
2. **`bench --json <PATH>` is renamed to `--json-out <PATH>`**, because `--json` is now the global flag. Spec §15 D line 1024 still says `fl_cli bench --profile --json`. `fl_cli --json bench --profile > docs/perf/<m>.json` or `--json-out` now does that job. I found no scripts or xtask callers.
3. **Additions beyond the brief's table:**
   - `--wander R` (needed for journal and activity, see above).
   - `--mods` on the agent commands, defaulting to the base mod.
   - The extra JSON fields `matched` (journal), `status` (replay-verify), `hash_a`/`hash_b`/`setup`/`ticks_compared` (bisect), and `entities`/`pending_commands` (save).
4. **`bisect-desync` exits 1 when the runs diverge** (`diff` semantics). `replay-verify` errors and divergences both exit 1; the JSON tells them apart (`error` or `status`).

---

## Fix round 1 (commit `45e1173` `fix(fl_cli,fl_observe): logs without profiler, non-vacuous bisect, JSON help`)

### Changes
1. **fl_observe logging bug (R64).**
   - `observe_layers` now always installs the profiler layer and switches it off through its filter (`filter_fn(move |m| enabled && is_profiled(m))`). The old `Option` layer reported `LevelFilter::OFF` when it was `None`.
   - New test `fl_observe::profiler::logs_without_profiler`: with `profiler: false`, `log.jsonl` gets the info record (with `tick`), the debug record is filtered out, and the profiler sees nothing.
   - `fl_cli::init_logs` is back to `profiler: false`; the workaround is gone.
   - `run_scenario_json_outputs_paths` now also checks that the logs reach stderr.
2. **bisect-desync.**
   - The JSON always carries `ticks_a`/`ticks_b`.
   - If the runs match over their common ticks but differ in length, it exits 1 with `diverged: false` and a `message` containing "different lengths".
   - `compared == 0` is `CliError::Check("nothing to compare: …")`, exit 1.
   - Tests: a 30-tick prefix against the full replay, and an empty replay.
3. **Help and version.**
   - The `--json` pre-scan uses `std::env::args_os()`.
   - `--json --help` prints `{"help"}` and `--json --version` prints `{"version"}`, both exit 0; I added `version` to the clap command. Other clap errors print `{"error"}` and keep clap's exit code.
   - Test: `help_and_version_are_json`.
4. **Save from a run dir.** New check in `save_then_load_continues_identically`: run 60 ticks → `save <run dir> --tick 20` → `load --ticks 40` gives the same final hash.
5. **Smaller items.**
   - `run-scenario` checks that the scenario exists before it creates the run dir. Test: an unknown id exits 1 and leaves the out dir empty.
   - The `--wander` missing-value test now also asserts the `{"error"}` JSON.
   - New `bench_json`-style binary test `baseline_build_mismatch_warns`.
   - `compare` returns a typed `CompareError::{NoCommonSize, Regression}`, matched in `bench_report`; the `expect_err` pre-check is gone.
   - `Report`, `CliError` and `hash_hex` moved to `src/report.rs`.

### Evidence
RED for the fl_observe test, run before the fix:
```
cargo nextest run --profile agent -p fl_observe -E 'test(logs_without_profiler)'
    thread 'logs_without_profiler' panicked at crates\fl_observe\tests\profiler.rs:299:5:
    assertion `left == right` failed:   left: 0   right: 1
```
GREEN:
```
cargo nextest run --profile agent -p fl_observe
     Summary [   0.596s] 15 tests run: 15 passed, 0 skipped
cargo nextest run --profile agent -p fl_cli
     Summary [   2.036s] 28 tests run: 28 passed, 0 skipped
```
Gate (`.gate-base` = 3c856ff):
```
PASS fmt (1.5s)
PASS clippy-drift (2.3s)
PASS claude-md (0.0s)
PASS check-scope (0.3s)
PASS protected (0.3s)
WARN test-guard (1.2s): test-guard: 2 findings (reviewer must accept each)
    TEST-GUARD crates/fl_cli/tests/bench.rs:60 RemovedAssert: assert!(err.contains("500 units"), "{err}");
    TEST-GUARD crates/fl_cli/tests/bench.rs:109 RemovedAssert: assert_eq!(final_hash(true), final_hash(false));
PASS clippy (7.4s)
PASS arch-check (3.7s)
PASS test-headless (34.0s)
SKIP test-client: no client package touched since 3c856ff (.gate-base)
PASS data-check (5.0s)
gate task: PASS in 56s
```
Both test-guard findings are rewrites, not removals:
- `bench.rs:60`: the assert is now `err.to_string().contains("500 units")`, plus a match on `CompareError::Regression` with one entry.
- `bench.rs:109`: the reorder from R58.

Deferred as R63: a shared `fl_sim::replay_app`. No action taken.
