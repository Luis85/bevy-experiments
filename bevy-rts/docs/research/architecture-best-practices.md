# Frontline: best practices for a framework that stays healthy for years

Research date 2026-10-23. Inputs: `docs/superpowers/specs/2026-10-23-frontline-rts-design.md` (the design), `docs/research/architecture.md` (prior research) and `docs/research/bevy020-api-reference.md`. This report builds on those documents and does not repeat them. Where it disagrees with them, it says so.

**How the claims were checked.**
- **[src]** means I read it in the `bevy_* 0.20.0` sources in `~/.cargo/registry`.
- **[probe]** means I ran a throwaway Cargo project in the scratchpad (`scratchpad/bp/`) with rustc and cargo 1.99.1.
- **[crates.io]** means I queried the crates.io API on 2026-10-23.
- Everything else is cited by URL in §7.

I modified no project.

---

## TL;DR: the ten changes that matter most

1. **Split every feature into two crates, `fl_feat_x` (sim) and `fl_feat_x_client`, instead of one crate with a `client` Cargo feature.** Cargo unifies features across the workspace, and the opt-out (`resolver.feature-unification`) is still unstable on cargo 1.99.1 [probe]. As a result, the gates the design mandates (`cargo clippy --workspace`, `cargo test --workspace`) compile every sim half *with* the client code and full Bevy. They can never prove the sim is headless, and per-crate lint rules can't tell the two halves apart.
2. **Run the sim on its own thread, behind a snapshot boundary, not as a synchronous `SubApp`.** A 15 ms tick (the G7 budget) run inline on the main thread blows the 16.7 ms frame on every tick frame. Pipelined rendering already moves the render world off-thread. Do the same for the sim: commands go in, a `SimSnapshot` plus a `SimEvents` buffer come out.
3. **Replace the shared sequential RNG with keyed, counter-based randomness**: `rng(seed, tick, sim_id, stream_id)` everywhere, not just in parallel phases. Otherwise every new feature that draws a number shifts every later draw, which breaks golden replays and couples unrelated features. Factorio hit exactly this: a global RNG blocked parallelising the selector combinator (FFF #421).
4. **Assign every runtime registry ID by sorted stable string, never by registration order**: command kinds, def components, stats, conditions, tags, effects and resources. Persist the string tables in replays and saves. Today the design only specifies this for content IDs.
5. **Add a save/load design now.** It is the largest missing piece. Use per-component save codecs registered like def components, versioned with migrations. Add two fitness tests: save, load and save again must produce identical bytes, and a "branch" test (save at tick N, reload, continue) must match an uninterrupted run (Factorio's "heavy mode").
6. **Adopt OpenRA-style *conditions* and a Paradox/0 A.D.-style *modifier* spec as core vocabulary.** These are the glue that lets features interact through data without depending on each other's crates.
7. **Give the content pipeline explicit load phases and data patch operations** (RimWorld PatchOperations, Factorio data → updates → final-fixes, Recoil `unitdefs_post`). Whole-object `override: true` doesn't compose across mods.
8. **Turn the global `SimSet` list into documented phases with named hook sub-sets. Disable auto sync points and place `ApplyDeferred` explicitly.** Funnel every spawn through one ordered spawner that assigns `SimId`s. Move hashing out of the schedule into the driver.
9. **Make architecture erosion fail CI**:
   - Per-crate `clippy.toml` bans. Verified: `disallowed-types` accepts `f32`/`f64` and `HashMap` [probe].
   - An `xtask arch-check` that runs a per-package `cargo tree`.
   - `cargo-deny` 0.20.2.
   - Public-API and serde-format snapshots for the contract crates.
   - ADRs (MADR 4.0).
   - A machine-readable ownership map for parallel agents.
10. **Debug and testbed tools must go through the command stream** (`DebugCommand` kinds, gated by a `cheats` flag in the match header). As specified, the spawn panel and the "invulnerable" toggle would mutate the sim from the client. That breaks replays and the §2 anti-goal.

---

## 1. Adopt now (baseline)

Effort: **S** ≤ 1 agent-day, **M** 2–5 agent-days, **L** 1–3 agent-weeks. Order = priority.

| # | Pattern | Crates touched | Effort |
|---|---|---|---|
| N1 | Two-crate features (sim / client) | `features/*` | S now, L later |
| N2 | Threaded sim runner + `SimSnapshot`/`SimEvents` view protocol + `TickDriver` | `fl_sim`, `fl_client`, new `fl_view` | M |
| N3 | Keyed counter-based RNG; ban the sequential sim RNG | `fl_math`, `fl_sim` | S |
| N4 | Stable-string registries for every runtime ID + string tables in replay/save headers | `fl_protocol`, `fl_content`, `fl_sim` | S–M |
| N5 | Sim schedule hygiene: phases, hook sub-sets, explicit sync points, spawn funnel, manual-stepping housekeeping | `fl_sim` | M |
| N6 | Mandatory component classification (hash / save / derived), checked at startup | `fl_sim` | S |
| N7 | Save/load skeleton with versioned codecs and round-trip and branch tests | `fl_sim`, new `fl_save`, `fl_test_utils` | M |
| N8 | Conditions (granted tokens + boolean `requires`) | `fl_sim` core vocabulary | M |
| N9 | Modifier/stat spec with a deterministic stacking order and a cached effective stat | `fl_sim` core vocabulary, `fl_content` | M |
| N10 | Content load phases + patch operations + post-process hooks + `format_version` | `fl_content` | M |
| N11 | Data "verbs": open registries for effects, triggers and AI considerations, like def components | `fl_content`, `fl_sim` | S (the mechanism) |
| N12 | Tags as interned bitsets plus class expressions | `fl_content`, `fl_sim` | S |
| N13 | Event model rules: Messages for sim gameplay, hooks for invariants, observers only order-independent | all sim crates (an ADR) | S |
| N14 | Relationships for squads, garrisons, transport, towing, crew, ownership (custom ones, not `ChildOf`) | `fl_sim`, features | S |
| N15 | Unit behaviour = order queue + activity stack, with the state as a table-stored enum | `fl_sim::orders` | M |
| N16 | Fitness functions in CI (clippy bans, xtask arch-check, cargo-deny, per-package builds) | repo root, new `xtask` | M |
| N17 | Contract-crate API and format snapshots (`cargo-public-api`/`insta`, `serde-reflection`) | `fl_protocol`, `fl_content`, `fl_view` | S |
| N18 | ADRs (MADR 4.0) + a deprecation policy + an ownership map for agents | `docs/adr/`, `OWNERS.toml` | S |
| N19 | Debug and testbed actions as protocol commands | `fl_protocol`, `fl_client::debug` | S |
| N20 | Lockstep-ready command envelope (execution tick, per-player sequence number, deterministic intra-tick order) | `fl_protocol` | S |
| N21 | Benchmarks: instruction counts in CI, wall-clock locally; Tracy spans | `fl_cli`, benches | S–M |
| N22 | Render scale baseline: one material per unit type, per-instance data via `MeshTag`/`GpuComponentArrayBuffer`, `VisibilityRange` LOD | `fl_client::view` | M |

### N1. Two-crate features
- **Why.**
  - Verified [probe]: in a workspace where `game` enables `feat/client` and `cli` doesn't, `cargo tree -p cli` shows no client dependency. Select `cli` and `game` together (as `--workspace` does), though, and `cli`'s copy of `feat` gets the client dependency.
  - `resolver.feature-unification = "package"` prints `ignoring ... without -Zfeature-unification` on cargo 1.99.1, and RFC 3692 is still unstabilised.
  - So with the design's layout:
    - (a) `cargo test --workspace` builds sim tests with `bevy_render` linked.
    - (b) A sim half that accidentally uses a client-only dependency still compiles in CI.
    - (c) One `clippy.toml` per crate can't ban `f32` in the sim half while allowing it in the client half. Clippy uses only the nearest `clippy.toml`; it doesn't merge them [probe].
    - (d) Code ownership can't be expressed per half.
- **How.**
  - Use `features/infantry/{sim,client}/Cargo.toml` → `fl_feat_infantry` and `fl_feat_infantry_client`.
  - Replace `GameFeature` with two traits:
    ```rust
    pub trait SimFeature: Send + Sync + 'static {
        const ID: &'static str; const VERSION: u32;
        fn deps(&self) -> &'static [&'static str] { &[] }   // other SimFeature IDs
        fn build(&self, sim: &mut SimBuilder);              // registries + systems
    }
    pub trait ClientFeature: Send + Sync + 'static {
        type Sim: SimFeature;                                // must be present
        fn build(&self, app: &mut App);
    }
    ```
  - `GameBuilder` sorts features topologically by `deps()`, rejects cycles and missing dependencies (Bevy has no plugin dependency resolution; only `App::is_plugin_added`/`get_added_plugins` exist [src]), and records `(ID, VERSION)` in the rules hash (see C9).
- **Effort.** S if done before M0 code. L as a retrofit after M4.

### N2. Threaded sim runner, view protocol, tick driver
- **Why.**
  - `SubApp::update` runs synchronously inside `App::update` on the main thread [src: `bevy_app/src/sub_app.rs`]. A tick frame then costs about sim tick + main-world work.
  - With `PipelinedRenderingPlugin` [src], frame time is about max(main, render), so a 10–15 ms tick alone eats the 60 fps budget.
  - Separately, a client "sync step" that reads sim components directly couples `fl_client` to every sim component type. Every sim refactor then breaks the client.
- **How.**
  - `fl_sim::SimRunner` owns the sim `World` on a dedicated thread.
    - Input: `crossbeam`/`std::sync::mpsc` channel of `TickInput { tick, commands }`.
    - Output: a double-buffered `Arc<SimSnapshot>` plus `Vec<SimEvent>` per tick.
  - Interpolation already delays the view by one tick, so pipelining costs no extra visible latency.
  - A new tiny crate `fl_view` (depends on `fl_math`/`fl_protocol` only) defines the snapshot schema: pose, heading, turret angles, HP fraction, flags, visibility-grid deltas, and the event enum registered by features. A `Publish` phase in the sim writes it, using `Changed<T>` filters.
  - The client depends on `fl_view`, never on sim component types. Features extend the snapshot with registered "view channels" (string-keyed, like def components).
  - `TickDriver` trait, with one implementation per mode:
    - `LocalDriver`: wall-clock accumulator.
    - `LockstepDriver`: advances only when every peer's commands for that tick are present.
    - `ReplayDriver`: as fast as possible.
    - `TestDriver`: explicit.
  - Headless keeps calling the runner synchronously.
- **Effort.** M. Do it in M0, because the M0 "sync and interpolation" work would otherwise be thrown away.

### N3. Keyed counter-based randomness
- **Why.**
  - With one sequential PCG in a resource, any system that adds or removes a draw shifts every later draw in the tick. That means: unrelated golden tests change when a feature is added, outcomes depend on system order across features, and parallelism is blocked.
  - FFF #421: Factorio's selector combinator was the only combinator that couldn't be multithreaded, because it "relied on a global random generator".
- **How.**
  - `fn roll(seed: u64, tick: u32, actor: SimId, stream: StreamId, n: u32) -> u64`, a stateless hash. Use a SplitMix64 or PCG-style finaliser over packed inputs, or xxh3 (`xxhash-rust` 0.8.19 [crates.io]).
  - `StreamId` comes from a sorted string registry (`"combat.dispersion"`), per N4.
  - Ban `rand`'s thread RNG and any `Rng` resource in sim crates with clippy (N16).
  - Keep `rand_pcg` 0.10.2 only for content-time or client use.
- **Effort.** S.

### N4. Stable-string registries for every runtime ID
- **Why.**
  - The design interns *content* IDs by sorted string, but leaves `CommandTypeId(u16)`, def-component IDs, hashable-component order, stat IDs and so on unspecified.
  - If any of these follow registration order, then adding, reordering or feature-gating a plugin silently renumbers them. Replays, saves, the network protocol and state hashes all break.
- **How.**
  - One generic `StableRegistry<K>`: register by `&'static str` (namespaced: `"core.move"`, `"infantry.lie_down"`). `freeze()` in `Plugin::finish` sorts the names and assigns dense IDs.
  - Every replay, save and network handshake carries the frozen string tables. Loading maps strings to current IDs and rejects unknown ones (or migrates them, N7/N10).
  - Hot reload may change *values* but not the *ID set*. If the ID set changes, reload rebuilds the match, because live entities hold dense IDs.
- **Effort.** S–M.

### N5. Sim schedule hygiene
- **Why.**
  - Bevy's `auto_insert_apply_deferred` defaults to `true` [src: `ScheduleBuildSettings`]. Adding any feature system with `Commands` and an ordering edge can add a sync point. That changes when *other* features' spawns become visible, which changes behaviour and golden hashes.
  - The `shuffle_seed` docs note that auto sync points also stop shuffling from being thorough.
  - Stepping a `World` by hand with `run_schedule` skips the `App` housekeeping:
    - `message_update_system` runs in `First` of the app [src: `bevy_app/src/sub_app.rs:91`].
    - `clear_trackers()` is called only by `SubApp::update` [src].
    - So Messages never rotate, and removal buffers grow.
  - Hashing as a `SimSet` invites a feature to schedule work after it.
- **How.**
  - `SimTick` settings: `ambiguity_detection: LogLevel::Error`, `auto_insert_apply_deferred: false`, and an explicit `set_executor_kind`. Don't depend on whichever `bevy_ecs` features got unified in.
  - Phases (coarse, closed, documented). Each is a `SystemSet` with named public hook sub-sets, ending in an explicit `ApplyDeferred`:
    `Input` (housekeeping: `message_update_system`; command decode and validate; a `ValidateCommand` hook) → `Think` (orders, activities, target acquisition, unit-level AI) → `Act` (movement, firing intents) → `Resolve` (projectiles, effects pipeline `Effects::{Generate, Modify, Apply}`, conditions, modifier recompute) → `Economy` (resources, production, construction, research; time-sliced, empty in iteration 1) → `Perceive` (spatial index maintenance, vision) → `Lifecycle` (death → wreck, the **single spawn funnel** that assigns `SimId`s in request order, despawn) → `Publish` (snapshot, events, `PlayerView`).
  - The driver, outside the schedule, then runs `hash_if_due()` and `world.clear_trackers()`.
  - All spawns become `SpawnRequest` Messages handled in `Lifecycle`. Nothing allocates `SimId`s in hooks or from unordered `Commands`.
  - Two sim systems may share a phase only if ambiguity detection accepts it. Use `MessageMutator` for "modify in flight" pipelines; its docs say it is "ideal for chains of systems that all want to modify the same messages" [src].
- **Effort.** M.

### N6. Mandatory component classification
- **Why.**
  - The design hashes "registered" components only. A feature that forgets to register a component causes desyncs the hash can't see, and the same component will be missing from saves.
- **How.**
  - Every sim `Component` must be registered as exactly one of:
    - `hashed + saved`: authoritative state.
    - `derived`: rebuildable from other state on load, such as the spatial-index cell or the effective-stat cache. It needs a `rebuild` system.
    - `transient`: cleared every tick.
  - At startup in debug and CI, iterate `world.components()` for every component whose type path starts with an `fl_` sim crate. Fail if any is unclassified.
- **Effort.** S.

### N7. Save/load skeleton
- **Why.**
  - Campaigns, reconnects and late join (state transfer is "likely the best option" per SnapNet) all need it, and the design never mentions saves.
  - Retrofitting serialization onto a live sim is how saves rot. Factorio built heavy mode because save/load must be deterministic and is therefore testable (FFF #270, wiki "Desynchronization").
  - `moonshine-save` 0.8 (bevy 0.20 [crates.io]) is reflection-based, and its README states it "does not support backwards compatibility, versioning, or validation". It is fine for a client settings file, wrong for the sim.
- **How.**
  - Save layout:
    ```
    header { format_version, build_id, features[(id, version)], mods[(id, version)], rules_hash, string_tables }
    sim    { tick, seed, sim_id_allocator, pending_commands[], resources[(name, ver, bytes)],
             entities[ SimId → [(component_name, ver, bytes)] ] }   // sorted by SimId, then name
    ```
  - Codecs are registered like def components: `sim.register_saved::<Suppression>("infantry.suppression", 1)`. Bytes are `postcard`.
  - Migrations are typed: keep `mod v1 { struct Suppression {...} }` and `impl From<v1::Suppression> for Suppression`. `Entity` never appears; relationships save as `SimId`.
  - Load spawns in `SimId` order, then runs the `derived` rebuilders.
  - Fitness tests in `fl_test_utils`:
    - (a) `save(load(save(w))) == save(w)` byte for byte.
    - (b) **Branch test**: run A to tick M. Run B to tick N < M, save, load into a fresh world, run to M. Compare state hashes every tick.
    - (b) also catches any system that depends on table iteration order, because loading re-creates tables in a different order than the live run's `swap_remove` history.
- **Effort.** M for the skeleton plus tests. Per-component codecs then get added with each feature.

### N8. Conditions
- **Why.**
  - OpenRA scaled to many mods because traits don't call each other. Traits *grant* named conditions (`GrantConditionOnDeploy`, `…OnDamageState`, `…OnTerrain`, `…OnPrerequisite`, `…WhileAiming`, and so on), and any trait can be enabled with `RequiresCondition: <boolean expression>` (OpenRA trait docs, release-20250330).
  - That lets a mod say "entrenched infantry get +50% cover" without engineering code depending on infantry code.
- **How.**
  - `Conditions` component: a small sorted `SmallVec<(ConditionId, u16 count)>`. Conditions come from N4 registries.
  - A grant API that returns a token; revoking the token decrements the count.
  - A tiny expression language (`&&`, `||`, `!`, parentheses), compiled at content link time to bytecode.
  - Def components may carry `requires: "deployed && !moving"`. The def's runtime system checks a cached boolean, recomputed when `Conditions` changes.
  - Grant sources are themselves def components (`GrantOnTerrain`, `GrantWhileMoving`, ...), so modders combine them in RON.
- **Effort.** M.

### N9. Modifiers and stats
- **Why.**
  - The design says "stat modifiers are a core concept" but specifies nothing. Every later layer (experience, morale, formations, tech, weather, colony traits) depends on getting the stacking rules and determinism right once.
  - Prior art:
    - HOI4: named modifiers summed per scope; separate multiplicative kinds; script-defined `modifier_definitions` with display metadata; tooltip breakdown.
    - 0 A.D.: `{Paths, Affects (class filter), Add, Multiply, Replace}` plus stackability `ignore | extend | replace | stack`. `ModifiersManager` applies player-wide modifiers before per-entity ones and caches results, because recomputing "every time is quite slow".
- **How.**
  - `StatId` registry (N4), for example `"combat.dispersion"`, `"move.speed"`, `"vision.range"`. Base values come from instantiated defs.
  - A modifier has: stat, op, value (`Fx`), source (`ModSourceId` + optional `SimId`), scope (`Global | Player | Team | Unit | Aura{radius}`), filter (class expression, N12), optional `requires` (N8), and a stacking key and policy.
  - Evaluation order, fixed in an ADR and done in `Fx` with defined rounding:
    `v = override.unwrap_or(base); v += Σ add; v *= (1 + Σ add_pct); v *= Π mul; v = clamp(v)`
    Within each bucket, order modifiers by `(source_id, sim_id)`, even though addition is commutative, so fixed-point rounding is deterministic.
  - Caching:
    - Each unit gets an `EffectiveStats` cache (a `derived` component, N6).
    - Recompute only when its modifier set, conditions or a relevant player/global scope changes, using a dirty flag through change detection.
    - Auras use the spatial grid and are re-evaluated on cell crossing or time-sliced.
  - Debug overlay: per-stat breakdown by source. It doubles as tooltip data later.
- **Effort.** M.

### N10. Content load phases, patches, post-processing
- **Why.**
  - Whole-object `override: true` means two mods touching the same unit conflict.
  - RimWorld solved this in A17 with XPath PatchOperations (`Add/Insert/Remove/Replace/AddModExtension/SetName/Sequence/FindMod/Conditional`). Patches run **before** inheritance, so patching a parent flows to its children.
  - Factorio runs every mod's `data.lua`, then every mod's `data-updates.lua`, then every `data-final-fixes.lua`. Mods can adjust each other without a strict ordering, and the engine records which mod changed which prototype.
  - Recoil games use `unitdefs_post.lua` for programmatic normalisation over all defs.
- **How.** `fl_content::load` runs these phases, each with path-aware errors:
  1. Discover mods → resolve `depends`/`load_after` → a total order. Break ties by mod id.
  2. Parse raw documents into an untyped tree (`ron::Value`).
  3. **Migrate** each document from its mod's declared `format_version` to the current one (code transforms on the tree, N-to-N+1 chain).
  4. **Patch**, pass 1 then pass 2 (`phase: Final`), over unresolved documents in mod order. Ops: `Set(path, value)`, `Merge(path, map)`, `Remove(path)`, `Add(path, value)`, `AddTag`, `RemoveTag`, `If(exists(path) | mod_loaded(id), [...])`. Record provenance (mod, file) per touched path for error messages and the "who changed this" debug view.
  5. Resolve inheritance per component (as designed).
  6. Deserialize typed defs (`deny_unknown_fields`).
  7. Feature **post-process hooks** (`register_content_post::<UnitDoc>(fn)`) for derived data, run in feature-ID order.
  8. Link → validate → freeze and intern → canonical rules hash.
  ```ron
  // mods/realism/patches/armor.patch.ron
  Patch(target: "base.de.pz3", ops: [
      Set("components.Hull.hp", 180),
      Merge("components.Armor", { "front": (45, 60) }),
      Remove("components.Supply"),
      AddTag("elite"),
  ])
  ```
  Keep `override: true` for whole replacement.
- **Effort.** M.

### N11. Data verbs: open registries for effects, triggers and considerations
- **Why.**
  - Rust features are compile-time, so a data-only modder can only use verbs that we register. RimWorld modders reach for Harmony IL patching when the hooks run out, and Rust has no equivalent.
  - The lever is a rich, composable data vocabulary, like OpenRA's weapon *warheads* lists.
- **How.**
  - The same registry mechanism as def components, for:
    - (a) **effects** on shells, abilities and triggers: `effects: [Damage(..), Suppress(..), GrantCondition("burning", 5s), SpawnActor(..)]`.
    - (b) **trigger conditions and actions** for `map.ron`.
    - (c) later, **AI considerations**.
  - Each verb is a registered `(name, DeserializeSeed, executor)`. Executors run in the `Resolve::Effects` sub-sets.
- **Effort.** S for the mechanism. The verbs arrive with features.

### N12. Tags as bitsets plus class expressions
- **Why.** Tags already exist as strings. Targeting rules, modifier filters (0 A.D. `Affects`), AI and UI all need fast checks such as "Infantry and not Sniper".
- **How.**
  - A `TagId` registry. A `Tags(FixedBitSet or u128)` runtime component; widen it when there are more than 128 tags.
  - Class expressions compile at link time to `(all_of, none_of, any_of)` masks.
- **Effort.** S.

### N13. Event model rules
Write these into an ADR and enforce them in review. All facts below are [src].

| Mechanism | What it is in 0.20 | Use in `fl_sim` | Use in `fl_client` |
|---|---|---|---|
| **Component hooks** (`on_add/on_insert/on_discard/on_remove`) | One per component, run on command application, can't be overridden by other crates | Maintain the owning crate's **invariants and indices**: spatial-grid membership, `SimId` → `Entity` index, relationship bookkeeping (relationships are built on hooks) | Rarely |
| **Messages** (`MessageWriter/Reader/Mutator`) | Buffered, pull-based, read by ordered systems; rotated by `message_update_system` | **Default for gameplay events** (`UnitDestroyed`, `ShotFired`, `SpawnRequest`, damage pipeline). Deterministic because the reading systems are ordered. Cheap in bulk | Draining `SimEvents` for VFX and sound |
| **Observers** (`On<E>`, `EntityEvent`) | Push-based and immediate. Several observers of one event are stored in `EntityHashMap<ObserverRunner>` (`observer/centralized_storage.rs`), so **their relative order is hash order of observer entities: deterministic for a fixed build, but unspecified** | **Only order-independent reactions**: insert a marker, push to a buffer. Never draw randomness, allocate `SimId`s or mutate shared state | **Default** for UI, picking (`On<PointerClick>`) and widget `Activate` |
| **Entity disabling** (`register_disabling_component`) | A global default query filter | Avoid. Hash, save and snapshot queries would silently skip disabled entities unless they all use `Allow<T>` | OK for prefab pools |

### N14. Relationships
- **Facts [src: `relationship/mod.rs`, `relationship_source_collection.rs`]:**
  - A `Relationship` is **one-to-many or one-to-one only**. Many-to-many isn't supported.
  - The target collection updates immediately through hooks.
  - `Vec<Entity>` targets append on insert, and `remove` uses an order-preserving `Vec::remove`, so the order is insertion order and deterministic.
  - `linked_spawn` despawns sources with the target.
  - `allow_self_referential` arrived in 0.19.
- **Mapping:**

| Gameplay | Source component (on) | Target collection (on) | Notes |
|---|---|---|---|
| Squad membership | `SquadMemberOf(Entity)` (soldier) | `SquadMembers(Vec<Entity>)` (squad) | The squad entity holds formation and orders |
| Garrison | `GarrisonedIn(Entity)` (squad/soldier) | `Garrison(Vec)` (building or trench) | The fire-slot assignment is a separate component |
| Transport | `PassengerOf(Entity)` | `Passengers(Vec)` (truck, halftrack) | |
| Towing | `TowedBy(Entity)` (gun) | `Towing(Entity)` (tractor), one-to-one | Inserting a second tow replaces the first: validate before inserting |
| Gun crew | `CrewOf(Entity)` | `Crew(Vec)` | "No crew → neutral" is a Lifecycle rule |
| Ownership | `OwnedBy(Entity)` (player entity) | `Owned(Vec)` | Capture = re-insert. Player state lives on player entities |

- **Rules:**
  - Don't use `ChildOf` in the sim. The client and third-party plugins attach transform and visibility semantics to it.
  - Don't use `linked_spawn` for gameplay. Whether passengers die or bail out is a rule, so handle it in `Lifecycle`.
  - Save relationships as `SimId`.
  - Embarked units get an explicit `Embarked` marker (SparseSet storage) and leave the spatial index. Use `Without<Embarked>` where needed, not disabling.
- **Effort.** S.

### N15. Unit behaviour: order queue plus activity stack
- **Why.**
  - OpenRA's `Activity` design has held up across many mods. Each activity is `Queued → Active → Canceling → Done`, and `Tick` returns done.
  - Child activities run first (`QueueChild`), successors chain (`Queue`), `Cancel(keepQueue)` handles interruption, and `IsInterruptible` blocks it.
  - That gives Shift-queue, interruptions (go prone, bail out) and composite orders (Load = MoveTo + Enter) cleanly.
- **How.**
  - `OrderQueue(VecDeque<OrderInstance>)` comes from commands.
  - `ActivityStack(SmallVec<ActivityInstance; 4>)` holds `(ActivityTypeId, payload index)`. Activity types are registered by features like command kinds.
  - Store the state as a **table-stored enum or ID**, not marker components. Transitions are then plain writes with no archetype moves.
  - Use **SparseSet markers** only for rare states that need fast filtering, such as `Burning` or `Embarked`.
  - Per-activity data lives in per-type components keyed by the activity.
  - Behaviour trees and utility AI are for AI controllers (§2), not per-unit core behaviour.
  - Crates: `seldom_state` is at Bevy 0.18 and `big-brain` at 0.15 [crates.io], and none is designed for lockstep. Write it in-house; it is about 300 lines.
- **Effort.** M.

### N16. Fitness functions in CI
1. **Per-crate `clippy.toml` in every sim crate** (`fl_math`, `fl_protocol`, `fl_content`, `fl_sim`, `fl_feat_*`). Clippy only reads the nearest file and doesn't merge [probe], so generate them from one template with `xtask`. Verified that `f32`, `f64`, `HashMap` and `Instant` are flagged:
   ```toml
   disallowed-types = [
     { path = "f32", reason = "sim is fixed-point" }, { path = "f64", reason = "sim is fixed-point" },
     { path = "std::collections::HashMap", replacement = "std::collections::BTreeMap" },
     { path = "std::collections::HashSet", replacement = "std::collections::BTreeSet" },
     { path = "bevy_platform::collections::HashMap", reason = "iteration order not part of our contract" },
     { path = "std::time::Instant" }, { path = "std::time::SystemTime" },
     { path = "bevy_ecs::system::ParallelCommands", reason = "nondeterministic command order" },
   ]
   disallowed-methods = [ { path = "std::time::SystemTime::now" }, { path = "std::thread::spawn" },
                          { path = "rand::rng" } ]
   ```
   Also add `clippy::float_arithmetic = "deny"` in the sim crates' `[lints]`. It catches float literals and arithmetic that `disallowed_types` misses [probe]. Exempt `fl_content`'s RON-to-`Fx` conversion module with a scoped `#[expect]`.
2. **`xtask arch-check`** (`cargo_metadata` 0.23.1):
   - A table of allowed edges: `fl_math` → none; `fl_protocol` → `fl_math`; `fl_feat_*` → core sim crates only, plus declared feature dependencies; `*_client` → anything client-side; no `fl_feat_*_client` → another feature's sim internals.
   - Per sim crate, run `cargo tree -p <crate> -e normal,build --prefix none` *one package at a time*, because unification hides violations [probe]. Fail on `bevy_render|bevy_window|bevy_input|bevy_time|winit|wgpu|bevy_internal`.
3. **Per-package headless jobs:** `cargo test -p fl_sim -p fl_feat_infantry ...`, with no client crates selected. This proves the sim compiles headless.
4. **`cargo-deny` 0.20.2:** licenses (no NC, ND or SA, matching the asset policy), advisories, `multiple-versions = "deny"` for `bevy_*` (catches a plugin pulling a second Bevy), and `[bans] deny = [{ crate = "bevy_dynamic_plugin" }, { crate = "bincode", reason = "discontinued, see C14" }]`.
5. **Ambiguity detection at `Error`, plus a shuffle-seed CI job** (`ScheduleBuildSettings::shuffle_seed`, needs the `debug` feature) running the determinism twin test.
6. **`bevy_lint`** is *not* ready: the latest `bevy_lint` v0.6.0 supports Bevy 0.18 only (TheBevyFlock/bevy_cli releases). Adopt it later (L-list).
- **Effort.** M overall. Item 1 alone is S.

### N17. Contract snapshots
- **Why.** Internal crates aren't published, so semver discipline means "no surprise changes to the contracts other agents build on".
- **How.**
  - `cargo-public-api` 0.52 diffs the public API of `fl_protocol`, `fl_view` and the `fl_sim` extension API (SimBuilder, phases, registries), stored as `insta` 1.49 snapshots.
  - `serde-reflection` 0.6 snapshots the *wire and save formats* of `PlayerCommand`, payloads and save codecs. A format change without a version bump fails CI.
  - `cargo-semver-checks` 0.51 works on stable with `--baseline-rev`, but its README says it misses type changes of fields and parameters, so treat it as optional.
- **Effort.** S.

### N18. ADRs, deprecation policy, ownership map
- **ADRs:** `docs/adr/NNNN-title.md` using MADR 4.0 (released 2024-09-17). Write the first ten from this report:
  - two-crate features
  - threaded sim
  - keyed RNG
  - stable registries
  - event-model rules
  - modifier stacking order
  - save format
  - replay compatibility policy
  - content load phases
  - AI placement
- **Deprecation:**
  - Rust contract crates: `#[deprecated(since = "M3", note = ...)]`, removed one milestone later.
  - Content keys: `#[serde(alias)]`, plus a `data-check` warning listing deprecated keys, plus a content migration (N10 step 3) when removed.
  - Saves: migrations are kept forever, or until an explicit save-break ADR.
- **Ownership for parallel agents:** `OWNERS.toml` maps paths to tracks. `fl_protocol`, `fl_view`, `fl_sim::{phases, registries}` and `fl_content` loader phases are **contract** paths: they change only through an ADR and the orchestrator. An `xtask check-scope --track <name>` fails a branch that touches paths outside its track. Snapshots (N17) make contract changes visible in review.
- **Effort.** S.

### N19. Debug actions are commands
Spawn, set owner, invulnerable, reveal-all (as a *vision* override), set HP, teleport and pause-with-orders all become `DebugCommand` kinds in the protocol. Replays need a `cheats_allowed` header flag, and lockstep peers must agree on it. Read-only inspection goes through the snapshot plus a debug query channel to the runner. Effort S.

### N20. Lockstep-ready command envelope
- **Why.**
  - Lockstep needs these: input delay (inputs sampled for a future tick; "the player sees their action about two frames late"), waiting for the slowest peer, deterministic ordering, and dropping or ignoring stale references (SnapNet, "Netcode Architectures Part 1").
  - Single-player should use the same path, so that bugs show up in single-player too.
- **How.**
  - `PlayerCommand { exec_tick, player, seq: u32, units: Vec<SimId>, kind: (CommandTypeId, Payload), queue }`.
  - The driver schedules local input at `now + input_delay_ticks`. Use 1 in single-player and an adaptive 2–6 in MP (100–300 ms at 20 Hz).
  - Within a tick, apply in `(player, seq)` order.
  - Handlers must tolerate dead or foreign `SimId`s: filter, then log.
  - Use `postcard` 1.1.3 for the wire format.
- **Effort.** S.

### N21. Benchmarks and profiling
- **Why.**
  - A 20% wall-clock regression gate on shared CI runners will flake.
  - Instruction counts are stable. `gungraun` 0.20 (the renamed `iai-callgrind`, Valgrind-based, Linux) gives one-shot, consistent counts.
- **How.**
  - CI gate: gungraun over fixed sim scenarios, with a regression threshold on instructions.
  - Dev machine: criterion 0.8.2 wall-clock baselines in `docs/perf/` (as designed).
  - Tracy: `bevy/trace_tracy` gives per-system spans for free. Add `info_span!` around budgeted queues (path, vision).
  - `bevy_ecs` `track_location` + `Ref::changed_by()` [src] tell you *which line* last changed a component. That is invaluable for desync hunts, so enable it in debug builds.
  - Use `DiagnosticsOverlay` (`bevy_dev_tools`) and `RenderDiagnosticsPlugin` [src] in the testbed.
- **Effort.** S–M.

### N22. Render scale baseline (0.20)
- **Facts [src]:**
  - GPU preprocessing with **GPU culling is on by default** (`GpuPreprocessingMode::Culling`) where compute shaders exist.
  - `StandardMaterial` is `#[bindless]`.
  - `MeshTag` passes a per-instance `u32` to shaders.
  - New in 0.20: `GpuComponentArrayBuffer`/`GpuComponentArrayBufferPlugin<C>` exposes per-instance component data to shaders without breaking batches.
  - `VisibilityRange` gives HLOD with dithered cross-fade.
  - `OcclusionCulling` is a camera component.
  - 0.19 moved LOD range checks to the GPU and batched depth prepasses.
- **How:**
  - One mesh and material per unit *type*. Faction colour, damage tint, selection highlight and fog fade come from `MeshTag` → palette, or from a `GpuComponentArrayBuffer` component read in an `ExtendedMaterial` (WESL).
  - Never a material per unit.
  - Three `VisibilityRange` levels per visual: full model, low-poly, and a billboard or merged squad impostor.
  - One shadow cascade for the orthographic iso camera.
  - Update transforms only for on-screen entities (as designed).
- **Effort.** M.

---

## 2. Adopt later (with triggers)

| # | Pattern | Trigger | Effort |
|---|---|---|---|
| L1 | **AI controllers outside the sim**: commanders run on one machine and emit `PlayerCommand`s through the normal (delayed) path | First AI faction | L |
| L2 | Layered AI: utility for target and priority choice, HTN for doctrine and build orders, influence maps for spatial reasoning; time-sliced | Same | L |
| L3 | Lockstep net crate: thin custom session over a transport, or `lightyear` 0.31 input-only P2P | MP milestone | L |
| L4 | Desync tooling: per-phase and per-component hashes, auto state dump, `fl_cli desync-diff` | With L3 (cheap in the skirmish phase too) | M |
| L5 | Campaign meta layer as a separate app state with its own save; match = `ScenarioSetup → MatchResult` | Campaign milestone | M |
| L6 | Economy and resource-graph core: stocks, flows, networks, reservations, sleeping entities | Base building or colony milestone | L |
| L7 | Vertex-animation-texture infantry + animation LOD | More than ~1,000 animated soldiers on screen | L |
| L8 | `.bsn` visual prefabs and UI in BSN; DefComponent → `Template` adapter | Bevy ships the `.bsn` loader (0.20 lists it as future work) | M |
| L9 | `bevy_lint` | It supports 0.20 | S |
| L10 | Scripting tier (Lua through `mlua`, or WASM) for missions and AI, commands-only API | Campaign scripting need | L |
| L11 | Late join and reconnect by state transfer (reuses N7) | MP polish | M |
| L12 | Content schema export for modders (`schemars` 1.2 → docs and editor hints) | First external modders | S |
| L13 | Projectile pools as SoA resources instead of entities | Profiling shows spawn/despawn churn | M |
| L14 | Entity update skipping with "movement intention" plus client-side prediction (Factorio robots skip up to 20–60 ticks) | Tick budget pressure from idle or travelling units | M |

Notes on the most important ones:

- **L1, where AI runs.** The design says an AI "reads a per-player view of the sim and emits PlayerCommands" but not *where* it runs. Recommendation: outside `SimTick`, as a controller on one designated peer (host, or round-robin per AI player). Its commands go through the lockstep envelope like a human's and are recorded in replays.
  - **Benefits:** AI may use floats, threads, async planning and time-slicing; it needs no determinism; it can't desync; and it can be swapped without changing the rules hash.
  - **Cost:** its commands get input delay. That is fine at RTS cadence.
  - **Requirement now:** `PlayerView` is built in `Publish` from fog-filtered state. It belongs in `fl_view` so AI and UI share one read model.
  - **Unit-level reflexes stay in the sim** (auto-target, prone under fire). Commander logic does not.
- **L2.** A 2024 ISART paper and industry guides converge on the same layering: strategic, operational, tactical.
  - Utility scoring suits competing goals (Dave Mark's Infinite Axis Utility System).
  - HTN suits designer-authored doctrine and fits our data-first content: methods and tasks in RON, registered primitive tasks.
  - GOAP's search cost and lower designer control make it a poor first choice.
  - Influence maps are per-team grids derived from `PlayerView` and the existing vision grid.
  - Bevy crates: `bevior_tree` 0.12 supports 0.20; `big-brain` (0.15), `bevy_dogoap` (0.16) and `bevy_behave` (0.18) don't [crates.io]. Since AI lives outside the sim, these are allowed but optional.
- **L3.** Prior research proposed `lightyear` 0.31 (bevy_app/bevy_ecs ^0.20 [crates.io]). It supports input-only deterministic replication and, since 0.29, input-only P2P with a fixed peer count. But it brings its own tick and timeline model into the client `App`. Our sim tick lives in a separate threaded world (N2), and our "inputs" are sparse commands, not per-tick input state. So evaluate with a spike. A thin custom lockstep (turns, acks, adaptive delay, a hash exchange every N ticks) over `quinn` or Steam is often simpler for RTS. Keep the transport behind a `NetSession` trait (Iron Curtain's `NetworkModel` idea).
- **L6, economy patterns that fit the tick budget.**
  - Resources are content (`ResourceId` registry). Use **stock** components (`Inventory(SmallVec<(ResourceId, Fx)>)`) and **flow** records.
  - Solve **networks** (supply chains, power, logistics) per network in aggregate, not per entity. Factorio groups entities that share network state for determinism and found network updates memory-bound (FFF #421).
  - **Sleep** idle producers and wake them on events (Roboport sleeping cut update cost from about 1 ms to 0.025 ms).
  - **Bucket** periodic work across ticks (radar chunks).
  - **Reservations** (RimWorld-style) for colonist and truck jobs, so two haulers never claim the same item.
  - All of this goes in the `Economy` phase (N5). Don't hard-code supply RU (§2 anti-goal); RU is just the first `ResourceId`.
- **L5, campaign.**
  - The meta layer is a separate `App` state that owns `CampaignState` (roster with experience, unlocks, rank) and never lives in the sim `World`.
  - It creates `ScenarioSetup { map, players, persistent_units: [(def_id, saved_state)] }`. The sim instantiates persistent units through the normal def path plus saved-state codecs (N7). On `MatchEnded` the sim publishes a `MatchResult`.
  - Campaign saves are a small separate format, versioned the same way and optionally embedding a match save.
  - Tech trees: the unlock graph is content. Researching *between* matches is meta. Researching *in* a match is a sim feature that applies player-scope modifiers (N9) and roster unlocks.

---

## 3. Design critique: specific changes

References are to sections of the design doc.

| # | Where | Problem | Change |
|---|---|---|---|
| C1 | §5.1 "client module behind a `client` Cargo feature"; §5.2 `GameFeature` | Workspace feature unification (verified; the opt-out is unstable) means the mandated `--workspace` gates compile sim halves with client code. Per-crate lint and dependency rules can't separate the halves | N1: two crates per feature; `SimFeature` + `ClientFeature` with `ID`, `VERSION`, `deps` |
| C2 | §5.3 "SubApp or client-held world stepped from FixedUpdate"; §14 | Both run the sim synchronously on the main thread, and a 15 ms tick breaks 60 fps. The "sync step" couples the client to sim component types | N2: threaded `SimRunner`, `fl_view` snapshot protocol, `TickDriver`. Resolve the §14 open question in an ADR |
| C3 | §5.4 "PCG RNG in a sim resource; draws in system order"; per-stream derivation only for parallel phases | Global draw-order coupling between features; replays perturbed by unrelated changes; blocks parallelism | N3: keyed RNG everywhere; ban the sequential RNG in the sim |
| C4 | §5.4 "Any entity iteration that affects state is sorted by SimId"; "pure compute → buffer → ordered apply" | Over-broad. Sorting 3,000 entities per system costs time, and the rule forbids safe parallelism: `par_iter_mut` writing only the iterated entity's own components is deterministic | Narrow the rule to *order-sensitive* operations: shared writes, spawns, `SimId`s, budgets and queues, cross-entity reads of same-tick writes. Keep a `SimIndex` (sorted `Vec<Entity>`) maintained by hooks for those. With N3, randomness is no longer order-sensitive. The branch test (N7) enforces it |
| C5 | §5.5 `CommandTypeId(u16)`; §5.2 registries | ID assignment unspecified; registration order would make IDs build-dependent | N4: sorted stable-string registries; string tables in headers |
| C6 | §5.4 "State hash: registered components"; §5.6 `Hash` set | Unregistered components evade the hash. Resources (tick, `SimId` allocator, player stocks, the vision grid) aren't mentioned. The hash inside the schedule invites later systems | N6 classification; hash components *and* resources; run the hash from the driver after the schedule |
| C7 | (absent) Save/load | Campaigns, reconnects, late join and long testbed sessions need it, and retrofitting it is the classic failure | N7 now, with the round-trip and branch tests |
| C8 | §5.6 `ApplyCommands → … → Hash` | No phase for economy, production, construction or publishing. Auto sync points make visibility timing depend on unrelated features. Manual stepping skips message rotation and `clear_trackers` | N5 phases, hook sub-sets, explicit `ApplyDeferred`, housekeeping, spawn funnel |
| C9 | §5.4 "Rules hash over canonical resolved content" | It doesn't cover the code: a peer with a different feature set or version desyncs late, and only the state hash notices. "Canonical" isn't defined | The handshake and replay header carry `build_id`, protocol version, `features[(id, version)]`, `mods[(id, version)]` and the rules hash. Canonical form = a stable binary encoding of resolved defs (sorted keys, raw `Fx` bits), never `Debug` |
| C10 | §8 "Stat modifiers are a core concept" | Unspecified, so the most-shared mechanism will be invented ad hoc per feature | N9 spec plus ADR, built in M2 before experience, morale and formations |
| C11 | §6.1 inheritance + §6.4 `override: true` | Mods can't compose edits; whole-component replacement is painful for modders | N10 phases, patch ops, post hooks, `format_version` and content migrations |
| C12 | §11 Spawn panel, invulnerable toggle, switch player, "reveal all" | Client mutation of the sim; testbed sessions can't be replayed | N19 `DebugCommand`s plus a `cheats_allowed` header flag. "Reveal all" is a client render override or a debug vision command, never a sim mutation |
| C13 | §8 AI: "reads a per-player view and emits commands" | Placement unstated. If it runs inside `SimTick`, all AI code must be deterministic and fixed-point and it eats the 15 ms budget | L1: outside the sim on a designated peer; `PlayerView` in `fl_view` |
| C14 | Prior research §2: `fl_protocol` "serde/bincode" | **bincode is discontinued**: 3.0.0 is only a notice with a compile error, saying "development on bincode has ceased" | `postcard` 1.1.3 (documented stable wire format). Ban bincode in cargo-deny |
| C15 | §5.4 "System-level parallelism only once ambiguity checks pass" | It doesn't say the executor kind. In the client binary `bevy_ecs/multi_threaded` will be unified in, and headless may differ | Set `set_executor_kind` explicitly in `build_sim_app`. Note that the sim's parallel systems share Bevy's global task pools with the client unless the runner thread uses its own pool |
| C16 | §6.4 Hot reload rebuilds `ContentDb` and re-instantiates stats | Interning reruns. If the ID set changes, live entities hold stale dense IDs | Value-only reloads in place; ID-set changes trigger a match rebuild from a snapshot (N4, N7). Record reloads as replay events or mark the replay "non-reproducible" |
| C17 | §5.7 "3,000 units" | Ambiguous. Infantry squads of 6–10 soldiers, crews, projectiles and wrecks can make the real entity count several times higher | Define the target as entities by kind (for example 3,000 units ≈ 8–12k sim entities plus N projectiles in flight). Benchmark that mix. Consider projectile pools (L13) |
| C18 | §12 "20% tick-time regression fails the bench check" | Wall-clock on CI is noisy | N21: instruction-count gate in CI, wall-clock baselines locally |
| C19 | §5.7 "sync step copies only entities that changed (change ticks)" | Change ticks are per-world and per-system `last_run`. A sync running outside a system in another world gets this wrong easily | Compute deltas *inside* the sim in `Publish` with `Changed<T>` (N2). Use `#[component(summary_tick)]` only if profiling shows the snapshot scan matters. It only speeds up `contiguous_iter` paths and is table-only [src] |
| C20 | §5.1 "CI checks dependency rules with `cargo tree`" | A workspace-wide `cargo tree` reflects unified features | Per-package `cargo tree -p`, run one package at a time (N16.2), plus per-package headless test jobs |
| C21 | §7 Infantry garrison / transport / towing (M4, M7) | No entity-graph model chosen; ad hoc `Vec<SimId>` fields drift out of sync | N14 custom relationships, with explicit eject and death rules in `Lifecycle` |
| C22 | §5.2 "Contribute command-panel buttons declaratively … which def component enables the button" | Enablement by def-component presence alone can't express "only when deployed" or "only with ammo" | Buttons carry `requires: <condition expr>` (N8) plus the def-component check |
| C23 | §6.1 `tags: [...]` strings | No defined semantics or speed | N12 bitset tags plus class expressions, used by targeting, modifiers, AI and UI |
| C24 | §13 parallel subagents "each owning separate crates or modules" | Nothing enforces it; contract drift is invisible | N17 snapshots, N18 `OWNERS.toml` + `check-scope`, contract paths behind ADRs |

Smaller notes:
- **`deny_unknown_fields` vs. forward compatibility.** Keep it for content: strictness beats leniency for modders. Saves must instead be versioned (N7), never "best effort".
- **Required components.** Use `#[require]` for *bookkeeping* invariants (`Mobile` requires `Velocity` and `PathState`; anything spatial requires `SpatialCell`). Never use it to supply gameplay defaults that should come from defs, because that masks missing content. 0.20 lists "more powerful required components, integrated with the `Template` trait" as future work, so expect changes.
- **BSN.**
  - `bsn!` (0.19) and its syntax "should largely be nailed down" (0.20), but there is no `.bsn` asset loader yet.
  - `Template`/`FromTemplate` live in `bevy_ecs` [src] with the shape `build_template(&self, &mut TemplateContext{entity: &mut EntityWorldMut, ..}) -> Result<Output>`: repeatable and clonable.
  - Shape `DefComponent::instantiate` the same way, so a later adapter is trivial. Don't depend on it from `fl_content` yet.
  - Use `bsn!` in the client for HUD and widgets now.
- **Plugin hygiene.**
  - Build registries in `build()` and freeze them in `finish()`.
  - Every crate exposes its public `SystemSet`s and documents their order (Bevy plugin guide).
  - Plugins never reach into each other's internals; only contract types cross crates.
  - `default-features = false` on Bevy in every crate, listing features explicitly (Bevy plugin guide).

---

## 4. Topic notes

### 4.1 Bevy 0.20 ECS for large projects
- **Plugin ordering.**
  - Bevy builds plugins in `add_plugins` order and calls `ready`, `finish` and `cleanup` afterwards [src]. There is no dependency solver.
  - Make order irrelevant: use registries frozen in `finish`, sorted IDs, and explicit set ordering. Keep a topological sort in `GameBuilder` (N1).
- **System sets.**
  - Coarse public phases, fine private sub-sets. Prefer `configure_sets` ordering to cross-crate `.before(system)` (Bevy best-practices repo; Bevy plugin guide).
  - 0.20's `chain_weak()`/`before_weak()` only order systems with conflicting access. That is fine in the client, but in the sim prefer strict ordering for readability. Determinism holds either way when ambiguity detection is at `Error`.
  - `remove_systems_in_set` (0.18) exists. Don't use it in the sim; gate features by content presence instead.
- **Observers vs messages vs hooks.** See N13. The 0.17 rationale for the split: they are "fundamentally very different things functionally". Observers gained `run_if` in 0.19.
- **Relationships.** See N14.
- **State machines.**
  - Enum or ID in table storage for frequent transitions (no archetype move).
  - SparseSet markers for rare, filterable states (`StorageType::SparseSet` gives "fast addition and removal … slower iteration" [src]).
  - Behaviour trees and utility AI for controllers.
  - 0.20 lists "mutually exclusive components" as future work. Revisit marker-based states then.
- **Change detection at scale.**
  - Scanning `Changed<T>` over about 10k entities at 20 Hz is cheap. Real costs are archetype churn and allocation.
  - 0.19 added `contiguous_iter`, and 0.20 added `summary_tick` to skip unchanged tables.
  - Use these for the snapshot and publish step only if profiling shows a need.
  - `despawn_all` (0.20) batches despawns for wreck cleanup.
- **Error handling.** 0.20 catches panics in systems, observers and commands and routes them to `FallbackErrorHandler` (which re-panics by default). In the sim, keep the re-panic in tests and CI. In release, log and *mark the match desynced* rather than continuing silently.

### 4.2 Lessons from moddable, long-lived games

| Game | Mechanism | Why it scaled | Copy |
|---|---|---|---|
| OpenRA | YAML actors = trait bags; `Inherits`, `-Trait`, `@suffix` instances; **conditions** with `RequiresCondition`; Activities | Traits don't know each other; conditions and multipliers connect them | Conditions (N8), activity stack (N15), multiple instances of a def component (`"Turret@hull"`) |
| Factorio | settings → data → data-updates → data-final-fixes, all mods per round; control stage `on_init`/`on_configuration_changed`/migrations; strict `on_load` rules; heavy mode | Cross-mod edits without load-order fights; migrations tracked per save; save/load treated as testable | Load phases (N10), save migrations plus branch test (N7), sleeping and bucketed updates (L6) |
| RimWorld | XML Defs with `ParentName`/`Abstract`; PatchOperations (XPath) before inheritance; `DefModExtension`; Harmony for code | Patching instead of overwriting fixed mod conflicts; mod extensions add data to others' defs | Patch ops (N10). For mod extensions, a mod may attach a registered def component to another mod's unit through a patch |
| Paradox (HOI4, Stellaris) | Named modifiers summed per scope; `modifier_definitions` with display metadata; dynamic modifiers evaluated in insertion order; on_actions | One vocabulary for every bonus source; data defines new modifier keys | Modifier spec (N9), stats registry with display metadata, event hooks as Messages |
| 0 A.D. | XML templates with `parent`; JS components with message passing; `ModifiersManager` (`Paths`/`Affects`/`Add`/`Multiply`/`Replace`, stackability); cached modified values | Path-addressed modifications decouple tech and auras from components | Class-expression filters (N12), cached effective stats, stackability policies |
| Recoil (BAR) | defs pipeline (`unitdefs_pre`, defs, `unitdefs_post`); `customParams`; synced and unsynced gadgets; `layer` ordering | Post-processing normalises everything; synced/unsynced split mirrors sim/client | Post-process hooks (N10.7), our sim/client crate split (N1), explicit feature order |

### 4.3 Later layers: what to reserve now
- **Lockstep.** Execution-tick commands with sequence numbers (N20); a `TickDriver` that can wait on input (N2); a hash exchange every N ticks (already designed); build, feature and mod identity in the handshake (C9); state transfer for late join (N7 → L11).
- **Desync debugging.**
  - Checksum per tick and within ticks to localise indeterminism (SnapNet). On mismatch, break the hash down by subsystem and component type (BugNet guide).
  - Our version, cheap to build now:
    - In debug builds, hash after each phase and per component type.
    - On mismatch, dump both worlds' saves (N7) and run `fl_cli desync-diff`.
    - Turn on `track_location` to see the mutating line.
- **Saves and schema evolution.** See N7 and N10.
  - Content uses `format_version` per mod and tree migrations.
  - Saves use per-component versions and typed migrations.
  - Replays are **only guaranteed on the same `build_id`** (the industry norm). Record it in an ADR, and keep the golden replays regenerated by a script.
- **Campaign separation.** See L5.
- **Economy.** See L6.

### 4.4 Erosion control in one picture
1. Compile-time: crate boundaries (N1); `pub(crate)` by default; `#[non_exhaustive]` on contract enums and structs.
2. Lint-time: per-crate `clippy.toml` bans and `float_arithmetic` (N16.1).
3. CI structural: `xtask arch-check` (allowed edges, per-package trees), per-package headless tests, cargo-deny.
4. CI behavioural: determinism twin run, shuffle-seed run, save round-trip and branch tests, golden replays, `data-check`, component-classification check (N6).
5. CI contractual: public-API and serde-format snapshots (N17).
6. Process: ADRs, deprecation windows, the ownership map and scope check (N18).

### 4.5 Performance architecture for 3,000 units
- **Sim.**
  - The algorithms dominate, not ECS iteration. The design's spatial grid, vision refcounts, budgeted pathing and staggering are right. Factorio's radar uses the same "per-chunk counter + update buckets" idea.
  - Add:
    - Movement intentions with skipped updates for cruising units (L14).
    - Aggregate network solving (L6).
    - Projectile pools (L13).
    - `par_iter_mut` for per-entity-local updates, once C4 is narrowed.
  - Keep hot components small and `Copy` in tables, and transient flags in SparseSet.
- **Render.**
  - GPU-driven by default in 0.20; bindless `StandardMaterial`; batching by mesh and material.
  - Keep per-instance variation in `MeshTag`/`GpuComponentArrayBuffer`; `VisibilityRange` HLOD (N22).
  - The 0.19 notes report `many_cubes` with culling going from 49.47 ms to 18.77 ms from GPU-side batching work.
- **Animation.**
  - Tanks are rigid hierarchies (turret and barrel transforms), which is cheap.
  - Skinned infantry is the risk: per-entity `AnimationPlayer` sampling plus joint transform propagation. 0.19 also computes skinned bounds from joints every frame.
  - Plan: animation LOD (lower update rate or a frozen pose far away), then VAT for distant soldiers (L7).
  - Track Bevy's stress-test numbers (metrics.bevy.org, `many_foxes`).
- **Profiling.** Tracy (`trace_tracy`), per-system spans, gungraun in CI, criterion locally, `DiagnosticsOverlay` in the testbed. Profile the sim headless and the client with the sim stubbed by a recorded snapshot stream, so the two budgets are measured separately.

---

## 5. Crate and tool versions (crates.io, 2026-10-23)

"bevy req" is the Bevy requirement declared by the latest release.

| Crate or tool | Latest | bevy req | Use |
|---|---|---|---|
| bevy / bevy_ecs | 0.20.0 (2026-10-08) | – | engine |
| cargo-deny | 0.20.2 | – | N16 |
| cargo-semver-checks | 0.51.0 | – | optional (N17) |
| cargo-public-api | 0.52.0 | – | N17 (needs nightly rustdoc JSON) |
| insta | 1.49.0 | – | snapshots |
| serde-reflection | 0.6.0 | – | format snapshots |
| cargo_metadata | 0.23.1 | – | xtask |
| cargo-nextest | 0.9.149 | – | test runner |
| cargo-machete | 0.9.2 | – | unused dependencies |
| cargo-modules | 0.28.0 | – | module graphs |
| bevy_lint (bevy_cli) | v0.6.0 | **0.18 only** | later (L9) |
| bevy_mod_debugdump | 0.17.0 | ^0.20.0 | schedule graphs in docs |
| gungraun (ex-iai-callgrind) | 0.20.0 | – | CI benches |
| criterion | 0.8.2 | – | local benches |
| tracy-client / tracing-tracy | 0.19.0 / 0.12.0 | – | profiling |
| postcard | 1.1.3 | – | wire and save bytes |
| bincode | 3.0.0 = **discontinued notice** | – | do not use |
| ron | 0.12.2 | – | content |
| xxhash-rust | 0.8.19 | – | hashes, keyed RNG |
| fixed | 1.31.0 | – | Fx |
| indexmap | 2.14.2 | – | ordered maps |
| proptest | 1.12.0 | – | property and command fuzzing |
| schemars | 1.2.3 | – | L12 |
| moonshine-save | 0.8.0 | ^0.20 | client-only state (no versioning) |
| moonshine-behavior | 0.6.0 | ^0.20 | reference for stack behaviours |
| lightyear | 0.31.0 | bevy_app/ecs ^0.20 | L3 candidate |
| bevy_replicon | 0.45.2 | ^0.20 | not for lockstep |
| bevy_ggrs | 0.22.0 | ^0.19 | no |
| bevior_tree | 0.12.0 | ^0.20 | AI option (L2) |
| big-brain / bevy_observed_utility / bevy_dogoap / bevy_behave | 0.22 / 0.2 / 0.5 / 0.5 | ^0.15 / ^0.15 / ^0.16 / ^0.18 | no |
| seldom_state | 0.16.0 | ^0.18.1 | no |
| bevy_gauge (stats) | 0.5.1 | ^0.19.0 | no (reference only) |
| bevy_animation_graph | 0.11.0 | ^0.19 | no |
| bevy_mod_skinned_aabb | 0.4.1 | ^0.18 | no (0.19 has built-in skinned bounds) |
| bevy_hanabi / bevy_mod_outline / bevy_tween | 0.19 / 0.13 / 0.13 | ^0.19 | no |
| bevy_rand / bevy_prng | 0.16.0 | ^0.20 | client only; sim uses keyed RNG |
| bevy_enhanced_input | 0.27.0 | ^0.20 | client input |
| avian3d | 0.8.0 | ^0.20 | client cosmetics only |

---

## 6. What I did not verify
- The `cargo-deny` `wrappers` workaround for "only client crates may depend on `bevy`": the docs describe `wrappers`, but third-party client crates that depend on the umbrella `bevy` crate would all need listing. I recommend the xtask instead.
- Exact observer execution order: I read the storage type (`EntityHashMap`), not a test of cross-build stability. The recommendation (no order-dependent observers in the sim) holds either way.
- Pipelined-rendering frame-time numbers on the dev machine. The sim-threading argument comes from code structure (`SubApp::update` is synchronous) plus the budget arithmetic, not a measurement.
- Whether `lightyear` 0.31's P2P mode fits a threaded sim world. Spike it in L3.
- `cargo-public-api` needs a nightly toolchain for rustdoc JSON. I didn't run it.

---

## 7. References

**Bevy (versions and sources)**
- Bevy 0.20 release notes: https://bevy.org/news/bevy-0-20/
- Bevy 0.19 release notes (BSN, resources as components, observer `run_if`, `contiguous_iter`, self-referential relationships, rendering-scale work): https://bevy.org/news/bevy-0-19/
- Bevy 0.18 release notes (`remove_systems_in_set`, feature collections): https://bevy.org/news/bevy-0-18/
- Bevy 0.17 release notes (Event/Message split rationale): https://bevy.org/news/bevy-0-17/
- Bevy plugin development guide: https://bevy.org/learn/quick-start/plugin-development/
- bevy_ecs 0.20 relationship module docs: https://docs.rs/bevy_ecs/0.20.0/bevy_ecs/relationship/index.html
- Local sources read: `bevy_ecs-0.20.0/src/{relationship,observer,message,entity_disabling.rs,template.rs,schedule/schedule.rs,component/mod.rs,change_detection}`, `bevy_app-0.20.0/src/{app.rs,sub_app.rs,plugin.rs}`, `bevy_render-0.20.0/src/{batching,gpu_component_array_buffer.rs,occlusion_culling,pipelined_rendering.rs}`, `bevy_camera-0.20.0/src/visibility/{mod.rs,range.rs}`, `bevy_mesh-0.20.0/src/components.rs`, `bevy_pbr-0.20.0/src/pbr_material.rs`
- Community best practices: https://github.com/tbillington/bevy_best_practices
- Tainted Coders, plugin organization: https://taintedcoders.com/bevy/patterns/plugin-organization
- Bevy stress tests and metrics: https://bevy.org/examples/stress-tests/many-foxes/ and https://metrics.bevy.org
- bevy_lint / bevy_cli releases: https://github.com/TheBevyFlock/bevy_cli/releases
- Tiny Glade (custom Bevy): https://en.wikipedia.org/wiki/Tiny_Glade

**Moddable and long-lived games**
- OpenRA trait reference (release-20250330): https://docs.openra.net/en/release/traits/
- OpenRA `Activity.cs`: https://github.com/OpenRA/OpenRA/blob/bleed/OpenRA.Game/Activities/Activity.cs
- OpenRA modding guide: https://github.com/OpenRA/OpenRA/wiki/Modding-Guide
- OpenRA forum on conditions and prerequisites: https://forum.openra.net/viewtopic.php?p=314176
- Factorio data lifecycle (2.1.23 docs): https://lua-api.factorio.com/latest/auxiliary/data-lifecycle.html
- Factorio FFF #421 "Optimizations 2.0" (2024-07-26): https://factorio.com/blog/post/fff-421
- Factorio FFF #270 (save/load determinism): https://cf-www.factorio.com/blog/post/fff-270
- Factorio FFF #340 (deep desyncs): https://cf-www.factorio.com/blog/post/fff-340
- Factorio FFF #188 (desync reports): https://factorio.com/blog/post/fff-188
- Factorio FFF #209 (optimisation): https://factorio.com/blog/post/fff-209
- Factorio wiki, desync and heavy mode: https://wiki.factorio.com/Desynchronization
- RimWorld PatchOperations: https://rimworldwiki.com/wiki/Modding_Tutorials/PatchOperations
- HOI4 modifiers: https://hoi4.paradoxwikis.com/Modifiers
- 0 A.D. ModifiersManager: https://docs.wildfiregames.com/javascript/simulation/ModifiersManager.html
- 0 A.D. entity component docs: https://docs.wildfiregames.com/entity-docs/a27.html
- Recoil widgets and gadgets: https://recoilengine.org/docs/guides/getting-started/widgets-and-gadgets/
- Recoil unit types basics (defs pipeline, `unitdefs_post`): https://recoilengine.org/docs/guides/getting-started/unit-types-basics/

**Netcode, AI, saves**
- SnapNet, "Netcode Architectures Part 1: Lockstep" (2023): https://www.snapnet.dev/blog/netcode-architectures-part-1-lockstep/
- BugNet, debugging desync in lockstep: https://bugnet.io/blog/how-to-debug-desync-in-deterministic-lockstep-games
- Lockstep chapter (input-delay figures): https://www.socratopia.app/library/math-for-game-devs-en/chapter-30
- lightyear: https://github.com/cBournhonesque/lightyear and https://github.com/cBournhonesque/lightyear/releases
- ISART (2024), A New Decision-Making System in RTS Games: https://www.isart.fr/wp-content/uploads/2024/10/A-New-Decision-Making-System-in-Real-Time-Strategy-Games_CNRS_RTS_ISART.pdf
- RTS AI layering overview: https://www.capermint.com/blog/real-time-strategy-ai-games/
- RTS AI Problems and Techniques: https://www.researchgate.net/publication/377312486_RTS_AI_Problems_and_Techniques
- Game AI Pro, Exploring HTN Planners through Example: http://www.gameaipro.com/GameAIPro/GameAIPro_Chapter12_Exploring_HTN_Planners_through_Example.pdf
- moonshine_save README (no versioning): https://github.com/Zeenobit/moonshine_save
- bincode 3.0.0 discontinuation notice: https://docs.rs/crate/bincode/3.0.0

**Erosion control and tooling**
- Clippy lint configuration (`disallowed-*`): https://doc.rust-lang.org/clippy/lint_configuration.html
- cargo-deny bans config: https://embarkstudios.github.io/cargo-deny/checks/bans/cfg.html
- cargo-semver-checks: https://github.com/obi1kenobi/cargo-semver-checks
- RFC 3692, feature unification (unstable): https://rust-lang.github.io/rfcs/3692-feature-unification.html
- MADR (4.0.0, 2024-09-17): https://adr.github.io/madr/
- Ford, Parsons, Kua, Sadalage, *Building Evolutionary Architectures*, 2nd ed. (fitness functions): https://www.oreilly.com/library/view/building-evolutionary-architectures/9781492097532/
- crates.io API (all version checks): https://crates.io/api/v1/crates/
