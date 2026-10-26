### Task 2: M1 save fixture, frozen `MoveOrder` v2, pose trace (M11)

**Track:** sim-core · **Refs:** ADR-0006 (R83) · **Lane 2** (`C:\fl\m2p02`, `m2p/t02-m1-fixture`) · first, alongside Task 1

**Files:**
- Modify: `crates/fl_sim/src/units.rs` (a frozen `pub(crate) mod v2` and one unit test), `crates/fl_test_utils/src/lib.rs`, `crates/fl_test_utils/tests/golden.rs`
- Create: `crates/fl_test_utils/src/trace.rs`, `crates/fl_test_utils/tests/m1_save.rs`
- Blessed: `crates/fl_test_utils/tests/replays/m1_testbed.save`, `crates/fl_test_utils/tests/replays/testbed_paths.poses`

**Interfaces:**
- Consumes: `fl_cli save`, `Scenario::{load, run_ticks, world}`, `fl_sim::{StateHasher, SimHash, SimIndex, Pose, Health, load_world, SimClock}`, `testbed_paths()` and `TICKS` (900) from `tests/support/testbed_paths.rs`.
- Produces:
```rust
// crates/fl_sim/src/units.rs — "core.move_order" v2 exactly as at tag m1; never edited again (Task 13 migrates from it)
pub(crate) mod v2 {
    #[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
    pub(crate) struct MoveOrder { #[serde(deserialize_with = "super::capped")] pub(crate) orders: Vec<Order> }
    #[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
    pub(crate) enum Order { Move { target: FxVec2 }, Rotate { facing: Angle }, Follow { leader: SimId, offset: FxVec2 } }
}
// crates/fl_test_utils/src/trace.rs
pub fn pose_hash(world: &World) -> u64
    // a StateHasher over every SimIndex entry in order: SimId, Pose.pos, Pose.heading, Health.hp (u32::MAX without Health)
pub fn pose_trace(scenario: &mut Scenario, ticks: u32) -> Vec<(Tick, u64)>   // after each of the next `ticks` ticks: (the tick that ran, pose_hash)
pub fn write_pose_trace(path: &Path, trace: &[(Tick, u64)]) -> std::io::Result<()>  // "<tick> <hash as 016x>\n" per entry
pub fn read_pose_trace(path: &Path) -> std::io::Result<Vec<(Tick, u64)>>
```

- [ ] **Step 0: Generate the M1 save fixture (R83), before any code change.** In a temporary worktree of tag `m1` (`git -C C:\fl\a worktree add C:\fl\m1fix m1`), with `CARGO_BUILD_BUILD_DIR=C:/t/fl/m1fix` (this build is lane 2's builder), run `cargo run -q -p fl_cli -- save crates/fl_test_utils/tests/replays/testbed_paths.replay --tick 310 --out C:/t/fl/m1_testbed.save --json`. Expected JSON: `"tick": 310`, `"entities": 13`, `"pending_commands": 0`. At tick 310 the replay has a Shift-queued rotate and move (tick 200), a follower (#5, tick 300) and player 1's flow group. Copy the file to `crates/fl_test_utils/tests/replays/m1_testbed.save.new`, remove the worktree (`git worktree remove C:\fl\m1fix`), and paste the JSON, the byte size and the sha256 into the report.
- [ ] **Step 1: Write the failing tests:**
```rust
// crates/fl_sim/src/units.rs (#[cfg(test)])
#[test] fn frozen_v2_reads_current_move_order_bytes()  // MoveOrder { orders: [Move (1, 2), Rotate 90°, Follow #3 (0, -8)] } → postcard →
                                                        //   v2::MoveOrder with the same three orders field by field
// crates/fl_test_utils/tests/m1_save.rs
#[test] fn m1_testbed_save_loads()                  // load_world(fl_catalog::features(), base_content(), fixture, hash_interval 1) → Ok;
                                                    //   SimClock tick 310; SimIndex len 13
#[test] fn m1_save_continues_with_m1_pose_trace()   // Scenario::load(fixture) then pose_trace(590) == the recorded trace's entries for ticks 310..=899
// crates/fl_test_utils/tests/golden.rs
#[test] fn testbed_paths_pose_trace_matches()       // pose_trace(testbed_paths().build(), TICKS) == read_pose_trace(replays/testbed_paths.poses);
                                                    //   with FL_RECORD_POSES=1 it writes replays/testbed_paths.poses.new instead
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim -p fl_test_utils -E 'test(/frozen_v2|m1_|pose_trace/)'`. Expected: FAIL (module and files missing).
- [ ] **Step 3: Implement.** `pose_hash` uses `fl_sim::StateHasher::new` and its `write_*` methods, so no new dependency.
- [ ] **Step 4: Record** `FL_RECORD_POSES=1 cargo nextest run --profile agent -p fl_test_utils --test golden -E 'test(pose_trace)'`, which writes `testbed_paths.poses.new` (900 lines). The tests that read the fixture and the trace pass only after the bless (step 6).
- [ ] **Step 5: Gate** `cargo xtask gate task --track sim-core`. Expected: PASS apart from the two tests that read the pending fixtures; paste which.
- [ ] **Step 6: Bless (lead)** `cargo xtask bless crates/fl_test_utils/tests/replays --reason "M2P Task 2: m1 testbed save fixture and testbed_paths pose trace (M11, R83)"`, then re-run the step 2 command. Expected: PASS.
- [ ] **Step 7: Commit** `test(fl_test_utils): m1 testbed save fixture, frozen MoveOrder v2, pose trace of the movement testbed` with trailers `Change-Class: R` and `Blessed: …`.

