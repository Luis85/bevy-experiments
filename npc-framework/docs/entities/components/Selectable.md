---
type: Component
---
# `Selectable`

**Crate:** `npc_view_helpers` · **File:** [`crates/npc_view_helpers/src/select.rs`](../../../crates/npc_view_helpers/src/select.rs) · **Capability:** view (generic)

Marker on a visual root (one that has [`VisualOf`](VisualOf.md)) whose sim entity can be picked with a click.
Selection itself is the view resource `Selected(Option<SimId>)`; the command UI's multi-selection is
`CommandSelection`. Neither touches the sim.

## Fields

Marker component, no fields.

## Lifecycle

- **Added by:** content spawners on visual roots: village villagers
  ([`village_view.spawn_visuals`](../systems/village_view.spawn_visuals.md)); testbed actors, items,
  doorways, trees, ore, stations, chests and vehicles
  ([`testbed_view.spawn_visuals`](../systems/testbed_view.spawn_visuals.md)).
- **Read by:** [`click_select`](../systems/click_select.md) (candidates for `pick`); the command UI's
  `candidates` / `pick_candidates` in [`command_ui.rs`](../../../crates/npc_view_helpers/src/command_ui.rs)
  ([`command_input`](../systems/command_input.md)), which also accepts `PickTarget` visuals for right-click
  targets only.
- **Removed by:** despawned with the visual root. The testbed's `item_transforms` also removes it from an
  item's root while the item is carried and inserts it again when the item lies in the world.

## Hashing and snapshots

- **State hash:** no. View components are never hashed.
- **World snapshot (ADR 0013):** not saved (view-only).

## Related

- [`VisualOf`](VisualOf.md) — required next to it.
- `PICK_HEIGHT` (0.7 m) / `PICK_RADIUS` (0.9 m): the pick test against the cursor ray.

## Example

```rust
commands.spawn((Name::new("visual Villager"), VisualOf(sim), Selectable, Transform::from_translation(at)));
// Later, any view system: which unit is selected?
if let Some(id) = selected.0 { /* look it up in SimIndex */ }
```
