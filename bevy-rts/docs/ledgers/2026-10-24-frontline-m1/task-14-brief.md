### Task 14: Determinism suite and golden replays for movement

**Track:** sim-core · **Refs:** ADR-0005, ADR-0006, spec §12.3 · **Lane 1** (after Tasks 12 and 13)

**Files:**
- Create: `crates/fl_test_utils/tests/movement_determinism.rs`
- Modify: `crates/fl_test_utils/tests/golden.rs`
- Blessed: `crates/fl_test_utils/tests/replays/testbed_paths.replay` (new) and `testbed_wander.replay` (if moved)

**Interfaces:**
- Consumes: everything from Tasks 5–13; `assert_deterministic`, `assert_shuffle_invariant`, `assert_save_round_trip`, `assert_branch`, `verify_replay`.
- Produces:
  - `record_testbed_paths() -> Replay`. The base testbed for 900 ticks, cheats on:
    - tick 5: player 0 moves #1–#4 to (720, 760). They cross the river; four units is below the group threshold, so these are single requests.
    - tick 5: player 1 moves #6–#13 (#11–#13 are T-34s spawned by `debug.spawn` at tick 1) to (300, 250). Its six Tracked units form a flow group; the truck and the rifleman get single requests.
    - tick 120: `debug.set_terrain` turns bridge x 70..=72 into water.
    - tick 200: player 0 Shift-queues rotate toward (720, 900), then a move to (650, 800).
    - tick 300: #5 follows #1.

    Hashes every tick.
  - Golden file `tests/replays/testbed_paths.replay`.

- [ ] **Step 1: Write the failing tests:**
```rust
// crates/fl_test_utils/tests/movement_determinism.rs
#[test] fn smoke_m1_testbed_paths_twin()            // assert_deterministic(testbed paths builder, 300)
#[test] fn m1_scenarios_shuffle_invariant()         // assert_shuffle_invariant on: testbed paths (600), an ASCII flow scenario (400),
                                                    //   ScaleScenario::on_terrain(500) (200)
#[test] fn m1_save_round_trip_mid_pathing()         // assert_save_round_trip at ticks 6 (requests queued), 130 (after set_terrain), 320 (follow active)
#[test] fn m1_branch_mid_pathing()                  // assert_branch(testbed paths, 6, 400) and (130, 500)
// crates/fl_test_utils/tests/golden.rs
#[test] fn testbed_paths_replay_verifies()          // fresh record verifies; ticks 900; inputs at ticks [1, 5, 120, 200, 300]
#[test] fn testbed_paths_semantics()                // C8-style semantic checks: the units player 0 sent cross on a bridge cell; no unit is ever on a water cell;
                                                    //   #1–#4 end within 3 m of their formation targets
// golden_replays_verify (existing) now also requires testbed_paths.replay
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_test_utils`. Expected: FAIL (the golden file is missing).
- [ ] **Step 3: Record.** `FL_RECORD_GOLDEN=1 cargo nextest run --profile agent -p fl_test_utils --test golden` writes `testbed_paths.replay.new` (and `testbed_wander.replay.new`). The lead blesses: `M1 Task 14: movement golden replay`.
- [ ] **Step 4: Run** `FL_SHUFFLE_SEEDS=3 cargo nextest run --profile agent -p fl_sim -p fl_test_utils` and `cargo run -p fl_cli -- replay-verify crates/fl_test_utils/tests/replays/testbed_paths.replay --json`. Expected: PASS, and `{"status":"verified", …}`. The replay's `build_id` names the commit it was recorded at, and `fl_sim`'s `build.rs` names the current one. Replay verification only warns on a `build_id` mismatch (ADR-0007 refinement R70), so a later commit still verifies.
- [ ] **Step 5: Gate** `cargo xtask gate pr`. Expected: `gate pr: PASS`. The controller fast-forwards `main`.
- [ ] **Step 6: Commit** `test(fl_test_utils): M1 determinism suite and the testbed_paths golden replay` with trailer `Blessed: …`.

---

## Phase IV — integration (lane 1)

