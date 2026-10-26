---
type: System
---
# `spawn_visuals` (village view)

**Crate:** `village_view` · **File:** [`content/village_view/src/visuals.rs`](../../../content/village_view/src/visuals.rs) · **Plugin:** `VillageViewPlugin` (`visuals::plugin`)

Gives every village sim entity its own visual root. Each entity with `Place` + `SimPos` gets a root with
`VisualOf(sim)`, `PlaceVisual` and model children per `PlaceKind` (house, field, bed, crate + barrel +
`SackPile` for a stockpile, stone + barrel for a well). Each `Villager` gets a root with `VisualOf(sim)`,
`Selectable`, `VillagerVisual` and a villager model child whose `on_villager_ready` observer finds the
`AnimationPlayer` and hides the Rogue kit's weapons. Entities already in `VisualIndex` are skipped. Its
partner `despawn_visuals` despawns the visual of every indexed sim entity that is gone (or no longer a
place/villager) and drops it from the index.

## Schedule

- **Schedule:** `Update`
- **Set:** `ViewSystems::Sync`
- **Ordering:** `(despawn_visuals, spawn_visuals).chain()`. Skips the frame until `ViewAssets` exists.

## Reads / writes

| Access | Data |
|---|---|
| reads | sim `Place`, `SimPos`, `Villager` (`Without<Place>`); `ViewAssets` |
| writes | `VisualIndex` (sim → visual root), new view entities via `Commands` (`ChildOf` children, `WorldAssetRoot` scenes) |
| messages / events | observer `on_villager_ready` on `WorldInstanceReady` |

## Determinism

View-only: reads sim components, never writes them, and never inserts anything on a sim entity.
`VisualIndex` is a view resource (a `BTreeMap`).

## Performance

- **Mechanic tag:** `view.visuals` (via `village_view::perf_tags` prefix `village_view::visuals`, added by
  `apps/village`) · **Budget:** 200 µs + 10 µs/agent (`view.visuals` in `NPC_BUDGET_TABLE`) · no span.

## Related

- [`villager_transforms`](villager_transforms.md) — moves the roots each frame.
- [`village_view.drive_animation`](village_view.drive_animation.md) — animates the models.
- [`testbed_view.spawn_visuals`](testbed_view.spawn_visuals.md) — the testbed's spawner.
- Components: [`VisualOf`](../components/VisualOf.md), [`PlaceVisual`](../components/PlaceVisual.md),
  [`VillagerVisual`](../components/VillagerVisual.md), [`Selectable`](../components/Selectable.md),
  [`Place`](../components/Place.md), [`Villager`](../components/Villager.md)
