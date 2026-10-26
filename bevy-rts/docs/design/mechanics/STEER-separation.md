# STEER.SEP — Separation steering
status: approved (lead, R148) · spec-version: 1 · crate: fl_sim::steer · milestone: M1
Depends on: MOVE.SPEED · Emits journal records: none

## Intent

Units that stand or drive closer than their hulls allow push each other apart, so crowds spread out instead of
stacking on one point. Moving units steer every tick; idle units steer on a staggered cadence to keep the cost down at
scale. A push is added to the unit's drive step but never carries it into a cell it cannot pass: the step slides along
walls and shores instead. This spec describes M1 as built (spec §7, ADR-0015, rulings R76, R113–R116;
`crates/fl_sim/src/steer.rs`, the step in `crates/fl_sim/src/movement.rs`).

## Rules

- STEER.SEP.1  The sim shall steer, each tick in `ThinkSet::Behaviour` after the path queue, every unit with `Mobility` and a `Footprint` whose path is `Following` or on a flow field, and every other such unit whose `SimId % Steering.idle_every` equals `tick % Steering.idle_every`; units without `Mobility` are neighbours but never steered, and without a spatial grid (no map) no unit is steered.
- STEER.SEP.2  When a steered unit `u` is closer than the pair distance `r = radius(u) + radius(v) + Steering.separation_margin_m` (`radius = max(width, length) / 2`) to another unit `v` with a `Pose` and a `Footprint`, the sim shall add `(u − v) / d × (r − d) × Steering.separation_gain` to `u`'s push, from positions as they were at the start of the tick.
- STEER.SEP.3  If two units are coincident (closer than 1/1024 m), then the sim shall push each `r × Steering.separation_gain` along x, toward +x for the higher `SimId` and −x for the lower.
- STEER.SEP.4  The sim shall cap a unit's push at `Steering.max_push_frac × road speed` per tick it stands for: one tick for a moving unit, `Steering.idle_every` ticks for an idle one (R113).
- STEER.SEP.5  The sim shall keep this tick's pushes as one `(SimId, push)` entry per steered unit, sorted by `SimId`, in a Transient resource emptied at the start of every tick, so steering moves no unit between archetypes (R76).
- STEER.SEP.6  If no candidate move of a unit (its drive step and push combined as in STEER.SEP.7) passes the swept-step check (MOVE.SPEED.7), then the sim shall leave the unit where it is.
- STEER.SEP.7  When a unit moves, the sim shall try its candidate moves in this order and take the first that moves it and passes the swept-step check: the drive step plus the push, the drive step alone, the x part of the whole move, its y part (R114).
- STEER.SEP.8  When a unit that had a drive step or a push stays where it is, the sim shall count it in `move.blocked`.
- STEER.SEP.9  While a moving unit drives past an idle unit within their pair distance, the sim shall push the idle unit aside, its path left `Idle`, while the mover keeps driving to its goal (R115).
- STEER.SEP.10  If a unit's own drive step was blocked (it stayed) and its current plan is at least 10 ticks old, then the sim shall set its path `Idle` with no corners so its order asks for a new one; with a younger plan it keeps the path and tries again next tick (R116).
- STEER.SEP.11  If a `Steering` rules value lies outside its valid range (Parameters), then the sim shall fail content loading with an error naming the field.

## Formulas

All values are `Fx`; products floor to the raw unit.

```text
radius(u)  = max(footprint.width, footprint.length) / 2
reach(u)   = radius(u) + widest radius present + separation_margin_m        (spatial query radius)
r(u, v)    = radius(u) + radius(v) + separation_margin_m
for each neighbour v of u in SimId order, with |u − v|² < r²:
    d = |u − v|
    part = (u − v) × ((r − d) × separation_gain / d)                       if d ≥ 1/1024 m
         = (+r × separation_gain, 0) if SimId(u) > SimId(v), else (−r × separation_gain, 0)   otherwise
push       = Σ part
cap        = (max_push_frac × speed_per_tick).mul_int(moving ? 1 : idle_every)
push       ← normalize(push) × cap      if |push| > cap
idle cadence: an idle unit is steered when SimId % idle_every == tick % idle_every

step candidates (movement): whole = step + push; then step; then (whole.x, 0); then (0, whole.y);
    each map-clamped, skipped if it does not move the unit or repeats an earlier one; the first that passes the
    swept-step check (and, on a flow field, ends in a cell with a direction) is taken; else stay
blocked re-plan: stayed ∧ step ≠ 0 ∧ tick − planned ≥ BLOCKED_REPLAN_TICKS (10)  ⇒  path Idle
```

Each unit computes its own terms from its own side, so a pair's two terms are opposite only to within one raw unit
(`(−x)·s` and `−(x·s)` may differ by one ulp): deterministic, not exactly symmetric.

## Parameters

| Name | Path key | Unit | Default | Valid range | Provenance | Confidence |
|---|---|---|---|---|---|---|
| separation_gain | `rules/Steering.separation_gain` | fraction of overlap per tick | 0.5 | [0, 4] | Guess | Low |
| separation_margin_m | `rules/Steering.separation_margin_m` | m | 0.5 | [0, 50] | Guess | Low |
| idle_every | `rules/Steering.idle_every` | ticks | 5 | 1..=64 | Guess | Low |
| max_push_frac | `rules/Steering.max_push_frac` | fraction of road speed | 0.5 | (0, 1] | Guess | Low |
| footprint | `unit:*/components.Hull.footprint.*` | m | per unit (sides 0.8–6.8) | (0, 50] for each entry; one per side | OursScaled | Medium |

