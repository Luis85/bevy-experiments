# Framework best practices: portable, extensible baseline (researched 2026-10-23)

Scope: how to turn `npc-framework` (Bevy 0.20, Rust 2024) into a baseline that (a) other Bevy projects can consume as a library, for example a large RTS that wants sophisticated units, and (b) a game can extend without editing the framework. All crate versions and Bevy requirements below were read from the crates.io API on 2026-10-23 (`/api/v1/crates/<name>/<ver>/dependencies`). They were not taken from READMEs, which are often stale.

Companion notes: `ai-prior-art.md` (BT and determinism background) and `bevy-0.20-cheatsheet.md`.

---

## 0. Portability: the framework as a library for other Bevy games (hard requirement)

### 0.1 How reusable Bevy libraries are shaped

| Library | Pattern worth copying |
|---|---|
| **avian3d 0.8** | `PhysicsPlugins::new(schedule)` takes any `ScheduleLabel` (default `FixedPostUpdate`). It is a `PluginGroup`, so users can `.set()` or replace individual plugins. Physics runs in its own `PhysicsSchedule` that the host places. [docs.rs/avian3d PhysicsPlugins](https://docs.rs/avian3d/0.8.0/avian3d/struct.PhysicsPlugins.html) |
| **bevy-tnua 0.33** | Every plugin takes a schedule in `new()` and defaults to `Default`. Backend-specific code lives in integration crates on top of a separate `bevy-tnua-physics-integration-layer` crate. This is the template for "framework emits intent, host backend executes it". [tnua migration guide](https://github.com/idanarye/bevy-tnua/blob/main/MIGRATION-GUIDES.md) |
| **bevy_replicon 0.45** | `RepliconPlugins` group; modules split into `shared`/`server`/`client`, with `client` and `server` features that compile out unused halves; no I/O, because transports are plugged in as backends through `shared::backend`; a `prelude`; public `ServerSystems`/`ClientSystems::{Receive, Send}` sets for the host to order against; a configurable `ServerPlugin::tick_schedule`; and a `test_app` feature. [docs.rs/bevy_replicon](https://docs.rs/bevy_replicon/0.45.2/bevy_replicon/) |
| **lightyear 0.31** | Many small `lightyear_*` crates (core, inputs, replication, deterministic, transport, …) behind one facade. Input integrations for `leafwing-input-manager` and `bevy_enhanced_input` are optional crates. Builds on replicon rather than duplicating it. [github.com/cBournhonesque/lightyear](https://github.com/cBournhonesque/lightyear) |
| **leafwing-input-manager 0.22** | `InputManagerPlugin::<A>` is generic over the host's own `Actionlike` type, so the library never owns the game's vocabulary. [docs.rs](https://docs.rs/leafwing-input-manager/latest) |

The common rules:
1. **The host chooses the schedule and tick rate.** A library never claims a global. Our `SimDriverPlugin` does the opposite: it overwrites `Time::<Fixed>` to 20 Hz, which would silently slow down Avian (`FixedPostUpdate`) and replicon's `ServerTick` in the host app.
2. **Public `SystemSet`s are the extension seams.** Hosts order their systems against them.
3. **Generic over host types** where the vocabulary belongs to the game: actions, need kinds, factions.
4. **Backends are plugged in**, not baked in.
5. **A facade crate with a `prelude`, feature flags per optional subsystem, one example per use case, and a Bevy↔crate version table.**

### 0.2 Separate the framework from game content

Today `npc_sim` mixes kernel code with villager content.
- **Kernel:** `ids`, `clock`, `rng`, `math`, `reservations`, `commands`, `journal`, `replay`, `hash`, `inspect`, `schedule`, and most of `needs`.
- **Villager content:** `villager.rs`, `actions.rs`, `scenario.rs`, `world.rs`.

The two are tied together in several places:
- `SimCommand::SpawnVillager` and `commands.rs` import `villager_bundle` and `VillagerTree`.
- `hash.rs` hard-codes `Eat`, `Work`, `Sleep` and the other content components.
- `NeedKind { Hunger, Energy }` and `PlaceKind` are closed enums.
- `npc_view` mentions villager types 51 times.

The result is that an RTS cannot use `npc_sim` without inheriting villagers.

**Target layout** (dependency arrows point down; nothing in a lower row may name anything in a higher one):

```
apps/village            apps/rts_units_demo          ← content + wiring only
content/village_content (villager defs, actions, tree RON, scenario)
npc_framework           facade: PluginGroup NpcPlugins, prelude, features
npc_view_helpers        optional: interpolation, VisualOf link, debug gizmos (generic)
npc_tools               journal sinks, replay CLI (npcctl), BRP methods, inspect JSON
npc_perf                (already generic)
npc_jobs                smart objects, Provides/advertisements, reservations, job queue
npc_needs               data-driven needs (NeedDef assets, hysteresis, decay)
npc_locomotion          MoveIntent/MoveStatus contract + a trivial straight-line mover
npc_ai                  BT IR + think (already generic)
npc_kernel              SimId, SimClock, SimTick/SimSet, rng, fixed math, command bus,
                        state-hash registry, determinism lints
```

**Extension points a host implements:**
- **Register content.** `app.register_action::<A>()`, `register_cond`, `register_need`, and `register_job_kind` take *stable string keys*; the integer ids are assigned at startup.
- **Commands.** Generic `SimCommand<C: GameCommand>`, or a typed-erased command registry, so `SpawnVillager` moves into content.
- **Hashing and saving.** `app.register_sim_component::<T: SimHash + Reflect>()`. Registration adds the component to the state hash and to inspect/save, so `hash.rs` no longer lists content components.
- **Locomotion backend.** The host consumes `MoveIntent` and writes `MoveStatus` (see §0.3).

### 0.3 Host integration seams

- **Movement and physics are owned by the host.** The framework writes `MoveIntent { goal, speed_class, tolerance }` and reads `MoveStatus { Running | Arrived | Blocked }`. `npc_locomotion` ships a deterministic straight-line or grid mover. An RTS swaps in its Avian, navmesh or flow-field adapter, the way tnua separates its integration layer from its backends. Actions such as `MoveToDest` then wait on `MoveStatus` instead of moving `SimPos` themselves.
- **Positions.** Keep `SimPos` (integer mm) as the canonical position in deterministic mode. Provide a `PositionSync` adapter in each direction:
  - `SimPos → Transform` for presentation (this exists today in `npc_view`);
  - `Transform → SimPos` for non-deterministic hosts where physics owns position.
- **Opt-in determinism.** Add a kernel feature or `DeterminismMode::{Strict, Relaxed}`:
  - *Strict*: integers only, SimId ordering, ambiguity = Error, and the hash checks.
  - *Relaxed*: an RTS that is server-authoritative or single-player may read `Transform`/f32 from physics. The framework's own logic stays integer either way, which costs little and keeps replays working.
- **Tick rate and schedule are configured, not constant.**
  - `NpcPlugins::new().tick_hz(20).run_in(FixedPostUpdate)`. The other choice is a framework-owned accumulator (`SimTime`) driven from `RunFixedMainLoop`, which never touches the global `Time<Fixed>`.
  - `TICKS_PER_SECOND` becomes a resource.
  - Speed and pause keyboard handling moves out of the library into the example apps, because a library must not bind Space or 1-4.
- **Coexistence in a foreign App.**
  - Prefix every public label (`NpcSimTick`, `NpcSet::*`).
  - Never `insert_resource` over something a host may already own; use `init_resource`, which `SimPlugin` already does.
  - Keep ambiguity = Error only on *our* schedule.
  - Expose sets the host can order against: `NpcSet::Inputs`, `NpcSet::Think`, `NpcSet::Actions`, `NpcSet::Effects`.
  - Run the sim schedule from one exclusive driver system. Then host plugins that add systems to `FixedUpdate` cannot reorder us.
- **Unit types are content, not code.** The data model stays the same; the content author writes trees, needs and job definitions in RON (§1). Only *new primitives* (an action kind, a condition, a scorer) need Rust.

### 0.4 Verifying portability

1. **A second consumer.** Build `apps/rts_units_demo`, which depends only on `npc_framework` (the facade) and uses no village content. It has 200 units, a `Squad` relationship, `GoTo`/`AttackMove` orders and a host-side mover (straight-line first, Avian adapter later). If it needs a private item, the API has a gap.
2. **Content-leak checks.**
   - `cargo-deny` `[bans]` with `wrappers`/`deny` rules, so `npc_kernel`, `npc_ai`, `npc_needs` and `npc_jobs` cannot depend on `bevy_render`, `bevy` (full), or `village_content`.
   - A fitness test (`cargo metadata` parsed in a test) that fails if a framework crate depends on any `content/*` or `apps/*` crate.
   - `grep`-style tests that fail if `villager` appears in framework crates.
3. **Build matrix.** `cargo hack check --each-feature` on the facade, plus `cargo check -p npc_kernel --no-default-features`, ensures headless builds stay render-free.
4. **API stability.** `cargo-semver-checks` on the facade once versions are tagged, and `#![deny(missing_docs)]` on public crates.

---

## 1. Framework extensibility

**Findings.**
- **Factorio**:
  - Prototypes are built in a *data stage* run in three rounds across all mods: `data.lua`, then `data-updates.lua`, then `data-final-fixes.lua`. This lets one mod modify another's prototypes without depending on load order. Load order itself is dependency depth, then natural name sort, which is deterministic.
  - After the stage, prototypes are frozen for runtime. The game records *which mod changed which prototype*.
  - Saves run versioned JSON and Lua *migrations* once per save. [Factorio data lifecycle](https://lua-api.factorio.com/latest/auxiliary/data-lifecycle.html)
- **RimWorld**:
  - Content is XML `Defs` with `Abstract`/`ParentName` inheritance.
  - Since A17, mods edit other mods' Defs through `PatchOperation`s that address nodes with XPath, instead of overwriting whole Defs. Whole-Def overwrites conflicted when several mods touched the same Def. [RimWorld wiki: PatchOperations](https://rimworldwiki.com/wiki/Modding_Tutorials/PatchOperations)
  - Behavior is code (JobDriver/Toils), referenced from data (ThinkTree, WorkGiver defs). [RimWorld wiki: example job](https://rimworldwiki.com/wiki/Modding_Tutorials/Code_MendingJob)
- **Bevy 0.20**:
  - Custom `AssetLoader`s for RON are the standard route ([custom asset example](https://bevy.org/examples/assets/custom-asset/)). `bevy_common_assets` 0.18 (Bevy ^0.20) removes the boilerplate.
  - Hot reload is the `file_watcher` feature.
  - **BSN** got new syntax and a `Ready` event in 0.20, but the `.bsn` *asset* format and hot reload are listed only as future work ([Bevy 0.20 notes](https://bevy.org/news/bevy-0-20/)). Do not build content on BSN yet. Use RON assets for definitions and BSN or code for presentation scenes.
- **Saves**: `moonshine-save` 0.8 (Bevy ^0.20) uses a model/view split with a `Save` marker. It explicitly offers *no versioning or migration*, and suggests doing that with hooks yourself. [docs.rs/moonshine-save](https://docs.rs/moonshine-save/0.8.0/moonshine_save/)

**Recommendations.**
- **Two-phase content pipeline, modeled on Factorio.**
  1. *Load*: read RON defs (`NeedDef`, `JobDef`, `TreeDef`, `UnitDef`), each keyed by a string `DefName`.
  2. *Patch*: run ordered `DefPatch` passes (base → updates → final-fixes) in a deterministic mod order.
  3. *Freeze*: resolve names to dense ids, validate references, compute a **content hash** over the frozen set, then hand it to the sim.

  Multiplayer peers compare the content hash at connect. Hot reload is allowed only before freeze or in a dev mode that also invalidates replays.
- **Stable keys, not registration order.** `ActionId(u16)` is currently the push index into `AiRegistry`. Inserting a plugin earlier changes every id and breaks recorded replays and saved trees. Serialize *names*. Assign ids by sorted name at freeze time, or store `(name, hash)`.
- **Typed leaf params.** `Leaf { param: i32 }` will not scale. Use a small `ParamValue` enum (`I32`, `DefRef(name)`, `BbKey`, `Duration(ticks)`) validated at freeze time against each action's declared param schema.
- **Open/closed recipe for a new mechanic.** A new mechanic is one new content crate with:
  - components and systems added to `NpcSet::Actions` or `NpcSet::Sense`;
  - actions, conditions and scorers registered by name;
  - `register_sim_component` for hashing and saving;
  - RON defs.

  Zero edits in kernel crates. Enforce it with the content-leak tests in §0.4.
- **Versioned schemas.** Every serialized root (`Replay`, `Save`, `TreeDef` file, journal JSONL header) carries `format_version` *and* the content hash. Keep `upgrade_vN_to_vN+1` functions and golden fixtures of old versions in `tests/fixtures/`. Replays already carry `version: 1`; trees and inspect JSON do not.
- **Feature flags.** Use Cargo features for *compile-time optional subsystems* (`tools`, `remote`, `view_helpers`, `strict_determinism`), the way replicon splits `client` and `server`. Gameplay toggles go in data (a `GameRules` def) and are hashed, so peers cannot disagree.

---

## 2. AI architecture at scale

**Findings.**
- **RimWorld**: priority ThinkTree nodes → JobGivers → a Job run by a JobDriver as Toils; WorkGivers return null when they don't apply. Stack traces show `ThinkNode_Priority.TryIssueJobPackage` called from `Pawn_JobTracker.DetermineNextConstantThinkTreeJob`. [Steam discussion trace](https://steamcommunity.com/workshop/filedetails/discussion/3513718710/600784181506611930), [rubenwardy on colonist AI and reservations](https://blog.rubenwardy.com/2022/07/17/game-ai-for-colonists/). Our BT-chooses / action-runs split already matches this.
- **Oxygen Not Included**: each duplicant has per-errand-type priorities, and each errand has a 1–9 priority. Duplicants take the highest available level first. Emergency/red-alert overrides exist, and community guidance warns that they starve self-care. [ONI wiki: Priority](https://oxygennotincluded.wiki.gg/wiki/Priority)
- **Dwarf Fortress**: *labors* are per-dwarf permissions. Jobs come from designations, workshops and manager work orders; an idle dwarf with the right labor claims a job. *Work details* group labor assignments. [DF wiki: Labor](https://dwarffortresswiki.org/DF2014:Labor), [Job](https://dwarffortresswiki.org/Job)
- **Halo 3 objectives**: designers declare a *tree of prioritized tasks* with capacities and filters. Squads "plinko" down to the best active task, and re-flow when tasks toggle. [Isla, GDC 2008 slides (WPI mirror)](https://web.cs.wpi.edu/~rich/courses/imgd4000-d09/lectures/halo3.pdf), [Game Developer retrospective](https://www.gamedeveloper.com/design/combat-evolved-the-encounter-design-of-halo-3)
- **Company of Heroes**: players order *squads only*; the squad AI (a core plus two flanks with leaders) makes members look smart. Hierarchical RTS research uses one abstraction per level: strategy → squad → unit. [AAAI hierarchical RTS search](https://cdn.aaai.org/ojs/12714/12714-52-16231-1-2-20201228.pdf)
- **Planners**:
  - F.E.A.R.'s GOAP uses three FSM states (Goto/Animate/UseSmartObject) plus A* over actions. [Orkin GDC 2006](https://pages.cs.wisc.edu/~dyer/cs540/handouts/gdc2006_orkin_jeff_fear.pdf)
  - HTN planning (total-order forward decomposition) is the more controllable successor for designer-authored behavior. [Fluid HTN, after Humphreys / Game AI Pro](https://awesome.ecosyste.ms/projects/github.com%2Fptrefall%2Ffluid-hierarchical-task-network)
- **AI LOD and time-slicing**: give per-frame budgets by LOD, use tiered update rates, and use simplified or frozen behavior far away. [Kistler et al.](https://opus.bibliothek.uni-augsburg.de/opus4/files/46021/46021.pdf)
  - **Lockstep caveat (ours):** LOD keyed on *camera* differs between peers and will desync. Key it on sim-state, for example "any hostile within N cells", a squad flag, or SimId-bucket staggering.
- **Bevy 0.20 ECS**:
  - `Relationship`/`RelationshipTarget` give one-to-many links, maintained by hooks. [docs.rs relationship](https://docs.rs/bevy_ecs/0.20.0/bevy_ecs/relationship/index.html)
  - Hooks `on_add`/`on_insert`/`on_discard`/`on_remove`/`on_despawn`, `#[require]`, `#[component(immutable)]`, and Table vs SparseSet storage. [docs.rs Component](https://docs.rs/bevy_ecs/0.20.0/bevy_ecs/component/trait.Component.html)
  - New in 0.20 ([Bevy 0.20 notes](https://bevy.org/news/bevy-0-20/)):
    - `#[component(summary_tick)]` skips unchanged columns.
    - `despawn_all` does batched despawns.
    - `chain_weak` exists. Do not use it in the sim, because it drops ordering between non-conflicting systems and ignores global-state dependencies.
    - `ScheduleBuildSettings::shuffle_seed` (`debug` feature) randomizes system order within the constraints, for property tests.

**Recommendations (the layered "choose → run" architecture):**

1. **Unit layer** (exists): BT selects; actions as SparseSet components run. Keep the event-driven think pass. Add `UtilitySelector` use-cases (job choice) with integer curves.
2. **Job layer** (P1). A `JobBoard` resource of job *offers*, posted by smart objects, designations and orders:
   - each offer has a `JobDef`, a priority from 1 to 9, and required labor tags;
   - each agent has a `WorkPriorities` table, the ONI/DF model;
   - matching is done in the Think phase, sorted by (priority, distance bucket, SimId);
   - `Reservations` gains *layers* and *capacity*, as RimWorld's does, and a claim TTL with an owner job id, so a leaked claim is detectable;
   - a BT leaf `TakeJob` binds the job to the blackboard.
3. **Squad layer** (P1). Make `Squad` an entity with its own `Brain`. Membership is a `Relationship` (`MemberOf(squad)` ↔ `Members`). The squad writes `Order`s to members; members' trees keep an `Order` branch, as today. Formation slots are computed by the squad in integer space.
4. **Commander/objective layer** (P2). Halo-style objective trees: prioritized tasks with capacity and filters. Squads flow to tasks; the player's orders and the AI commander both write the same `Order` commands, which matters for lockstep.
5. **Perception** (P1): a `Sense` set that writes blackboard facts at a staggered rate (`(SimId + tick) % N`), reading an integer spatial grid.
6. **Archetype churn.** Keep action components SparseSet. Use marker components only for low-frequency state. Prefer `Changed<T>` and `summary_tick` on rarely changing big components (`Needs` decays every tick, so it should not get it). Avoid observers in the sim hot path. Observers run on trigger order, which is deterministic only if triggers are issued in SimId order. Hooks are fine for invariant maintenance (index maps, reservation release on despawn).
7. **Planners**: no GOAP or HTN now. If needed, put an HTN *inside a leaf* for squad tactics later; it is easier to debug than GOAP.

---

## 3. World and movement

**Findings.**
- **Factorio biters**: pathfinding is hierarchical. Chunks are contracted into connected *components*, a coarse path is found over them, and that path guides the fine A* ([FFF #317](https://factorio.com/blog/post/fff-317)). Cheap cases skip pathfinding: walk straight with avoidance, and path only on failure ([FFF #117](https://factorio.com/blog/post/fff-117)).
- **Flow-field tiles** (Emerson, *Game AI Pro 360: Movement*): sector cost fields, portals and integration/flow fields per goal sector move hundreds to thousands of agents and support formations without per-agent repaths ([T&F abstract](https://www.taylorfrancis.com/books/9780429100277/chapters/10.1201/b16725-29)). Claims about how Supreme Commander 2 does this are contradictory in public sources, so treat them as unverified.
- **Crates**, with versions and Bevy requirements from crates.io:
  - **Navmesh**: `vleue_navigator` 0.16 / `polyanya` 0.17 (Bevy ^0.19.1, glam f32); `bevy_landmass` 0.12 (Bevy ^0.19); `bevy_rerecast` 0.5 (Bevy ^0.19). `oxidized_navigation` 0.12 has not been updated since 2024-12 and requires Bevy ^0.15.
  - **Grid**: `bevy_flowfield_tiles_plugin` 0.16 (Bevy ^0.20, published 2026-10-22); `bevy_northstar` 0.7 (HPA*, Bevy ^0.19). `pathfinding` 4.16 is engine-agnostic, has generic `Ord` costs, and depends on integer-sqrt.

**Recommendations.**
- **For lockstep: our own integer grid.** Build a cost grid in i32, a coarse component/portal graph (Factorio-style), A* from the `pathfinding` crate with integer costs and SimId tie-breaks, and integer flow fields for squad moves sharing one goal. f32 navmesh crates are fine *behind the `MoveIntent` adapter* for non-lockstep hosts. Every one of them is glam/f32 and none promises cross-platform determinism.
- **Spatial index**: an integer uniform grid (bucket = cell, `Vec<SimId>` sorted), rebuilt or updated in a `Spatial` set before `Sense`. `bevy_spatial` is on Bevy ^0.16; avoid it.
- **Local avoidance** for crowds (P2): an integer separation steering pass, deterministic in SimId order. RVO libraries (`dodgy_2d` via landmass) are f32.
- **Path requests are budgeted.** Use a `PathRequests` queue serviced up to N per tick in (priority, SimId) order. The budget is a *count*, never wall-clock time. A time budget would make results depend on machine speed and desync peers.

---

## 4. Determinism and multiplayer

**Findings.**
- **Age of Empires**: sending unit state would cap the game at about 250 moving units on a 28.8k modem, so AoE runs deterministic lockstep and sends commands only. [1500 Archers](https://www.gamedeveloper.com/programming/1500-archers-on-a-28-8-network-programming-in-age-of-empires-and-beyond)
- **Factorio**: lockstep with a fast, incomplete heuristic CRC every tick; desync reports compare full server and client state dumps. Game speed is capped by the slowest peer, and latency is hidden with an input buffer. [FFF #55](https://factorio.com/blog/post/fff-55), [#76](https://factorio.com/blog/post/fff-76), [#188](https://factorio.com/blog/post/fff-188)
- **lightyear 0.31** (Bevy ^0.20, released 2026-10-21): server-authoritative replication (built on replicon), prediction and rollback, interpolation, and an *input-only deterministic* mode usable with lockstep. Input-only mode requires a fixed peer count; late join uses a snapshot. [repo](https://github.com/cBournhonesque/lightyear), [releases](https://github.com/cBournhonesque/lightyear/releases)
- **bevy_replicon 0.45.2** (Bevy ^0.20): server-authoritative state replication.
- **bevy_ggrs 0.22**: requires Bevy ^0.19. Rollback re-simulates and snapshots the whole state, so it suits fighting games, not colony or RTS state.

**Recommendations.**
- **Opposing armies (RTS)**: deterministic lockstep with commands only. That is our current design. Use lightyear's input-only mode or a thin custom transport.
- **Shared-colony co-op**: lockstep works too, since everything is already deterministic. Server-authoritative with replicon is a fallback for drop-in/drop-out and large late-join worlds. Keep both open by making `SimCommand` the *only* input; this is already true.
- **Desync detection**:
  - Keep the full FNV hash on checkpoints.
  - Add a cheap per-tick *heuristic* hash (clock, entity count, sum of positions), as Factorio does.
  - Add *per-subsystem* hashes (needs, positions, AI, reservations), so the first diverging subsystem is named.
  - Add an `inspect` JSON dump on mismatch, so two dumps can be diffed. The CLI already has diverge search.
- **Fixed-point**: integer mm and milli-units are enough for now. Introduce `fixed` 1.31 (`I32F32`) only if fractional math such as trig or steering needs it, behind kernel `SimScalar` aliases. Forbid floats in sim crates with lints (§5).
- **Sim isolation**: keep a custom schedule with an exclusive driver for now. Bevy 0.20 made extraction generic over worlds (`#[extract_app(...)]`, new `bevy_extract` crate; [migration guide](https://bevy.org/learn/migration-guides/0-19-to-0-20/)), which makes a dedicated sim `SubApp` and World viable later (P2). It would guarantee that no presentation component can ever land on a sim entity, and allow snapshotting the sim World.

---

## 5. Engineering workflow

**Testing pyramid for a deterministic sim:**

| Layer | Tooling | What we test |
|---|---|---|
| Unit | `cargo nextest` 0.9 | BT node semantics, reservations, needs hysteresis |
| Scenario | headless `App` + `run_ticks` (exists) | "hungry + empty pile → works then eats" |
| Property | `proptest` 1.12 | random command streams: invariants (no double-claim, `reserved ≤ food`, no leaked claims, hash equal across two runs); **`shuffle_seed`** schedule shuffling must not change the hash |
| Golden/snapshot | `insta` 1.49 | journal excerpts and `inspect` JSON of fixed scenarios; old replay/save fixtures still load |
| Soak | ignored tests, nightly CI | 100 simulated days × N seeds; starvation, flip-flop and claim-leak counters |
| Benchmark regression | `criterion` 0.8 locally; `gungraun` 0.20 (formerly iai-callgrind, Linux/Valgrind) in CI for instruction counts | think pass at 1/100/1000 agents; budgets in `npc_perf` |
| Mutation (occasional) | `cargo-mutants` 27 | strength of the kernel tests |

**Architecture fitness and CI:**
- **`clippy.toml` per sim crate**, all lints verified to exist in local clippy:
  - `disallowed_types` = `std::collections::HashMap`, `HashSet`, `f32`, `f64`, `std::time::Instant`, `bevy::time::Time`;
  - `disallowed_methods` = `rand::thread_rng`, `f32::sin` and friends;
  - plus `#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type, clippy::float_cmp)]` in `npc_kernel`, `npc_ai`, `npc_needs`, `npc_jobs` and `npc_locomotion`'s deterministic mover.
  - This would already flag the `f64` grid-layout math in `scenario.rs`.
- **Clippy pedantic subset**: `cast_possible_truncation`, `cast_sign_loss`, `cast_precision_loss` and `as_conversions` in sim crates, because integer overflow and truncation is the new float risk.
- **`cargo-deny` 0.20**: licenses (CC0 assets aside), advisories, and `[bans]` encoding the crate boundaries from §0.4.
- **CI**: GitHub Actions on Windows and Linux (and later macOS aarch64), running fmt, clippy `-D warnings`, `nextest`, `cargo hack --each-feature`, `cargo-deny`, and a **cross-platform determinism job**. The job runs the same replay on each OS and compares checkpoint hashes; it is the only real proof of lockstep readiness.
- **Process**:
  - Keep the spec → plan → implement → verify loop in `docs/superpowers/`.
  - Add **ADRs** (`docs/adr/NNNN-title.md`, the Nygard format: context, decision, consequences) for irreversible calls: integer sim, SimId ordering, BT-vs-utility, lockstep-first, crate split.
  - The specs' "Calls made by the designer" sections are ADR candidates.
- **Semver for internal APIs**:
  - Version the facade (`npc_framework 0.x`); minor bumps may break, patch bumps may not.
  - Run `cargo-semver-checks` 0.51 in CI.
  - Keep a `CHANGELOG.md` and a Bevy↔framework version table.
  - Publish MSRV (already 1.97.1).

---

## 6. Gap analysis against current code

**Already strong (keep):**
- Integer sim state.
- `SimId` ordering with a sorted think pass.
- Counter-based RNG.
- Ambiguity detection = Error on `SimTick`.
- Sim and presentation split (`VisualOf`, never writing sim).
- A single command layer, with replays that include version and checkpoints.
- Journal and diverge tooling.
- Event-driven think with `ThinkRequest`.
- Shared `TreeDef` with a content hash.
- SparseSet action components.
- `init_resource` (does not clobber host values).
- A headless test suite, including determinism.
- `npc_perf` is already generic.

**Missing or risky:**

| # | Gap | Risk |
|---|---|---|
| G1 | `npc_sim` mixes kernel and villager content (`commands.rs` → `villager_bundle`, `hash.rs` lists `Eat`/`Work`…, closed `NeedKind`/`PlaceKind`) | Not portable; every new mechanic edits the core |
| G2 | `SimDriverPlugin` overwrites the global `Time<Fixed>` to 20 Hz and binds Space/1-4 | Breaks host physics and networking timing; key conflicts |
| G3 | `ActionId`/`CondId`/`TreeId` are registration-order indices; `Leaf.param: i32` | Replays, saves and content files break when plugin order changes; params can't express content |
| G4 | No MoveIntent seam: actions move `SimPos` directly | Host can't plug in pathfinding or physics |
| G5 | No RON/asset content path; no content hash over all defs | Unit types are code; peers can't verify content |
| G6 | No CI, no `clippy.toml`/`deny.toml`, no float or HashMap lints (`scenario.rs` uses `f64::sqrt`) | Determinism and boundary regressions go unnoticed |
| G7 | Single global hash; no per-subsystem or heuristic hash | Slow desync triage at scale |
| G8 | Reservations have no layers, TTL or job owner; no job board | Group iteration (many agents, many jobs) will leak or contend |
| G9 | No spatial index; `find_*` probably scan all places | O(N·M) at 500+ agents |
| G10 | `npc_view` hard-codes villager types (51 references) | View helpers not reusable |
| G11 | No ADRs; no semver or changelog; no second consumer app | Portability unproven |

---

## 7. Prioritized roadmap

### P0: baseline hardening (before groups)

| Action | Effort | Rationale |
|---|---|---|
| **Split crates**: extract `npc_kernel` (ids, clock, rng, math, commands bus, journal, replay, hash registry, schedule) from `npc_sim`; move villager code into `content/village_content`; add the `npc_framework` facade with `NpcPlugins` and a `prelude` | L | G1; the foundation for everything else, and cheapest while the code is about 8k LOC |
| **Driver rework**: `NpcPlugins::new().tick_hz(n).run_in(label)` with its own accumulator; speed and pause input moves to `apps/village`; `TICKS_PER_SECOND` becomes a resource | S | G2; the main blocker for embedding in a host App |
| **Registration API with stable names**: `register_action/cond/scorer/need/sim_component` keyed by string; ids frozen sorted by name; replays and trees store names; typed `ParamValue` | M | G3; open/closed extension and durable replays |
| **Generic command extension**: `SimCommand` core variants plus `Content(Box<dyn ...>)`, or `SimCommand<C>`; move `SpawnVillager` to content | M | G1; RTS-specific orders without editing the kernel |
| **State-hash registry** (`SimHash` trait, registered components hashed in SimId order) plus per-subsystem hashes | M | G1/G7 |
| **Determinism and boundary lints**: `clippy.toml` `disallowed_types/methods`, `float_arithmetic`, `iter_over_hash_type`; `deny.toml` bans; a metadata fitness test | S | G6; cheap and permanent |
| **CI**: fmt, clippy, nextest, cargo-deny, `cargo hack`, Windows and Linux determinism replay job | M | G6 |
| **Proptest invariants plus a `shuffle_seed` determinism test; insta goldens for journal and inspect** | M | Catch ordering bugs before groups multiply them |
| **ADRs 0001–0006** for decisions already made; CHANGELOG; version table | S | G11 |

### P1: enablers for the next iteration (groups)

| Action | Effort | Rationale |
|---|---|---|
| **`npc_locomotion` seam**: `MoveIntent`/`MoveStatus` plus the built-in deterministic mover; the existing `MoveToDest` uses it | M | G4; host-owned movement |
| **`apps/rts_units_demo`** using only the facade: 200 units, squads, orders | M | Proves portability (§0.4) |
| **Data-driven needs and trees**: `NeedDef`/`TreeDef` RON via `bevy_common_assets`; load → patch → freeze; content hash checked at load and in replays | M | G5; unit types become content |
| **Job board plus layered reservations** (capacity, layer, owner job, TTL, leak detector) and `WorkPriorities` | L | G8; colony-sim group work, as in RimWorld, ONI and DF |
| **Squad entity**: `MemberOf`/`Members` relationship, squad `Brain`, orders to members, integer formation slots | M | Groups for the RTS |
| **Integer spatial grid plus a staggered `Sense` set** | M | G9; perception at scale |
| **Integer grid A\*** (`pathfinding` crate), budgeted `PathRequests` | M | First real obstacles; deterministic |
| **Bench suite**: think, needs and path at 1/100/1000 agents; `gungraun` in CI | S | Guards the 500+ NPC goal |
| **Generic view helpers** (`npc_view_helpers`): interpolation, `VisualOf`, gizmo overlay driven by registered inspectors | M | G10 |

### P2: later

| Action | Effort | Rationale |
|---|---|---|
| Hierarchical paths (chunk components, Factorio-style) plus integer flow fields for squad moves | L | RTS crowds |
| Objective trees (Halo 3 style) for an AI commander | L | Opponent AI and encounter design |
| Sim-state-keyed AI LOD and time-slicing (never camera-keyed in lockstep) | M | Scale to thousands of units |
| Multiplayer spike: lightyear input-only lockstep for armies; replicon server-authoritative for co-op drop-in | L | The two target multiplayer shapes |
| Sim as a `SubApp`/separate World via `bevy_extract` | M | Hard isolation and snapshotting |
| Save/load: moonshine-save-style marker plus *our own* versioned migrations (Factorio-style, run once per save) | M | Saves with schema evolution |
| DefPatch ops (RimWorld-style targeted patches on RON defs) for mods | M | Modding |
| `cargo-semver-checks` gate once 0.x APIs settle; publish to crates.io if wanted | S | External consumers |

---

## 8. Crates table (verified against crates.io 2026-10-23)

| Crate | Version | Requires Bevy | Verdict | Note |
|---|---|---|---|---|
| bevy_common_assets | 0.18.0 | ^0.20 | **Adopt** (P1) | RON def loading |
| ron | 0.12.2 | n/a | **Adopt** | content format |
| pathfinding | 4.16.0 | none | **Adopt** (P1) | generic A* with integer costs |
| fixed | 1.31.0 | none | Evaluate | only if fractional fixed-point is needed |
| bevy_replicon | 0.45.2 | ^0.20 | Evaluate (P2) | server-authoritative co-op |
| lightyear | 0.31.0 | ^0.20 (bevy_app/ecs) | Evaluate (P2) | input-only deterministic mode; released 2 days ago |
| bevy_ggrs / ggrs | 0.22.0 / 0.13.0 | ^0.19 | Avoid | rollback does not fit RTS/colony state; no Bevy 0.20 release |
| moonshine-save | 0.8.0 | ^0.20 | Evaluate (P2) | no versioning; add our own |
| bevy_asset_loader | 0.28.1 | ^0.20 | Evaluate | view/app loading states |
| avian3d | 0.8.0 | ^0.20 | Host adapter only | never inside the deterministic sim |
| bevy-tnua | 0.33.0 | ^0.20 | Pattern reference | integration-layer design |
| bevy_flowfield_tiles_plugin | 0.16.0 | ^0.20 | Evaluate as reference | float use and determinism not documented; likely write our own integer version |
| vleue_navigator / polyanya | 0.16.0 / 0.17.1 | ^0.19.1 / none (glam f32) | Evaluate (non-lockstep hosts) | no Bevy 0.20 release yet |
| bevy_landmass / landmass | 0.12.1 / 0.9.3 | ^0.19 / none | Evaluate (adapter) | f32 crowds with avoidance |
| bevy_rerecast | 0.5.0 | ^0.19 | Evaluate later | navmesh generation |
| bevy_northstar | 0.7.0 | ^0.19 | Evaluate | HPA* grid |
| oxidized_navigation | 0.12.0 | ^0.15 | Avoid | stale since 2024-12 |
| bevy_spatial | 0.11.0 | ^0.16 | Avoid | write an integer grid instead |
| bevior_tree | 0.12.0 | ^0.20 | Reference only | already analysed |
| bevy_behave | 0.5.0 | ^0.18 | Avoid | behind |
| big-brain | 0.22.0 | ^0.15 | Avoid | archived |
| leafwing-input-manager / bevy_enhanced_input | 0.22.0 / 0.27.0 | ^0.20 | Apps only | never in framework crates |
| cargo-nextest, cargo-deny, cargo-hack, cargo-semver-checks, cargo-mutants | 0.9.149, 0.20.2, 0.6.45, 0.51.0, 27.1.0 | n/a | **Adopt** (CI) | |
| insta, proptest, criterion, gungraun | 1.49.0, 1.12.0, 0.8.2, 0.20.0 | n/a | **Adopt** (tests) | gungraun is Linux/Valgrind only |

### Additional sources
- Bevy 0.20 release notes: https://bevy.org/news/bevy-0-20/ · migration guide 0.19→0.20: https://bevy.org/learn/migration-guides/0-19-to-0-20/
- crates.io API (versions and dependencies): https://crates.io/api/v1/crates/{name}/{version}/dependencies
- Factorio FFF #55/#76/#188/#117/#317: https://factorio.com/blog/post/fff-55 (and -76, -188, -117, -317)
- Nygard, "Documenting Architecture Decisions": https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions
