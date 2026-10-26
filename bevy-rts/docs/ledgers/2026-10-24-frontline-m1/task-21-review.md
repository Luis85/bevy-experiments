### Spec Compliance

- ✅ Spec compliant, with disclosed, accepted-in-kind gaps: `gate milestone` cannot PASS until the lead blesses `docs/perf/M1.json.new` (perf-file row only), and no fixed power plan was set (see Minor 4).
- ⚠️ Cannot verify from diff: the live walkthrough results (screenshots excluded), the m0-tag interleaved timings (raw logs in `target\t21\`), and the gate rows (report only). The whole-branch review is a separate seat.

### Strengths
- The checklist separates PASS (live), PASS (test), HUMAN and NOT DONE. Counts match the report: 35 rows = 31 live + 4 test (rows 10, 24, 28, 33); 4 human items. Qualified results are said so in the row: row 1 hill, row 6 at 4/5, row 35 startup only, row 21 issued via `fl.issue_command` and not the console.
- The five parked visual issues (T17: camera focus y=0, half-cell border, triangle vs bilinear, flat rings; plus the faint hill) are in "Visual notes". The T18 gizmo carry is rows 25-27, and the T20 carry (testbed_ai is startup-only) is row 35. I grep-checked a sample of the test names the checklist cites (including `click_empty_ground_clears_unless_shift`, `console_terrain_issues_debug_set_terrain`, `brp_debug_query_round_trip` and `m0_testbed_save_loads_under_m1`) and all exist.
- The perf numbers are consistent across `M1.md`, `M1.json.new` and the report. Terrain 0.521/4.613/9.897 ms and the interleaved M0 vs flat table match in all three. The flat deltas of +55/+58/+63 % recompute correctly. The 20 % miss is stated plainly, and the document says that the loaded `M0.json` would have passed `--baseline`. The justification names `compute_steering` +2.56 ms and `move_system` +1.04 ms. In `M1.json.new`, `rows[1]` is the terrain 3000 row (4.613) and the flat run is not in `rows`. `meta` has `head`, rustc, machine and power plan.
- `debugging.md` matches the code on the items I checked. The CLI has `query`, `map-gen`, `--flat` and `--force`. Remote methods `fl.debug_query`, `fl.select` and `fl.selection` exist. The keys exist (Home camera.rs:289; H/R/F/P/=/-/F4 orders.rs:150-156; F3 map_view.rs:42; F5/F6/F7 overlays.rs:601-603). The console has `terrain`, `reload` and `replay`. `hot_reload` and `hot_reload_poll_ms` exist (fl_observe config.rs:24-38, repo `debug.ron`). `MAX_WAYPOINTS`=32 and `MAX_COMMAND_UNITS`=256 are defined (commands.rs:102-106). The reject codes are mapped (commands.rs:158-160, 86). The counter and span names match. R87, R88, R133 and R134 are described as ruled in the ledger.
- The spec "Revision 2.4" only references existing ADRs (0007, 0009, 0010, 0012, 0014-0016) and R-numbers; all 43 cited R numbers occur in `progress.md`, and the ones I spot-read (R81, R83, R89, R90, R92, R93, R96, R105, R113, R114, R130, R136, R138, R140, R142) match the ledger wording.
- The codec fix is minimal, and its RED/GREEN evidence is in the report.

### Issues

#### Critical (Must Fix)
None.

#### Important (Should Fix)
1. `docs/debugging.md` (file lines ~534 and ~619, the `fl.entity` and `fl.journal_since` examples) still shows M0 output: `"core.move_order": {"waypoints": []}` and a journal record `"kind": "core.move_order"`, `"data": {"waypoints": 1}`. M1 replaced these with `orders` and `core.order_changed`, and the document says so itself in §5 and §8. The header says every response was run and pasted, and the brief says to run every documented command and check the output. Re-run both and paste the M1 output.

#### Minor (Nice to Have)
1. `docs/perf/M1.md` per-system section: "per-unit costs scale linearly (+0.17 / +1.64 / +3.73 ms)". That is about +0.34, +0.55 and +0.75 ms per 1,000 units, so superlinear, which matches `steer.pairs` growing from 1.4k to 21k to 53k. Say "superlinear, driven by steer.pairs". The per-system rows sum to +3.83 against the stated +3.872 (rounding or median effects); a one-line note would do.
2. `docs/perf/M1.md` render stress: "the terrain mesh costs nothing measurable" rests on one interleaved pair run under 30-55 % background CPU (59.9/61.2 against 58.5/60.3, so around the 60 fps limit). The first four stress runs say "CPU load not sampled". The text does say a fully idle pair is still owed, so this is only wording; "not measurable under load" would be accurate.
3. Determinism step: the brief's command (`FL_SHUFFLE_SEEDS=3 cargo nextest run --profile agent -p fl_sim -p fl_test_utils`, expected PASS) was not run as written. A filtered `--release` subset was run instead, with 54 of 55 passing; the one failure reads the journal, which release builds do not record. The checklist says exactly this, but the row is headed "PASS for determinism". Run the brief's command once in debug, or relabel the row "PASS (filtered, release)".
4. R128 and the brief ask for the user to confirm a fixed power plan. The report says changing it is "not permitted", but nothing shows the user was asked. The run is Balanced and disclosed. The controller should either ask or accept it.
5. The codec test pins the list of registered names only. It does not decode a `core.rotate`, `core.follow` or `debug.set_terrain` JSON payload. One round-trip assertion each would pin the payload field names (`toward`, `leader`, `min_cell`/`max_cell`/`terrain`) that the docs now advertise.
6. The README "Headless tools" block does not list `query` or `map-gen`; they are only in `debugging.md`.

### Missing carries
- None are missing from the ledger list. T4 map-gen regeneration command: done (§3). T17 parked visuals: done (checklist "Visual notes"). T18 `fl.debug_query` and F5-F7 docs plus gizmo demo rows: done. T20 testbed_ai startup-only note: done (row 35). R128 (T13): interleaved M0 against M1 --flat, median of 5 and the per-system note are done. The fixed power plan is not done and is listed as not done (Minor 4).
- Pending outside this task: the lead's bless of `docs/perf/M1.json.new` and a rerun of the `perf-file` row; the whole-branch review (brief step 9) is dispatched separately.

### Assessment

**Task quality:** Approved (the Important item is a doc refresh).

**Reasoning:** The checklist and perf docs are honest about what was verified, the 20 % overrun and the unmet power-plan step. The code and CLI claims I grep-checked exist. The one defect is two stale M0 examples in `debugging.md` that contradict its own M1 sections.
