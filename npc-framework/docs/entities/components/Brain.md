---
type: Component
---
# `Brain`

**Crate:** `npc_ai` · **File:** [`crates/npc_ai/src/runtime.rs`](../../../crates/npc_ai/src/runtime.rs) · **Capability:** AI (behavior trees)

Which shared behavior tree an agent runs. An entity with `Brain` and a `SimId` is an *agent*: it thinks in
`NpcSet::Think` and gets re-think requests from the clock systems. Trees are registered by name
(`register_tree(name, build)`) and frozen sorted by name, so `TreeId`s are stable for a given content set.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `tree` | `TreeId` (`u16`) | index into `AiRegistry` trees (sorted by tree name); look it up with `AiRegistry::tree_id(name)` |

`#[require(BrainState, Blackboard, BrainStats)]`: inserting `Brain` adds those three with defaults.

## Lifecycle

- **Added by:** content spawn bundles (`villager_bundle` in `content/village_content/src/scenario.rs`, the
  testbed layout); snapshot load (custom fn, by tree name).
- **Read by:** [`think_system`](../systems/think_system.md) (`With<Brain>` selects agents) and `npc_ai::think`
  (picks the `TreeDef`); [`day_night_system`](../systems/day_night_system.md) and
  [`rethink_timer_system`](../systems/rethink_timer_system.md) (`With<Brain>`); `drain_ai_trace` (node names);
  `npc_command::exec`, the inspector.
- **Written / removed by:** content that swaps trees at run time (then call `register_brain_hash`, see below).
  `think` stops a stale `ActiveLeaf` with `Aborted` before starting a leaf of the new tree.

## Hashing and snapshots

- **State hash:** **off by default.** Opt in with `npc_kernel::hash::register_brain_hash(app)` before the
  freeze; it is then hashed as `brain` (`u16` tree id). Opting in changes the content hash and every recorded
  hash. Content whose units switch trees should opt in.
- **World snapshot (ADR 0013):** saved as `brain` with custom fns that store the **tree name**, not the id
  (`save_brain`/`load_brain` in [`snapshot.rs`](../../../crates/npc_kernel/src/snapshot.rs)). An unknown tree
  name fails a strict load (a lenient load skips it).
- Not `Reflect` (npc_ai depends on `bevy_ecs` only).

## Related

- [`BrainState`](BrainState.md), [`Blackboard`](Blackboard.md), [`BrainStats`](BrainStats.md) — required companions.
- [`ActiveLeaf`](ActiveLeaf.md) — the running leaf. [`ThinkRequest`](ThinkRequest.md) — forces a re-think.

## Example

```rust
app.register_tree("guard", |t| { /* TreeBuilder: selector, guards, leaves by name */ });
// After the freeze (Plugin::finish / ensure_frozen):
let tree = world.resource::<AiRegistry>().tree_id("guard").expect("registered");
spawn_sim(world, (SimPos::new(0, 0), Brain { tree }));
```
