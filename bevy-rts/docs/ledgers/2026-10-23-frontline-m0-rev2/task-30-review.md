# Task 30 review (3500b33..56e3425, ignoring 4fb22bd)

### Spec Compliance

- ✅ Spec compliant, with documented deviations. Every listed file has a hunk: `docs/debugging.md`, `docs/m0-demo-checklist.md`, `docs/perf/M0.md`, `docs/perf/M0.json`, `README.md`. The extra edits are the builder fix, `probes/README.md` and the spec rev 2.1, all in the report's carry list.
- Deviation, disclosed and acceptable: Step 4 power plan was Balanced, not a user-confirmed fixed plan. It is recorded in both M0 files (`M0.md:5-7`, `M0.json meta.machine.power_plan`). A rerun on a fixed plan is still owed.
- Deviation: Step 5 is met in the median, not in every run. 2 of 7 runs were below 60 fps (`M0.md` "Render stress"). The doc says so and labels the checklist row "idle-machine rerun owed". It does not claim a clean pass.
- Deviation: Step 6 says "through `bevy_brp_mcp`". The checklist header says "driven only over BRP", with the `fl.*` and `brp_extras/*` methods. It does not say whether the MCP wrapper or raw JSON-RPC was used. The methods and results are the same either way.
- Step 9 (whole-branch review) is left to the controller, as the report states.
- ⚠️ Cannot verify from the diff: the gate outputs (`pr`, `milestone`, `task`) and the nextest result are only in the report. The 212/213 failure is the known `unclassified_sim_type_fails_build` in release.

Checks of the focus areas:
- **M0.json against M0.md.** I loaded the JSON. `rows` gives 500/1680/0.5366/1.3141/3.2986, 3000/10000/4.4312/7.9554/12.8503 and 5000/16680/9.3348/16.3291/27.4434. These match the `M0.md` table to 3 decimals. `meta.repeat` is 5. Entity counts are 1.7k, 10k and 17k as expected. The render-stress medians check out (the median of the 7 runs is 98.8, best 104.0, 5 of 7 are at least 60). The `commit` field says "3500b33 (sim code…)", which is honest. The head SHA is not recorded, but the sim code did not change.
- **Demo checklist claims.** Each row is labelled PASS, PASS (test) or NOT RUN. The unrun items (click-to-place, console typing) are flagged NOT RUN. Rows 3, 12, 16, 17 and 18 rest on named unit tests, not live checks, and the labels say so. The ADR table has 44 bullets plus D1/D2. Three entries are honestly marked PARTIAL or deferred (0004 replay remap, 0009 live hook denial, 0012).
- **`debugging.md` commands.** All 17 `fl.*` method names are registered in code (grep: entity, hash, issue_command, journal_since, load, load_scenario, pause, perf, resume, save, save_scenario, screenshot, set_speed, sim_state, spawn, step, toggle_overlay). The `fl_cli` commands data-check, hash-dump, bench, run-scenario, inspect, journal, replay-verify, replay dump, save, load and bisect-desync exist in `fl_cli/src/main.rs`. The `trace`, `trace_chrome`, `trace_tracy` and `dynamic` features exist in `fl_game/Cargo.toml`. F3 is the grid overlay (`map_view.rs:35`). `logs/latest.txt` is written (`fl_observe/src/logging.rs:55`).
- **`debug.ron` load order.** `build()` now reads the config first (`builder.rs:247-255`) and passes `RunConfig` through `start_sim`, `set_up_sim` and `scenario_loader`. Both load paths use it (scenario at `:296`, save at `:307`). The change is correct: `cfg` is `Some` exactly for the windowed shell, and headless tests keep the default. The hash is unaffected by markers (`fl_cli/tests/bench.rs::final_hash(phase_markers)`). No new test pins the wiring (see Important).
- **Spec rev 2.1.** It has 28 lines, each citing an R-number and ADR. They restate ledger rulings (R5, R15, R20, R41…R68) and decide nothing new. Two lines are worth confirming against the ledger text: R20 is a bundle of several rulings, and R65 describes a process fact, not a contract. Both belong in a "refinements" list only loosely. Status line updated.

### Strengths

- Docs are grounded in real output, with real hashes that cross-check. The tick-20 hash `8b3418f4fcb9d9af` matches `hash-dump`. The save/load hash `0x28c67f8b5038110a` equals the uninterrupted run.
- The perf docs state their load caveat, the unmet-in-2-runs result, and the logging finding, without spinning them.
- The builder fix is small and keeps phase markers consistent across `fl.load*` reloads.
- `M0.json` is internally consistent with `M0.md`.

### Issues

#### Critical
None.

#### Important
1. `crates/fl_game/src/builder.rs:247-255` has no test for the new wiring (the report admits this). A regression would silently bring back empty F2 phase rows. Add a headless-shell or `RunConfig` test that asserts the markers are on when `profiler` is true. This needs a seam, because `load_debug_config(None)` reads `debug.ron` from the working directory.
2. `fl_sim/src/classify.rs` `unclassified_sim_type_fails_build` fails in release (known). The checklist still lists it as PASS in ADR-0006 row `m0-demo-checklist.md:1070` and the "same suite in debug builds" row. This is true for debug only. The fix wave should gate the test with `#[cfg(debug_assertions)]`. After the fix, the 212/213 caveat in the checklist (line 1035) should be removed.
3. DEBUG spawn logging at about 30 ms per tick (known). It is documented as a finding but not fixed. With the default `debug.ron` (`fl_sim=debug`), every windowed dev run at stress scale is 4x over the 15 ms budget. This is worse than documented: the live F2 panel will show budget warnings, and the stress number in `M0.md` is measured only under `RUST_LOG`/release conditions that avoid it. Fix by logging spawns at TRACE, or by setting the `debug.ron` default to `info`.
4. `BRP_EXTRAS_PORT` is documented in `CLAUDE.md:109` but ignored by `fl_game` (known). `debugging.md:390-393` correctly says so. The contradiction stays until the contract text is fixed. Fix the contract docs in the same wave, not the doc here.

#### Minor
1. The perf numbers (bench and stress) come from a machine at 96-100 % load on the Balanced plan (known). The bench is mean 4.43 ms against a 15 ms budget, so the margin covers it. The stress claim (98.8 median) is weaker. The checklist rows 23 and 0002 are marked "idle-machine rerun owed", which is honest. Keep that rerun on the M0 close-out list.
2. `builder.rs:556-565` `observe_setup` now creates the run dir before content loads, so a content error leaves an empty run dir (self-reported). Cosmetic.
3. `M0.md` has one over-long line (the "Meshes and materials are already shared…" paragraph, line 2782) that is not wrapped like its neighbours. Cosmetic.
4. The checklist says "driven only over BRP" but does not state whether `bevy_brp_mcp` was used (brief Step 6). Add one phrase.
5. `M0.json meta.commit` records `3500b33` but not the head SHA of the docs commit that holds the numbers. Fine for a sim-only baseline, but state it explicitly.

### Assessment

**Task quality:** Needs fixes (small fix wave, no rework).

**Reasoning:** The spec deliverables are complete and honestly caveated. The docs match the code (method names, commands, features) and `M0.json` matches `M0.md`. The `debug.ron` order fix is correct but untested. The remaining items are the four known issues, which the fix wave should close, plus the missing wiring test.
