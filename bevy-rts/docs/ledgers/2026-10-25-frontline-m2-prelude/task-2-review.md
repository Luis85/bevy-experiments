### Spec Compliance

- ✅ Spec compliant (every listed file has its hunk; interfaces match the brief).
- ⚠️ Cannot verify from diff: the brief wants the fixture mid-path with a Shift-queued rotate+move, the follower and player 1's flow group; the report adds "queued path requests". The save is opaque postcard. Verified: header build id is `0.0.0+95f03a6` (m1_testbed.save.new offset 0x03) and `git rev-parse m1^{commit}` = 95f03a6, so it really is from tag m1; size 194510 and sha256 a2143462...40cc match the report; `core.path_flow_cache`, `core.path_queue`, `core.path_follow` are registered in it. Not verified: that the queue holds pending requests or any unit is on `PathState::Flow` at tick 310 (names are present regardless of contents). By the scenario, tick 310 has the tick-200 queue (rotate+move) and the tick-300 follow; the tick-5 group move (the flow group) is the one most likely already drained. Controller check: run the `core.paths` debug query (or a throwaway test) on the fixture and look for a Flow unit and a non-empty PathQueue. If the queue is empty, take the save a few ticks after a re-issued request: tick 300-302 (the follow's repath request, just after FOLLOW_TICK) or just after tick 120 (the drown repath), or add a scenario command that issues a fresh group move right before the save tick.

### Strengths
- Fixture provenance is sound: built from the m1 commit, build id in header proves it.
- `v2::MoveOrder`/`Order` match `git show m1:units.rs` (285-318) field by field and variant by variant (Move, Rotate, Follow order preserved; `capped` attr kept); only `Serialize` dropped.
- `pose_hash` is straightforward and matches the brief (SimIndex order, id, pos, heading, hp or u32::MAX). Write/read round-trip format is exact (`{:016x}`), read errors are reported with path and line.
- Trace comparison is exact `assert_eq!` on `Vec<(Tick,u64)>`; the m1-save test cross-checks an M1-produced state against the current build's recorded hashes for 310..899 (length-asserted), which is a real behaviour check, not a tautology.

### Issues

#### Critical (Must Fix)
None.

#### Important (Should Fix)
- crates/fl_sim/src/units.rs:72-102 `frozen_v2_reads_current_move_order_bytes` encodes the live `MoveOrder` and decodes it as v2. It does not pin the v2 encoding: when Task 13 changes `MoveOrder`/`Order` this test fails or has to be rewritten, and it never touches M1-written bytes. Task 13's `migrate_from::<v2::MoveOrder>(2)` therefore has no standing guarantee. Fix: assert a literal postcard byte string for the same three orders (decode it as v2, and optionally check the bytes equal the current encoding today), so the test keeps passing unchanged after Task 13. (Brief-mandated shape; still a defect.)

#### Minor (Nice to Have)
- crates/fl_sim/src/units.rs:38 `v2` uses `super::capped` and `crate::MAX_WAYPOINTS`, which are live, not frozen; Task 13 changing either silently changes v2 decoding. Note in the doc comment (or copy the cap constant into v2).
- crates/fl_test_utils/tests/m1_save.rs:348-359 `m1_testbed_save_loads` asserts only tick 310 and 13 entities; nothing guards the brief's "mid-path, flow units, queued orders" content, so a fixture regenerated at a different scenario state would pass. Add a check that some unit has a non-empty `MoveOrder` with a Follow/Rotate, and a `PathState::Flow` unit.
- crates/fl_test_utils/tests/golden.rs:290-293 doc says the trace was "recorded at tag m1"; it was recorded on the task branch build (brief step 4). Only 310..899 is cross-validated against m1 output. Reword.
- Report: gate task ran 1047s against a 300s budget, no RED run captured (both reported, neither blocks).

### Assessment

**Task quality:** Needs fixes (one small test change: pin v2 bytes literally).

**Reasoning:** The fixture provenance, frozen types and trace mechanics are correct; the v2 codec test is the only item that does not give Task 13 the stable pin the brief wants, and the flow-unit/queued-request content of the fixture needs a controller-side confirmation.

Bless: m1_testbed.save.new yes (provenance verified; content check above is advisory); testbed_paths.poses.new yes (900 lines, ticks 0..899, contiguous, consistent with the m1 continuation).