`BLOCKED_REPLAN_TICKS` (10) is a code constant in `fl_sim::movement`, not content. The road speed in the cap is the
unit's `Mobility.road_speed_kmh` (MOVE.SPEED).

## Acceptance criteria

- AC-1 (STEER.SEP.1): 100 idle tanks closer than their pair distance are steered at most ⌈100 / idle_every⌉ per tick, each once per `idle_every` ticks → crates/fl_sim/tests/steering.rs::idle_steering_is_staggered
- AC-2 (STEER.SEP.1, STEER.SEP.5): the pushes ascend by `SimId`, match `steer.units`, never move a unit between archetypes, and are gone in a tick whose stagger matches no idle unit → crates/fl_sim/tests/steering.rs::steer_pushes_are_per_tick_and_sorted
- AC-3 (STEER.SEP.2, STEER.SEP.3): two coincident idle tanks end at least their pair distance (less 0.1 m) apart, the lower id to −x and the higher to +x → crates/fl_sim/tests/steering.rs::overlapping_idle_units_separate
- AC-4 (STEER.SEP.4): a push larger than the cap is cut to `max_push_frac × road speed` per tick it stands for → open: no isolated test (no test measures a push against the cap)
- AC-5 (STEER.SEP.2, STEER.SEP.6): ten tanks on a one-cell grass strip between water spread along the strip and never stand on or sweep a water cell → crates/fl_sim/tests/steering.rs::separation_never_pushes_into_water
- AC-6 (STEER.SEP.6): a crowd ordered through itself past a lake never enters or sweeps a blocked cell → crates/fl_sim/tests/steering.rs::crowd_never_enters_blocked_cells
- AC-7 (STEER.SEP.7): a move whose whole candidate is blocked takes the unpushed step, then the x part, then the y part, in that order → open: no isolated test (AC-5 and AC-6 show only that no taken move enters a blocked cell)
- AC-8 (STEER.SEP.8): a tank driven at water stops at the shore and its blocked step is counted once in `move.blocked` → crates/fl_sim/tests/terrain.rs::units_stop_at_blocked_cells
- AC-9 (STEER.SEP.9): an idle tank 1.5 m off a mover's line is pushed aside and stays idle, and the mover reaches its goal → crates/fl_sim/tests/steering.rs::mover_pushes_an_idle_unit_aside
- AC-10 (STEER.SEP.10): a tank stranded in water by a terrain change is blocked over 50 times in 110 ticks but asks for at most one path per 10 ticks → crates/fl_sim/tests/steering.rs::blocked_step_replans_with_backoff
- AC-11 (STEER.SEP.10): a forced path blocked by a one-cell wall is `Idle` with no corners by tick 20 → crates/fl_sim/tests/steering.rs::fast_unit_never_hops_a_one_cell_wall
- AC-12 (STEER.SEP.11): every out-of-range `Steering` value is a content error naming its field → crates/fl_sim/tests/terrain.rs::steering_rules_ranges_are_validated
- AC-13 (STEER.SEP.11): the base `Steering` values are 0.5, 0.5 m, 5 and 0.5 → crates/fl_sim/tests/steering.rs::base_steering_rules_are_the_brief_values

## Non-goals · Open questions · Guesses

- Non-goal: yielding by choice. An idle unit is pushed aside by a mover's separation rather than stepping out of the
  way itself (R115); yielding is C7's.
- Non-goal: smoothing of the idle push cap over the stagger period (R113, C7).
- Open question: no test isolates the order of the step candidates (STEER.SEP.7) or measures the push cap
  (STEER.SEP.4); their acceptance criteria (AC-7, AC-4) are open, so `trace` reports both rules untraced until the
  test-author adds tests.
- Open question: a unit sliding under a steady push without nearing its corner never re-plans (the backoff fires
  only when it stays put); parked for C7 crowd tuning. A head-on jam is C7's too (R127).
- Open question: a stranded unit re-plans every 10 ticks without escaping (R116); an unsticking rule is due before
  destructible bridges (M7).
- Open question: non-mobile footprints widen every query's reach (`widest radius present`); parked until buildings
  (M4).
- Guess G-STEER-1: `rules/Steering.separation_gain` = 0.5, the fraction of a pair's overlap each unit pushes per
  steered tick. Replace by tuning crowd spreading time on the testbed.
- Guess G-STEER-2: `rules/Steering.separation_margin_m` = 0.5 m, the gap kept between footprints (also part of the
  formation slot pitch, ORDER.QUEUE.17). Replace by tuning crowd density against path blocking.
- Guess G-STEER-3: `rules/Steering.idle_every` = 5, the stagger period of idle steering in ticks. Replace by
  measuring steering cost at 3,000 units against how long idle overlaps take to clear.
- Guess G-STEER-4: `rules/Steering.max_push_frac` = 0.5, the largest push per tick as a fraction of road speed.
  Replace by tuning how fast a mover clears idle units from its way.

## Changelog

- v1 (2026-10-25, M2 prelude Task 4): first version, backfilled from M1 as built; approved by the lead (R148), on the
  M2 prelude ratification list.
