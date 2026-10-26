---
type: System
---
# `actor_transforms`

**Crate:** `testbed_view` · **File:** [`content/testbed_view/src/visuals.rs`](../../../content/testbed_view/src/visuals.rs) · **Plugin:** `TestbedViewPlugin` (`visuals::plugin`)

Moves each testbed actor's visual root. Position lerps `PrevSimPos → SimPos` by
[`interp_alpha`](interp_alpha.md) (`Motion::step`, continuous at speeds above 1x, like the village). A rider
(`MountedIn`) sits on its vehicle's visual: the vehicle visual's transform plus `seat_local(kind, vehicle,
seat)`, facing the vehicle's way. Otherwise it turns towards its walking direction, or towards the sim
`Facing` when standing (`TURN_RATE` 10/s). It is one step of the testbed's animate chain, which also holds
`tree_states`, `ore_states`, `swing_doors`, `vehicle_transforms`, `spin_wheels` and `item_transforms`.

## Schedule

- **Schedule:** `Update`
- **Set:** `ViewSystems::Animate`
- **Ordering:** `((tree_states, ore_states, swing_doors), vehicle_transforms, spin_wheels, actor_transforms,
  item_transforms).chain()`: vehicles move first so riders read this frame's vehicle transform; items (tools
  in hands) follow the actors.

## Reads / writes

| Access | Data |
|---|---|
| reads | `Time`, `SimTime`, `SimRate`, `SimControl`, `SimClock` (`MotionInputs`); sim `SimPos`, `PrevSimPos`, `Facing`, `MountedIn`; `Vehicle` + `Prop` of the ridden vehicle; `SimIndex`, `VisualIndex`; vehicle visual `Transform` |
| writes | actor visual `Transform`, `ActorVisual.motion` (incl. `moving`) |
| messages / events | none |

## Determinism

View-only: never writes sim state. The sim's seat offsets are world-axis mm (vehicles have no heading in
the sim); the view reads them as vehicle-local, purely for drawing.

## Performance

- **Mechanic tag:** `view.visuals` (prefix `testbed_view::visuals`) · **Budget:** 200 µs + 10 µs/agent
  (`view.visuals`) · no span.

## Related

- [`testbed_view.spawn_visuals`](testbed_view.spawn_visuals.md) — creates the roots.
- [`testbed_view.drive_animation`](testbed_view.drive_animation.md) — uses `motion.moving` for the walk pose.
- [`villager_transforms`](villager_transforms.md) — the village's version.
- Components: [`ActorVisual`](../components/ActorVisual.md), [`VehicleVisual`](../components/VehicleVisual.md),
  [`DoorVisual`](../components/DoorVisual.md), [`MountedIn`](../components/MountedIn.md),
  [`Facing`](../components/Facing.md), [`PrevSimPos`](../components/PrevSimPos.md)
