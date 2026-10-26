---
type: Component
---
# `WorkSite`

**Crate:** `village_content` · **File:** [`content/village_content/src/world.rs`](../../../content/village_content/src/world.rs) · **Capability:** village content

A place where a villager works a shift that adds food to a stockpile (the village field). One worker at a time
(claimed through `Reservations`, capacity 1).

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `work_ticks` | `u32` | ticks of work per shift (scenario: `FIELD_WORK_TICKS` = 300) |
| `yield_food` | `u32` | food units added to the stockpile per completed shift (scenario: `FIELD_YIELD_FOOD` = 3) |
| `stockpile` | `SimId` | the `Stockpile` that receives the yield |

## Lifecycle

- **Added by:** `spawn_plot` in [`scenario.rs`](../../../content/village_content/src/scenario.rs), with
  `Place { kind: Field }`, `SimPos` and `Approach` (no `Provides`).
- **Read by:** `find_work_site` (lowest-`SimId` site free for the agent; sites whose stockpile has room are
  preferred) and `holds_work_site` in [`villager.rs`](../../../content/village_content/src/villager.rs)
  (conditions `forage`, `day_work`); `do_work_start` (copies `work_ticks` into [`Work`](Work.md)) and
  [`work_system`](../systems/work_system.md) (writes `AddFood` for `stockpile` / `yield_food`) in
  [`actions.rs`](../../../content/village_content/src/actions.rs); the `work` agent affordance.
- **Written / removed by:** never.

## Hashing and snapshots

- **State hash:** no (no `SimHash` impl; static place data).
- **World snapshot (ADR 0013):** saved as `work_site` (snapshot-only, `register_snapshot_component_named` in
  `register_snapshot_entries`). Not reflected.

## Related

- [`Work`](Work.md) — the running shift.
- [`Stockpile`](Stockpile.md) — where the yield goes.
- [`resolve_effects_system`](../systems/resolve_effects_system.md) — applies the yield.

## Example

```rust
spawn_sim(world, (
    Place { kind: PlaceKind::Field, yaw_deg: 0 },
    SimPos::new(3000, 5000),
    Approach(SimPos::new(3000, 2200)),
    WorkSite { work_ticks: 300, yield_food: 3, stockpile },
));
```
