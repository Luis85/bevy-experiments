---
name: add-unit-type
description: Add a new NPC/unit type (e.g. an engineer, a soldier squad member, a trader), or a new behaviour for an existing unit type (an RTS engineer that repairs walls on its own or on order), by composing needs, a behaviour tree, actions, order executors and opt-in capabilities in content, without touching core crates. Use when asked to create a new kind of agent, or teach an existing one something, for the village, the Actor Testbed or the RTS demo.
argument-hint: "<unit type or behaviour> [game: village|testbed|rts]"
---

# Add a unit type or behaviour: $ARGUMENTS

A unit type is **content**: a marker component, a tree, the actions/conditions it uses, its needs, the
capability components it carries, and a spawn command or scenario entry. A new behaviour of an existing type is
the same pieces: a branch in its tree and/or an order executor. Rules: `.claude/rules/content.md` and
`.claude/rules/sim.md`. API guide: `docs/using-npc-framework.md` (§4.3 action systems and sets, §6.5 orders).

## 1. Decide where it lives

| Game | Where | Pattern to copy |
|---|---|---|
| village | `content/village_content/src/<unit>.rs`, its own `<Unit>Plugin` next to `VillagePlugin` | `villager.rs` (tree, actions, conds), `commands.rs` (`SpawnVillager`) |
| testbed | `content/testbed_content/src/`: a sheet template (`content.rs`), a `Behaviour` + tree (`tree.rs`), a scenario actor (`scenario.rs`, `layout::spawn_unit`) | `TestbedPlugin`, `scenarios/*.json` |
| rts | `apps/rts_units_demo/src/sim.rs`: a `UnitKind` variant, a tree name, spawn in `spawn_world` | `RtsContentPlugin` (the `has_unit_order` → `ExecuteOrder` branch over `RtsIdle`) |

Depend only on `npc_framework`. Core crates (`npc_ai`, kernel/needs/jobs/locomotion/nav, the capability crates,
`npc_perf`, tools, facade, view helpers) are **off limits**. If the API cannot express the unit, stop and
propose the core change separately.

## 2. Tree branch or order executor?

| The unit should ... | Use |
|---|---|
| decide by itself (sees a damaged wall, gets hungry, it is night) | a **tree branch**: a guard condition + an action leaf |
| do it when a player, AI player or script says so (right-click "repair that wall") | an **order executor**: `UnitOrder::Custom { name, args }` with `register_custom_order(name, OrderExecutor)`, or a game verb on `Interact` with `register_interact_verb_executor(verb, name, ..)` |
| both | both, sharing one helper (the walk-into-reach + act step: `npc_command::approach_then`) |

The tree decides *what* runs. Orders reach the tree through its order branch (`has_unit_order` → the
`ExecuteOrder` leaf), and the executor then drives that order. `OrderExecutor` lifecycle (§6.5 in the guide):

- `validate(world, unit, order)` runs **per unit**, on the unit after redirects, when the `IssueOrder` is
  submitted and again at its tick. One refusal rejects the **whole command** for every unit, unless it is issued
  with `.skip_invalid(true)`: then refused units are skipped and journaled `OrderFailed { reason: "skipped: .." }`.
  `command_ui` right-click orders use `skip_invalid` by default.
- `start` runs when the order starts, and again when a paused order resumes.
- `tick` runs on every engine pass while the order runs. Return `Running`, `Done` or `Failed(reason)` (journal
  `OrderDone` / `OrderFailed`).
- `cancel` runs **whenever the order stops running**: replaced, stopped, paused by the tree (a more urgent branch
  won) **and** after `start`/`tick` returned `Done` or `Failed`. Put all cleanup there (release claims, clear the
  `MoveIntent`, remove marker components).
- The engine (`NpcCommandSet::Execute`) runs **after `NpcSet::Sense`, before `NpcSet::Think`**, one unit at a time
  in `SimId` order. It writes `MoveIntent`, which moves in the same tick, and it reads last tick's `MoveStatus`, so
  arrival is noticed one tick later. An executor never issues orders to its own unit.

## 3. Test first

- Scenario test: spawn one unit of the new type (plus what it interacts with), run N ticks headless, assert it
  starts its key action (journal `LeafStart`, `OrderStarted`, `Interacted`) and finishes it (state change).
- RTS demo: copy `apps/rts_units_demo/tests/small_scenario.rs`. It has `small_app(seed)` (the demo's sim, no
  driver), `spawn_unit(&mut app, kind, player, pos)` (one unit as `spawn_world` makes it, no squad),
  `run_until`, and `journal_kinds`. Village: `content/village_content/tests/common/mod.rs`. Testbed:
  `try_new_sim_app_with(seed, name, ..)` + `TestbedIds::id(name)`.
- Look ids up, never hard-code them: `TestbedIds`, `VillageIds`, the `SimId` that `spawn_sim` returned, or
  `next_sim_id(&app)` for a runtime spawn.
- An order test also checks the refusal path: another player's unit, or a unit `validate` refuses, with and
  without `skip_invalid`.
- A determinism check: two runs with the same seed give the same `state_hash`.
- Inner loop: `cargo test -p <crate> --test <file>` (seconds to minutes). The full `cargo xtask verify` takes
  40–50 min, so run it once at the end.

## 4. Compose

- [ ] Marker component (`#[derive(Component, Serialize, Deserialize, Reflect)]`), hashed with `impl SimHash` +
      `register_sim_component::<T>()`; a kind name via `register_kind_name`.
