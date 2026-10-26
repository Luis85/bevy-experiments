---
type: Component
---
# `DoorVisual`

**Crate:** `testbed_view` · **File:** [`content/testbed_view/src/visuals.rs`](../../../content/testbed_view/src/visuals.rs) · **Capability:** testbed view

`pub(crate)` state on a doorway's visual root: the leaf's current swing angle and the hinge pivot it rotates.
The sim door is a 1 m cell; the view squeezes the Dungeon doorway model to `DOOR_SX` = 0.5 along the wall and
re-parents the leaf onto its own hinge so it swings without shearing (crate docs, `# Scale`).

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `angle` | `f32` | current leaf angle, radians (0 closed, `DOOR_OPEN` = -95° open) |
| `pivot` | `Option<Entity>` | the hinge pivot entity; `None` until the doorway model is ready (private) |

## Lifecycle

- **Added by:** [`testbed_view.spawn_visuals`](../systems/testbed_view.spawn_visuals.md) for `Prop { Doorway }`
  (with `VisualOf`, `Selectable`). Its frame model child (`DoorFrame`) has the `on_doorway_ready` observer:
  it finds the `wall_doorway_door` node, spawns the `door hinge` pivot and sets `pivot`.
- **Written by:** `swing_doors` (in the `ViewSystems::Animate` chain with `tree_states`, `ore_states`): moves
  `angle` towards open/closed at `DOOR_RATE` (2.4 rad/s, real time) from the sim `Door.open`, and eases the
  pivot's rotation.
- **Removed by:** `despawn_visuals`.

## Hashing and snapshots

- **State hash:** no. View components are never hashed.
- **World snapshot (ADR 0013):** not saved (view-only).

## Related

- [`Door`](Door.md) — the sim state (`open`) it follows.
- [`Prop`](Prop.md) — `Doorway` kind. [`VisualOf`](VisualOf.md) — the link.

## Example

```rust
// swing_doors: follow the sim door, never write it.
let open = sims.get(of.0).is_ok_and(|d: &Door| d.open);
let want = if open { DOOR_OPEN } else { 0.0 };
door.angle += (want - door.angle).clamp(-step, step);
```
