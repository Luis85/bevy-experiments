---
type: Component
---
# `HostPosition`

**Crate:** `npc_kernel` · **File:** [`crates/npc_kernel/src/driver.rs`](../../../crates/npc_kernel/src/driver.rs) · **Capability:** kernel (host driver)

The host's position for a sim entity, for hosts that own movement (e.g. read back from a physics engine).
Only used in `DeterminismMode::Relaxed`: at the start of every tick the sim copies it into `SimPos`. In
`Strict` mode (the default) it is ignored. Replays are then only as deterministic as the host's mover.

## Fields

| Field | Type | Unit / meaning |
|---|---|---|
| `0` | [`SimPos`](SimPos.md) | millimetres, the position the host wants the sim to use this tick |

## Lifecycle

- **Added by:** the host app (not by any framework code on this checkout). Enable the mode with
  `NpcPlugins::new().determinism(DeterminismMode::Relaxed)` (see `docs/using-npc-framework.md`).
- **Read by:** [`apply_host_positions_system`](../systems/apply_host_positions_system.md) (`NpcSet::Inputs`),
  which writes `SimPos = HostPosition` when they differ.
- **Written / removed by:** the host only.

## Hashing and snapshots

- **State hash:** no (the copied `SimPos` is hashed).
- **World snapshot (ADR 0013):** saved as `host_position`, snapshot-only, registered in
  `npc_kernel::snapshot::register_kernel_entries` ([`snapshot.rs`](../../../crates/npc_kernel/src/snapshot.rs)).

## Related

- [`SimPos`](SimPos.md) — the value it overwrites.
- [`drive_sim`](../systems/drive_sim.md) — the driver; `DeterminismMode` is set on `NpcKernelPlugin`.

## Example

```rust
// Host system, after physics, before the driver runs NpcSimTick.
fn physics_to_sim(mut q: Query<(&Transform, &mut HostPosition)>) {
    for (t, mut host) in &mut q {
        host.0 = SimPos::new((t.translation.x * 1000.0) as i32, (t.translation.z * 1000.0) as i32);
    }
}
```
