---
type: Component
---
# `VehicleVisual`

**Crate:** `testbed_view` · **File:** [`content/testbed_view/src/visuals.rs`](../../../content/testbed_view/src/visuals.rs) · **Capability:** testbed view

`pub(crate)` state on the visual root of a testbed vehicle (`Prop` `Cart` or `Wagon`, which also carry a sim
`Vehicle`): its interpolation and heading, and its wheel pivots so the wheels spin as it rolls.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `kind` | `PropKind` | `Cart` or `Wagon` (private) |
| `motion` | `Motion` | interpolation and yaw (radians), as on [`ActorVisual`](ActorVisual.md) |
| `wheels` | `Vec<Entity>` | wheel pivot entities, filled by `on_vehicle_ready` (private) |
| `spin` | `f32` | wheel angle, radians (private) |
| `last` | `Option<Vec3>` | the root's translation last frame, metres (private) |

## Lifecycle

- **Added by:** [`testbed_view.spawn_visuals`](../systems/testbed_view.spawn_visuals.md) for `Prop { Cart |
  Wagon }` (with `VisualOf`, `Selectable`). Its model child (`VehicleModel`) gets the `on_vehicle_ready`
  observer, which inserts a pivot at each wheel's hub.
- **Written by:** `vehicle_transforms` (position and yaw, turning at half the actor turn rate) and
  `spin_wheels` (wheel rotation from the distance rolled along its forward axis), both in the
  [`actor_transforms`](../systems/actor_transforms.md) chain.
- **Read by:** [`actor_transforms`](../systems/actor_transforms.md) (riders sit on the vehicle visual's
  transform plus a seat offset).
- **Removed by:** `despawn_visuals`.

## Hashing and snapshots

- **State hash:** no. View components are never hashed.
- **World snapshot (ADR 0013):** not saved (view-only).

## Related

- [`Prop`](Prop.md), [`Vehicle`](Vehicle.md), [`MountedIn`](MountedIn.md) — the sim state it draws.
- [`VisualOf`](VisualOf.md) — the link.

## Example

```rust
// A rider's place: the vehicle visual's transform plus the seat, in the vehicle frame.
let vt = vehicle_visuals.get(vis_index.map[&vehicle_entity])?;
transform.translation = vt.translation + vt.rotation * seat_local(prop.kind, vehicle, seat);
```
