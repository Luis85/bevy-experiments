---
paths:
  - "content/**"
  - "apps/**"
---

# Game content: register, don't patch core

Content is everything game-specific: unit types (villager, RTS units), their needs, smart objects, actions,
conditions, trees, scenarios and content commands. A game must be able to add its own content without
forking or editing core crates.

Where content lives: `content/<game>_content` with its view in `content/<game>_view`, and `apps/*`:

- the shipped game, Island Village: `content/island_content` (the `living_world` island: residents,
  economy, colony, terrain, saves under `src/village/**`; its one construction
  `island_content::construction::shipped()`), `content/island_view`, host `apps/island_game`;
- the examples: `content/village_content` / `village_view` (host `apps/village`) and
  `content/testbed_content` / `testbed_view` (host `apps/actor_testbed`), plus the RTS demo's own content
  in `apps/rts_units_demo`.

The island and the examples share no code and never depend on each other in either direction
(ADR 0044, checked by `arch_fitness`); code the island once shared with the testbed is a diverged copy,
so a fix on one side does not reach the other. The testbed hosts no island: the older island tiers
(scenarios `ai_village`, `living_village`, `island_colony` and their worlds) are removed, so do not add
a branch for a world that does not ship.

Content depends only on the facade `npc_framework` (plus the `bevy_*` sub-crates its derives expand to;
`arch_fitness` checks this) and registers by **stable name** through `NpcAppExt`: `register_action`,
`register_cond`, `register_scorer`, `register_tree`, `register_need`, `register_sim_component::<T>()`,
`register_command::<C>()`, plus `register_kind_name` and `set_inspector`. See `VillagePlugin` in
`content/village_content/src/lib.rs` for the pattern; the island's plugins are `IslandBasePlugin`
(`content/island_content/src/lib.rs`) and `IslandAgentPlugin` (`src/agent.rs`), and its registered names
are its own (`island_unit`, `island_ids`, `island_idle`, `island.world_rules`, ..). Before extending the
island's mechanics read `docs/architecture-island.md` (who owns which state and execution).

## Rules

- **No core edits for content.** Adding a unit type, need, action or smart object must not touch core
  crates (`npc_ai`, kernel/needs/jobs/locomotion, `npc_perf`, tools, facade). If the API cannot express it,
  stop and propose the core change separately (spec + ADR) instead of special-casing content in core.
- **Core never names content.** No villager types, ids or strings in core; `arch_fitness` checks the
  dependency graph.
- **Stable names, not ids.** Trees, journal and replays refer to actions/conditions by name. Never depend
  on registration order; ids are frozen sorted by name (need ids too: `Energy` < `Hunger` < `Thirst`, so a new
  need can renumber existing ones; look them up with `NeedRegistry::id`). A new name changes the content hash:
  say so.
- **SimId stability.** `SimId`s are handed out 1, 2, 3, .. in spawn order: the scenario's entities first,
  then runtime spawns (`SpawnVillager`, a scenario's actors after its world) get the next ids. Spawn new
  scenario entities **after** the existing ones so existing scenario ids stay put. Ids after them still move
  (a well spawned after the villager makes the `SpawnVillager` villager 7, not 6), and replay command files
  keep their numbers: stage such replays with a documented id remap (`docs/rebaselining.md` §5). In tests,
  look ids up (`VillageIds`, `TestbedIds::id(name)`, `IslandIds` on the island, `next_sim_id(&app)` in the
  village tests' `common`) instead of writing `SimId(6)`.
- **The island builds one world.** Every headless island test and the replay factory build
  `construction::shipped()` (through `island_content::scenario::try_new_sim_app`), and New Game builds
  `construction::from_preset` of the chosen world, so tests pin the world the player gets. Island data
  (catalog, resident tree `trees/resident.json`, `worlds/living_world.json`) is content: its authoring
  guides are `docs/island-ai-authoring.md` and `docs/island-economy-authoring.md`. Island saves of another
  build are refused, never migrated (ADR 0042).
- **Content is sim code:** all rules in `.claude/rules/sim.md` apply (integers, `SimId` order, `sim_rand`,
  no `Time`, no `HashMap`). Its view part follows `.claude/rules/view.md`.
- **Smart objects advertise** (`Provides`, `Approach`); actions query advertisements and reserve through
  `Reservations` instead of hard-coding entity references.
- **Guards:** every `Abort::LowerPriority` condition must imply its child can start.
- **Commands:** new external inputs are content commands (`ContentCommand` with `NAME`, `validate`,
  `apply`, optional `unit`); they are accepted as `{"type": NAME, ..fields}` everywhere. Keep JSON shapes
  accepted by `npcctl`/BRP backward compatible.
- **Hosts own the clock and input:** apps choose tick rate and schedule; content never writes `Time<Fixed>`.

## Proving it

- Scenario test first (headless), plus a journal check for the new behaviour (`npcctl journal --kind ...`;
  the island is `--scenario island:living_world`). Island tests live in `content/island_content/tests/`
  (helpers in `tests/life_support/`).
- A new or renamed island registration changes the island's content hash and every island state hash:
  say so; the island replays (`tests/replays/island_*`, staged in `tests/replays.next/`) are re-baselined
  by the same procedure as the others.
- Existing replays and goldens must still pass. If the change is meant to alter behaviour, re-baseline by
  the one procedure in `docs/rebaselining.md` (stage → evidence → user approval → swap); never edit
  `tests/replays/**` or snapshots yourself.
- `cargo xtask verify` (`cargo xtask verify --island` is a faster check of the island crates and island
  replays while you work; it replaces neither `--quick` nor the full gate).
