---
type: Component
---
# `Idle`

**Crate:** `village_content` · **File:** [`content/village_content/src/actions.rs`](../../../content/village_content/src/actions.rs) · **Capability:** village content (actions)

A villager standing still for a while (leaf `IdleFor`, the end of the `Wander` branch). The length is drawn
with the counter-based RNG, so it is deterministic. `SparseSet` storage.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `remaining` | `u32` | ticks left; starts at `sim_range(seed, me, tick, RNG_IDLE, 60, max(param, 61))`, i.e. `60..param` |

## Lifecycle

- **Added by:** `idle_for_start` (param `ParamValue::Int`, the tree uses 200) in
  [`actions.rs`](../../../content/village_content/src/actions.rs). Also sets `Activity::Idle`.
- **Read / written by:** [`village_content.idle_system`](../systems/village_content.idle_system.md)
  (decrements; `Success` at 0).
- **Removed by:** `stop_idle`.

## Hashing and snapshots

- **State hash:** yes, as `idle` (`remaining` as `u32`; `register_sim_component` in `register_components`).
- **World snapshot (ADR 0013):** saved as `idle` (automatic with hashing).

## Related

- [`Activity`](Activity.md) — `Idle` meanwhile (also the value after every other action stops).
- [`TbIdling`](TbIdling.md) — the testbed's equivalent.

## Example

```rust
// Tree (villager.rs): the fallback branch.
b.sequence("Wander", |b| {
    b.leaf("PickWanderPoint", "PickWanderPoint", ());
    b.leaf("MoveToDest", "MoveToDest", ());
    b.leaf("IdleFor", "IdleFor", 200);
});
```
