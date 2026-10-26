# MOVE.SPEED — Terrain speed, blocking and the drive step
status: approved (lead, R148) · spec-version: 1 · crate: fl_sim::movement · milestone: M1
Depends on: none · Emits journal records: none

## Intent

A unit drives at its road speed scaled by the terrain under it, never enters a cell its movement class or size cannot
pass, and turns its hull toward where it is going before it sets off. This spec covers how fast a unit moves on a
cell, which cells block which units (terrain kind, slope, clearance for wide hulls), the turn-then-drive rule, the
swept-step passability check and arrival at a path's corners. It describes M1 as built (spec §7, ADR-0014,
`crates/fl_sim/src/{movement,units}.rs`, `crates/fl_sim/src/nav/{grid,los}.rs`); path planning is PATH.QUEUE, the
separation push added to the step is STEER.SEP.

## Rules

- MOVE.SPEED.1  When a unit with `Mobility` spawns, the sim shall convert its `road_speed_kmh` to metres per tick as `road_speed_kmh × 10 / (36 × SIM_TICK_HZ)` and its `turn_rate_deg_s` to the angle `turn_rate_deg_s / SIM_TICK_HZ` per tick, rounded to the nearest angle unit.
- MOVE.SPEED.2  While a unit drives, the sim shall make its step length per tick its road speed times `clamp(pass + (1 − pass) × terrain_factor, Movement.min_speed_frac, 1)`, with `pass` that of the cell the unit stands in (open ground and cells off the map pass 1).
- MOVE.SPEED.3  The sim shall treat a cell as blocked for a movement class when the cell's terrain kind lists that class in its `blocks`; terrain names the content does not define are open ground, and cells off the map are blocked.
- MOVE.SPEED.4  The sim shall treat a cell as blocked for a movement class when the height step from it to any 4-neighbour inside the map exceeds `Movement.max_slope_m` for that class.
- MOVE.SPEED.5  The sim shall let a unit of profile `(class, size)`, `size = clamp(ceil(footprint width / cell), 1, Path.max_clearance)`, stand with its centre in cell `(x, y)` only if the clearance of `class` at the anchor `(x − (size − 1) / 2, y − (size − 1) / 2)` is at least `size`.
- MOVE.SPEED.6  While a driving unit's heading, after this tick's turn of at most its turn rate toward its drive point, is off that direction by more than `Movement.turn_in_place_deg`, the sim shall take no drive step for the unit (it turns in place; its separation push still applies, STEER.SEP.7).
- MOVE.SPEED.7  If any cell the segment of a unit's move sweeps is one its profile cannot pass (including the two side cells of a diagonal and a wall thinner than the step), then the sim shall not make that move; a unit standing in a cell it cannot pass may still move within it or out to a passable cell.
- MOVE.SPEED.8  When a unit following a path is within one step of its current corner and its own drive step is taken, the sim shall put it on the corner and make the next corner current, setting the path `Done` after the last corner.
- MOVE.SPEED.9  While a unit following a path is within `Movement.arrive_m` of its last corner but more than one step from it, the sim shall set the path `Done` where the unit stands.
- MOVE.SPEED.10  The sim shall clamp every step to the map, so no unit position leaves it.
- MOVE.SPEED.11  Where no navigation grid exists (no terrain), the sim shall treat the whole map as open ground of pass 1.
- MOVE.SPEED.12  If a `Movement` rules value, a terrain `pass` or a `Mobility` value lies outside its valid range (Parameters), then the sim shall fail content loading with an error naming the field.

## Formulas

All values are `Fx` (32.32 fixed point) unless stated; products floor to the raw unit.

