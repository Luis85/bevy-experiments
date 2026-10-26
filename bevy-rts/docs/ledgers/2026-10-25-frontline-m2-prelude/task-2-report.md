# Task 2 report
Status: DONE_WITH_CONCERNS (fixtures await bless)

## Fixture (Step 0)
Built from tag m1 in temp worktree C:/fl/m1fixture (build dir C:/t/fl/m1fix); worktree removed.
JSON: {"entities":13,"hash":"0x2efcb781c3d2728f","pending_commands":0,"tick":310}
Size 194510 bytes; sha256 a21434620048bf426bd6f7affb46c6d54c73a82fc2b683d853383927733740cc
(cli warned the replay was recorded with build 9799309 vs 95f03a6: harmless.)

## Implemented
- fl_sim/units.rs: frozen `pub(crate) mod v2` (MoveOrder, Order) with `#[allow(dead_code, reason)]` until Task 13; test frozen_v2_reads_current_move_order_bytes.
- fl_test_utils/src/trace.rs: pose_hash, pose_trace, write_pose_trace, read_pose_trace (re-exported).
- tests/m1_save.rs (2 tests), golden.rs testbed_paths_pose_trace_matches (FL_RECORD_POSES=1 writes .new).
- Pending: replays/m1_testbed.save.new, replays/testbed_paths.poses.new (900 lines).

## Tests
- With the .new files temporarily copied to their final names (then removed): `cargo nextest run --profile agent -p fl_sim -p fl_test_utils -E 'test(/frozen_v2|m1_|pose_trace/)'` -> 17 run, 17 passed. The M1 save continues with identical pose hashes for ticks 310..899.
- `cargo xtask gate task --track sim-core`: fmt/clippy/api/data-check PASS; test-headless FAIL only on the 3 tests reading pending fixtures: m1_testbed_save_loads, m1_save_continues_with_m1_pose_trace, testbed_paths_pose_trace_matches. Final line "gate task: FAIL (test-headless) in 1047s" (budget 300s; shared builders/cold cache).
- No separate RED run captured (new tests reference missing items/files by construction).

## Lead step
`cargo xtask bless crates/fl_test_utils/tests/replays --reason "M2P Task 2: m1 testbed save fixture and testbed_paths pose trace (M11, R83)"`, then rerun; commit needs `Blessed:` trailer (the commit already has Change-Class: R).

## Fix round 1
- units.rs: `frozen_v2_*` decodes a literal 25-byte postcard string (3 orders; bytes from the m1 encoding, which the unchanged types still produce) via `v2`, plus a 33-order over-cap rejection. `mod v2` now owns its `capped` and a copied cap of 32; no `super::`/`crate::` dependence.
- m1_save.rs `m1_testbed_save_loads` asserts mid-path content. Scan of the tick-310 fixture (13 units): 7 PathState::Following, 6 PathState::Flow, 5 queued Rotate/Follow orders, 0 pending path requests (queue drained; first 40 ticks show the same counts). Fixture tick kept at 310; `m1_testbed.save.new` unchanged (sha a2143462...40cc).
- golden.rs doc reworded (trace recorded on the prelude branch; 310..899 cross-checked against m1 output by m1_save.rs).
- `testbed_paths.poses.new` unchanged.
- Tests (.new copied to final names then removed): `cargo nextest run --profile agent -p fl_sim -p fl_test_utils -E 'test(/frozen_v2|m1_|pose_trace/)'` -> 17 passed. Clippy clean apart from pre-existing clippy.toml rand warnings.
