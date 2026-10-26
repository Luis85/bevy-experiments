### Spec Compliance

- ✅ Spec compliant for the code: every interface in the brief exists with the stated behavior (LEVEL_BUDGET, SMOKE_BUDGET, heavy_filter, task_headless_filter(.., heavy), task_time, append_gate_times, Accepted/load_accepted/suppress, summary(n, suppressed)); OWNERS.toml `[protected]` gains both files.
- Deviation: the brief says to start the allowlist empty ("only the lead adds entries"); the implementer seeded 39 entries on the lead's dispatch. Judged below.
- ⚠️ Cannot verify from the diff: the nightly `task-time` row was never run; Step 5/6 numbers come from the report (Step 6 did not reach `gate task: PASS`).

### Strengths
- Heavy exclusion is applied only on the task path: `headless_filter` (gate.rs ~400) returns `not (excl)` for pr/nightly/milestone, and `pr_runs_heavy_tests` pins both sides. All 24 heavy lines name a real file and fn (checked each against crates/*/tests/*.rs). fl_cli and fl_content are in the headless set (arch.rs:47, 68), so pr/nightly/milestone run all of them. No heavy test is dropped from every level.
- I3 holds: no heavy line names fl_catalog or the golden binary, and `& not (heavy)` is appended after the content term (precedence is correct, since each heavy term is parenthesised).
- Parsing is strict and tested (field count with line number, comments, blanks, missing file = none). The committed heavy list and allowlist both have parse tests.
- Budget footer, nightly-only row, JSONL log and the `check_task_times` message naming both times match the brief.

### Issues

#### Critical (Must Fix)
None.

#### Important (Should Fix)
1. xtask/test-guard-accepted.toml, whole file and accepted.rs:809-823: the allowlist has no multiplicity. `suppress` uses `any()`, so one entry hides every identical (file, kind, text) finding, including a new one. The file has 3 identical astar.rs NewAssume entries (lines 1042-1058) and 2 identical movement.rs `assert!(order.waypoints.is_empty());` entries, which are redundant as written. Each should be one entry with a `count`, or the suppressed count should be capped per entry.
2. xtask/test-guard-accepted.toml:1024-1028, 1219-1220, 1207-1208, 1231-1232 etc.: some keys are fragments, not statements: `let margin = world`, `shuffle_seed,`, `for seed in shuffle_seeds() {`, `let mut runner = TestRunner::new_with_rng(`. Any later NewSeedOverride or ToleranceChanged on such a line in that file is silently hidden. That undermines the stated purpose, that a NEW same-kind finding in the same file is still reported. Use a test (function) key or a stronger text. The M17-era dispatch said keys are (file, test, kind), but the implementation and brief key on (file, kind, text) with no test name.
3. xtask/test-guard-accepted.toml:1234-1244: the two gate.rs entries are ruled "M1 ledger: Task 13 review", but the M1 ledger's Task 13 (lines 78-85) is the scale scenarios and records no test-guard acceptance. These are the old `task_filter(&touched,&smoke,excl)` asserts, probably the "3 false positives" of the fix wave (ledger line 118) or the I3 change. The provenance is wrong. Spot-check of 10 seeded entries against the ledger: debug_query should_panic (Task 2) ✓; astar prop_assume (Task 6, 3 accepted) ✓; commands.rs margin (R109) ✓; orders.rs shuffle_seed (Task 9 NewSeedOverride) ✓; hpa/movement_determinism seed overrides (generic NewSeedOverride, plausible) ✓; steering prop_assume (fits ledger lines 51/75) ~; commands.rs:705-style ToleranceChanged in movement.rs (two entries) is attributed only to "M1 ledger" ~; observe/save/feature/hash RemovedAssert (Tasks 7/8, ledger gives counts only) ~; perf.rs RemovedAssert (not named in the ledger) ~; gate.rs ✗. About 27 of 39 rulings are the non-id "M1 ledger", while the brief wants a ruling id. The seeding was derived by running test-guard over the M1 range and accepting everything, so the claim "all recorded as accepted" is unverified against the ledger's per-task counts.
4. gate.rs `task_time` (~470-489): the smoke run is timed with its compile. At nightly, the earlier rows build per-package (`-p`), but `headless_run` at Task level uses the unified `--workspace` selection; the report itself saw 277 s of cold unified compile in one run. The first run can then fail the 120 s SMOKE_BUDGET on compile alone, a false nightly FAIL that tells the reader to move tests to heavy-tests.txt when the cause is the build. Fix: build once (`nextest run --no-run`, untimed) before the two timed runs. Also `ctx.content` is cloned into task_ctx, so a content-touching nightly diff adds fl_catalog/golden to the "smoke alone" run; set `content: false` for the first run (and decide for the second).

#### Minor
5. gate.rs `task_time`: `Outcome::Skip`/`Warn` from `headless_run` (nothing matched) is treated as a pass with ~0 s times, so a broken selection passes the budget row. Fail on Skip.
6. No test pairs `content = true` with a non-None heavy (the existing content test at gate.rs ~1081-1100 only passes `None`), and nothing asserts that no heavy line names fl_catalog or the golden binary. A future heavy entry for either would silently break I3. Add a test that the committed list does not intersect CONTENT_TESTS.
7. heavy-tests.txt has no stale-entry check: a renamed heavy test silently moves back to the task gate (harmless but unnoticed), and a typo excludes nothing. A nightly `nextest list` cross-check would catch it.
8. `scale_*` pulls in two 1 s fl_cli map_gen tests (heavy-tests.txt:47-48); harmless, as the report notes.
9. accepted.rs tests: nothing covers the same file and kind with different text staying reported (the key claim). Add that assertion. `from_name` duplicates the FindingKind variant list by hand (mod.rs ~926-945); a new variant is silently un-allowlistable. Add a test that every Debug name round-trips.
10. No RED run was captured (report line 24). Step 6 did not reach PASS (reasons are external). The two new gate.rs test-guard findings (`None` arg added to asserts) are false positives the lead must accept.

### Assessment

**Spec verdict:** compliant for code; allowlist seeding deviates from "start empty" and its provenance is partly wrong.
**Task quality:** Needs fixes (Important 1-4).

**Reasoning:** The gate trim is sound and conservative: heavy tests are excluded only at task level, still run at pr, nightly and milestone, and I3 is untouched. The allowlist mechanism works but is weaker than its purpose (no multiplicity, fragment keys), and its seed carries unverified or incorrect rulings. The nightly task-time row will probably mis-measure because of compile time.

**Bless:**
- `xtask/heavy-tests.txt`: yes (24 entries valid, no content-rule overlap; add the Minor 6 guard test).
- `xtask/test-guard-accepted.toml`: no, until duplicates are collapsed, fragment keys are tightened, and the gate.rs rulings are corrected or removed.
