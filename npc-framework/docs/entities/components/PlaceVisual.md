---
type: Component
---
# `PlaceVisual`

**Crate:** `village_view` · **File:** [`content/village_view/src/visuals.rs`](../../../content/village_view/src/visuals.rs) · **Capability:** village view

`pub(crate)` marker on a village place's visual root. It tells `place_transforms` which roots follow a
`Place`'s position and yaw. A stockpile root also has a `SackPile { sim, shown }` child that shows
`min(food, 10)` sack models.

## Fields

Marker component, no fields.

## Lifecycle

- **Added by:** [`village_view.spawn_visuals`](../systems/village_view.spawn_visuals.md) for every sim entity
  with `Place` + `SimPos` (with `VisualOf`, a `Transform` from `SimPos` and `yaw_deg`). Model children per
  `PlaceKind`: house, field, bed, crate + barrel + `SackPile` (stockpile), stone + barrel (well).
- **Read by:** `place_transforms` (`With<PlaceVisual>`), part of
  [`villager_transforms`](../systems/villager_transforms.md)'s `ViewSystems::Animate` group.
- **Removed by:** `despawn_visuals` when the place leaves the sim.

## Hashing and snapshots

- **State hash:** no. View components are never hashed.
- **World snapshot (ADR 0013):** not saved (view-only).

## Related

- [`Place`](Place.md) — the sim component it draws.
- [`VisualOf`](VisualOf.md) — the link. [`Stockpile`](Stockpile.md) — read by `update_sacks`.

## Example

```rust
commands.spawn((
    Name::new(format!("visual {:?}", place.kind)),
    VisualOf(sim),
    PlaceVisual,
    place_transform(place, *pos), // SimPos -> metres, yaw_deg -> radians
    Visibility::default(),
));
```
