# Task 10 review: flow fields for group moves

Diff: `review-t10.diff` (4e79841..44aa360, plus the system-order and API `.new` diffs). Read in three passes: sim sources, then tests, then the extra files and `.new`. Read-only. Nothing was built or run.

Focused checks outside the diff, one per named risk:
- **Grouping depends on selection order?** `commands.rs:529-530` sorts and dedups `ctx.units` by `SimId` before any handler runs, so group membership, slot order and `space_slots` order do not depend on selection or entity order.
- **Flow units mishandled by orders?** `orders.rs:153-160` and `:233-241` treat `Flow` under a current `Move` as in progress. `Idle` re-requests a single path. `follow_needs_path` (`:294-296`) lists `Flow`.
- **Tile costs disagree with the integration?** `hpa.rs:13-15` and `:340-348` give one node per cell (`dedup`) and intra edges from an in-cluster `dijkstra_from`. So a tile's costs equal the integration's at its nodes, and the exit `binary_search` keys are unique.
- **Group requests in the hash?** `path.rs:185-186` hashes `RequestKind::Group`.

### Spec Compliance

- ✅ **Spec compliant.**
  - `FlowCache`: `flow.rs:933-943` defines it. `nav/mod.rs:1509` registers it as Derived `"core.path_flow_cache"`. It is reset in `install_nav` and in `rebuild_nav`'s no-terrain branch (`mod.rs:1363,1387`). Its maps are `BTreeMap<FlowKey,_>` and `BTreeMap<ClusterId,_>`.
  - The API matches the brief: `cold_cost`, `ensure_tile`, `direction`, `clear_profile`, `len` (`flow.rs:1104-1203`). `FlowDir` and `FlowWork` are as specified.
  - `RunConfig.flow_cache_cold` is at `build.rs:56,102-103`.
  - `core.move` grouping: groups per profile when there are at least `flow_min_group` units, groups go first in `MoveProfile` order, then singles in `SimId` order (`commands.rs:222-252`). Queued moves still make single requests (`:216-221`).
  - `process_group`:
    - the charge is `cold_cost`;
    - waiting members are taken in `SimId` order and set to `Flow{key}` with their slot as goal;
    - members with no route get single requests at the back of the queue;
    - the result is journaled as `"flow"`;
    - a group with no waiting member is `"stale"` with charge 0 (`path.rs:2004-2016,2058-2135`).
  - `prepare_flow_tiles` runs in the `ActSet::Move` core slot, chained before `move_system`, in `SimId` order (`movement.rs:383-417,700`).
  - `move_system` Flow handling (`movement.rs:559-598`): `Toward` steers to the cell centre, and `Arrived` checks `line_clear` and then goes to `Following [goal]` or `Idle`.
  - The counters and the `flowfield_build` span (kinds `abstract` and `tile`) are present.
  - `on_terrain_changed` calls `clear_profile` (`mod.rs:1463-1471`).
  - All 11 brief tests are present, the proptest at 64 cases on 48×48 maps.
  - Carries:
    - R95: `dijkstra_seeded` and `next_toward_source` (`astar.rs:759-824`).
    - R99: the group result's cost is 0 and documented (`path.rs:1855-1857`).
    - R100 and R101 are honoured.
    - Task 9 Minor 2: `space_slots` (`orders.rs:1625-1674`).
  - R104: no new wire payload. `space_slots` only sees clamped slots and clamps its candidates.
- ⚠️ **Cannot verify from the diff:**
  - The gate after bless. `test-headless` and `api` failed only on the pending `.new` files; the controller should re-run `gate task --track contract,sim-core,perf` after blessing.
  - Whether `assert_branch` saves mid-queue with a pending Group request. Group requests are usually processed in their own tick, so the save round trip of a queued `Group` is probably unexercised (see Minor 6).

**Determinism and R84 checklist:**
- **FlowCache is Derived and never hashed or saved.** It is registered Derived only (`mod.rs:1509`), and its `cold` flag survives a reset (`mod.rs:1392-1402`).
- **Outcomes are the same cold, warm, after load and after eviction.**
  - `direction` and `cold_cost` are pure functions of `NavGrid`/`HpaGraphs`, and a miss builds before answering (`flow.rs:1153-1158,1183`).
  - Eviction never removes the key being built (`flow.rs:1083-1099`).
  - Oracles: `cache_does_not_change_outcomes` (cold vs warm, terrain change included, non-vacuous through `cold_nodes > warm_nodes`), `flow_cache_overflow_does_not_change_outcomes` (4 live keys, max 2, `len() ≤ 2` asserted), and `branch_mid_flow_follow` (after load) on both scenarios.
  - The report's mutation checks show both oracles fail when a miss returns `None`.
