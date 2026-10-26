# Task 14 review: M1 determinism suite and the testbed_paths golden replay

Diff: `review-t14.diff` (b26d84a on gate base 9799309). Read in two passes (golden.rs + suite top half, then suite bottom half + support file). Read-only; no builds. Commands run: `target/debug/fl_cli.exe` (built 06:05, the commit) `replay-verify`, `replay dump --json`, `save` and `inspect` on a scratch copy of `testbed_paths.replay.new` and on a tick-900 save written to the scratchpad; Python decode of `assets/base/maps/testbed_1km.{terrain,height}.png`.

### Spec Compliance

- ✅ Spec compliant, with one deviation, which I accept (see ruling 1). Every test in the brief's Step 1 is present with the brief's arguments:
  - `smoke_m1_testbed_paths_twin` (movement_determinism.rs:~474, 300 ticks);
  - `m1_scenarios_shuffle_invariant` (testbed paths 600, ascii_flow 400, `ScaleScenario::on_terrain(500)` 200);
  - `m1_save_round_trip_mid_pathing` (ticks 6, 130, 320);
  - `m1_branch_mid_pathing` ((6, 400) and (130, 500));
  - `testbed_paths_replay_verifies` (golden.rs:~122: 900 ticks, 900 hashes, inputs [1, 5, 120, 200, 300]);
  - `testbed_paths_semantics`;
  - `golden_replays_verify`, which now requires both goldens (golden.rs `GOLDEN`).
- The scenario values match the brief (support/testbed_paths.rs:52-113). I checked them against the recorded `.new` with `fl_cli replay dump --json`:
  - tick 1: three `debug.spawn` commands from player 1;
  - tick 5: `core.move` from p0 for [1, 2, 3, 4] and `core.move` from p1 for [6..13];
  - tick 120: `debug.set_terrain` on (70, 122)–(72, 127), set to `base.water`;
  - tick 200: `core.rotate` and `core.move` from p0, both with `queue: true`;
  - tick 300: `core.follow` from p0, unit [5], leader 1.
  - That is 5 inputs carrying 9 commands, hash interval 1, 900 hashes. Bridge A really is cells x 70..=72 on rows 122..=127 (PNG decode), so the drowning removes the whole bridge.
