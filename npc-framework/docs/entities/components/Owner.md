---
type: Component
---
# `Owner`

**Crate:** `npc_command` · **File:** [`crates/npc_command/src/types.rs`](../../../crates/npc_command/src/types.rs) · **Capability:** command (RTS orders)

Says which player may order a unit or a squad. Every `IssueOrder`, `CreateSquad` and `DisbandSquad` names a
`PlayerId` and is rejected unless it matches the target's `Owner` (`check_owner` in
[`command.rs`](../../../crates/npc_command/src/command.rs)). The check runs when the command is submitted and
again when it applies.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `0` | `PlayerId` (`pub u32`, serializes as a plain number) | The owning player. Default `PlayerId(0)`. |

## Lifecycle

- **Added by:** content spawn bundles (e.g. `testbed_content::layout`, `rts_units_demo::sim`). It is a
  required component of [`Commandable`](Commandable.md) and [`Squad`](Squad.md) (`#[require(.., Owner)]`), so
  a unit spawned without one gets `Owner(PlayerId(0))`. `create_squad` spawns a squad with `Owner(player)`.
- **Read by:** `check_owner` / `owner_of` (validation of `IssueOrder`, `CreateSquad`, `DisbandSquad`);
  the command × agent-io glue (`command_section`, `command_affordances`, `order_command`, `owner_info`) in
  [`glue/command_agent.rs`](../../../crates/npc_framework/src/glue/command_agent.rs), where the acting unit's
  owner is the player of the agent's orders; `npc_view_helpers::command_ui` (selection, context orders);
  `testbed_view` overlay.
- **Written / removed by:** nothing in the framework after spawn. The command engine never reads it.

## Hashing and snapshots

- **State hash:** yes, as `command_owner` (`register_sim_component` in `register_command_hash`,
  [`crates/npc_command/src/hash.rs`](../../../crates/npc_command/src/hash.rs)). Only with `CommandPlugin`.
- **World snapshot (ADR 0013):** saved as `command_owner` (automatic with hashing).

## Related

- [`Commandable`](Commandable.md) — requires it.
- [`Squad`](Squad.md) — requires it; squads are owned like units.
- [`command_engine_system`](../systems/command_engine_system.md) — runs the orders the owner gave.

## Example

```rust
use npc_framework::command::{Commandable, Owner, PlayerId, IssueOrder, UnitOrder, submit_order};

world.spawn((SimId(5), SimPos::new(0, 0), Commandable::new(120), Owner(PlayerId(1))));
// Player 1 may order unit 5; player 0 would be rejected ("unit 5 is owned by player 1, not player 0").
submit_order(world, &IssueOrder::new(PlayerId(1), [SimId(5)], UnitOrder::Move { pos: (8000, 0) }))?;
```
