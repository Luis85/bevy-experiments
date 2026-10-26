---
type: System
---
# `villager_transforms`

**Crate:** `village_view` · **File:** [`content/village_view/src/visuals.rs`](../../../content/village_view/src/visuals.rs) · **Plugin:** `VillageViewPlugin` (`visuals::plugin`)

Moves each villager's visual root between ticks. It lerps `PrevSimPos → SimPos` by
[`interp_alpha`](interp_alpha.md). When several ticks ran since the last frame (speed above 1x), it lerps from
the last drawn position instead, so motion stays continuous. Facing turns smoothly towards the walking
direction (`TURN_RATE` 12/s, real `Time` delta). A villager whose `Activity` is `Sleep` is placed lying on
the bed its `Sleep.bed` names (bed position, bed yaw, `LIE_OFFSET`). In the same group: `place_transforms`
(place roots follow `SimPos` and `yaw_deg`) and `update_sacks` (respawns the stockpile's sack models when
`min(food, 10)` changes).

## Schedule

- **Schedule:** `Update`
- **Set:** `ViewSystems::Animate`
- **Ordering:** `(place_transforms, villager_transforms, update_sacks)`, unordered among themselves (no
  `.chain()`).

## Reads / writes

| Access | Data |
|---|---|
| reads | `Time`; `SimTime`, `SimRate`, `SimControl`, `SimClock`, `SimIndex` (all optional); sim `SimPos`, `PrevSimPos`, `Activity`, `Sleep` (`With<Villager>`); beds `Place` + `SimPos` (`With<Bed>`) |
| writes | visual `Transform`, `VillagerVisual` (`from`, `to`, `tick`, `yaw`) |
| messages / events | none |

## Determinism

View-only: never writes sim state. Floats live only in view components. It reads the driver's `SimTime`,
never `Time<Fixed>`.

## Performance

- **Mechanic tag:** `view.visuals` (prefix `village_view::visuals`) · **Budget:** 200 µs + 10 µs/agent
  (`view.visuals`) · no span.

## Related

- [`village_view.spawn_visuals`](village_view.spawn_visuals.md) — creates the roots.
- [`interp_alpha`](interp_alpha.md) — the lerp factor.
- [`actor_transforms`](actor_transforms.md) — the testbed's version.
- Components: [`VillagerVisual`](../components/VillagerVisual.md), [`PrevSimPos`](../components/PrevSimPos.md),
  [`Activity`](../components/Activity.md), [`Sleep`](../components/Sleep.md), [`Bed`](../components/Bed.md)
