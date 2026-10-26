# Using npc-framework in your Bevy game

This guide is for a game that is **not** this repository's village or testbed: your own Bevy app, your own
units, map, camera, input, physics and tick rate. The motivating case is a large-scale RTS that wants
sophisticated, deterministic units without forking the framework. Everything here goes through one crate, the
facade `npc_framework`; you never edit a framework crate to add content.

The reference integration is `apps/rts_units_demo`: it depends on `npc_framework` and `bevy` only
(`xtask/tests/arch_fitness.rs` `rts_demo_uses_only_facade` pins that), registers two unit types by name, runs
the sim at 10 Hz next to its own 60 Hz physics, and drives 200 units with RTS orders. Read it next to this guide:
`apps/rts_units_demo/src/{sim,host,view,main}.rs`. The fully loaded example is the Actor Testbed
(`content/testbed_content`, every capability on).

The headless snippets of §4–§6 were compile-checked and run when this guide was written (wave H1: a scratch crate
on the facade with every headless capability; the sentry ran 3000 ticks twice with equal state hashes, and an app
with every capability and all the §6 registrations froze and ran). They are not a test in the workspace, so if one
drifts, trust the API docs (`cargo doc -p npc_framework --open`) and the cited sources. The app-level snippets
(§3, §9) are excerpts of `apps/rts_units_demo/src/{main,sim,host,view}.rs`.

Per-type reference: every main ECS component ([components index](entities/components/index.md)) and every main
system ([systems index](entities/systems/index.md)) has its own page: fields and units, which systems read and
write it, its `NpcSet` and ordering, and whether it is hashed and/or saved in world snapshots (ADR 0013).

