---
type: Component
---
# `NavState`

**Crate:** `npc_nav` · **File:** [`crates/npc_nav/src/lib.rs`](../../../crates/npc_nav/src/lib.rs) · **Capability:** nav

Where a [`NavAgent`](NavAgent.md)'s navigation stands: no intent, a queued request, following a path, or a
failed request. Managed by the plugin; extenders read it, for example to wait for a path or react to a
failure.

## Variants

| Variant | Fields | Meaning |
|---|---|---|
| `Idle` (default) | none | no `MoveIntent` |
| `Pending` | `goal: SimPos`, `tick: u64` | a `PathRequest` for `goal` has been queued since `tick` (`SimClock` tick) |
| `Following` | `goal: SimPos` | walking a [`Path`](Path.md) to `goal` |
| `Failed` | `goal: SimPos`, `reason: PathFailReason` | the last request failed (`StartBlocked`, `GoalBlocked`, `OutOfBounds`, `Unreachable`, `SearchLimit`); retried on any `GridChanged` or by `retry_failed_path` |

`goal()` returns the goal of any non-`Idle` state.

## Lifecycle

- **Added by:** the `#[require(NavState)]` on `NavAgent` (as `Idle`).
- **Written by:** [`nav_plan_system`](../systems/nav_plan_system.md) (every transition: new or changed goal →
  `Pending`; served → `Following` or `Failed`; no intent → `Idle`). Also `retry_failed_path`
  (`Failed` → `Pending`, called by `npc_interaction`'s `key_replan_system`), and `npc_command` group
  followers (`exec.rs` inserts `Following` with a path derived from the leader's).
- **Read by:** [`grid_path_mover_system`](../systems/grid_path_mover_system.md) (`Following` + `Path` → move;
  `Failed` → `Blocked`; otherwise `Moving`); `npc_command` `follow_leader`; `npc_interaction` `key_replan_system`.

## Hashing and snapshots

- **State hash:** yes, as `nav_state` (`register_sim_component` in `NavPlugin`). Encoding: u8 discriminant
  (0..=3), then `goal`, plus `tick` (u64) for `Pending` and the reason (u8) for `Failed`.
- **World snapshot (ADR 0013):** saved as `nav_state` (automatic with hashing).

## Related

- [`Path`](Path.md) — present only while `Following` (`nav_plan_system` removes it on every other transition).
- [`MoveStatus`](MoveStatus.md) — `Pending` reports `Moving`, `Failed` reports `Blocked { ticks }`.

## Example

```rust
use npc_framework::nav::NavState;

match world.get::<NavState>(agent) {
    Some(NavState::Failed { reason, .. }) => info!("no path: {reason:?}"),
    Some(NavState::Pending { tick, .. }) => { /* queued since `tick`, still Moving */ }
    _ => {}
}
// After changing what a pass-cost hook decides for this agent (e.g. it got a key):
npc_framework::nav::retry_failed_path(world, agent);
```
