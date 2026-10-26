---
type: Component
---
# `VillagerVisual`

**Crate:** `village_view` · **File:** [`content/village_view/src/visuals.rs`](../../../content/village_view/src/visuals.rs) · **Capability:** village view

`pub(crate)` state on a villager's visual root: the smoothed facing and the interpolation endpoints. It lets
motion stay continuous at speeds above 1x, when several ticks run between two frames.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `yaw` | `f32` | current smoothed yaw, radians (model +Z faces this way) |
| `from`, `to` | `Vec3` | interpolation endpoints, metres (world space) |
| `tick` | `Option<u64>` | the `SimClock.tick` the endpoints belong to |

All fields are private to the module.

## Lifecycle

- **Added by:** [`village_view.spawn_visuals`](../systems/village_view.spawn_visuals.md) on the villager root
  (with `VisualOf`, `Selectable`, `Transform`); its model child gets `VillagerModel { root }` and the
  `on_villager_ready` observer, which puts `VillagerAnimTarget` on the entity holding the `AnimationPlayer`.
- **Written by:** [`villager_transforms`](../systems/villager_transforms.md) each frame.
- **Removed by:** `despawn_visuals` (despawns the root with its children).

## Hashing and snapshots

- **State hash:** no. View components are never hashed.
- **World snapshot (ADR 0013):** not saved (view-only; rebuilt from `SimPos`).

## Related

- [`VisualOf`](VisualOf.md) — the link to the villager.
- [`Villager`](Villager.md), [`PrevSimPos`](PrevSimPos.md), [`SimPos`](SimPos.md) — what it interpolates.
- [`PlaceVisual`](PlaceVisual.md) — the place counterpart.

## Example

```rust
// visuals.rs spawn: endpoints start at the spawn position.
let at = sim_to_world(*pos);
commands.spawn((VisualOf(sim), Selectable, VillagerVisual { from: at, to: at, ..default() },
                Transform::from_translation(at), Visibility::default()));
```
