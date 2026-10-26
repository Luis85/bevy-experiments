---
type: System
---
# `perception_text`

**Crate:** `testbed_view` · **File:** [`content/testbed_view/src/panels.rs`](../../../content/testbed_view/src/panels.rs) · **Plugin:** `TestbedViewPlugin` (`panels::plugin`)

Writes the F6 perception panel for the selected unit: its facing and sensors (vision range and half field
of view, hearing radius, proximity radius), what kind it is (`Perceivable` bits), the sensed percepts (up to 10: name, kind,
distance, sense, confidence) and remembered ones (up to 8: name, kind, distance, ticks since last seen,
confidence fading over `PerceptionConfig.forget_after_ticks`), and the last noise heard. Its sibling
`perception_gizmos` draws the selected unit's vision cone, rays to what it senses, memory markers and the
last noise. With F6 on and nothing selected, `toggle_panels` turns on the generic all-agent
`PerceptionGizmos` from `npc_view_helpers` instead. Runs only while `TestbedPanels.perception` is on.

## Schedule

- **Schedule:** `Update`
- **Set:** `ViewSystems::Present`, with `sheet_text` and `perception_gizmos` (unordered)
- **Ordering:** `toggle_panels` in `ViewSystems::Input` handles F5/F6.

## Reads / writes

| Access | Data |
|---|---|
| reads | `TestbedPanels`, `Selected`, `SimIndex`, `SimClock`, `PerceptionConfig`, `TestbedIds`; sim `SimPos`, `Sensors`, `Facing`, `PerceptionMemory`, `Perceivable` |
| writes | the perception panel `Text` (view entity); `perception_gizmos` writes only `Gizmos` |
| messages / events | none |

## Determinism

View-only: reads perception state, never writes it. Confidence is computed for display with
`confidence_at(now, forget)`, a read-only call.

## Performance

- **Mechanic tag:** `view.panels` (prefix `testbed_view::panels`) · **Budget:** none in `NPC_BUDGET_TABLE` ·
  no span.

## Related

- [`sheet_text`](sheet_text.md) — F5.
- [`perception_system`](perception_system.md) — fills `PerceptionMemory`.
- Components: [`Sensors`](../components/Sensors.md), [`Perceivable`](../components/Perceivable.md),
  [`Facing`](../components/Facing.md), [`PerceptionMemory`](../components/PerceptionMemory.md)
