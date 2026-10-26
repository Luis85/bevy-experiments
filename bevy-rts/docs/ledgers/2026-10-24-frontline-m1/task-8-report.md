# Task 8 report — path requests, order queue, path following

**Status:** DONE_WITH_CONCERNS (see Concerns). Commit `2782dc9` on `m1/phase-a` (base `ab51abb`).
Protected artifacts are regenerated as `.new` only; nothing blessed.

## What was implemented

- **`crates/fl_sim/src/path.rs` (new).** `PathFollow` (Hashed `core.path_follow` v1),
  `PathState` (Idle / Waiting{request} / Following / Flow{key} / Done / Failed), `FlowKey`,
  `PathQueue` (Hashed `core.path_queue` v1), `PathRequest`, `RequestKind` (Single / Group).
  - `enqueue_path(world, unit, goal) -> u32` (journals in `Phase::Think`) and crate-private
    `enqueue_path_in(.., phase)` (commands journal in `Phase::Input`): push back, unit's
    `PathFollow` → `Waiting{id}` with no corners, journal `core.path_request`
    `{request, goal, profile, kind: "single"}`.
  - `process_path_queue` (exclusive, core slot of `ThinkSet::Behaviour`): pops while
    `spent < Path.budget_per_tick`, at least one per tick (R98). Stale = unit gone or not
    `Waiting{request: id}` → `core.path_result` `"stale"`, charge 0. Otherwise `plan_path` from
    the current pose with the unit's profile, charged `PathWork::charge()` (cold cost, R84), applied
    immediately (`Following` with corners, `Failed` for `Unreachable`, `planned = tick`), so
    results apply in request order. Journal `core.path_result`
    `{request, status, corners, cost, charge, waited}`. Counters `path.requests` (processed),
    `path.queue_len` (left), `path.nodes_expanded`, `path.los_cells`, `path.budget_spent`.
  - `invalidate_paths` (called at the end of `nav::on_terrain_changed`): in `SimIndex` order,
    every `Following` unit whose remaining route (pos → `corners[next..]`) touches a rebuilt
    cluster's rect (exact segment/box test in Fx, unit-tested) → `Idle`, journaled
    `core.order_changed` `"replanned"` (phase Input). `advance_orders` re-requests in the same tick.
  - Debug query `core.paths` `{"units":[ids]}` → per id `{state, goal:[xr,yr], corners, next,
    orders:[{"move":[xr,yr]}|{"rotate":raw}|{"follow":id}], profile:{class,size}}`; `Err` on
    missing/non-array `units`, non-u32 ids, unknown ids, immobile units, no map. Never panics.
  - R78 fills: `core.path_queue` → empty; `core.path_follow` → `PathFollow::default()` on every
    `Mobility` entity (SimIndex order). Codecs trace `MoveClass`/`PathState` and `RequestKind`.
- **`crates/fl_sim/src/orders.rs` (new).** `Reason` + `journal_order_changed`
  (`core.order_changed {orders, current: move|rotate|follow|none, reason}`, replaces
  `core.move_order`). `advance_orders` (exclusive, `ThinkSet::Orders`, units in SimId order):
  pops `Done` moves (`completed`), `Failed` moves (`failed`); `Idle` + current Move → request;
  Rotate pops when heading == facing (path kept Idle); Follow (minimal until Task 9): pops when the
  leader is gone, requests `leader + offset` when Idle; no orders + Done/Failed → Idle.
- **`units.rs`.** `MoveOrder { orders: Vec<Order> }` v2, `Order { Move, Rotate, Follow }` (R69),
  `Codec::new(2).migrate_from::<v1::MoveOrder>(1).trace_enum::<Order>()` (R70 format guard).
  `MobilityDef` also inserts `PathFollow::default()`.
