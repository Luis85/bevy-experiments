# Task 8 re-review, fix round 1 (c3f82c1)

## Finding verdicts
- Important 1 (per-tick allocations): ADDRESSED. orders.rs:327 and movement.rs:114-118 take the cached `QueryState` as a system param, and no query is built per tick. The scratch Vecs are taken, filled, sorted, cleared and put back (orders.rs:335-348, movement.rs:130-151). OrderScratch is registered Transient `Class::TransientReset(OrderScratch::clear)` (orders.rs:372-379); `clear` keeps capacity. R72: the reset runs in the Housekeeping slot every tick (classify.rs:794+). It is never hashed or saved, and each system empties its buffer anyway, so nothing survives a tick and hashes and saves are unaffected. The two fields are disjoint, so the take-and-restore calls cannot collide.
- Minor 1 (second nav install site): ADDRESSED. `nav::install_nav` (nav/mod.rs:198) is called by `rebuild_nav` (:191) and `setup_scenario_def` (scenario.rs:552). `plan_for` with a grid but no graphs now debug_asserts and returns Unreachable, not a straight line (path.rs:467-470).
- Minor 2 (rule ranges): ADDRESSED. movement.rs link checks `turn_in_place_deg` in (0, 180] and `arrive_m` > 0. The test covers 0, 180.5, -10, 180 ok, arrive 0 and -0.5 (terrain.rs, movement_rules_ranges_are_validated). It is stricter than my suggested [0, 180] for turn=0, which is fine.
- Minor 3 (duplicate class_name): ADDRESSED. One `pub(crate)` `nav::class_name`; the path.rs copy is gone.
- Minor 4 (cap on decode): ADDRESSED. `units::capped` is applied to v2 `orders` and v1 `waypoints`; a failure becomes `LoadError::Codec{core.move_order}`. `over_cap_move_order_fails_to_load` covers 33 failing and 32 loading at both versions. The codec snapshot is unchanged by it.
- Minor 5 (m0_save tautology): ADDRESSED. It now needs a `found` path_result and at least one unit whose position changed.
- Minor 6 (pop order within a tick): ADDRESSED. `queue_pops_in_request_order_not_sim_id_order` issues 40 reversed single-unit commands. Each tick's result set must be the next contiguous block of request ids, and some tick must hold more than one result. The doc comment on `results_by_tick` explains the limit of the journal.
- Minors 7-9: carried to Tasks 9-11 (out of this round).
- Minor 10 (report accuracy): the fix report states the alloc baselines correctly.

## New findings
None Critical or Important.
- Minor: `plan_for` returns Unreachable for a grid without graphs only in release builds; debug builds panic. Intended.
- Minor: in `OrderScratch::due` and `wanderers`, a panic between take and restore would leave an empty default buffer. Harmless (capacity is regrown).
- Note: `formats__save_codecs.snap.new` and `system_order__sim_tick.snap.new` have mtime 22:45, later than the other `.new` files (22:36). Their content in the diff matches the first submission. They were re-emitted by a later test run, so there is no concern.

## Bless per .new
- `system_order__sim_tick.snap.new`: bless. Slots match the brief.
- `formats__save_codecs.snap.new`: bless. `core.move_order@2 reads 1`, `core.path_follow@1`, `core.path_queue@1`.
- `xtask/snapshots/api/fl_sim.txt.new`: bless. Changes are additive pub items (Order, PathState, RequestKind, cluster_rect); the new fix items are pub(crate).
- `testbed_wander.replay.new`: bless. It is dated after the commit (22:36 vs 22:35) and the report says replay-verify gave "verified: 400 ticks" at c3f82c1.
- `perf_counters__scale_{500,3000,5000}.snap.new`: bless. Only path.* counters were added; spatial totals drift by under 0.5%. Totals match the first submission.
- `docs/perf/budgets.ron.new`: bless. The path.* counters are added. alloc.count goes 77->78 (scale_500) and 67->72 (scale_3000), and 92->85 (scale_5000). The measured maxima are +1, +4 and -5. This is explained by owned corner lists and the new BTreeMap counter nodes. alloc.bytes budgets rise slightly; spatial budgets move in line with the snapshots.

Verdict: all findings addressed, no new Critical or Important breakage.
