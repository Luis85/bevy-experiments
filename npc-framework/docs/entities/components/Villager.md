---
type: Component
---
# `Villager`

**Crate:** `village_content` · **File:** [`content/village_content/src/world.rs`](../../../content/village_content/src/world.rs) · **Capability:** village content

The villager marker. It names the house the villager lives in and its walking speed. Its
`#[require(CurrentOrder)]` gives every villager an empty kernel `CurrentOrder`, so villagers can take RTS
orders. Needs, brain and position are separate components (see the example).

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `home` | `SimId` | the `Home` place it lives in (its bed is picked from beds whose `Bed.home` matches) |
| `speed` | `i32` | walking speed, mm per tick (scenarios use `VILLAGER_SPEED` = 60, i.e. 1.2 m/s) |

## Lifecycle

- **Added by:** `villager_bundle` in [`scenario.rs`](../../../content/village_content/src/scenario.rs)
  (used by `spawn_plot` and by the `SpawnVillager` command in
  [`commands.rs`](../../../content/village_content/src/commands.rs)). Pulls in `CurrentOrder`.
- **Read by:** `reserve_bed_start` (home), `pick_wander_point_start` (home approach) and `start_walk`
  (speed) in [`actions.rs`](../../../content/village_content/src/actions.rs); the villager conditions in
  [`villager.rs`](../../../content/village_content/src/villager.rs); `kind_name` (`"villager"`); the agent
  sections in [`agent.rs`](../../../content/village_content/src/agent.rs); the view
  ([`village_view.spawn_visuals`](../systems/village_view.spawn_visuals.md),
  [`villager_transforms`](../systems/villager_transforms.md),
  [`village_view.update_text`](../systems/village_view.update_text.md)); `npcctl`'s day summary
  (`apps/npcctl/src/sim.rs`).
- **Written / removed by:** nothing changes it after spawn.

## Hashing and snapshots

- **State hash:** no (no `SimHash` impl; identity data only).
- **World snapshot (ADR 0013):** saved as `villager` (snapshot-only, `register_snapshot_component_named` in
  `register_snapshot_entries`, [`lib.rs`](../../../content/village_content/src/lib.rs)). Also reflected
  (`register_type`, BRP type path `npc_sim::world`).

## Related

- [`Home`](Home.md), [`Bed`](Bed.md) — where it lives and sleeps.
- [`Activity`](Activity.md) — what it is visibly doing.
- [`OrderMove`](OrderMove.md) — set while it follows a `GoTo` order.

## Example

```rust
// scenario.rs: the bundle every villager is spawned with.
let bundle = (
    Villager { home: house, speed: VILLAGER_SPEED },
    pos,
    PrevSimPos(pos),
    world.resource::<NeedRegistry>().full(),
    Activity::Idle,
    Brain { tree },
);
spawn_sim(world, bundle);
// At runtime: {"type":"SpawnVillager","pos":[0,0],"home":1}
```
