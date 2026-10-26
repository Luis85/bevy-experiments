# Task 1 report: gate time trim and test-guard allowlist (M17, M16)

Status: DONE_WITH_CONCERNS. Commit f97ce6e.

## Implemented
- `xtask/src/gate.rs`
  - `LEVEL_BUDGET`, `SMOKE_BUDGET`, `budget()`; every level's footer prints `WARN gate <level> took <n>s (budget <b>s)` over budget.
  - `heavy_filter(root)` / `parse_heavy` (errors name `xtask/heavy-tests.txt:<line>`); `task_headless_filter(.., heavy)` appends `& not (<heavy>)`; `headless_filter(level, ..)` keeps pr/nightly/milestone free of the heavy exclusion.
  - `test_headless` is now `headless_run(ctx, log, touched)`; `task_time` (nightly-only row) runs it twice (touched = [] and [fl_sim, fl_test_utils]) at task level; `check_task_times` fails naming both times.
  - `append_gate_times` writes one JSON line per gate run to `target/xtask/gate-times.jsonl` (called from `gate::run`).
  - `test-guard` row uses `run_accepted` and the two-argument `summary`.
  - `Ctx` derives `Clone`.
- `xtask/src/test_guard/accepted.rs` (new): `Accepted`, `load_accepted`, `suppress`; `mod.rs` gains `FindingKind::from_name`, `run_accepted`, `summary(n, suppressed)`; `main.rs` `test-guard` subcommand uses them.
- `OWNERS.toml` `[protected]` gains `xtask/heavy-tests.txt` and `xtask/test-guard-accepted.toml`.
- `xtask/heavy-tests.txt` (24 tests, measured), `xtask/test-guard-accepted.toml` (seeded, see below).

## Heavy list (measured)
Ran `cargo nextest run --profile agent -p fl_sim -p fl_test_utils -p fl_catalog -p fl_content -p fl_cli` (585 passed), read `junit.xml`. Rule applied: >10 s non-smoke non-milestone tests (`incremental_nav_equals_full_rebuild` 13 s, `invalidate_equals_full_rebuild` 12 s, `layers_round_trip` 11 s), every `scale_*` and `counters_*` test, plus the three named M17 tests. The 12 `smoke_` tests total 29 s (< 120 s), so no `smoke_` test is listed. Times were taken under concurrent lane-2 load.

## Allowlist seed
The brief said to start empty; the dispatch said to seed with M1's accepted findings. I seeded 39 entries: `cargo xtask test-guard --base m0` over the M1 range, all of which the M1 ledger records as accepted per task (ledger lines name counts and rulings, not texts, so rulings are `R109` for commands.rs:705, `M1 ledger: Task N review` where the task is clear from the file, else `M1 ledger`). After seeding: `test-guard: 0 findings (39 accepted findings suppressed)`.

## Tests
- `cargo nextest run --profile agent -p xtask`: 109/109 pass (new: heavy_filter_lists_each_test, heavy_line_needs_three_fields, committed_heavy_list_parses, task_filter_excludes_heavy, pr_runs_heavy_tests, task_time_row_is_nightly_only, task_time_fails_over_either_budget, level_budget_warns, gate_times_appended, summary_counts_suppressed, accepted_findings_are_suppressed, accepted_entry_needs_ruling, committed_allowlist_parses). No separate RED run was captured (tests and code were written together); the tests are the ones from the brief's list.
- `cargo xtask gate task --base 3d78b45` (Step 5): `test-headless` PASS in 404 s total, of which 277 s was the cold unified-workspace compile and 89 s the tests (514 tests run, 55 skipped); before: 916 s (ledger line 76). Scope of that run: every package touched since 3d78b45, which is larger than M1 Task 14's own diff, so it runs more tests than the plan's scenario. Whole gate 1068 s on that cold build dir; protected row failed as expected on `docs/perf/M1.json` and the two new protected files.
- `cargo xtask gate task --track contract` (Step 6), warm: fmt, clippy-drift, claude-md, clippy (110 s), arch-check, test-headless (98 s, xtask + smoke set), data-check PASS; whole gate 264 s. Last line is `gate task: FAIL (check-scope, protected)`, not PASS, for reasons outside the code: check-scope flags the docs agents' files (`docs/design/**`, plan) as outside `contract`, and protected flags `docs/design/mechanics/*.md` and my two new protected files until the lead blesses them. test-guard WARN: 2 RemovedAssert findings at `xtask/src/gate.rs` (content_change test: the two `task_headless_filter(.., excl)` asserts now pass a `None` heavy argument). The lead should add those two to the allowlist (or accept them).
- The nightly `task-time` row itself was not run (nightly takes hours); its decision logic is unit tested via `check_task_times`.

## Concerns
1. The Step 6 expected `gate task: PASS` cannot occur until the lead blesses the two protected files and the docs agents' scope is separated.
2. The Step 5 target (<= 150 s) holds for test execution (89 s) but the row time includes a cold compile (277 s).
3. Two gate.rs test-guard findings above need the lead's acceptance.
4. `scale_*` also matches two cheap `fl_cli` map_gen tests (1 s each); kept verbatim to the brief.
5. Tooling note: a shared `/tmp/edit2.py` from another session existed; I did not run it.

## Fix round 1 (R154)
- RED: after adding the new tests, `cargo nextest run --profile agent -p xtask` failed to compile: `cannot find function judge_task_time / parse_heavy_entries / stale_heavy`, `Accepted has no field count/note/test`, `Finding has no field test`, `FindingKind has no ALL`. GREEN: 118/118 xtask tests pass.
- Important 1+2: `Finding` gains `test` (the enclosing fn, base-side for removed asserts/tests; rendered as `Kind in <fn>:`). Allowlist key is (file, test, kind, whole normalized text) with `count` (default 1, 0 is an error); a group of identical findings is suppressed only while its size <= the summed count, otherwise all are reported. Tests: same text/other test stays, a third identical finding is reported.
- Important 3: re-seeded from `cargo xtask test-guard --base m0` run in a worktree at tag m1: 39 findings, 38 entries (one with `count = 2`), all `ruling = "R154"`; a `note` only where the ledger names the task review (Task 2 debug_query, Task 6 astar prop_assume, Task 9 orders.rs seed, Task 10/R109 commands.rs). The two gate.rs M1 entries no longer cite Task 13. Verified: the m1 worktree with the new file reports 0 findings, 39 suppressed. Plus the two new gate.rs false positives (R154): 40 entries, 41 findings.
- Important 4: `task_time` builds once untimed (`nextest run --no-run`, log `task-time-build`), times only test execution, runs both with `content: false`.
- Minor 5: `judge_task_time`: a Skip/Warn run gives a Warn row; Fail passes through. Minor 6: tests that content tests survive the heavy filter and that the committed list names no fl_catalog / golden test. Minor 7: `stale_heavy_outcome` (task-level test-headless) runs `cargo nextest list --message-format oneline -E <heavy>` and fails naming entries that match no test. Minor 9: different-text/test tests; `FindingKind::ALL` + exhaustive `ordinal` + round-trip test replace the hand-copied list (a new variant fails to compile in `ordinal`, then the test until ALL is updated). Minor 8 left.
- Gate: `cargo xtask gate task --track contract` -> `gate task: FAIL (check-scope, protected)` in 128 s; only the external rows fail (docs agents' files; protected: docs/design/mechanics and the two protected xtask files awaiting bless). test-headless PASS 86.7 s (includes the stale-heavy check), test-guard PASS (0 findings).