- **Charge does not depend on cache state (R84).**
  - A group is charged `entry.settled`, a pure function of `(graph, dest)` (`flow.rs:1119`). Reads that rebuild tiles or integrations are counted only in `flowfield.*`, never in `spent` (`path.rs:2005-2006`).
  - Membership (`on_field`) comes from pure `direction` (`path.rs:2098-2107`).
- **Integration is deterministic and integer-only.**
  - Heap keys: `(cost, node)` in `integrate` and `(g, y, x)` in `dijkstra_seeded`. A cell is pushed again only at a strictly lower cost, so keys are unique per live entry.
  - Exit ties go to the lower node through `.min()` on `(cost, q)` (`flow.rs:994-998`). An equal-cost seed keeps no parent and uses its exit.
  - All arithmetic is `u32` with saturating adds. There are no floats, no hash maps and no `Local`.
- **No-revisit argument holds.**
  - Within a tile the cost falls strictly at each step.
  - Across a border, from seed `v` to exit `q`: `T_D(q) ≤ cost[q] < cost[q] + w = T_C(v)`. This uses exact intra edges, which the `hpa.rs` check confirms.
  - The proptest checks reachability equivalence and no revisits.
- **Invalidation.** Every key of every profile is dropped when any cluster rebuilds. Integrations span the whole graph, so per-key granularity is exact. Dropping every profile follows R100. Flow units keep `Flow`, and their next read rebuilds the field. `invalidate_paths` skips them correctly: they have no corners (`path.rs:729`).
- **Grouping is deterministic.** Units arrive SimId-sorted (check above), are grouped in a `BTreeMap<MoveProfile,_>` and kept in SimId order; singles are sorted.
- **System-order snapshot.** It adds exactly one line, `Act/Move prepare_flow_tiles` before `move_system` (`.snap.new`).

### Strengths
- **The R84 design is clean.** Purity is enforced structurally: every public read goes through `entry()`/`tile()`, and those build on a miss. The cold mode is a real oracle, emptying the cache lazily at its first use in each tick, and the overflow test asserts that the cache really overflowed.
- **The flow-field soundness argument is written down** in the module docs (`flow.rs:865-869`) and backed by a strong proptest: both directions of reachability equivalence, the step check and no revisits.
- **The `move_system` refactor preserves Following behaviour.** `drive`, `speed_at` and `to_idle` reproduce the old logic statement for statement (the `last && dist > speed && dist <= arrive` condition is equivalent), and the golden replays are unchanged.
- **Edge cases are tested:** stale groups, members that left, unrouted members (island), queued moves, mixed profiles with a truck single request.
- **`prepare_flow_tiles` allocates nothing per tick.** It uses a cached `QueryState` and the Transient `OrderScratch.flow`, consistent with the R102 lesson.

### Issues

#### Critical (Must Fix)
None.

#### Important (Should Fix)
None.

#### Minor (Nice to Have)
1. **Eviction can thrash within a tick** (`flow.rs:1073,1083-1099`). Eviction runs inside `entry()` in the middle of a tick, and every key read this tick has `last_used == now`. So with more live keys than `flow_cache_max` (base 32), cyclic access evicts keys still needed later in the same tick. Each key is then rebuilt, a whole-graph integration, up to twice per tick: once in `prepare_flow_tiles` in SimId order and once in `move_system` in archetype order. None of this is budget-charged (brief-mandated: read builds are never charged), so R84's "bounded by budget accounting" does not hold for read rebuilds. Outcomes are unaffected. Suggested fix: evict only in `begin_tick`, so a key is built at most once per tick, or never evict a key that a live `Flow` unit references. At least measure it in Task 13 (see the rulings).
2. **A cold group request scales with the whole graph** (`flow.rs:952-978`). The integration settles every reachable node wherever the group stands: about 262k nodes on a 1024² map with 16-cell clusters, against `budget_per_tick` 8000. Such a request runs alone (the at-least-one rule) and stalls that tick's CPU. This is implementer concern 4, and it shares the trigger of the R98 parking.
3. **`"flow"` status with nobody on the field** (`path.rs:2128,2009`). With no `HpaGraph` for the profile, or no grid, `process_group` returns `Some(0)`. The journal then records `"flow"` with charge 0 while every member was sent to single requests. This is unreachable today, because `core.move` groups only units that have a grid profile. A distinct status such as `"split"`, or a doc line, would avoid a misleading journal.
4. **`slot_pitch` takes the largest footprint over the whole command** (`commands.rs:260-273`). A mixed tank and rifleman move spaces the riflemen at the tank pitch (6.5 m). This is harmless, but it is worth documenting next to the spacing note.
5. **The proptest does not check the corner rule on diagonal steps** (`flow.rs` test `check_field`, around line 2943). It checks that each step goes to a neighbour and that the next cell has a direction, but not that a diagonal step's side cells are passable, which off-centre units depend on (see ruling 4). The corner rule comes from `dijkstra_seeded`, so the risk is low; a one-line `prop_assert` would pin it.
6. **Coverage gaps:**
   - No test of a group request held over by the budget: a group behind spent budget, waiting a tick, and its save round trip as a pending `Group`.
   - The overflow test's `warm_nodes > 0 && cold_nodes > 0` is weak. `len() ≤ 2` with 4 keys does prove overflow, but `warm_nodes > 4 × one-integration` would show that reads actually rebuilt.
