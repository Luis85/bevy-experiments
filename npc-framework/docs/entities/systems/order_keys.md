---
type: System
---
# `order_keys`

**Crate:** `testbed_view` · **File:** [`content/testbed_view/src/command.rs`](../../../content/testbed_view/src/command.rs) · **Plugin:** `TestbedViewPlugin` (`command::plugin`, which also adds `CommandUiPlugin` if missing)

The testbed's RTS hotkeys on top of the generic command UI. Z / X / C set the group-move formation to Line /
Box / Wedge (`FORMATION_SPACING_MM` = 1500 mm), V back to the sim's default; H issues `Hold`, B `Stop`, U
`Unload` for the current `CommandSelection`. `Unload` goes to the vehicles selected or ridden by the
selection (sorted, deduplicated `SimId`s). Every order is a recorded `IssueOrder` command. The same plugin
registers the right-click resolver `testbed.use_tool` (`context_tool_order`: a tree or ore rock with uses
left → `Interact { verb: "use_tool_on" }`) and `inspect_target` (in `ViewSystems::Animate`: when `Selected`
is not an actor or vehicle, it falls back to the first commanded unit).

## Schedule

- **Schedule:** `Update`, exclusive (`fn order_keys(world: &mut World)`)
- **Set:** `ViewSystems::Input` (gated by `not(modal_open)` with the explorer's prompt modal and by
  `not(editor_typing)` while a field of the F7 notes editor has focus)
- **Ordering:** none beyond the set.

## Reads / writes

| Access | Data |
|---|---|
| reads | `ButtonInput<KeyCode>`, `CommandUi` (player), `CommandSelection`, `SimIndex`; sim `Vehicle`, `MountedIn` |
| writes | `CommandFormation` (view resource) |
| messages / events | `submit(world, None, IssueOrder::new(player, units, order).to_command())`; a rejection is logged with `warn!` |

## Determinism

Never mutates sim components: orders reach the sim only as recorded `SimCommand`s, applied in
`NpcSet::Inputs`, so replays reproduce them. The `Unload` vehicle list is sorted and deduplicated by
`SimId`.

## Performance

- **Mechanic tag:** `view.command` (via `testbed_view::perf_tags` prefix `testbed_view::command`) · **Budget:**
  none · no span.

## Related

- [`command_input`](command_input.md) — the generic selection and right-click orders.
- [`command_engine_system`](command_engine_system.md) — executes the orders.
- Components: [`OrderQueue`](../components/OrderQueue.md), [`MountedIn`](../components/MountedIn.md),
  [`Vehicle`](../components/Vehicle.md)
