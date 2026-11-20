# 0044: The island in its own crates, older island tiers removed

Date: 2026-11-13. Status: accepted for the island (`living_world`) and the
Actor Testbed. Supersedes in part 0022, 0028, 0030, 0032, 0034, 0035, 0037
and 0042, and affects 0024 and 0036 (see "Earlier decisions" below).

## Context

The island village game grew inside the Actor Testbed crates:
`content/testbed_content/src/village/**` (about 43k lines), most of
`content/testbed_view` and the frontend half of `apps/actor_testbed`. The
normal entry was `game_menu`, which dispatched into the `actor_testbed`
library. There was no seam. The island used the testbed's unit spawn, ids,
item kinds and sheet templates; the testbed's scenario loader, world
presets, explorer host and views called back into the island.

Four island tiers lived side by side: `ai_village` (world
`island_village`), `living_village` (world `living_homestead`),
`island_colony`, and `living_world`. Only `living_world` ships. The older
tiers were kept as developer and replay fixtures behind five profile gates
(`frontier::enabled`, `needs_driven`, `geography::is_expanded`,
`colony::enabled`, `market::enabled`), about 230 call sites in 38 files.
Every island change had to keep three unshipped worlds working.

Two more facts shaped the cut:

- No protected oracle guarded an island. All 8 replays, all 6 snapshots,
  the proptest regressions file and `bench/baseline.json` guard
  `village_content` or the Actor Testbed.
- A crate rename does not change replay or snapshot bytes; the content hash
  and state hash use registered stable names. Removing a registration does
  change every state hash. So a throwaway `living_world` replay can prove
  bit-identity through every structural step, as long as registrations are
  held constant until one dedicated step.

The owner decided the scope: the island crates only, and no backwards
compatibility.

## Decision

**Three island crates.** The island is `content/island_content` (the
simulation, its scenario and world data, its tests), `content/island_view`
(presentation) and `apps/island_game` (the shipped game: title,
New/Continue, play, pause, local saves). `island_content` depends on the
facade only; `island_view` on `bevy`, the facade and `island_content`;
`island_game` on all three.

**No dependency between the island and the examples, in either direction.**
No `island_*` crate reaches `testbed_content`, `testbed_view`,
`actor_testbed`, `village_content`, `village_view` or `village`.
`testbed_content`, `testbed_view`, `actor_testbed`, `game_menu`,
`village_content` and `village_view` never reach an island crate.
`xtask/tests/arch_fitness.rs` enforces both (constants `ISLAND` and
`LEGACY_CONTENT`), together with the headless rule for `island_content` and
the canaries `island_game -> island_view -> {bevy_render, island_content}`
and `npcctl -> island_content`. `deny.toml` lists `island_view` and
`island_game` as `bevy` wrappers and `island_game` as the only wrapper of
`island_view`.

**Copy, then prune each side.** The three crates were created as copies of
the testbed crates (per sub-path, never a whole crate directory). The
testbed side then lost everything island; the island side lost everything
testbed and explorer. Shared base code (unit spawn, ids, item kinds, the
idle tree, scenario loader, world preset model, clipboard bridge, parts of
`visuals.rs`, `anim.rs`, `assets.rs`, `camera.rs`) is duplicated and
diverges. Nothing was lifted into a generic crate in this step. Both view
crates read the same on-disk `assets/` folder.

**`living_world` is the only island.** Scenarios `ai_village`,
`living_village` and `island_colony` and worlds `island_village`,
`living_island`, `living_homestead` and `island_colony` are deleted. Every
profile gate was resolved to its `living_world` value and the gate
functions deleted, then the code that became unreachable was removed.
`SCENARIO_FILES` and the world `BUILTIN_PRESETS` hold one entry each.

**`island_game` is the shipped binary.** With no flag it opens the title on
the island's own save. `--scenario`, a bare `--island-save` (without
`--front-end`), `--island-new`, `--commands` or `--record` select the
developer host (no title, straight into the world). `--front-end` is still
accepted and keeps the player flow with a custom save. `game_menu` is the
developer menu only; with no flag it opens that menu, and
`--developer-menu` is still accepted. Its main screen offers the Actor
Testbed, the village and the RTS demo, and has no island entry.
`scripts/package_island.ps1` ships `island_game.exe`.

**The island owns its save folder.** The default save is
`<home>/island-village/island.world.json`, with the world rules beside it as
`living_world.json` (`island_game::{ISLAND_HOME_DIR, SAVE_FILE, RULES_FILE,
island_home}`; home is `USERPROFILE` or `HOME`). The developer menu's
agents folder is not read.

