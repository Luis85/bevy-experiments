---
type: Component
---
# `Squad`

**Crate:** `npc_command` · **File:** [`crates/npc_command/src/types.rs`](../../../crates/npc_command/src/types.rs) · **Capability:** command (RTS orders)

A squad is its own sim entity (with a `SimId` and an [`Owner`](Owner.md)) whose members are ordered together.
`IssueOrder` naming a squad id expands it to its members (`expand_units`). A group `Move` / `AttackMove` to one
squad uses the squad's formation unless the order names one. The journal / inspect kind name of a squad entity
is `"squad"` (the `~npc_command` kind namer).

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `members` | `Vec<SimId>` | Members, sorted by `SimId`, deduplicated. Members that left the sim or joined another squad are pruned. |
| `formation` | `Formation` | `{ kind: FormationKind (Line, Box, Wedge), spacing_mm: i32 }`; default from `CommandConfig::default_formation` (`Box`, 1500 mm). |

## Lifecycle

- **Added by:** `create_squad` (applied by the recorded `CreateSquad` command), which spawns the entity with
  `spawn_sim((Squad { .. }, Owner(player)))` and puts [`MemberOf`](MemberOf.md) on each member
  ([`crates/npc_command/src/command.rs`](../../../crates/npc_command/src/command.rs)). `Owner` is also required.
- **Read by:** `expand_units` and `issue_to` (squad formation), `check_owner` (a squad may be ordered),
  `DisbandSquad`, `kind_name`, `inspect::command_json`.
- **Written by:** `prune_squads` inside [`command_engine_system`](../systems/command_engine_system.md) (drops
  members whose `MemberOf` no longer names this squad).
- **Removed by:** `DisbandSquad::apply` despawns the squad entity (members stay, without `MemberOf`).

## Hashing and snapshots

- **State hash:** yes, as `squad` (`register_sim_component` in `register_command_hash`,
  [`crates/npc_command/src/hash.rs`](../../../crates/npc_command/src/hash.rs)): member count, ids, formation
  kind (`u8`) and spacing.
- **World snapshot (ADR 0013):** saved as `squad` (automatic with hashing).

## Related

- [`MemberOf`](MemberOf.md) — the back link on each member.
- [`Owner`](Owner.md) — squads are owned and checked like units.

## Example

```rust
// Recorded command JSON (submitted like any SimCommand):
// {"type":"CreateSquad","player":1,"members":[5,6,7],"formation":{"kind":"Line","spacing_mm":2000}}
let squad: SimId = create_squad(world, PlayerId(1), &[SimId(5), SimId(6), SimId(7)], None); // replayed sim code only
submit_order(world, &IssueOrder::new(PlayerId(1), [squad], UnitOrder::Move { pos: (9000, 0) }))?;
```