- Deviation: `testbed_paths_semantics` runs past tick 900 (until #1–#4 are idle, at most 2400 ticks). The golden replay itself stays at 900 ticks, as the brief says.
- ⚠️ Cannot verify from the diff alone:
  - the bless commit (Step 3) and its `Blessed:` trailer (Step 6);
  - `gate pr` (Step 5).
  - The controller should confirm `golden_replays_verify` and `test-headless` go green after the bless.

### Strengths

- One shared builder (`support/testbed_paths.rs`) means the golden replay and the suite run the exact same match. Its constants (`PARTY`, `FLOW_GROUP`, `BRIDGE_A`, `RIVER_ROWS`) name the scenario facts the assertions depend on.
- Every split point asserts its own precondition, so no test passes vacuously if the scenario drifts:
  - the queue holds exactly #9 and #10, issued at tick 5 (`m1_save_round_trip_mid_pathing`);
  - the bridge A cells are water and the six tanks are on the flow field (tick 130);
  - #5 follows #1 (tick 320);
  - more than 100 requests are queued, some issued at tick 0 (`m1_branch_mid_path_queue`);
  - the A→B→A follow cycle and #5→#1 are both live at tick 330 (`m1_branch_mid_follow_cycle`);
  - the M0 save has requests queued at tick 41.
- `continue_from_save` covers what `assert_branch` cannot: reloading with different content while the builder's later commands stay scheduled. That those commands survive is proven, not assumed: splits at 150 and 151 still have to match the unbroken run through the tick-200 and tick-300 commands.
- The value-reload test has two non-vacuity checks: the hashes are equal before the reload and differ at 450. The report also describes a temporary mutation (old content → `diverged at tick 150`).
- `m1_shuffle_moves_no_game_system` turns a real limitation into an executable fact, and the module docs explain it accurately. I checked the Bevy behaviour at `bevy_ecs-0.20.0/src/schedule/schedule.rs:1305-1320`: it shuffles node insertion order and keeps every edge.
- The random-orders proptest uses a fixed ChaCha seed with persistence off. That avoids the 7b lesson of a random seed flaking a gate row. The milestone variant is a superset of the gate's cases (same stream).
- The report's account of the golden replay holds up: `fl_cli replay-verify` on the `.new` → `verified`, 900 ticks, with only the R70 `build_id` warning.

### Issues

#### Critical (Must Fix)
None.

#### Important (Should Fix)
None.

#### Minor (Nice to Have)

1. **golden.rs `SEMANTIC_MAX_TICKS` doc, plus report §golden and Concern 2: the stated finish time is wrong.**
   - Both say #1 finishes "near tick 2000". It does not. I continued a tick-900 save with `fl_cli inspect`: #1 is `Following` at tick 1640 (612.05, 803.89) and `Idle` at 1650 on its slot (610.32, 805.01).
   - #4 is idle by about 1100, #3 by 1287 and #2 by about 1308 (`planned` ticks).
   - Fix: change the doc to "≈1650". The cap of 2400 can stay, or drop to 2000.
2. **movement_determinism.rs `m1_branch_across_value_reload`: no split before the reload.**
   - The splits are 150, 151 and 320, all at or after `RELOAD_TICK`. That proves a save taken after a reload, loaded with the reloaded content, matches the unbroken run.
   - It does not prove the case of saving before the reload, loading, and then applying `apply_content_values` to the loaded world. In that case derived caches are cold, as after a load, instead of warm.
   - Fix: add a split at, say, 140. Use the old content for the load, then reload at 150 on the loaded sim and compare to `unbroken` from 140 on. This is cheap with the existing helpers.
3. **movement_determinism.rs `random_orders_branch`: ignores `PROPTEST_CASES`.**
   - The nightly level excludes `milestone_` tests (xtask/src/gate.rs:362-366) and sets `PROPTEST_CASES=4096` (gate.rs:372-374). So nightly runs only the 12 gate cases of this proptest and never the 256-case variant.
   - Fix: take the count from `PROPTEST_CASES` with a default, as hpa.rs:929-939 `cases()` does: `random_orders_branch(cases(12))`. Keep the fixed seed. See ruling 4.
4. **Report, Tests run 4: warnings in the clippy output.** "only the two pre-existing `fl_sim/clippy.toml` rand-path warnings" means the output is not clean. These warnings are pre-existing and not from this task. Park them for the gate-time and noise cleanup.
5. **movement_determinism.rs `m1_shuffle_moves_no_game_system`: a brittle tripwire.**
   - It compares the whole order for equality, so the first legitimately unordered system fails it. The test's own doc says that is intended.
   - Acceptable as is. When it fires, the fix is to update the module docs and replace the equality with "the shuffle moves exactly the documented unordered systems".
6. **The M0 save fixture test is meaningful, but its migration check is vacuous for this fixture.**
   - m0_save.rs:83-101 checks MoveOrder v1→v2 migration, but the fixture's v1 orders are all empty (its own comment says so). Waypoint migration is covered in `pathing.rs`.
   - The new `m1_m0_save_branches_mid_pathing` adds the missing M1 coverage of the M0 save: queued requests at 41, mid-route at 200, and at least 8 of 10 units moved.
   - No change needed. Recorded so the next milestone's fixture (R83) is cut from a state with non-empty orders.

### Checks for the controller's named risks

- **Branch and save mid-flow-group.** Covered at:
  - tick 130 (`m1_save_round_trip_mid_pathing` / `m1_branch_mid_pathing`);
  - tick 600, re-routed along the bank (`m1_branch_mid_flow_reroute`);
  - ASCII ticks 20 and 31, right after the bridge drowns (`m1_ascii_flow_branch`).
- **Mid-follow (A→B→A).** `m1_branch_mid_follow_cycle` asserts #9→#10, #10→#9 and #5→#1 at tick 330, then branches 330 → 600.
- **Mid budget-delayed request queue.**
  - Testbed tick 6: the queue is exactly [(#9, t5), (#10, t5)].
  - Scale scenario on terrain, tick 2: more than 100 requests waiting, some issued at tick 0.
  - M0 save, tick 41: the queue is not empty.
- **Across a value reload.** Covered on the save side, with the ADR-0012 rationale that replays carrying a reload are NonReproducible. The pre-reload split is missing (Minor 2).
- **M0 save fixture test still meaningful.** Yes (Minor 6).
- **The golden records the inputs it claims.** Yes (see Spec Compliance; checked with `fl_cli replay dump --json`).
- **Release journal dependence.** movement_determinism.rs, golden.rs and support/testbed_paths.rs never read the journal (grep finds no `journal`). This matches the report's run: 47/47 release determinism tests passed.

### Rulings on the implementer's concerns

**(1) #1–#4 cannot reach their slots in 900 ticks: a brief-value problem, not a movement defect.**

- The units move at exactly their designed terrain speed:
  - Measured on #1, the pz2, which is Tracked at 40 km/h = 0.5556 m/tick (`core.mobility.speed`).
  - On grass the factor is `0.8 + 0.2 × 0.6 = 0.92`, so the expected speed is 0.511 m/tick.
  - Observed: 153.3 m in ticks 300→600 is 0.511 m/tick; 104.6 m in ticks 1000→1200 is 0.52 m/tick.
  - No crawling at `min_speed_frac`, no oscillation, no re-plan loop. `path_follow.planned` is 5, then 120 (the drowning), and holds at 120 through tick 1400. Then it is 1473, the queued move.
- The only stall is ticks 121→~135:
  - this is turn-in-place: the heading error is about 100°, above `turn_in_place_deg` 30, and the unit turns at 5°/tick, so it waits about 20 ticks;
  - the re-plan happened in the same tick as the drowning (`planned: 120`).
- Even the straight-line distance rules out 900 ticks. Start (350, 350) to the first slot (680, 760) is 552 m, which takes 994 ticks at full road speed.
- #1's actual route is about 850 m, all on passable grass and road:
  - 61 m toward bridge A before the drowning;
  - about 410 m to bridge B at (722, 486);
  - 24 m across the bridge;
  - 202 m to the slot (680, 760);
  - 83 m to the queued slot (610.3, 805.0).
  - At ≈0.51 m/tick plus the turns, that is about 1650 ticks, matching the observed idle at about 1645.
- Slot spacing (R110) is benign here:
  - the first-move slots are target ± 40 m (`formation_max_spread_m`, line spread 90 m);
  - the queued slots are (610, 805), (649, 796), (657, 795) and (683, 804);
  - all are on grass or field, away from the ridge (cells x ≈140-147, y ≈136-169), the deep forest and the swamp (PNG decode).
- Ruling: accept the deviation, the semantics test running on until idle. Amend the plan text for Task 14 to read "until #1–#4 are idle (≈1650 ticks)". No defect and no owning task.
- Do not lengthen the golden replay. At 900 ticks it already covers:
  - the flow group;
  - the drowned-bridge re-plans of #1's single path and of the flow field;
  - #4's queued rotate and move (it reached its first slot at 832);
  - the follow.

**(2) Shuffle seeds move none of the game systems.**

- The shuffle runs stay. They are cheap, and they start exercising reordering as soon as a feature adds an unordered system, which M2's feature crates will. In M1 they prove only that the setting changes no outcome. The module docs say exactly that, and `m1_shuffle_moves_no_game_system` pins it.
- The ordering guard is three layers:
  1. `SimTick` ambiguity detection at `LogLevel::Error`, which is binding (schedule.rs:194-205). Its only allowances are `Journal`, which `step_tick` sorts, and `SimCounters`, whose adds commute (builder.rs:118-122).
  2. The insta system-order snapshot (spec §12.3), which makes every order change a reviewed diff.
  3. The golden replays.
- `smoke_shuffle_invariant`'s unordered probes keep proving that the mechanism works.
- Record in the ledger: this closes the parked shuffle note. When the pin test first fails, the fix is docs plus a pin that lists the systems the shuffle moves, not ordering those systems.

**(3) 40 pre-existing tests fail in release because they read the journal.**

- Acceptable; not this task's concern.
- Spec §11 says the journal is disabled in release builds, and no gate row (task, pr, milestone, nightly) builds release. They all run `nextest --profile agent` on the dev profile (xtask/src/gate.rs:378-388).
- Binding rule: determinism tests (twin, shuffle, save round trip, branch, golden) must never read the journal, so they hold in any build profile. This suite complies.
- Park as a C-list item for the M2 prelude: journal-reading tests set `RunConfig.journal = Some(true)` (fl_sim/src/build.rs:45,181), for example as a default in fl_test_utils' builder at builder.rs:306/560, so a full `--release` run is clean.
  - First check that no alloc or perf-budget test depends on the journal being off.
- Optionally, add a milestone-level release row with the implementer's determinism filter: debug and release must agree on hashes (R104).

**(4) Proptest seed and case-count policy.** One policy for all crates:

- **Seed.** Gate rows (task, pr, milestone) draw from the fixed seed, `TestRng::deterministic_rng(ChaCha)`, with `failure_persistence: None`. Any failure found gets a committed regressions file or a pinned test.
- **Case count.** The count is `PROPTEST_CASES` when set, otherwise a per-test default. Use one shared helper with hpa.rs `cases()` semantics.
- **Random seeds.** Random-seed exploration happens only at nightly level, behind an explicit env opt-in (for example a nightly `PROPTEST_RNG_SEED`, already listed in test_guard `SEED_VARS`).
- **Today the crates are inconsistent:**
  - fixed seed: hpa.rs `hpa_cost_ratio_tripwire` and the new suite;
  - random seed, honouring `PROPTEST_CASES`: hpa.rs's `proptest!` block (`cases(64)`);
  - random seed, ignoring `PROPTEST_CASES` (`with_cases` constants): astar.rs:546, flow.rs:1225, terrain.rs:707, steering.rs:876 and fl_content/tests/terrain.rs:578.
- For Task 14: keep the fixed seed, and make `random_orders_branch` honour `PROPTEST_CASES` (Minor 3).
- Migrating the other crates and moving the helper into fl_test_utils (or a tiny shared test module) is the M2-prelude C-list item the ledger already anticipated after 7b.

**(5) Out-of-list file `crates/fl_test_utils/tests/support/testbed_paths.rs`: accept.**

- It is the only way for two test binaries to share one builder without duplicating it or growing fl_test_utils' public API, which would need an API snapshot bless.
- There is precedent: `fl_protocol/tests/support/format_guard.rs`, plus R108, R109 and R119.
- `#[allow(dead_code, reason = …)]` is the right choice over `#[expect]`, because each binary uses a different subset of the items.

### Bless recommendation

**Bless `crates/fl_test_utils/tests/replays/testbed_paths.replay.new` → `testbed_paths.replay`: yes.**

- It verifies on the current build (only the R70 build_id warning).
- Its header is format 3, `Scenario("base.testbed")`, cheats on, 900 ticks, hash interval 1, 900 hashes.
- Its inputs are exactly the brief's.
- `testbed_wander.replay` correctly stays unmoved: per the report its `.new` was header-only, differing only in `build_id`.

### Assessment

**Task quality:** Approved

**Reasoning:** The suite covers every split point the plan and the controller named, and every split asserts its own precondition. The golden records exactly the brief's inputs and verifies. The one deviation comes from the brief's 900-tick figure being too short for the distances and speeds, not from a movement defect: the units move at their exact terrain speed and #1 is idle at about tick 1650. The remaining items are Minor: a doc figure, a pre-reload split, and honouring `PROPTEST_CASES`.