**The explorer and AI-agent hosts no longer host an island.**
`actor_testbed --explorer-home`, `npcctl serve --explorer-home` and the
developer menu's explorer work on testbed worlds only (default
`plain_explorer`). The read-only world projection (`--world-state-out`,
`projection.rs` in both hosts, the run-file handover metadata) had an
island-only producer and is removed. An agent reaches the island through
`island_game --remote`, which serves `npc/*`, `agent/*` and `world/state`.
`npcctl` knows the island as content `island`: replays are named
`island:<name>`, and a bare `living_world` qualifies to
`island:living_world` (`npcctl replay record --scenario living_world`).

**One shipped construction.** `island_content::construction::shipped()` is
`from_preset` of the built-in `living_world` world: the embedded
`living_world` scenario with that world's descriptor (`Construction`, JSON
key `world.construction`) written in. The headless constructor
(`try_new_sim_app`), and so the replay factory and every test, builds
`shipped()`; the game's New/Continue builds `from_preset` of the chosen
world. Tests and replays pin the world the player gets (perception memory
256, `RevealsFog`). `load_with` refuses an island scenario without its
construction. `worlds/living_island.json` and the scenario file's inline
`living_world` block are removed; the world preset is the one source.

**One hash-changing step, after the structure is final.** Step 5 dropped
registrations the island inherited and did not use (the `TestbedDrive`
command, the wander and worker trees, the walk/wander/tool-work actions and
`TbWalking`, the `book`, `scroll` and `store_key` item kinds and the study
table, the skeleton sheet templates, the fog-statics and LIDAR-obstacle
shaping). The four testbed recipes and the testbed item kinds and skills
the island data relies on stay. It renamed the inherited names:

| Was | Now |
|---|---|
| `ExplorerContext` (JSON `explorer_context`) | `island_content::construction::Construction` (JSON `construction`) |
| `TestbedPlugin`, `TestbedAgentPlugin`, `TestbedViewPlugin` | `IslandBasePlugin`, `IslandAgentPlugin`, `IslandViewPlugin` |
| `TestbedIds`, `TestbedUnit`, `TestbedPanels`, `TbIdling` | `IslandIds`, `IslandUnit`, `IslandPanels`, `IslandIdling` |
| stable names `testbed_unit`, `testbed_prop`, `testbed_ids`, `tb_idling` | `island_unit`, `island_prop`, `island_ids`, `island_idling` |
| action `TbIdle`, tree `testbed_idle` | `IslandIdle`, `island_idle` |
| verb gates `testbed.allowed_actions`, `testbed.world_rules` | `island.allowed_actions`, `island.world_rules` |
| kind namer `testbed`, observation section `testbed` | `island`, `unit` |

`island_content::world_runtime` was reduced to `ActiveWorld`, `prepare`,
`enabled`, `apply` and the system-switch verb gate. In the view, robot
bodies and the key and scroll models are gone, and the fog can no longer be
turned on (no F8 toggle, no `--fog`); `island_game` no longer accepts
`--panels explorer|map`. `ISLAND_SAVE_VERSION` went from 2 to 3.

**An island tier in `xtask`.** The plan adds `cargo xtask verify --island`
(fmt, clippy and tests for the three island packages, the fast soak
`island_soak_fast`, `arch_fitness` and goldens; no `deny`, not together
with `--quick`). `xtask/src/main.rs` is protected; the change is drafted as
a patch for the owner and is not in the tree when this ADR is written. The
Stop hook stays on `--quick` (D6).

## Consequences

- An island change touches three crates and one world. There is no profile
  gate to keep true for unshipped tiers.
- The Actor Testbed, the village, the RTS demo, the explorer hosts and the
  developer menu stay as examples of the generic framework. Their 8
  replays, 6 snapshots, proptest regressions and `bench/baseline.json` are
  untouched; the testbed replays and the `npcctl` testbed snapshot passed
  after every step.
- Duplicated base code now exists twice (about 170k lines were copied and
  roughly a third pruned). A fix in the testbed's `visuals.rs`, `anim.rs`,
  `assets.rs`, `camera.rs`, clipboard bridge or scenario loader does not
  reach the island, and the reverse. Lifting the generic parts into
  `npc_view_helpers` is a later, separate step.
- Leftovers the island still carries, to be cut in a later cleanup:
  `island_view::{fog, overlay, panels, command}` (fog is permanently off;
  F1/F5/F6 are developer diagnostics behind `--dev-keys`; the island's
  selection and orders run through `CommandUiPlugin`), the skeleton
  `ModelKind`s that no template spawns, and `village::roles::TESTBED`, the
  fallback role table for a world without catalog data (ADR 0042), which no
  shipped island world reaches.
- Capture scripts name `island_game.exe`, pass `--island-rules`, and use
  `island_view::` / `island_content::` reflect type paths. They were edited
  and not run.
