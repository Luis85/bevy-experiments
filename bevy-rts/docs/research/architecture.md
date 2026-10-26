# Extensible WWII isometric RTS on Bevy 0.20: architecture research

Research was done on 2026-10-23. Bevy 0.20.0 was published to crates.io on 2026-10-08. I checked every crate version below against the crates.io API: the "bevy req" column is the `bevy` / `bevy_*` requirement declared by that crate's latest published release. I created no files. The only thing written was a throwaway query script in the scratchpad.

## TL;DR recommendation

1. **Separate the simulation from presentation from day one.** The simulation is a deterministic, fixed-tick, fixed-point Bevy `App` that uses only `bevy_app` and `bevy_ecs`. It runs standalone in tests, the CLI and a future server. Inside the client it runs as a `SubApp` with its own `World`. Rendering, input and UI live in the main app and only read sim state, through the extract step.
2. **Make players' commands the only input to the simulation.** Input, AI, replays and (later) lockstep netcode all produce the same `PlayerCommand` stream. That gives you replays and multiplayer almost for free later, and it gives you deterministic tests now.
3. **Build content from RON files as bags of named components** (the OpenRA-style traits model):
   - Plugins register new "def components" under a short name, so a new mechanic means a new plugin plus new RON keys, not edits to a core schema.
   - Templates inherit from each other at load time.
   - String IDs are namespaced by mod and interned to `u16` IDs in a deterministic order.
   - A rules hash makes sure every peer has the same data.
   - Hot reload goes through Bevy's asset server, in dev builds only.
4. **Use one plugin per feature, each with two halves:** `XSimPlugin` and `XClientPlugin`. Crates enforce the layering. Start with about 10 crates and split feature modules into their own crates only when compile times or ownership call for it.
5. **No scripting in the MVP.** RON plus Rust plugins cover units, weapons, factions and maps. Leave a seam for mission scripting (Lua through `mlua`) after the MVP. WASM is only worth it if you later need sandboxed, untrusted mods.
6. **Write the pathfinding and spatial grid yourself, inside the sim.** Use a grid with one cost layer per movement class plus clearance, HPA* over the plain A* from the `pathfinding` crate, and a uniform-grid spatial hash. The Bevy pathfinding plugins are either not deterministic, not on 0.20 yet, or archived.

---

## 1. Prior art

### Bevy RTS and strategy projects

