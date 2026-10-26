# testbed_content: the Actor Testbed

The Actor Testbed (actor-capabilities spec §4.9) is a small walled compound where every opt-in capability is
switched on: perception (Per-roll detection), rules, agent control, interaction, RTS command, vehicles,
progression and crafting. It serves as the showcase and as the integration test. All of its content lives in this
crate and is registered by stable name; no core crate is touched.

```rust
let mut app = testbed_content::new_sim_app(1, "testbed"); // or try_new_sim_app(seed, name, 0)
npc_framework::prelude::run_ticks(app.world_mut(), 600);
```

Headless session with agents. `--scenario` takes a file path or the name of an embedded scenario:

```sh
cargo run -p npcctl -- serve --scenario testbed --port 15702
cargo run -p npcctl -- serve --scenario content/testbed_content/scenarios/doors.json
cargo run -p npcctl -- agent observe --unit 38 --format text
```

The other npcctl subcommands take the testbed too: `--scenario testbed` (the showcase), `testbed:<name>` (an
embedded scenario file, e.g. `testbed:smithy`) or a scenario file path. Replays name their scenario
`testbed[:<name>]` and are rebuilt by `testbed_content::replay::try_new_replay_app` (the testbed's replay
`AppFactory`); a scenario file is recorded as a session record.

```sh
cargo run -p npcctl -- run --scenario testbed --ticks 4800 --pretty      # + content_summary (units, doors, items)
cargo run -p npcctl -- inspect --scenario testbed:smithy --tick 100 --unit 33
cargo run -p npcctl -- journal --scenario testbed:smithy --ticks 600 --kind OrderDone,Interacted
cargo run -p npcctl -- replay record --scenario testbed:vehicles --ticks 1200 --commands cmds.json --out r.json
cargo run -p npcctl -- replay verify tests/replays/testbed_orders_craft.json
cargo run -p npcctl -- diverge --scenario testbed --seed-a 1 --seed-b 2 --ticks 600
cargo run --release -p npcctl -- bench --scenario testbed --ticks 2000
```

## Layout

The map uses 1 m cells, and +z (north) is up. To print it:
`cargo test -p testbed_content --test scenarios print_testbed -- --ignored --nocapture`.

```
       x: -13 ........ 0 ........ 12 ...... 20
  10 .#########################........
   9 .#...........#...........#........
   8 .#.........F.#.....h..C..#........     workshop (W, A, F, book) | store (pickaxe, hammer, scroll, chest)
   7 .#..W....A...#...p.......#........
   6 .#...........#...........#........
   5 .#....b......#.......s...#........
   4 .#...........#...........#........
   3 .######D###########L######........     workshop door (closed) / store door (locked, key:store)
   2 .#.......................#.....2..
   1 .#.......................#........
   0 .#.....1...1.............#...2....     faction 1 (player 1) inside, faction 2 (player 2) east, outside
  -1 .#..............1........#........
  -2 .#...................k...#.....2..
  -3 .#..T....................#........
  -4 .#....1.......f..........#...2....
  -5 .#.......x...............#........
  -6 .#...............c.......#........     c = ByCrew cart
  -7 .#..T.................O..#........     T = trees (tool:axe -> logs), O = ore rock (tool:pickaxe -> ore)
  -8 .#.....T..l..............#........
  -9 .#.......................#........
 -10 .############D############........     gate (closed, unlocked)
 ...
 -15 ........T....V....................     V = Direct armored wagon, two more trees
 -16 ...................T..............
```

Legend:

- Structure: `#` wall, `D` closed door, `L` locked door, `/` open door.
- Workstations: `W` workbench, `A` anvil, `F` forge. `C` is a chest.
- Items: `x` axe, `p` pickaxe, `h` hammer, `k` store key, `b` book, `s` scroll, `f` bread, `l` logs.
- Actors: `1`/`2` show the owner.

The cell `(x, z)` has its centre at `(x·1000+500, z·1000+500)` mm (`layout::cell`). Each straight wall run is one
sim entity: `Prop { Wall }`, plus a `GridBlocker` and a `GridOccluder` sized to the run's footprint. Doors are
`Door` entities in one-cell gaps. `TestbedIds` maps names to `SimId`s. Some names are fixed: `gate`,
`workshop_door`, `store_door`, `workbench`, `anvil`, `forge`, `chest`, `book`, `pickaxe`, `hammer`, `scroll`,
`tree_0`..`tree_4`, `ore_rock`, `axe`, `store_key`, `bread`, `logs`, `cart`, `wagon`. Names given in the scenario
are added to these.

