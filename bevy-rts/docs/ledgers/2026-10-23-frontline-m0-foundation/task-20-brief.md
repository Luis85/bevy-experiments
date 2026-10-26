### Task 20: `fl_cli` agent commands (`--json` everywhere)

Phase C2, after T16. It owns `crates/fl_cli/src/{agent,inspect,journal_cmd}.rs` and modifies `lib.rs` and `main.rs`.

**Interfaces — Produces.** A global `--json` flag. With it, every subcommand prints exactly one JSON document to stdout; logs go to stderr. Without it, output is human-readable.

| command | behaviour |
|---|---|
| `run-scenario <ID|path.scenario.ron> --ticks N [--seed S] [--trace] [--dump-state-every K] [--out <dir>]` | Runs headless. Writes `<run dir>/{log.jsonl, journal.jsonl, replay.json, states.jsonl}`; states holds `inspect_world` plus every entity every K ticks. `--trace` sets the filter to `trace`. JSON output: `{run_dir, ticks, final_hash, rules_hash, journal_entries, rejected_commands}`. |
| `inspect <replay.json|scenario> --tick T [--entity N]` | Re-simulates to tick T and prints `inspect_world`, or `inspect_entity` when `--entity` is given. |
| `journal <run dir|journal.jsonl> [--kind K] [--sim-id N] [--player P] [--from T] [--to T] [--limit L]` | Filters and prints entries in tick order. |
| `replay-verify <replay.json>` | Exit 0 if every hash matches. Otherwise exit 1, printing `{tick, expected, actual}`. |
| `bisect-desync <a> <b>` | `a`/`b` are replays or run dirs. It re-simulates both with hash interval 1 and reports the first divergent tick, plus `inspect_world` diffs of both at that tick (entity ids whose dumps differ). |
| `data-check`, `hash-dump`, `bench` | Gain `--json`. |

- [ ] **Step 1: Write the failing tests** (`tests/agent.rs`; run the binary with `--json` and parse stdout with serde_json).
  - `run_scenario_json_outputs_paths`: the files exist and the journal has ≥ 1 entry.
  - `inspect_entity_at_tick`: base.testbed at tick 10, entity 1, gives a JSON object with `components.Pose`.
  - `journal_filter_by_kind`.
  - `replay_verify_ok_and_tampered`: an edited replay exits 1 with the tick.
  - `bisect_desync_finds_tick`: two replays that differ by one command at tick 37 give a first divergence at tick 37, or 38 if the hash is recorded after the tick; document which.
  - `scenario_file_path_accepted`.
- [ ] **Step 2: Run** `cargo test -p fl_cli --test agent`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo test -p fl_cli --test agent`. Expected: PASS.
- [ ] **Step 5: Gate + commit** `feat(fl_cli): agent debugging commands with --json`.