- [1. Concepts in one page](#1-concepts-in-one-page)
- [2. Add the dependency](#2-add-the-dependency)
- [3. NpcPlugins: the builder](#3-npcplugins-the-builder)
- [4. Register content by name](#4-register-content-by-name)
- [5. The host owns schedule, tick and movement](#5-the-host-owns-schedule-tick-and-movement)
- [6. Capabilities](#6-capabilities)
- [7. Glue: what happens when two capabilities meet](#7-glue-what-happens-when-two-capabilities-meet)
- [8. Determinism rules for your content](#8-determinism-rules-for-your-content)
- [9. View helpers and debug tooling in your app](#9-view-helpers-and-debug-tooling-in-your-app)
- [10. Testing and debugging your integration](#10-testing-and-debugging-your-integration)
- [11. Versions](#11-versions)

## 1. Concepts in one page

- **The sim** runs on its own schedule, `NpcSimTick`, in whole ticks, with sets
  `NpcSet::Inputs → Needs → Sense → Think → Actions → Locomotion → Effects → Cleanup`. It is integer-only
  (`SimPos` in mm, needs in milli-units 0..=100 000), orders entities by `SimId`, and uses a counter-based RNG,
  so the same seed and the same commands give the same state hash on every OS (ADRs 0002–0004).
- **Units think with behaviour trees** (`npc_ai`): a flat `TreeDef` registered by name, guards with
  `Abort::LowerPriority` for interrupts, and leaves that start **actions**. An action is an ordinary component
  plus an ordinary system (ADR 0005). Think runs only for units whose leaf finished or that asked to re-think.
- **Everything is registered by stable name** (`NpcAppExt`, ADR 0009): needs, actions, conditions, scorers,
  trees, hashed components and resources, commands. Ids are frozen sorted by name when the app finishes, so
  plugin and registration order never change ids, hashes or replays.
- **The only way to change the sim from outside is a `SimCommand`** (`submit`): validated, journaled, applied
  in `(tick, seq)` order and recorded for replays. Your UI, AI director, network layer and agents all submit
  commands.
- **The host owns time, input and movement** (ADR 0008): you pick the schedule and tick rate, the framework
  never writes `Time<Fixed>` or binds keys, and movement is a seam (`MoveIntent` → your mover → `MoveStatus`).
- **Capabilities are opt-in** (perception, rules, interaction, RTS command, vehicles, progression, crafting,
  agent control, grid pathfinding): each is a feature plus a plugin; without it nothing of it runs or is
  hashed. Pairs of capabilities meet in facade **glue** (ADRs 0011, 0012).
- **Debug tooling never changes results** (ADR 0006): the journal, inspection, replays, the profiler and the
  BRP methods work on your game too.

## 2. Add the dependency

The framework is not on crates.io yet; depend on it by path (or git) from your workspace:

```toml
[dependencies]
bevy = "0.20"   # your own Bevy, with your own features
npc_framework = { path = "../npc-framework/crates/npc_framework", default-features = false, features = [
    "tools", "perf", "locomotion",          # the defaults
    "nav", "command",                       # what the RTS demo uses
    # "perception", "rules", "interaction", "vehicles", "progression", "crafting", "agent-io",
    "view-helpers",                         # generic presentation helpers (pulls `bevy`)
    # "remote",                             # BRP npc/* methods (debug builds; pulls bevy_remote)
] }
serde = { version = "1", features = ["derive"] }
```

| Feature | Brings | Notes |
|---|---|---|
| `tools` (default) | `npc_tools`: CLI helpers, bench harness, command files, the replay recorder | headless |
| `perf` (default) | `npc_perf`: per-system / per-mechanic profiler and budgets | §9.2 |
| `locomotion` (default) | `npc_locomotion`: `MoveIntent`/`MoveStatus`, `StraightLineMover` in `NpcPlugins` | §5.2 |
| `nav` | `npc_nav`: deterministic grid A*, `NavPlugin`, `NavAgent` | §6.1 |
| `perception` | `npc_perception`: sensors, line of sight, hearing, memory | §6.2 |
| `rules` | `npc_rules`: character sheet, skills, traits, 3d6 rolls | §6.3 |
| `interaction` | `npc_interaction`: items, inventory, equipment, doors, tools, verbs | §6.4 |
| `command` | `npc_command`: owners, RTS orders, queues, formations, squads | §6.5 |
| `vehicles` | `npc_vehicles` (+ `interaction`): seats, mounting, crew-driven / direct vehicles | §6.6 |
| `progression` | `npc_progression` (+ `rules`): XP, levels, spend policies, learning | §6.7 |
| `crafting` | `npc_crafting` (+ `interaction`, `rules`): recipes, workstations, crafting orders | §6.8 |
| `agent-io` | `npc_agent_io` (+ `tools`): external controllers, observations, affordances, sessions, CLI/MCP clients | §6.9 |
| `view-helpers` | `npc_view_helpers`: `VisualOf`, interpolation, panels, selection, screenshots, gizmos per capability, the RTS command UI with `command` | §9 |
| `remote` | BRP `npc/*` (+ `agent/*` with `agent-io`, `npc/sheet` with `rules`) | debug builds |

The headless crates depend on `bevy_ecs`/`bevy_app` sub-crates only; the `bevy` umbrella (and so your renderer
features) comes in only with `view-helpers` or `remote`. The facade re-exports the sub-crates as modules
(`npc_framework::{kernel, ai, needs, jobs, locomotion, nav, perception, rules, interaction, command, vehicles,
progression, crafting, agent_io, perf, tools, view_helpers, glue}`) plus `bevy_app`, `bevy_ecs`, `bevy_reflect`,
and `npc_framework::prelude::*` holds what content uses day to day.

## 3. NpcPlugins: the builder

```rust
use bevy::prelude::*;
use npc_framework::prelude::*;

App::new()
    .add_plugins(DefaultPlugins)
    .insert_resource(Time::<Fixed>::from_hz(60.0))                  // the host's own fixed rate
    .insert_resource(SpatialGrid::new(1000, GridBounds::centered(96, 96))) // 1 m cells, 96 × 96
    .add_plugins((
        NpcPlugins::new().seed(1).tick_hz(10).run_in(FixedUpdate).with_command(),
        NavPlugin,
        MyUnitsPlugin,                                                // §4
    ))
    .run();
```

`NpcPlugins` is a `PluginGroup`: the kernel (`NpcKernelPlugin`), needs, jobs, the spatial grid
(`SpatialPlugin`, unhashed and a no-op without blockers), with `locomotion` the `StraightLineMover`, and the
driver. `NpcPlugins::new().set(..)` replaces a member plugin as usual (`PluginGroup::set`); `disable` is a
`PluginGroupBuilder` method, so call it after `.build()`: `NpcPlugins::new().build().disable::<StraightLineMover>()`.

| Method | Default | Effect |
|---|---|---|
| `.seed(n)` | 0 | world seed of the counter RNG |
| `.tick_hz(n)` | 20 | sim ticks per second of host time |
| `.run_in(schedule)` | `FixedUpdate` | the host schedule the driver system runs in (`Update`, `FixedUpdate`, your own) |
| `.without_driver()` | – | no driver: you run ticks yourself (`run_ticks(world, n)`): tests, servers, lockstep |
| `.pacing(Pacing::EveryRun)` | `HostTime` | `SimControl.multiplier` ticks per schedule run instead of real time |
| `.determinism(DeterminismMode::Relaxed)` | `Strict` | let the host own positions (§5.3) |
| `.with_perception()` | off | `PerceptionPlugin` |
| `.with_rules()` | off | `RulesPlugin` |
| `.with_per_roll_detection()` | off | perception + rules with the `per_roll` detection check active |
| `.with_interaction()` | off | `InteractionPlugin` |
| `.with_command()` | off | `CommandPlugin` |
| `.with_vehicles()` | off | `VehiclesPlugin` + interaction |
| `.with_progression()` | off | `ProgressionPlugin` + rules |
| `.with_crafting()` | off | `CraftingPlugin` + interaction + rules |
| `.with_agent_io()` | off | `AgentIoPlugin` |

Every `with_*` also adds the glue plugin of each pair that is on (§7); do not add those plugins yourself as
well (Bevy rejects duplicates). `NavPlugin` is not part of the group: add it next to it. The equivalent tuple
form (`(NpcPlugins::new(), PerceptionPlugin, RulesPlugin, AgentIoPlugin::default(),
glue::PerceptionRulesPlugin::default(), ..)`) is documented on `NpcPlugins`.

The driver keeps its own accumulator (`SimTime`), runs at most 100 ticks per schedule run, and reads only the
delta of the schedule it runs in. Pause, speed and single steps are the plain resource `SimControl`
(`paused`, `multiplier`, `pending_steps`): bind them to your own keys; the framework binds none.

## 4. Register content by name

A **unit type** is content: a marker component, needs, actions, conditions, a tree and a spawn command. Here is
a complete one, a sentry that walks to its post, looks around, and rests when its alertness runs low. The
pattern is the RTS demo's `RtsContentPlugin` (`apps/rts_units_demo/src/sim.rs`) and the village's
`VillagePlugin` (`content/village_content/src/lib.rs`).

### 4.1 Needs

```rust
use npc_framework::prelude::*;

pub const ALERTNESS: NeedDef = NeedDef {
    name: "Alertness",   // journal / SetNeed name; the id is assigned sorted by name
    key: "alertness",    // JSON key in inspection
    order: 0,            // presentation order
    decay_per_tick: 30,  // milli-units per tick
    thresholds: NeedThresholds { low_enter: 40_000, low_exit: 80_000, urgent_enter: 15_000, urgent_exit: 50_000 },
};
```

A unit gets every registered need with `world.resource::<NeedRegistry>().full()` (a `Needs` component). Levels
(`NeedLevel::Ok | Low | Urgent`) change with hysteresis and are journaled as `NeedLevel`. Look ids up by name
(`NeedRegistry::id("Alertness")`), never by position.

- **Need ids are sorted by name**, not by registration or `order`: at freeze (`App::finish`, or `ensure_frozen(world)`)
  the defs are sorted by `name` and numbered densely, and `Needs.values[id]` follows that order. The village
  has `Energy` = 0 and `Hunger` = 1, and a new `Thirst` would be 2, but a new `Comfort` would take id 1 and
  move `Hunger` to 2. So look ids up after freeze and never store a raw index. A new need is a new name, so it
  changes the content hash and the state hash encoding (`Needs` is length-prefixed): re-baseline
  ([rebaselining.md](rebaselining.md)). `order` only sorts presentation (inspect, the agent observation).
- **`SetNeed` timing.** `SetNeed {unit, need, value}` (and any content command) applies in `NpcSet::Inputs` of
  the tick it is queued for. `submit(world, None, ..)` queues it for the next tick that runs, so nothing changes
  until then. In that tick, `NpcSet::Needs` decays the new value once and updates its level (hysteresis,
  journal `NeedLevel`, a `ThinkRequest`). After `submit` + `run_ticks(world, 1)` the value is `value -
  decay_per_tick` and the level matches it. Code that writes `Needs` directly (a test, a content command's
  `apply`) sees the old level until the next `NpcSet::Needs` pass. Call `update_level(need, &thresholds)` there
  if the level must follow at once (the agent scenario loader does).

### 4.2 Components and the state hash

Every component that is sim state is registered so it is hashed, diffed and inspected by name:

```rust
use bevy_ecs::prelude::*;
use bevy_reflect::Reflect;
use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[reflect(Component)]
pub struct Sentry {
    pub post: SimPos,
}

impl SimHash for Sentry {
    const NAME: &'static str = "sentry";          // stable: renaming it changes the content hash
    fn sim_hash(&self, h: &mut Fnv64) {           // fixed-width integers, in a fixed order
        h.i32(self.post.x);
        h.i32(self.post.z);
    }
}
```

`register_sim_component::<T>()` needs `Component + SimHash + Serialize + Reflect`; resources use
`register_sim_resource::<R>()` (`SimHashResource`). A component you forget to register is not in the state
hash, so a desync in it shows up late or never.

### 4.3 Actions

An action is a `SparseSet` component inserted by its start function, removed by its stop function, and ticked by
one system in `NpcSet::Actions` that sets the leaf's status. Only that system changes the status after start.

```rust
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
#[component(storage = "SparseSet")]
#[reflect(Component)]
pub struct Resting { pub remaining: u32 }
// impl SimHash for Resting { const NAME = "sentry_resting"; .. }

fn rest_start(world: &mut World, agent: Entity, param: &ParamValue) -> Status {
    let ticks = param.as_int().unwrap_or(100).max(1);
    world.entity_mut(agent).insert(Resting { remaining: u32::try_from(ticks).unwrap_or(1) });
    Status::Running
}

fn rest_stop(world: &mut World, agent: Entity, _reason: StopReason) {
    if let Ok(mut e) = world.get_entity_mut(agent) {
        e.remove::<Resting>();   // and release any reservation it holds
    }
}

fn rest_system(registry: Res<NeedRegistry>, mut q: Query<(&mut Resting, &mut ActiveLeaf, &mut Needs)>) {
    let Some(alert) = registry.id(ALERTNESS.name) else { return };
    for (mut rest, mut leaf, mut needs) in &mut q {
        if leaf.status != Status::Running { continue; }
        let need = needs.get_mut(alert);
        need.value = (need.value + 400).min(NEED_MAX);
        rest.remaining = rest.remaining.saturating_sub(1);
        if rest.remaining == 0 { leaf.status = Status::Success; }
    }
}
```

Walking goes through the locomotion seam: write a `MoveIntent`, end on `MoveStatus`, checked in
`NpcSet::Locomotion` **after** the movers so the walk ends in the tick it arrives:

```rust
fn go_start(world: &mut World, agent: Entity, _param: &ParamValue) -> Status {
    let Some(post) = world.get::<Sentry>(agent).map(|s| s.post) else { return Status::Failure };
    set_move_intent(world, agent, MoveIntent::to(post, 60));   // 60 mm per tick
    world.entity_mut(agent).insert(GoingToPost);
    Status::Running
}

fn go_stop(world: &mut World, agent: Entity, _reason: StopReason) {
    clear_move_intent(world, agent);
    if let Ok(mut e) = world.get_entity_mut(agent) { e.remove::<GoingToPost>(); }
}

fn arrival_system(mut q: Query<(&MoveStatus, &mut ActiveLeaf), With<GoingToPost>>) {
    for (status, mut leaf) in &mut q {
        if leaf.status != Status::Running { continue; }
        match status {
            MoveStatus::Arrived => leaf.status = Status::Success,
            MoveStatus::Blocked { .. } => leaf.status = Status::Failure,
            _ => {}
        }
    }
}
```

The `StraightLineMover` (default) walks straight and reports `Blocked` in front of a blocking cell; with
`NavPlugin`, units with `NavAgent` follow grid A* paths instead; your own mover can own any unit (§5.2). The
same action works with all three.

**A second action system goes into the same `.chain()`.** Every action system takes `&mut ActiveLeaf`. Two
unordered systems that both do are an ambiguity, and ambiguity detection is `Error` in Strict mode, so the build
panics. Chain all of a content's action systems in `NpcSet::Actions`: the village has `(order_system,
move_to_system, eat_system, work_system, sleep_system, idle_system).chain()`, the testbed `(walk_cancel_system,
idle_system).chain()`. Chain the arrival checks in `NpcSet::Locomotion` `.after(NpcMoverSet)` the same way. Two
plugins that both add action systems order theirs with `.after(..)` on the other's system or set. The framework's
own systems around these sets, in tick order (from the plugins' `configure_sets`):

| Where | Framework systems |
|---|---|
| after `Sense`, before `Think` | `NpcCommandSet::Execute`: `command_engine_system` (orders; the `ExecuteOrder` leaf's status) |
| `Think` | `think_system`: leaves start and stop here, so your `start`/`stop` fns run in `Think` |
| after `Think`, before `Actions` | `NpcInteractionSet::Begin`: `auto_door_system` |
| `Actions` | none: only content action systems (and nothing of the framework writes `ActiveLeaf` here) |
| after `Actions`, before `Locomotion` | `NpcVehicleSet::Drive` → `Gate`: `drive_vehicle_system` (the `DriveVehicle` leaf), `vehicle_gate_system` |
| `Locomotion` | `NpcMoverSet`: `straight_line_mover_system`, then (nav) `nav_plan_system` → `grid_path_mover_system` |
| after `Locomotion`, before `Effects` | `NpcVehicleSet::Crew` (`seat_follow_system`, `claim_seats_system`), `NpcInteractionSet::Complete` (`complete_interactions_system`, `key_replan_system`: the `Interact` leaf), `NpcCraftingSet::Complete` (`crafting_complete_system`: the `Craft` leaf) |
| after `Effects`, before `Cleanup` | `AgentIoSet::Advance`: `external_advance_system` (externally controlled units) |

The framework's leaf writers sit in their own sets, ordered against yours, so they never conflict with your
action systems. Only your own systems need the `.chain()`. A `MoveIntent` written in `Actions` (or by the order
engine) moves in the same tick's `Locomotion`. The order engine reads last tick's `MoveStatus`, so it notices an
order's arrival one tick later.

Smart objects are claimed in `Reservations` (`npc_jobs`, journaled `Reserve`/`Release`, dropped when either side
leaves the sim). A capacity-1 target (a bed): `free_for(&reservations, bed, me)`; a target with several slots (a
well with 3 buckets, a stockpile with `food` units): `free_slots(&reservations, well, capacity, me)` (slots left
for `me`: its own claim does not count) and `try_reserve_slot(world, me, well, capacity) -> Option<u32>` (claims
one, journaled; returns `me`'s slot index in claim order, `Reservations::slot_of`).

### 4.4 Conditions and trees

```rust
fn tired(world: &World, agent: Entity) -> bool {
    let (Some(reg), Some(needs)) = (world.get_resource::<NeedRegistry>(), world.get::<Needs>(agent)) else {
        return false;
    };
    reg.id(ALERTNESS.name).is_some_and(|id| needs.get(id).level == NeedLevel::Urgent)
}

fn sentry_tree(b: &mut TreeBuilder) {
    b.selector("Sentry", |b| {
        b.guard("Tired", "sentry_tired", Abort::LowerPriority, |b| {
            b.leaf("Rest", "SentryRest", 200);           // a typed leaf param (ParamValue::Int)
        });
        b.sequence("Watch", |b| {
            b.leaf("GoToPost", "SentryGoToPost", ());
            b.leaf("Look", "SentryRest", 40);
        });
    });
}
```

Builder nodes: `selector`, `sequence`, `utility(name, scorers, ..)` (a utility selector over registered scorers,
`fn(&World, Entity) -> i32`), `guard(name, cond, Abort::None | Abort::LowerPriority, child)` and
`leaf(name, action, param)` with `()`, an `i32`, an `(x, z)` pair, a `&str` name or a `ParamValue`. Trees name
actions and conditions; an unknown name panics at freeze time listing the known ones. **Guard rule:** an
`Abort::LowerPriority` condition must imply its child can start, otherwise every re-think aborts and restarts
the lower work. Units run a tree through `Brain { tree }` with `tree = AiRegistry::tree_id("sentry")` (after the
freeze); they keep their own `BrainState` and `Blackboard`.

### 4.5 Commands and spawning

External input is a `ContentCommand`: `{"type": NAME, ..fields}` on the wire, the same in `npcctl --commands`,
BRP `npc/command`, replays and agent sessions. Spawn sim entities with `spawn_sim`, which assigns the `SimId`.

```rust
use npc_framework::kernel::commands::check_pos;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnSentry { pub pos: (i32, i32) }

impl ContentCommand for SpawnSentry {
    const NAME: &'static str = "SpawnSentry";

    fn validate(&self, world: &World) -> Result<(), CommandError> {     // must never panic
        check_pos("pos", self.pos)?;
        if world.resource::<AiRegistry>().tree_id(SENTRY_TREE).is_none() {
            return Err(CommandError::NotReady { missing: "the sentry tree (SentryPlugin)".to_owned() });
        }
        Ok(())
    }

    fn apply(self, world: &mut World) {
        let Some(tree) = world.resource::<AiRegistry>().tree_id(SENTRY_TREE) else { return };
        let pos = SimPos::new(self.pos.0, self.pos.1);
        let needs = world.resource::<NeedRegistry>().full();
        spawn_sim(world, (pos, PrevSimPos(pos), Sentry { post: pos }, needs, Brain { tree }));
    }
}
```

Implement `fn unit(&self) -> Option<SimId>` when the command targets an existing unit (the journal entry's
unit). Built-in commands: `Despawn {unit}`, `SetNeed {unit, need, value}`, `Order {unit, order}` (the kernel's
`GoTo`/`Clear` orders), plus every capability's own (`IssueOrder`, `Interact`, `Craft`, `Mount`, ..).

### 4.6 The plugin

```rust
pub struct SentryPlugin;

impl Plugin for SentryPlugin {
    fn build(&self, app: &mut App) {
        app.register_need(ALERTNESS)
            .register_action("SentryRest", rest_start, rest_stop)
            .register_action("SentryGoToPost", go_start, go_stop)
            .register_cond("sentry_tired", tired)
            .register_tree(SENTRY_TREE, sentry_tree)
            .register_sim_component::<Sentry>()
            .register_sim_component::<Resting>()
            .register_sim_component::<GoingToPost>()
            .register_command::<SpawnSentry>()
            .register_kind_name("sentry", |w, e| w.get::<Sentry>(e).map(|_| "sentry"))
            .add_systems(NpcSimTick, (
                rest_system.in_set(NpcSet::Actions),
                arrival_system.in_set(NpcSet::Locomotion).after(NpcMoverSet),
            ));
    }
}
```

The whole `NpcAppExt` surface (`KernelAppExt` + `NeedsAppExt`):

| Method | Registers |
|---|---|
| `register_need(NeedDef)` | a need |
| `register_action(name, start, stop)` | a BT action (`StartFn = fn(&mut World, Entity, &ParamValue) -> Status`, `StopFn = fn(&mut World, Entity, StopReason)`) |
| `register_cond(name, fn(&World, Entity) -> bool)` | a BT condition |
| `register_scorer(name, fn(&World, Entity) -> i32)` | a utility scorer |
| `register_tree(name, fn(&mut TreeBuilder))` | a tree |
| `register_sim_component::<T>()` / `register_sim_component_with::<T>(json)` | a hashed, inspected component |
| `register_sim_resource::<R>()` | a hashed resource |
| `register_command::<C>()` | a content command |
| `register_kind_name(name, fn(&World, Entity) -> Option<&str>)` | the `kind` shown in inspection and `Spawn` |
| `on_sim_id_leave(name, hook)` | cleanup when a `SimId` despawns (e.g. release claims) |
| `add_freeze_hook(name, fn(&mut World) -> Vec<(String, String)>)` | a freeze step whose names join the content hash |
| `set_inspector(Inspector)` | replace the generic inspector (the village does) |

Each capability adds its own `*AppExt` (§6). Registration works before or after `NpcPlugins`, in any order;
after the freeze (`App::finish`, or `npc_framework::kernel::registry::ensure_frozen`) it panics. Every new or
renamed name changes the **content hash**, which replays carry, so a replay recorded before the change no
longer verifies: that is deliberate.

### 4.7 Scenarios and replays

There is no scenario format you must use. A headless app is three lines; the RTS demo's `headless_app` and the
village's `new_sim_app` are the pattern:

```rust
let mut app = App::new();
app.add_plugins((NpcPlugins::new().seed(seed).without_driver(), SentryPlugin));
app.finish();
app.cleanup();
spawn_my_map(app.world_mut());              // spawn_sim(..) after the freeze
run_ticks(app.world_mut(), 1000);
let hash = state_hash(app.world_mut());     // npc_framework::kernel::hash::state_hash
```

To get replays (`npc_kernel::replay::{record, verify}`), write an **app factory**
`fn(seed: u64, scenario: &str, units: u32) -> Result<App, String>` that builds the app, spawns the scenario and
inserts `ScenarioInfo { name, villagers }`; `replay::verify(&replay, factory)` re-runs a replay and compares
every checkpoint (`content/village_content/src/replay.rs`, `content/testbed_content/src/replay.rs`). To get
agent sessions (`npcctl serve`-style), register a scenario loader with `npc_agent_io` (§6.9).

## 5. The host owns schedule, tick and movement

### 5.1 Schedule and tick rate

`run_in(FixedUpdate)` runs the driver inside your fixed update at `tick_hz` ticks per second of *that
schedule's* time, from the driver's own accumulator: with `Time::<Fixed>::from_hz(60.0)` and `tick_hz(10)` the
sim ticks every sixth physics step and your 60 Hz stays 60 Hz (`host_fixed_timestep_is_untouched_in_demo`).
`run_in(Update)` ties it to frames (the village does). A lockstep or server host uses `.without_driver()` and
calls `run_ticks(world, 1)` when a turn's commands are complete.

Order your systems against the sim with the public sets: `NpcDriverSet` (the driver system in your schedule),
`NpcSet::*` and `NpcMoverSet` inside `NpcSimTick`. Ambiguity detection is `Error` on `NpcSimTick` in Strict mode,
so two of your sim systems that conflict must be ordered (`.chain()`, `.before()`).

### 5.2 Movement: the MoveIntent seam

Actions write `MoveIntent { goal, speed_mm_per_tick, tolerance_mm }` and wait for
`MoveStatus::{Idle, Moving, Arrived, Blocked { ticks }}`. A mover turns intents into motion:

- `StraightLineMover` (in `NpcPlugins`): straight at the intent's speed, integer steps, stops in front of
  blocking grid cells. Disable it with `NpcPlugins::new().build().disable::<StraightLineMover>()` if you move everything
  yourself.
- `NavPlugin` + `NavAgent` on a unit: grid A* (§6.1).
- **Your mover** (flow fields, a navmesh, Avian): put `ExternalMover` on the units it owns and run it in
  `NpcMoverSet`. In Strict mode it writes `SimPos` itself, in integers:

```rust
fn my_mover(mut q: Query<(&MoveIntent, &mut SimPos, &mut MoveStatus), With<ExternalMover>>) {
    for (intent, mut pos, mut status) in &mut q {
        *pos = pos.step_towards(intent.goal, intent.speed_mm_per_tick);
        *status = if pos.dist2(intent.goal) <= i64::from(intent.tolerance_mm).pow(2) {
            MoveStatus::Arrived
        } else {
            MoveStatus::Moving
        };
    }
}

app.add_systems(NpcSimTick, my_mover.in_set(NpcMoverSet));
```

`MoveIntent`/`MoveStatus` and the spatial grid are hashed only when a game or capability registers them
(`register_locomotion_hash`, `register_spatial_hash`; `npc_nav` does).

### 5.3 Relaxed mode: positions from your physics

When positions come from float physics (Avian, Rapier), Strict determinism is not possible for those units.
`NpcPlugins::new().determinism(DeterminismMode::Relaxed)` lets the host own them: write `HostPosition(SimPos)` on
the sim entity (from your physics, every frame) and the sim copies it into `SimPos` at the start of each tick
(`NpcSet::Inputs`). Ambiguities then only warn, and replays are only as deterministic as your physics. Keep Strict
for lockstep multiplayer; use Relaxed for a single-player game with physics-driven units (pinned by
`relaxed_mode_takes_host_positions` in `crates/npc_framework/tests/host.rs`).

### 5.4 The spatial grid

The kernel's `SpatialGrid` (insert your own before the plugins: `SpatialGrid::new(cell_mm,
GridBounds::centered(w, h))`; default 1 m cells, 128 × 128) indexes every `SimPos` by cell, holds static
`CellInfo` (blocking, occluding, tag bits, extra cost) merged with `GridBlocker(Footprint)` / `GridOccluder`
entities, answers sorted `in_radius` / `in_box` queries and traces segments. Walls, doors and line of sight all
read it. It is unhashed unless a capability registers it.

Footprint-aware queries, for walls and buildings whose centre says little about where they are:
`grid.footprint_area(pos, footprint)` (the `(min, max)` mm rectangle of the cells it covers),
`grid.cells_around(pos, footprint, ring)` (the cells exactly `ring` cells outside it, sorted),
`grid.nearest_free_cell_around(from, pos, footprint, max_ring)` (where a unit stands to work on it) and
`spatial::nearest_blocker_by_edge(world, from, max_mm, |world, entity| filter)` (the `GridBlocker` nearest by
edge distance, ties by `SimId`; `BlockerHit { id, entity, dist2, area }`).

## 6. Capabilities

Each capability is a feature, a `with_*()` switch (or its plugin), components you put on your entities, and
hooks registered by stable name. Without its plugin nothing of it runs or is hashed, so adding one changes the
content hash and your recorded replays (re-record them).

### 6.1 Grid pathfinding (`nav`)

```rust
app.add_plugins((NpcPlugins::new().with_command(), NavPlugin));
spawn_sim(world, (pos, NavAgent, /* .. */));   // NavAgent requires ExternalMover: nav owns its intents
```

Deterministic A* (costs 10/14, no corner cutting, ties by `(f, h, cell)`), line-of-sight smoothing, a per-tick
node budget (`NavBudget`, default 4000 nodes, requests served in `(tick, SimId)` order with carry-over),
replanning on `GridChanged` or a persistent `Blocked`. Journal `PathFound`/`PathFailed`; perf `nav`,
`nav.astar`; path gizmos with `view-helpers` (`nav_gizmos`). Hook: `register_pass_cost(tag_bit, fn(&World,
Entity, CellCoord, CellInfo) -> PassCost)` makes cells with a tag bit cost more, block, or carry a path marker
(interaction's doors use bit 0). Group moves reuse the leader's path shifted per formation slot.

### 6.2 Perception (`perception`)

```rust
pub const ENEMY: u32 = 1 << 0;   // your kind bits
pub const LOOT: u32 = 1 << 1;

spawn_sim(world, (
    SimPos::new(0, 0),
    Sensors::new().vision(20_000, 60).hearing(8_000).interest(ENEMY | LOOT), // 20 m, ±60°, 8 m hearing
    Facing::default(),
    Perceivable::new(ENEMY),
));
app.register_cond("sees_enemy", npc_framework::perception::conds::sees::<ENEMY>);
```

Vision (integer FOV against a cos² table, line of sight over occluding cells), proximity and hearing (`Noise`
stimuli from `emit_noise` / the `EmitNoise` command, no-ops without the plugin); scans staggered `(SimId + tick)
% PerceptionConfig::stagger`; a bounded `PerceptionMemory` per unit (forgets after N ticks, evicts the oldest);
`PerceptionFacts` mirrors the nearest percept into the blackboard; units face their movement unless
`FixedFacing`. Conditions `sees_kind`, `remembers_kind`, `heard_noise` test the unit's `interest` mask; register
`sees::<MASK>`, `remembers::<MASK>`, `heard::<MASK>` under your own names. Journal `Perceived`/`Lost`; inspect
`perception_json`; gizmos (cone, rays, memory) with `view-helpers`.

Hook (`PerceptionAppExt`): detection checks by name, `fn(&World, &Detection) -> Option<u16>` (confidence
permille, `None` = not detected); the default is `automatic`:

```rust
fn camouflage(world: &World, d: &Detection) -> Option<u16> {
    let hidden = d.target.is_some_and(|t| world.get::<Stealthy>(t).is_some());
    (!hidden || d.dist2 < 3_000_i64 * 3_000).then_some(1000)
}
app.register_detection_check("camouflage", camouflage).use_detection_check("camouflage");
```

With `rules` too, `with_per_roll_detection()` activates `per_roll` (§7).

### 6.3 Character rules (`rules`)

```rust
app.register_skill(SkillDef::new("Lockpicking", Stat::Dx, Difficulty::Hard).default_stat(Stat::Dx, -5))
    .register_trait(TraitDef::new("Nimble", 5).max_level(2).stat(Stat::Dx, 1))
    .register_sheet_template(
        SheetTemplate::new("thief").attributes(10, 13, 11, 10).skill("Lockpicking", 4).unspent(5),
    );

// a unit: `npc_framework::rules::template_sheet(world, "thief")` → a CharacterSheet component
fn pick_lock(world: &mut World, unit: SimId) -> bool {
    roll_skill(world, unit, "Lockpicking", -2, "thief.pick_lock").is_some_and(|r| r.success)
}
```

A point-buy `CharacterSheet` (ST/DX/IQ/HT, derived HP/Will/Per/FP/Basic Speed/Move, traits, skill points,
unspent points), skills/traits/templates by name (checked at freeze), 3d6 roll-under (`roll_skill`,
`roll_stat`, `roll_against`, `quick_contest`, `resisted_roll`) with dice keyed `(seed, unit, tick, purpose,
index)`: give every call site its own `purpose`. Every roll is journaled as `Roll`. `spend_points(world, unit,
&Purchase)` validates purchases. Hook: `set_rule_set(RuleSet)` swaps the cost tables, crit rules, default chain,
dice and resolution functions (a d20 system without touching callers). BRP `npc/sheet` with `remote`.

### 6.4 Interaction (`interaction`)

```rust
use npc_framework::interaction::Ctx;

fn can_pray(world: &World, ctx: &Ctx) -> Result<(), String> {
    if world.get::<Interactable>(ctx.target).is_some() { Ok(()) } else { Err("nothing to pray at".to_owned()) }
}
fn pray(_world: &mut World, _ctx: &Ctx) { /* the effect: sorted, integer, through commands or direct state */ }

app.register_item_kind(ItemKindDef::new("lockpick").weight_g(50).tag("tool:lockpick"))
    .register_interaction(
        InteractionDef::new("pray", can_pray, pray).duration(60).check("Lockpicking", 0).reach(2_000),
    );
```

Items (`Item { kind, stack, quality }` on the ground with `SimPos` or carried with `CarriedBy`), `Inventory`
(slots, weight), `Equipment` (`Equipment::hands()`), `Interactor` on actors, `Door { open, locked_by, key }`
(`Door::closed()`, `Door::locked("key:gate")`; a door cell blocks and occludes while closed, and nav plans
through doors the unit may open, opening them on the way), `ToolTarget` for "use a tool on it" (trees, ore).
Built-in verbs: `pick_up`, `drop`, `give`, `open`, `close`, `lock`, `unlock`, `equip`, `unequip`, `use_tool_on`
(`mount`/`dismount` are reserved for vehicles). One path for everything: the `Interact` leaf
(`ParamValue::Name(verb)`, target/item from blackboard slots 7/6), the `Interact {unit, verb, target, item?}`
command, doors on nav paths and orders all call `begin`. Builder: `.offer(fn)`, `.duration(t)` /
`.duration_fn`, `.check(skill, mod)` / `.check_fn`, `.reach(mm)` / `.no_reach()`, `.no_reservation()`,
`.noise(radius, kind)`, `.item(ItemArg)`, `.requires(fn)`. Precondition errors are the reasons an agent sees as
`why_not` and an order fails with. Hooks (`InteractionAppExt`): `override_interaction`,
`set_skill_check_hook`, `set_noise_hook`, `add_busy_check`, `add_finished_hook`. Journal `Interacted`,
`ItemMoved`, `DoorChanged`.

### 6.5 RTS command (`command`)

```rust
// on each unit
(Commandable::new(150), Owner(PlayerId(1)), NavAgent)        // speed mm/tick (+ arrive_mm), owner

// in its tree: orders win over everything below
b.selector("Soldier", |b| {
    b.guard("Orders", HAS_UNIT_ORDER, Abort::LowerPriority, |b| {
        b.leaf("ExecuteOrder", EXECUTE_ORDER_ACTION, ());
    });
    b.leaf("Idle", "SentryRest", 30);
});

// from your UI, AI player or network layer
let cmd = IssueOrder::new(PlayerId(1), units, UnitOrder::Move { pos: (10_000, 5_000) })
    .queued(false)                                                 // true = shift-queue
    .formation(Formation::new(FormationKind::Wedge, 1_500));
submit_order(world, &cmd)?;
```

Every order is the recorded `IssueOrder { player, units, order, queue, formation? }`, checked against `Owner`
on submit and again at its tick. `UnitOrder`: `Move`, `AttackMove` (a tagged move until combat exists), `Stop`,
`Hold`, `Follow { target }`, `Patrol { points }`, and the data-only `Interact { verb, target }`,
`Mount { vehicle }`, `Unload`, which need an executor (the glue registers them). Per-unit `OrderQueue`
(current + shift-queue; replacing journals `OrderFailed { reason: "replaced" }`), integer formations (`Line`,
`Box`, `Wedge`; blocked slots moved to the nearest free cell), squads (`Squad`, `MemberOf`, `CreateSquad`/
`DisbandSquad`, or `create_squad(world, player, &ids, formation)` for unrecorded setup). Units without a
`Brain` execute orders directly; units with one need the order branch above. Hooks (`CommandAppExt`):
`register_order_executor(kind, name, OrderExecutor { validate, start, tick, cancel })` and
`register_order_redirect(name, fn(&World, Entity) -> Option<Entity>)` (a `ByCrew` vehicle forwards to its
driver this way). Journal `OrderIssued/Started/Done/Failed`; inspect `command_json`.

**Your own orders.** `UnitOrder` is closed, but it has two open doors, both registered by name (content
hash entries `order_custom` / `order_verb`, so an app that registers none hashes as before):

```rust
// A new order kind: {"type":"Custom","name":"repair","args":{"wall":12}}
app.register_custom_order("repair", OrderExecutor::new(repair_start, repair_tick)
    .with_validate(repair_validate)          // per unit: only engineers
    .with_cancel(repair_cancel));
let order = UnitOrder::custom_with("repair", &RepairArgs { wall: 12 })?;   // args: any Serialize object
// in the executor: let args: RepairArgs = order.custom().unwrap().1.to_typed()?;

// Or a game verb on the Interact order, next to the interaction glue's verbs:
app.register_interact_verb_executor("repair", "my_game.repair", repair_executor());
```

The journal's `order` field is the custom order's `name` (`OrderDone { order: "repair" }`). A per-verb executor
is consulted before the `"Interact"` kind executor (the command × interaction glue's), so `Interact { verb:
"repair" }` runs yours and `open` still runs the glue's.

**Executor lifecycle** (`OrderExecutor`): `validate` runs **per unit** when the order is issued (on the unit after
redirects); `start` when the order starts and again when a paused order resumes; `tick` every engine pass;
`cancel` whenever it stops running: replaced, stopped, paused by the tree, *and* after `start`/`tick` returned
`Done` or `Failed`, so cleanup lives in one place. The engine (`NpcCommandSet::Execute`) runs after
`NpcSet::Sense` and before `NpcSet::Think`, one unit at a time in `SimId` order.

**Mixed selections.** One unit failing `validate` rejects the whole `IssueOrder`, unless it is issued with
`.skip_invalid(true)` (`"skip_invalid":true`): the order goes to the units that pass, each skipped unit is
journaled `OrderFailed { reason: "skipped: <why>" }`, and `submit_order_report(world, &cmd)` returns an
`IssueReceipt { tick, seq, units, skipped: [SkippedUnit { unit, reason }] }` up front (`cmd.partition(world)` without
submitting). It is still rejected when no unit can take it. The default (`false`) is not serialized, so recorded
commands are unchanged.

**Approach, then act.** `npc_command::approach` is the one "walk into reach" engine (the interaction glue's
`Interact` and `Mount` orders use it): an `ApproachSpec { reach, goal, tolerance_mm }` with `Reach::Point { point,
mm }` or `Reach::Area { min, max, mm }` (edge distance to a footprint), and `approach_step(world, unit, &spec)` →
`InReach` (walk stopped: act now) / `Walking` / `Blocked` (after `CommandConfig::block_fail_ticks`). It walks
through `MoveIntent` at the unit's `Commandable` speed, so a `NavAgent` path-finds. In an executor:

```rust
fn repair_tick(world: &mut World, unit: Entity, _: &UnitOrder) -> OrderStep {
    let spec = world.get::<Repairing>(unit).unwrap().approach;    // planned once in start:
    // ApproachSpec::to_footprint(grid, here, wall_pos, wall_footprint, 1_200, 300, 4)
    approach_then(world, unit, &spec, |world, unit| restore_hp(world, unit))
}
```

`ApproachSpec::to_point(point, reach_mm)` is the glue's (stop within 3/4 of the reach). Tuning is
`CommandConfig` (insert it before the plugin; hashed): `default_formation`, `follow_distance_mm`,
`follow_retarget_mm`, `block_fail_ticks`, `max_queue`, `max_group`, `max_patrol_points`, `relocate_radius_cells`,
`leader_wait_ticks` (each documented on the field).

### 6.6 Vehicles (`vehicles`)

```rust
let jeep = Vehicle::by_crew(120)                                   // orders go to its driver
    .seat(Seat::driver())
    .seat(Seat::passenger().offset(0, -700));
let tank = Vehicle::direct(80)                                     // it is a unit itself
    .seat(Seat::driver().optional())
    .seats(2, Seat::passenger());
spawn_sim(world, (pos, tank, Commandable::new(80), Owner(PlayerId(1)), NavAgent));
```

Seats (`Driver`, `Passenger`, `Gunner`; `required`, offsets), `MountedIn`/`Occupants` by `SimId`, mounting as
the interaction verbs `mount`/`dismount` and the commands `Mount {unit, vehicle, role?}`, `Dismount {unit}`,
`Unload {vehicle}` (unload puts crew on free cells next to the vehicle). Mounted units follow their seat; their
own `MoveIntent` is dropped; conditions `Mounted`, `MountedAsDriver`, `HasDriveGoal`. `ByCrew`: the driver's
`DriveVehicle` action steers the vehicle toward its `DriveGoal` (`set_drive_goal`, `command_vehicle_to`); no
driver → `VehicleStalled`. `Direct`: the vehicle moves through locomotion/nav like any unit, crew aboard. Journal
`Mounted`, `Dismounted`, `VehicleStalled`.

### 6.7 Progression (`progression`)

```rust
app.register_xp_source(XpSourceDef::new("kill").scale_pct(150));
// on a unit: Experience::default() (+ SpendPolicy::Ai, the default, or SpendPolicy::Player)
award_xp(world, unit, 50, "kill")?;
```

`Experience { xp, level }`; XP sources by name (`roll`, `craft`, `job`, `discovery` built in; roll XP comes from
the rules roll feed: successes of registered skills); a level curve (default triangular, base 100, 5 points per
level, max 50; `set_progression_config`); `SpendPolicy::Ai` spends at once on the most used skill, `Player`
banks points for `SpendPoints`. Learning: `grant_practice` (from rolls), `grant_study`, `grant_teaching`, with
caps (the `study`/`teach` verbs are glue, §7). Journal `XpGained`, `LevelUp`, `SkillImproved`.

### 6.8 Crafting (`crafting`)

```rust
app.register_skill(SkillDef::new("Smithing", Stat::Iq, Difficulty::Average).default_stat(Stat::Iq, -5))
    .register_item_kind(ItemKindDef::new("ore").stackable(10))
    .register_item_kind(ItemKindDef::new("ingot").stackable(10))
    .register_recipe(
        RecipeDef::new("ingot").input("ore", 2).station("forge").skill("Smithing", 0)
            .duration(80).output("ingot", 1).xp(15),
    )
    .add_craft_hook("my_game.on_craft", |world, done: &CraftCompleted| { /* done.quality, done.outputs */ });
spawn_sim(world, (SimPos::new(0, 0), Workstation::new(["forge"])));
```

Recipes are validated at freeze (an unknown skill or item kind panics with the known names). One path for the
`Craft` leaf, the `Craft {unit, recipe, workstation?}` command and orders: claim the workstation and the
inputs, roll the skill at the end (purpose `craft.<recipe>`), quality `min(margin / 2, 5)` (+2 on a critical
success); a failure keeps the inputs, a critical failure consumes them and damages the tools (`Durability`).
`CraftingOrders` + `PostCraftOrder` / `post_order` make a job board (`crafting_jobs`, `next_order_for`).
`CraftError` reasons double as `why_not`. Journal `CraftStarted`, `CraftDone`, `CraftFailed`.

### 6.9 Agent control (`agent-io`)

External agents (an LLM through `npcctl`, an MCP client, a script) attach to units and act through the same
commands as everything else; [agent-experiments.md](agent-experiments.md) is the user's guide. As a game you
register what your units show and can do:

```rust
use npc_framework::agent_io::{
    ActionPlan, ActionRequest, Affordance, AgentCtx, AgentIoAppExt, ControlMode, Rejection, Section, Step,
};
use serde_json::json;

fn sentry_section(world: &World, ctx: &AgentCtx) -> Option<Section> {
    let s = world.get::<Sentry>(ctx.entity)?;
    Some(Section::new(json!({"post": [s.post.x, s.post.z]}))
        .line(format!("Your post is at ({} m, {} m).", s.post.x / 1000, s.post.z / 1000)))
}

fn sentry_affordances(world: &World, ctx: &AgentCtx) -> Vec<Affordance> {
    if world.get::<Sentry>(ctx.entity).is_none() { return Vec::new(); }
    vec![Affordance::new("rest", "Rest a while").arg("ticks", "1..=1000 (default 100)").est_ticks(100)]
}

fn rest_handler(_world: &World, ctx: &AgentCtx, req: &ActionRequest) -> Result<ActionPlan, Rejection> {
    if ctx.mode() != ControlMode::Direct {
        return Err(Rejection::new("direct_only", "resting needs Direct control"));
    }
    let ticks = req.int_arg("ticks").unwrap_or(100).clamp(1, 1000);
    Ok(ActionPlan::Queue {
        label: format!("Rest {ticks} ticks"),
        steps: vec![Step::Action { action: "SentryRest".to_owned(), param: ParamValue::Int(ticks as i32) }],
        est_ticks: Some(ticks as u32),
    })
}

app.register_observation("sentry", sentry_section)
    .register_affordances("sentry", sentry_affordances)
    .register_action_handler("rest", rest_handler);
```

`AgentIoPlugin` adds the controller commands (`AttachController`, `DetachController`, `AgentAct`, `AgentCancel`,
`AgentClear`), injects a top-priority `ExternalControl` guard into **every** tree at freeze time (Direct mode runs
a queue of `Step`s that name your registered actions, so an agent-driven unit uses the same actions as an
autonomous one), the built-in `self` section and `wait` affordance, the event buffer and latency stats. Use
`Affordance::why_not(..)` for things that are visible but not possible; `ActionPlan::Commands` for
Advisor-mode plans (orders). `register_perception(name, fn)` makes observations egocentric
(`omniscient: false`; the perception glue does this). Serve it: windowed with feature `remote`
(`AgentRemotePlugin` on BRP, after `NpcRemotePlugin`), or headless with `npc_agent_io::session::Session` and a
scenario loader (`ScenarioLoaders::add("my_game", fn(&ScenarioDoc, seed) -> Result<LoadedScenario, String>)`;
`npcctl` registers the village's and the testbed's, `content/testbed_content/src/scenario.rs` is the pattern).

## 7. Glue: what happens when two capabilities meet

Capability crates depend on each other only along ADR 0012's whitelist (progression → rules, crafting →
interaction + rules, vehicles → interaction); everything else that needs two capabilities lives in
`npc_framework::glue::<a>_<b>`, compiled only when both features are on and added by `NpcPlugins` when both
`with_*` are on (ADR 0011):

| Glue plugin | When | What you get |
|---|---|---|
| `PerceptionRulesPlugin` | perception + rules | detection check `per_roll` (3d6 under Per + distance band + sense + target modifiers by name, e.g. the built-in `stealthy` reading `Stealthy { penalty }`; `PerRollConfig`; rolls journaled with purpose `perception.detect`), active with `with_per_roll_detection()`; hook `register_per_roll_modifier` |
| `PerceptionAgentPlugin` | perception + agent-io | observations show only what the unit perceives (+ memory), `omniscient: false` |
| `RulesAgentPlugin` | rules + agent-io | section `sheet`, affordance `spend_points`, the `SpendPoints` command |
| `InteractionRulesPlugin` | interaction + rules | interaction skill checks roll with `npc_rules` (purpose `interaction.<verb>`) |
| `InteractionPerceptionPlugin` | interaction + perception | door and tool noises become `Noise` stimuli |
| `InteractionAgentPlugin` | interaction + agent-io | an affordance (with `why_not`) and handler for every verb, section `inventory` |
| `CommandInteractionPlugin` | command + interaction | the `Interact` order (walk into reach, then the verb; fails with the verb's reason); with `view-helpers` the item/door context orders |
| `CommandVehiclesPlugin` | command + vehicles | `ByCrew` orders go to the driver, drivers drive (`Move`/`AttackMove`), `Mount` and `Unload` orders; with `view-helpers` vehicle → `Mount` |
| `CraftingInteractionPlugin` | crafting | no interaction starts while a unit crafts |
| `ProgressionCraftingPlugin` | progression + crafting | craft XP and practice |
| `ProgressionInteractionPlugin` | progression + interaction | verbs `study` (`Teaches` items) and `teach` (`Teacher` units) |
| `CommandAgentPlugin` | command + agent-io | section `command`, actions `order` (any owned units/squads, formations) and `execute_orders` |
| `VehiclesAgentPlugin` | vehicles + agent-io | `mount`/`dismount`/`drive_to`/`unload` affordances, the `DriveTo` command, section `vehicle` |
| `ProgressionAgentPlugin` | progression + agent-io | section `progression` |
| `CraftingAgentPlugin` | crafting + agent-io | `craft:<recipe>` affordances per nearby workstation, section `crafting` |

Each glue plugin registers names, so it changes the content hash only when both capabilities are on.

## 8. Determinism rules for your content

Your sim systems, actions, conditions, commands and hooks are sim code (`.claude/rules/sim.md` in this repo):

- Never read `Time`, `Time<Fixed>`, `Instant`, `SystemTime` or frame delta; read `SimClock { tick }`.
- No floats in sim state or decisions: positions are `SimPos` (mm, `i32`), speeds mm/tick, distances via
  `dist2` / integer square roots. Floats are fine in the view.
- No `HashMap`/`HashSet` for anything iterated: `BTreeMap`, `BTreeSet`, sorted `Vec`. Never act in query or
  `Entity` order when the result matters: collect, sort by `SimId`, act.
- Randomness: `npc_framework::kernel::rng::sim_rand(seed, sim_id, tick, purpose)` / `sim_range`, one `purpose`
  constant per call site; never a thread RNG.
- Change state from outside only by `SimCommand`; apply cross-entity effects sorted in `NpcSet::Effects`.
- Register every sim component and resource for the hash, with fixed-width encodings (no `usize`, no float bits,
  no `DefaultHasher`).
- Copy the repo's `clippy.toml` (disallowed types/methods) and deny `clippy::float_arithmetic`,
  `clippy::float_cmp`, `clippy::iter_over_hash_type` in your sim crate or module, as
  `apps/rts_units_demo/src/sim.rs` does.

Prove it with a test that runs the same seed twice and compares `state_hash`, and once with the journal on and
off.

## 9. View helpers and debug tooling in your app

### 9.1 View helpers (`view-helpers`)

The view reads the sim and never writes it: one visual entity per sim entity, linked by `VisualOf(sim_entity)`,
interpolated between ticks, animated from sim state. From `apps/rts_units_demo/src/view.rs`:

```rust
use npc_framework::view_helpers::{NpcViewHelpersPlugin, Selectable, VisualIndex, VisualOf, sim_to_world};
use npc_framework::view_helpers::interp::interp_alpha;

app.add_plugins(NpcViewHelpersPlugin);           // ViewSystems sets, VisualIndex, selection, F3/F4 panels, screenshots

fn spawn_visuals(mut commands: Commands, mut index: ResMut<VisualIndex>, units: Query<(Entity, &SimPos, &RtsUnit)>) {
    for (sim, pos, unit) in &units {
        if index.map.contains_key(&sim) { continue; }
        let root = commands.spawn((VisualOf(sim), Selectable, Transform::from_translation(sim_to_world(*pos)),
                                   Visibility::default())).id();
        index.map.insert(sim, root);
    }
}
// move_visuals: lerp PrevSimPos → SimPos by interp_alpha(Some(&sim_time), Some(&rate), Some(&control))
```

- `ViewSystems::{Input, Sync, Animate, Present}` chained in `Update`; `DebugPanels { journal, perf }` toggles the
  journal (F3 in the demos) and perf (F4) panels; `ScreenshotRequest { path, after_secs }` saves a PNG and exits;
  `Selected` is the view-side selection.
- Gizmos per capability: `nav_gizmos` (paths), `perception_gizmos` (cones, rays, memory), `vehicle_gizmos` (crew).
- With `command`: `command_ui::CommandUiPlugin`, the RTS command UI (not part of `NpcViewHelpersPlugin`; add
  it). Click/box selection of the UI player's units (Shift adds), Ctrl+1–9 / 1–9 control groups, right-click
  orders for the selection (Shift queues), selection rings and order-chain gizmos. It only emits `IssueOrder`
  commands, so any part of it can be replaced. Bindings are data (`CommandBindings`); `CommandUi { player,
  enabled, gizmos }` says whom it commands.

The **right-click order kind** comes from `RightClickMode`, which your hotkeys set (the library binds none):
`RightClickKind::Move` (default: the context order: ground → `Move`, a unit → the first `ContextOrders` resolver
that answers, the glue registering item → `pick_up`, door → `open`/`unlock`, vehicle → `Mount`; else `Follow`),
`AttackMove`, `Patrol` (Shift+click collects points in `PatrolDraft`, a plain click issues them) or
`Custom(name)` (your `RightClickOrders` resolver, `fn(&World, &RightClick) -> Option<UnitOrder>`). Set it every
frame while a key is held (`RightClickMode::held(kind)`) or for one order (`RightClickMode::once(kind)`), in a
system ordered `.before(NpcCommandUiSet)`.

Register resolvers with `CommandUiAppExt`, in any plugin order (before or after `CommandUiPlugin`):
`app.add_right_click_order(name, f)` / `app.add_context_order(name, f)`, or the `_subset` variants whose resolver
also picks **which selected units** take the order: `fn(&World, &RightClick) -> Option<ClickOrder>` (context:
`fn(&World, &RightClick, Entity) -> Option<ClickOrder>`), `ClickOrder::only(units, order)` or `ClickOrder::all(order)`
(the subset is intersected with the selection). Right-click orders go out with `skip_invalid` (resource
`SkipInvalidOrders`, default `true`), so units an executor refuses are skipped (logged), not the whole order;
`issue_to_units(world, &units, order, queue, skip_invalid)` is the scripted form. Put `PickTarget` on the visual
root of a wall, building or resource to make it a right-click target (`pick_unit`, `RightClick::target`) that
selection never picks. From `apps/rts_units_demo/src/view.rs`:

```rust
use npc_framework::view_helpers::command_ui::{
    CommandFormation, CommandUiPlugin, NpcCommandUiSet, RightClickKind, RightClickMode, issue_to_selection,
};

app.add_plugins((NpcViewHelpersPlugin, CommandUiPlugin))
    .add_systems(Update, hotkeys.in_set(ViewSystems::Input).before(NpcCommandUiSet));

fn hotkeys(keys: Res<ButtonInput<KeyCode>>, mut right_click: ResMut<RightClickMode>,
           mut formation: ResMut<CommandFormation>) {
    let kind = if keys.pressed(KeyCode::KeyA) { RightClickKind::AttackMove }
        else if keys.pressed(KeyCode::KeyP) { RightClickKind::Patrol }
        else { RightClickKind::Move };
    if right_click.kind != kind { *right_click = RightClickMode::held(kind); }
    if keys.just_pressed(KeyCode::KeyZ) { formation.0 = Some(Formation::new(FormationKind::Line, 1500)); }
}
```

Helpers for hosts with their own input, scripts or tutorials: `pick_unit(world, ray)` (the unit under a ray, the
same picker clicks use; the raw `view_helpers::pick` is public too), `select_units` / `select_in_rect`
(scripted selection; set `DragBox` to draw a marquee), `unit_screen_positions`, `issue_to_selection(world,
order, queue)` (S / H keys: `Stop`, `Hold`), and the pure `click_select`, `box_select`, `store_group`,
`recall_group`, `context_order`, `right_click_order`, `order_command(_with)`, `ground_point`, `world_to_sim`.

Keys, the camera and your HUD belong to your app. Never insert `Transform`, meshes or picking components on sim
entities. Adding a windowed host app *inside this workspace* has one extra step (`deny.toml`'s `bevy` ban):
[adding-a-host-app.md](adding-a-host-app.md).

### 9.2 The profiler

With `perf` + `view-helpers`, two calls wire the profiler into a windowed app (`apps/rts_units_demo/src/main.rs`):

```rust
app.add_plugins(DefaultPlugins.set(npc_framework::perf::log_plugin()))   // LogPlugin + the profiler layer
    .add_plugins(
        npc_framework::perf::NpcPerfPlugin::new()                           // tags, budgets, PerfHandle, end_frame
            .tag("my_game::sim", "game.sim")
            .tag("my_game::view", "game.view"),
    );
```

`log_plugin()` is Bevy's `LogPlugin` with the process-wide `PerfCollector` as its `custom_layer` and the
`npc=debug` filter (Bevy's `trace` feature must be on for per-system spans: `bevy/trace`). `NpcPerfPlugin` sets
`MechanicTags::default_npc()` plus yours (`.tag(prefix, mechanic)`, `.tags_with(|t| ..)`), the budgets
(`PerfBudgets::default_npc().scaled_for_profile()` unless `.budgets(..)`; `.budget(mechanic, fixed_us,
per_agent_us)` adds a host row), inserts `PerfHandle` (the F4 panel and BRP `npc/perf` read it) and adds
`end_frame_system` in `Last` (`.end_frame(false)` if you close frames yourself). A breach logs one `warn!` per
mechanic, then `debug!` (`.breach_log(BreachLog::Warn)` warns every time; the testbed's `--perf-warn`).

Add `log_plugin()` **before** any plugin that adds sim systems: Bevy creates a system's span when the system is
added, and a span created before the subscriber exists is never timed. A host that must build the sim first (the
testbed builds its scenario app, then the window) calls `perf::install_log_plugin_now()` at the start of `main` and
adds `DefaultPlugins.disable::<LogPlugin>()`.

Content gives its own mechanics a budget without touching `npc_perf`: `app.register_budget("my_game.dig", 10, 2)`
(`perf::PerfAppExt`, in the prelude with `perf`: µs at one agent plus µs per agent; any plugin order). The rows land
in the app's `PerfBudgetTable`; `NpcPerfPlugin` adds them (at one agent, scaled for the profile) in
`Plugin::finish`, and `npcctl bench` uses `PerfBudgets::default_npc_for_world(world, agents)`. The village
registers its `actions.eat/drink/work/sleep/idle` rows this way (`village_content::VILLAGE_BUDGETS`).
Headless hosts call `npc_perf::install_global` before building the app and `PerfCollector::end_frame` per tick
(`apps/npcctl`'s `bench`). Mechanics and budgets: [debugging.md §7](debugging.md#7-performance).

### 9.3 BRP, journal, replays, CLI

- `remote`: add `npc_framework::tools::remote::NpcRemotePlugin` (and with `agent-io` the
  `npc_framework::agent_io::remote::AgentRemotePlugin` after it) in debug builds; it serves `npc/*` on
  `127.0.0.1:15702` (see `apps/village/src/main.rs`). Add your own methods with `RemoteAppExt::add_remote_method`.
  With `view-helpers` too, `npc_framework::view_helpers::remote` does the windowed setup in one call:
  `RemoteArgs` (the `--remote`/`--no-remote`/`--remote-port`/`--out-dir` flags; pass `cfg!(debug_assertions)` so
  release builds stay off unless asked), `install_remote` (`NpcRemotePlugin`, `npc/screenshot`, the output root)
  and `record_from_startup`. BRP has no Origin/Host checks (`bevy_remote` 0.20 has no request hook), so keep it
  a development tool; any method of yours that takes a file path should go through `npc_tools::out_path`
  (`OutputRoot::file`), as `npc/record/stop` and `npc/screenshot` do ([debugging.md §3.2](debugging.md#32-calling-brp)).
- The journal: `JournalPlugin::default()` (or `{ verbose: true, .. }`), or `set_journal(world, on, verbose)` at
  runtime, filtered by `JournalFilter`; content effects as `JournalKind::Effect` JSON:
  `record_effect(world, Some(unit), "WallRepaired", &WallRepaired { wall, hp })` journals
  `{"type":"WallRepaired","wall":..,"hp":..}` (field order kept), and `JournalKind::effect(kind, &payload)` builds
  the entry for a system holding `ResMut<Journal>`. `JournalKind`, `JsonObject`, `set_journal` and
  `record_effect` are in the prelude.
- Replays: `npc_framework::kernel::replay::{record, verify, Recorder}` with your app factory (§4.7).
- `npcctl` knows only the village and the testbed; for your game, write a small headless binary (the RTS demo's
  `--headless-ticks N` prints arrived units, ticks and the state hash) or add your factory to a fork of
  `apps/npcctl/src/sim.rs`. Everything else in [debugging.md](debugging.md) applies unchanged.

## 10. Testing and debugging your integration

- **Headless scenario tests** first: build the app without a driver, spawn, `run_ticks`, assert on the journal
  (`LeafStart` of your action, `OrderDone`, `NeedLevel`) and on state. See `apps/rts_units_demo/tests/demo.rs`
  (all 200 units arrive by tick 1500; two runs give the same summary and hash) and
  `content/testbed_content/tests/`.
- **Determinism:** same seed twice → same `state_hash`; journal on vs off → same hash; a different seed → a
  different hash.
- **Host coexistence:** assert your `Time<Fixed>` is unchanged after the plugins build
  (`host_fixed_timestep_is_untouched_in_demo`).
- **Why did a unit do that:** the journal and the inspector (`npc_framework::kernel::inspect::unit_json`), or
  BRP `npc/journal` / `npc/unit` on the running game; recipes in [debugging.md §9](debugging.md#9-recipes).

What the RTS demo, the first foreign host, ran into, and where it stands (README "Portability"): modifier
orders on right click are now `RightClickMode` (§9.1); the unit picker is public (`pick_unit`, `pick`); scripted
selection and its marquee are `select_units` + `DragBox`; the profiler is `log_plugin()` + `NpcPerfPlugin`
(§9.2); `create_squad`, `state_hash` and `content_hash` are in the prelude; `deny.toml`'s `bevy` ban still lists
the apps of *this* workspace that may depend on `bevy` ([adding-a-host-app.md](adding-a-host-app.md)).

## 11. Versions

| npc_framework | Bevy | Rust | Notes |
|---|---|---|---|
| 0.1.x (unreleased, this repository) | 0.20 | ≥ 1.97.1 (CI: stable) | facade API may still change; `CHANGELOG.md` lists every addition |

The project follows Semantic Versioning once the facade is published. A Bevy upgrade is a framework minor
version; the replay format carries its own `version` (2, with the content hash; version 1 files still load) and
every replay carries the content hash, so recorded runs say which content they belong to.
