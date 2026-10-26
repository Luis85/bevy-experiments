---
type: Component
---
# `MemberOf`

**Crate:** `npc_command` · **File:** [`crates/npc_command/src/types.rs`](../../../crates/npc_command/src/types.rs) · **Capability:** command (RTS orders)

The squad a unit belongs to (at most one). It is the authoritative side of membership: a squad's `members`
list keeps only units whose `MemberOf` still names it.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `0` | `SimId` | The squad entity's `SimId`. |

## Lifecycle

- **Added by:** `create_squad` (applied by `CreateSquad`), on every member; joining a new squad overwrites the
  old link ([`crates/npc_command/src/command.rs`](../../../crates/npc_command/src/command.rs)).
- **Read by:** `prune_squads` in [`command_engine_system`](../systems/command_engine_system.md); `DisbandSquad`;
  the command × agent-io glue (`squad` field of the `command` observation section,
  [`glue/command_agent.rs`](../../../crates/npc_framework/src/glue/command_agent.rs)); `inspect::command_json`.
- **Removed by:** `DisbandSquad::apply` (only when it still names that squad); `prune_squads` when the squad
  left the sim (e.g. a `Despawn` of the squad entity).

## Hashing and snapshots

- **State hash:** yes, as `squad_member` (`register_sim_component` in `register_command_hash`,
  [`crates/npc_command/src/hash.rs`](../../../crates/npc_command/src/hash.rs)).
- **World snapshot (ADR 0013):** saved as `squad_member` (automatic with hashing).

## Related

- [`Squad`](Squad.md) — the squad entity and its sorted member list.

## Example

```rust
// Which squad is unit 5 in?
let squad: Option<SimId> = world.get::<MemberOf>(unit).map(|m| m.0);
```
