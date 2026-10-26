---
type: System
---
# `sleep_system`

**Crate:** `village_content` · **File:** [`content/village_content/src/actions.rs`](../../../content/village_content/src/actions.rs) · **Plugin:** `VillagePlugin`

Runs every sleeper (`Sleep`). Each tick it adds the bed's energy `Provides.amount_per_tick` to energy
(clamped to `0..=NEED_MAX`) and checks the wake-up condition: `Full` wakes at `NEED_MAX`, `Morning` wakes when
it is day and energy is above the energy need's `thresholds.low_exit`. Fails when the bed is gone or does
not provide energy. Returns early when no `Energy` need is registered.

## Schedule

- **Schedule:** `NpcSimTick`
- **Set:** `NpcSet::Actions`, not exclusive
- **Ordering:** sixth in the village action chain (after `work_system`, before `idle_system`).

## Reads / writes

| Access | Data |
|---|---|
| reads | `Sleep` (`bed`, `until`); `Provides` `With<Bed>`; `SimIndex`, `SimClock`, `SimConfig` (`is_night(tick)`), `NeedRegistry` (`id(ENERGY)`, `def(..).thresholds.low_exit`) |
| writes | `Needs` (energy), `ActiveLeaf.status` |
| messages / events | none |

## Determinism

Integer needs math; night is computed from `SimClock.tick` and `SimConfig`, never wall time. Energy and
thirst decay are paused meanwhile by the `PauseDecay` that `sleep_in_bed_start` inserted.

## Performance

- **Mechanic tag:** `actions.sleep` (prefix `village_content::actions::sleep_system`) · **Budget:** 10 µs +
  2 µs/villager (`VILLAGE_BUDGETS`) · no span.

## Related

- [`decay_needs_system`](decay_needs_system.md) — skips paused needs.
- [`villager_transforms`](villager_transforms.md) — draws sleepers lying in the bed.
- Components: [`Sleep`](../components/Sleep.md), [`Bed`](../components/Bed.md),
  [`PauseDecay`](../components/PauseDecay.md), [`Needs`](../components/Needs.md)
