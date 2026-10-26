# FLOW.GROUP — Group moves on shared flow fields
status: approved (lead, R148) · spec-version: 1 · crate: fl_sim::nav::flow · milestone: M1
Depends on: PATH.QUEUE, MOVE.SPEED, ORDER.QUEUE · Emits journal records: core.path_request, core.path_result

## Intent

When a player orders a large group to one place, the units share one flow field toward the destination instead of
each planning its own path: one queued request, one budget charge, and every member steers down the same field to
its own formation slot. The fields are cached between ticks, but the cache is outcome-neutral: what it holds, and
when it evicts, never changes any unit's state, any charge or the tick a request completes on. This spec describes
M1 as built (spec §5.7, ADR-0015, rulings R84, R86, R109–R112, R124; `crates/fl_sim/src/nav/flow.rs`,
`crates/fl_sim/src/path.rs`, `crates/fl_sim/src/commands.rs`, `crates/fl_sim/src/movement.rs`).

## Rules

- FLOW.GROUP.1  When a non-queued `core.move` gives a move to at least `Path.flow_min_group` units of one movement profile, the sim shall give that set one group request toward the command's target, each member keeping its own formation slot as its goal.
- FLOW.GROUP.2  When a non-queued `core.move` gives a move to fewer than `Path.flow_min_group` units of a profile, or to a unit with no profile, the sim shall give each such unit a single request.
- FLOW.GROUP.3  When a `core.move` applies with `queue`, the sim shall make no group request (a unit whose queued move becomes current asks alone).
- FLOW.GROUP.4  When one command makes group and single requests, the sim shall enqueue its group requests first, in `MoveProfile` order, then its single requests in `SimId` order.
- FLOW.GROUP.5  When a group request is processed, the sim shall key its field by the cluster holding the request's goal and the profile of its first still-waiting member, charge the request the field's cold cost (the abstract nodes its integration settles, whether or not the cache held it), and put every waiting member whose cell has a direction on the field (`Flow`, its own slot as goal, `planned` this tick), in `SimId` order; the result is journaled `flow`.
- FLOW.GROUP.6  If a waiting member's cell has no direction on the field (no route from it), then the sim shall give that member a single request at the back of the queue.
- FLOW.GROUP.7  If no member of a group request still exists and waits on it, then the sim shall discard the request at charge 0 as `stale`; otherwise the members that left are skipped.
- FLOW.GROUP.8  If a group request's profile has no HPA* graph (or there is no navigation grid), then the sim shall journal its result as `single` at charge 0 and give every waiting member a single request.
- FLOW.GROUP.9  While a unit is on a flow field outside the destination cluster, the sim shall drive it toward the centre of the farthest of the next four cells along the field's direction chain that it can see.
- FLOW.GROUP.10  While a unit is on a flow field, the sim shall not take a step or push that ends in another cell without a direction.
- FLOW.GROUP.11  When a unit on a flow field stands in the destination cluster and sees its slot, the sim shall switch it to a straight single-corner path to the slot.
- FLOW.GROUP.12  When a unit on a flow field stands in the destination cluster but does not see its slot, the sim shall set its path `Idle`, so its order plans a single path.
- FLOW.GROUP.13  The sim shall make every flow-field read answer from the navigation data alone, building any missing integration or tile before it answers, so the cache's contents, its eviction and `RunConfig::flow_cache_cold` change only the observe-only `flowfield.*` counters, never state, charges or the tick a request completes on.
- FLOW.GROUP.14  When a tick starts (and never during a read), the sim shall evict cached fields in least-recently-used order (smallest `(last read tick, FlowKey)`) while the cache holds more than `Path.flow_cache_max` and the field was not read in the previous tick, then while it holds more than `Path.flow_cache_hard_max` whatever their last read.
- FLOW.GROUP.15  When a terrain change rebuilds a cluster of a profile's HPA* graph, the sim shall drop that profile's cached fields while units on them stay on the field, whose next read rebuilds it from the new terrain.

## Formulas

