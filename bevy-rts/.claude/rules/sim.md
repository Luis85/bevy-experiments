---
paths:
  - "crates/fl_sim/**"
  - "crates/fl_view/**"
  - "crates/fl_protocol/**"
  - "features/*/sim/**"
---

# Simulation rules (spec §5.4–§5.6; ADR-0003, ADR-0004, ADR-0005, ADR-0006)

## Determinism

- Sim state is `Fx` and integers only. No `f32`/`f64` (presentation floats live in the client); every sim
  crate denies `clippy::float_arithmetic`. Content decimals become `Fx` once, at load.
- Randomness: `fl_sim::rolls(seed, tick, sim_id, stream)` only, where `stream` is the `Stream` token that
  `SimBuilder::register_stream(name)` returns (capture it in a closure or pass it with `with_input`). The raw
  `fl_math::Rolls::new`/`roll` are clippy-banned in sim crates. World-level draws use `SimId(0)`. No RNG state in a
  resource, component or `Local`; `Pcg32` is for tools and the client.
- Never key, order or iterate sim state by `Entity` (`EntityHashMap`/`EntityHashSet`/`EntityIndexMap`/`EntityIndexSet`,
  `BTreeMap<Entity, _>`, sorting by `Entity`): `Entity` bits differ between a live run and a load. Use `SimId`.
- Banned (the generated `clippy.toml` enforces it): std, `bevy_platform`, `hashbrown` and `indexmap` hash collections, the Entity-keyed maps and sets, `RandomState`, `Instant`,
  `SystemTime`, `Time`, `ParallelCommands`, `Pcg32`, `thread::spawn`, `rand::rng`/`random`, `env::var`. The
  one exception is the runner thread in `fl_sim::runner`, behind a scoped `#[expect]` with a reason.
- Ordering: order-sensitive operations follow `SimIndex` (ascending `SimId`): shared writes, spawns,
  despawns, `SimId` allocation, budgets and queues, and cross-entity reads of same-tick writes. Entity-local
  updates may iterate in any order, including `par_iter_mut`. Cross-entity parallel work writes per-entity
  results into a buffer indexed by `SimId`, then applies them sequentially in `SimId` order.
- Executor: single-threaded, set explicitly. `SimTick` uses ambiguity detection at `LogLevel::Error` and
  `auto_insert_apply_deferred: false`.
- Hidden state: no `Local<T>` in sim systems; no gameplay logic on `Changed`/`Added`/`RemovedComponents`
  (`Changed<T>` only in `Phase::Publish`); no observers (component hooks maintain indices only); no `Entity` in
  hashed or saved state, commands, journal or replays.
- Hashing writes registry-backed IDs as their stable name key, never the dense ID. Every runtime ID comes from
  a stable-string registry that is sorted and frozen.
- Content IDs inside command payloads are names (`"base.de.pz3"`), never dense IDs.

## Classification

Register every sim component and resource as exactly one of:
- `Hashed`: authoritative; in the state hash and saves, with a versioned codec.
- `Derived`: rebuilt from hashed state on load. A feature registers **one** rebuild,
  `SimBuilder::register_rebuild(fn)`, that rebuilds all its `Derived` state; `load_world` runs the rebuilds in
  feature load order (core first, each feature after its deps). Hook-maintained state needs none.
- `Transient`: per-tick, enforced by the framework (R72). Components: `Class::Transient`, removed from every entity
  at the start of each tick. Resources: `Class::transient()` (reset to `Default`), `Class::TransientReset(fn)`, or
  `Class::TransientKept("reason")` when the owner empties it elsewhere. Never hashed or saved.
- `External` (resources only): fixed at build from the `MatchHeader`.

An unclassified type fails `build_sim_app` in debug builds and in every test; in debug builds `step_tick` checks
again when a tick registered new types, and asserts every entity with a classified component has a `SimEntity`
(spawn only through `SpawnRequest`/`setup_spawn`). `ChildOf`, `Children` and `Observer` are forbidden in the sim.

## Schedule: phases and hook sub-sets

`Input → Think → Act → Resolve → Economy → Perceive → Lifecycle → Publish`; each phase ends with one explicit
`ApplyDeferred`. Features add systems **only** into these 18 hook sub-sets:

| Phase | Sub-sets |
|---|---|
| `Input` | `Housekeeping`, `Validate`, `Apply` |
| `Think` | `Orders`, `Behaviour` |
| `Act` | `Move`, `Fire` |
| `Resolve` | `Projectiles`, `Effects`, `Stats` |
| `Economy` | `Main` |
| `Perceive` | `Spatial`, `Vision` |
| `Lifecycle` | `Death`, `Spawn`, `Despawn` |
| `Publish` | `Snapshot`, `Events` |

- Prefer `configure_sets` ordering over cross-crate `.before(system)`; `MessageMutator` for in-flight chains.
- Every in-tick spawn is a `SpawnRequest` Message handled in `Lifecycle::Spawn`; setup and load use
  `setup_spawn`. Add message types only with `SimBuilder::add_message`; `step_tick` clears every buffer at the end
  of the tick (R72), so a message is read in the tick it was written, and state that must reach the next tick is
  `Hashed`.
- `step_tick` is the only way to run `SimTick`: inbox → `SimTick` → hash if due → take outputs → clear message
  buffers → `clear_trackers` → advance clock.
- Reject a command with a stable code: core `RejectReason` variants, or `RejectReason::Feature("<feature>.<reason>")`;
  never add a variant for a feature.
- Commands apply in `(player, seq)` order; the sim rejects `Late`, `Early` and `DuplicateSeq`. `debug.*` kinds
  need `MatchHeader.cheats_allowed`.

## Crates

- `fl_protocol` → `fl_math`; `fl_view` → `fl_math`, `fl_protocol`; neither has any Bevy dependency.
- `fl_sim` and feature sim crates use `bevy_app` + `bevy_ecs` only. Feature-to-feature edges appear in `deps()`
  and in `[package.metadata.fl] sim_deps`.
- `fl_protocol`, `fl_view` and the `fl_sim` extension API are contract paths: change them only with an ADR.
- Observability: journal records, `SimCounters` and spans for every mechanic; the sim never reads them back.
