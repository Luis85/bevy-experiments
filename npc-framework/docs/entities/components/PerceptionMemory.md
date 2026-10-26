---
type: Component
---
# `PerceptionMemory`

**Crate:** `npc_perception` · **File:** [`crates/npc_perception/src/memory.rs`](../../../crates/npc_perception/src/memory.rs) · **Capability:** perception (opt-in, `PerceptionPlugin`)

What an agent perceived: a bounded map of percepts by `SimId`, plus the latest heard noise. Only
[`perception_system`](../systems/perception_system.md) writes it. A percept is forgotten
`PerceptionConfig::forget_after_ticks` (default 200) after it was last sensed. Beyond
`PerceptionConfig::memory_capacity` (default 32) the oldest percepts are evicted.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `percepts` | `BTreeMap<SimId, Percept>` | remembered things, ascending `SimId` |
| `last_noise` | `Option<HeardNoise>` | the latest noise heard |

`Percept`:

| Field | Type | Unit / meaning |
|---|---|---|
| `last_seen_tick` | `u64` | tick it was last sensed, by any sense |
| `pos` | `SimPos` | where it was then (mm); the noise's position for hearing |
| `via` | `SenseKind` | `Vision`, `Proximity` or `Hearing` (that order is the priority) |
| `confidence` | `u16` | permille, from the detection check (`CONFIDENCE_FULL` = 1000) |
| `kinds` | `u32` | its `Perceivable` kinds when observed (0 if none, e.g. heard only) |
| `sensed` | `bool` | sensed by vision or proximity at the agent's latest scan |

`HeardNoise`: `tick: u64`, `pos: SimPos`, `source: Option<SimId>`, `kind: u32`.

Queries: `get(id)`, `sensed()`, `senses_kind(mask)`, `remembers_kind(mask)`, `nearest(from, mask, sensed_only)`
(ties: lower `SimId`), `to_json()`. `Percept::confidence_at(now, forget_after_ticks)` decays confidence
linearly to 0 at the forget age.

## Lifecycle

- **Added by:** required by [`Sensors`](Sensors.md), so it starts empty.
- **Written by:** [`perception_system`](../systems/perception_system.md) only. A heard noise sets `last_noise`
  and, if it has a source, inserts or refreshes a hearing percept (it does not overwrite a percept that is
  currently `sensed`). On the agent's stagger tick the scan replaces sensed percepts, clears `sensed` on the
  rest (journal `Lost { forgotten: false }`) and drops expired ones; the capacity is enforced on every pass
  (`Lost { forgotten: true }`). New (or newly sensed by the scan) percepts journal `Perceived { target, via }`.
- **Read by:** conditions `sees_kind`, `remembers_kind`, `heard_noise` and the generic `sees`, `remembers`,
  `heard` ([`conds.rs`](../../../crates/npc_perception/src/conds.rs)); the system's `PerceptionFacts` pass;
  the `per_roll` check ([`perception_rules.rs`](../../../crates/npc_framework/src/glue/perception_rules.rs):
  an already sensed target keeps its confidence unless `reroll_sensed`); the perception × agent-io glue
  (`perception` section and known provider,
  [`perception_agent.rs`](../../../crates/npc_framework/src/glue/perception_agent.rs)); `perception_json`.
  View only: `perception_gizmos`, `testbed_view` F6 panel.

## Hashing and snapshots

- **State hash:** yes, as `perception_memory` (`register_sim_component_with` in `register_perception_hash`,
  [`crates/npc_perception/src/lib.rs`](../../../crates/npc_perception/src/lib.rs); the JSON fn is
  `PerceptionMemory::to_json`, used for inspect and diffs). Every percept field is hashed, then `last_noise`.
- **World snapshot (ADR 0013):** saved as `perception_memory` (automatic with hashing; snapshots use the serde
  form, not `to_json`).

## Related

- [`Sensors`](Sensors.md), [`Perceivable`](Perceivable.md) — inputs.
- [`PerceptionFacts`](PerceptionFacts.md) — mirrors the nearest sensed percept to the blackboard.

## Example

```rust
use npc_framework::perception::PerceptionMemory;

const FOOD: u32 = 1 << 1;
let here = *world.get::<SimPos>(agent).unwrap();
if let Some(mem) = world.get::<PerceptionMemory>(agent)
    && let Some((food, p)) = mem.nearest(here, FOOD, false) // also remembered, not only seen now
{
    println!("food #{} last seen at tick {} via {}", food.0, p.last_seen_tick, p.via.name());
}
```
