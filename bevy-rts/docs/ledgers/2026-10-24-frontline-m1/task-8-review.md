# Task 8 review — path requests, order queue, path following (ab51abb..2782dc9)

Reviewed in two passes over review-t8.diff (src, then tests + `.new` artifacts). Outside the diff I made these focused checks:
(1) scenario.rs/nav/mod.rs/terrain.rs: does live setup and load both build HpaGraphs?
(2) MovementRules::link: is `turn_in_place_deg` range-checked?
(3) bevy_ecs 0.20 sources: which cached-query params exist?
(4) mtime/cmp of the `.new` files against the commit time.
I made no edits and ran no cargo commands.

## Spec Compliance

- ✅ Spec compliant. Every brief file has its hunk.
  - Types and signatures match: PathFollow, PathState, FlowKey, PathQueue, PathRequest, RequestKind, `enqueue_path`, MoveOrder v2 with the `Order` enum.
  - System slots match: Orders = advance_orders → wander (chained, exclusive `fn(InMut<Stream>, &mut World)`); Behaviour = process_path_queue; Act/Move = move_system.
  - Handlers match the brief:
    - unqueued move → `[Move]` and a request per unit in SimId order;
    - queued move → append, and request only when it becomes the only order; the cap counts every order kind and rejects the whole command (commands.rs:574,585);
    - stop and teleport → `Idle` with no corners.
  - Journal kinds, fields and counters are as briefed. MOVING comes from Following/Flow. All 17 brief tests are present, plus the ported `queued_waypoints_are_capped`, the m0_save extension and a regression test for the scenario HPA graphs.
  - Extra, justified edits:
    - scenario.rs (fixes a Task 7 R53 gap; see ruling a);
    - `HpaGraphs::cluster_rect`;
    - feature.rs build calls;
    - tests/hpa.rs (Task 7 review minors);
    - tests/terrain.rs (rewrite forced by goal snapping).
- ⚠️ Cannot verify from diff:
  - **Charge independence from cache state.** R84 is met because a request's charge is `PathWork::charge()` from a `plan_path` over HpaGraphs. That holds only if incrementally invalidated graphs always equal `HpaGraphs::build` (the Task 7 proptest, unchanged code). Controller: confirm the proptest still runs in the gate.
  - **Step 6 gate.** It has not passed yet. test-headless failed fail-fast on the bless-pending budgets, so `smoke_counters_scale_500` and `smoke_pathing_twin_and_shuffle` never ran inside a passing gate. Re-run the gate after the bless.
  - **Golden, perf and API `.new` predate the commit.** They were written at 21:42–21:48; 2782dc9 was committed at 21:52. The replay header carries build `0.0.0+ab51abb`, the pre-commit HEAD. No evidence shows the replay `.new` verifies against the committed code; the gate's golden test compares the committed golden, not the `.new`.

### Checklist results (with evidence)
- **Classification.**
  - PathQueue is Hashed `core.path_queue` v1, and PathFollow is Hashed `core.path_follow` v1 (path.rs:658–677). Both hash every field, with tag bytes on the enums.
  - Nothing is in flight: requests are planned and applied in the same pop (path.rs:364–443), so the only pending state is the queue plus `Waiting{id}`.
  - HpaGraphs is Derived. The load path rebuilds it through rebuild_core → rebuild_nav (nav/mod.rs:81–95). Mid-request save/load is covered by `branch_with_queued_requests` (pathing.rs:562; it asserts the queue is non-empty at the branch tick).
- **Request order is independent of entity order.**
  - Commands enqueue over `ctx.units` (SimId order).
  - advance_orders collects and then sorts by SimId (orders.rs:153–159); wander does the same (movement.rs:133–139).
  - invalidate_paths walks SimIndex (path.rs:498+).
  - The queue is FIFO. The budget is charged per request with the cold `PathWork`; stale requests charge 0. The loop breaks only when `processed > 0 && spent >= budget`, so at least one request runs per tick (R98).
