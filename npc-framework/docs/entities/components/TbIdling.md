---
type: Component
---
# `TbIdling`

**Crate:** `testbed_content` · **File:** [`content/testbed_content/src/actions.rs`](../../../content/testbed_content/src/actions.rs) · **Capability:** testbed content (actions)

The running `TbIdle` action: stand still for a number of ticks, then `Success`. The testbed trees use it as
the fallback leaf, after a wander or a tool job, and while riding a vehicle. `SparseSet` storage.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `remaining` | `u32` | ticks left; starts at the leaf param (`ParamValue::Int`, default 40, at least 1) |

## Lifecycle

- **Added by:** `idle_start` (action `TbIdle`) in
  [`actions.rs`](../../../content/testbed_content/src/actions.rs). Tree params: `Ride` 20, `Rest` 10,
  `Linger` 60, `Idle` 40 ([`tree.rs`](../../../content/testbed_content/src/tree.rs)).
- **Read / written by:** [`testbed_content.idle_system`](../systems/testbed_content.idle_system.md)
  (decrements; `Success` at 0).
- **Removed by:** `idle_stop`.

## Hashing and snapshots

- **State hash:** yes, as `tb_idling` (`remaining` `u32`; `register_sim_component` in `TestbedPlugin`,
  [`lib.rs`](../../../content/testbed_content/src/lib.rs)).
- **World snapshot (ADR 0013):** saved as `tb_idling` (automatic with hashing).

## Related

- [`TbWalking`](TbWalking.md) — the other testbed action.
- [`Idle`](Idle.md) — the village equivalent (random length).

## Example

```rust
// tree.rs: the last child of every testbed tree.
b.leaf("Idle", TB_IDLE, 40);
```
