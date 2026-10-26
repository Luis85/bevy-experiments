---
type: System
---
# `update_text` (village F1 overlay)

**Crate:** `village_view` · **File:** [`content/village_view/src/overlay.rs`](../../../content/village_view/src/overlay.rs) · **Plugin:** `VillageViewPlugin` (`overlay::plugin`)

Writes the F1 debug overlay text: tick, day and clock time (`SimConfig.day_length`; the day starts at
06:00), night flag, speed or `PAUSED`, the cached state hash, the selected unit, every stockpile (`food
f/c`, sorted by `SimId`), then per villager (sorted by `SimId`) its `Activity`, hunger/energy/thirst bars
with % and level, the running tree path, and the `BB_TARGET` / `BB_DEST` slots; then all reservations and a
key help line. Every part is optional: a missing resource or component leaves its line out. It runs only
while `OverlayState.visible` (default on; `toggle_overlay` flips it on F1).

## Schedule

- **Schedule:** `Update`
- **Set:** `ViewSystems::Present`
- **Ordering:** `(update_hash, update_text, draw_gizmos).chain()`. `update_hash` is exclusive (`&mut World`):
  while visible and the full sim is installed, it recomputes `npc_framework::kernel::hash::state_hash(world)`
  once per new tick into `HashCache`. `draw_gizmos` draws approach circles, a yellow line to each villager's
  `BB_DEST`, and an orange ring on every reserved target.

## Reads / writes

| Access | Data |
|---|---|
| reads | `OverlayState`, `HashCache`, `Selected`; `SimClock`, `SimConfig`, `SimControl`, `NeedRegistry`, `AiRegistry`, `Reservations` (optional); `Stockpile`; per villager `SimId`, `Needs`, `Activity`, `Brain`, `BrainState`, `Blackboard` |
| writes | the overlay `Text` (view entity) |
| messages / events | none |

## Determinism

View-only: reads sim state, never writes it. `state_hash` only reads the world. Rows are sorted by `SimId`
so the text is stable.

## Performance

- **Mechanic tag:** `view.overlay` (prefix `village_view::overlay`) · **Budget:** 500 µs (`view.overlay`) ·
  no span. The hash is the expensive part; it is cached per tick and skipped when hidden.

## Related

- [`npc_view_helpers.journal_panel.update_panel`](npc_view_helpers.journal_panel.update_panel.md),
  [`npc_view_helpers.perf_panel.update_panel`](npc_view_helpers.perf_panel.update_panel.md) — F3 / F4.
- [`testbed_view.update_text`](testbed_view.update_text.md) — the testbed's F1 overlay.
- Components: [`Activity`](../components/Activity.md), [`Stockpile`](../components/Stockpile.md),
  [`Needs`](../components/Needs.md)