- **Stale requests.** A request is stale unless its unit exists and is `Waiting{request: id}` (path.rs:385–409). Stop, teleport, re-move and despawn each make it stale. All four cases are tested.
- **v1→v2 migration.** `migrate_from::<v1::MoveOrder>(1)` (units.rs:436) maps each waypoint to `Order::Move` (units.rs:278). The formats `.new` shows `core.move_order@2 reads 1` (R70 guard). It is tested on synthetic non-empty v1 bytes (pathing.rs:584) and on the M0 fixture, where every v1 order is empty.
- **R77.** The cap counts every order kind when a command adds one. Nothing re-checks it when a save is decoded (Minor 4).
- **Hull threshold 45°→30°.** The brief's move_system reads `turn_in_place_deg` and base rules.ron sets 30 (plan line 606). M0 hard-coded 45°. movement.rs:187 uses the rule, and `hull_rotates_before_moving` pins 30°. Justified.
- **Golden diverges from tick 0.** Sufficient: tick 0 already hashes the new PathQueue resource and a PathFollow on every unit, so the hash domain changed. The behavioural reasons explain the later stream: snapped and routed goals, corner following, 30°, and pop one tick later. Wander re-target timing is unchanged, since the pop and the new target both land in tick t+1 Orders. The command stream is unchanged; only the header sha and hashes differ.
- **Perf.**
  - `path.*` counters are plausible: the flat scale map gives direct lines, so charge = LOS cells and nodes_expanded = 0, with no backlog at budget 8000. The ±0.5% spatial drift is plausible.
  - The report swaps the alloc baselines: the budgets file had scale_3000 at 67 and scale_500 at 77, not the reverse.
- **Journal and counters.** As briefed. `path_request.profile` is `Option` (null without a grid), which is acceptable.
- **R72.** The diff adds no messages.
- **Debug query.** `core.paths` returns Err for every bad-args case and is tested (pathing.rs `paths_debug_query`). The only infallible lookups are of SimIndex/Content, which always exist.

## Strengths
- Stale detection by request id on the unit (`Waiting{id}`) is simple and exact, and makes stop, re-move, teleport and despawn work uniformly at zero charge.
- Plans apply in the same pop, which removes in-flight state entirely. That keeps save/load trivial and guarantees results apply in request order.
- `advance_unit` is a bounded loop: every iteration pops or returns, and the queue holds at most 32 orders. Each transition is documented.
- The implementer found and fixed a real live-vs-load divergence (scenario setup missing HpaGraphs) and pinned it with a regression test plus a `debug_assert`.
- The implementer replaced the brief's budget fixture, under which the budget assertion was vacuous, with one where requests actually share ticks. The test asserts >2 ticks and some tick with >1 result.
- Test-guard ports are faithful or stricter (`queued_waypoints_are_capped` now checks value equality, code and LastSeq). I accept all 12 findings, including the terrain.rs rewrite and the new-test ToleranceChanged at pathing.rs:287.

## Issues

### Critical (Must Fix)
None.

### Important (Should Fix)
1. **Avoidable per-tick allocation regression baked into a protected budget** (orders.rs:153, movement.rs:133; docs/perf/budgets.ron.new).
   - Cause: `world.query::<…>()` builds a fresh QueryState every tick in both exclusive systems, and the `due`/`idle` Vecs are rebuilt every tick.
   - Effect: alloc.count rises 67→118 (scale_3000), 77→118 (scale_500) and 92→133 (scale_5000), +40–55%.
   - Fix: bevy_ecs 0.20 has `&mut QueryState<D, F>` as a SystemParam with no world access (system_param.rs:2302). It composes with `&mut World` (system_param.rs:822), so `fn advance_orders(world: &mut World, q: &mut QueryState<(Entity, &SimEntity, &MoveOrder, &PathFollow)>)` caches it. Use `Local<Vec<_>>` scratch for the sort buffers.
   - This is a cache in system state, not sim state, so determinism is unaffected. Re-record budgets afterwards. Do not bless the raised alloc budgets.

### Minor (Nice to Have)
1. **Second nav install site** (scenario.rs:150–155).
   - setup_scenario_def now builds HpaGraphs by hand next to `rebuild_nav`'s copy (nav/mod.rs:91–94). Task 10 adds the flow cache in rebuild_nav ("Task 10 adds the flow cache after them"), so this is the same drift that caused the bug.
   - Fix: add one `install_nav(world, grid)` helper (grid + HpaGraphs + later caches) that both sites call. Optionally make the release-build fallback in `plan_for` (path.rs:331) unreachable rather than silently returning a straight line.
