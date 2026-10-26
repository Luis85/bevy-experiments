# Task 14 report: M1 determinism suite and golden replays for movement

Lane 1, `C:\fl\a`, branch `m1/phase-a`, gate base `9799309`.

## What I implemented

### `crates/fl_test_utils/tests/support/testbed_paths.rs` (new, out of the Files list — see Concerns)
The brief's movement testbed as a builder, shared by `golden.rs` and `movement_determinism.rs`
through `#[path = "support/testbed_paths.rs"] mod testbed_paths;` (the pattern of
`fl_protocol/tests/support/format_guard.rs`), so the golden and the suite run the very same match.
Values verbatim from the brief:
- tick 1: player 1 `debug.spawn`s three `base.su.t34` (#11–#13) at (380, 680), (410, 680), (440, 680),
  heading 270° (the brief gives no position; grass 30 m north of player 1's line);
- tick 5: player 0 moves #1–#4 to (720, 760); player 1 moves #6–#13 to (300, 250);
- tick 120: `debug.set_terrain` cells (70, 122)..=(72, 127) → `base.water` (bridge A);
- tick 200: player 0 Shift-queues `core.rotate` toward (720, 900), then `core.move` to (650, 800), for #1–#4;
- tick 300: player 0 orders #5 to follow #1.
Cheats on, hashed every tick (the builder default).

Observed behaviour (probe run, not committed): at tick 5 #6, #7, #8, #11, #12, #13 are on one flow
field (FlowKey cluster 52, Tracked size 1), #1–#4 on single paths, #9/#10 wait in the queue
(requests 5 and 6, issued tick 5, served tick 6 — the group request ate the tick-5 budget). After
bridge A drowns the group's field is rebuilt with the same key; the group drives to the north bank
and east along it to bridge B (x 180..=182), #6 is on bridge B at tick 899. #1–#4 cross on bridge B;
#4 is idle by tick ~1100, #3 by ~1290, #2 by ~1310, #1 by ~1650 (review figures; my probe only sampled ticks 1500 and 1999). #1 (pz2, the westmost) first headed
west for bridge A; when it drowned at tick 120 its position stayed unchanged until ~130 (presumably re-planning and turning) and it re-routed
to bridge B, so the golden also covers a single path invalidated by a terrain change. No unit ever stands on water (2000 ticks).

### `crates/fl_test_utils/tests/golden.rs`
- `record_testbed_paths() -> Replay`: `testbed_paths()` for 900 ticks.
- `FL_RECORD_GOLDEN=1` now writes both `testbed_wander.replay.new` and `testbed_paths.replay.new`.
- `golden_replays_verify` requires both `testbed_paths.replay` and `testbed_wander.replay`.
- `testbed_paths_replay_verifies`: start `Scenario("base.testbed")`, 900 ticks, 900 hashes, inputs at
  ticks [1, 5, 120, 200, 300], verifies.
- `testbed_paths_semantics` (C8-style): runs the builder past the replay's end until #1–#4 have no
  orders (at most 2400 ticks): every tick no unit (via `unit_cells`) is on a `base.water` cell; the
  only terrain #1–#4 ever stood on inside the river rows 122..=127 is `base.bridge`, and each ends
  north of the river; each of #1–#4 ends within 3 m of its formation slot (the target of its last
  `Order::Move` read at tick 201, after the queued orders applied).
  **Deviation:** the brief implies the checks hold within the replay's 900 ticks, but with the
  brief's exact values #1–#4 are still south of/on the way to the river at tick 900 (≈700 m of
  route; #1 finishes at about tick 1650). The semantic test therefore runs the same builder on until
  idle; the golden replay stays 900 ticks as specified.

### `crates/fl_test_utils/tests/movement_determinism.rs` (new)
| test | what |
|---|---|
| `smoke_m1_testbed_paths_twin` | `assert_deterministic(testbed_paths, 300)`; non-vacuity: 13 units, #6–#13 owned by player 1, the six Tracked on a flow field and #1–#4 `Following` at tick 6, #5 following #1 and #1–#4 holding `[Move, Rotate, Move]` at tick 301 |
| `m1_scenarios_shuffle_invariant` | `assert_shuffle_invariant` on testbed paths (600), the ASCII flow scenario (400), `ScaleScenario::on_terrain(500)` (200) |
| `m1_shuffle_moves_no_game_system` | pins what the shuffle permutes (below) |
| `m1_save_round_trip_mid_pathing` | `assert_save_round_trip` at tick 6 (asserts the queue is exactly #9 and #10, issued tick 5), 130 (asserts bridge A cells are water and the group is on the flow field), 320 (asserts #5 follows #1) |
| `m1_branch_mid_pathing` | `assert_branch(testbed paths, 6, 400)` and `(130, 500)` |
| `m1_branch_mid_flow_reroute` | branch at 600 → 900, with the group re-routed along the bank toward bridge B (asserted on flow) and #5 following |
| `m1_branch_mid_follow_cycle` | testbed paths + tick 320: #9 follows #10 and #10 follows #9 (A→B→A) on real terrain after the bridge loss; asserts the cycle and #5→#1 at tick 330; branch 330 → 600 |
| `m1_branch_mid_path_queue` | scale on terrain (500 units): at tick 2, >100 requests queued, some issued tick 0 (budget-delayed two ticks); branch 2 → 150 |
| `m1_ascii_flow_branch` | ASCII river map, 8-tank flow group + truck + rifleman, enemy tank, bridge A drowned at 30: asserts the 8 on flow at 20; branch 20 → 250 and 31 → 250 |
| `m1_branch_across_value_reload` | see "Reload" below |
| `m1_m0_save_branches_mid_pathing` | see "M0 save" below |
| `m1_random_orders_branch` | fixed-seed proptest, 12 cases (see "Proptests") |
| `milestone_m1_random_orders_branch_long` | the same seed, 256 cases (`milestone_` → milestone gate only; ~37 s) |

Helper `continue_from_save(&mut Scenario, content)`: saves the scenario, loads the save with the
given content through `load_world`, and swaps the loaded `SimRunner` and a restored `TestDriver`
into the same `Scenario` (both fields are `pub`), so the builder's later commands stay scheduled.
`assert_branch` cannot do this because it reloads with the scenario's original content.
Sanity check (temporary, not committed): loading the post-reload save with the *old* content makes
the helper report `diverged at tick 150`, so the comparison is not vacuous.

**Reload (ADR-0012).** A replay containing a reload is `NonReproducible` by design, so the reload is
tested on the save side only. `faster_t34()` copies `assets/base` into a tempdir, edits
`units/su/t34.unit.ron` `road_speed_kmh: 54.0 → 72.0` (value-only, same ids) and loads it. The
testbed paths match runs to tick 150 (four of the flow group's six are T-34s, mid-route after the
bridge loss), `apply_content_values` reloads it between ticks; saves at 150 (right after the reload),
151 and 320 are loaded with the reloaded content and run on to 450: hashes from the split equal the
unbroken run's and the saves at 450 are byte-identical. Non-vacuity: the unreloaded run's hashes
equal it before 150 and differ at 450.

**M0 save (R78/R81/R83).** `m0_save.rs` stays as it is (load + fills + migration + twin run +
orders reach the queue and move). The suite adds `m1_m0_save_branches_mid_pathing`: load
`m0_testbed.save` (tick 40), order both players' units across the river, branch at 41 (requests
queued, asserted) and 200 (mid-route) → 400 against the unbroken run; non-vacuity: ≥ 8 of the 10 M0
units moved and the state hash changed.

### What the shuffle runs permute (the parked R-earlier limitation)
Bevy 0.20 (`bevy_ecs-0.20.0/src/schedule/schedule.rs:1306`, `feature = "debug"`) shuffles the
*node insertion order* of the flattened dependency graph before the topological sort, keeping every
edge. So it can only move systems no constraint orders. I printed `system_order` of the game's
`SimTick` (25 lines: 17 systems + 8 phase syncs) for `None` and each of the three seeds: **zero lines
differ** — every M1 game system is ordered (hooks chained, core systems chained inside each hook).
`m1_shuffle_moves_no_game_system` pins this (it fails, with a doc pointer, if a change adds an
unordered system). Consequence, stated in the module docs: in M1 the shuffle runs of real scenarios
prove only that the shuffle setting changes no outcome; they do not exercise reordering. What keeps
unordered systems safe is the `SimTick` ambiguity detection at `LogLevel::Error`. The mechanism
itself remains covered by `determinism.rs::smoke_shuffle_invariant` (eight unordered probe roots,
which do move).

### Proptests (fixed seed)
`m1_random_orders_branch` builds random cases on the ASCII river map (4 tanks, a truck, a rifleman):
1–9 orders at ticks 0..120 — move / rotate (with or without Shift), follow (incl. self-follow
rejections and cycles), stop, drown bridge A or B — and a random branch tick 1..200, checked with
`assert_branch(.., split, 200)`. It uses `TestRunner::new_with_rng(Config { cases,
failure_persistence: None, .. }, TestRng::deterministic_rng(RngAlgorithm::ChaCha))` (the pattern of
`hpa.rs::hpa_cost_ratio_tripwire`) and does not read `PROPTEST_CASES`, so every gate run draws the
same 12 cases; the milestone variant draws the first 256 of the same stream. No regressions file is
written (persistence off). The suite's other tests are not random.

## Golden replays
- `crates/fl_test_utils/tests/replays/testbed_paths.replay.new` (new, 12,458 bytes): written by
  `FL_RECORD_GOLDEN=1 cargo nextest run -p fl_test_utils --test golden`. Format 3, start
  `Scenario("base.testbed")`, 900 ticks, a hash every tick, inputs at [1, 5, 120, 200, 300].
  `cargo run -q -p fl_cli -- replay-verify crates/fl_test_utils/tests/replays/testbed_paths.replay.new --json`
  → `{"status": "verified", "ticks": 900, "warnings": []}`. For the lead to bless:
  `cargo xtask bless crates/fl_test_utils/tests/replays/testbed_paths.replay.new --reason "M1 Task 14: movement golden replay"`.
- `testbed_wander.replay`: **not moved.** The re-recorded `.new` differed from the committed file in
  6 bytes only, all in the header's `build_id` string (`0.0.0+87407dc` vs `0.0.0+9799309`); hashes
  and inputs identical. I deleted that `.new`; nothing to bless.

## Tests run
All on commit `b26d84a` unless noted.

1. `cargo nextest run -p fl_sim -p fl_test_utils -p fl_catalog -p fl_cli --no-fail-fast` (test
   profile, debug assertions on; before the commit, same tree):
   `Summary [66.188s] 501 tests run: 500 passed, 1 failed` — the one failure is
   `fl_test_utils::golden golden_replays_verify`: "the blessed golden replay …/testbed_paths.replay is
   missing" (expected until the lead blesses the `.new`; Step 2's expected FAIL).
2. `FL_SHUFFLE_SEEDS=3 cargo nextest run --release --profile agent -p fl_sim -p fl_test_utils
   --no-fail-fast`: `450 tests run: 409 passed, 41 failed`. The 41: `golden_replays_verify` (bless
   pending) plus 40 **pre-existing** fl_sim/fl_test_utils tests that read the journal (pathing 11,
   flow 11, orders 9, reload 5, scale_mix 1, steering 1, terrain 1, m0_save
   `m0_testbed_save_loads_under_m1` 1: e.g. "one result for request 0", "a path request within 50
   ticks"). The journal is on only with `debug_assertions` (`build.rs:181`), so these cannot pass in
   release; no file of this task touches them, and no gate row runs release. See Concerns.
   The determinism tests in release with three shuffle seeds, isolated:
   `FL_SHUFFLE_SEEDS=3 cargo nextest run --release -p fl_sim -p fl_test_utils -E 'binary(movement_determinism) | binary(determinism) | test(/shuffle|twin|branch|round_trip|deterministic/)'`
   → `Summary [29.812s] 47 tests run: 47 passed, 403 skipped` (all 13 suite tests including the
   milestone proptest, determinism.rs, and every fl_sim twin/shuffle/branch test).
3. `cargo run -q -p fl_cli -- replay-verify crates/fl_test_utils/tests/replays/testbed_paths.replay.new --json`
   → `{"status": "verified", "ticks": 900, "warnings": []}`.
4. `cargo clippy -p fl_test_utils --all-targets -- -D warnings`: clean (only the two pre-existing
   `fl_sim/clippy.toml` rand-path warnings).
5. `cargo xtask gate task --track sim-core` (after the commit):
   ```
   PASS fmt (3.5s)
   PASS clippy-drift (1.6s)
   PASS claude-md (0.0s)
   PASS check-scope (0.2s)
   PASS protected (0.2s)
   PASS clippy (25.6s)
   PASS arch-check (3.9s)
   FAIL test-headless (78.8s)
   SKIP test-client: no client package touched since 9799309 (.gate-base)
   SKIP api: no API crate touched since 9799309 (.gate-base)
   PASS data-check (7.9s)
   gate task: FAIL (test-headless) in 122s
   ```
   test-headless: `51 tests run: 50 passed, 1 failed` — the failure is `golden_replays_verify`
   (missing blessed `testbed_paths.replay`). After the bless it should pass; `gate pr` (Step 5) is
   the controller's, after the bless.

TDD: RED was the missing golden (`golden_replays_verify` fails as above); the suite's other tests
are tests of existing behaviour and passed on first green run, with non-vacuity checks at every
split tick and one temporary mutation (old content after a reload → `diverged at tick 150`).

## Files changed
- `crates/fl_test_utils/tests/movement_determinism.rs` (new)
- `crates/fl_test_utils/tests/golden.rs`
- `crates/fl_test_utils/tests/support/testbed_paths.rs` (new; out of the Files list)
- `crates/fl_test_utils/tests/replays/testbed_paths.replay.new` (new, bless pending)

## Self-review
- Every save/branch test asserts its precondition at the split tick (queue contents, flow state,
  follow orders, drowned cells), so none is vacuous if the scenario drifts.
- No `Local<T>`, no sim code changed; tests only.
- Test runtime (test profile): the suite's gate tests take 1–6 s each; the milestone proptest ~37 s.

## Concerns
1. Out-of-list file `tests/support/testbed_paths.rs`: needed so `golden.rs` and the suite share one
   builder instead of duplicating it. check-scope may list it (same track, same crate's tests).
2. Semantic-check deviation: #1–#4 cannot reach their slots within 900 ticks with the brief's values;
   `testbed_paths_semantics` runs on until idle (≤ 2400 ticks). The golden stays 900 ticks.
3. `golden_replays_verify` fails until the lead blesses `testbed_paths.replay` (expected, Step 2).
4. 40 journal-reading fl_sim/fl_test_utils tests fail in release builds (journal off without
   `debug_assertions`); pre-existing, outside this task. If release runs of the full suite are meant
   to be clean, those tests need `RunConfig.journal = Some(true)` or a `cfg(debug_assertions)` gate.
5. The shuffle runs in M1 permute no game system (stated above and pinned); real reorder coverage is
   ambiguity detection's job. Worth recording in the ledger as the resolution of the parked note.

## Commits
- `b26d84a` test(fl_test_utils): M1 determinism suite and the testbed_paths golden replay
  (no `Blessed:` trailer: the lead blesses `testbed_paths.replay.new` and adds it).

## Fix round 1

Commit `b1f3b3d` test(fl_test_utils): Task 14 fix round 1: pre-reload split, PROPTEST_CASES, finish tick.

What changed:
- **Minor 1 / R137.** The `SEMANTIC_MAX_TICKS` doc in `golden.rs` now says #1 finishes at about tick 1650, and this report's golden section and Concern 2 say the same. They give the review's per-unit figures: #4 ~1100, #3 ~1290, #2 ~1310, #1 ~1650. My earlier "~2000" came from a probe that only sampled ticks 1500 and 1999. The cap stays at 2400. The plan's Task 14 `testbed_paths_semantics` line (docs/superpowers/plans/2026-10-24-frontline-m1.md) now says the test runs past the replay's 900 ticks until #1–#4 are idle (≈1650 ticks, R137), and that the golden stays at 900 ticks.
- **Minor 2.** `m1_branch_across_value_reload` has a new pre-reload split. It saves at tick 140, loads with the old base content (`continue_from_save(.., base_content())`), runs to 150, and calls `apply_content_values` with the reloaded content on the loaded sim, whose derived caches are cold. It then runs to 450 and compares with the unbroken run: hashes from 140 on, and byte-identical saves at 450. The doc comment lists the new case.
- **Minor 3 / R140.** The suite has a `cases(default)` helper with the semantics of `hpa.rs`'s `cases()`: it uses `PROPTEST_CASES` when that is set, otherwise the default. `m1_random_orders_branch` uses `cases(12)` and `milestone_m1_random_orders_branch_long` uses `cases(256)`. Both keep the fixed ChaCha seed with failure persistence off, so a larger count runs a superset of the default's cases. The module docs now say this. `random_orders_branch` is the suite's only proptest.

Golden `.new`: **unchanged.** I re-recorded it with `FL_RECORD_GOLDEN=1`. The new file differed from the committed `.new` in 7 bytes (offsets 10–16), all inside the header's `build_id` (a new HEAD). Hashes and inputs were identical, so I restored the committed `.new`. The `testbed_wander.replay.new` that the recorder also wrote is deleted again (only its `build_id` differs).

Tests run:
- `FL_SHUFFLE_SEEDS=3 cargo nextest run --no-fail-fast -p fl_sim -p fl_test_utils -E 'binary(movement_determinism) | binary(determinism) | binary(golden) | binary(m0_save) | test(/shuffle|twin|branch|round_trip|deterministic/)'`
  → `Summary [38.231s] 55 tests run: 54 passed, 1 failed, 403 skipped`. The failure is `golden_replays_verify`, which is waiting for the bless.
- `PROPTEST_CASES=3 cargo nextest run -p fl_test_utils --test movement_determinism m1_random_orders_branch`
  → both proptest tests pass. The milestone one finishes in 0.55 s instead of ~37 s, which shows the env var sets the count.
- `cargo xtask gate task --track sim-core,docs`:
  ```
  PASS fmt (4.2s)
  PASS clippy-drift (1.8s)
  PASS claude-md (0.0s)
  PASS check-scope (0.2s)
  PASS protected (0.2s)
  PASS clippy (28.4s)
  PASS arch-check (3.8s)
  FAIL test-headless (74.4s)
  SKIP test-client: no client package touched since 9799309 (.gate-base)
  SKIP api: no API crate touched since 9799309 (.gate-base)
  PASS data-check (15.0s)
  gate task: FAIL (test-headless) in 129s
  ```
  test-headless: `51 tests run: 50 passed, 1 failed`. The failure is `golden_replays_verify`, because the blessed `testbed_paths.replay` is missing.

New concern (R140 at nightly). The nightly gate sets `PROPTEST_CASES=4096`, so `m1_random_orders_branch` would run 4096 cases there. Each case takes about 0.14 s (256 cases ≈ 37 s), so that is about 9–10 minutes. The nextest `agent` profile kills a test after 180 s (`slow-timeout` 60 s × 3). Unless something changes, this test will time out at nightly. Options, for the controller:
- cap the count in this test;
- give this test a longer timeout in `.config/nextest.toml`;
- have the nightly gate pass a count scaled to the test.
I did not change anything for this.
