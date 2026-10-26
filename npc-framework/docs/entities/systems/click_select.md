---
type: System
---
# `click_select`

**Crate:** `npc_view_helpers` · **File:** [`crates/npc_view_helpers/src/select.rs`](../../../crates/npc_view_helpers/src/select.rs) · **Plugin:** `NpcViewHelpersPlugin` (`select::plugin`)

Single-unit selection with the left mouse button. On a left click it casts the cursor ray from the 3D camera
and calls `pick` over every `Selectable` visual: the unit whose body point (`PICK_HEIGHT` = 0.7 m above its
feet) is nearest the ray, within `PICK_RADIUS` = 0.9 m and in front of the camera (ties: closer to the
camera). The result goes into the view resource `Selected(Option<SimId>)`; a click on nothing clears it.
The same module adds `drop_stale` (deselects a unit that left the sim) and `draw_selection` (two lime
circles under the selected unit).

## Schedule

- **Schedule:** `Update`
- **Set:** `ViewSystems::Input` (`click_select`, `drop_stale` `.chain()`); `draw_selection` in
  `ViewSystems::Present`. `ViewSystems::{Input, Sync, Animate, Present}` are chained by
  `NpcViewHelpersPlugin`.
- **Ordering:** `.chain()` with `drop_stale`.

## Reads / writes

| Access | Data |
|---|---|
| reads | `ButtonInput<MouseButton>` (optional), primary `Window`, `(Camera, GlobalTransform)` `With<Camera3d>`, `VisualIndex`, `VisualOf` `With<Selectable>`, sim `SimId` / `SimPos`, visual `GlobalTransform` |
| writes | `Selected` (view resource) only |
| messages / events | none |

## Determinism

View-only: only reads sim state, never writes it. Selection is not part of any hash or replay. Feet come from
the visual's `GlobalTransform`, else from `SimPos`.

## Performance

- **Mechanic tag:** `view.select` (via `MechanicTags::default_npc` prefix `npc_view_helpers::select`) ·
  **Budget:** none · no span.

## Related

- [`command_input`](command_input.md) — the RTS command UI's multi-selection (it keeps `Selected` on its
  first unit).
- Components: [`Selectable`](../components/Selectable.md), [`VisualOf`](../components/VisualOf.md)