- **`movement.rs`.** `wander_system` is now exclusive `fn(InMut<Stream>, &mut World)` chained
  after `advance_orders` in `ThinkSet::Orders`; same draws (x then y of `rolls(seed, tick, id,
  "debug.wander")`), sets `[Move]` and enqueues. `move_system` follows corners: within `arrive_m`
  of the last corner (but beyond one step) → Done; otherwise turns by `turn_rate`, turns in place
  while |error| > `Movement.turn_in_place_deg` (30°; replaces M0's hard-coded 45°), moves
  `speed × speed_factor`, snaps onto a corner within one step and advances `next`. A step into an
  impassable cell is skipped, counts `move.blocked`, path → Idle. Current Rotate (not following)
  turns toward `facing`.
- **`commands.rs`.** `core.move`: unqueued → `[Move]` + request per unit in SimId order; queued →
  append, request only if it was the only order; `MAX_WAYPOINTS` over all orders →
  `core.queue_full`, nothing changed. `core.stop`: orders cleared, path Idle, no corners
  (pending request turns stale), reason `stopped`.
- **`debug_cmds.rs`.** `debug.teleport` clears `orders` and sets the path Idle.
- **`publish.rs`.** `MOVING` while `PathFollow` is `Following` or `Flow`.
- **`nav/hpa.rs`.** Only `HpaGraphs::cluster_rect(id) -> Option<CellRect>` added (not split into
  `nav/plan.rs`: not touched substantially).
- **`scenario.rs` (outside the Files list, necessary).** `setup_scenario_def` inserted the
  `NavGrid` but never the `HpaGraphs` (a Task 7 gap). Live scenario matches therefore had no
  graphs while a loaded save rebuilt them, so plans differed and `smoke_branch` diverged at the
  first post-load path request. Now it builds `HpaGraphs` with the grid (R53). Regression test
  `scenario_setup_builds_the_hpa_graphs`. `plan_for` also `debug_assert!`s that a grid never
  comes without graphs.

### Decisions the brief left open

- **R97:** I aligned the fixture rather than making the partial scan exact.
  `unreachable_goal_goes_to_closest_point` puts the lake (with a 2×2 island) inside one cluster,
  the north-east one, so the cluster of the best settled node holds the reachable cell nearest the
  goal. The test asserts that the final cell equals the nearest reachable cell **of the whole map**
  (full-map Dijkstra, `(octile, y, x)` tie-break). Its comment says the plan scans the start
  cluster plus the best node's cluster.
- **R99:** `path_result.cost` is documented as an octile lower bound for direct lines. Nothing
  compares it as a true cost.
- **Task 7 review minors:** `hpa_cost_near_optimal` now also asserts `cost >= optimal` for
  searched plans (no direct line from→to). `wide_profile_avoids_narrow_gap` now compares
  `wide.cost` with the tank's true A* optimum instead of its direct-line estimate.
- **Journal phase of `enqueue_path`:** the brief's signature has no phase. The public fn journals
  in Think, and commands use `enqueue_path_in(.., Phase::Input)`.
- **Wander assignment** journals `path_request` but not `order_changed`, because it is not a
  command. Its completion journals `completed`.
- **Bevy API (for the lead to record in docs/research):** in bevy_ecs 0.20, `&mut World` is a
  `SystemParam` (`system_param.rs:822`), so `fn(InMut<T>, &mut World)` is an ordinary function
  system and `.with_input(v)` and `.chain()` work on it. A precedent is `exclusive_in_out(_: In<A>,
  _: &mut World)` at `system/mod.rs:1755`. VERIFIED by compiling it here.

## Tests

`crates/fl_sim/tests/pathing.rs` holds all 17 brief tests plus `scenario_setup_builds_the_hpa_graphs`:
tank_paths_around_lake, crosses_river_only_by_bridge, unreachable_goal_goes_to_closest_point,
hull_rotates_before_moving, queued_moves_run_in_order, stop_clears_orders_and_pending_request,
stale_request_is_discarded, despawned_unit_request_discarded, queue_respects_budget_and_order,
no_request_starves, bridge_destroyed_mid_route_repaths, wander_goes_through_the_queue,
branch_with_queued_requests, move_order_v1_migrates, path_counters_and_journal,
paths_debug_query, smoke_pathing_twin_and_shuffle. There is also a unit test `segment_box_contact` in path.rs.

`queue_respects_budget_and_order` fixture note: the plan's "wall" fixture made every request cost
about 1000 work, so each tick processed exactly one and the budget check was vacuous. The fixture is a
pond with goal (20,10), which spreads 40 requests over 13 ticks: up to 6 per tick, with cheap
direct lines (24–28) and searches (100–620). The test also asserts >2 ticks and some tick with
>1 result.

### TDD evidence
- RED: `cargo nextest run --profile agent -p fl_sim --test pathing` →
  `error[E0432]: unresolved imports fl_sim::Order, fl_sim::PathFollow, fl_sim::PathQueue,
  fl_sim::PathState` / `no field orders on type &fl_sim::MoveOrder`. This was expected because the
  types did not exist yet.
- GREEN: same command → `17 tests run: 17 passed` (18 with the HPA setup test).

### Full run (this session)
`cargo nextest run -p fl_sim -p fl_test_utils -p fl_catalog -p fl_cli` →
`384 tests run: 378 passed, 6 failed`. The 6 failures are exactly the protected artifacts
awaiting bless: golden_replays_verify, save_codecs_of_every_feature, smoke_sim_tick_system_order,
smoke_counters_scale_500, counters_scale_3000, milestone_counters_scale_5000. `cargo fmt --check`
is clean, and `cargo clippy --workspace --all-targets -D warnings` is clean except
`probe_dynlink`, which is pre-existing and stale (references `fl_sim::GameFeature`/`SimConfig`) and
not a default member. The two `clippy.toml` rand warnings are pre-existing.

### Test-guard findings (ported or changed assertions)
- `fl_sim/tests/commands.rs`:
  - The `waypoints()` helper now maps `orders` to Move targets (and panics on a non-move).
  - `move_command_reaches_target`: after arrival the move is still present for one tick (popped by
    `advance_orders` next tick). The test asserts `[v(200,100)]`, steps 1, then asserts empty.
  - `queued_waypoints_are_capped`: it now also asserts the orders are unchanged by value, that
    `QueueFull.code() == "core.queue_full"`, and that `LastSeq` advanced (R45).
- `fl_sim/tests/movement.rs`: `order.waypoints.is_empty()` → `order.orders.is_empty()` (×2).
- `fl_sim/tests/save.rs`: `order.waypoints == [v]` → `order.orders == [Order::Move{target: v}]` (×2).
- `fl_sim/tests/runner.rs`:
  - The `waypoints()` helper is ported as above.
  - `publish_full`'s MOVING bit now comes from `PathFollow` Following/Flow.
  - `other_queries_answer`'s journal kinds changed `["command_applied","move_order"]` →
    `["command_applied","order_changed","path_request","path_result"]`.
- `fl_sim/tests/observe.rs`:
  - `applied_and_rejected_commands_are_journaled` now reads `core.order_changed` (`orders` 1 /
    `current` "move" / `reason` "command"; stop → 0 / "none" / "stopped") instead of
    `core.move_order {waypoints}`.
  - `journal_entries_sorted`: tick 3's expected keys gain `order_changed`, `path_request` (Input)
    and `path_result` (Think), and the pings slice moved `[2..5]` → `[4..7]`.
  - `journal_identical_across_shuffle_seeds`: count changed `10*5+2` → `10*5+4`.
- `fl_sim/tests/terrain.rs` `units_stop_at_blocked_cells` (RemovedAssert, rewritten). A move into
  water now snaps the goal to the shore (centre x = 14 m), so the old `pos.x > 15` and `blocked > 0`
  no longer hold for a planned move. Part 1 asserts the tank never enters the water and ends at
  x > 13. Part 2 forces a `Following` path straight into the water and asserts it never enters,
  stops with x > 15, `move.blocked` counted exactly once, and the path `Idle`.
- `fl_sim/tests/hpa.rs`: Task 7 review minors (added assertion; replaced `wide.cost > tank.cost`
  with a comparison against the tank's A* optimum).
- `fl_test_utils/tests/m0_save.rs`:
  - New assertions that `PathQueue` is empty, every Mobility unit has `PathFollow::default()`, and
    each v1 MoveOrder migrated to Move orders. The fixture's units stand still at tick 40, so their
    v1 orders are empty; non-empty migration is covered by `pathing.rs::move_order_v1_migrates`.
  - A loaded copy is ordered to move, and a `core.path_request` must appear within 50 ticks. The
    fixture has no wanderers, so a move is issued.

## Protected artifacts (`.new`, for the lead; never blessed)

Suggested bless reason: `M1 Task 8: path queue, MoveOrder v2, wander through the queue`.

1. **`crates/fl_test_utils/tests/snapshots/system_order__sim_tick.snap.new`**: `wander_system`
   moved from Think/Behaviour to Think/Orders, after the new `orders::advance_orders`, and
   `path::process_path_queue` was added in Think/Behaviour.
2. **`crates/fl_test_utils/tests/snapshots/formats__save_codecs.snap.new`**:
   `core.move_order@1` → `core.move_order@2 reads 1` (`orders: Seq(Order)`, the `Order` enum and
   `SimId`). New blocks `core.path_follow@1` (`PathFollow`, `PathState`, `FlowKey`, `ClusterId`,
   `MoveProfile`, `MoveClass`, `Tick`) and `core.path_queue@1` (`PathQueue`, `PathRequest`,
   `RequestKind`, `SimId`, `Tick`).
3. **`xtask/snapshots/api/fl_sim.txt.new`**: new public `Order`, `PathFollow`, `PathState`,
   `FlowKey`, `PathQueue`, `PathRequest`, `RequestKind`, `enqueue_path` and
   `HpaGraphs::cluster_rect`. `MoveOrder::waypoints` → `MoveOrder::orders`. `cargo xtask api`:
   "1 of 5 crates changed: fl_sim".
4. **`crates/fl_test_utils/tests/replays/testbed_wander.replay.new`** (`FL_RECORD_GOLDEN=1`).
   The old golden diverges at **tick 0** (expected 0x23b82a0646d91582, actual 0x57ef0ca1ac59fc73) because:
   - the state hash now includes the `core.path_queue` resource and a `core.path_follow`
     component on every unit;
   - wander and move targets now go through path requests with goal snapping (R92 carry: wanderers
     route around water instead of stopping at shores);
   - movement follows planned corners and turns in place past 30° (rules) instead of 45°;
   - finished orders pop one tick later (in `advance_orders`).

   The recorded `.new` verifies: `fl_cli replay-verify` → "verified: 400 ticks". The command
   stream is unchanged; only hashes moved.
5. **`docs/perf/budgets.ron.new`** (`FL_RECORD_BUDGETS=1 … --test perf_counters -E 'test(/scale/)'`)
   and **`perf_counters__scale_{500,3000,5000}.snap.new`**:
   - New counters `path.requests`, `path.los_cells` and `path.budget_spent`. Per-tick max budgets:
     scale_500 9 / 417 / 417; scale_3000 29 / 1564 / 1564; scale_5000 50 / 2625 / 2625. Totals
     over 200 ticks: 304 / 14004, 1822 / 84030 and 2967 / 136243.
   - The scale map is flat, so every wander request is a direct line: `path.nodes_expanded` stays
     0 (not recorded), and charge = LOS cells. `path.queue_len` stays 0 (8000 budget, no backlog).
   - `spatial.query_candidates` and `spatial.relocations` shift slightly (±0.5%) because wanderers
     now pop finished orders a tick later and turn in place past 30° instead of 45°, so positions
     differ.
   - Allocations per tick went up (budgets): alloc.count 77→118 (scale_500), 67→118 (scale_3000)
     and 92→133 (scale_5000); alloc.bytes 5975→9615, 18850→25155 and 29250→40850. [Corrected in
     fix round 1: the first version of this report swapped the scale_500 and scale_3000
     baselines. Superseded by fix round 1, which removes the regression.] The causes are the per-tick `QueryState`
     and `Vec` built by the exclusive `advance_orders` and `wander_system`, plus path
     corners/requests.

## Gate

**Run 1:** `cargo xtask gate task --track contract,sim-core,content,tools,docs` → `gate task: FAIL
(check-scope, test-headless, api) in 393s`.
- check-scope: `docs/perf/budgets.ron.new → perf`. The brief's track is `sim-core + perf`, and the
  coordinator's list leaves out perf.
- test-headless: a `rust-lld` "undefined symbol anon.…llvm" link failure of `fl_sim` test
  `schedule`. This was a stale incremental cache in `C:/t/fl/c5/fdf61b108f2dc9/debug/incremental/schedule-*`
  left by the interrupted earlier gate run. I deleted that directory, and the test then links.

**Run 2** (perf track added): `cargo xtask gate task --track contract,sim-core,content,tools,docs,perf`:

```
PASS fmt (9.8s)
PASS clippy-drift (3.6s)
PASS claude-md (0.0s)
PASS check-scope (0.4s)
PASS protected (0.4s)
WARN test-guard (3.1s): test-guard: 12 findings (reviewer must accept each)
PASS clippy (38.6s)
PASS arch-check (5.9s)
FAIL test-headless (83.9s)
SKIP test-client: no client package touched since ab51abb (.gate-base)
FAIL api (46.4s)
PASS data-check (5.8s)
gate task: FAIL (test-headless, api) in 199s
```

Both failures are bless-pending artifacts only:
- **test-headless:** `smoke_counters_scale_500` / `counters_scale_3000` exceed the *committed*
  budgets (alloc, new `path.*` counters with no budget), and fail-fast stopped the rest. The
  measured maxima × 1.25 headroom equal `budgets.ron.new`, for example `path.budget_spent` 333 →
  417 and `alloc.bytes` 7692 → 9615.
- **api:** "fl_sim: changed" (`fl_sim.txt.new`).

The full `--no-fail-fast` run of the 4 packages (above) shows only the 6 protected tests failing.

Test-guard findings (12): the ported assertions listed under "Test-guard findings", plus
`pathing.rs:287 ToleranceChanged`. That one is a new test's 30° threshold, which is the
`Movement.turn_in_place_deg` value of the base rules and not a changed tolerance.

## Concerns

1. **Per-tick allocations up by about 40–50 per tick** at scale (QueryState creation in two
   exclusive systems each tick). Caching the QueryStates in a Transient resource would remove most
   of it. I did not do this, to stay within scope. The perf owner should decide.
2. **`scenario.rs` edited outside the Files list.** This fixes a Task 7 R53 gap (live scenario
   matches had no `HpaGraphs`) and was required for `smoke_branch`. Please ratify.
3. **The Follow order is minimal** (pop on missing leader, re-plan when Idle). The repath rules
   belong to Task 9. No handler produces Follow or Rotate yet.
4. **`move.blocked` is now a safety net.** Planned paths never step into blocked cells, and
   terrain changes re-plan. The rewritten `units_stop_at_blocked_cells` forces a bad path to
   cover it (test-guard RemovedAssert, listed above).
5. **The brief gave the queue test a wall fixture, which made the budget assertion vacuous.** I
   replaced it with a pond fixture so several requests share a tick.

---

# Fix round 1 (review task-8-review.md; rulings R101–R103)

**Commit:** `c3f82c1` fix(fl_sim): Task 8 fix round 1: cached order queries, one nav install, rule
and cap checks. HEAD is `c3f82c1`, and every `.new` was regenerated at this HEAD.

## What changed

- **Important 1 / R102, allocations.**
  - `advance_orders(world: &mut World, query: &mut OrdersQuery)` and
    `wander_system(InMut<Stream>, &mut World, &mut WanderQuery)` now keep their `QueryState` as a
    system param (bevy_ecs 0.20 `system_param.rs:2302`; no world access; an archetype cache, not
    sim state).
  - Their `due`/`wanderers` sort buffers live in a new Transient resource `OrderScratch`
    (`"core.order_scratch"`, `Class::TransientReset(clear)`, capacity kept). Each system takes the
    buffer, fills it, sorts it, empties it and puts it back.
  - **Deviation from the dispatch wording:** the ruling said "`Local` scratch vectors". The global
    constraints say "no `Local<T>` in sim systems ... scratch buffers kept between ticks are
    resources classified `Class::TransientReset(<clear that keeps capacity>)` ... Never
    `Local<T>`". I followed the constraint. The effect is the same: no per-tick allocation.
- **Minor 1 / R101, one nav install.** `nav::install_nav(world, grid)` inserts the grid and builds
  and inserts the `HpaGraphs`; Task 10's flow cache goes here. `rebuild_nav` (load) and
  `setup_scenario_def` (live setup) both call it. `plan_for` on a grid without graphs now
  `debug_assert!`s and returns `Unreachable` in release, instead of a straight line through
  obstacles.
- **Minor 2, rules ranges.** `MovementRules::link` checks `turn_in_place_deg` in (0, 180] and
  `arrive_m` above 0. Tests are in `terrain.rs::movement_rules_ranges_are_validated`: 0, 180.5 and
  -10 fail, 180 loads, and arrive 0 and -0.5 fail.
- **Minor 3.** One `nav::class_name` (`pub(crate)`) is used by `core.paths`; the copy in
  `path.rs` is gone.
- **Minor 4, R77 for loaded state.** `MoveOrder.orders` (v2) and v1 `waypoints` deserialize
  through `units::capped`. More than `MAX_WAYPOINTS` is a serde error, so it becomes
  `LoadError::Codec { name: "core.move_order" }`. Test
  `pathing.rs::over_cap_move_order_fails_to_load` checks that 33 fails and 32 loads, at v2 and
  at v1. The codec format snapshot is unchanged by this.
- **Minor 5.** `m0_save.rs` now also requires a `core.path_result` with status `"found"` and at
  least one ordered unit whose position changed within the 50 ticks.
- **Minor 6.** New test `queue_pops_in_request_order_not_sim_id_order`: 40 single-unit commands
  issued #40 to #1, so request ids run against sim ids. Each tick's results must be the next
  contiguous block of request ids (a FIFO prefix of the queue); popping by sim id would take
  other sets. The `results_by_tick` doc comment explains why the journal does not show the order
  within a tick.
- Minors 7-9 are carried to Tasks 9-11 (ledger). They are untouched here.

## TDD evidence

- **RED** (`cargo nextest run --profile agent --no-fail-fast -p fl_sim --test pathing --test
  terrain -p fl_test_utils --test m0_save`, before the source changes):
  `52 tests run: 50 passed, 2 failed`
  - `over_cap_move_order_fails_to_load`: `v2: an over-cap order queue loaded`
  - `movement_rules_ranges_are_validated`: `turn_zero: the mod loads`

  The queue-order and M0-save strengthenings passed at once: they pin behaviour that was already
  right.
- **RED for allocations:** per-tick alloc.count maxima measured at `2782dc9` (table below).
- **GREEN:** the same tests pass. Full run: `cargo nextest run --profile agent --no-fail-fast -p
  fl_sim -p fl_test_utils -p fl_catalog -p fl_cli` gives `386 tests run: 380 passed, 6 failed`.
  The 6 failures are the bless-pending artifacts: golden_replays_verify,
  save_codecs_of_every_feature, smoke_sim_tick_system_order and the three perf_counters tests.

## Allocations: alloc.count per-tick max (measured; budget = ceil(max x 1.25) in parentheses)

| scale | M0 baseline (`budgets.ron`) | Task 8 `2782dc9` | fix round 1 `c3f82c1` |
|---|---|---|---|
| scale_500  | 61 (77) | 94 (118) | 62 (78) |
| scale_3000 | 53 (67) | 94 (118) | 57 (72) |
| scale_5000 | 73 (92) | 106 (133) | 68 (85) |

alloc.bytes budgets: scale_500 5975 -> 9615 -> **6435**; scale_3000 18850 -> 25155 -> **20155**;
scale_5000 29250 -> 40850 -> **29820**.

**The remaining increase** (+1 / +4 allocations and about +370 / +1045 / +460 bytes measured at
500 / 3000 / 5000) comes from two places. Nothing else in the new code allocates per tick in the
steady state:
1. **The plan's corner list, one per processed request.** `plan_path` returns an owned
   `Vec<FxVec2>`, which moves into `PathFollow.corners`. Scale wanderers request up to 9 / 23 / 39
   paths in a tick. Removing it needs a reusable corner buffer through `plan_path` (a Task 7 API).
2. **B-tree nodes for the three new counter names** (`path.requests`, `path.los_cells`,
   `path.budget_spent`). `SimCounters` is a `BTreeMap` cleared every tick (parked Minor 10), so
   each new name costs node allocations every tick.

The rest of the path machinery reuses its capacity:
- The PathQueue `VecDeque` keeps its capacity across pops.
- A popped `MoveOrder.orders` keeps its capacity for the next wander push.
- The journal is off in perf runs.
- `trace!` is disabled.

scale_5000 is now below the M0 baseline. M0's wander re-target timing differed, so the max tick
differs.

## Regenerated `.new` at `c3f82c1` (never blessed)

- `system_order__sim_tick.snap.new`: unchanged from the first submission (wander to Think/Orders
  after `advance_orders`; `process_path_queue` added in Think/Behaviour).
- `formats__save_codecs.snap.new`: unchanged from the first submission (`core.move_order@2 reads
  1`, `core.path_follow@1`, `core.path_queue@1`).
- `xtask/snapshots/api/fl_sim.txt.new`: as before. All fix-round items are `pub(crate)`.
- `testbed_wander.replay.new` (`FL_RECORD_GOLDEN=1`): its header carries `0.0.0+c3f82c1`.
  `fl_cli replay-verify` on a copy gives **"verified: 400 ticks"**. It changed for the reasons
  given in the first submission (new hashed state from tick 0, wander through path requests, 30°
  turn, pop a tick later). The fix round changes no outcome.
- `perf_counters__scale_{500,3000,5000}.snap.new`: totals identical to the first submission.
  - New `path.requests`, `path.los_cells`, `path.budget_spent` totals: 304 / 14004 / 14004;
    1822 / 84030 / 84030; 2967 / 136243 / 136243.
  - `spatial.query_candidates` and `spatial.relocations` shift by under 0.5%. Wander now pops a
    tick later and turns in place past 30° instead of 45°, so positions differ.
- `docs/perf/budgets.ron.new`, re-recorded from scratch at `c3f82c1`:
  - **New counters:** `path.requests` 9 / 29 / 50, `path.los_cells` 417 / 1564 / 2625 and
    `path.budget_spent` 417 / 1564 / 2625 (500 / 3000 / 5000). On the flat scale map every
    wander path is a straight line, so `path.nodes_expanded` and `path.queue_len` stay 0.
  - **Changed budgets:** alloc and spatial values as in the table above.

## Gate at `c3f82c1`

`cargo xtask gate task --track contract,sim-core,content,tools,docs,perf` (tracks per R103(d)):

```
PASS fmt (4.6s)
PASS clippy-drift (2.0s)
PASS claude-md (0.0s)
PASS check-scope (0.3s)
PASS protected (0.3s)
WARN test-guard (1.8s): test-guard: 12 findings (reviewer must accept each)
PASS clippy (44.7s)
PASS arch-check (6.0s)
FAIL test-headless (345.3s)
SKIP test-client: no client package touched since ab51abb (.gate-base)
FAIL api (54.8s)
PASS data-check (14.7s)
WARN gate task took 475s (budget 300s)
gate task: FAIL (test-headless, api) in 475s
```

test-headless fails only on bless-pending artifacts: `smoke_sim_tick_system_order` (insta) and
`counters_scale_3000` ("path.budget_spent / path.los_cells / path.requests has no budget"). No
alloc budget is exceeded any more, even against the committed budgets. The run is fail-fast. api
fails on `fl_sim: changed`. The test-guard findings are the same 12 already accepted (the
pathing.rs line moved 287 to 288).
