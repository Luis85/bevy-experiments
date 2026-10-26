---
type: System
---
# `spawn_visuals` (testbed view)

**Crate:** `testbed_view` · **File:** [`content/testbed_view/src/visuals.rs`](../../../content/testbed_view/src/visuals.rs) · **Plugin:** `TestbedViewPlugin` (`visuals::plugin`)

Gives every testbed sim entity with a `Prop`, an `Item` or a `TestbedUnit` its own visual root carrying
`VisualOf(sim)`. Per prop kind: wall runs tiled from their `GridBlocker` footprint (`StaticVisual`), doorways
with a squeezed frame and a hinged leaf (`DoorVisual`, `Selectable`), trees with tree and stump models
(`TreeVisual`, `Selectable`), the ore rock (`OreVisual`, `Selectable`), stations and chests (`StaticVisual`, `Selectable`), cart and wagon
(`VehicleVisual`, `Selectable`, wheel pivots). Items get `ItemVisual` + `Selectable`. Actors get
`ActorVisual` + `Selectable`, a model chosen by `ModelKind::for_actor(template, owner)` and a faction ring.
An entry whose visual no longer exists is respawned. Its partner `despawn_visuals` removes visuals whose
sim entity has none of the three components any more.

## Schedule

- **Schedule:** `Update` (`make_rings` builds the ring mesh/materials in `Startup`)
- **Set:** `ViewSystems::Sync`
- **Ordering:** `(despawn_visuals, spawn_visuals).chain()`. Skips the frame until `ViewAssets` and `Rings` exist.

## Reads / writes

| Access | Data |
|---|---|
| reads | sim `SimId`, `Prop`, `SimPos`, `GridBlocker`; `Item` (`Without<Prop>`, `Without<TestbedUnit>`); `TestbedUnit`, `Owner`; `ViewAssets`, `Rings` |
| writes | `VisualIndex`; new view entities through `Commands` (roots, `ChildOf` model children, rings) |
| messages / events | observers `on_doorway_ready`, `on_vehicle_ready`, `on_actor_ready` on `WorldInstanceReady` |

## Determinism

View-only: reads sim components, never writes them, and never inserts anything on a sim entity. Model
variety uses `vary(SimId, n)`, a pure function of the id.

## Performance

- **Mechanic tag:** `view.visuals` (via `testbed_view::perf_tags` prefix `testbed_view::visuals`, added by
  `apps/actor_testbed`) · **Budget:** 200 µs + 10 µs/agent (`view.visuals`) · no span.

## Related

- [`actor_transforms`](actor_transforms.md) — moves actors, vehicles and items.
- [`testbed_view.drive_animation`](testbed_view.drive_animation.md) — animates actors.
- [`village_view.spawn_visuals`](village_view.spawn_visuals.md) — the village's spawner.
- Components: [`VisualOf`](../components/VisualOf.md), [`ActorVisual`](../components/ActorVisual.md),
  [`VehicleVisual`](../components/VehicleVisual.md), [`DoorVisual`](../components/DoorVisual.md),
  [`Prop`](../components/Prop.md), [`TestbedUnit`](../components/TestbedUnit.md)