```text
speed_per_tick   = road_speed_kmh.mul_int(10) / Fx(36 × SIM_TICK_HZ)        (= kmh / 72 at 20 Hz; only the division rounds)
turn_per_tick    = Angle::from_degrees(turn_rate_deg_s / Fx(SIM_TICK_HZ))     (nearest angle unit)
factor(cell)     = clamp((ONE − pass) × terrain_factor + pass, Movement.min_speed_frac, ONE)
                   (in this operation order, ADR-0014; pass = 1 off the map and on unknown terrain)
step_length      = speed_per_tick × factor(cell under the unit)

blocked(class, c) = class ∈ blocks(terrain(c))  ∨  max_step(c) > Movement.max_slope_m[class]
max_step(c)       = max over 4-neighbours n of c inside the map of |height(n) − height(c)|
clearance(class, x, y) = 0                                         if blocked(class, (x, y))
                       = min(Path.max_clearance,
                             1 + min(c(x+1, y), c(x, y+1), c(x+1, y+1)))   otherwise; c = 0 off the map
size(unit)        = clamp(ceil(footprint.width / cell_m), 1, Path.max_clearance)   (1 without a footprint; integer)
passable(p, x, y) = (x, y) on the map  ∧  clearance(p.class, x − (p.size − 1) / 2, y − (p.size − 1) / 2) ≥ p.size

aim (one tick):   d = target − pos;  reached = |d| ≤ step_length
                  heading ← rotate_towards(heading, angle(d), turn_per_tick)
                  if |signed_diff(heading, angle(d))| > turn_in_place: no drive step this tick (the push still applies)
                  else to = map.clamp(reached ? target : pos + normalize(d) × step_length)
```

`cell_m` is the map's cell side (4 m on the base maps). With the base roster every footprint is at most 3.6 m wide,
so every base profile is size 1 on 4 m cells; size 2 and 3 appear in test content.

## Parameters

| Name | Path key | Unit | Default | Valid range | Provenance | Confidence |
|---|---|---|---|---|---|---|
| min_speed_frac | `rules/Movement.min_speed_frac` | fraction of road speed | 0.05 | (0, 1] and ≥ 100 / 65535 | Guess | Low |
| max_slope_m | `rules/Movement.max_slope_m.*` | m per cell step | Foot 100, Wheeled 1.5, HalfTrack 2.0, Tracked 2.5 | ≥ 0 for each entry; one per class | Guess | Low |
| turn_in_place_deg | `rules/Movement.turn_in_place_deg` | degrees | 30 | (0, 180] | Guess | Low |
| arrive_m | `rules/Movement.arrive_m` | m | 0.5 | > 0 | Guess | Low |
| max_clearance (owned by PATH.QUEUE) | `rules/Path.max_clearance` | cells | 3 | 1..=255 | Guess | Low |
| pass | `terrain:*/pass` | fraction of road speed | per terrain (road, bridge 1.0; grass 0.8; forest 0.7; deep forest 0.6; field 0.5; water, swamp 0.0) | [0, 1] | Guess | Low |
| blocks | `terrain:*/blocks.*` | movement class | per terrain (water, swamp: every class; deep forest: Wheeled, HalfTrack, Tracked; others none) | `Foot`, `Wheeled`, `HalfTrack`, `Tracked` | ResearchDerived | Medium |
| class | `unit:*/components.Mobility.class` | movement class | per unit | `Foot`, `Wheeled`, `HalfTrack`, `Tracked` | ResearchDerived | High |
| road_speed_kmh | `unit:*/components.Mobility.road_speed_kmh` | km/h | per unit (5–60) | (0, 200] and > 0 m per tick | OursScaled | Medium |
| terrain_factor | `unit:*/components.Mobility.terrain_factor` | fraction | per unit (0.35–0.8) | [−1, 1] | OursScaled | Low |
| turn_rate_deg_s | `unit:*/components.Mobility.turn_rate_deg_s` | degrees per second | per unit (55–180) | (0, 720] and ≥ 1 angle unit per tick | Guess | Low |
| footprint width (owned by STEER.SEP) | `unit:*/components.Hull.footprint.0` | m | per unit (0.8–3.6) | (0, 50] | OursScaled | Medium |