```text
grouping (core.move, not queued):  by_profile[p] = members of profile p, in SimId order
                                   group if |by_profile[p]| ≥ Path.flow_min_group, else single requests
key                                = (cluster_of(cell_of(request.goal)), profile of the first waiting member)
integration                        = Dijkstra over the profile's HPA* graph from every node of the destination
                                     cluster at cost 0; pops the smallest (cost, node index); a cost changes only
                                     on a strictly lower value
cold cost (group charge)           = number of abstract nodes the integration settles   (u32)
tile (one cluster, built lazily)   = dijkstra_seeded inside the cluster, each entrance node e seeded with
                                     min over edges e → q (q in another cluster) of cost(q) + edge cost
                                     (ties to the lower node index); direction = the cell it was reached from,
                                     or for a seed nothing cheaper reached, its exit q
direction(cell)                    = Arrived in the destination cluster; Toward(next cell) on a tile; None if no
                                     route from the cell
look-ahead                         = from Toward(first), follow the chain up to 4 cells (first included) while the
                                     unit's position sees each cell's centre; drive to the farthest seen
eviction at tick start             = sort entries by (last read tick, key); drop while len > flow_cache_max and
                                     last read < previous tick; then drop while len > flow_cache_hard_max
```

Following directions lowers the cost to the destination by at least one road step per cell, so a unit on a field
never revisits a cell and ends in the destination cluster; a cell has a direction exactly when a full-map Dijkstra
from the destination cluster's passable cells reaches it.

## Parameters

| Name | Path key | Unit | Default | Valid range | Provenance | Confidence |
|---|---|---|---|---|---|---|
| flow_min_group | `rules/Path.flow_min_group` | units | 6 | ≥ 1 | Guess | Low |
| flow_cache_max | `rules/Path.flow_cache_max` | fields | 32 | ≥ 1 | Guess | Low |
| flow_cache_hard_max | `rules/Path.flow_cache_hard_max` | fields | 128 | ≥ `flow_cache_max` | Guess | Low |
| cluster_cells (owned by PATH.QUEUE) | `rules/Path.cluster_cells` | cells | 16 | ≥ 1 | Guess | Medium |

`cluster_cells` is owned by PATH.QUEUE (guess G-PATH-1); it sets the destination of a field.

## Acceptance criteria

