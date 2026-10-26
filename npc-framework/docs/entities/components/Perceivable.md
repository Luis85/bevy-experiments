---
type: Component
---
# `Perceivable`

**Crate:** `npc_perception` · **File:** [`crates/npc_perception/src/sensors.rs`](../../../crates/npc_perception/src/sensors.rs) · **Capability:** perception (opt-in, `PerceptionPlugin`)

Marks things that vision or proximity can sense, with a bit set of content kinds (for example bit 0 = unit,
bit 1 = food). Vision and proximity never sense entities without it. Hearing does not need it: a heard noise
with a `source` still creates a percept, with `kinds` taken from the source's `Perceivable` (0 if it has none).

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `kinds` | `u32` | content-defined kind bits; matched against `Sensors::interest`, `PerceptionFacts::kinds` and the `sees::<MASK>` / `remembers::<MASK>` masks |

## Lifecycle

- **Added by:** content spawns. The testbed puts it on units, items, doors, trees, ore, stations and vehicles,
  with bits from `testbed_content::world::kinds`
  ([`content/testbed_content/src/layout.rs`](../../../content/testbed_content/src/layout.rs)).
- **Read by:** [`perception_system`](../systems/perception_system.md). On a tick with a noise or a due
  scan it collects every `(Entity, SimId, SimPos, Perceivable)` (a cached `QueryState`), sorts by `SimId`, and merges that list with the spatial-grid radius
  query. The perception × agent-io glue (`perceived_or_remembered`) treats entities *without* it as known by
  touch within `TOUCH_MM` (1500 mm). View only: `testbed_view` panels.
- **Written / removed by:** nothing in the framework.

## Hashing and snapshots

- **State hash:** yes, as `perceivable` (`register_sim_component` in `register_perception_hash`,
  [`crates/npc_perception/src/lib.rs`](../../../crates/npc_perception/src/lib.rs)). Encoding: `kinds` as `u32`.
- **World snapshot (ADR 0013):** saved as `perceivable` (automatic with hashing).

## Related

- [`Sensors`](Sensors.md) — the observer side.
- [`Stealthy`](Stealthy.md) — makes a target harder to detect with the `per_roll` check.
- [`PerceptionMemory`](PerceptionMemory.md) — stores the `kinds` seen at observation time.

## Example

```rust
use npc_framework::perception::{Perceivable, conds::sees};

const FOOD: u32 = 1 << 1;
spawn_sim(world, (SimPos::new(3_000, 0), Perceivable::new(FOOD)));
// In Plugin::build: a condition for behaviour trees.
app.register_cond("sees_food", sees::<FOOD>);
```