`min_speed_frac`'s lower bound keeps every path cost inside a `u16`. `max_clearance` is owned by PATH.QUEUE (guess
G-PATH-3) and the footprint by STEER.SEP; `turn_rate_deg_s` is G-ROSTER-2 in `GDD.md` §Guesses.

## Acceptance criteria

- AC-1 (MOVE.SPEED.1): a spawned 36 km/h, 90 °/s tank moves 0.5 m and turns 4.5° per tick → crates/fl_sim/tests/movement.rs::kmh_converts_to_m_per_tick
- AC-2 (MOVE.SPEED.2): `speed_factor` equals the formula on road, grass, forest and pass-0.5 cells, clamps to `min_speed_frac` on water and handles a negative factor → crates/fl_sim/tests/terrain.rs::speed_formula_matches_spec
- AC-3 (MOVE.SPEED.2): a truck covers 40 m in 40 ticks on road and 0.83–0.85 of that on grass → crates/fl_sim/tests/terrain.rs::truck_slows_down_off_road
- AC-4 (MOVE.SPEED.3): water is blocked for every class and every profile → crates/fl_sim/tests/terrain.rs::water_blocks_every_class
- AC-5 (MOVE.SPEED.3): deep forest blocks Wheeled, HalfTrack and Tracked but not Foot → crates/fl_sim/tests/terrain.rs::deep_forest_blocks_vehicles_not_foot
- AC-6 (MOVE.SPEED.3): bridge cells across water pass for every class while the water beside them blocks → crates/fl_sim/tests/terrain.rs::bridge_cells_pass_over_water
- AC-7 (MOVE.SPEED.4): a 9 m height step blocks the cells on both sides of it for a truck but not for a rifleman → crates/fl_sim/tests/terrain.rs::steep_slope_blocks_wheeled_not_foot
- AC-8 (MOVE.SPEED.4): on the testbed map the ridge blocks Wheeled while the hill stays passable → crates/fl_sim/tests/terrain.rs::testbed_ridge_blocks_wheeled_hill_stays_passable
- AC-9 (MOVE.SPEED.5): a 3 m tank (size 1) fits a one-cell gap that a 7 m tank (size 2) cannot stand in → crates/fl_sim/tests/terrain.rs::clearance_blocks_wide_units_in_narrow_gaps
- AC-10 (MOVE.SPEED.5): sizes 1 and 2 anchor at the centre cell and size 3 one cell toward −x and −y; squares off the map are not passable → crates/fl_sim/tests/terrain.rs::profile_anchor_is_centred
- AC-11 (MOVE.SPEED.6): a tank ordered behind itself keeps its position for 10 ticks while its heading changes every tick, then arrives facing the target → crates/fl_sim/tests/movement.rs::tank_turns_before_moving
- AC-12 (MOVE.SPEED.6): while the heading error exceeds 30° the tank stays put, and within 30° it drives → crates/fl_sim/tests/pathing.rs::hull_rotates_before_moving
- AC-13 (MOVE.SPEED.7): a forced diagonal through the shared corner of two water cells is never driven → crates/fl_sim/tests/steering.rs::step_never_cuts_a_blocked_corner
- AC-14 (MOVE.SPEED.7): a 2.5 m-per-tick rifleman never hops a one-cell wall on 1 m cells → crates/fl_sim/tests/steering.rs::fast_unit_never_hops_a_one_cell_wall
- AC-15 (MOVE.SPEED.7): a tank driven at water stops at the shore, its blocked step counted once in `move.blocked` → crates/fl_sim/tests/terrain.rs::units_stop_at_blocked_cells
- AC-16 (MOVE.SPEED.7): on random maps with random move, stop and rotate commands no unit ever stands on a cell its profile cannot pass → crates/fl_sim/tests/steering.rs::units_never_enter_blocked_cells
- AC-17 (MOVE.SPEED.8): a unit at 0.5 m per tick reaches its target exactly on tick 200 and the order pops the next tick → crates/fl_sim/tests/commands.rs::move_command_reaches_target
- AC-18 (MOVE.SPEED.8): a tank paths round a lake through its corners, never on a blocked cell, and finishes within `arrive_m` of its goal → crates/fl_sim/tests/pathing.rs::tank_paths_around_lake
- AC-19 (MOVE.SPEED.9): a unit within `arrive_m` of its last corner but more than one step from it stops there, path `Done` → open: no isolated test (AC-17 and AC-18 cannot tell this branch from the corner snap)
- AC-20 (MOVE.SPEED.10): a move to a point outside the map ends at the clamped point → crates/fl_sim/tests/movement.rs::move_target_outside_map_is_clamped
- AC-21 (MOVE.SPEED.11): on a flat map without terrain a tank moves exactly its road speed per tick → crates/fl_sim/tests/terrain.rs::units_without_terrain_keep_m0_movement
- AC-22 (MOVE.SPEED.12): every out-of-range `Movement` value is a content error naming its field → crates/fl_sim/tests/terrain.rs::movement_rules_ranges_are_validated
- AC-23 (MOVE.SPEED.12): out-of-range `Mobility` speed, factor and turn rate are content errors naming the field → crates/fl_sim/tests/movement.rs::mobility_rejects_out_of_range_values
- AC-24 (MOVE.SPEED.12): speeds and turn rates that round to nothing per tick are content errors → crates/fl_sim/tests/movement.rs::mobility_rejects_rates_that_round_to_zero_per_tick

