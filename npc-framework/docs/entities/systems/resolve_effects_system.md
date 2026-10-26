---
type: System
---
# `resolve_effects_system`

**Crate:** `village_content` · **File:** [`content/village_content/src/effects.rs`](../../../content/village_content/src/effects.rs) · **Plugin:** `VillagePlugin`

Applies the village's cross-entity effects. Action systems write `SimEffect { source, kind }` messages; this
system drains all of them, sorts them and applies them, so the result does not depend on the order the
action systems ran. `AddFood` adds to a stockpile, clamped to `capacity`; `TakeFood` subtracts, saturating at
0. An effect whose target is missing (no such `SimId`, or no `Stockpile` on it) is dropped.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Effects`, not exclusive
- **Ordering:** none beyond the set (Effects runs after Locomotion, before Cleanup). The `SimEffect` message
  type is added by `add_message::<SimEffect>()` in `add_systems`.

## Reads / writes

| Access | Data |
|---|---|
| reads | `SimIndex`, `SimClock` (optional) |
| writes | `Stockpile.food`; `Journal` (one `Effect { effect: <EffectKind JSON>, applied }` per effect, in apply order, when enabled) |
| messages / events | drains `Messages<SimEffect>` directly (`messages.drain()`; it does not rely on `Messages::update`, which only runs in `First`) |

## Determinism

`SimEffect` derives `Ord` comparing `source` (`SimId`) first, then `kind`; the drained `Vec` is
`sort_unstable`d, which is total for equal elements. Integer math only. Effect JSON: e.g.
`{"type":"AddFood","stockpile":3,"amount":3}`.

## Performance

- **Mechanic tag:** `effects` (via `village_content::perf_tags` prefix `village_content::effects`) ·
  **Budget:** 15 µs + 2 µs/agent (`effects` in `NPC_BUDGET_TABLE`) · no span. Returns at once when no
  message is pending.

## Related

- [`eat_system`](eat_system.md) (`TakeFood`), [`work_system`](work_system.md) (`AddFood`) — the writers.
- Components: [`Stockpile`](../components/Stockpile.md)