- AC-1 (FLOW.GROUP.1, FLOW.GROUP.5): eight tanks ordered across a river make one `group` request listing all eight, journaled `flow` with a positive charge, and every member is on the field heading for its own slot → crates/fl_sim/tests/flow.rs::group_move_uses_one_flow_request
- AC-2 (FLOW.GROUP.2): five tanks, below the group threshold, make five single requests → crates/fl_sim/tests/flow.rs::small_group_uses_single_requests
- AC-3 (FLOW.GROUP.1, FLOW.GROUP.2, FLOW.GROUP.4): six tanks, six riflemen and one truck make two group requests, riflemen first (profile order), then the truck's single request → crates/fl_sim/tests/flow.rs::mixed_profiles_split
- AC-4 (FLOW.GROUP.3): a Shift-queued move of eight tanks makes eight single requests → crates/fl_sim/tests/flow.rs::queued_group_move_uses_single_requests
- AC-5 (FLOW.GROUP.5): a group request behind a single request that spent the budget waits one tick, then puts all eight members on the field → crates/fl_sim/tests/flow.rs::group_request_waits_for_budget
- AC-6 (FLOW.GROUP.6): a member standing on a cell ringed by water gets a single request at the back of the queue while the other five go on the field → crates/fl_sim/tests/flow.rs::unrouted_member_gets_a_single_request
- AC-7 (FLOW.GROUP.7): a group stopped before its request is processed is journaled `stale` at charge 0 → crates/fl_sim/tests/flow.rs::stale_group_request_is_discarded
- AC-8 (FLOW.GROUP.7): two members stopped before processing are skipped and the other four go on the field → crates/fl_sim/tests/flow.rs::members_that_left_are_skipped
- AC-9 (FLOW.GROUP.8): a group whose profile has no graph is journaled `single` at charge 0, followed by one single request per member → crates/fl_sim/tests/flow.rs::group_without_a_graph_is_journaled_single
- AC-10 (FLOW.GROUP.9): eight tanks on a field bending round a lake never stop to turn once under way → crates/fl_sim/tests/steering.rs::flow_units_look_ahead_along_the_field
- AC-11 (FLOW.GROUP.9): a group following its field across a river by the near bridge ends within 2 m of every slot by tick 900 → crates/fl_sim/tests/flow.rs::flow_units_reach_their_slots
- AC-12 (FLOW.GROUP.10): every unit on a field stands in a cell with a direction every tick → crates/fl_sim/tests/steering.rs::flow_units_stay_on_the_field
- AC-13 (FLOW.GROUP.11): an arriving member that sees its slot is switched to a one-corner path to it → open: no isolated test (AC-11 shows only that members end at their slots)
- AC-14 (FLOW.GROUP.12): an arriving member that cannot see its slot goes `Idle` and asks for a single path → open: no isolated test (no test tells the two arrival branches apart)
- AC-15 (FLOW.GROUP.13): a run with a cold flow cache has the same hash stream as a warm one over 600 ticks, while rebuilding more → crates/fl_sim/tests/flow.rs::cache_does_not_change_outcomes
- AC-16 (FLOW.GROUP.13): sixty fields followed at once, over the cap, give the same hashes, charges and budget spent as a cache with room for all → crates/fl_sim/tests/flow.rs::flow_cache_overflow_does_not_change_outcomes
- AC-17 (FLOW.GROUP.13): on random terrain a cell has a direction exactly when the destination cluster's passable cells reach it, and directions lead there → crates/fl_sim/tests/flow.rs::flow_directions_reach_destination
- AC-18 (FLOW.GROUP.14): entries go only at a tick start, least recently used first, never one read in the previous tick, and an evicted key rebuilds with the same cold cost → crates/fl_sim/tests/flow.rs::flow_cache_evicts_only_at_tick_start
- AC-19 (FLOW.GROUP.14): 36 live keys against a cap of 32 rebuild no integration after the request tick → crates/fl_sim/tests/flow.rs::flow_cache_live_keys_never_thrash
- AC-20 (FLOW.GROUP.14): 200 one-off keys, 25 read per tick, never leave more than the hard max cached at a tick start → crates/fl_sim/tests/flow.rs::flow_cache_hard_max_bounds_memory
- AC-21 (FLOW.GROUP.14): a cache with no cap still holds to its hard max → crates/fl_sim/tests/flow.rs::flow_cache_hard_max_applies_without_a_cap
- AC-22 (FLOW.GROUP.15): a group whose near bridge drowns stays on its field, which rebuilds, and every member crosses by the far bridge to its slot → crates/fl_sim/tests/flow.rs::flow_invalidated_by_terrain_change

## Non-goals · Open questions · Guesses

- Non-goal: regrouping after a load. A unit re-planned after a rebuild-load drives on a single path (R130);
  regrouping onto a shared field is an order mechanic for C7.
- Non-goal: a stall rule. In M1 a unit on a field stays on it while it is blocked; Task 15 (MCR-002) adds the rule
  that a flow unit falls back to a single path after 10 ticks without moving.
- Open question: a cold group request over the budget still runs whole in its tick (at least one request per tick,
  PATH.QUEUE.3); resumable integration is parked with R98 (M6 carry-over).
- Open question: no test isolates the two arrival branches (FLOW.GROUP.11, FLOW.GROUP.12); their acceptance criteria
  are open, so `trace` reports them untraced until the test-author adds tests.
- Open question: per-class formation slot pitch and terrain-aware slots are C7 work (R110); M1 slots are
  ORDER.QUEUE's.
- Guess G-FLOW-1: `rules/Path.flow_min_group` = 6, the fewest same-profile units a move needs to share a flow
  field. Replace by measuring where one field's cold cost drops below the sum of the members' single requests.
- Guess G-FLOW-2: `rules/Path.flow_cache_max` = 32 and `rules/Path.flow_cache_hard_max` = 128 (4 × the cap), the
  fields kept between ticks and the ceiling at a tick start. Replace by measuring field memory and live-key counts
  in large battles on the 2 km map.

## Changelog

- v1 (2026-10-25, M2 prelude Task 4): first version, backfilled from M1 as built; approved by the lead (R148), on the
  M2 prelude ratification list.
