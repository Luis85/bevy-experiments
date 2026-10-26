### Task 9: `core.rotate`, `core.follow`, Shift-queue semantics, formation offsets

**Track:** sim-core · **Refs:** ADR-0007 (refinement), spec §7 · **Lane 1**

**Files:**
- Modify: `crates/fl_sim/src/{commands,orders,movement,path}.rs`, `crates/fl_test_utils/src/builder.rs` (`Scenario::world_mut`)
- Create: `crates/fl_sim/tests/orders.rs`
- Blessed: system order (if it changes), API, golden.

**Interfaces:**
- Consumes: `RotateCmd`, `FollowCmd` (Task 2); `Order`, `enqueue_path`, `PathFollow` (Task 8); `MovementRules.{formation_max_spread_m, follow_distance_m, follow_repath_ticks}`.
- Produces:
```rust
pub fn formation_targets(positions: &[FxVec2], target: FxVec2, max_spread: Fx, map: &MapInfo) -> Vec<FxVec2>;
    // centroid c = Σ pos / n; offset_i = pos_i − c; R = max |offset_i|; if R > max_spread every offset × max_spread / R; target_i = map.clamp(target + offset_i)
impl Scenario { pub fn world_mut(&mut self) -> &mut World; }   // fl_test_utils, for between-tick setup_despawn in tests
// "core.rotate": unit_can = Mobility; facing_i = (toward − pos_i).angle() (current heading if equal); not queued → orders [Rotate], PathFollow Idle; queued → append
// queued rotate and follow obey MAX_WAYPOINTS like a queued move: any unit with full orders → the whole command "core.queue_full", nothing changed
// "core.follow": leader must exist with a Pose, else RejectReason::Feature("core.invalid_target"); the leader is removed from the units; offset_i = (pos_i − leader) scaled to
//   length min(|pos_i − leader|, follow_distance_m); advance_orders re-plans to leader + offset when the state is Idle/Done, or when
//   tick − planned ≥ follow_repath_ticks and |leader + offset − goal| > one cell; a missing leader pops the order ("completed")
// "core.move" with more than one unit uses formation_targets (queued moves too, from the positions at command time)
```

- [ ] **Step 1: Write the failing tests** (`crates/fl_sim/tests/orders.rs`):
```rust
#[test] fn formation_keeps_relative_offsets()    // 3 tanks 10 m apart on an open map, move 300 m: final pairwise offsets within 1 m of the starting ones
#[test] fn formation_spread_is_capped()          // units 200 m apart: |target_i − target| ≤ 40 m for all i
#[test] fn formation_targets_pinned()            // pure: positions (0,0),(10,0) → target (100,50): [(95,50),(105,50)]
#[test] fn rotate_turns_in_place()               // position unchanged; heading reaches facing within ceil(diff / turn_rate) ticks; the order pops
#[test] fn rotate_queued_after_move()            // Shift: move, then rotate → arrives, then faces the point
#[test] fn follow_tracks_moving_leader()         // leader moves 200 m: follower ends within follow_distance + 4 m; path requests ≤ ticks / 20 + 2
#[test] fn follow_ends_when_leader_is_gone()     // setup_despawn(leader) between ticks → the follower's order pops; it stops
#[test] fn follow_self_or_missing_leader_rejected()   // FollowCmd { leader: own SimId } alone → "core.no_valid_units"; a missing leader → "core.invalid_target";
                                                      //   state unchanged, LastSeq advanced
#[test] fn shift_queue_mixed_orders()            // move, rotate, move queued → executed in order; order_changed "completed" three times
#[test] fn queued_orders_share_the_cap()         // 32 queued orders of mixed kinds; a queued rotate or follow → "core.queue_full"; orders unchanged
#[test] fn orders_twin_and_shuffle()
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test orders`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim -p fl_test_utils`. Expected: PASS (protected `.new` pending).
- [ ] **Step 5: Regenerate and bless (lead)** the golden (the testbed moves two units, so formation applies) and API: `M1 Task 9: rotate, follow, formation offsets`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track sim-core`. Expected: PASS.
- [ ] **Step 7: Commit** `feat(fl_sim): core.rotate, core.follow, Shift-queue for every order, group moves keep formation offsets` with trailer `Blessed: …`.

