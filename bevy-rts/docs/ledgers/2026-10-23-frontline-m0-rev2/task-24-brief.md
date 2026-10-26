### Task 24: `fl_cli` agent commands with `--json` (rev-1 T20)

**Track:** tools · **Refs:** ADR-0007, ADR-0006 · **Lane 2**

**Files:**
- Create: `crates/fl_cli/src/{agent,inspect,journal_cmd,save_cmd}.rs`, `crates/fl_cli/tests/agent.rs`
- Modify: `crates/fl_cli/src/{lib,main}.rs`

**Interfaces:**
- Consumes: `Replay`, `ReplayRecorder`, `verify_replay`, `first_divergence` (Task 18); `save_world`, `load_world` (Task 13); `fl_observe::{init_headless, new_run_dir, JournalSink}`.
- Produces: a global `--json` flag. With it, every subcommand prints exactly one JSON document on stdout and logs go to stderr.

| command | behaviour |
|---|---|
| `run-scenario <ID\|path.scenario.ron> --ticks N [--seed S] [--trace] [--dump-state-every K] [--out <dir>]` | Runs headless and writes `<run dir>/{log.jsonl, journal.jsonl, run.replay, states.jsonl}`. JSON: `{run_dir, ticks, final_hash, rules_hash, journal_entries, rejected_commands}` |
| `inspect <run.replay\|scenario\|file.save> --tick T [--entity N]` | Re-simulates to tick T; prints `inspect_world`, or `inspect_entity` with `--entity` |
| `journal <run dir\|journal.jsonl> [--kind K] [--sim-id N] [--player P] [--from T] [--to T] [--limit L]` | Filtered entries in tick order |
| `replay-verify <file.replay>` | Exit 0 when Verified. Exit 1 with `{tick, expected, actual}` on divergence. Exit 3 with `"non-reproducible"` |
| `replay dump <file.replay> --json` | `Replay::to_json` |
| `save <ID\|file.replay> --tick T --out <file.save>` | Runs to T and writes `save_world` |
| `load <file.save> --ticks N` | Continues N ticks and prints the final hash |
| `bisect-desync <a> <b>` | Replays or run dirs, re-simulated with hash interval 1. Reports the first divergent tick (the tick whose inputs differ, because hashes are taken after each tick) and the ids whose `inspect_entity` differs there |
| `data-check`, `hash-dump`, `bench` | Gain `--json` |

- [ ] **Step 1: Write the failing tests** (`tests/agent.rs`; they run the binary with `--json` and parse stdout):
```rust
#[test] fn run_scenario_json_outputs_paths()     // the files exist; journal_entries ≥ 1
#[test] fn inspect_entity_at_tick()              // base.testbed, tick 10, entity 1 → JSON with components["core.pose"]
#[test] fn journal_filter_by_kind()              // --kind core.unit_spawned → only that kind
#[test] fn replay_verify_ok_and_tampered()       // ok → exit 0; one command target edited → exit 1 with "tick"
#[test] fn replay_dump_json_lists_inputs()
#[test] fn save_then_load_continues_identically()   // 100-tick final hash == save at 40 + load + 60 ticks
#[test] fn bisect_desync_finds_tick()            // two replays differing by one command at tick 37 → first divergence 37
#[test] fn scenario_file_path_accepted()
#[test] fn non_reproducible_replay_reported()    // a replay with a ContentReload event → exit 3
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_cli --test agent`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track tools`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_cli): agent debugging commands (run, inspect, journal, replay verify/dump, save/load, bisect) with --json`.