- `content/island_content/clippy.toml` is a byte copy of the testbed one,
  created by the owner (the hook blocks agents from creating it).

## What was removed

| Area | Removed |
|---|---|
| Scenarios | `ai_village`, `living_village`, `island_colony` |
| World presets | `island_village`, `living_island`, `living_homestead`, `island_colony` |
| Profile gates | `frontier::enabled`, `needs_driven`, `geography::is_expanded`, `colony::enabled`, `market::enabled`, `catalog::frontier_profile` |
| Legacy-only sim code | the fixed-economy profession work (`ai::gather_for`, `buy_input`, `manufacture`, fixed patch replenishment, fixed dividend and levy), the Rust `island_life` tree, the fixed spawn tables, compact geography (`geography::register_living`), the Rust recipe and item tables (`catalog::register_living`), the legacy 64x64 terrain brush bodies, the legacy hunting, repose and production paths |
| View | `homestead_panel/**` (the Home & farm panel, F2), legacy terrain picking (`shape_tools::picking::terrain_hit`, `Limits::LEGACY`), the colony scene's legacy terrain, lantern and smoke systems, robot bodies, key and scroll models, the fog toggle |
| From the testbed side | `village/**`, `projection.rs`, `trees/resident.json`, all island scenarios and presets, `Layout::Island`, the island fields of `WorldSpec` and `WorldPreset` (`living_world`, `colony`, `island_ai`, `economy`, `island_geography`, `scene`), `EconomyModel`, `IslandGeography`, `WorldScene`, the explorer host's village frame and notes bridge, the island briefing, every island view module |
| `actor_testbed` | `frontend/**`, `island.rs`, `island/session.rs`, `projection.rs`, flags `--front-end`, `--dev-keys`, `--island-*`, `--ui-gallery`, `--mute`, `--world-state-*`; the default scenario is `testbed` |
| `game_menu` | `frontend_dispatch.rs`, the `actor_testbed` library dependency, `Entry::Island`, the local-island quit path, the main screen's island buttons, the projection handover, the Worlds editor's Resident AI page and island fields |
| `npcctl` | `src/projection.rs`, `village::host::frame`, `tests/island_playtest.rs` |
| From the island side | `explorer/**` (except the forked construction descriptor and the clipboard bridge), `prompts/`, `walls.rs`, actor presets `rover` and `sentinel`, the 11 testbed and explorer scenarios, presets `plain_explorer`, `survival`, `night_shift`, the compound layout, the testbed-only view modules (`explorer_modal`, `explorer_panel`, `landmarks`, `model_preview`, `robots`), the host's `actor_editor`, `ai_client`, `ai_history`, `editor_mode`, `explorer`, `projection`; the explorer world mechanics in `world_runtime` |
| Docs | `docs/ai-village.md`, `living-homestead.md`, `living-island-economy.md`, `island-village-playtest.md`, `island-village-stream.md` |
| Scripts | `scripts/playtest/{capture_island_colony,capture_colony_menu,capture_menu,capture_sessions,client_smoke,live_smoke,test_client_smoke}.py`. `capture_native.py` and `capture_native_homestead.py` stay: every kept capture script imports their helpers; their own `main()` cannot run |

## Tests deleted and the coverage lost

Not ported (explorer on an island, decision D1):

- `tests/world_projection.rs`, `tests/island_scheduler_events.rs`,
  `apps/npcctl/tests/island_playtest.rs`, the homestead resume test in
  `tests/explorer_host.rs`, the three island briefing tests in
  `explorer/workspace.rs`, the two local-island quit tests and the
  projection handover test in `game_menu`.

Deleted on the island side in step 3:

- `tests/island_notes_lifecycle.rs` (explorer notes across lives).
- `tests/island_village.rs`: explorer credential succession, a saved dead
  actor claimed for a new life, and the short-year production run built
  through the explorer `world_doc`.
- `tests/island_economy_soak.rs`: the three-seed accelerated seasonal
  fixture.
- `tests/island_agent_information.rs` and
  `tests/living_agent_information.rs`: the explorer briefing text.
- `tests/living_geography_presets.rs`: rejection of an incompatible world
  model (needed the plain explorer preset).
- Unit tests: the prompt-copy clipboard payloads, "every embedded scenario
  has a description", the vehicle rider seat offsets in the island view.

Deleted on the testbed side in step 2 with the island code they tested:
the fog deer test, the panels frontend test, three island tests in
`visuals.rs`.

