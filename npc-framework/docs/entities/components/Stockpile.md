---
type: Component
---
# `Stockpile`

**Crate:** `village_content` · **File:** [`content/village_content/src/world.rs`](../../../content/village_content/src/world.rs) · **Capability:** village content

The village's food store. Work shifts add food, meals take it. Each food unit is one reservation slot, so two
villagers never count on the same unit. It changes only through `SimEffect` messages (applied in
`NpcSet::Effects`) and the `SetFood` command.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `food` | `u32` | food units now (scenario start: `STOCKPILE_START_FOOD` = 4) |
| `capacity` | `u32` | upper bound for `food` (scenario: `STOCKPILE_CAPACITY` = 10) |

## Lifecycle

- **Added by:** `spawn_plot` in [`scenario.rs`](../../../content/village_content/src/scenario.rs), with
  `Place { kind: Stockpile }`, `Approach` and `Provides { need: hunger, amount_per_tick:
  STOCKPILE_HUNGER_PER_TICK }` (1000).
- **Read by:** `find_food` / `available_food_for` (lowest-`SimId` pile with a slot free for the agent),
  `find_work_site` (prefers sites whose pile is below capacity) in
  [`villager.rs`](../../../content/village_content/src/villager.rs); `reserve_food_start` (capacity = `food`),
  `eat_food_start` and [`eat_system`](../systems/eat_system.md) (fails on `food == 0`); the inspector
  (`UnitSnapshot.stockpile`, [`inspect.rs`](../../../content/village_content/src/inspect.rs)); agent sections;
  the view: `update_sacks` (sacks = `min(food, 10)`) and the F1 overlay line `stockpile #id food f/c`.
- **Written by:** [`resolve_effects_system`](../systems/resolve_effects_system.md) (`AddFood` clamps to
  `capacity`, `TakeFood` saturates at 0); `SetFood` command (`{"type":"SetFood","stockpile":3,"food":5}`,
  validated `food <= capacity`) in [`commands.rs`](../../../content/village_content/src/commands.rs).

## Hashing and snapshots

- **State hash:** yes, as `stockpile` (`food` then `capacity`, `u32` each; `register_sim_component` in
  `register_components`, [`lib.rs`](../../../content/village_content/src/lib.rs)).
- **World snapshot (ADR 0013):** saved as `stockpile` (automatic with hashing). BRP type path `npc_sim::world`.

## Related

- [`WorkSite`](WorkSite.md) — fills it. [`Eat`](Eat.md) — empties it.
- [`Provides`](Provides.md) — the hunger rate of a meal.

## Example

```rust
// Read it from a view or a test (never write it from the view).
for (id, pile) in &stockpiles {
    println!("#{} food {}/{}", id.0, pile.food, pile.capacity);
}
```