### For the view (wave G2)

- The `Prop.kind` of an entity picks its model: `wall`, `doorway`, `tree`, `ore_rock`, `workbench`, `anvil`,
  `forge`, `chest`, `cart`, `wagon`.
- `Prop.yaw_deg` is 0 for runs along x and 90 for runs along z.
- Items use `Item.kind`; doors use `Door.open`; actors use `TestbedUnit` and `Owner`, with faction 1 =
  adventurers and 2 = skeletons.
- Models and their measured scales are in `assets/testbed/` and `docs/_notes/testbed-assets-measured.md`.

## Registered content

- **Item kinds:** `axe`, `pickaxe`, `hammer` (tools `tool:*`, main hand), `log`, `plank`, `ore`, `ingot`
  (stackable materials), `bread` (food), `book`, `scroll`, `store_key` (`key:store`). Weights and tags are in
  `src/content.rs`.
- **Learning** (`content::TEACHES` / `TEACHERS`, the progression × interaction glue): every `book` item
  `Teaches { Carpentry, max 12, 200 ticks }` and every `scroll` `Teaches { Smithing, max 12, 200 ticks }` (the
  `study` verb: one point per session, capped by the book and at the attribute + 4); units from the `smith`
  template are a `Teacher { [Smithing], 100 ticks }` (the `teach` verb; the teacher must be at least the
  student's level + 1). Mined ore and chopped logs land on the ground next to the target (pick them up).
- **Skills:**

  | Skill | Attribute | Difficulty | Default |
  |---|---|---|---|
  | `Carpentry` | DX | Average | DX−5 |
  | `Smithing` | IQ | Average | IQ−5 |
  | `Woodcutting` | ST | Easy | ST−4 |
  | `Mining` | ST | Average | ST−5 |
  | `Stealth` | DX | Average | DX−5 |

- **Traits:** `Keen Eyes` (+1 Per per level) and `Clumsy` (−1 DX).
- **Sheet templates:** `carpenter`, `smith`, `woodcutter`, `scout`, `skeleton_warrior`, `skeleton_minion`,
  `novice`. Each has a different spread of attributes and skills.
- **Recipes:**

  | Recipe | Workstation | Inputs | Tool | Skill | Output |
  |---|---|---|---|---|---|
  | `planks` | workbench | log 1 | | Carpentry | plank 2 |
  | `ingot` | forge | ore 2 | | Smithing | ingot 1 |
  | `forge_axe` | anvil | ingot 1 + plank 1 | hammer | Smithing −2 | |
  | `forge_pickaxe` | anvil | ingot 2 + plank 1 | hammer | Smithing −3 | |

- **Trees:** `testbed_idle`, `testbed_wander`, `testbed_worker`. They are documented in `src/tree.rs`. All of them
  contain the order branch `has_unit_order` → `ExecuteOrder`, the `HasDriveGoal` → `DriveVehicle` branch and a
  `Mounted` branch. The worker tree also chops and mines with an equipped tool.
- **Actions and conditions:** `TbIdle`, `TbWalk`, `TbPickWander`, `TbFindToolWork`, `tb_has_tool_work`.
- **Sim components:** `testbed_unit`, `testbed_prop`, `tb_idling`, `tb_walking`.
- **Command:** `TestbedDrive { unit, pos }`. It sets the drive goal of a seated driver.
- **Observation section:** `testbed`.
- **Default actors:** each has `Sensors` (15 m vision cone at ±60°, 10 m hearing), a `CharacterSheet` from its
  template, `Experience`, an `Inventory` (8 slots, 30 kg), `Equipment` (both hands), `Interactor`, `Commandable`,
  `Owner` and `NavAgent`.
- **Vehicles:** the cart and the wagon both carry `Commandable`, `Owner(1)` and `NavAgent`.

## Scenario files (`scenarios/`, embedded as `SCENARIO_FILES`)

Every file has `"content": "testbed"`. The schema is in `src/scenario.rs`. The `world` object takes these keys:
`layout` (`compound`, `open` or `frontier`), `furnish`, `store_locked`, `detection` (`per_roll` or `automatic`), `stagger`, `items` and
`open_doors`. Actors are `{"kind":"unit","template":..}` objects with optional `name`, `owner`, `behaviour`,
`facing`, `fixed_facing`, `equip`, `carry`, `spend_policy`, `wander_radius_mm`, `speed_mm_per_tick` and
`stealthy`.

| File | What it sets up |
|---|---|
| `testbed.json` | The showcase: 8 actors (4 per faction), wandering, a woodcutter at work, Per-roll detection. |
| `doors.json` | An observer facing a book behind the closed workshop door. |
| `locked_store.json` | A keeper near the store key and a warden at the store door carrying a key. |
| `chop.json` | A lumberjack with an axe and a bare-handed novice, both next to a tree. |
| `craft.json` | A master carpenter and a novice at the workbench, each carrying 8 logs. |
| `perception.json` | Open field: a Per 12 scout and a Per 8 minion, each 9 m from a book. |
| `vehicles.json` | A driver and a passenger next to the cart; the wagon outside the gate. |
| `formation.json` | Four faction-1 units in the yard and one enemy outside. |
| `agent.json` | `doors.json` with an Advisor controller pre-attached to the observer. |
| `smithy.json` | The crafting chain: a `smith` by the store key carrying a plank, a `joiner` (carpenter, `player` spend policy, 8 logs) at the workbench and a novice `apprentice` by the book. |
| `explorer.json` | The `frontier` layout (the compound plus, on a 128-cell grid: two tree groves west and east, three ore rocks north-east, a walled ruin to the north with a chest and a scroll, bread, logs and a far book scattered about) with one `scout`, `explorer`, in the yard under a `Direct` controller `explorer` for an AI agent. |

## The AI explorer (`src/explorer/`, `prompts/`)

The support code of the actor testbed's explorer mode and of `apps/game_menu` (std + serde, no sim state):
`prompt` renders the agent prompt from `prompts/start.md`, `resume.md`, `resume_changed.md`, `resume_rebuilt.md`
plus `common.md` (embedded with `include_str!`; edit the text there, then `cargo test -p testbed_content
explorer::prompt`), `home` the agent folders (`slug`, `create_agent`, `list_agents`), `names` random agent and
world names, and `settings` the menu's agents-folder setting (the diary, memory and proposals are
`npc_framework::agent_io::notes`, which the F7 editor and the notes methods share). The flow and the folder layout: `docs/agent-experiments.md` §9.

