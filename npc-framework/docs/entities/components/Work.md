---
type: Component
---
# `Work`

**Crate:** `village_content` · **File:** [`content/village_content/src/actions.rs`](../../../content/village_content/src/actions.rs) · **Capability:** village content (actions)

A running work shift (leaf `DoWork`). When it counts down to 0 the site's stockpile gets the yield through a
`SimEffect`. An interrupted shift yields nothing. `SparseSet` storage.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `site` | `SimId` | the `WorkSite` being worked (the claimed `BB_TARGET`) |
| `remaining` | `u32` | ticks left in the shift (starts at `WorkSite.work_ticks`) |

## Lifecycle

- **Added by:** `do_work_start` (param `"DayOnly"` fails at night; `"AnyTime"` always starts) in
  [`actions.rs`](../../../content/village_content/src/actions.rs). Also sets `Activity::Work`.
- **Read / written by:** [`work_system`](../systems/work_system.md) (decrements `remaining`; at 0 writes
  `AddFood` and sets the leaf to `Success`).
- **Removed by:** `stop_work` (takes it; on `Finished` releases the site claim, on `Aborted` drops all
  claims).

## Hashing and snapshots

- **State hash:** yes, as `work` (`site.0`, `remaining`, `u32` each; `register_sim_component` in
  `register_components`).
- **World snapshot (ADR 0013):** saved as `work` (automatic with hashing).

## Related

- [`WorkSite`](WorkSite.md) — the place. [`Stockpile`](Stockpile.md) — gets the yield.
- [`resolve_effects_system`](../systems/resolve_effects_system.md) — applies `AddFood`.

## Example

```rust
// Tree leaf (villager.rs): ReserveWork -> MoveToDest -> DoWork("DayOnly").
b.leaf("DoWork", "DoWork", WORK_DAY_ONLY);
// Reading a running shift:
if let Some(work) = world.get::<Work>(agent) {
    println!("working #{} for {} more ticks", work.site.0, work.remaining);
}
```
