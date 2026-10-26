---
type: System
---
# `crafting_complete_system`

**Crate:** `npc_crafting` · **File:** [`crates/npc_crafting/src/run.rs`](../../../crates/npc_crafting/src/run.rs) · **Plugin:** `CraftingPlugin`

Finishes every craft whose duration is over (`Crafting::ends <= SimClock.tick`) by calling `complete_craft`.
That removes [`Crafting`](../components/Crafting.md), re-checks the **claimed** plan (`check_claimed`: same
recipe, workstation and skill checks, and the planned tools/inputs still carried and sufficient; otherwise
`invalid`), rolls the skill (`npc_rules::roll_skill`, purpose `craft.<recipe>`, modifier = recipe modifier +
the tools' `Item::quality` sum; no `CharacterSheet` or no skill on the recipe = margin-0 success), and applies
the result:

- **success / critical success:** inputs consumed; outputs spawned at quality `QualityRule::quality(margin,
  crit)` into the inventory, or on the ground when it does not fit; an order counts down.
- **failure:** time spent, nothing consumed. **critical failure:** inputs consumed, no output, tools worn by
  `crit_fail_wear`.

Every attempt wears each tool ([`Durability`](../components/Durability.md); broken tools despawn). Then all
claims are released, the `CraftingHooks` run in name order with a `CraftCompleted` (the facade's
`progression_crafting` glue awards XP), and a `Tree` craft's `ActiveLeaf` gets `Success`/`Failure`.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcCraftingSet::Complete` (configured `.after(NpcInteractionSet::Complete).before(NpcSet::Effects)`);
  exclusive (`crafting_complete_system(world: &mut World, crafters: &mut CrafterQuery)`, a cached
  `QueryState<(&SimId, Entity, &Crafting)>`)
- **Ordering:** only the set constraints.

## Reads / writes

| Access | Data |
|---|---|
| reads | `SimClock`, `SimId`, `Crafting`, `RecipeRegistry`, `CraftingConfig`, `CraftingHooks`, `Workstation`, `Inventory`, `Equipment`, `CarriedBy`, `Item`, `CharacterSheet`, `SimPos`, `Reservations` |
| writes | removes `Crafting`; `Item.stack` / `Item.quality`; `Durability`; despawns used-up inputs and broken tools; spawns output `Item`s; `Inventory`/`CarriedBy`; `CraftingOrders`; `Reservations`; `ActiveLeaf.status` |
| messages / events | journal `CraftDone`, `CraftFailed { outcome: "failed" / "critical" / "invalid" }`, `Roll`, `Release`, `Spawn` |

## Determinism

Due crafters are collected as `(SimId, Entity)`, sorted by `SimId`, and each is skipped if it left the sim
meanwhile. Plans keep tools and inputs in recipe order with items per key in `SimId` order; quality and
wear are integer math; the roll is counter-based (`craft.<recipe>`).

## Performance

- **Mechanic tag:** `crafting` (via `tags.rs` prefix `npc_crafting`) · **Budget:** 15 µs + 2 µs/agent
  (`crafting` row) · opens `info_span!("mechanic", name = "crafting")` (nested in the system's own `crafting`-tagged span, so `PerfLayer` does not time it a second time).

## Related

- [`complete_interactions_system`](complete_interactions_system.md) — runs just before.
- Components: [`Crafting`](../components/Crafting.md), [`CraftLeaf`](../components/CraftLeaf.md), [`Workstation`](../components/Workstation.md), [`CraftingOrders`](../components/CraftingOrders.md), [`Durability`](../components/Durability.md)
