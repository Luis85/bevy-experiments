---
type: Component
---
# `Commandable`

**Crate:** `npc_command` · **File:** [`crates/npc_command/src/types.rs`](../../../crates/npc_command/src/types.rs) · **Capability:** command (RTS orders)

Marks a unit that takes RTS orders and says how it moves while running them. It requires an
[`OrderQueue`](OrderQueue.md) and an [`Owner`](Owner.md) (`#[require(OrderQueue, Owner)]`). Units without a
`Brain` run their orders directly; units with a `Brain` run them only while their tree runs the
`ExecuteOrder` leaf (see [`ExecutingOrder`](ExecutingOrder.md)).

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `speed_mm_per_tick` | `i32` | Speed of order moves, mm per tick (written into `MoveIntent`). Default 100. |
| `arrive_mm` | `i32` | How close (mm) a `Move` / `Patrol` point counts as reached (the `MoveIntent` tolerance). Default 0. |

`Commandable::new(speed)` sets the speed and `arrive_mm: 0`.

## Lifecycle

- **Added by:** content spawn bundles (`testbed_content::layout` for units and vehicles,
  `rts_units_demo::sim`). The command × vehicles glue docs ask for it on `Direct` and `ByCrew` vehicles too.
- **Read by:** [`command_engine_system`](../systems/command_engine_system.md) (its query is
  `With<Commandable>`; speed and `arrive_mm` for every built-in walk); `check_owner` and `CreateSquad::validate`
  (only commandable units or squads may be ordered); `npc_command::approach_step`; the command × vehicles glue
  (`tolerance`); the command × agent-io glue (`owned_units`, `commandable` flag in the `command` section);
  `npc_view_helpers::command_ui` (what the cursor can select); `inspect::command_json`.
- **Written / removed by:** nothing in the framework after spawn.

## Hashing and snapshots

- **State hash:** yes, as `commandable` (`register_sim_component` in `register_command_hash`,
  [`crates/npc_command/src/hash.rs`](../../../crates/npc_command/src/hash.rs)). Only with `CommandPlugin`.
- **World snapshot (ADR 0013):** saved as `commandable` (automatic with hashing).

## Related

- [`OrderQueue`](OrderQueue.md), [`Owner`](Owner.md) — required components.
- [`ExecutingOrder`](ExecutingOrder.md) — gates execution for `Brain` units.
- Not related to the kernel's `CurrentOrder`: `npc_command` never reads or writes it.

## Example

```rust
use npc_framework::command::{Commandable, Owner, PlayerId};

// A unit walking 120 mm/tick that counts a waypoint as reached within 200 mm.
world.spawn((
    SimId(7),
    SimPos::new(1000, 0),
    Commandable { speed_mm_per_tick: 120, arrive_mm: 200 },
    Owner(PlayerId(1)),
    NavAgent, // optional: path-find through npc_nav instead of walking straight
));
```
