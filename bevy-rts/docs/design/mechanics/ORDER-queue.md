# ORDER.QUEUE — Move, rotate, follow and stop orders
status: approved (lead, R148) · spec-version: 1 · crate: fl_sim::orders · milestone: M1
Depends on: PATH.QUEUE, MOVE.SPEED, STEER.SEP · Emits journal records: core.order_changed

## Intent

A player orders units to move, to turn and face a point, to follow another unit, or to stop. An order either
replaces what a unit was doing or, Shift-queued, waits behind its other orders, up to 32 per unit. A group move keeps
the group's shape, scaled into a spread limit and spaced so no two hulls are sent to the same spot. This spec
describes M1 as built (spec §7, rulings R69, R77, R104–R107, R110; `crates/fl_sim/src/{orders,commands}.rs`): the
closed `Order` enum held in `MoveOrder`, which the order queue and activity stack of ADR-0018 replace in Task 13
(MCR-001) with these rules kept.

## Rules

- ORDER.QUEUE.1  When a `core.move`, `core.rotate` or `core.follow` command applies without `queue`, the sim shall make that order the only order (the current one) of each of the command's units.
- ORDER.QUEUE.2  When a `core.move`, `core.rotate` or `core.follow` command applies with `queue`, the sim shall append the order to each of the command's units' orders.
- ORDER.QUEUE.3  When a `core.move`, `core.rotate` or `core.follow` command changes a unit's orders, the sim shall journal `core.order_changed` with reason `command` for that unit.
- ORDER.QUEUE.4  The sim shall clamp every wire-supplied point of an order command to the map before any arithmetic on it (R104).
- ORDER.QUEUE.5  If a command with `queue` names a unit that already holds `MAX_WAYPOINTS` (32) orders of any kind, then the sim shall reject the whole command with code `core.queue_full` and change no unit's orders or path; a command without `queue` replaces a full queue.
- ORDER.QUEUE.6  When a `core.stop` command applies, queued or not, the sim shall drop every order of each of its units, set the unit's path `Idle` with no corners (so a queued request turns stale) and journal `core.order_changed` with reason `stopped`.
- ORDER.QUEUE.7  The sim shall advance orders first in `ThinkSet::Orders`, unit by unit in `SimId` order: a current move whose path is `Done` pops (reason `completed`), one whose path `Failed` pops (reason `failed`), one whose path is `Idle` asks for a path; after a pop the next order becomes current in the same tick, and a unit with no orders left has an ended path set back to `Idle`.
- ORDER.QUEUE.8  When a `core.rotate` toward point `t` applies, the sim shall fix the unit's facing as the direction from its start point to `t` (with `queue`, the target of the last move already in its orders, else its position; its current heading when the start point is `t`); while the rotate is current the unit turns in place by at most its turn rate per tick, and the order pops (`completed`) once its heading equals the facing, at once for a unit without a `Pose`.
- ORDER.QUEUE.9  When a `core.follow` of a leader applies, the sim shall give each follower the offset `pos − leader.pos` shortened to at most `Movement.follow_distance_m` (direction kept), or exactly `follow_distance_m` behind the leader's heading for a follower on the leader's spot.
- ORDER.QUEUE.10  If a `core.follow` names a leader that does not exist or has no `Pose`, then the sim shall reject the command with code `core.invalid_target`.
- ORDER.QUEUE.11  The sim shall drop a unit from the units of a `core.follow` that names it as the leader, so a unit told to follow itself alone is rejected with `core.no_valid_units`.
- ORDER.QUEUE.12  When a follow order is current and the follower's path is `Idle`, the sim shall ask for a path to the goal `clamp(leader.pos + offset)` in that tick.
- ORDER.QUEUE.13  While a follower's request waits, or fewer than `Movement.follow_repath_ticks` have passed since its current plan was applied, the sim shall not ask for another path for it.
- ORDER.QUEUE.14  When `Movement.follow_repath_ticks` have passed since a follower's plan was applied, the sim shall ask for a new path if the goal drifted more than one map cell from the planned one while it drives, more than `max(Movement.arrive_m, cell / 2)` when its path is `Done`, and always when its path `Failed`.
- ORDER.QUEUE.15  The sim shall complete a follow order (reason `completed`) only when its leader no longer exists or has no `Pose`; while the leader lives the follow stays current, and orders queued behind it wait (R107).
- ORDER.QUEUE.16  When a `core.move` names more than one unit, the sim shall give each unit the slot `clamp(target + offset)`, its offset from the units' centroid (positions when the command applies) scaled by `Movement.formation_max_spread_m / R` when the farthest offset `R` exceeds that spread.
- ORDER.QUEUE.17  The sim shall then space the slots, in unit order, at least the pitch apart (the largest footprint side among the units plus `Steering.separation_margin_m`): a slot closer than the pitch to an earlier one moves to the first free point of the pitch lattice around it, ring by ring, and the spacing wins over the spread limit (R110).

## Formulas

