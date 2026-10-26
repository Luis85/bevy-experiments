---
type: Component
---
# `PerceptionFacts`

**Crate:** `npc_perception` · **File:** [`crates/npc_perception/src/memory.rs`](../../../crates/npc_perception/src/memory.rs) · **Capability:** perception (opt-in, `PerceptionPlugin`)

Opt-in: asks the perception system to mirror the nearest **sensed** percept of some kinds into the agent's
`Blackboard`, so behaviour-tree leaves can read a target id and position without querying memory. It is
written on each of the agent's scan ticks.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `kinds` | `u32` | kind mask the nearest percept must match (`PerceptionMemory::nearest(pos, kinds, true)`) |
| `target_slot` | `u8` | `Blackboard` slot index (0..8) that gets `BbValue::U32(sim_id)` |
| `pos_slot` | `u8` | `Blackboard` slot index that gets `BbValue::Pos(x, z)` (mm) |

When nothing matches, both slots get `BbValue::None`. Slots outside the blackboard are ignored. A slot is only
written when its value changes.

## Lifecycle

- **Added by:** content or the host. No spawn in this repo adds it; only `crates/npc_perception/tests/` do.
- **Read by:** [`perception_system`](../systems/perception_system.md) (`write_facts`, on ticks where
  `is_due(id, tick, stagger)` holds, after the agent's memory update).
- **Written / removed by:** nothing (configuration only). The system writes the `Blackboard`, not this
  component.

## Hashing and snapshots

- **State hash:** yes, as `perception_facts` (`register_sim_component` in `register_perception_hash`,
  [`crates/npc_perception/src/lib.rs`](../../../crates/npc_perception/src/lib.rs)). Encoding: `kinds` (`u32`),
  `target_slot`, `pos_slot` (`u8`).
- **World snapshot (ADR 0013):** saved as `perception_facts` (automatic with hashing).

## Related

- [`PerceptionMemory`](PerceptionMemory.md) — the source of the facts.
- [`Sensors`](Sensors.md) — `PerceptionFacts` does nothing on an entity without it.

## Example

```rust
use npc_framework::perception::{PerceptionFacts, Sensors};

const FOOD: u32 = 1 << 1;
// Slot 0: id of the nearest visible food, slot 1: its position.
world.entity_mut(agent).insert(PerceptionFacts { kinds: FOOD, target_slot: 0, pos_slot: 1 });
```
