---
type: Component
---
# `Home`

**Crate:** `village_content` · **File:** [`content/village_content/src/world.rs`](../../../content/village_content/src/world.rs) · **Capability:** village content

Marks a house a villager can live in. Villagers point at it with `Villager.home`; beds belong to it through
`Bed.home`.

## Fields

Marker component, no fields.

## Lifecycle

- **Added by:** `spawn_plot` in [`scenario.rs`](../../../content/village_content/src/scenario.rs), with
  `Place { kind: House }`, `SimPos` and `Approach`.
- **Read by:** `SpawnVillager` (`resolve_home`: the given `home` must have `Home`, else the lowest-`SimId`
  entity with `Home` is used) in [`commands.rs`](../../../content/village_content/src/commands.rs); the
  `village` agent section in [`agent.rs`](../../../content/village_content/src/agent.rs). The wander centre in
  `pick_wander_point_start` is the home's `Approach`, found through `Villager.home`, not through this marker.
- **Written / removed by:** never.

## Hashing and snapshots

- **State hash:** no (no `SimHash` impl).
- **World snapshot (ADR 0013):** saved as `home` (snapshot-only, `register_snapshot_component_named` in
  `register_snapshot_entries`, [`lib.rs`](../../../content/village_content/src/lib.rs)). Not reflected.

## Related

- [`Villager`](Villager.md) — its `home` field.
- [`Bed`](Bed.md) — the beds of this home.
- [`Place`](Place.md) — the kind (`House`).

## Example

```rust
let (house, _) = spawn_sim(world, (
    Place { kind: PlaceKind::House, yaw_deg: 0 },
    SimPos::new(-6000, -5000),
    Approach(SimPos::new(-6000, -2600)),
    Home,
));
```