7. **Process: TDD order.** The report admits the implementation was written before the tests, with RED obtained afterwards by stashing. The evidence shows the tests depend on the code, not test-first. The mutation checks partly compensate.
8. **Gate test-guard WARN** at `commands.rs:705` (`let margin = world…separation_margin_m`). This is sim code in `slot_pitch`, not a test tolerance, so it is a false positive. The reviewer accepts it.

### Assessment

**Task quality:** Approved

**Reasoning:** The cache is structurally outcome-neutral: reads are pure and rebuild on a miss, the charge is the pure settled count, and the cold, overflow and branch oracles hold. The integration and tiles are deterministic, integer-only and provably free of revisits. The remaining items are a perf hazard that R84 already accepted plus coverage and documentation nits.

### Recommended rulings
- **Out-of-list edits: accept (R108 precedent).**
  - `tests/pathing.rs` raises `flow_min_group` to 64, so the Task 8 queue tests keep testing single requests; their assertions are unchanged.
  - The `fl_test_utils` builder gains `flow_cache_cold`, which the brief's oracle tests need, and `reload` keeps it.
- **test-guard `commands.rs:705`: accept as a false positive.** Park: test-guard's `ToleranceChanged` should ignore non-test sources.
- **`space_slots` spreads past `formation_max_spread_m`: accept.**
  - Amend spec §7 and the plan: `formation_max_spread_m` caps the formation before spacing, and spacing wins, so a dense group spans about √n × pitch.
  - Carry to C7: terrain-aware slots, since a spaced slot may land in water and the unit then needs one snapped single request, and per-class pitch for mixed groups (Minor 4).
- **Cell-centre steering, with tanks turning in place at 45° bends: accept until Task 11.**
  - Carry to Task 11: look ahead along the `direction()` chain with line of sight and steer to the furthest clear cell. It stays outcome-neutral because `direction` is pure. Count its line-of-sight cells as movement work, not path budget.
  - Note for Task 11: an off-centre unit crosses a diagonal's side cells, which the corner rule keeps passable. Separation pushes must keep that guarantee, or `Blocked` drops units off the field.
- **"Arrived" means inside the destination cluster, plus one extra single request when the slot is out of line of sight or in a neighbouring cluster: accept.** It is bounded at one single request per unit and deterministic. Revisit with Task 11/C7 if group arrivals look staggered.
- **A cold group request exceeding `budget_per_tick` on big maps: accept under R98.** Fold it into R98's trigger (a map over 1024 cells per side, or Task 13 profiling). Task 13 adds a group-move perf scenario that budgets `flowfield.abstract_nodes` and `path.budget_spent`.
- **New: the R84 cost wording needs an amendment.** Read rebuilds are never charged, so a thrash with more live keys than `flow_cache_max` costs up to two integrations per key per tick, with no budget bound (Minor 1). Task 13 should measure a scenario with more than 32 live keys. If it shows up, evict only at the tick boundary or pin keys that live `Flow` units reference. Either change is outcome-neutral.

### Bless per `.new`
- `crates/fl_test_utils/tests/snapshots/system_order__sim_tick.snap.new`: **bless.** The only change is `Act/Move fl_sim::movement::prepare_flow_tiles` before `move_system`.
- `xtask/snapshots/api/fl_sim.txt.new`: **bless.** It only adds items:
  - `nav::flow::{FlowCache, FlowDir, FlowWork}` and their `nav` re-exports;
  - `dijkstra_seeded` and `DistanceField::next_toward_source`;
  - `RunConfig::flow_cache_cold`;
  - `space_slots`.

  Nothing was removed or changed.
- Reason: `M1 Task 10: flow fields`.