## Tests (`tests/scenarios.rs`, headless)

- **Loading:** every scenario file loads through the `npc_agent_io` loader registry and runs.
- **The default testbed has every piece:** factions, sheets, doors, vehicles, workstations and items. It also
  checks the ASCII map.
- **Bad scenarios** are rejected with a reason.
- **Doors and vision:** a unit sees through an open door but not through a closed one; after the door closes the
  target is `Lost` but still remembered.
- **Per rolls:** detection follows the Per stat (targets differ by Per, and the first success equals
  `Perceived`).
- **`agent/observe`** shows only what the unit perceives (`world/state` shows everything); the `testbed` and
  `sheet` sections are present.
- **Key and lock:** the key unlocks the store door (explicit `unlock` / `open` verbs).
- **Key and pathing:** a unit picks up the key, then a `Move` order into the store unlocks and opens the door
  along the way (nav door markers).
- **Chopping:** it needs an equipped axe (`needs tool:axe`). A chop yields 2 logs, uses up one of the tree's
  uses and makes a Woodcutting roll.
- **Autonomous woodcutter:** in the default testbed the woodcutter chops trees on its own.
- **Planks and skill:** plank quality and the success count follow Carpentry skill. Each attempt is a
  `craft.planks` roll.
- **Cart:** the cart is mounted with the `Interact` verb `mount` (driver seat first, then the passenger seat) and
  driven via `TestbedDrive`. The crew rides along at its seat offsets, and the passenger dismounts.
- **Wagon:** the `Direct` wagon takes a `Move` order and path-finds around the compound (`PathFound`, never
  inside a wall). An order from player 2 is rejected.
- **Formation:** a group `Move` (Box, 1.5 m) puts the units on distinct slots around the target. Ordering an
  enemy unit is rejected.
- **Determinism:** the same seed gives the same hashes, with the journal on or off; a different seed gives
  different hashes.

## Glue scenario tests (`tests/glue.rs`, wave G2b)

- **Mount orders and the cart:** `Mount { cart }` orders board it (driver seat first); a `Move` order to the cart
  goes to its driver (`npc_vehicles.crew` redirect), the passenger rides at its seat offset, then a queued
  `Unload` puts both on free cells.