```text
order targets      target = map.clamp(payload point)                               (R104, before any arithmetic)
formation (n ≥ 2)  c = Σ pos_i / n;  offset_i = pos_i − c;  R = max |offset_i|
                   offset_i ← offset_i × spread / R   if R > spread   (spread = Movement.formation_max_spread_m)
                   slot_i = map.clamp(target + offset_i)
slot spacing       pitch = max over units of max(footprint.width, footprint.length) + Steering.separation_margin_m
                           (0 without footprints: no spacing; capped at 4096 m)
                   for each slot in unit order: kept if |slot − p| ≥ pitch for every placed p (squared, exact Fx);
                   else the first candidate slot + (dx, dy) × pitch, map-clamped, over Chebyshev rings r = 1, 2, …
                   (within a ring by (dx² + dy², dy, dx)) that is ≥ pitch from every placed slot;
                   none within ⌊√n⌋ + 1 rings: the slot is kept
rotate facing      from = last queued move's target (with queue) | pos | t;  facing = angle(t − from), or the
                   heading when t = from
follow offset      o = pos − leader.pos;  o ← o × d / |o| if |o| > d;  o = −unit(leader.heading) × d if |o| = 0
                   (d = Movement.follow_distance_m)
follow goal        g = map.clamp(leader.pos + o)
follow re-plan     due = (tick − planned ≥ Movement.follow_repath_ticks) ∧
                         (Following/Flow: |g − goal| > cell;  Done: |g − goal| > max(arrive_m, cell / 2);  Failed)
```

## Parameters

| Name | Path key | Unit | Default | Valid range | Provenance | Confidence |
|---|---|---|---|---|---|---|
| formation_max_spread_m | `rules/Movement.formation_max_spread_m` | m | 40 | [0, 4096] | Guess | Low |
| follow_distance_m | `rules/Movement.follow_distance_m` | m | 8 | [0, 4096] | Guess | Low |
| follow_repath_ticks | `rules/Movement.follow_repath_ticks` | ticks | 20 | ≥ 1 | Guess | Low |
| arrive_m (owned by MOVE.SPEED) | `rules/Movement.arrive_m` | m | 0.5 | > 0 | Guess | Low |
| separation_margin_m (owned by STEER.SEP) | `rules/Steering.separation_margin_m` | m | 0.5 | [0, 50] | Guess | Low |

`MAX_WAYPOINTS` (32) is a code constant, not content (R77): it bounds the save decoder. `arrive_m` is owned by MOVE.SPEED
(G-MOVE-5) and `separation_margin_m` by STEER.SEP (G-STEER-2).

## Acceptance criteria