2. **`Movement.turn_in_place_deg` and `arrive_m` are not range-checked** (movement.rs:56 `link`). Task 8 is their first consumer: movement.rs:187 converts degrees to an Angle raw value, so 360 wraps to 0 (turn in place unless exactly aligned) and negative values wrap large. Add link checks: turn_in_place_deg in [0, 180], arrive_m ≥ 0.
3. **Duplicated `class_name` helper** (path.rs:574 duplicates nav/mod.rs:165). Reuse the nav one with `pub(crate)`.
4. **R77 holds only at command time.** A decoded v2 save, or a v1 save with more than 32 waypoints, loads an over-cap queue. Nothing breaks: a later queued move is rejected with queue_full. A decode check in the codec would make the invariant hold for loaded state too.
5. **The m0_save "path_request within 50 ticks" assertion is near-tautological** (m0_save.rs:117–127). It issues moves and then checks a request exists. Also assert a `found` path_result, or that a unit's position changed.
6. **`queue_respects_budget_and_order` sorts results per tick** (pathing.rs:441). It proves the tick partition is ascending but not the pop order within a tick. This is bounded by the journal's (phase, kind, sim_id) ordering. Acceptable; a comment would help.
7. **Follow order (minimal, Task 9 carry).** After a Follow path reaches `Done` or `Failed`, nothing re-requests it (orders.rs:202–215), so the unit is stuck until the leader despawns. No handler produces Follow yet. Carry to Task 9 explicitly.
8. **A blocked step re-requests every tick with no backoff** (movement.rs:236 → advance_orders). It can livelock with the known corner-cutting case (Task 5 carry → Task 11). Carry with that item.
9. **Queue processing shares ThinkSet::Behaviour with future feature behaviours** (path.rs:678). A Behaviour-set caller of `enqueue_path` would be processed this tick or next depending on ambiguous order. Document "enqueue from Input/Orders only", or order process_path_queue explicitly. Carry to Tasks 9/10.
10. **Report accuracy.** The alloc baselines are swapped for scale_500 and scale_3000, and the `.new` timing (pre-commit) is not mentioned.

## Recommended rulings
- **(a) scenario.rs: accept the out-of-scope fix.** It closes a Task 7 R53 gap and is required for live/load parity. Load does build the graphs (Derived: rebuild_core → rebuild_nav, nav/mod.rs:81–95). Setup should route through one shared install helper (Minor 1), in this fix round, before Task 10 extends rebuild_nav.
- **(b) Allocations: fix now; do not raise budgets.** Use `&mut QueryState` params and `Local` scratch Vecs (Important 1), then re-record budgets.ron. Only the residual from inherent path corners and requests may raise alloc budgets.
- **(c) R97: accept the fixture alignment.** The test comment states the candidate set (start cluster plus the best node's cluster), and the assertion compares against the whole-map nearest reachable cell. This satisfies "aligns test wording".
- **(d) Gate tracks: accept sim-core + perf.** The brief already names this set. The coordinator's dispatch list omitted perf; future dispatches that touch `docs/perf/budgets.ron` must include the perf track.
- **Test-guard:** accept all 12 findings.

## Bless per `.new`
- `system_order__sim_tick.snap.new`: **bless.** It matches the brief's slots.
- `formats__save_codecs.snap.new`: **bless.** It shows `core.move_order@2 reads 1`, `core.path_follow@1` and `core.path_queue@1`, as briefed.
- `xtask/snapshots/api/fl_sim.txt.new`: **bless.** The Important fix touches only `pub(crate)` items.
- `testbed_wander.replay.new`: **bless after verifying it at the fix-round HEAD** (`fl_cli replay-verify`). It was recorded before 2782dc9 was committed. The tick-0 divergence is explained; the alloc fix does not change outcomes.
- `perf_counters__scale_{500,3000,5000}.snap.new`: **bless after the fix-round gate re-runs them.** The totals are plausible, and the alloc fix does not touch counters.
- `docs/perf/budgets.ron.new`: **do not bless.** Re-record after Important 1. The `path.*` entries are fine; the alloc.count and alloc.bytes increases are not accepted as-is.

## Assessment
**Task quality:** Needs fixes

**Reasoning:** The core sim is deterministic and save/load-safe. Requests are FIFO in SimId order, charged cold, stale requests are exact, all queue and path state is Hashed with R78 fills, and the v1→v2 migration is correct and guarded. The one blocking item is a cheap, avoidable +40–55% per-tick allocation regression that would otherwise be baked into the protected budgets file.
