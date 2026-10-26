---
type: Component
---
# `Needs`

**Crate:** `npc_needs` · **File:** [`crates/npc_needs/src/lib.rs`](../../../crates/npc_needs/src/lib.rs) · **Capability:** needs (always in `NpcPlugins`)

An agent's needs: one `Need` per registered `NeedDef`, indexed by `NeedId`. Content registers each need once
(`register_need(NeedDef { .. })`); at freeze the defs are sorted by name and get dense ids, so
`values[id.0]` is the need with that id. Values are satisfaction: `NEED_MAX` (100 000) is full, 0 is depleted.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `values` | `Vec<Need>` | one entry per `NeedDef`, in `NeedId` order (name order, not `NeedDef::order`) |

`Need` (not a component):

| Field | Type | Unit / meaning |
|---|---|---|
| `value` | `i32` | milli-units, `0..=NEED_MAX` (`NEED_MAX = 100_000`) |
| `level` | `NeedLevel` | `Ok` / `Low` / `Urgent`, recomputed with hysteresis by `update_level` |

The `NeedDef` that governs each entry: `name` (stable, used by `SetNeed` and the journal), `key` (JSON field
name), `order` (presentation only, never ids or hash), `decay_per_tick` (milli-units per tick) and
`thresholds` (`low_enter`/`low_exit`/`urgent_enter`/`urgent_exit`: a worse level is entered when
`value < *_enter` and left only when `value > *_exit`).

## Lifecycle

- **Added by:** content spawn bundles, normally `world.resource::<NeedRegistry>().full()` (all `NEED_MAX`,
  level `Ok`), e.g. `villager_bundle` in [`content/village_content/src/scenario.rs`](../../../content/village_content/src/scenario.rs).
- **Read by:** [`decay_needs_system`](../systems/decay_needs_system.md); content guards and utility scoring
  (village `villager.rs`, `agent.rs`); inspect/overlay views (`needs_json`).
- **Written by:** [`decay_needs_system`](../systems/decay_needs_system.md) (decay + level); the core
  `SetNeed { unit, need, value }` command (via `SetNeedHandler`, `set_need_apply`: sets `value` only; the
  level is recomputed by the decay pass later in the same tick, since `Inputs` runs before `Needs`); content actions that restore needs (village eat, drink, sleep in
  [`content/village_content/src/actions.rs`](../../../content/village_content/src/actions.rs), using
  [`Provides`](Provides.md)).

## Hashing and snapshots

- **State hash:** yes, as `needs` (`register_sim_component_with::<Needs>(needs_json)` in `NpcNeedsPlugin`).
  Encoding: length (u32), then per need `value` (i32) and level (u8: Ok 0, Low 1, Urgent 2).
- **World snapshot (ADR 0013):** saved as `needs` (automatic with hashing).

## Related

- [`PauseDecay`](PauseDecay.md) — gates decay per need.
- [`Provides`](Provides.md) — places that restore a need.
- `NeedRegistry` (resource) — the frozen `NeedDef`s; `id(name)`, `def(id)`, `full()`, `display_order()`.

## Example

```rust
use npc_framework::needs::{NeedId, NeedRegistry, Needs};
use npc_framework::prelude::{SimPos, spawn_sim};

// Spawn with full needs.
let needs = world.resource::<NeedRegistry>().full();
let (_id, agent) = spawn_sim(world, (SimPos::new(0, 0), needs));

// Read one need by name.
let hunger: NeedId = world.resource::<NeedRegistry>().id("Hunger").unwrap();
let value = world.get::<Needs>(agent).unwrap().get(hunger).value; // milli-units
```

Command JSON: `{"type": "SetNeed", "unit": 5, "need": "Hunger", "value": 20000}`.
