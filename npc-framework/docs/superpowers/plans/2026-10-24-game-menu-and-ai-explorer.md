# Game Menu and AI Explorer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A game menu that starts every demo, and an AI explorer testbed: one actor driven by an external AI agent through a pasted prompt, with a diary and proposals on disk, an in-game panel showing them, and real world-state saves for resume.

**Architecture:** Three layers, built bottom-up. (1) A generic world snapshot in `npc_kernel` (a snapshot registry next to the hash registry; every crate registers its tick-deciding state; save → load → run equals an uninterrupted run). (2) The explorer: a fixed `explorer` scenario and a std-only `testbed_content::explorer` module (names, folders, prompts, settings, diary/proposal parsing), wired into `actor_testbed` behind new flags, with a prompt modal and an F7 diary panel in `testbed_view`. (3) `apps/game_menu`: a Bevy window that launches the existing apps as child processes.

**Tech Stack:** Rust ≥ 1.97.1, Bevy 0.20 (`bevy_ui`, `ui_widgets`, `bevy_clipboard`), serde/serde_json, the npc-framework facade.

**Spec:** `docs/superpowers/specs/2026-10-24-game-menu-and-ai-explorer-design.md`

**Deviation from the spec (needs the dev's OK):** spec §5.3 puts the two mismatch buttons in the in-game modal. A save is loaded before the window opens, and switching worlds inside a running window would force the view to rebuild every visual. So the menu checks the save first (`actor_testbed --check-save`) and shows the same two buttons, with the same outcomes, before the game opens (Task 13). Started by hand without the menu, `actor_testbed` exits with code 3 and names the two flags.

## Global Constraints

- Golden rules in `CLAUDE.md`: determinism (no `Time`, wall clock, `HashMap` order or `Entity` order in sim code; integers in sim state), core/content boundary, no host hijack, protected oracles, `cargo xtask verify` prints `"ok":true`.
- Protected files are never edited by the implementer: `deny.toml`, `clippy.toml`, `xtask/src/main.rs`, `xtask/tests/arch_fitness.rs`, goldens, snapshots under `**/snapshots/**`, `tests/replays/**`, `bench/baseline.json`, `.claude/**`. When a task needs one changed, stop and ask the user with the exact one-line change (`docs/adding-a-host-app.md`).
- Existing replays, goldens and snapshots must stay byte-identical: the snapshot feature only adds registrations; it must not change any state hash or content hash of an existing scenario. If a registration would change the content hash, it must not (snapshot entries are not part of `ContentHash`).
- Bevy 0.20 APIs: check `docs/research/bevy-0.20-cheatsheet.md` or the registry source, never memory.
- View code reads sim state and never writes it; the app owns keys and `SimControl` (`.claude/rules/view.md`).
- Controller name `explorer`, mode `Direct`, `on_idle: Idle`. Agent files: `start_prompt.md`, `resume_prompt.md`, `diary.md`, `proposals/`, `<world slug>.world.json`, plus `agent.json` (names and dates).
- Autosave every 60 s of wall time and on quit; write `<file>.tmp` then rename.
- Default agents folder `<home>/npc-agents`; settings file `<config dir>/npc-framework/settings.json` (`%APPDATA%` on Windows, `$XDG_CONFIG_HOME` or `~/.config` elsewhere).
- Default BRP port 15702; the explorer takes the first free port in 15702..=15721.
- CHANGELOG line per task that changes behaviour; commit after every task with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. Agent or world names with path-illegal characters (`/ \ : * ? " < > |`), only spaces, or two names that slug to the same folder: New must produce a valid folder and refuse to overwrite an existing agent folder, naming it. → Task 7 tests `slug_*` and `create_agent_refuses_existing`.
2. The agents folder is unset, missing or unwritable: the menu says so and will not start the explorer until it is fixed; `actor_testbed` exits with code 2 and a message naming the folder. → Task 7 `create_agent_reports_unwritable_root`, Task 12 `explorer_disabled_without_valid_folder`.
3. A truncated or corrupt save (crash mid-write before the rename, hand edits): reported as unreadable with the path, never a panic; New/Resume still work for other agents. → Task 4 `read_reports_truncated_file`.
4. Diary or proposal files missing, empty, not UTF-8, or caught half-written: the panel shows what parses and never panics. → Task 7 `diary_entries_tolerate_*`, `read_proposals_skips_unreadable`.
5. Port 15702 already taken by another game: the explorer binds the next free port and the prompt names it. → Task 9 `pick_port_skips_taken`.

---

## Phase A: World snapshot (core)

### Task 1: ADR and the snapshot registry with component and resource round trip

**Files:**
- Create: `docs/adr/0013-world-snapshot.md`
- Create: `crates/npc_kernel/src/snapshot.rs`
- Modify: `crates/npc_kernel/src/lib.rs` (module + exports), `crates/npc_kernel/src/registry.rs:174-182,232-260` (auto-register snapshot entries), `crates/npc_kernel/src/hash.rs` (nothing removed; `SimHashResource` gains a `DeserializeOwned` path only through the new registry)
- Test: `crates/npc_kernel/tests/snapshot.rs`

**Interfaces:**
- Produces (in `npc_kernel::snapshot`):
  - `pub const SNAPSHOT_VERSION: u32 = 1;`
  - `#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)] pub struct WorldSnapshot { pub version: u32, pub content_hash: u64, pub tick: u64, pub header: serde_json::Map<String, Value>, pub entities: Vec<EntitySnapshot>, pub resources: BTreeMap<String, Value> }`
  - `#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)] pub struct EntitySnapshot { pub id: u64, pub components: BTreeMap<String, Value> }` (entities sorted by `id`)
  - `#[derive(Resource, Default)] pub struct SnapshotRegistry` with private sorted entries `{ name: &'static str, save: fn(&World, Entity) -> Option<Value>, load: fn(&mut EntityWorldMut, Value) -> Result<(), String> }` for components and `{ name, save: fn(&World) -> Option<Value>, load: fn(&mut World, Value) -> Result<(), String> }` for resources.
  - `pub trait SnapshotAppExt { fn register_snapshot_component<T: Component + Serialize + DeserializeOwned>(&mut self) -> &mut Self; fn register_snapshot_resource<R: Resource + Serialize + DeserializeOwned>(&mut self) -> &mut Self; fn register_snapshot_resource_with(&mut self, name: &'static str, save: fn(&World) -> Option<Value>, load: fn(&mut World, Value) -> Result<(), String>) -> &mut Self; fn add_snapshot_rebuild(&mut self, name: &'static str, f: fn(&mut World)) -> &mut Self; }` implemented for `App`, re-exported through `KernelAppExt` users.
  - `pub fn save(world: &mut World) -> Result<WorldSnapshot, String>` (reads only; `&mut` for the frozen content hash).
  - `pub fn load(world: &mut World, snap: &WorldSnapshot, mode: LoadMode) -> Result<LoadReport, LoadError>`; `pub enum LoadMode { Strict, Lenient }`; `#[derive(Debug, Default, Serialize)] pub struct LoadReport { pub skipped: Vec<String>, pub defaulted: Vec<String> }`; `pub enum LoadError { Version { found: u32, expected: u32 }, ContentHash { found: u64, expected: u64 }, Bad(String) }` with `Display`.
- `register_sim_component::<T>()` additionally registers `T` in `SnapshotRegistry`; its bound grows `+ DeserializeOwned` (the inventory found every registered component already derives `Deserialize`). `register_sim_component_with` takes the same extra bound. `register_sim_resource::<R>()` registers `R` when `R: Serialize + DeserializeOwned`; resources without serde get an explicit `register_snapshot_resource_with` in Task 2.
- `SimClock` (add serde derives) and `SimIdAllocator` (add serde derives) are registered as snapshot resources by the kernel plugin; `SimClock.tick` is restored by `load`.

- [ ] **Step 1: Write ADR 0013** — context (worlds that live for hours; replay from tick 0 grows without bound), decision (a snapshot registry separate from the hash registry; snapshot entries are not part of `ContentHash`; save between ticks only; load into an app built from the same scenario and seed; derived state rebuilt), consequences (every crate registers its tick-deciding state; the invariant test is the completeness check; tooling resources are not saved), alternatives rejected (replay-only resume; Bevy `DynamicScene` via reflection, because `BrainState`, `ExternalQueue` and others have no `Reflect`).

- [ ] **Step 2: Write the failing tests** in `crates/npc_kernel/tests/snapshot.rs`, using a minimal app with two test components (`A(i32)`, `B { v: Vec<u32> }`) registered through `register_sim_component` and one resource:

```rust
#[test] fn save_lists_entities_by_sim_id_and_components_by_name() { /* 3 entities spawned out of order → snap.entities ids == [1,2,3]; components keys sorted */ }
#[test] fn save_load_save_is_identity() { let s1 = save(&mut a); load(&mut fresh, &s1, LoadMode::Strict).unwrap(); assert_eq!(save(&mut fresh).unwrap(), s1); assert_eq!(state_hash(fresh.world()), state_hash(a.world())); }
#[test] fn load_restores_allocator_and_clock() { /* after load, next spawn_sim gets the original next id; SimClock.tick == snap.tick */ }
#[test] fn strict_load_rejects_other_version_and_content_hash() { /* version 99 → LoadError::Version; content_hash ^ 1 → LoadError::ContentHash */ }
#[test] fn lenient_load_reports_unknown_and_missing() { /* snapshot with component "Gone" and without resource R → report.skipped == ["Gone"], report.defaulted == ["R"] */ }
#[test] fn registering_snapshot_entries_keeps_content_hash() { /* content_hash of an app with and without an extra register_snapshot_component is equal */ }
```

- [ ] **Step 3: Run** `cargo test -p npc_kernel --test snapshot` — Expected: FAIL (module `snapshot` not found).

- [ ] **Step 4: Implement `snapshot.rs`** and the registry hooks. `load` steps, in order: check version and content hash (Strict: error; Lenient: continue, record in `skipped`); insert a `SnapshotLoading` marker resource; despawn every `SimId` entity in id order; restore resources (unknown names → `skipped`, registered names absent from the snapshot → `defaulted`, keeping the built value); spawn each entity as `(SimId(id),)` and apply each component's `load` (unknown → `skipped`); set `PrevSimPos = SimPos` for entities that have `SimPos`; run every `add_snapshot_rebuild` fn in name order; clear `Journal` entries and remove `SnapshotLoading`. While `SnapshotLoading` exists, `SimIdHooks` leave hooks and the `Spawn`/`Despawn` journal writes are skipped (`ids.rs:40-81,155-170`).

- [ ] **Step 5: Run** `cargo test -p npc_kernel --test snapshot` — Expected: PASS; then `cargo test -p npc_kernel` — Expected: PASS.

- [ ] **Step 6: Commit** — `feat(kernel): world snapshot registry, save and load (ADR 0013)`

### Task 2: Register all tick-deciding state, rebuild derived state, settle change detection

The inventory of state outside the hash registry (from the planning sweep; paths relative to the repo root) is this task's checklist. Each line names who registers what.

**Files:**
- Modify (registration + serde derives where missing):
  - `crates/npc_kernel/src/ids.rs:123` `SimIdAllocator` (Task 1), `crates/npc_kernel/src/commands.rs:320` `CommandQueue` (resource, custom fn: `pending` and `applied_through`), `crates/npc_kernel/src/spatial.rs:248` `SpatialGrid` (custom fn: static layer and `pending` only; rebuild fn re-indexes entities), `crates/npc_kernel/src/schedule.rs:109-116` (`BrainState`, `ActiveLeaf`, `Blackboard` are hashed via `add_component` but need snapshot entries).
  - `crates/npc_ai/src/runtime.rs:17-21,53-55` `Brain` (add serde: the tree name), `ThinkRequest` (add serde) — registered by the kernel's schedule plugin.
  - `crates/npc_needs/src/lib.rs:236` `PauseDecay`; `crates/npc_jobs/src/lib.rs:33,40,53` `Provides`, `Approach`, `Reservations` (add serde).
  - `crates/npc_locomotion/src/lib.rs:118` `ExternalMover`.
  - `crates/npc_nav/src/lib.rs:213`, `crates/npc_nav/src/astar.rs:59,110` `NavQueue` with its active search and scratch (add serde; the `MessageCursor<GridChanged>` is not saved: after load it starts at the current message buffer, which is empty at a tick boundary).
  - `crates/npc_perception/src/noise.rs:23` pending `Messages<Noise>` (custom fn: the buffered noises; the hash already covers them as `pending_noises`).
  - `crates/npc_agent_io/src/lib.rs:123-127`, `control.rs:103,239` `ExternalController`, `ExternalQueue`; `api.rs:127` `AgentIoState` (resource).
  - `crates/npc_interaction/src/door.rs:91` `DoorCells`: rebuild fn only (derived).
  - Content: `content/testbed_content/src/lib.rs:86-89`, `content/village_content` — any content component or resource not yet registered that later ticks read (check `TbIdling`, `TbWalking` are registered; `TestbedIds` is a spawn mirror: save it as a resource so names survive resume).
- Modify: `crates/npc_kernel/src/snapshot.rs` (settle step)
- Test: `content/testbed_content/tests/snapshot.rs`, `content/village_content/tests/snapshot.rs`

**Interfaces:**
- Consumes: Task 1 `save`, `load`, `SnapshotAppExt`.
- Produces: `pub fn assert_resume_equivalent(make: impl Fn() -> App, at: u64, run: u64, every: u64)` in a shared test helper `crates/npc_tools/src/snapshot_check.rs` (behind `cfg(any(test, feature = "test-helpers"))` or as a plain pub fn; builder's call): builds two apps, runs one to `at`, saves, loads into a freshly built second app, runs both `run` more ticks and asserts `state_hash` equal every `every` ticks, printing the first mismatch with `entity_diffs`.

- [ ] **Step 1: Write the failing invariant tests**:

```rust
// content/testbed_content/tests/snapshot.rs
#[test] fn resume_equals_uninterrupted_testbed() { for at in [1, 37, 400, 1203, 2400] { assert_resume_equivalent(|| new_sim_app(1, "testbed"), at, 1200, 20); } }
#[test] fn resume_equals_uninterrupted_every_embedded_scenario() { for name in ["doors","locked_store","chop","craft","perception","vehicles","formation","agent","smithy"] { assert_resume_equivalent(|| new_sim_app(1, name), 150, 600, 10); } }
#[test] fn resume_mid_action_states() { /* smithy with the testbed_orders_craft commands queued: save at ticks where a unit is mid-walk (MoveStatus moving), mid-craft (Crafting present), waiting on a key (failed path in key_replan), mounted (MountedIn present) — find each tick by scanning, assert found, then assert_resume_equivalent */ }
#[test] fn resume_keeps_pending_commands() { /* queue a command for tick 500, save at 300, load: it applies at 500 in both */ }
// content/village_content/tests/snapshot.rs
#[test] fn resume_equals_uninterrupted_village() { for at in [1, 600, 1800, 4000] { assert_resume_equivalent(|| village_content::scenario::new_sim_app(1, "village", 25), at, 1200, 20); } }
```

- [ ] **Step 2: Run** `cargo test -p testbed_content --test snapshot` — Expected: FAIL with a first mismatch tick and an entity diff (missing state).

- [ ] **Step 3: Register the inventory's state** (files above), one crate at a time, re-running the tests after each; every mismatch names the next missing piece.

- [ ] **Step 4: Settle change detection on load.** A freshly loaded world makes every component look `Added`/`Changed` to sim systems that have never run (`key_replan_system`, `door.rs:315-345`; `spatial_update_system`, `spatial.rs:884-948`), which an uninterrupted run would not see. In `load`, after the rebuild fns: initialize the `NpcSimTick` schedule and set every system's last-run tick to the world's current change tick (check the Bevy 0.20 API for `System::set_last_run` / schedule system access in the registry source), so the first tick after load sees only real changes. If Bevy offers no such access, use `world.clear_trackers()` plus a documented equivalent and say so in ADR 0013.

- [ ] **Step 5: Run** `cargo test -p testbed_content --test snapshot` and `cargo test -p village_content --test snapshot` — Expected: PASS.

- [ ] **Step 6: Run** `cargo xtask verify --quick` — Expected: `"ok":true` (no existing hash, replay or golden changed).

- [ ] **Step 7: Commit** — `feat: register tick-deciding state for snapshots; resume equals uninterrupted run`

### Task 3: Property test with random commands

**Files:**
- Test: `content/village_content/tests/properties.rs` (add), `content/testbed_content/tests/snapshot.rs` (add)

**Interfaces:**
- Consumes: Task 2 `assert_resume_equivalent`; the village property-test command generator used by `slow_random_commands_keep_invariants`.

- [ ] **Step 1: Write** `slow_random_commands_resume_equivalent` (village): a `prop_state_machine!` test like `slow_random_commands_keep_invariants` (`properties.rs:519-522`, same `config()` and `slow_` prefix so nextest treats it as slow) with random commands, a random save tick in 1..4000, then `assert_resume_equivalent` for 1200 ticks. And `random_orders_resume_equivalent` (testbed): random `IssueOrder` Move/Interact commands from the testbed's command vocabulary, 8 seeds.
- [ ] **Step 2: Run** `cargo test -p village_content --test properties slow_random_commands_resume_equivalent` — Expected: PASS; fix any newly found missing state in the owning crate.
- [ ] **Step 3: Commit** — `test: snapshot resume property tests`

### Task 4: Files, facade and the command line

**Files:**
- Modify: `crates/npc_kernel/src/snapshot.rs` (file helpers), `crates/npc_framework/src/lib.rs` + prelude (re-export `snapshot`), `apps/npcctl/src/main.rs` (subcommands), `docs/debugging.md` (document), `CHANGELOG.md`
- Test: `crates/npc_kernel/tests/snapshot.rs`, `apps/npcctl/tests/snapshot.rs`

**Interfaces:**
- Produces: `pub fn write_atomic(path: &Path, snap: &WorldSnapshot) -> std::io::Result<()>` (writes `path.with_extension("json.tmp")`, then renames); `pub fn read(path: &Path) -> Result<WorldSnapshot, String>` (the error names the path and says "unreadable save: …"); `pub fn check(world: &mut World, snap: &WorldSnapshot) -> Result<(), LoadError>` (version + content hash only, no load).
- `npcctl snapshot save --scenario testbed[:<name>] --seed N --ticks T --out FILE` and `npcctl snapshot check --scenario … FILE` → JSON `{"ok":bool,"error":..}` (exit 0/1/2).

- [ ] **Step 1: Write failing tests**: `write_atomic_leaves_no_tmp` (after write, only `FILE` exists); `read_reports_truncated_file` (first half of a valid file → `Err` containing the path and "unreadable save", no panic); `check_flags_content_hash` ; npcctl: `snapshot_save_then_check_ok` and `snapshot_check_other_scenario_fails` (exit 1, `error` mentions "content hash").
- [ ] **Step 2: Run** `cargo test -p npc_kernel --test snapshot` and `cargo test -p npcctl --test snapshot` — Expected: FAIL.
- [ ] **Step 3: Implement** the helpers and subcommands; re-export through the facade as `npc_framework::snapshot`.
- [ ] **Step 4: Run** both test commands — Expected: PASS.
- [ ] **Step 5: Commit** — `feat: snapshot files, facade export and npcctl snapshot`

---

## Phase B: Explorer content

### Task 5: The frontier layout and the `explorer` scenario

**Files:**
- Modify: `content/testbed_content/src/scenario.rs:41-110,142-170` (`Layout::Frontier`, grid size per layout, embedded list), `content/testbed_content/src/layout.rs` (`spawn_frontier`), `content/testbed_content/README.md` (describe it)
- Create: `content/testbed_content/scenarios/explorer.json`
- Test: `content/testbed_content/tests/scenarios.rs`

**Interfaces:**
- Produces: `Layout::Frontier` (serde `"frontier"`): compound walls and furnishing exactly as `Compound`, then `spawn_frontier(world, ids)` spawns the outside features **after** them (so compound ids are the same as in every compound scenario). Grid: `GridBounds::centered(128, 128)` for `Frontier`, `GRID_CELLS` (64) otherwise.
- `explorer.json`: `"layout": "frontier"`, `furnish: true`, `store_locked: true`, `detection: "per_roll"`; one actor `{ "kind": "unit", "name": "explorer", "template": "scout", "pos": [0, -6500], "facing": [0, 1] }` (inside the compound yard, no `fixed_facing`); `"controllers": [{ "actor": 0, "controller": "explorer", "mode": "Direct", "on_idle": "Idle" }]`.
- Outside features (fixed; adjust positions only to keep them reachable): two tree groves (6+ trees each) west and east of the compound; three ore rocks to the north-east; a ruin to the north (a 7×5 wall ring with a 1-cell gap, a `Chest` prop inside, a `scroll` item inside); scattered items: `bread` ×2, `log` ×3, `book` ×1 out of sight of the start position. Name the notable ones in `TestbedIds` (`ruin_chest`, `ruin_scroll`, `far_book`).

- [ ] **Step 1: Write failing tests**: `explorer_loads_with_one_controlled_actor` (one unit named `explorer`, controller `explorer` attached in Direct mode); `explorer_compound_ids_match_testbed` (ids of `gate`, `workshop_door`, `store_door`, `store_key` equal those in `testbed:smithy`); `explorer_outside_features_exist` (at least 12 trees, 3 ore rocks, `ruin_chest` and `ruin_scroll` outside the compound bounds); `explorer_actor_cannot_see_far_book_at_start` (after 10 ticks the actor's `PerceptionMemory` lacks `far_book`); `explorer_reaches_the_ruin` (an `IssueOrder` Move to the ruin's gap completes within 2400 ticks).
- [ ] **Step 2: Run** `cargo test -p testbed_content --test scenarios explorer` — Expected: FAIL.
- [ ] **Step 3: Implement** `Layout::Frontier`, `spawn_frontier`, `explorer.json`, the embedded entry, and the README row.
- [ ] **Step 4: Run** `cargo test -p testbed_content` — Expected: PASS (corpus and existing scenario tests unchanged).
- [ ] **Step 5: Add** `explorer` to the Task 2 `resume_equals_uninterrupted_every_embedded_scenario` list and run it — Expected: PASS.
- [ ] **Step 6: Commit** — `feat(testbed): frontier layout and the explorer scenario`

### Task 6: Prompt templates

**Files:**
- Create: `content/testbed_content/prompts/start.md`, `resume.md`, `resume_changed.md`, `resume_rebuilt.md`, `common.md`
- Create: `content/testbed_content/src/explorer/prompt.rs`
- Test: unit tests in `prompt.rs`

**Interfaces:**
- Produces: `pub enum PromptKind { Start, Resume, ResumeChanged, ResumeRebuilt }`; `pub struct PromptVars { pub agent: String, pub world: String, pub url: String, pub unit: u32, pub npcctl: PathBuf, pub dir: PathBuf, pub npcctl_missing: bool }`; `pub const CONTROLLER: &str = "explorer";` `pub fn render_prompt(kind: PromptKind, v: &PromptVars) -> String`.
- Placeholders: `{AGENT} {WORLD} {URL} {UNIT} {CONTROLLER} {NPCCTL} {DIR} {DIARY} {PROPOSALS}`; `common.md` is appended to every variant; the resume variants start with their own preamble.
- Content: spec §6 point by point (who you are; real time ~20 ticks/s; mm, +z north; observe/act/events commands with `--url {URL} --unit {UNIT} --controller {CONTROLLER}`, `--format text --brief`, `--expect-tick/--expect-seq`, `--until-idle`; walking and interacting as in `.claude/skills/play-actor/SKILL.md` §2; the MCP alternative with a `.mcp.json` snippet using `{NPCCTL}` and `{URL}`; rules: only `agent/*`, never `world.mutate_*`, `world.insert_*`, `npc/command`, `npc/pause`, `npc/step`, speed changes, never edit game files; diary format: append-only `## tick T — title` entries; proposals: `{PROPOSALS}/NNN-<slug>.md` with the header lines `title:`, `status: new`, `written: tick T, <date>`, `prompted by:`; only the dev changes `status` to `liked`/`pursuing`/`rejected`; never re-propose an existing idea; no fixed goal, write to the diary at least every few actions). `ResumeChanged`: "your world was saved by an older version of the game and may have changed; check what you remember". `ResumeRebuilt`: "your world was rebuilt from scratch; what you remember may no longer be there".
- When `npcctl_missing`, the prompt keeps the path and adds one line: "npcctl is not built yet; the user must run `cargo build -p npcctl`".

- [ ] **Step 1: Write failing tests**: `every_variant_fills_every_placeholder` (no `{` + uppercase + `}` left, for all four kinds); `start_names_paths_and_unit` (contains the url, `--unit 33`, `--controller explorer`, the diary and proposals paths); `resume_variants_say_read_diary_first`; `rules_forbid_mutation` (contains `world.mutate_*` and `npc/command` in the forbidden list).
- [ ] **Step 2: Run** `cargo test -p testbed_content explorer::prompt` — Expected: FAIL.
- [ ] **Step 3: Implement** with `include_str!` and plain `str::replace`.
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(testbed): explorer prompt templates`

### Task 7: Agent folders, names, settings and diary/proposal parsing

**Files:**
- Create: `content/testbed_content/src/explorer/mod.rs`, `names.rs`, `home.rs`, `notes.rs`, `settings.rs` (std + serde only; no Bevy, no wall clock: callers pass seeds and dates)
- Modify: `content/testbed_content/src/lib.rs` (`pub mod explorer;`)
- Test: unit tests in each file

**Interfaces:**
- `names.rs`: `pub enum NameKind { Agent, World }`; `pub fn random_name(kind: NameKind, seed: u64) -> String` (two word lists per kind, ≥ 40 words each, e.g. agents "Wren Ashdown", worlds "Hollowmere"; picked with `npc_framework::kernel::rng::sim_rand`-style SplitMix on `seed`).
- `home.rs`: `pub fn slug(name: &str) -> String` (ASCII lowercase letters and digits, every other run becomes one `-`, trimmed, at most 48 chars, empty → `unnamed`); `pub struct AgentDir { pub dir: PathBuf }` with `start_prompt()`, `resume_prompt()`, `diary()`, `proposals()`, `meta()` (`agent.json`), `save(world: &str) -> PathBuf` (`<slug(world)>.world.json`), `backup_save(world: &str, date: &str) -> std::io::Result<PathBuf>` (renames to `<slug>.world.<date>.json`); `#[derive(Serialize, Deserialize)] pub struct AgentMeta { pub agent: String, pub world: String, pub created: String }`; `pub fn create_agent(root: &Path, agent: &str, world: &str, created: &str) -> Result<AgentDir, String>` (creates the folder, `proposals/`, an empty `diary.md` with a `# Diary of {agent}` line, and `agent.json`; refuses an existing folder naming it; an unwritable root → error naming the root); `pub struct AgentEntry { pub meta: AgentMeta, pub dir: AgentDir, pub has_save: bool }`; `pub fn list_agents(root: &Path) -> Vec<AgentEntry>` (sorted by agent name; folders without a readable `agent.json` skipped).
- `notes.rs`: `pub struct DiaryEntry { pub heading: String, pub body: String }`; `pub fn diary_entries(text: &str) -> Vec<DiaryEntry>` (split on lines starting with `## `, newest first, text before the first `## ` ignored); `pub struct Proposal { pub file: String, pub title: String, pub status: String, pub written: String }`; `pub fn parse_proposal(file: &str, text: &str) -> Proposal` (header lines `key: value` until the first blank line; missing title → file stem, missing status → `new`); `pub fn read_proposals(dir: &Path) -> Vec<Proposal>` (`*.md`, sorted by file name, unreadable or non-UTF-8 files skipped); `pub fn read_lossy(path: &Path) -> String` (missing → empty, invalid UTF-8 → lossy).
- `settings.rs`: `#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)] pub struct Settings { pub agents_dir: PathBuf }`; `impl Settings { pub fn default_for(home: &Path) -> Self; pub fn file(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf>; pub fn load(path: &Path, home: &Path) -> Self; pub fn save(&self, path: &Path) -> std::io::Result<()> }` (`load` falls back to the default on a missing or bad file; `file` uses `APPDATA`, else `XDG_CONFIG_HOME`, else `HOME/.config`, plus `npc-framework/settings.json`); `pub fn validate_agents_dir(p: &Path) -> Result<(), String>` (exists and is writable, or can be created).

- [ ] **Step 1: Write failing tests** (one per behaviour; the Review Focus ones by name): `slug_strips_illegal_characters` (`"a/b\\c:d*e?f\"g<h>i|j"` → `"a-b-c-d-e-f-g-h-i-j"`), `slug_of_blank_is_unnamed`, `slug_caps_length`, `random_name_is_stable_per_seed_and_varies`, `create_agent_writes_layout`, `create_agent_refuses_existing` (second create with a name that slugs the same → `Err` naming the folder), `create_agent_reports_unwritable_root`, `list_agents_skips_folders_without_meta`, `diary_entries_newest_first`, `diary_entries_tolerate_empty_and_no_headings`, `diary_entries_tolerate_half_written_last_entry`, `parse_proposal_defaults`, `read_proposals_skips_unreadable` (a non-UTF-8 file and a directory named `x.md` are skipped), `settings_round_trip_and_fallback`, `settings_file_prefers_appdata`.
- [ ] **Step 2: Run** `cargo test -p testbed_content explorer` — Expected: FAIL.
- [ ] **Step 3: Implement** the four files.
- [ ] **Step 4: Run** — Expected: PASS; `cargo clippy -p testbed_content -- -D warnings` clean (the determinism lints in `clippy.toml` must not need an allow; if one fires on file I/O, stop and ask).
- [ ] **Step 5: Commit** — `feat(testbed): explorer agent folders, names, settings, notes`

---

## Phase C: Explorer mode in the Actor Testbed

### Task 8: Explorer flags, New and Resume startup

**Files:**
- Create: `apps/actor_testbed/src/explorer.rs`
- Modify: `apps/actor_testbed/src/main.rs:6-35,63-207,269-310` (usage, `Args`, `parse_args`, `main`)
- Test: unit tests in `explorer.rs`

**Interfaces:**
- Consumes: Task 4 `snapshot::{read, check, load, LoadMode}`; Task 5 `explorer` scenario; Task 7 `explorer::*`.
- Flags: `--explorer-home DIR --agent NAME [--world NAME] [--resume [--load-anyway | --fresh-world]] [--check-save]`. `--agent` without `--resume` is New and needs `--world`. With `--explorer-home`, `--scenario` must not be given (usage error).
- Produces: `pub struct ExplorerArgs { pub home: PathBuf, pub agent: String, pub world: Option<String>, pub resume: bool, pub on_mismatch: Option<Mismatch>, pub check_only: bool }`; `pub enum Mismatch { LoadAnyway, FreshWorld }`; `pub enum Start { New, Resumed(LoadReport), ResumedChanged(LoadReport), Rebuilt }`; `pub fn build_explorer_app(args: &ExplorerArgs, host: impl FnOnce(&mut App)) -> Result<(App, Start, AgentDir), String>`:
  - New: `create_agent`, scenario `explorer` with the actor's name set to the agent name, seed from wall-clock nanos (app code, not sim), written into the snapshot header later.
  - Resume: read `agent.json` and the save; build from the save's scenario + seed (stored in `header.scenario` and `header.seed`); `check`; on OK `load(Strict)`; on mismatch without `on_mismatch` → `Err("save from another build: … use --load-anyway or --fresh-world")` (exit code 3); `LoadAnyway` → `load(Lenient)`; `FreshWorld` → `backup_save` the old file, keep the freshly built world.
  - `--check-save` prints `{"ok":true}` or `{"ok":false,"error":"…","kind":"content_hash"|"version"|"unreadable"}` and exits 0/1 without opening a window.
- The sim starts paused (`SimControl.paused = true`); the Task 10 modal unpauses.

- [ ] **Step 1: Write failing tests** (headless; `host` adds nothing): `new_creates_folder_and_names_actor` (TestbedIds has the agent's name; `agent.json` written); `new_refuses_existing_agent`; `resume_restores_tick_and_position` (New → run 300 ticks → save via Task 9's `save_now` or `snapshot::write_atomic` → Resume → tick 300, same `SimPos`); `resume_with_other_build_needs_a_choice` (corrupt the content hash in the file → `Err` mentioning both flags); `fresh_world_backs_up_old_save`; `check_save_reports_kind`.
- [ ] **Step 2: Run** `cargo test -p actor_testbed explorer` — Expected: FAIL.
- [ ] **Step 3: Implement** `explorer.rs` and the flag wiring in `main.rs` (exit 2 for usage, 3 for a save that needs a choice).
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(actor_testbed): explorer mode startup (new, resume, check-save)`

### Task 9: Port, prompt files and autosave

**Files:**
- Modify: `apps/actor_testbed/src/explorer.rs`, `apps/actor_testbed/src/main.rs` (`debug_tools::install`)
- Test: unit tests in `explorer.rs`

**Interfaces:**
- Produces: `pub fn pick_port(first: u16, last: u16) -> Option<u16>` (first port where `TcpListener::bind(("127.0.0.1", p))` succeeds; the listener is dropped before BRP binds); `pub fn write_prompts(dir: &AgentDir, kind: PromptKind, vars: &PromptVars) -> std::io::Result<String>` (writes `start_prompt.md` for Start, else `resume_prompt.md`; returns the text); `pub fn npcctl_path() -> (PathBuf, bool)` (sibling of `current_exe`, `npcctl.exe` on Windows; `bool` = missing); `#[derive(Resource)] pub struct Autosave { pub path: PathBuf, pub every: Duration, pub last: Instant, pub header: Map<String, Value> }`; system `autosave` (wall clock; view-side `Update`, after the driver set, never mid-tick) calling `snapshot::save` + `write_atomic`; and a save on `AppExit`.
- Snapshot header: `agent`, `world`, `scenario` (the scenario doc as JSON), `seed`, `saved` (RFC 3339 wall-clock date).

- [ ] **Step 1: Write failing tests**: `pick_port_skips_taken` (bind 15702 in the test, expect `Some(15703)` or later); `write_prompts_writes_start_file`; `resume_rewrites_resume_file` (two writes, the second wins); `autosave_round_trips` (headless app, run 200 ticks, call the autosave fn directly, `read` + `check` succeed and `tick == 200`).
- [ ] **Step 2: Run** `cargo test -p actor_testbed explorer` — Expected: FAIL.
- [ ] **Step 3: Implement**; in explorer mode force `RemoteArgs.remote = true` with the picked port (exit 2 with a message when no port in 15702..=15721 is free).
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(actor_testbed): explorer port, prompt files, autosave`

### Task 10: The prompt modal

**Files:**
- Create: `content/testbed_view/src/explorer_modal.rs`
- Modify: `content/testbed_view/src/lib.rs` (module, exports), `content/testbed_view/Cargo.toml` and `apps/actor_testbed/Cargo.toml` (Bevy features for clipboard / ui_widgets if needed: keep both feature lists in sync as their comments demand), `apps/actor_testbed/src/main.rs` (insert the modal, unpause on close)

**Interfaces:**
- Produces: `#[derive(Resource)] pub struct PromptModal { pub title: String, pub text: String, pub notes: Vec<String>, pub open: bool }` (notes: the port, a missing `npcctl`, a lenient-load report); `#[derive(Message)] pub struct PromptModalClosed;` `pub struct ExplorerModalPlugin;` — a centred panel (max 900 px wide, scrollable text, monospace 13 px), **Copy** (writes `text` to the clipboard via `bevy_clipboard`; the button label shows "Copied" for 2 s; on a clipboard error a note "copy failed: select the text in <path> instead"), **OK** (closes, sends `PromptModalClosed`). The app adds a system: on `PromptModalClosed`, `SimControl.paused = false`. The modal blocks the RTS command UI's clicks while open.
- Title per start: New "Your agent's start prompt"; Resume "Resume prompt"; changed/rebuilt titles say so.

- [ ] **Step 1: Implement** the plugin and its app wiring (Bevy 0.20 APIs from the cheatsheet / registry source: `ui_widgets` buttons, `bevy_clipboard`).
- [ ] **Step 2: Verify visually**: `cargo run -p actor_testbed -- --explorer-home target/npc-out/agents --agent "Test Wren" --world "Hollowmere" --screenshot explorer_modal.png --after-secs 6`; Read the PNG: the modal is centred with the prompt, Copy and OK visible, the world behind it, tick 0 in the F1 overlay.
- [ ] **Step 3: Verify the flow by hand**: Copy, paste somewhere (matches `start_prompt.md`), OK unpauses (tick advances).
- [ ] **Step 4: Run** `cargo xtask verify --quick` — Expected: `"ok":true`.
- [ ] **Step 5: Commit** — `feat(testbed_view): explorer prompt modal`

### Task 11: The F7 diary and proposals panel

**Files:**
- Create: `content/testbed_view/src/explorer_panel.rs`
- Modify: `content/testbed_view/src/lib.rs`, `apps/actor_testbed/src/main.rs` (insert `ExplorerFiles` in explorer mode)
- Test: unit test of the text builder in `explorer_panel.rs`

**Interfaces:**
- Consumes: Task 7 `diary_entries`, `read_proposals`, `read_lossy`.
- Produces: `#[derive(Resource)] pub struct ExplorerFiles { pub dir: PathBuf }`; `pub fn panel_text(diary: &[DiaryEntry], proposals: &[Proposal], max_entries: usize) -> String` (pure; the 8 newest diary entries, each body cut to 6 lines, then "Proposals" with `[status] title` lines; empty states "No diary entries yet" / "No proposals yet"). F7 toggles; a 1 s timer re-reads only when a file's modification time changed. Panel style as `panels.rs::panel_text` (right side, max width 560 px). Only present when `ExplorerFiles` exists.

- [ ] **Step 1: Write failing test** `panel_text_shows_newest_and_statuses` (3 entries + 2 proposals → newest heading first, `[liked]` present) and `panel_text_empty_states`.
- [ ] **Step 2: Run** `cargo test -p testbed_view explorer_panel` — Expected: FAIL.
- [ ] **Step 3: Implement** the panel.
- [ ] **Step 4: Run** — Expected: PASS. Then visually: write a sample `diary.md` and two proposals into a test agent folder, run with `--panels` extended to accept `explorer` (add it to `parse_panels`) and `--screenshot explorer_panel.png --after-secs 6`; Read the PNG.
- [ ] **Step 5: Commit** — `feat(testbed_view): F7 explorer diary and proposals panel`

---

## Phase D: The game menu

### Task 12: The `game_menu` app: main screen, settings, launching

**Files:**
- Create: `apps/game_menu/Cargo.toml`, `apps/game_menu/src/main.rs` (window, screens as a Bevy `States` enum), `apps/game_menu/src/launch.rs` (pure: entries, executable discovery, arguments), `apps/game_menu/src/text_field.rs` (a minimal single-line text input, unless Bevy 0.20 ships one: check first)
- Modify: workspace `Cargo.toml` (member), `content/testbed_content/src/scenario.rs` (`pub fn embedded_scenarios() -> &'static [(&'static str, &'static str)]`: name and one-line description, with a unit test that every embedded name has a description)
- **Stop and ask the user** to append `"game_menu"` to the `bevy` entry's `wrappers` in `deny.toml` (protected; `docs/adding-a-host-app.md`), stating the three points that doc lists.

**Interfaces:**
- `launch.rs`: `pub enum Entry { Village, Testbed(&'static str), RtsDemo, ExplorerNew { agent: String, world: String }, ExplorerResume { agent: String, choice: Option<Mismatch> } }` (`Mismatch` re-declared here as `LoadAnyway`/`FreshWorld`, mapped to flags); `pub fn command_for(entry: &Entry, settings: &Settings) -> (&'static str, Vec<String>)` (exe stem + args, e.g. `("actor_testbed", ["--scenario","doors"])`, `("actor_testbed", ["--explorer-home",<dir>,"--agent",<name>,"--world",<name>])`); `pub fn find_exe(stem: &str, dir: &Path) -> Option<PathBuf>`; `pub fn build_command(stem: &str) -> Vec<String>` (`["cargo","build","-p",stem]`).
- Screens: Main (Explore with an AI agent · Actor Testbed · Village · RTS units demo · Settings · Quit), Testbed (one button per `embedded_scenarios()` entry with its description, plus Back), Settings (agents folder text field, Save validates with `validate_agents_dir`, error shown inline), Running ("<name> is running", the window hidden via `Window.visible = false` while the child runs; `Child::try_wait` each frame; on exit the window shows again and a non-zero exit code is shown as a one-line error on Main), Missing ("<stem> is not built", **Build** runs `cargo build -p <stem>` as a child with a "building…" state when `cargo` and the workspace root are found, else the command is shown to copy).
- Explorer entry disabled with "set a valid agents folder in Settings" while `validate_agents_dir` fails.
- `--screenshot PATH --after-secs S --screen main|testbed|settings|explorer_new|explorer_resume` for visual checks.

- [ ] **Step 1: Write failing tests** in `launch.rs`: `command_for_testbed_scenario`, `command_for_explorer_new`, `command_for_resume_with_choice` (`--load-anyway` / `--fresh-world`), `find_exe_adds_windows_suffix`, `explorer_disabled_without_valid_folder` (a pure `fn explorer_enabled(settings: &Settings) -> Result<(), String>`); in `scenario.rs`: `every_embedded_scenario_has_a_description`.
- [ ] **Step 2: Run** `cargo test -p game_menu` and `cargo test -p testbed_content embedded` — Expected: FAIL.
- [ ] **Step 3: Implement** the crate (after the user's `deny.toml` edit).
- [ ] **Step 4: Run** the tests — Expected: PASS. Visual: `cargo run -p game_menu -- --screen main --screenshot menu_main.png --after-secs 3`, the same for `testbed` and `settings`; Read each PNG.
- [ ] **Step 5: Commit** — `feat: game_menu app with settings and launching`

### Task 13: Explorer New and Resume screens

**Files:**
- Modify: `apps/game_menu/src/main.rs`, `apps/game_menu/src/launch.rs`

**Interfaces:**
- Consumes: Task 7 `random_name`, `list_agents`, `slug`; Task 8 `--check-save` JSON.
- New: two text fields prefilled with `random_name(Agent/World, seed)` (seed from wall-clock nanos), a dice button per field for another random name, **Start** disabled while a name is blank or `slug(agent)` names an existing agent folder (inline "an agent with this name exists: resume it instead").
- Resume: a list from `list_agents` (agent, world, last saved date from the save header, "no save yet" when `has_save` is false and **Resume** disabled). On Resume: run `actor_testbed --explorer-home … --agent … --check-save` (no window). `ok` → launch. Not ok with `kind` `content_hash`/`version` → a dialog "This world was saved by another version of the game: <error>" with the two buttons **Load anyway** and **Fresh world, keep the diary**, each launching with its flag. `unreadable` → the error with only **Fresh world, keep the diary**.
- Produces: `pub fn parse_check(stdout: &str) -> CheckResult` (`Ok`, `Mismatch(String)`, `Unreadable(String)`; bad JSON → `Unreadable`).

- [ ] **Step 1: Write failing tests**: `parse_check_ok`, `parse_check_mismatch`, `parse_check_garbage_is_unreadable`, `start_blocked_for_existing_agent` (pure `fn new_world_problem(root, agent, world) -> Option<String>`).
- [ ] **Step 2: Run** `cargo test -p game_menu` — Expected: FAIL.
- [ ] **Step 3: Implement**.
- [ ] **Step 4: Run** — Expected: PASS. Visual: `--screen explorer_new` and `--screen explorer_resume` (with a test agents folder holding two agents) screenshots; Read them.
- [ ] **Step 5: Commit** — `feat(game_menu): explorer new and resume screens`

---

## Phase E: Docs and the end-to-end run

### Task 14: Docs, end-to-end check, full verify

**Files:**
- Modify: `README.md` (quick start: `cargo run -p game_menu` first; the explorer in the showcase table), `docs/agent-experiments.md` (new section "The AI explorer": the flow, the folder layout, the prompt variants, how resume and the two buttons behave), `content/testbed_content/README.md` (explorer row from Task 5), `CLAUDE.md` crate map row for `apps/game_menu` and the snapshot line in the `npc_kernel` row, `CHANGELOG.md`

- [ ] **Step 1: Write the docs.**
- [ ] **Step 2: Build everything the menu launches**: `cargo build -p game_menu -p actor_testbed -p village -p rts_units_demo -p npcctl` — Expected: success.
- [ ] **Step 3: End-to-end by hand** (spec §7): start the menu, Settings → a test agents folder, Explore → New, copy the prompt, paste it into a fresh Claude Code session in another terminal; watch for 10 minutes: the actor moves, `diary.md` grows, at least one proposal appears, F7 shows both. Quit (autosave), Resume from the menu: the actor stands where it was, the resume prompt says to read the diary first, the agent continues. Record what happened in `docs/_notes/explorer-e2e.md`.
- [ ] **Step 4: Run** `cargo xtask verify` — Expected: `"ok":true` (quote the summary line).
- [ ] **Step 5: Commit** — `docs: game menu and AI explorer`
