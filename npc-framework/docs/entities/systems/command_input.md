---
type: System
---
# `command_input`

**Crate:** `npc_view_helpers` (feature `command`) · **File:** [`crates/npc_view_helpers/src/command_ui.rs`](../../../crates/npc_view_helpers/src/command_ui.rs) · **Plugin:** `CommandUiPlugin` (opt-in; added by `testbed_view`)

The RTS command UI's input pass: mouse and keys become a multi-selection, control groups and **recorded
orders**. Left click selects one own unit (Shift: add/remove), a left drag box-selects; Ctrl+1–9 stores a
control group, 1–9 recalls it; right click issues an order for the selection (Shift: queue) of the kind
`RightClickMode` says (`Move` context order, `AttackMove`, `Patrol` draft, `Custom(name)`). Keys come from
the `CommandBindings` resource, so the app decides them. Does nothing while `CommandUi.enabled` is false.

## Schedule

- **Schedule:** `Update`, exclusive (`fn command_input(world: &mut World)`)
- **Set:** `NpcCommandUiSet`, which is configured `.in_set(ViewSystems::Input)`
- **Ordering:** a host system that sets `RightClickMode` in the frame of the click runs
  `.before(NpcCommandUiSet)`. Siblings: `(prune_selection, sync_selected).chain()` in `ViewSystems::Sync`
  (drop units that left the sim; `Selected` follows the first selected unit), `draw_orders` in
  `ViewSystems::Present` (selection rings, order chains, patrol draft, drag box).

## Reads / writes

| Access | Data |
|---|---|
| reads | `ButtonInput<MouseButton>`, `ButtonInput<KeyCode>`, cursor + camera, `Selectable` / `PickTarget` visuals (`VisualOf` + `GlobalTransform`), `SimIndex`, `Owner`, `CommandUi`, `CommandBindings`, `RightClickMode`, `ContextOrders` / `RightClickOrders` resolvers |
| writes | view resources `CommandSelection`, `ControlGroups`, `DragBox`, `PatrolDraft`, `RightClickMode` |
| messages / events | submits `IssueOrder` as a `SimCommand` (kernel `submit`; applied in `NpcSet::Inputs` of the next open tick) |

## Determinism

View-only on sim state: it never mutates sim components. Orders reach the sim only as recorded
`SimCommand`s (with `skip_invalid` per `SkipInvalidOrders`), so replays see exactly the orders that were
given.

## Performance

- **Mechanic tag:** `view` (`npc_view_helpers::command_ui` has no own prefix, so the `npc_view_helpers`
  fallback matches) · **Budget:** none · no span.

## Related

- [`click_select`](click_select.md) — the single-select used without the command UI; both share `pick`.
- [`order_keys`](order_keys.md) — the testbed's Hold/Stop/Unload and formation keys.
- [`apply_commands_system`](apply_commands_system.md), [`command_engine_system`](command_engine_system.md) — where the orders land.
- Components: [`Selectable`](../components/Selectable.md), [`OrderQueue`](../components/OrderQueue.md),
  [`Owner`](../components/Owner.md)
