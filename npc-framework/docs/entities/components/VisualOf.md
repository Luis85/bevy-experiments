---
type: Component
---
# `VisualOf`

**Crate:** `npc_view_helpers` · **File:** [`crates/npc_view_helpers/src/visuals.rs`](../../../crates/npc_view_helpers/src/visuals.rs) · **Capability:** view (generic, facade feature `view-helpers`)

The link from a visual root entity to the sim entity it shows. Every drawn sim entity gets its own visual
entity with `VisualOf(sim)`; nothing is ever inserted on the sim entity (rule in `.claude/rules/view.md`).
The reverse map, sim entity → visual root, is the `VisualIndex` resource (`BTreeMap<Entity, Entity>`), kept
by the content's visual spawner.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `0` | `Entity` | the sim entity this visual presents |

## Lifecycle

- **Added by:** content spawners, on the visual root only:
  [`village_view.spawn_visuals`](../systems/village_view.spawn_visuals.md) (places and villagers),
  [`testbed_view.spawn_visuals`](../systems/testbed_view.spawn_visuals.md) (props, items, actors), and
  `apps/rts_units_demo/src/view.rs`.
- **Read by:** `click_select` / `draw_selection` ([`click_select`](../systems/click_select.md)) and the
  command UI pickers ([`command_input`](../systems/command_input.md)); every transform/animation system
  ([`villager_transforms`](../systems/villager_transforms.md), [`actor_transforms`](../systems/actor_transforms.md),
  [`village_view.drive_animation`](../systems/village_view.drive_animation.md),
  [`testbed_view.drive_animation`](../systems/testbed_view.drive_animation.md)); overlay and F6 gizmos (they
  draw from the visual's `GlobalTransform`).
- **Removed by:** the despawn half of the spawners (`despawn_visuals`) despawns the visual when its sim
  entity is gone.

## Hashing and snapshots

- **State hash:** no. View components are never hashed.
- **World snapshot (ADR 0013):** not saved. Visual entities are rebuilt from sim state by the spawner; a
  loaded world gets fresh visuals.

## Related

- [`Selectable`](Selectable.md) — marks a visual root as clickable.
- [`VillagerVisual`](VillagerVisual.md), [`PlaceVisual`](PlaceVisual.md), [`ActorVisual`](ActorVisual.md),
  [`VehicleVisual`](VehicleVisual.md), [`DoorVisual`](DoorVisual.md) — content state on visual roots.

## Example

```rust
// A content spawner: one visual root per sim entity, never on the sim entity.
let root = commands
    .spawn((VisualOf(sim), Selectable, Transform::from_translation(sim_to_world(*pos)), Visibility::default()))
    .id();
index.map.insert(sim, root);

// A view system reading the sim through the link.
for (of, mut transform) in &mut visuals {
    if let Ok(pos) = sims.get(of.0) { transform.translation = sim_to_world(*pos); }
}
```
