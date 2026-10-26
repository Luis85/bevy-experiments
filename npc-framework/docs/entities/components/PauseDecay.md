---
type: Component
---
# `PauseDecay`

**Crate:** `npc_needs` · **File:** [`crates/npc_needs/src/lib.rs`](../../../crates/npc_needs/src/lib.rs) · **Capability:** needs

Pauses the decay of selected needs on one agent, e.g. energy and thirst while asleep. A bit mask over
`NeedId`s: bit `id.0` set means that need does not decay. Levels are still recomputed every tick. It only
gates decay; it is not sim state of its own. `SparseSet` storage, because content inserts and removes it often.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `mask` | `u64` | bit `id.0` set = need `id` does not decay. Covers the first 64 needs only |

Helpers: `PauseDecay::of(&[NeedId])` builds the mask (panics on an id ≥ 64); `pauses(id)` tests a bit
(false for ids ≥ 64).

## Lifecycle

- **Added by:** content actions. The village `Sleep` start inserts `PauseDecay::of(&[energy, thirst])`
  next to its `Sleep` component ([`content/village_content/src/actions.rs`](../../../content/village_content/src/actions.rs)).
- **Read by:** [`decay_needs_system`](../systems/decay_needs_system.md) (as `Option<&PauseDecay>`).
- **Removed by:** the same content action's stop (village sleep stop removes it).

## Hashing and snapshots

- **State hash:** no. It has no `SimHash` impl. The decay it suppresses shows up in the hashed
  [`Needs`](Needs.md) values.
- **World snapshot (ADR 0013):** saved as `pause_decay` (snapshot-only, `register_snapshot_component_named`
  in `NpcNeedsPlugin`), so a sleeping agent keeps its paused needs after a load.

## Related

- [`Needs`](Needs.md) — the values it keeps from decaying.

## Example

```rust
use npc_framework::needs::{NeedRegistry, PauseDecay};

fn start_rest(world: &mut World, agent: Entity) {
    let reg = world.resource::<NeedRegistry>();
    let energy = reg.id("Energy").unwrap();
    world.entity_mut(agent).insert(PauseDecay::of(&[energy]));
}

fn stop_rest(world: &mut World, agent: Entity) {
    world.entity_mut(agent).remove::<PauseDecay>();
}
```