- **The wagon with crew:** both units walk out through the gate (nav opens it) on `Mount { wagon }` orders; the
  `Direct` wagon is ordered itself (`Move`, then `Unload`) and carries them.
- **Queued `Interact` orders:** pick up the key → unlock → open the store → walk in → pick up the pickaxe and
  the hammer → walk to the ore rock; `use_tool_on` says "equip tool:pickaxe first" until `equip`.
- **Mine, smelt, forge:** two ore (Mining checks), the `ingot` at the forge, the `forge_axe` at the anvil with
  the hammer carried; the axe's quality follows the Smithing margin (`margin / 2`, +2 on a crit) and the rolls
  are Smithing −2; the craft awards `craft` XP.
- **Crafting XP and the AI policy:** planks give `craft` and `roll` XP up to level 1; the `Ai` policy spends
  every banked point at once on Carpentry, the most used skill.
- **The Player policy:** the joiner banks the level's 5 points; `SpendPoints` buys Carpentry (a too dear
  purchase is refused).
- **Study and teaching:** the apprentice studies the book (Carpentry, `SkillImproved { via: "study" }`); the
  joiner may not ("already Carpentry 14 (it teaches up to 12)"). The smith teaches the apprentice Smithing once
  it walked over ("too far" before); two equal smiths cannot teach each other ("teacher level 13 must be at
  least the student's 13 + 1").
- **Agent io, fetch and mine:** a scripted controller (like `npc_agent_io::greedy`) on the `Direct` smith uses
  only `agent/observe` and `agent/act`: it walks with `order` + `execute_orders`, and acts by affordance
  (`pick_up@key`, `unlock@door`, `open@door`, `pick_up@pickaxe`, `equip@pickaxe`, `use_tool_on@ore_rock`, then
  `pick_up@<ore>`), reading `why_not`s on the way ("door is locked: needs key:store", "needs tool:pickaxe", "too
  far: .."). The same session is recorded and replays bit for bit without the agent (a removed action does not).
- **Advisor and a squad:** `CreateSquad` (Wedge), then an Advisor's `order` to the squad moves its members to
  their own slots; ordering the enemy is refused ("owned by player 2").
- **Faction ownership:** orders across players (units, the cart, the wagon, `Interact`, a mixed group) are
  rejected; an agent on a skeleton sees and orders only player 2's units.

## Corpus (`tests/replays/testbed_*.json`, `tests/corpus.rs`)

| File | Scenario | Ticks | Commands | What |
|---|---|---|---|---|
| `testbed_default_2days` | `testbed` | 4800 | 0 | The showcase, two days (wandering, chopping, Per rolls). |
| `testbed_orders_craft` | `testbed:smithy` | 2400 | 18 | The fetch-key → unlock → open → pickaxe/hammer → mine → pick up ore → forge chain as `IssueOrder`s, then `Craft` ingot and forge_axe (retried until it succeeds at tick 1520). |
| `testbed_vehicles` | `testbed:vehicles` | 1200 | 8 | `Mount` orders to the cart, a `Move` + `Unload` to the cart (its driver drives), `Mount` orders to the wagon, a `Move` + `Unload` to the wagon. |
| `testbed_agent_session` | session record (`smithy`) | 600 | 16 | The fetch agent's session (`AttachController`, `AgentAct`, ..), re-run without the agent. |

`content/testbed_content/tests/corpus.rs` verifies them (and `content/village_content/tests/corpus.rs` skips
them); `cargo xtask verify` checks every corpus file with `npcctl replay verify`. A new registered name in the
testbed changes its content hash and so fails these files: that needs an approved re-baseline
(`docs/rebaselining.md`).

## Still open (wave G2a and later)

- `apps/actor_testbed` (the windowed view, wave G2a): models from `assets/testbed`; RTS selection, control groups,
  context orders; F5 sheet and F6 perception panels; screenshot checks.
- A testbed inspector (`set_inspector`) that embeds `perception_json`, `interaction_json`, `vehicle_json`,
  `command_json`, `crafting_json` and `experience_json`, plus content `perf_tags` (npcctl `inspect` shows the
  kernel's generic inspector until then).
- More scenarios as replays (`doors`, `chop`, `craft`, `perception`, `formation`, `agent`): each one is a
  `npcctl replay record --scenario testbed:<name>` away, but new corpus files need approval.
