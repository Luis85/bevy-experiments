---
type: Component
---
# `Place`

**Crate:** `village_content` · **File:** [`content/village_content/src/world.rs`](../../../content/village_content/src/world.rs) · **Capability:** village content

Marks a village smart object (house, bed, stockpile, field, well) and says what it is. The sim uses it only
for naming. The view picks the model by `kind` and turns it by `yaw_deg`. It has `#[require(SimPos)]`.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `kind` | `PlaceKind` | `House`, `Bed`, `Stockpile`, `Field`, `Well` |
| `yaw_deg` | `i32` | model facing, whole degrees around +Y; a presentation hint the sim never reads |

## Lifecycle

- **Added by:** `spawn_plot` and `spawn_well` in [`scenario.rs`](../../../content/village_content/src/scenario.rs),
  together with the role component (`Home`, `Bed`, `Stockpile`, `WorkSite` or `Well`), an `Approach` and,
  for bed, stockpile and well, a `Provides`.
- **Read by:** `kind_name` in [`world.rs`](../../../content/village_content/src/world.rs) (`"house"`, `"bed"`,
  `"stockpile"`, `"field"`, `"well"` for journal `Spawn` and inspect); the `village` agent section in
  [`agent.rs`](../../../content/village_content/src/agent.rs); the view:
  [`village_view.spawn_visuals`](../systems/village_view.spawn_visuals.md) (model per kind), `place_transforms`
  and [`villager_transforms`](../systems/villager_transforms.md) (bed yaw for the lying pose).
- **Written / removed by:** nothing changes it after spawn.

## Hashing and snapshots

- **State hash:** no (no `SimHash` impl).
- **World snapshot (ADR 0013):** saved as `place` (snapshot-only, `register_snapshot_component_named` in
  `register_snapshot_entries`). Also reflected (`register_type`, BRP type path `npc_sim::world`).

## Related

- [`Home`](Home.md), [`Bed`](Bed.md), [`Stockpile`](Stockpile.md), [`WorkSite`](WorkSite.md),
  [`Well`](Well.md) — the role a place plays.
- [`PlaceVisual`](PlaceVisual.md) — its visual root in the village view.
- [`Approach`](Approach.md) — where to stand to use it.

## Example

```rust
spawn_sim(world, (
    Place { kind: PlaceKind::Bed, yaw_deg: 90 },
    SimPos::new(-2500, -5500),
    Approach(SimPos::new(-2500, -4000)),
    Bed { home: house },
    Provides { need: energy, amount_per_tick: BED_ENERGY_PER_TICK },
));
```
