---
type: Component
---
# `ExternalController`

**Crate:** `npc_agent_io` · **File:** [`crates/npc_agent_io/src/control.rs`](../../../crates/npc_agent_io/src/control.rs) · **Capability:** agent control (agent-io)

The external controller (an agent: Claude Code through `npcctl`, an MCP client, a script) attached to a unit.
Mutating agent commands must name its `id` (`owned` check). The mode decides how the agent drives the unit.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `id` | `String` | The controller's id, 1..=64 characters. |
| `mode` | `ControlMode` | `Direct` (default: the unit's tree is suspended by the injected `ExternalControl` branch; the agent picks every action), `Advisor` (the tree keeps running; the agent gives orders), `Observer` (read-only). |
| `on_idle` | `IdlePolicy` | What a `Direct` unit does with an empty queue: `Idle` (default: stand still) or `Autopilot` (fall back to its own tree). |

## Lifecycle

- **Added by:** `attach_now`, applied by the recorded `AttachController` command, and called directly by
  scenario loaders to pre-attach before tick 0 ([`scenario.rs`](../../../crates/npc_agent_io/src/scenario.rs)).
  `Direct` needs a `Brain`. Attaching a different controller (with `force`) cancels the old queue. Attach also
  inserts an [`ExternalQueue`](ExternalQueue.md) in `Direct` mode and a `ThinkRequest` when the unit has a `Brain`.
- **Read by:** the `external_control` condition (`cond_external_control`); `external_action_start`;
  [`external_action_system`](../systems/external_action_system.md) (both passes; a unit no longer `Direct`
  loses its queue); `owned` (validation of `DetachController`, `AgentAct`, `AgentCancel`, `AgentClear`);
  `api.rs` (`agent/observe`, `agent/act`, `agent/list_units`), `observe::ctx` (`AgentCtx::controller`),
  `events.rs`; `journal_skipped_commands`; the `testbed_view` overlay.
- **Removed by:** `DetachController::apply` (also inserts `ThinkRequest` so the tree takes over).

## Hashing and snapshots

- **State hash:** yes, as `external_controller`: added with `SimHashRegistry::add_component` directly in
  `AgentIoPlugin::build` ([`crates/npc_agent_io/src/lib.rs`](../../../crates/npc_agent_io/src/lib.rs)): `id`,
  `mode` and `on_idle` as `u8`.
- **World snapshot (ADR 0013):** saved as `external_controller`, registered separately with
  `register_snapshot_component_named` in the same `build` (not through `register_sim_component`, which
  needs `Reflect`; this type does not derive it).

## Related

- [`ExternalQueue`](ExternalQueue.md) — the `Direct` unit's action queue.
- [`external_dispatch_system`](../systems/external_dispatch_system.md),
  [`external_advance_system`](../systems/external_advance_system.md) — the two passes that run the queue.
- Journal kinds `ControllerAttached { controller, mode, on_idle }` / `ControllerDetached { controller }`.

## Example

```json
{"tick": 0, "cmd": {"type": "AttachController", "unit": 5, "controller": "agent",
                    "mode": "Direct", "on_idle": "Idle", "force": false}}
```
