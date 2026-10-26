---
type: System
---
# `update_text` (testbed F1 overlay)

**Crate:** `testbed_view` · **File:** [`content/testbed_view/src/overlay.rs`](../../../content/testbed_view/src/overlay.rs) · **Plugin:** `TestbedViewPlugin` (`overlay::plugin`)

Writes the testbed's F1 overlay text: tick, speed or `PAUSED`, the cached state hash, the command UI state
(player, formation, how many selected), then the selected unit: its name (from `TestbedIds`), template and
owner, pose (`pose_of`), external agent controller, running interaction, crafting, ride and seat, needs, the
running tree path, the order queue, inventory and equipment, plus key help. Every part is optional; a
missing resource or component leaves its line out. Runs only while `OverlayState.visible` (default on; F1 via
`toggle_overlay`).

## Schedule

- **Schedule:** `Update`
- **Set:** `ViewSystems::Present`
- **Ordering:** `(update_hash, update_text, draw_markers).chain()`. `update_hash` is exclusive and recomputes
  `state_hash(world)` once per new tick while visible (as in the village). `draw_markers` draws gold rings
  under items lying in the world (`Without<CarriedBy>`, `Without<Prop>`).

## Reads / writes

| Access | Data |
|---|---|
| reads | `OverlayState`, `HashCache`, `Selected`, `CommandUi`, `CommandSelection`, `CommandFormation`; `SimClock`, `SimControl`, `SimIndex`, `AiRegistry`, `NeedRegistry`, `TestbedIds` (optional); per unit `SimId`, `SimPos`, `TestbedUnit`, `Owner`, `Brain`, `BrainState`, `Needs`, `OrderQueue`, `Interacting`, `Crafting`, `MountedIn`, `Inventory`, `Equipment`, `ExternalController`, `Vehicle`; `Item`; `VisualIndex` + `ActorVisual` |
| writes | the overlay `Text` (view entity) |
| messages / events | none |

## Determinism

View-only: reads sim state, never writes it; `state_hash` only reads.

## Performance

- **Mechanic tag:** `view.overlay` (prefix `testbed_view::overlay`) · **Budget:** 500 µs (`view.overlay`) · no span.

## Related

- [`village_view.update_text`](village_view.update_text.md) — the village's F1 overlay.
- [`sheet_text`](sheet_text.md) (F5), [`perception_text`](perception_text.md) (F6),
  [`testbed_view.explorer_panel.refresh`](testbed_view.explorer_panel.refresh.md) (F7).
- Components: [`TestbedUnit`](../components/TestbedUnit.md), [`OrderQueue`](../components/OrderQueue.md),
  [`Inventory`](../components/Inventory.md), [`Equipment`](../components/Equipment.md)