| Project | Bevy | Status |
|---|---|---|
| [DigitalExtinction/Game](https://github.com/DigitalExtinction/Game) | 0.13 | archived, last push 2024-09; 393 stars |
| [valeriyr/ferrets](https://github.com/valeriyr/ferrets) | 0.18 (sim uses `bevy_ecs` only) | active (2026-10), 5 stars |
| [tonianev/eonmark](https://github.com/tonianev/eonmark) | 0.19.1 (the sim crate has no engine dependency) | active, pre-alpha |
| [iron-curtain-engine/iron-curtain](https://github.com/iron-curtain-engine/iron-curtain) and its [design docs](https://iron-curtain-engine.github.io/iron-curtain-design-docs/) | Bevy, version not pinned in the workspace | early; no playable build |
| [ColCarlo/ReTzar](https://github.com/ColCarlo/ReTzar) | **=0.20.0** | active, 35 stars |
| [tvdboom/marabunta](https://github.com/tvdboom/marabunta) | 0.15.3 | 2025, small |

**Digital Extinction (DE)**
- **Layout:** a virtual workspace (`members = ["crates/*"]`) with about 30 crates: audio, behaviour, camera, combat, conf, connector, construction, controller, core, energy, gui, index, loader, lobby, lobby_client, lobby_model, map, menu, messages, movement, multiplayer, net, objects, pathing, spawner, terrain, test_utils, types, uom, and others. Shared versions are kept in `[workspace.dependencies]`.
- **Units in data:** one `assets/objects/<name>.obj.json` per object, holding the footprint hull, the collision mesh, and optional sections such as `cannon` (range, damage, charge time) or `flight`.
- **The catch with that data:** object kinds are a closed Rust enum (`ObjectType` → `ActiveObjectType` → `UnitType`, in `crates/types/src/objects.rs`). Adding a unit therefore needs a code change, which is exactly what we want to avoid.
- **Pathfinding:** custom triangulation plus Polyanya any-angle search on a navmesh (`crates/pathing`: `triangulation.rs`, `polyanya.rs`, `graph.rs`).
- **Spatial queries:** a custom grid index with `parry` colliders (`crates/index`: grid, aabb, range, segment).
- **Selection:** in-house controller and picking crates.
- **Networking:**
  - It is not lockstep. All traffic is relayed through a "DE Connector" server over a custom UDP protocol, with a REST lobby.
  - Each client is authoritative over the entities it controls, simulates everything, and broadcasts unreliable sync messages.
  - The docs explicitly accept short-term divergence between clients, paid for with bandwidth.
- **Takeaways:** the many-crates workspace and the JSON object files worked well. Avoid closed enums for unit types. Non-deterministic relay sync is simpler but rules out cheap replays.

**ferrets**
- This is the closest blueprint to what you want.
- **Crates:** `ferrets-math` (fixed-point), `-random`, `-geometry`, `-pathfinder` (deterministic grid), `-physics`, `-content` ("content vocabulary and registry"), `-script` (Lua content loading), `-simulation`, `-replay`, `-network` (lockstep P2P or host relay, over TCP or UDP), `-steam`, `-bevy-plugin`, plus a demo.
- **Simulation:** fixed tick with fixed-point math and state checksums. AI is written in Lua against an integer-only view of the world and issues the same commands a human does.
- **Takeaways:** strict crate layering, and a sim that depends only on `bevy_ecs`. Note that its content lives in Lua; I recommend RON for the MVP instead.

**eonmark**
- **Simulation:** a fixed-point sim at 20 Hz, driven by a command log.
- **Crates:** `sim` (pure Rust), `rules` (RON schema, loader and validator; it computes a `rules_hash`), `ai`, `game` (the only crate that uses Bevy), and `sim-cli` (`selftest` runs the same ticks on two threads and compares hashes; `data-check` validates `data/`).
- **Data conventions:** RON uses `deny_unknown_fields`, integer-scaled values (`speed_tiles_per_s_x100`), and the list index as the type ID.
- **Dependencies:** `fixed` 1.31, `ron` 0.12, `pathfinding` 4.16.
- **Takeaways:** develop headless first, and make data validation a CI gate. Its typed per-kind schema is very clear, but adding a mechanic means editing the schema crate.

**Iron Curtain**
- A C&C/OpenRA-compatible engine on Bevy.
- **Design:** the sim is pure and float-free and runs as FixedUpdate systems. It imports nothing from the net crate and shares only `ic-protocol`. Netcode is relay lockstep behind a `NetworkModel` trait.
- **Modding** is tiered: YAML data (with inheritance resolved at load time), then Lua, then WASM.
- **Takeaway:** a well-argued tiered modding model, but it is only documentation so far.

**ReTzar**
- A single crate in which `bevy` is behind an optional `game` feature, so the library, tools and tests build headless. It has a deterministic clock and fidelity tests.
- It vendors a patched `bevy_render`, which is a maintenance risk worth noting.
- **Takeaway:** feature-gating the engine is a cheap way to keep logic testable.

**marabunta**
- Uses `bevy_renet` for networking and the plain `pathfinding` crate for movement. A useful reference for a small multiplayer RTS. It is not data-driven.

### Non-Rust prior art worth copying

- **[OpenRA](https://github.com/OpenRA/OpenRA):** actors are YAML bags of traits. `Inherits: ^Tank` pulls in an abstract template, and `-TraitName:` removes an inherited trait. This is the model for "a unit is a list of components, and new traits come from code modules".
- **0 A.D.:** XML entity templates with `parent=` inheritance, plus scripted components.
- **[Recoil/Spring](https://github.com/beyond-all-reason/RecoilEngine):** Lua unit definitions, with gameplay "gadgets" acting as plugins.

### What prior art means for us

There is no shipped, large Bevy RTS. Every serious recent attempt (ferrets, eonmark, Iron Curtain) independently converged on the same design: a deterministic fixed-point sim, a command stream, a headless CLI, and data-driven content. DE, the most complete Bevy RTS so far, chose non-deterministic sync and closed enums; both are lessons in what to avoid.

---

## 2. Recommended workspace and plugin boundaries

```
blitz/
├─ Cargo.toml                 # virtual workspace, [workspace.dependencies], [workspace.lints], bevy pinned "=0.20.x"
├─ crates/
│  ├─ blitz_math/             # Fx (fixed-point), FxVec2, Angle, deterministic RNG, checked ops   (no bevy)
│  ├─ blitz_protocol/         # PlayerCommand, SimId, Tick, PlayerId, interned type IDs, serde/bincode (no bevy)
│  ├─ blitz_content/          # RON schemas, DefRegistry, mod manifests, inheritance, interning,
│  │                          #   validation, rules hash; pure loader load_dir() (serde/ron/erased-serde, bevy_ecs for Component)
│  ├─ blitz_sim/              # the sim App: SimTick schedule, SimSet ordering, SimId allocation,
│  │   ├─ map/                #   terrain grid, height, terrain types, movement-class cost layers, clearance
│  │   ├─ orders/             #   command application, order queues, unit behaviour states
│  │   ├─ movement/           #   HPA*/A*, steering/separation, formations, hull/turret rotation
│  │   ├─ combat/             #   weapons, ammo, armour/penetration by facing, damage, suppression/morale
│  │   ├─ vision/             #   line of sight, per-player visibility grid (fog is gameplay state)
│  │   └─ spatial/            #   uniform-grid spatial hash
│  │                          # each folder is one Plugin; promote to its own crate when it grows
│  ├─ blitz_ai/               # skirmish AI: reads the sim, emits PlayerCommand only
│  ├─ blitz_replay/           # record and replay command streams plus per-N-tick hashes
│  ├─ blitz_net/              # (post-MVP) lockstep session; wraps lightyear or a thin custom transport
│  ├─ blitz_client/           # full-bevy presentation
│  │   ├─ assets/             #   AssetLoaders for *.unit.ron etc., hot reload → ContentDb, loading states
│  │   ├─ view/               #   visual entity per sim entity, interpolation, animation, terrain mesh
│  │   ├─ fog/                #   upload the visibility grid to a texture, material extension
│  │   ├─ camera/  input/  selection/  ui/  audio/  vfx/
│  ├─ blitz_game/             # bin: assembles BlitzSimPlugins + BlitzClientPlugins + features + mods
│  ├─ blitz_cli/              # headless bin: data-check, replay verify, hash-dump, bench, bot-vs-bot
│  └─ blitz_test_utils/       # ScenarioBuilder, fixture content, assertions
├─ features/                  # optional mechanic crates, the "plugin" extension path
│  └─ blitz_feat_artillery/   #   e.g. indirect fire: mod sim; #[cfg(feature="client")] mod client;
├─ assets/base/               # the base game is itself a mod
│  ├─ mod.ron  factions/  units/  weapons/  maps/  models/  textures/  sfx/
└─ mods/                      # third-party data mods (same layout as assets/base)
```

**Dependency rules** (crates make these real, instead of relying on convention):
- `math` and `protocol` depend on nothing from Bevy.
- `content` depends on `math` and `protocol`.
- `sim` depends on `content` and uses only `bevy_app` + `bevy_ecs`. It never pulls in `bevy` with rendering.
- `client` depends on `sim` and full `bevy`.
- `ai` and `replay` depend on `sim` and `protocol`.
- No sim crate may depend on client, net or `bevy_render`. CI can check this with `cargo tree -e normal -p blitz_sim | grep bevy_render`.

### How the sim runs

- `blitz_sim::build_sim_app(content) -> App` builds the simulation.
- **Headless** (tests, CLI, server): that `App` is the whole program, stepped with `world.run_schedule(SimTick)`.
- **In the client:** it is inserted as a `SubApp` labelled `SimApp`.
  - Bevy 0.20's `SubApp` has `add_plugins`, `add_systems`, `set_extract`, and a public `update_schedule` field.
  - The extract function moves this frame's `PlayerCommand`s from the main world into the sim, and copies the sim state the view needs (positions, headings, health, visibility grid) back out. That costs a few thousand small copies per tick, which is trivial.
- **Why a separate `World`:**
  - Entity IDs and archetype layout stay identical on every peer, whatever the renderer spawns (particles, UI).
  - The client cannot accidentally mutate sim state.
  - The alternative, running the sim in FixedUpdate of the same `World` as Iron Curtain does, works too. But then you must never serialize or sort by `Entity`, and render code can write to sim components.
- **Tick driver:** the sim's own update schedule runs `SimTick` k times, where k is the number of fixed steps the main app accumulated (for example 20 Hz). The view interpolates between the previous and current sim pose using `Time<Fixed>::overstep_fraction()`.
- **System order:** inside `SimTick`, systems go in ordered sets: `ApplyCommands → Orders → Behaviour → Movement → Combat → Vision → Lifecycle → Hash`.
  - Start with `ExecutorKind::SingleThreaded`.
  - Set `ambiguity_detection: LogLevel::Error`.
  - Move to multi-threaded execution only once there are no ambiguities.

### The extension contract

Each feature implements both halves:

```rust
pub trait GameFeature {
    fn sim(&self, sim: &mut App);        // registers def components, sim systems in SimSet::*
    fn client(&self, app: &mut App) {}   // visuals, UI, sounds for the same feature
}
// blitz_game: one line per feature or mod crate
GameBuilder::new().feature(ArtilleryFeature).feature(AirSupportFeature).run();
```

A new mechanic (say, flamethrowers igniting forests) becomes:
1. A new crate with a `Burning` def component.
2. A sim system in `SimSet::Combat`.
3. A client fire VFX plugin.
4. A RON key `"Flamethrower": (...)` on the units that carry it.

None of that touches core code. Truly new data (units, weapons, factions, maps) needs no Rust at all.

Rust dynamic plugins are **not recommended**. Rust has no stable ABI, and Bevy's own `bevy_dynamic_plugin` was last published for 0.14.2. Code-level plugins should be compile-time crates, selected by Cargo feature or a small launcher.

---

## 3. Data-driven definitions

**Format: RON, loaded through Bevy's asset server for hot reload, plus a pure loader for headless use.** BSN (`bsn!` arrived in 0.19 and gained syntax improvements in 0.20) is code-first. The 0.20 release notes list a `.bsn` asset file format as future work, so don't build content on it. It could later carry visual prefabs.

### Rules

1. **Composition at runtime, inheritance only when authoring.**
   - A unit file has an optional `inherits` and a map of named components.
   - Inheritance is resolved at load time, one component at a time: the child replaces the parent's component wholesale, and `remove: [...]` drops inherited ones (like OpenRA's `-Trait`).
   - Abstract templates (`abstract: true`) are never spawned.
   - Field-level merge can come later if modders want it.
2. **Open component set via a registry.**
   - Plugins call `app.register_def_component::<ArmorDef>("Armor")`.
   - The loader deserializes each map value with a registry-driven `DeserializeSeed` (type-erased, using `erased-serde` 0.4.10).
   - `bevy_common_assets`' `RonAssetPlugin` needs `T: Deserialize`, so it cannot do this. Write your own `AssetLoader`, built in `Plugin::finish()` once every plugin has registered.
   - Use `bevy_common_assets` 0.18 only for fixed-schema files such as `mod.ron` and factions.
3. **Keep "def" types separate from runtime components.**
   - A def (`TurretsDef`) is authoring-friendly: floats, string references.
   - `DefComponent::instantiate(&self, &ContentDb) -> impl Bundle` turns it into runtime components with fixed-point values and interned IDs, exactly once, at spawn.
   - Parsing decimal text into `f64` and converting to fixed-point is deterministic, so floats in RON are fine. eonmark's `_x100` integers are an alternative if you want zero floats anywhere.
4. **IDs.**
   - Content IDs are strings namespaced by mod (`"base.de.pz4_f2"`).
   - After loading every enabled mod, sort them and intern to `UnitTypeId(u16)`, `WeaponTypeId(u16)` and so on. Every machine gets the same numbers.
   - Cross-references (unit → weapon → ammo, faction → roster) are resolved in a link pass that reports errors with file and field (`serde_path_to_error` 0.1.20).
   - A **rules hash** (for example xxh64 over the canonical resolved content) goes into replays and the lobby handshake.
5. **Mods.**
   - `mod.ron` holds `{ id, version, depends, load_after }`.
   - A later mod may override an existing ID only with an explicit `override: true`, so accidental collisions are errors.
   - Unknown fields are always errors: `#[serde(deny_unknown_fields)]` on every def, and unknown component names are rejected.
6. **Hot reload.**
   - Enable Bevy's `file_watcher` feature.
   - On `AssetEvent::Modified`, rebuild the `ContentDb`, send `ContentReloaded`, and re-instantiate stat components on live entities while keeping their state (HP ratio, ammo fraction).
   - Dev and single-player only, because reloading changes the rules hash.

### Example: weapon (`assets/base/weapons/kwk40_l43.weapon.ron`)

```ron
Weapon(
    id: "base.kwk40_l43",
    name: "7.5 cm KwK 40 L/43",
    class: Cannon,
    range: (min: 0.0, max: 900.0),            // metres; converted to Fx at load
    reload_ticks: 80,                          // 4.0 s at 20 Hz
    aim_ticks: 10,
    dispersion_mrad: 1.2,
    projectile: Ballistic(muzzle_velocity: 740.0, arc: Flat),
    ammo: {
        "apcbc": (kind: ArmorPiercing, damage: 135,
                  penetration_mm: [(0.0, 89), (500.0, 81), (1000.0, 72)]),   // range→mm, linear interp
        "he":    (kind: HighExplosive, damage: 55, splash_radius: 6.0, suppression: 30),
    },
    fx: (muzzle: "fx/muzzle_large", impact: "fx/impact_ap", sound: "sfx/kwk40"),  // client-only, ignored by the sim
)
```

### Example: unit, abstract template plus concrete unit

```ron
// assets/base/units/_templates.unit.ron
Unit(
    id: "base.abstract.medium_tank",
    abstract: true,
    tags: ["vehicle", "armored", "tracked"],
    components: {
        "Selectable": (),
        "Mobility":   (class: Tracked, max_speed: 10.0, accel: 1.5, turn_rate_deg: 35.0),
        "Vision":     (range: 400.0, eye_height: 2.5),
        "Crew":       (count: 5, bail_out_below: 0.15),
    },
)

// assets/base/units/de/pz4_f2.unit.ron
Unit(
    id: "base.de.pz4_f2",
    inherits: Some("base.abstract.medium_tank"),
    name: "Pz.Kpfw. IV Ausf. F2",
    faction: "base.germany",
    cost: 220,
    visual: (model: "models/de/pz4_f2.glb#Scene0", scale: 1.0),    // client-only
    components: {
        "Hull":     (hp: 420, footprint: (3.0, 5.9)),
        "Armor":    (front: 50, side: 30, rear: 20, top: 10),        // mm, by facing
        "Mobility": (class: Tracked, max_speed: 11.0, accel: 1.6, turn_rate_deg: 38.0),  // replaces template's
        "Turrets": [
            (mount: "turret_joint", traverse_deg_s: 15.0,
             weapons: [ (weapon: "base.kwk40_l43", ammo: { "apcbc": 50, "he": 37 }) ]),
            (mount: "hull_mg", traverse_deg_s: 60.0, arc_deg: 30.0,
             weapons: [ (weapon: "base.mg34", ammo: { "ball": 3000 }) ]),
        ],
    },
    remove: [],
)
```

### Rust side, inside a feature plugin

```rust
#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct ArmorDef { front: u16, side: u16, rear: u16, top: u16 }

impl DefComponent for ArmorDef {
    fn validate(&self, _: &LinkCtx) -> Result<(), DefError> { Ok(()) }
    fn instantiate(&self, _: &ContentDb, e: &mut EntityWorldMut) { e.insert(Armor::from(self)); }
}

impl Plugin for CombatSimPlugin {
    fn build(&self, app: &mut App) {
        app.register_def_component::<ArmorDef>("Armor")
           .register_def_component::<TurretsDef>("Turrets")
           .add_systems(SimTick, (acquire_targets, aim_and_fire, resolve_hits).chain().in_set(SimSet::Combat));
    }
}
```

Factions use a fixed schema: id, name, roster of unit IDs, reinforcement and support options such as artillery or air strikes, and AI doctrine. A map is a `map.ron` (metadata, players, objects, triggers) plus PNG layers for height and terrain type, which are easy to author before a real editor exists.

**Scripting verdict.** Not for the MVP.
- When campaign missions need triggers, start with data triggers: condition/action lists in `map.ron`.
- Then add Lua (`mlua` 0.12.2, no Bevy dependency; or `bevy_mod_scripting` 0.23, which needs 0.20 and supports Lua 5.1–5.4, LuaJIT, Luau and Rune but is described as work in progress with a churning API).
- In lockstep, scripts must only issue commands or read an integer view, as ferrets does.
- `rhai` 1.27 is pure Rust and easy, but slower, and `bevy_mod_scripting` no longer lists it.
- WASM (`wasmtime` 50) gives the best sandbox and determinism for untrusted mods, at a big tooling cost. Defer it.

---

## 4. Pathfinding, spatial queries, fog, networking

**Pathfinding: write it in the sim, on a grid.** Blitzkrieg-scale games have tens to a few hundred units per side, and the terrain matters: roads, forest, rivers and bridges, trenches, and different unit sizes.
- One integer cost layer per movement class (Foot, Wheeled, Tracked), plus a clearance map so a tank won't path through gaps only infantry fit.
- HPA* over clusters of about 16×16 cells for long routes. Use plain A* from `pathfinding` 4.16.0 for refinement and short paths. It has no Bevy dependency and uses `rustc-hash`/`indexmap`, so its hashing is deterministic.
- Dynamic changes (a blown bridge, wrecks, new buildings) invalidate the affected clusters.
- Local movement: separation and steering in fixed-point. Tanks rotate the hull before moving. Formation offsets come from a group leader.
- Flow fields are a post-MVP optimisation for many units ordered to one destination.

How the off-the-shelf options compare:

| Option | Latest | bevy req | 0.20? | Verdict |
|---|---|---|---|---|
| bevy_flowfield_tiles_plugin | 0.16.0 | ^0.20 | **yes** | Good for prototyping or as a reference. Fields are generated asynchronously in `PostUpdate` with f32 values, so it is not lockstep-safe. Sector-based. Actor sizes are handled with separate field sets. |
| vleue_navigator (navmesh, polyanya) | 0.16.0 | ^0.19.1 | **no** (a dependabot PR #149 to 0.20 is open) | Any-angle navmesh, f32. Use it if you drop determinism or go free-form 3D. |
| landmass / bevy_landmass | 0.9.3 / 0.12.1 | bevy_landmass ^0.19.0 | **no** (PR #195 for 0.20.0-rc.1 and a `bevy-0.20` branch exist) | Navmesh plus local avoidance, f32. |
| oxidized_navigation | 0.12.0 | ^0.15 | **no**; the repo is archived | Avoid. |
| bevy_northstar (HPA* grid) | 0.7.0 | ^0.19 | **no** | Good HPA* reference design. |
| polyanya | 0.17.1 | none | n/a | Use if you later want any-angle paths on a navmesh in the sim (that's what DE did). |
| hierarchical_pathfinding | 0.5.0 | none | n/a | Last release in 2021; reference only. |

**Spatial queries.**
- A uniform-grid spatial hash inside the sim. Cell size is about half the typical weapon range.
- Rebuild it each tick with a counting sort into a flat `Vec<SimId>`. Iteration is ordered, which is deterministic and cheap for thousands of entities.
- Re-acquire targets in staggered batches: each unit every N ticks, offset by `SimId % N`. Break ties by `(distance, SimId)`.
- `bevy_spatial` 0.11 (bevy ^0.16) is not on 0.20. `kiddo` 6.3 and `rstar` 0.13 are fine for client-side, float-based queries such as cursor hover.

**Determinism checklist for the sim.**
- Fixed-point math (`fixed` 1.31 behind your own `Fx` wrapper; `I32F32` or `I48F16`).
- No `std` `HashMap` with `RandomState`; use `BTreeMap`, `IndexMap` with an Fx hasher, or sorted vectors.
- Seeded RNG stored in a sim resource (your own PCG, or `bevy_rand`/`bevy_prng` 0.16, which support 0.20).
- No `Time` inside the sim; tick count only.
- No float physics (keep `avian3d`/`rapier` out of the sim; they are fine for cosmetic debris on the client).
- Commands and saves reference `SimId`, never `Entity`.

**Fog of war.**
- Visibility is **sim state**: one grid per player, recomputed every few ticks from vision sources, with line of sight against terrain height and blocking objects. Targeting and the AI need it.
- The client uploads the local player's grid to an `R8Unorm` image (only when it changes), samples it in an `ExtendedMaterial<StandardMaterial, FogExt>` on terrain, and hides or ghosts enemy visuals in fogged cells.
- Bilinear sampling plus a short temporal lerp gives soft edges.
- `bevy_fog_of_war` 0.5.0 supports 0.20 but is 2D-only (`Camera2d`/sprites); use it as a design reference only.

**Selection and input.**
- Single-click picking: Bevy's built-in picking with `MeshPickingPlugin`.
- Box select: project unit positions with `Camera::world_to_viewport`, which is simpler and faster than picking.
- Input mapping: `bevy_enhanced_input` 0.27.0 or `leafwing-input-manager` 0.22.0; both support 0.20.
- Every UI action produces a `PlayerCommand`, never a direct state change.

**Networking (post-MVP, but design for it now).**
- Use lockstep with inputs only.
- `lightyear` 0.31.0 (requires `bevy_app`/`bevy_ecs` ^0.20) supports **deterministic replication of inputs only**, compatible with lockstep or rollback. It has a `deterministic_replication` example and supports UDP, WebTransport, WebSocket and Steam (via aeronet).
- `bevy_replicon` 0.45.2 (0.20 yes) is state replication, the wrong model for hundreds of units.
- `bevy_ggrs` 0.22 (bevy ^0.19, so no) does rollback, which is costly for RTS-sized state.

**Other crates checked**

| Crate | Latest | bevy req | Supports 0.20? |
|---|---|---|---|
| bevy_asset_loader | 0.28.1 | ^0.20.0 | yes |
| iyes_progress | 0.18.0 | ^0.20.0 | yes |
| bevy_common_assets | 0.18.0 | ^0.20 | yes |
| bevy_rts_camera | 0.15.0 | ^0.20 | yes |
| bevy_egui | 0.43.1 | ^0.20.0 | yes |
| bevy-inspector-egui | 0.38.0 | ^0.20.0 | yes |
| bevy_kira_audio | 0.27.1 | ^0.20 | yes |
| moonshine-save | 0.8.0 | ^0.20 | yes |
| bevy_mod_debugdump | 0.17.0 | ^0.20.0 | yes |
| bevy_vector_shapes | 0.14.0 | ^0.20 | yes |
| avian3d | 0.8.0 | ^0.20 | yes |
| bevy_rapier3d | 0.37.0 | ^0.20 | yes |
| bevy_mod_scripting | 0.23.0 | ^0.20 | yes |
| bevy_hanabi (VFX) | 0.19.0 | ^0.19 | **no** |
| bevy_mod_outline (selection outlines) | 0.13.0 | ^0.19.0 | **no** |
| bevy_framepace | 0.22.0 | ^0.19 | **no** |
| bevy_tweening | 0.16.0 | ^0.19 | **no** |
| bevy_seedling | 0.8.0 | ^0.19.0 | **no** |
| big-brain (AI) | 0.22.0 | ^0.15.0 | **no** (stale) |
| bevy_spatial | 0.11.0 | ^0.16 | **no** |

Crates with no Bevy dependency, so never blocked by a Bevy release: `ron` 0.12.2, `fixed` 1.31.0, `pathfinding` 4.16.0, `polyanya` 0.17.1, `erased-serde` 0.4.10, `xxhash-rust` 0.8.19, `mlua` 0.12.2, `rhai` 1.27.0, `wasmtime` 50.0.0.

Bevy 0.20 itself adds a built-in `PanOrbitCamera` (not RTS-shaped), WESL shaders, per-column change ticks (`#[component(summary_tick)]`), `despawn_all`, and schedule shuffling for tests (`shuffle_seed`).

---

## 5. Testing approach

1. **Pure unit tests, no Bevy:** `blitz_math` (fixed-point edge cases and overflow), `blitz_content` (parse, inheritance, link errors, rules-hash stability), pathfinding over hand-made grids. Add `proptest` for path validity (paths stay inside passable cells and respect clearance) and for damage and penetration formulas.
2. **Headless sim tests.**
   - The sim needs no `MinimalPlugins` because it doesn't use `Time`.
   - `let mut app = build_sim_app(fixtures::base()); …; for _ in 0..600 { app.world_mut().run_schedule(SimTick); }`
   - Wrap it in `blitz_test_utils::ScenarioBuilder`: `.map_flat(64,64).unit("base.de.pz4_f2", P0, at(10,10)).command(Tick(1), AttackMove{..}).run_ticks(600)`, then assert on outcomes.
   - Test individual systems by building an `App` with just the plugin under test and a hand-built world. Or use `world.run_system_once(system)`.
3. **Client-side systems that use `Time`:** `App::new().add_plugins(MinimalPlugins)` (in 0.20 that's TaskPool, FrameCount, Time and ScheduleRunner) plus `TimeUpdateStrategy::ManualDuration(..)` or the new `FixedTimesteps(n)`, so each `app.update()` advances an exact number of fixed steps. Test input-to-command mapping by injecting input messages and asserting the emitted `PlayerCommand`s. No GPU needed.
4. **Determinism gates.**
   - Run each scenario twice and require identical per-tick state hashes.
   - Run with Bevy 0.20 schedule shuffling (`ScheduleBuildSettings::shuffle_seed`, which needs the `debug` feature) to prove there is no hidden order dependence.
   - Keep golden replays (command log plus expected hashes every N ticks) and verify them on a **Windows, Linux and macOS CI matrix**.
5. **Data gate:** `blitz_cli data-check assets/` loads every mod, validates references and enforces `deny_unknown_fields`. It runs in CI and as a pre-commit hook for modders.
6. **Rendering:** keep it thin and smoke-test only. Optionally use Bevy's `bevy_ci_testing` feature (`CiTestingPlugin`) for scripted screenshot runs on a GPU runner later.
7. **Benchmarks:** `criterion` over `SimTick` with 200, 1000 and 3000 units, and over path queries, run on PRs to catch regressions.
8. **CI:**
   - GitHub Actions: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo nextest`, `cargo deny`.
   - `Swatinem/rust-cache`; the Linux job needs `libasound2-dev libudev-dev`.
   - Run the sim and content tests separately; they're fast and need no GPU.
   - Use `bevy/dynamic_linking` in dev builds only, never in releases.

---

## 6. Risks

1. **Bevy churn and ecosystem lag.** Bevy breaks its API every release (roughly 3–4 months), and two weeks after 0.20 the navmesh crates, `bevy_hanabi`, `bevy_mod_outline`, `bevy_framepace` and `bevy_ggrs` are still on 0.19. Mitigations:
   - Keep the sim on `bevy_app`/`bevy_ecs` only.
   - Prefer crates with no Bevy dependency in the sim.
   - Pin `bevy = "=0.20.x"` and upgrade once per release on a branch.
   - Budget for owning selection outlines and VFX yourself, or wait for those crates to catch up.
2. **Determinism is hard to retrofit.** The hazards are floats, `RandomState` hash maps, system-order ambiguity, parallel iteration, `Entity` IDs and third-party float physics. Committing to fixed-point plus hash tests from week one is the main up-front cost of this architecture, and the best value: replays, lockstep and reproducible bug reports all depend on it.
3. **Fixed-point friction.** Overflow, missing trig functions and awkward ergonomics. Mitigations: wrap it in your own `Fx` and `FxVec2` types, use lookup tables or CORDIC for trig, check operations in debug builds, and keep presentation in f32.
4. **Open component schemas trade type safety for extensibility.** A typo in a component name or field can go unnoticed unless you have `deny_unknown_fields`, an unknown-component error, a link pass with path-aware errors and the CI data gate. Optionally generate a JSON Schema from the registry for editor autocompletion.
5. **Scripting scope creep.** Lua or WASM is a large surface: API design, sandboxing, determinism, debugging. Keep it out of the MVP and use data triggers first.
6. **Pathfinding quality is gameplay-critical.** Mixed unit sizes, tank turning circles, convoy traffic jams on roads, dynamic obstacles. Plan dedicated time; HPA* invalidation and local steering are where RTS projects stall.
7. **Hot reload versus multiplayer and rules hash.** Disable it outside dev builds. Make reload re-instantiate stats without breaking runtime state.
8. **BSN is not data-file-ready.** `.bsn` assets are listed as future work in the 0.20 notes, and scenes or templates may change again in 0.21. Keep the def-to-spawn step behind your own `DefComponent::instantiate` so it can emit BSN templates later.
9. **Thin prior art.** The only sizeable Bevy RTS (DE) is archived on 0.13, and the deterministic ones (ferrets, eonmark, Iron Curtain) are young, with 2–35 stars and no shipped game between them. Treat them as design references, not dependencies. Spike the sim, pathfinding and fog early.
10. **Compile times and Windows dev loop.** Bevy plus many crates is slow to build. Use `dynamic_linking` in dev, `lld`, `opt-level = 1` for your own code and 3 for dependencies, and keep most work in the fast headless crates, which eonmark also does.
11. **IP and branding.** Don't use the "Blitzkrieg" name or its assets. Market it as an original WWII RTS.