- [ ] Needs: reuse registered needs where they fit; new ones as in `/add-mechanic`.
- [ ] Actions: reuse registered ones by name (village `MoveToDest`, `IdleFor`, `PickWanderPoint`; testbed `TbIdle`,
      `TbWalk`; the capability leaves `ExecuteOrder`, `Interact`, `Craft`, `DriveVehicle`). Add unit-specific
      ones as `SparseSet` components + one system each in `NpcSet::Actions`. **Several action systems go into one
      `.chain()`**: they all write `ActiveLeaf`, and an unordered pair is an ambiguity error. In the RTS demo, that
      means `(idle_system, repair_system).chain().in_set(NpcSet::Actions)`. No framework system writes `ActiveLeaf`
      inside `NpcSet::Actions`; the guide's §4.3 table lists what runs around it.
- [ ] Tree: `app.register_tree("<unit>", build)` with `selector`/`sequence`/`guard`/`leaf`, actions and
      conditions by name. RTS-commandable units put the order branch **first**, then the unit's own
      (autonomous) branches, then idle:

      ```rust
      b.selector("Engineer", |b| {
          b.guard("Orders", HAS_UNIT_ORDER, Abort::LowerPriority, |b| {     // 1. orders win
              b.leaf("ExecuteOrder", EXECUTE_ORDER_ACTION, ());
          });
          b.guard("AutoRepair", "rts.wall_needs_repair", Abort::LowerPriority, |b| {   // 2. on its own
              b.leaf("Repair", "RtsRepair", ());
          });
          b.leaf("Idle", RTS_IDLE, 30);                                      // 3. otherwise
      });
      ```

      An incoming order aborts the autonomous branch (the order guard is higher), and pausing an order for a
      more urgent branch calls the executor's `cancel`. The guard rule applies: an `Abort::LowerPriority`
      condition implies its child can start, so `wall_needs_repair` must check what `RtsRepair`'s `start` needs
      (a reachable damaged wall).
- [ ] Capabilities it carries (only those the app enables): `Commandable` + `Owner(PlayerId)` (+ `NavAgent`) for
      orders; `Sensors` + `Facing` + `Perceivable` for perception; a `CharacterSheet` (`template_sheet(world,
      "<template>")`) + `Experience` + `SpendPolicy` for rules/progression; `Interactor` + `Inventory` +
      `Equipment` for interaction. `testbed_content::layout::spawn_unit` shows them all together.
- [ ] Orders (if commanded): an executor (§2) registered by name, plus a right-click/context resolver in the view
      if a player issues it (`CommandUiAppExt::add_right_click_order`, `_subset` to order only some of the
      selection; `PickTarget` on non-unit targets such as walls).
- [ ] Spawn: a `ContentCommand` with a stable `NAME` (`register_command::<C>()`) and/or a scenario entry; ids come
      from `spawn_sim`, never from `Entity`. Spawn new scenario entities after the existing ones (SimId
      stability, `.claude/rules/content.md`).
- [ ] Perf: tag the new systems (`perf_tags` / the host's `MechanicTags`) and register a budget
      (`app.register_budget(mechanic, fixed_us, per_agent_us)`), as in `/add-mechanic` step 3.
- [ ] Agents (optional, with `agent-io`): an observation section / affordances for it (`register_observation`,
      `register_affordances`, `register_action_handler`).
- [ ] View (optional): a visual linked by `VisualOf`, animation from sim state (`.claude/rules/view.md`; testbed
      `content/testbed_view`, RTS `apps/rts_units_demo/src/view.rs`).

## 5. Add a behaviour to an existing unit type

1. Find its tree (`register_tree` in the content plugin; the RTS demo's `engineer_tree`) and the branch the new
   behaviour must beat or yield to.
2. Choose tree branch vs executor (§2). For a tree branch, insert the guard at the right priority, between the
   order branch and idle for an RTS unit. For an executor, register it by name and keep the tree as is: the order
   branch already runs `ExecuteOrder`.
3. Add the action component + system to the content's existing `.chain()` in `NpcSet::Actions`, and the
   condition by name.
4. Test the new behaviour on a small scenario (§3), and also test that the existing behaviour still wins where it
   should (an order still interrupts it; idle still runs when there is nothing to do).
5. A new registered name changes that content's hash. For the village or testbed, that means a staged re-baseline
   of its replays (`docs/rebaselining.md`). The RTS demo has no corpus: its tests and the summary just change.

## 6. Check

```sh
cargo test -p <crate> --test <unit>
cargo run -p npcctl -- journal --seed 1 --ticks 4000 --unit <id> --kind LeafStart,Abort --limit 40        # village
cargo run -p npcctl -- journal --scenario testbed --ticks 2400 --unit <id> --kind LeafStart,OrderDone --limit 40
cargo run -p rts_units_demo -- --headless-ticks 1500                                                     # rts: JSON summary
git diff --stat            # must show no files under crates/
```

- Existing replays and goldens stay unchanged unless the user approved a re-baseline. A new name in the testbed
  changes the testbed's content hash, so `tests/replays/testbed_*` fail: say so before. Follow
  `docs/rebaselining.md`: stage, show evidence, ask; the user swaps.

## 7. Gate

- [ ] `CHANGELOG.md` line; README/docs if users will spawn it.
- [ ] `/verify-visual` if it has a visual.
- [ ] `cargo xtask verify` → `"ok":true`. Quote the result.
