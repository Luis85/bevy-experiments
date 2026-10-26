---
type: Component
---
# `Blackboard`

**Crate:** `npc_ai` · **File:** [`crates/npc_ai/src/runtime.rs`](../../../crates/npc_ai/src/runtime.rs) · **Capability:** AI (behavior trees)

A small fixed-slot store an agent's leaves use to pass data to each other (a target id chosen by one leaf, a
destination read by the next). Eight slots, integer values only. Content picks the slot meanings
(`BbKey` constants such as the village's `BB_TARGET`, `BB_DEST`).

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `slots` | `[BbValue; 8]` | indexed by `BbKey(u8)` (0..=7; a larger key panics) |

`BbValue` variants: `None` (default), `U32(u32)` (e.g. a `SimId`'s number), `I32(i32)`, `Pos(i32, i32)`
(x, z in mm). Access with `get(key)` / `set(key, value)`.

## Lifecycle

- **Added by:** required by [`Brain`](Brain.md) (all slots `None`).
- **Written by:** action `start` functions and action systems (village `actions.rs`/`orders.rs`, testbed
  `actions.rs`, `npc_interaction::leaf`, `npc_crafting::leaf`, `npc_vehicles::drive`, `npc_agent_io::control`),
  the perception pass (`crates/npc_perception/src/system.rs` writes the facts configured in `memory.rs`).
- **Read by:** the same, conditions, the inspector, the village F1 overlay.
- `npc_ai::think` never touches it.

## Hashing and snapshots

- **State hash:** yes, as `blackboard` (`hashed.add_component::<Blackboard>()` in
  [`schedule.rs`](../../../crates/npc_kernel/src/schedule.rs); per slot a `u8` tag 0..=3 then its ints).
- **World snapshot (ADR 0013):** saved as `blackboard` (`register_kernel_entries`,
  [`snapshot.rs`](../../../crates/npc_kernel/src/snapshot.rs)).

## Related

- [`Brain`](Brain.md) — requires it.
- [`ActiveLeaf`](ActiveLeaf.md) — leaves that read and write it.

## Example

```rust
const BB_TARGET: BbKey = BbKey(0);

fn pick_target(world: &mut World, agent: Entity, target: SimId) {
    if let Some(mut bb) = world.get_mut::<Blackboard>(agent) {
        bb.set(BB_TARGET, BbValue::U32(target.0));
    }
}
```
