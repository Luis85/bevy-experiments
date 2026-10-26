---
paths:
  - "crates/npc_ai/**"
  - "crates/npc_kernel/**"
  - "crates/npc_needs/**"
  - "crates/npc_jobs/**"
  - "crates/npc_locomotion/**"
  - "crates/npc_nav/**"
  - "crates/npc_perception/**"
  - "crates/npc_rules/**"
  - "crates/npc_agent_io/**"
  - "crates/npc_interaction/**"
  - "crates/npc_vehicles/**"
  - "crates/npc_command/**"
  - "crates/npc_progression/**"
  - "crates/npc_crafting/**"
  - "crates/npc_framework/src/glue/**"
  - "content/*_content/**"
---

# Simulation crates: determinism rules

These crates produce the state hash. Same seed + same `SimCommand`s must give the same hash on every OS,
with any system order the scheduler may pick, and with the journal and profiler on or off.

Every sim crate (`npc_ai`, `npc_kernel`, `npc_needs`, `npc_jobs`, `npc_locomotion`, `npc_nav`,
`npc_perception`, `npc_rules`, `npc_agent_io`, `npc_interaction`, `npc_vehicles`, `npc_command`,
`npc_progression`, `npc_crafting`, `village_content`) carries the same `clippy.toml`
(disallowed hash containers, clocks, thread RNG, `bevy_time::Time`, `f32`/`f64`) and denies,
at the crate root:
`clippy::float_arithmetic`, `clippy::float_cmp`, `clippy::iter_over_hash_type`,
`clippy::disallowed_types`, `clippy::disallowed_methods`.
The facade's capability glue (`npc_framework/src/glue/*`) denies the same lints per module; the facade has
no `clippy.toml`, so review it for clocks, hash containers and floats by hand.
The one sanctioned clock reader is the kernel driver (`npc_kernel::driver::host_delta`, allowed with a
`// determinism:` comment): it only decides how many ticks run.

## Never

- Read `Time`, `Time<Fixed>`, `Instant::now`, `SystemTime::now`, frame delta or any wall clock. Read
  `SimClock { tick }` (and `SimConfig` for day length). Clippy `disallowed-*` enforces this.
- Use `HashMap`/`HashSet` (std or `bevy::platform`) for sim state or anything iterated. Use `BTreeMap`,
  `BTreeSet` or a sorted `Vec`.
- Iterate in `Entity` order or query order when the result matters. Collect, sort by `SimId`, then act.
- Use floats in sim state or decisions (`#![deny(clippy::float_arithmetic)]`). Positions are `SimPos` in mm
  (`i32`), speeds mm/tick, needs `i32` milli-units `0..=100_000`, distances via `dist2`/integer `isqrt`.
  An exemption needs `#[allow(clippy::float_arithmetic)]` plus a `// determinism: <why>` comment, and only
  for code that never feeds sim state (perf spans).
- Use a stateful or thread RNG (`rand::thread_rng`, `rand::rng`). Use `rng::sim_rand(seed, sim_id, tick,
  purpose)` / `sim_range`, with a new `purpose` constant per use site.
- Insert presentation components (`Transform`, meshes, materials, `VisualOf`) on sim entities, or depend on
  `npc_view_helpers`, `bevy_render`, `bevy_pbr`, `wgpu`, or any `content/*` crate (checked by `arch_fitness`).
- Use `Single<..>` or "the villager" resources: everything must work for N agents.
- Let tooling (journal, inspect, perf, BRP) change sim results. Profiling data lives in non-hashed
  resources; perf breaches never go in the journal.

## Always

- Put systems in `NpcSimTick` inside an `NpcSet` (`Inputs → Needs → Sense → Think → Actions → Effects → Cleanup`).
  Ambiguity detection is `Error`: if two systems conflict, order them (`.chain()`, `.before()`), never
  silence the check.
- Change state from outside only through `SimCommand` (applied in `NpcSet::Inputs` in `(tick, seq)` order);
  a new external input is a `ContentCommand` registered with `register_command::<C>()`.
- Apply cross-entity effects sorted in `NpcSet::Effects` (the village's `SimEffect` is the pattern).
- Register every new sim component with `register_sim_component::<T>()` (`T: SimHash`, a stable
  `SimHash::NAME`) and every hashed resource with `register_sim_resource::<R>()`; never hand-edit the hash.
  Encodings are fixed: fixed-width little-endian ints, `u8` enum discriminants, length-prefixed `Vec`s
  (the registry adds the presence tag). Ids are frozen sorted by name, so never depend on registration
  order; a new name changes the content hash (and so invalidates recorded replays): say so.
- Action = `SparseSet` component + one system in `NpcSet::Actions` that writes `ActiveLeaf` status. Only the
  action's own system changes its status after `start`. Abort releases reservations synchronously.
- Every `Abort::LowerPriority` guard condition must imply its child can start (see `villager_tree` docs);
  otherwise every re-think aborts and restarts lower work.
- Journal new decisions/effects an agent will need to explain (generic kinds in `npc_kernel::journal`;
  content effects as `JournalKind::Effect` JSON), and tag new systems with a mechanic: generic crates in
  `MechanicTags::default_npc`, content in its own `perf_tags`.

## Proving it

- A scenario test first (`content/village_content/tests/` for the village, unit tests in the generic
  crate otherwise); `tests/determinism.rs`, `registration.rs` and the replay corpus must still pass.
  Never edit `tests/replays/**` or snapshots; a hash change needs user approval (stage → evidence →
  approval → swap: `docs/rebaselining.md`).
- `cargo xtask verify`.
