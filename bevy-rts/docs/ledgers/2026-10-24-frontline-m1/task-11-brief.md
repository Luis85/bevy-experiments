### Task 11: Steering and separation

**Track:** sim-core + perf · **Refs:** spec §7, §5.4 (pure compute, ordered apply), ADR-0006 (R72 Transient), R76 · **Lane 1**

**Files:**
- Create: `crates/fl_sim/src/steer.rs`, `crates/fl_sim/tests/steering.rs`
- Modify: `crates/fl_sim/src/{movement,lib}.rs`
- Blessed: system order, API, golden, budgets and perf snapshots (`steer.units`).

**Interfaces:**
- Consumes: `SpatialGrid::query_radius`, `Footprint`, `SteeringRules`, `NavGrid::passable`.
- Produces (R76: no per-tick Transient component; a Transient component is removed from every entity each tick, an archetype move per unit):
```rust
#[derive(Resource, Clone, Debug, Default, PartialEq, Eq)] pub struct SteerPushes { .. }   // Transient "core.steer",
    // Class::TransientReset(SteerPushes::clear): emptied at the start of each tick by Housekeeping, capacity kept (no per-tick allocation)
impl SteerPushes {
    pub fn clear(&mut self);
    pub fn get(&self, id: SimId) -> FxVec2;      // this tick's separation push in metres; FxVec2::ZERO for a unit not steered (binary search)
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}
    // one (SimId, FxVec2) entry per steered unit, pushed in SimIndex order, so sorted by SimId
pub(crate) fn compute_steering(..);   // ThinkSet::Behaviour core, after process_path_queue; the only writer of SteerPushes
```
- **Who is steered.** Units with `Mobility` and `Footprint` that are moving, plus idle units whose `sim_id % idle_every == tick % idle_every`.
- **Neighbours.** `query_radius(pos, r_max)`, where `radius(u) = max(width, length) / 2` and the pair distance is `r = radius(u) + radius(v) + separation_margin_m`.
- **Push.** The sum, over neighbours v ≠ u in `SimId` order with `d < r`, of `(u − v) / d × (r − d) × separation_gain`. A coincident pair (d = 0) pushes along +x for the higher `SimId` and −x for the lower. The push is capped at `max_push_frac × speed per tick`.
- **Positions read.** Only positions as of the start of the tick (nothing moves before `ActSet::Move`), so each push is order-free. `compute_steering` walks `SimIndex` and appends one entry per steered unit to `SteerPushes`.
- **`move_system` integration.** Add `SteerPushes::get(sim_id)` to the step. If the target cell is impassable, try the x component alone, then the y component alone, else stay (count `move.blocked`). `steer.units` counts the units steered.

- [ ] **Step 1: Write the failing tests** (`crates/fl_sim/tests/steering.rs`):
```rust
#[test] fn overlapping_idle_units_separate()     // two tanks spawned on one point → by tick 40 their distance ≥ r − 0.1 m
#[test] fn separation_never_pushes_into_water()  // 10 tanks crowded on a 1-cell shore strip, 200 ticks → every tick every unit's cell passable
// proptest (64 cases; milestone_ variant at 1024): random ASCII maps, random units and random move/stop/rotate commands, 300 ticks →
//   every tick, every unit with Mobility stands on a cell passable for its profile
#[test] fn units_never_enter_blocked_cells()
#[test] fn milestone_units_never_enter_blocked_cells_long()
#[test] fn idle_steering_is_staggered()          // 100 idle units: "steer.units" ≤ moving + ceil(100 / 5) per tick
#[test] fn steer_pushes_are_per_tick_and_sorted() // after a tick: SteerPushes entries strictly ascending by SimId, len == "steer.units";
                                                  //   after a later tick that steers no unit it is empty (Housekeeping reset it);
                                                  //   the units' archetypes are the same before and after a steering tick
#[test] fn smoke_steering_twin_and_shuffle()
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_sim --test steering`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_sim -p fl_test_utils`. Expected: PASS (protected `.new` pending).
- [ ] **Step 5: Regenerate** system order, API, the golden, budgets and perf snapshots. The lead blesses them: `M1 Task 11: steering and separation`.
- [ ] **Step 6: Gate** `cargo xtask gate task --track sim-core --track perf`. Expected: PASS. The controller then runs `gate pr` and fast-forwards `main` (end of Phase II, lane 1).
- [ ] **Step 7: Commit** `feat(fl_sim): separation steering, axis sliding at blocked cells, staggered idle steering` with trailer `Blessed: …`.