Older-tier tests in step 4 (decision D3): the older scenario and world
names in test code were swapped to `living_world` (68 files, 154 sites) and
the island tests run once: 820 passed, 242 failed, 37 ignored. The swapped
tests that failed, plus 7 that are older-tier by name or fixture, were
deleted: 228 tests. Eighteen files were emptied and removed:
`colony_shortage_recovery`, `colony_warmth_recovery`,
`island_agent_information`, `island_economy_soak`,
`island_environment_contract`, `island_named_crafting`,
`island_named_production`, `living_agent_information`, `living_geography`,
`living_geography_presets`, `living_homestead`, `living_homestead_agent`,
`living_hunting`, `living_projection_geography`, `living_replacement`,
`living_routine_polish`, `living_stability`, `living_survival`. Also
deleted: `regional_world::regional_projection_uses_authored_settlement_names_and_positions`
with the orphaned `island_content::projection` module. Four failures not
caused by a name swap were left for the closing phase
(`island_demography` conservation, `island_fields` granaries,
`island_indoors` resting outside, `island_live_homestead`).

Coverage lost, in plain terms:

- The agent briefing text for an island, and any check that an AI agent can
  play an island through the explorer host.
- Explorer credential succession and notes carried across lives on an
  island.
- The world-state projection file.
- Two accelerated older-tier production fixtures and the three-seed
  seasonal soak of the fixed economy.
- Every assertion that contrasted an older tier with `living_world`, and
  the swapped tests that asserted fixed positions, counts or names on the
  old fixtures. `living_world` behaviour those tests would have covered if
  rewritten (hunting, homestead verbs, speech, market, the life tree, the
  player journey) is a closing-phase task.
- Loading or migrating a save of an older tier.

## How the island is guarded now

- **During the split:** three throwaway `living_world` replays (seeds 7,
  11, 23; 24000 ticks; 40 checkpoints each; content hash
  8487267918308494104), recorded with the base build before any edit and
  renamed to `island:living_world`. They verified bit for bit after steps
  1 and 3, after the gate resolution of step 4 and at the end of step 4.
  Step 5 changed the hash on purpose, so they no longer apply. They live
  under `target/island-split/` and are not oracles.
- **Structure:** the `arch_fitness` rules above; `island_view/tests/plugin.rs`
  (the whole view plugin on the shipped world, headless, three frames).
- **Behaviour:** the island tests in `content/island_content/tests`.
- **Oracles:** none yet. The next step stages island replays in
  `tests/replays.next/` and goldens as `*.snap.new`, with an evidence note
  in `docs/_notes/`; the owner approves and swaps (`docs/rebaselining.md`).

## What is not kept compatible

- **Saves.** A save made by an older tier, or by an earlier build of the
  living world, is refused with one plain sentence
  (`island_save::check_island_save`, ADR 0042). Nothing is migrated.
  `ISLAND_SAVE_VERSION` is 3, so saves made before step 5 are refused too.
- **Save location.** `living-world.world.json` and
  `local-island.world.json` in the agents folder (`<home>/npc-agents/`) are
  not read, moved or offered.
- **Scenario and world names.** `ai_village`, `living_village`,
  `island_colony`, `island_village`, `living_island`, `living_homestead` do
  not load in any host. `npcctl --scenario testbed:living_world` no longer
  exists; the name is `island:living_world`.
- **Replay headers.** An island replay recorded before the split says
  `"scenario":"testbed:living_world"` and is not claimed by the island
  content. No such replay is in the corpus.
- **Explorer runs on island worlds.** Agent workspaces created on
  `island_village`, `living_island` or `living_homestead` cannot be resumed.
- **Entry points.** `game_menu` no longer starts the island.
  `actor_testbed --front-end` and `--island-*` are usage errors.
- **Reflect type paths, perf tags and stable names.** `testbed_view::...`
  and `testbed_content::...` paths for island types became
  `island_view::...` and `island_content::...`; BRP queries and trace
  filters that name the old paths find nothing. The renamed stable names in
  the table above change the island's content hash and every island state
  hash.

## Earlier decisions

These ADRs stay as history:

- 0022, 0028, 0030, 0032, 0034 (superseded in part): the tiers they define
  (`ai_village`, `living_village`, the homestead panel and its input) are
  removed; what `living_world` inherited stands.
- 0035 (superseded in part): `island_colony` as the showcase, and its
  ownership table paths.
- 0037 (superseded in part): the regional world stands; the "older profiles
  unchanged" guarantees and the legacy save path no longer apply.
- 0042 (superseded in part): the save identity stands; `roles::TESTBED` as
  logic shared by the testbed and the island does not (each side has its
  own copy).
- 0024 (affected): the read-only world projection is removed from the
  hosts.
- 0036 (affected): the one-window flow stands, in `island_game` instead of
  `game_menu` dispatching into `actor_testbed`.
