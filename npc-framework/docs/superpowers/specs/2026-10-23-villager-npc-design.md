# Villager NPC — iteration 1 design

Date: 2026-10-23 · Status: draft for approval
Research backing this design: `docs/research/bevy-0.20-cheatsheet.md`, `docs/research/assets.md`, `docs/research/ai-prior-art.md`.

## 1. Intent

We are building a reusable 3D NPC/actor framework in Rust on **Bevy 0.20.0**. It is meant for system-heavy games:

- colony sims (RimWorld-like needs and jobs)
- Blitzkrieg-like RTS (many units, squads, orders)
- games whose behavior emerges from systems interacting

Long term it must support multiplayer in two shapes: several players sharing one colony, and opposing players commanding armies.

Growth path (user's order): **single NPC → group of NPCs → world interactions**.

This document covers **iteration 1: one villager living his life on one world tile.** He has needs (hunger, energy), and a behavior tree that gives him visible reasons for what he does: he works to earn food, eats when hungry, sleeps when tired or at night, and wanders when nothing is pressing. Iteration 1 must be built the way 500 NPCs and a lockstep multiplayer game will need it later, even though it shows only one.

## 2. Goals

1. One animated villager on a textured world tile with a house, a bed, a food stockpile, and a work site (grain field), all free CC0 assets.
2. **Needs**: hunger and energy, decaying over time, with hysteresis thresholds.
3. **Behavior tree** of our own, ECS-native. Shared tree definition plus per-agent memory. Multi-tick actions with Running/Success/Failure. Reactive interrupts (an urgent need aborts lower-priority work).
4. **A small economy loop**: working the field adds food to the stockpile; eating takes food from it. When the stockpile is full he has free time; when it is empty he must work before he can eat.
5. **Day/night cycle** that visibly changes lighting and makes sleep more attractive at night.
6. **Deterministic, headless-capable simulation** separate from presentation. The same seed and the same inputs give a bit-identical state hash.
7. **Debug visibility**: on-screen panel and gizmos showing needs, current tree path/action, target, reservations, stockpile, time of day.
8. Test suite proving tree semantics, villager scenarios, and determinism, all runnable without a window.

## 3. Non-goals (iteration 1)

- Multiple NPCs and groups (but nothing may assume there is only one; see anti-goals).
- Pathfinding and obstacle avoidance. He walks straight lines on open ground. Props are placed so straight paths are fine, and approach points sit in front of props.
- Player commands and selection (the tree keeps a hook slot for orders, but no input UI).
- Networking, save/load, RON/asset-loaded trees (the IR must be serializable, but only the Rust builder exists).
- Multiple agent types and factions.
- Physics, terrain height, multiple tiles.

## 4. Anti-goals (fail even if it "works")

- Sim code that assumes one NPC: `Single<..>` queries in sim systems, global "the villager" resources.
- Sim decisions that read `Time`, frame delta, floats with transcendentals, `HashMap` iteration order, or `Entity` ordering.
- The sim depending on rendering, or presentation components inserted on sim entities.
- A villager that flip-flops between branches every few ticks, or gets stuck forever (deadlock / starvation with food available).
- A tree that can only be understood by reading code. The debug view must show why he is doing what he does.

## 5. Constraints

- **Bevy 0.20.0, Rust edition 2024, MSRV 1.97.1.** The local toolchain is 1.96.1, so the builder runs `rustup update stable` first (approved as part of this design).
- **Location**: a new Cargo workspace at `npc-framework/` in the `the-factory` repo, on branch `claude/bevy-3d-npc-framework-705347`. The repo root is an Obsidian QMS vault, so everything stays inside `npc-framework/` and the folder gets its own `.gitignore` (`target/`).
- Windows 11 dev machine. Use the dev profile opt-levels from the cheat-sheet; `dynamic_linking` is optional for local iteration only.
- Assets: CC0 only, downloaded by a checked-in script, **committed** to the repo under `npc-framework/assets/` with a `CREDITS.md` listing source and license. (Builder's call to commit only the files actually used, to keep the repo small.)

## 6. Architecture (decided)

### 6.1 Workspace layout

```
npc-framework/
  Cargo.toml                  # workspace
  crates/
    npc_ai/                   # generic BT IR + runtime. Depends on bevy_ecs/bevy_app only, no game rules.
    npc_sim/                  # deterministic simulation: ids, clock, rng, fixed-point movement,
                              # needs, smart objects, reservations, actions, villager tree. Headless.
    npc_view/                 # presentation: models, animation, interpolation, camera, lighting, debug UI.
  apps/
    village/                  # binary: wires DefaultPlugins + npc_sim + npc_view, spawns the scenario.
  assets/                     # CC0 models/textures + CREDITS.md
  scripts/fetch_assets.ps1    # (or .sh) reproducible asset download
  docs/
```

Dependency direction: `npc_view → npc_sim → npc_ai`. `npc_sim` and `npc_ai` must build and test without rendering features. Use `bevy` with `default-features = false` and only the ECS/app pieces, or the `bevy_ecs`/`bevy_app` sub-crates directly.

### 6.2 Simulation core (`npc_sim`)

- **Tick schedule**: a custom `SimTick` schedule at **20 ticks/s**. The client runs it from `FixedUpdate` (`Time::<Fixed>::from_hz(20.0)`). Sim systems never read `Time`; they read `SimClock { tick: u64 }`. Tests run `world.run_schedule(SimTick)` directly. Ambiguity detection on `SimTick` = Error.
- **Speed control**: 0× (pause) / 1× / 3× / 10×, implemented by changing virtual time speed, or by running SimTick k times per fixed step. It must never change what one tick does.
- **Identity**: `SimId(u32)` from a sim-owned counter, plus a `SimId → Entity` index resource. Anything order-sensitive iterates sorted by `SimId`.
- **Numbers**: positions are `SimPos { x: i32, z: i32 }` in **millimetres** (a 32 m tile → ±16 000). Speeds are mm/tick. Normalisation uses integer `isqrt` on i64. Needs are `i32` in milli-units (0..=100 000). No floats in sim state.
- **RNG**: counter-based, `hash(world_seed, sim_id, tick, purpose) → u64` (SplitMix64). No stateful global RNG.
- **Day/night**: `day_length = 2 400 ticks` (2 min real time at 1×). The clock exposes `time_of_day` and `is_night` (night = last 30 % of the day). Builder may tune.
- **State hash**: a system or function computes a 64-bit hash over all sim components in `SimId` order. It is used by the determinism tests and shown in the debug panel.

### 6.3 Behavior tree runtime (`npc_ai`)

- **Flat IR**: `TreeDef { nodes: Vec<NodeDef> }` in pre-order with child index ranges, stored in a `TreeRegistry` resource and referenced by `TreeId`. It is serializable (serde) and has a content hash.
- **Node kinds for iteration 1**:
  - `Selector`, `Sequence`
  - `Guard { cond, abort: None | LowerPriority }`
  - `Leaf { action, params }`
  - `Wait`, implementable as a leaf
  - `UtilitySelector` (integer scores, ties → lower index): implement it, but iteration 1's tree does not have to use it.
- **Registries**: `ActionRegistry` (`ActionId` → start/abort functions) and `CondRegistry` (`CondId` → predicate). Leaves reference ids plus typed params. `npc_ai` knows nothing about villagers.
- **Per-agent state**:
  - `Brain { tree: TreeId }`
  - `BrainMem` (flat small vec, one word per node)
  - `Blackboard` (small fixed slots, e.g. target `SimId`, target position)
  - `ActiveLeaf { node, status, started_tick }`
- **Actions are components**: starting a leaf inserts a `SparseSet` action component (`MoveTo`, `Work`, `Eat`, `Sleep`, `Idle`). One ordinary system per action kind advances it and writes the status. The tree only waits on the status.
- **Think pass**:
  - It runs single-threaded, sorted by `SimId`, only for agents that are dirty: a need crossed a threshold, the active leaf finished, or a re-think timer fired. There is no per-tick full re-evaluation.
  - Reactive guards above the running path are re-checked on dirty agents. A passing higher-priority guard with `abort = LowerPriority` aborts the running branch. Abort is synchronous: it removes the action component and releases reservations.
- **Status contract**: `start` may return Success, Failure, or Running immediately. After that, only the action's own system changes the status.

### 6.4 Tick order (inside `SimTick`, chained)

1. **ApplyInputs**: empty in iteration 1; reserved for orders and network commands.
2. **Needs**: decay, threshold crossing with hysteresis → mark dirty.
3. **Sense**: refresh blackboard facts, such as nearest available food and whether a work site is free.
4. **Think**: aborts, starts, reservation claims.
5. **Actions**: one system per action component.
6. **ResolveEffects**: stockpile add/remove and need restore, applied in sorted order.
7. **Cleanup**: release finished reservations, advance `SimClock`.

### 6.5 World: smart objects and reservations

- Places are sim entities with a `SimId`, a `SimPos`, and an `approach` point:
  - `Home` (with a `Bed`)
  - `Stockpile { food: u32, capacity: u32 }`
  - `WorkSite { kind: Field, work_ticks, yield_food }`
- Objects advertise what they provide (`Provides { need, amount_per_tick }` for the bed and the stockpile). The tree and actions query these advertisements instead of hard-coding entity references. This is The Sims' smart-object idea in its smallest form.
- `Reservations` resource: `BTreeMap<(target SimId, slot), claimant SimId>`. Work sites, beds, and food units are reserved on start and released on finish or abort. It is trivial with one villager, but it is the mechanism groups will need.

### 6.6 Villager behavior (required behaviors)

| # | Branch | Trigger | Does | Interrupts lower branches? |
|---|---|---|---|---|
| 1 | Eat (urgent) | hunger urgent and food in stockpile | reserve food → walk to stockpile → eat N ticks → hunger restored, food −1 | yes |
| 2 | Sleep (urgent) | energy urgent | walk home → sleep until energy full | yes |
| 3 | Sleep (night) | night | walk home → sleep until morning (and at least until rested) | no: a running work shift or meal finishes first, then he goes to bed |
| 4 | Eat (casual) | hunger below "peckish" and food available | as 1 | no |
| 5 | Work | **day**, stockpile below capacity, work site free | reserve field → walk → work N ticks → stockpile +yield | no (finishes unless 1/2 fire) |
| 6 | Wander / idle | otherwise | pick a random point near home (counter RNG) → walk → idle a random number of ticks | — |

Consequences:
- Hunger urgent with an **empty** stockpile: branch 1 fails, so (in the day) he goes to work (5), which refills the stockpile, and he then eats. This is the "earning his food" loop. *Exception at night: when hunger is urgent and the stockpile is empty, a "forage" variant of Work (same as 5, but without the day condition) sits between branches 2 and 3, so he never starves in the dark.*
- He has a day rhythm: wake, eat if peckish, work shifts until the stockpile is full, free time (wander/idle), and bed at nightfall.
- **Nightfall and morning are re-think triggers**: the clock marks all agents dirty at day/night transitions. Because branch 3 does not abort, a work shift in progress completes before he walks home.
- Thresholds use hysteresis, e.g. urgent enter < 20 %, exit > 60 %. Exact numbers and decay rates are the builder's to tune so that a 2-minute day shows roughly 1–2 work shifts, 1–2 meals, and one night's sleep.
- An "order" slot between branches 2 and 3 is **not** built; leave a comment marking where RTS orders will slot in.

### 6.7 Presentation (`npc_view`)

- **Separate visual entities**: each sim agent or object gets a visual entity linked by `VisualOf(Entity)`. The visual reads sim components and never writes them.
- **Smooth movement**: transform interpolation between previous and current `SimPos` using `Time<Fixed>::overstep_fraction()`. Yaw faces the movement direction, smoothed.
- **Animation state** is derived from the active action component, with cross-fades of about 0.2 s:
  - walk = `Walking_A`
  - work = `1H_Melee_Attack_Chop` or `Interact`, looped
  - eat = `Use_Item`
  - sleep = `Lie_Down` → `Lie_Idle` → `Lie_StandUp`
  - idle = `Idle`
  - The builder verifies the clip names by dumping the GLB and adjusts.
- **Assets** (see `docs/research/assets.md`):
  - KayKit Adventurers `Rogue_Hooded.glb` for the villager
  - KayKit Medieval Hexagon house, grain field, sack/barrel/crate stockpile, trees, rocks, fences
  - KayKit Furniture Bits bed (placed beside/inside the house footprint; it may sit outside if the house is closed)
  - Ground: flat 32×32 m plane with a tinted ambientCG grass texture, or a solid colour if the texture clashes (builder's call)
  - Visible stockpile fill (e.g. number of sacks shown = food count) is nice-to-have.
- **Camera**: RTS-style. WASD/arrow keys pan, mouse wheel zooms, Q/E rotate, starting at about 45° looking at the tile.
- **Day/night**: the directional light's angle, colour and intensity follow `time_of_day`; ambient light dims at night but the scene stays readable.
- **Debug overlay** (toggle F1):
  - Text panel: tick, day and time, speed, state hash, stockpile food, villager hunger/energy as bars or percentages, active tree path (e.g. `Root > Work > MoveTo`), active action, reservation list.
  - Gizmos: line to the current target, circles at reservations and approach points.
- **Controls**: Space = pause, 1/2/3/4 = speed 1×/3×/10×/0×, F1 = debug overlay.

## 7. Testing and verification

- `npc_ai` unit tests: selector/sequence semantics, guard abort behaviour, Running → Success/Failure flows, abort releasing state, IR content hash stability.
- `npc_sim` scenario tests (headless, `MinimalPlugins` or bare `World` + `SimTick`):
  - With an empty stockpile and urgent hunger, he works and then eats.
  - Urgent hunger interrupts wander and work.
  - He sleeps at night and wakes in the morning.
  - A full stockpile leads to wander/idle.
  - Reservations are released on abort.
  - He never starves over 10 simulated days.
  - No branch flip-flop: the number of branch switches per day stays below a bound.
- **Determinism test**: two independent worlds with the same seed run for 20 000 ticks and their state hashes are equal; a different seed gives a different hash.
- `cargo clippy --workspace -- -D warnings` and `cargo test --workspace` pass.
- **Visual check**: the app supports `--screenshot <path> --after-secs N` (Bevy screenshot API, then exit), so an agent can capture and inspect frames without a human.

## 8. Success criteria

`cargo run -p village` opens a window showing the villager on the tile, visibly cycling through work, eat, sleep (at night), and wander, with appropriate animations, smooth movement, and day/night lighting. The F1 overlay explains each decision. All tests pass, including the determinism test.

## 9. Calls made by the designer (not discussed with the user, so flag if wrong)

- **Night outranks work**: he does not start work shifts at night, except when hunger is urgent and the stockpile is empty. A shift already in progress at nightfall finishes. Casual eating ranks above work, so he eats breakfast before going to the field.
- Needs clamp at 0. "Never starves" means hunger never reaches 0 in the 10-day test.
- Interrupted work loses its shift progress; no partial yield.
- Food reservations are count-based: a claimant reserves 1 unit, and `reserved ≤ food` holds. Work sites and beds allow 1 claimant each.
- State hash uses a fixed, platform-independent algorithm (e.g. FNV-1a or SplitMix64 over little-endian bytes), never `std`'s `DefaultHasher`.
- Assets come from the KayKit packs (CC0) and are committed.
- The camera is RTS-style; the debug overlay is on F1.

## 10. Left to the builder

- Exact crate-internal module structure, type names beyond those named here, tuning numbers, prop placement and scale.
- Whether to use the `bevy_ecs`/`bevy_app` sub-crates or `bevy` with features off for the sim crates.
- Asset download script language. Asset subset and texture choice (within CC0 and the KayKit style).
- Whether interpolation is hand-rolled or uses `bevy_transform_interpolation`. Hand-rolled is preferred because positions live in sim components, not `Transform`.
