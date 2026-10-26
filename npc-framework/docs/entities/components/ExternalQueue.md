---
type: Component
---
# `ExternalQueue`

**Crate:** `npc_agent_io` · **File:** [`crates/npc_agent_io/src/control.rs`](../../../crates/npc_agent_io/src/control.rs) · **Capability:** agent control (agent-io)

A `Direct` unit's action queue. Each queued action is a list of `Step`s, mostly registered behaviour-tree
actions run by name (their own systems advance them, as when the tree runs them). The injected `ExternalAction`
leaf works through it. The leaf has no action component of its own: its state is this queue plus `ActiveLeaf`.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `pending` | `VecDeque<QueuedAction>` | Waiting actions, front = next. At most `MAX_QUEUE` (64). |
| `current` | `Option<RunningAction>` | The running action: `item: QueuedAction`, `step: u32` (index), `step_running: bool`, `wait_left: u32` (ticks left of a `Wait`). |
| `cancel_current` | `bool` | Set by a cancel / clear / replace command; the next system pass aborts the current action. |

`QueuedAction { handle: u32, verb: String, label: String, steps: Vec<Step> }`; `handle` is what `agent/act`
returned. `Step` is `Action { action: String, param: ParamValue }`, `SetBlackboard { key: u8, value: BbValue }`
(key `0..8`, instant) or `Wait { ticks: u32 }` (at most `MAX_WAIT_TICKS` = 1 000 000). 1..=`MAX_STEPS` (32)
steps per action. `busy()` is `current.is_some() || !pending.is_empty()`.

## Lifecycle

- **Added by:** `attach_now` in `Direct` mode; `AgentAct::apply` when missing.
- **Written by:** `AgentAct::apply` (push, or with `replace` drain `pending` and set `cancel_current` when an action is running),
  `AgentCancel::apply`, `AgentClear::apply`; `advance` / `abort_current` / `cancel_all`, called from
  `external_action_start` / `external_action_stop` and from
  [`external_action_system`](../systems/external_action_system.md).
- **Read by:** `cond_external_control` (`Autopilot` holds only while `busy()`); `AgentAct::validate` (queue
  full, with `with_same_tick_acts`); `api::busy_reason`, `agent/act`, the `self` observation section
  (`observe::queue_json`).
- **Removed by:** [`external_action_system`](../systems/external_action_system.md) when the unit is no longer
  `Direct` (detached or re-attached as `Advisor` / `Observer`), after cancelling everything.

## Hashing and snapshots

- **State hash:** yes, as `external_queue`: `SimHashRegistry::add_component` directly in `AgentIoPlugin::build`
  ([`crates/npc_agent_io/src/lib.rs`](../../../crates/npc_agent_io/src/lib.rs)); pending items, current item
  with `step`, `step_running`, `wait_left`, then `cancel_current`.
- **World snapshot (ADR 0013):** saved as `external_queue`, registered separately with
  `register_snapshot_component_named` in the same `build` (the type does not derive `Reflect`, which
  `register_sim_component` needs).

## Related

- [`ExternalController`](ExternalController.md) — must be `Direct` for the queue to exist.
- [`external_dispatch_system`](../systems/external_dispatch_system.md),
  [`external_advance_system`](../systems/external_advance_system.md) — the passes that advance it.
- Journal kind `ExternalAction { handle, verb, status, latency_ticks }` (`queued`, `started`, `done`, `failed`,
  `cancelled`, `rejected`).

## Example

```json
{"tick": 120, "cmd": {"type": "AgentAct", "unit": 5, "controller": "agent", "handle": 3,
  "verb": "go", "label": "walk to the well", "replace": false,
  "steps": [{"type": "SetBlackboard", "key": 1, "value": {"Pos": [4000, 2000]}},
            {"type": "Action", "action": "MoveToDest", "param": "None"},
            {"type": "Wait", "ticks": 100}]}}
```