- AC-1 (ORDER.QUEUE.1, ORDER.QUEUE.2): a queued move appends to the orders and an unqueued move replaces them all → crates/fl_sim/tests/commands.rs::queued_moves_append
- AC-2 (ORDER.QUEUE.2, ORDER.QUEUE.3, ORDER.QUEUE.7, ORDER.QUEUE.8): a move, a queued rotate and a queued move run in order (A, then facing the point from A, then B), journaled `command` three times then `completed` three times → crates/fl_sim/tests/orders.rs::shift_queue_mixed_orders
- AC-3 (ORDER.QUEUE.4): extreme wire coordinates in move and rotate commands, queued or not, never panic, and a rotate faces the clamped point → crates/fl_sim/tests/orders.rs::extreme_payload_values_never_panic
- AC-4 (ORDER.QUEUE.4): a move to a point outside the map ends at the clamped point → crates/fl_sim/tests/movement.rs::move_target_outside_map_is_clamped
- AC-5 (ORDER.QUEUE.5): 32 queued moves fit, a 33rd is `core.queue_full` and changes nothing, and an unqueued move replaces the full queue → crates/fl_sim/tests/commands.rs::queued_waypoints_are_capped
- AC-6 (ORDER.QUEUE.5): the cap counts moves, rotates and follows alike, a queued rotate or follow naming a full unit is rejected for every unit it names, and an unqueued rotate replaces the queue → crates/fl_sim/tests/orders.rs::queued_orders_share_the_cap
- AC-7 (ORDER.QUEUE.6): a stop with `queue` set still drops every order at once → crates/fl_sim/tests/orders.rs::stop_with_queue_is_never_queued
- AC-8 (ORDER.QUEUE.6): a stop while the unit's request waits clears its orders, leaves the request stale and the unit in place, journaled `stopped` → crates/fl_sim/tests/pathing.rs::stop_clears_orders_and_pending_request
- AC-9 (ORDER.QUEUE.7): two queued moves are driven in order, A before B, each popping `completed` → crates/fl_sim/tests/pathing.rs::queued_moves_run_in_order
- AC-10 (ORDER.QUEUE.8): a rotate turns the unit in place, at most its turn rate per tick, and pops once it faces the point → crates/fl_sim/tests/orders.rs::rotate_turns_in_place
- AC-11 (ORDER.QUEUE.8): a rotate queued after a move faces the point as seen from the move's target, not from the start (R105) → crates/fl_sim/tests/orders.rs::rotate_queued_after_move
- AC-12 (ORDER.QUEUE.8): a rotate given to a unit without a `Pose` applies and completes at once → crates/fl_sim/tests/orders.rs::rotate_without_pose_applies_and_completes
- AC-13 (ORDER.QUEUE.9, ORDER.QUEUE.14, ORDER.QUEUE.15): a follower 12 m from its leader gets an 8 m offset, stays within reach while the leader drives 200 m (re-planning on the way, at most once per `follow_repath_ticks`), and keeps its follow after the leader arrives → crates/fl_sim/tests/orders.rs::follow_tracks_moving_leader
- AC-14 (ORDER.QUEUE.9): a follower on its leader's spot keeps `follow_distance_m` behind the leader's heading → crates/fl_sim/tests/orders.rs::follower_on_its_leader_keeps_behind_it
- AC-15 (ORDER.QUEUE.10, ORDER.QUEUE.11): following itself (`core.no_valid_units`) or a missing leader (`core.invalid_target`) is rejected and changes nothing → crates/fl_sim/tests/orders.rs::follow_self_or_missing_leader_rejected
- AC-16 (ORDER.QUEUE.12): a follow becoming current with an `Idle` path asks for its first path in that same tick → open: no isolated test (the follow tests count requests over many ticks)
- AC-17 (ORDER.QUEUE.13, ORDER.QUEUE.14): a follower at its goal asks for nothing; a failed path is not retried before `follow_repath_ticks` and is retried on it; a stopped follower re-plans once the leader is more than half a cell away → crates/fl_sim/tests/orders.rs::follow_replans_ended_paths_on_a_cadence
- AC-18 (ORDER.QUEUE.14): a stopped follower ignores leader nudges smaller than half a cell → crates/fl_sim/tests/steering.rs::stopped_follower_ignores_small_leader_nudges
- AC-19 (ORDER.QUEUE.15): when the leader despawns the follow pops `completed` and the follower stops → crates/fl_sim/tests/orders.rs::follow_ends_when_leader_is_gone
- AC-20 (ORDER.QUEUE.16): formation slots are pinned for a 200 m spread scaled to 40 m and clamped onto the map → crates/fl_sim/tests/orders.rs::formation_targets_pinned
- AC-21 (ORDER.QUEUE.16): a group within the spread keeps its relative offsets at the target → crates/fl_sim/tests/orders.rs::formation_keeps_relative_offsets
- AC-22 (ORDER.QUEUE.16): a group wider than the spread is scaled into it, keeping each pair's direction → crates/fl_sim/tests/orders.rs::formation_spread_is_capped
- AC-23 (ORDER.QUEUE.17): a slot closer than the pitch moves to the first free lattice point (south first), slots already apart stay, and a zero pitch changes nothing → crates/fl_sim/tests/flow.rs::space_slots_pinned
- AC-24 (ORDER.QUEUE.17): two clumps of 128 a kilometre apart, scaled into a 40 m spread, still get slots a pitch apart and within about √n × pitch of the target → crates/fl_sim/tests/flow.rs::spread_group_slots_keep_minimum_spacing
- AC-25 (ORDER.QUEUE.17): 64 tanks packed 2 m apart get move slots at least the pitch (6 m hull side plus 0.5 m margin) apart → crates/fl_sim/tests/flow.rs::move_command_spaces_formation_slots

## Non-goals · Open questions · Guesses

- Non-goal: attack, guard, patrol and other order kinds; registered order kinds arrive with the order queue (C7).
- Non-goal: terrain-aware formation slots and per-class slot pitch (R110, C7); a slot may lie on a cell its unit
  cannot pass, and the move's plan snaps it (PATH.QUEUE.7).
- Open question: a queued rotate's facing is fixed when the command applies (R105); C7 resolves it when the order
  becomes current instead.
- Open question: no test pins the `failed` pop of ORDER.QUEUE.7 (a current move whose plan is `Unreachable`). A
  candidate for the test-author.
- Open question: no test isolates ORDER.QUEUE.12 (a follower with an `Idle` path planning in that same tick); its
  acceptance criterion is open, so `trace` reports the rule untraced until the test-author adds one.
- Open question: following a leader of another player is allowed in M1; visibility rules for it are M3's (Task 9
  review Minor 8).
- Guess G-ORDER-1: `rules/Movement.formation_max_spread_m` = 40 m, the furthest a formation slot lies from the move's
  target. Replace by tuning group moves on the testbed (C7 formations).
- Guess G-ORDER-2: `rules/Movement.follow_distance_m` = 8 m, the most a follower keeps from its leader. Replace by
  tuning escorts and columns on the testbed.
- Guess G-ORDER-3: `rules/Movement.follow_repath_ticks` = 20 ticks (one second), the shortest interval between a
  follower's re-plans. Replace by measuring path work against how far behind followers fall.

## Changelog

- v1 (2026-10-25, M2 prelude Task 4): first version, backfilled from M1 as built; approved by the lead (R148), on the
  M2 prelude ratification list.