## Non-goals · Open questions · Guesses

- Non-goal: acceleration, braking and momentum. A unit is at full step length from its first driving tick (M1).
- Non-goal: per-unit slope limits or slope-dependent speed; slope only blocks, per class.
- Open question: spec §7 says tracked and wheeled units turn the hull before moving; M1 applies the turn-in-place
  rule (MOVE.SPEED.6) to every class, Foot included. Infantry turns 180 °/s in the base roster, so it rarely shows.
- Open question: no test pins MOVE.SPEED.9 (a unit within `arrive_m` but more than one step away); its acceptance
  criterion is open, so `trace` reports the rule untraced until the test-author adds one.
- Open question: no test pins a `pass` value out of [0, 1] being rejected (`fl_content` `TerrainDef` validation);
  AC-22 to AC-24 cover the `Movement` and `Mobility` ranges only.
- Guess G-MOVE-1: `rules/Movement.min_speed_frac` = 0.05, the least fraction of road speed any unit keeps on any
  passable cell; with the base roster (terrain factors 0.35 and up) the clamp never binds. Replace by tuning
  against crossings of slow terrain once a unit's factor can drop a cell's speed below it.
- Guess G-MOVE-2: `rules/Movement.max_slope_m` (Foot 100, Wheeled 1.5, HalfTrack 2.0, Tracked 2.5 m per cell
  step), the steepest neighbour step each class climbs. Replace with gradients derived from vehicle data, rescaled
  to the cell size.
- Guess G-MOVE-3: every terrain `pass` value (`terrain:*/pass`: road and bridge 1.0, grass 0.8, forest 0.7, deep
  forest 0.6, field 0.5, water and swamp 0.0), informed by the research notes on terrain passability but not
  measured. Replace by a tuning sweep of crossing times per class.
- Guess G-MOVE-4: `rules/Movement.turn_in_place_deg` = 30, the heading error above which a unit turns in place
  rather than driving. Replace by tuning for how sharply vehicles appear to corner.
- Guess G-MOVE-5: `rules/Movement.arrive_m` = 0.5 m, how close to the last corner counts as arrived. Replace by
  tuning against formation slot spacing and separation (STEER.SEP) once crowds are tuned (C7).

## Changelog

- v1 (2026-10-25, M2 prelude Task 4): first version, backfilled from M1 as built; approved by the lead (R148), on the
  M2 prelude ratification list.
