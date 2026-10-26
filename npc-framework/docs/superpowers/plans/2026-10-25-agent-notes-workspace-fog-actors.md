# Agent Notes, Agent Workspace, Detached Mode, Fog of War and Actor Presets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Game-owned agent notes (diary, memory, proposals), a prepared MCP-only agent folder, detached worlds that can move into the window, Age-of-Empires fog of war, and actor presets (human and robots with realistic, ROS-shaped sensors) built in an actor editor with a 3D preview.

**Architecture:** Five phases, each shippable on its own. (A) A host-side notes store in `npc_agent_io` exposed as agent and player methods over BRP, the headless session, `npcctl` and MCP, with an F7 editor. (B) `testbed_content::explorer::workspace` generates the agent folder (`AGENTS.md`, prompts, `.claude/` settings, `.mcp.json`). (C) Window-free explorer startup moves into `testbed_content::explorer::host`; `npcctl serve` hosts detached worlds; the menu starts, stops and moves them. (D) A snapshot-only per-actor fog record updated from perception, drawn by `testbed_view`. (E) Actor presets as data, new perception sensors (LIDAR, camera, radar) with ROS-shaped readings, robot bodies, and an actor-editor mode in `actor_testbed`.

Later additions: one minute of perception memory in explorer worlds (Task 20, Phase C) and (H) the actor's map of landmarks after Phase G, from `2026-10-25-actor-map-and-memory-design.md`; notes as one Obsidian file per entry (Task 24, Phase C), from `2026-10-25-notes-as-obsidian-files-design.md`; the agent feedback increment (Tasks 25–26 in Phase C, 27–28 in Phase D), from `2026-10-25-agent-feedback-increment-design.md`.

**Tech Stack:** Rust ≥ 1.97.1, Bevy 0.20, serde/serde_json, the npc-framework facade, Claude Code project config.

**Spec:** `docs/superpowers/specs/2026-10-25-agent-notes-and-workspace-design.md` (builds on `2026-10-24-game-menu-and-ai-explorer-design.md`).

## Global Constraints

- `CLAUDE.md` golden rules: determinism (no wall clock, `HashMap` order or `Entity` order in sim code; integers in sim state), core/content boundary, no host hijack, protected oracles, `cargo xtask verify` prints `"ok":true`.
- Protected files are never edited by an implementer (`deny.toml`, `clippy.toml`, `xtask/src/main.rs`, `xtask/tests/arch_fitness.rs`, goldens, `**/snapshots/**`, `tests/replays/**`, `bench/baseline.json`, `.claude/**`); stop and ask with the exact change.
- **No existing state hash, content hash, replay or golden may change.** `register_sim_component` adds a presence byte for every `SimId` entity and enters the content hash, so every new per-entity state in this plan is registered **snapshot-only** (`register_snapshot_component_named` / `_with`, ADR 0013), never through `register_sim_component`. New enum variants (e.g. `Sensor`) must not change the encoding of existing variants.
- Notes are host-side: never sim state, never hashed, never in the snapshot; a note write must not change the sim.
- Note limits: title ≤ 200 chars, body ≤ 8192 bytes. Proposal statuses: `new`, `liked`, `pursuing`, `rejected`; only a player sets `status`.
- Agent folder generated files (rewritten every start/resume): `AGENTS.md`, `CLAUDE.md`, `prompts/`, `.claude/settings.json`, `.claude/settings.local.json`, `.mcp.json`. Owned files (never rewritten by generation): `diary/`, `memory/`, `proposals/`, `map/` (one Obsidian file per entry from Task 24; before it `diary.md`, `memory.md`), `agent.json`, `*.world.json`, `run.json` (written only by the running host).
- Claude Code config (verified against the Claude Code docs): `.claude/settings.json` = `{"permissions":{"defaultMode":"dontAsk","allow":["mcp__npc__*","Read(./**)"],"deny":["Bash","PowerShell","Edit","Write","NotebookEdit","WebFetch","WebSearch","Monitor"],"blockReadsOutsideWorkingDirectories":true}}`; `.claude/settings.local.json` = `{"enabledMcpjsonServers":["npc"]}`; `.mcp.json` = `{"mcpServers":{"npc":{"type":"stdio","command":"<npcctl path, backslashes JSON-escaped>","args":["mcp","--url","<url>"]}}}`; `CLAUDE.md` = `@AGENTS.md`.
- Exception to the hash rule (map/memory spec M1, Task 20): explorer worlds' `PerceptionConfig` changes to 1200 ticks / 128 things; no replay or golden covers the `explorer` scenario, and every other scenario keeps its hash.
- Map: landmarks are host-side files `map/<name>.md` in Task 24's Obsidian format (owned, never generated, never in the save, not sim state); name ≤ 200 chars, note ≤ 8192 bytes; no delete; Move orders only ever record `pos`.
- Handover port: the window retries the detached world's port for up to 10 s and never moves to another port.
- Copy and paste only through `testbed_content::explorer::clipboard`; never `taskkill /IM`; never send real OS input to the desktop.
- Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`; a CHANGELOG `[Unreleased]` line per behaviour change.
- **Phase gate (the dev's rule): every successful phase produces a build artifact on the main checkout before the next phase starts.** At the end of each phase (A–E, I, G, H and F): (1) `cargo xtask verify` prints `"ok":true` on the branch; (2) merge the branch into `main` in the main checkout (repository root `C:\Projects`) with `git merge --no-ff` (only after checking the index is clean and none of the dev's uncommitted files are under `npc-framework/`; never touch them); (3) in `C:\Projects\npc-framework` run `cargo build --release -p game_menu -p actor_testbed -p npcctl -p village -p rts_units_demo` and report the binaries' paths (`target\release\*.exe`); (4) only then start the next phase. A failing step stops the phase; nothing is merged.

## Review Focus

1. The agent and the dev edit the same note at once (agent over MCP while the F7 editor is open): the second save is rejected with "changed since you read it" and nothing is overwritten. → Task A1 `stale_update_is_rejected`, Task A3 `editor_reports_stale_save`.
2. A diary written by today's game (old `## tick T - title` entries, no ids) or edited by hand (stray text, a BOM, CRLF): the store reads it, assigns ids once, keeps unplaceable text verbatim, and an update never drops it. → Task A1 `old_diary_migrates_once`, `unplaceable_text_survives_update`.
3. "Open in window" while the detached world's save fails or its port is still held: the world keeps running detached and the menu says why; no world state or note is lost. → Task C3 `open_in_window_aborts_on_failed_save`, Task C2 `exact_port_retries_then_exits_4`.
4. A preset file edited by hand into something invalid (unknown sensor, negative range, over-budget stats, missing model): the editor and the New screen show it as invalid with the reason and the game never spawns it. → Task E1 `invalid_presets_are_reported`.
5. Fog with the actor inside a building at night, behind closed doors, and after a resume: nothing outside its line of sight is revealed, explored cells survive save/resume, units in explored-but-not-visible areas stay hidden. → Task D1 `fog_respects_line_of_sight`, `explored_survives_resume`; Task D2 `units_hidden_outside_visible`.

---

## Phase A: Notes through the game

### Task 1 (A1): The notes store

**Files:**
- Create: `crates/npc_agent_io/src/notes.rs`, `crates/npc_agent_io/src/notes/format.rs`
- Modify: `crates/npc_agent_io/src/lib.rs` (module, exports)
- Test: `crates/npc_agent_io/tests/notes.rs`

**Interfaces:**
- Produces: `#[serde(rename_all = "snake_case")] pub enum Book { Diary, Memory, Proposals }`; `pub enum Writer { Agent, Player }`; `#[serde(rename_all = "snake_case")] pub enum ProposalStatus { New, Liked, Pursuing, Rejected }`; `#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)] pub struct Stamp { pub tick: u64, pub date: String }`; `pub struct Note { pub id: u32, pub title: String, pub body: String, pub created: Stamp, pub updated: Stamp, pub status: Option<ProposalStatus>, pub prompted_by: Option<String> }` (serde); `pub struct NewNote { pub title: String, pub body: String, pub prompted_by: Option<String> }`; `pub struct NoteChange { pub title: Option<String>, pub body: Option<String>, pub status: Option<ProposalStatus> }`; `pub enum NotesError { TooLong { field: &'static str, max: usize }, NotFound(u32), Stale { id: u32, current: Stamp }, Forbidden(&'static str), Io(String) }` with `Display` ("changed since you read it" for `Stale`); `pub const TITLE_MAX: usize = 200; pub const BODY_MAX: usize = 8192;`
- `pub struct NotesStore` with `pub fn open(dir: &Path) -> Result<Self, NotesError>`, `pub fn list(&mut self, book: Book) -> Result<Vec<Note>, NotesError>` (oldest first), `pub fn add(&mut self, book: Book, by: Writer, new: NewNote, at: Stamp) -> Result<Note, NotesError>`, `pub fn update(&mut self, book: Book, by: Writer, id: u32, expected: &Stamp, change: NoteChange, at: Stamp) -> Result<Note, NotesError>`. Before every operation a file whose mtime changed is re-read. Writes are atomic (temp + rename). `Writer::Agent` setting `status` → `Forbidden("only the player sets a proposal's status")`.
- Files: `diary.md`, `memory.md`: a `## <title>` heading, then `<!-- note id=N created=TICK@DATE updated=TICK@DATE -->`, then the body; text before the first heading is the preamble, kept verbatim. Old headings `## tick T - title` without a comment: id assigned in file order on first read and written back once; `created`/`updated` = tick T, date "". `proposals/NNN-<slug>.md`: header lines `title:`, `status:`, `written:`, `prompted by:`, plus `id:` and `updated:` added on migration; unknown header lines kept; `NNN` = id. Strip a leading BOM; accept CRLF.

- [ ] **Step 1: Write failing tests** in `crates/npc_agent_io/tests/notes.rs`: `add_then_list_round_trips` (each book), `ids_are_sequential_per_book`, `limits_are_enforced` (201-char title, 8193-byte body → `TooLong`), `stale_update_is_rejected` (update with an old `expected` → `Stale` with the current stamp, file unchanged), `agent_cannot_set_status` (`Forbidden`), `player_sets_status`, `old_diary_migrates_once` (fixture with two `## tick 40 - Woke up` entries → ids 1,2, the file now carries the comments, a second open does not rewrite it), `old_proposal_migrates` (a Task-6-format proposal), `unplaceable_text_survives_update` (preamble + a heading-less block kept byte-for-byte after an update), `external_edit_is_picked_up` (edit the file on disk, `list` shows it), `bom_and_crlf_are_accepted`.
- [ ] **Step 2: Run** `cargo test -p npc_agent_io --test notes` — Expected: FAIL (module missing).
- [ ] **Step 3: Implement** `notes.rs` (store, mtime cache) and `notes/format.rs` (parse/render per book).
- [ ] **Step 4: Run** the tests — Expected: PASS; `cargo clippy -p npc_agent_io --all-targets -- -D warnings` clean.
- [ ] **Step 5: Commit** — `feat(agent_io): notes store (diary, memory, proposals)`

### Task 2 (A2): Notes methods, CLI and MCP

**Files:**
- Modify: `crates/npc_agent_io/src/api.rs` (methods, dispatch, params; factor the controller check out of `act` at `api.rs:677-708` into `fn check_controller(world, unit, controller) -> Result<(), Rejection>`), `crates/npc_agent_io/src/mcp.rs`, `apps/npcctl/src/agent.rs`, `apps/npcctl/src/main.rs`, `docs/debugging.md` §4
- Test: `crates/npc_agent_io/tests/notes_api.rs`, `apps/npcctl/tests/agent.rs`

**Interfaces:**
- Consumes: Task A1.
- Produces: resource `#[derive(Resource, Default)] pub struct NotesDirs(pub BTreeMap<SimId, PathBuf>)` (the host maps a unit to its agent folder; a store per folder is opened lazily and kept in a non-hashed `NotesStores` resource). Methods (added to `METHODS` and `api::call`, so BRP and the session pick them up):
  - Agent (controller required, same check as `act`): `agent/notes/list {unit, controller, book}`, `agent/notes/add {unit, controller, book, title, body, prompted_by?}`, `agent/notes/update {unit, controller, book, id, expected: {tick, date}, title?, body?}`.
  - Player (no controller; writer = player): `notes/list {unit, book}`, `notes/add {unit, book, title, body, prompted_by?}`, `notes/update {unit, book, id, expected, title?, body?, status?}`.
  - A unit absent from `NotesDirs` → RPC invalid "unit N keeps no notes". `at` stamps use the current `SimClock.tick` and a host-provided date fn resource `NotesClock(fn() -> String)` (the host passes wall-clock RFC 3339; tests pass a fixed fn).
  - MCP tools: `notes_list`, `notes_add`, `notes_update` (agent methods; `controller` defaulted from the MCP server's `--controller` arg, default `explorer`), and the missing fields on existing tools: `act.steps`, `observe.brief`, `events.until_idle`, `events.dedupe`, `events.min_events` (schemas list every field; the existing exact-name test in `mcp.rs` is updated).
  - CLI: `npcctl agent notes list|add|update --unit --controller --book ...` and `npcctl notes list|add|update --unit --book [--status] ...`.

- [ ] **Step 1: Write failing tests**: `agent_writes_need_the_controller` (`not_attached` / `not_controller` rejections), `player_methods_need_no_controller`, `agent_cannot_pose_as_player` (agent method with `status` field → invalid params), `unit_without_notes_is_an_error`, `note_writes_leave_the_state_hash_unchanged` (testbed app: hash before == after an add/update), `mcp_lists_every_field` (schema has `steps`, `brief`, `until_idle`, `dedupe`, `min_events`, and the three notes tools), npcctl: `notes_add_and_list_over_a_session` (serve the `explorer` scenario with a `NotesDirs` mapping via a test hook, add, list).
- [ ] **Step 2: Run** `cargo test -p npc_agent_io --test notes_api` and `cargo test -p npcctl --test agent` — Expected: FAIL.
- [ ] **Step 3: Implement**; update `docs/debugging.md` §4 with the six methods.
- [ ] **Step 4: Run** the tests and `cargo xtask verify --quick` — Expected: PASS / `"ok":true`.
- [ ] **Step 5: Commit** — `feat: notes agent and player methods, npcctl notes, MCP notes tools`

### Task 3 (A3): F7 becomes the notes editor; the explorer host maps its actor

**Files:**
- Modify: `content/testbed_view/src/explorer_panel.rs` (becomes the editor), `apps/actor_testbed/src/explorer.rs` (insert `NotesDirs { actor → agent dir }` and `NotesClock`), `content/testbed_content/src/explorer/notes.rs` (keep only what the menu still needs, or delete and switch callers to the store)
- Test: unit tests in `explorer_panel.rs`, `apps/actor_testbed` explorer tests

**Interfaces:**
- Consumes: Task A1 store (in-process, `Writer::Player`), Task A2 `NotesDirs`.
- The panel: three tabs (Diary, Memory, Proposals; F7 toggles, Tab cycles); a scrollable list (newest first) with title, stamps and status; **Add** opens title + body fields (Bevy `TextInput`, multi-line body); **Edit** on a selected entry; proposal rows get status buttons. Save calls `add`/`update` with the `updated` stamp read when editing began; `Stale` shows "changed meanwhile — reloaded" and reloads. While a panel text field has focus, `ViewSystems::Input` is gated (like the prompt modal's `modal_open`) so typing never triggers game keys. Same `ExplorerFiles`-style 1 s refresh via the store (external/agent edits appear).

- [ ] **Step 1: Write failing tests**: `editor_adds_a_diary_entry` (headless: activate Add, type via `TextEdit` events, Save → the store lists it), `editor_sets_a_proposal_status`, `editor_reports_stale_save` (store updated behind the editor's back → message, no overwrite), `typing_in_the_editor_blocks_game_keys`, actor_testbed: `explorer_maps_its_actor_to_the_agent_folder`.
- [ ] **Step 2: Run** `cargo test -p testbed_view explorer_panel` and `cargo test -p actor_testbed explorer` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests; screenshot `--panels explorer` with a migrated sample folder and view it.
- [ ] **Step 5: Commit** — `feat(testbed_view): F7 notes editor`

### Task 19 (A4): Act-and-wait and since-last-look observations (runs after Task 3, before Phase B)

**Files:**
- Modify: `crates/npc_agent_io/src/{api.rs,events.rs,observe.rs,remote.rs,session.rs,mcp.rs}`, `apps/npcctl/src/agent.rs`, `docs/debugging.md` §4, `CHANGELOG.md`
- Test: `crates/npc_agent_io/tests/act_and_wait.rs`, `apps/npcctl/tests/agent.rs`

**Interfaces:**
- `agent/act_and_wait {…every agent/act field…, until_idle?: bool (default true), kinds?: [EventKind], timeout_ms?: u64 (default 30000, max 120000), observe?: {format?: "json"|"text", brief?: bool}}` → `{act: <agent/act result>, events: <agent/events result for the seqs after the act>, observation: <agent/observe result>}`. A rejected act returns at once with `act.accepted == false` and no wait. Implemented as a long-poll like `agent/events`: a watching method on BRP (beside `agent/events` in `remote.rs`) and a parked poll in the session (`session.rs`), so it never blocks a frame.
- `agent/observe` gains `since_seq?: u64` → adds a `changes` section: `{sensed_new: [...], lost: [...], inventory_gained: [...], inventory_lost: [...], orders: [{order, result}], interactions: [...]}` built from the agent's event buffer since that seq, plus in text format a first line "Since your last look: …" (or "Nothing changed since your last look."). A `since_seq` older than the buffer → `changes.truncated: true`.
- CLI: `npcctl agent act … --wait [--kinds K,..] [--timeout-ms N]` and `npcctl agent observe … --since-seq S`. MCP: tool `act_and_wait` (all fields in its schema) and `observe.since_seq`.

- [ ] **Step 1: Write failing tests**: `act_and_wait_returns_events_and_observation` (testbed: Move order → result has `OrderDone` and the new position), `rejected_act_does_not_wait`, `act_and_wait_times_out` (`events.timed_out == true`, observation still returned), `act_and_wait_does_not_block_frames` (session keeps ticking while parked), `observe_since_reports_changes` (pick up an item → `inventory_gained`), `observe_since_old_seq_is_truncated`, `text_has_since_line`, `mcp_lists_act_and_wait`, npcctl `act_wait_flag`.
- [ ] **Step 2: Run** `cargo test -p npc_agent_io --test act_and_wait` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests and `cargo xtask verify --quick` — Expected: PASS / `"ok":true`.
- [ ] **Step 5: Commit** — `feat(agent_io): act-and-wait and since-last-look observations`

Task 4 (B1) then teaches the one-call loop in `AGENTS.md` and the prompts: `act_and_wait` for each step, `observe` with `since_seq` when only checking.

## Phase B: The prepared agent folder

### Task 4 (B1): Workspace generator

**Files:**
- Create: `content/testbed_content/src/explorer/workspace.rs`, `content/testbed_content/prompts/agents.md`
- Modify: `content/testbed_content/src/explorer/{mod.rs,home.rs,prompt.rs}`, `content/testbed_content/prompts/{common,start,resume,resume_changed,resume_rebuilt}.md`
- Test: unit tests in `workspace.rs`, `prompt.rs`

**Interfaces:**
- Produces: `pub struct WorkspaceVars { pub prompt: PromptVars, pub actor: ActorSummary }` (`ActorSummary { name, body, description, sensors: Vec<String>, actions: Vec<String> }`, filled from Task E1's preset later; until then the human scout's values); `pub const GENERATED: [&str; 6] = ["AGENTS.md", "CLAUDE.md", "prompts", ".claude/settings.json", ".claude/settings.local.json", ".mcp.json"];` `pub fn write_workspace(dir: &AgentDir, kind: PromptKind, vars: &WorkspaceVars) -> std::io::Result<()>` (writes every generated file atomically, never touches owned files); `AgentDir::prompt(kind) -> PathBuf` = `prompts/start.md` | `prompts/resume.md` | `prompts/resume_changed.md` | `prompts/resume_rebuilt.md` (replaces `start_prompt.md`/`resume_prompt.md`; old files are deleted on the next write).
- Content: settings exactly as in Global Constraints; `.mcp.json` command = the npcctl path with backslashes (JSON-escaped), `args: ["mcp","--url",URL,"--controller","explorer"]`; `AGENTS.md` (template `prompts/agents.md`): who you are, the world, your body and sensors (from `ActorSummary`), the MCP tools by name (`observe`, `act`, `events`, `attach`, `notes_list`, `notes_add`, `notes_update`) with argument examples, the notes rules (diary for what happened, memory for lasting facts, proposals for the dev; only through the tools; never files), "trust this folder when Claude Code asks, otherwise your tools are blocked", and the `npcctl` command lines for AI tools without MCP. The start/resume prompts shrink to: who you are, read `AGENTS.md`, then (resume) list your memory and the last diary entries with `notes_list`, observe, continue.

- [ ] **Step 1: Write failing tests**: `workspace_writes_every_generated_file`, `settings_are_mcp_only` (parse `.claude/settings.json`: allow == `["mcp__npc__*","Read(./**)"]`, deny contains `Bash`, `PowerShell`, `Edit`, `Write`, `WebFetch`; `defaultMode == "dontAsk"`; `blockReadsOutsideWorkingDirectories == true`), `mcp_json_is_valid_with_a_windows_path` (`C:\Program Files\npc\npcctl.exe` round-trips through serde), `claude_md_imports_agents_md`, `owned_files_are_never_touched` (pre-existing diary/save bytes unchanged), `old_prompt_files_are_removed`, prompt tests updated (`prompts_point_at_agents_md`, `resume_lists_memory_first`, no file-writing instructions remain).
- [ ] **Step 2: Run** `cargo test -p testbed_content explorer` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(testbed): prepared agent workspace (AGENTS.md, MCP-only Claude Code settings)`

### Task 5 (B2): Wire the workspace into the testbed and the menu

**Files:**
- Modify: `apps/actor_testbed/src/explorer.rs` (`write_prompts` → `write_workspace` on every start/resume, modal `file` = `prompts/<kind>.md`), `apps/game_menu/src/explorer.rs` (New shows "open your AI tool in <folder>"; Resume detail unchanged), `docs/agent-experiments.md` §9, `CHANGELOG.md`
- Test: actor_testbed explorer tests, game_menu tests

- [ ] **Step 1: Write failing tests**: `new_writes_the_workspace`, `resume_rewrites_generated_files_only`, `an_old_folder_gets_its_workspace_on_resume`, menu: `new_screen_names_the_agent_folder`.
- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests and `cargo xtask verify --quick` — Expected: PASS / `"ok":true`. Hands-off check: run explorer New in a temp agents folder and list the folder (all generated files present).
- [ ] **Step 5: Commit** — `feat: explorer writes the agent workspace on every start`

## Phase C: Detached worlds

### Task 6 (C1): Window-free explorer host in `testbed_content`

**Files:**
- Create: `content/testbed_content/src/explorer/host.rs`
- Modify: `apps/actor_testbed/src/explorer.rs` (keep only window/view parts: modal, F7, keys, port probing that needs BRP; call `host`), `content/testbed_content/src/explorer/mod.rs`
- Test: move the window-free tests from actor_testbed into `content/testbed_content/tests/explorer_host.rs`

**Interfaces:**
- Produces (no wall clock inside; the caller passes `seed`, `now: String`): `pub struct HostArgs { pub home: PathBuf, pub agent: String, pub world: Option<String>, pub resume: bool, pub on_mismatch: Option<Mismatch>, pub preset: Option<String> }`; `pub enum Start { New, Resumed(LoadReport), ResumedChanged(LoadReport), Rebuilt }`; `pub fn build_world(args: &HostArgs, seed: u64, now: &str, plugins: impl FnOnce(&mut App)) -> Result<(App, Start, AgentDir), String>`; `pub fn save_now(world: &mut World, dir: &AgentDir, now: &str) -> Result<(), String>`; `pub fn check_save(home: &Path, agent: &str) -> CheckJson`; `pub struct RunFile { pub pid: u32, pub port: u16, pub mode: RunMode, pub started: String, pub agent: String, pub world: String }` with `write`/`remove`/`read_live(dir) -> Option<RunFile>` (a dead pid → `None`); `pub enum RunMode { Window, Detached }`. Behaviour identical to today's actor_testbed explorer startup (all its existing tests pass unchanged against the moved code).

- [ ] **Step 1: Move the tests first** (they fail to compile against the new paths); add `run_file_round_trips`, `stale_run_file_is_ignored`.
- [ ] **Step 2: Run** `cargo test -p testbed_content --test explorer_host` — Expected: FAIL.
- [ ] **Step 3: Move the code; actor_testbed calls `host`.** Clippy's determinism lints on testbed_content must stay clean without allows.
- [ ] **Step 4: Run** `cargo test -p testbed_content -p actor_testbed` — Expected: PASS.
- [ ] **Step 5: Commit** — `refactor: window-free explorer host in testbed_content`

### Task 20 (C0): One minute of perception memory in explorer worlds (runs after Task 6, before Task 7)

Spec: `docs/superpowers/specs/2026-10-25-actor-map-and-memory-design.md` M1.

**Files:**
- Modify: `content/testbed_content/src/explorer/host.rs` (after the world is built or loaded, set `PerceptionConfig { forget_after_ticks: EXPLORER_FORGET_AFTER, memory_capacity: EXPLORER_MEMORY_CAPACITY, ..existing }`), `content/testbed_content/src/explorer/workspace.rs` + `prompts/agents.md` (AGENTS.md: "you remember what you sensed for about a minute after you lose it; keep what matters longer in your notes"), `CHANGELOG.md`
- Test: `content/testbed_content/tests/explorer_host.rs`

**Interfaces:**
- Produces: `pub const EXPLORER_FORGET_AFTER: u64 = 1200` (60 s at 20 Hz) and `pub const EXPLORER_MEMORY_CAPACITY: u32 = 128` in `host.rs` (the implementer may lower the capacity to no less than 64 if a perception perf budget fails, and says so). Task 15 makes both world-preset numbers (perception rules), defaulting to these.
- Only explorer worlds change: `scenario.rs`'s shared `PerceptionConfig`, the framework defaults (`DEFAULT_FORGET_AFTER`, `DEFAULT_MEMORY_CAPACITY`), the village and every replay/golden stay unchanged. No replay or golden covers the `explorer` scenario today; if one does, stop and stage a re-baseline.
- Old explorer saves resume with the new values (the host applies them after loading); a resume must still succeed and report as today.

- [ ] **Step 1: Write failing tests**: `explorer_remembers_for_a_minute` (the actor senses an item, walks out of sight; at 59 s after last sensed it is still in its `PerceptionMemory`, at 61 s it is gone), `explorer_memory_holds_more_than_32`, `resumed_old_save_gets_the_new_memory` (a save written with the old config resumes; `PerceptionConfig` then has the new values), `other_scenarios_keep_the_default_memory` (a `testbed` scenario world keeps `DEFAULT_FORGET_AFTER`), `agents_md_mentions_memory_span`.
- [ ] **Step 2: Run** `cargo test -p testbed_content --test explorer_host` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests, `cargo test -p npc_perception` (perf tests) and `cargo xtask verify --quick` — Expected: PASS / `"ok":true`.
- [ ] **Step 5: Commit** — `feat(testbed): explorer actors remember for a minute`

### Task 24 (C0b): Notes as one Obsidian file per entry (runs after Task 20, before Task 7)

Spec: `docs/superpowers/specs/2026-10-25-notes-as-obsidian-files-design.md` (O1–O7).

**Files:**
- Create: `crates/npc_agent_io/src/notes/frontmatter.rs` (flat YAML frontmatter reader/writer, hand-written; no new crate), `crates/npc_agent_io/src/notes/filename.rs` (safe readable names)
- Modify: `crates/npc_agent_io/src/{notes.rs,notes/format.rs}` (folder-per-book store, id index, rename on title change, migration), `crates/npc_agent_io/src/{api.rs,mcp.rs}` (additive `unreadable` in list results), `content/testbed_content/src/explorer/{home.rs (read_lossy and folder creation: `diary/`, `memory/`, `proposals/`),workspace.rs + prompts/*.md (frontmatter on AGENTS.md and prompts; copied prompt text without it)}`, `content/testbed_view/src/explorer_panel.rs` (F7 reads the new layout, shows unreadable files), `apps/actor_testbed` + `npcctl` hosts' `NotesClock` (RFC 3339 UTC `…Z`), `CHANGELOG.md`
- Test: `crates/npc_agent_io/tests/notes*.rs`, workspace and F7 tests

**Interfaces:**
- Layout: `diary/YYYY-MM-DD <title>.md` (UTC date of `created`, fixed), `memory/<title>.md`, `proposals/<title>.md`; flat folders, subfolders and non-`.md` files ignored. Safe names: drop `* " \ / < > : | ? # ^ [ ]`, collapse spaces, trim spaces/dots, reserved Windows names get `_`, cut to 120 chars, empty → `<Kind> <id>`; duplicates ` (2)`, ` (3)` in creation order.
- Frontmatter (exact keys, this order): `id`, `type` (`diary|memory|proposal|landmark`), `title` (landmarks: `name`), `aliases` (earlier titles), `tags` (`[npc/<type>]`), `agent`, `world`, `author` (`agent|player`, last writer), `created`, `created_tick`, `updated`, `updated_tick`, `revision`; proposals add `status`, `prompted_by`; landmarks add `pos`, `kind`, `thing` (quoted `"#ID"`). Unknown keys kept verbatim on update; YAML-hostile values quoted.
- Store: index by frontmatter `id` (a renamed file is the same entry); a title change writes temp → renames onto the new name → removes the old file, and appends the old title to `aliases`; same `id` twice → higher `revision` (then newer mtime) wins, the others are `unreadable` ("duplicate id N"); broken/missing frontmatter → `unreadable` with the reason; unreadable files are never overwritten. `list` keeps its result shape and adds `unreadable: [{file, reason}]`.
- API unchanged: the `updated` stamp stays `{tick, date}` with `#rN` in `date`, rebuilt from `updated_tick`/`updated`/`revision`; stale checks as today.
- Migration (once per book): `diary.md` / `memory.md` present and no `.bak` → each entry becomes a file (ids, titles, stamps, text kept; unplaceable text → an entry titled "Kept from diary.md" / "Kept from memory.md"), then the old file is renamed `.bak`; ids that already have a file are skipped (a rerun is safe). Old-style proposals (no frontmatter) are rewritten under their readable name and the old file removed. Old free-form dates are kept verbatim, quoted.
- Generated: `AGENTS.md` and `prompts/*.md` begin with `type: agent-guide|prompt`, `agent`, `world`, `generated: true` (no date; an unchanged folder regenerates byte-identical); `CLAUDE.md` stays exactly `@AGENTS.md`; the prompt text the modal and menu copy has no frontmatter. Owned files list: `diary/`, `memory/`, `proposals/`, `map/`, `agent.json`, `*.world.json`, `run.json` (+ `diary.md.bak`, `memory.md.bak`).

- [ ] **Step 1: Write failing tests**: `each_kind_round_trips`, `yaml_hostile_titles_round_trip`, `safe_names` (each forbidden char, reserved names, length, empty), `same_title_gets_a_number`, `title_change_renames_and_keeps_an_alias`, `renamed_by_hand_is_found_by_id`, `unknown_properties_survive_update`, `broken_frontmatter_is_unreadable_and_untouched`, `duplicate_id_higher_revision_wins`, `old_folder_migrates_once` (entries, ids, stray text, `.bak`, rerun no-op), `old_proposal_gets_frontmatter`, `api_stamp_shape_unchanged` (existing notes tests pass with only path expectations changed), `generated_files_have_frontmatter_and_are_stable`, `copied_prompt_has_no_yaml`, `claude_md_is_exactly_the_import`, F7 `editor_lists_unreadable_files`.
- [ ] **Step 2: Run** `cargo test -p npc_agent_io --test notes` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests, `cargo test -p testbed_content -p testbed_view -p actor_testbed -p npcctl` and `cargo xtask verify --quick` — Expected: PASS / `"ok":true`.
- [ ] **Step 5: Commit** — `feat(agent_io): notes as one Obsidian file per entry`

### Task 25 (C0c): Clearer movement and vehicle feedback (runs after Task 24, before Task 7)

Spec: `docs/superpowers/specs/2026-10-25-agent-feedback-increment-design.md` F1 (proposals 001, 003, 010).

**Files:**
- Modify: `crates/npc_nav/src/{astar.rs,lib.rs}` (on `GoalBlocked`/unreachable, keep the explored node closest to the goal and what blocked: wall cell, door `SimId`, out of bounds), `crates/npc_command/src/{exec.rs,command.rs}` (`OrderFailed` carries `nearest_reachable` and `blocker`; Move labels in metres), `crates/npc_framework/src/glue/vehicles_agent.rs` (`steered_vehicle` driver-seat check; autopilot reason), the agent act layer in `crates/npc_agent_io` (fractional/invalid `pos` rejected with the millimetre hint), `CHANGELOG.md`
- Test: `crates/npc_nav/tests/nav.rs`, `crates/npc_framework/tests/glue_agent.rs`, `crates/npc_agent_io/tests/*`

**Interfaces:**
- `OrderFailed { .., reason: "blocked", nearest_reachable: Option<[i32; 2]>, blocker: Option<Blocker> }` with `Blocker = Wall { cell: [i32; 2] } | Door { id: SimId, locked: bool } | OutOfBounds`; text: "Move failed: blocked by closed door #12; nearest reachable (11.20 m, 3.40 m)". Event JSON and text are not hashed; recorded commands are unchanged; no replay or golden may move.
- Move labels/results: "move to (0.00 m, -0.03 m)". A `pos` with a fraction or outside `i32` → rejected `invalid` with "positions are integer millimetres: 6.5 m is 6500".
- `drive_to`: a unit in a `Driver` seat of a `ByCrew` vehicle can drive (the reported case: "Mounted {role: Driver, seat: 0}", then refused "not in its driver seat"). A vehicle on its own behaviour (autopilot) refuses with "wagon #30 is on autopilot: <how to take control>"; every reason matches the observed state.

- [ ] **Step 1: Write failing tests**: `blocked_move_reports_nearest_reachable_and_door`, `blocked_move_out_of_bounds`, `move_label_in_metres`, `fractional_pos_hint`, `driver_seat_can_drive_to` (mount as Driver → `drive_to` accepted, the wagon moves), `autopilot_reason_matches_state`.
- [ ] **Step 2: Run** them — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests and `cargo xtask verify --quick` — Expected: PASS / `"ok":true`, no replay or golden moved.
- [ ] **Step 5: Commit** — `fix: movement and vehicle feedback (blocked moves, metres, drive_to in the driver seat)`

### Task 26 (C0d): Clearer item and crafting feedback (runs after Task 25, before Task 7)

Spec: `docs/superpowers/specs/2026-10-25-agent-feedback-increment-design.md` F2 (proposals 002, 005, 006, 008).

**Files:**
- Modify: `crates/npc_interaction/src/{builtin.rs,items.rs}` (yield overflow placement reported; full-pack refusal names what to drop; hand-held slot rule), `crates/npc_crafting/src/run.rs` (the missing-input error lists every missing input and tool), `crates/npc_framework/src/glue/*` (interaction, crafting and rules agent glue: offer lines show load after yield + "will drop"; craft odds hint from the tool-use helper; consumed inputs; a used default skill on the Skills line), `CHANGELOG.md`
- Test: `crates/npc_interaction/tests/interaction.rs`, `crates/npc_crafting/tests/*`, `crates/npc_framework/tests/glue_agent.rs`

**Interfaces:**
- `Interacted` result/event gains `dropped: [{item: SimId, pos: [i32; 2]}]` and a `reason` ("pack would carry 30950 of 30000 g"); text "2 × log dropped at (24.08 m, 1.10 m): pack would carry 30950 of 30000 g". Offer: "yields 2 × log (pack 24950 -> 30950 of 30000 g, will drop)".
- Craft unavailable: "needs 1 plank (has 0), 1 ingot (has 2), tool hammer (has 1)"; available: "consumes 1 plank, 1 ingot; uses hammer"; odds "[Carpentry 7 (default DX-5) vs 3d6: ~16%, low odds]".
- Full pack: "inventory is full (8 slots): drop 1 of [plank #66, ingot #89] to make room" (lightest first, up to 3). The pack line states whether the main-hand item counts; if "9 of 8" is a counting bug, fix it, else explain it.
- Skills line: a skill used at its default appears ("Carpentry 7 (default DX-5)"). Sheets, rolls and rules are unchanged (no hash moves; only observation text/JSON change).

- [ ] **Step 1: Write failing tests**: `yield_overflow_says_where_it_dropped`, `offer_shows_load_after_yield`, `craft_names_every_missing_input`, `craft_line_shows_odds`, `full_pack_names_what_to_drop`, `hand_item_slot_rule_is_stated`, `used_default_skill_is_listed`.
- [ ] **Step 2–4: Implement, run, verify --quick** (no replay or golden moved).
- [ ] **Step 5: Commit** — `feat: item and crafting feedback (drops, every missing input, odds, full pack)`

### Task 7 (C2): `npcctl serve` hosts a detached explorer world; exact-port window start

**Files:**
- Modify: `crates/npc_agent_io/src/session.rs` (`Session::start_app(app: App, options) -> Result<Session, String>`; `SessionOptions.on_frame: Option<Box<dyn FnMut(&mut World)>>` and `on_stop`), `apps/npcctl/src/agent.rs` (serve flags), `apps/npcctl/Cargo.toml` (`ctrlc` for Ctrl+C → graceful stop; `cargo deny check` must pass — stop and ask if it does not), `apps/actor_testbed/src/main.rs` (`--exact-port`)
- Test: `apps/npcctl/tests/explorer_serve.rs`, actor_testbed tests

**Interfaces:**
- `npcctl serve --explorer-home DIR --agent NAME [--world W] [--resume [--load-anyway|--fresh-world]] [--preset P] [--port N]` → `host::build_world` + `NotesDirs` + `Session::start_app`; prints `{"listening":URL,"port":N,"agent":..,"world":..,"prompt":<path>}`; writes the workspace and `run.json` (mode detached); autosave every 60 s via `on_frame`; on `session/stop` or Ctrl+C: save, remove `run.json`, exit 0.
- `actor_testbed --explorer-home .. --resume --exact-port N`: bind exactly N, retrying for 10 s; if still taken exit **4** with "port N is still in use". No prompt modal on this path (the agent is already playing); a 5 s notice "moved here from detached" instead.

- [ ] **Step 1: Write failing tests**: `serve_starts_a_new_explorer_world` (spawn npcctl, read the listening line, `agent/observe` works, `run.json` present), `serve_saves_and_cleans_up_on_stop`, `serve_resumes_the_same_tick`, `notes_work_detached`, actor_testbed: `exact_port_retries_then_exits_4` (hold the port 11 s), `exact_port_skips_the_modal`.
- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests and `cargo xtask verify --quick`.
- [ ] **Step 5: Commit** — `feat: detached explorer worlds (npcctl serve), exact-port window resume`

### Task 8 (C3): Menu — start detached, watch, stop, move between modes

**Files:**
- Modify: `apps/game_menu/src/{explorer.rs,launch.rs,run.rs,screens.rs}`, `apps/actor_testbed/src/{explorer.rs,main.rs}` (close-to-detached), `docs/agent-experiments.md`, `CHANGELOG.md`
- Test: game_menu tests, actor_testbed tests

**Interfaces:**
- `Entry::ExplorerNew`/`ExplorerResume` gain `mode: RunMode`; New and Resume show **Start in window** and **Start detached**. Detached screen: prompt text with **Copy**, URL, status line "<agent> in <world> running detached on port N, tick T" (tick via `session/info` every 2 s), **Open in window**, **Stop**. The menu tracks detached children and, on Resume, adopts any live `run.json` (row shows "running detached — Open / Stop").
- **Open in window**: `session/stop` → wait for exit (≤ 15 s) → if the save failed (non-zero exit or no newer save) abort with the reason and keep it running (restart detached from the last save if the process died) → start `actor_testbed --resume --exact-port N`; exit 4 → restart detached on N and say so.
- In the window, closing an explorer world asks "Keep it running detached?" (Yes: save, spawn `npcctl serve --resume --port N` detached from the window process, exit; No: save and exit). A key (F9) does the same move on purpose.

- [ ] **Step 1: Write failing tests**: `detached_entry_builds_serve_args`, `detached_screen_polls_tick`, `stop_saves_and_returns_to_resume`, `open_in_window_aborts_on_failed_save`, `open_in_window_falls_back_when_port_busy`, `resume_list_adopts_running_worlds`, actor_testbed: `close_offers_detached` (headless: send `WindowCloseRequested`, choose Yes → a serve child is spawned with the same port — use a fake spawner).
- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests; screenshots of the Detached screen and the close dialog.
- [ ] **Step 5: Commit** — `feat(game_menu): detached worlds, open in window, keep running detached`

## Phase D: Fog of war

### Task 27 (D0): Reach sense and remembering everything sensed (opens Phase D, before Task 9)

Spec: `docs/superpowers/specs/2026-10-25-agent-feedback-increment-design.md` F3 (proposals 007, 009, 011).

**Files:**
- Modify: `crates/npc_perception/src/{lib.rs,system.rs,memory.rs}` (an opt-in `ReachSense { radius_mm }` sensor component: all-round, never through walls, feeds `PerceptionMemory` like vision), `crates/npc_framework/src/glue/perception_agent.rs` (reach-sensed things listed; vehicles and units listed under "You remember" with their last position — find and fix why they went missing), `content/testbed_content/src/explorer/host.rs` (explorer actor gets `ReachSense { radius_mm: 1500 }`), `content/testbed_content/src/explorer/workspace.rs` + `prompts/agents.md` (body senses list reach), `CHANGELOG.md`
- Test: `crates/npc_perception/tests/*`, `crates/npc_framework/tests/glue_agent.rs`, `content/testbed_content/tests/explorer_host.rs`

**Interfaces:**
- `ReachSense` is registered snapshot-only (Global Constraints) and present only on actors that opt in (the explorer actor now; Task 11's presets: human `reach` sense, robots a `proximity` sensor using the same component). Scenarios without it keep their hashes; no replay or golden may move.
- Sensed by reach: containers, workstations, doors, items, vehicles, units within 1.5 m whatever the facing; their verbs are offered. Text: "Within reach: chest #54 (behind you), forge #15."
- Memory: every sensed kind (items, props, containers, stations, doors, vehicles, units) is stored and listed under "You remember" with its last position; the one-minute rule applies (Task 20).
- Fog (Task 9) also reveals what the reach sense senses.

- [ ] **Step 1: Write failing tests**: `chest_behind_in_reach_is_sensed`, `station_in_reach_stays_usable_after_turning`, `reach_sense_does_not_cross_walls`, `vehicles_and_units_are_remembered`, `scenarios_without_reach_keep_their_hash`, `agents_md_lists_reach`.
- [ ] **Step 2–4: Implement, run, verify --quick** (no replay or golden moved).
- [ ] **Step 5: Commit** — `feat(perception): reach sense; vehicles and units are remembered`

### Task 9 (D1): Explored and visible cells (sim-side, snapshot-only)

**Files:**
- Create: `crates/npc_perception/src/fog.rs`
- Modify: `crates/npc_perception/src/{lib.rs,system.rs}`, `content/testbed_content/src/layout.rs` (explorer actor gets `RevealsFog`)
- Test: `crates/npc_perception/tests/fog.rs`, `content/testbed_content/tests/snapshot.rs`

**Interfaces:**
- Produces: marker `RevealsFog` (snapshot-only `reveals_fog`); `ExploredCells { width: u32, bits: Vec<u64> }` (snapshot-only `explored_cells`); `VisibleCells { bits: Vec<u64> }` (snapshot-only `visible_cells`, recomputed every scan tick); `SeenStatics(BTreeMap<SimId, SeenState>)` (snapshot-only `seen_statics`) with `SeenState { kind: KindBits, pos: SimPos, open: Option<bool>, felled: Option<bool> }` — the last-seen state of statics (walls, doors, trees, rocks, stations, items) in explored cells; `pub fn reveal_cells(grid: &SpatialGrid, from: SimPos, facing: Facing, sensor: &Sensor) -> Vec<CellCoord>` (vision: cells in the cone and range with `line_of_sight`; other sensors: Task E3). Updated in `NpcSet::Sense` after `perception_system`, on the same stagger ticks, only for `RevealsFog` entities. Integer only.

- [ ] **Step 1: Write failing tests**: `fog_respects_line_of_sight` (a wall between actor and a cell: not revealed), `closed_door_blocks_fog`, `explored_grows_while_walking`, `seen_statics_keep_last_door_state` (door closed after being seen open → `SeenStates.open == Some(true)` until seen again), `explored_survives_resume` (assert_resume_equivalent on the explorer + explored bits equal), `fog_does_not_change_existing_hashes` (testbed/village state hash at tick 600 equals the pre-change constant recorded from `main`).
- [ ] **Step 2: Run** `cargo test -p npc_perception --test fog` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests and `cargo xtask verify --quick` (replays/goldens unchanged).
- [ ] **Step 5: Commit** — `feat(perception): explored/visible cells and last-seen statics for fog of war`

### Task 10 (D2): Drawing the fog

**Files:**
- Create: `content/testbed_view/src/fog.rs`
- Modify: `content/testbed_view/src/{lib.rs,visuals.rs}`, `apps/actor_testbed/src/main.rs` (`--fog on|off`, default on in explorer mode), `content/testbed_content/src/explorer/settings.rs` (`fog_of_war: bool`, default true), `apps/game_menu` (Settings checkbox, passes `--fog`)
- Test: unit tests in `fog.rs`

**Interfaces:**
- Consumes: Task D1 components on the played actor.
- A per-cell fog mask (one texel per grid cell: black = never explored, ~55% dark = explored not visible, clear = visible) drawn over the ground and dimming everything in explored-not-visible cells (builder's choice of technique — a mask plane, a shader on a ground-covering overlay, or per-visual tinting — judged by screenshot: the result must read as Age of Empires fog: black, greyed, clear). Units whose cell is not visible are hidden (the played actor never). Statics in explored-not-visible cells are drawn in their `SeenStatics` state (a door seen open stays open in the fog). F8 toggles fog; with fog off everything shows as today.

- [ ] **Step 1: Write failing tests** (pure functions): `mask_values_for_three_states`, `units_hidden_outside_visible`, `statics_use_last_seen_state`, `fog_off_shows_everything`.
- [ ] **Step 2: Run** `cargo test -p testbed_view fog` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests; screenshots at tick 0 and after a 2-minute scripted walk (`--commands`) with fog on, and one with fog off; view them.
- [ ] **Step 5: Commit** — `feat(testbed_view): fog of war`

### Task 28 (D3): Searched ground in observe (after Task 10, closes Phase D)

Spec: `docs/superpowers/specs/2026-10-25-agent-feedback-increment-design.md` F4 (proposal 004).

**Files:**
- Modify: the perception or fog agent glue in `crates/npc_framework/src/glue/` (summary from Task 9's explored cells), `content/testbed_content/src/explorer/workspace.rs` + `prompts/agents.md`, `CHANGELOG.md`
- Test: Task 9's fog tests, `crates/npc_framework/tests/glue_agent.rs`

**Interfaces:**
- Text line (only for actors with an explored-cells record): "Searched: 34% within 60 m, 12% of the map; least searched: north-east, east." JSON `searched: {cell_mm: 10000, radius_mm: 60000, within_radius_pct, map_pct, sectors: [{dir: "N", explored, total}, …8]}`, from the explored cells coarsened to 10 m; read-only, never hashed.

- [ ] **Step 1: Write failing tests**: `searched_summary_counts_explored_cells`, `least_searched_sectors`, `no_summary_without_fog_record`.
- [ ] **Step 2–4: Implement, run, verify --quick.**
- [ ] **Step 5: Commit** — `feat: searched-ground summary in observe`

## Phase E: Actor presets, robot sensors and the actor editor

### Task 11 (E1): Actor presets as data

**Files:**
- Create: `content/testbed_content/src/actors.rs`, `content/testbed_content/actors/{human,rover,sentinel}.json`
- Modify: `content/testbed_content/src/{layout.rs (UnitSpec gains `preset: Option<String>`; spawn_unit applies a preset),lib.rs}`, `content/testbed_content/src/explorer/{home.rs (AgentMeta gains `preset: String`, default "human"),settings.rs (`actors_dir`)}`
- Test: `content/testbed_content/tests/actors.rs`

**Interfaces:**
- Produces: `#[serde(deny_unknown_fields)] pub struct ActorPreset { pub name: String, pub description: String, pub body: Body, pub model: String, pub sheet: SheetSpec, pub speed_mm_per_tick: i32, pub carry_capacity: u32, pub actions: BTreeSet<String>, pub sensors: Vec<SensorSpec>, pub perception: PerceptionSpec }`; `pub enum Body { Human, Robot }`; `SheetSpec` = the rules sheet's attributes/skills/traits by name (point-buy validated with the rules registry); `SensorSpec` (serde tag `type`): `vision {range_mm, half_fov_deg}`, `hearing {radius_mm}`, `lidar {range_mm, fov_deg, beams, noise_ppm}`, `camera {range_mm, half_fov_deg}`, `radar {range_mm, half_fov_deg, range_res_mm, bearing_res_mdeg}`, `reach {radius_mm}` (the vicinity sense of Task 27: all round, lifts fog; the dev's decision: default 1500 mm, configurable per preset, 0 or absent = none; the Human, Rover and Sentinel built-ins carry `reach {radius_mm: 1500}`, applied as `ReachSense`; replaces the host's fixed `EXPLORER_REACH_MM` once presets apply); `PerceptionSpec { detection: Option<String> }` (memory span and capacity are world settings, map/memory spec M1 — not per preset); `pub fn load_presets(dir: &Path) -> Vec<(PathBuf, Result<ActorPreset, String>)>` (sorted by file name; built-ins copied into an empty `actors_dir` on first run); `pub fn validate(p: &ActorPreset, world: &World) -> Result<(), Vec<String>>` (budget, ranges 1..=50_000 mm, reach 0..=5_000 mm, fov 1..=180, beams 1..=720, known actions — the interaction/crafting/vehicle verbs registered in the app — known model names); `pub fn apply_preset(world: &mut World, entity: Entity, p: &ActorPreset)` (sheet, speed, carry, `AllowedActions` (snapshot-only `allowed_actions`; affordances and verbs outside the set are rejected "this actor cannot <verb>"), sensors via Task E2 builders, `ActorModel(String)` snapshot-only for the view).
- Built-ins: **human** (scout sheet, today's vision 15 m/60° + hearing 10 m, all actions), **rover** (robot, LIDAR 20 m/360°/360 beams + camera 15 m/35°, all actions but craft), **sentinel** (robot, radar 30 m/60° + camera 15 m/35°, all actions but craft).

- [ ] **Step 1: Write failing tests**: `builtins_load_and_validate`, `invalid_presets_are_reported` (unknown sensor type, negative range, over-budget sheet, unknown model, unknown action → each a reason, never spawned), `explorer_spawns_the_chosen_preset`, `disallowed_action_is_rejected`, `human_preset_matches_todays_scout` (explorer state hash with `preset: human` at tick 600 equals today's — no behaviour change for the default).
- [ ] **Step 2: Run** `cargo test -p testbed_content --test actors` — Expected: FAIL.
- [ ] **Step 3: Implement.** Until Task E2 lands, `validate` rejects LIDAR/camera/radar specs with "this sensor arrives with the robot sensors (Task E2)" (no stand-in sensors); the built-in rover and sentinel presets therefore show as invalid until E2, and their tests (`builtins_load_and_validate`) assert human only until E2 extends them.
- [ ] **Step 4: Run** the tests and `cargo xtask verify --quick`.
- [ ] **Step 5: Commit** — `feat(testbed): actor presets (human, rover, sentinel)`

### Task 29 (E0): Quitting the game stops every process it started (runs after Task 11, before Task 12)

The dev's decision (2026-10-26): "quitting the game stops all processes". Supersedes spec G7's "a detached world outlives the menu" for menu Quit. The window's "Keep it running detached?" on closing a window stays as it is: closing one game window does not quit the game.

**Files:**
- Modify: `apps/game_menu/src/{run.rs,detached.rs,screens.rs,main.rs}`, `apps/actor_testbed/src/{explorer.rs,main.rs}` (a graceful quit request over BRP), `docs/agent-experiments.md` §9, `CHANGELOG.md`
- Test: game_menu tests, actor_testbed tests

**Interfaces:**
- Menu Quit (button, window close, Ctrl+C on the menu's console) stops every process the game runs, then exits:
  - detached worlds the menu tracks (started by it, moved to detached from a window, or adopted from a live `run.json`): `session/stop` (saves first), wait ≤ 15 s each, in parallel;
  - game windows it started (actor_testbed, village, rts_units_demo …): a graceful quit request — actor_testbed explorer windows save and exit (a new BRP method `app/quit`, served only to localhost like the rest; it runs the same path as closing the window with "No": save, remove `run.json`, exit); other windows exit through `app/quit` too; a window without BRP or that does not answer within 15 s is killed by its own pid (never by image name);
  - a stop that fails (a refused save) keeps the menu open on a "Quitting…" screen that names the world and the reason, with **Quit anyway** (kill by pid, the save stays at the last autosave) and **Back**.
- A "Quitting…" screen lists each process and its state while it stops; the menu exits when all are gone.
- The menu never touches processes it did not start or adopt; `npcctl mcp` servers belong to the AI tool's session (they exit when that session closes) and are left alone — `docs/agent-experiments.md` says so.
- Already in progress (Open in window / Stop): Quit waits for it first (Task 8's busy rule), then stops everything.

- [ ] **Step 1: Write failing tests**: `quit_stops_detached_worlds_and_waits_for_their_save` (fake Control: stop answered, child exits 0), `quit_asks_windows_to_quit_then_kills_after_timeout` (fake spawner/child), `quit_with_a_refused_save_offers_quit_anyway`, `quit_leaves_untracked_processes_alone`, actor_testbed `app_quit_saves_and_exits` (headless: the BRP method writes the save and removes `run.json`).
- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests, `cargo xtask verify --quick`; hands-off: start a detached world and a testbed window from the menu's code paths, quit, list processes (none of them left) and show the save is newer.
- [ ] **Step 5: Commit** — `feat(game_menu): quitting the game stops every process it started`

### Task 12 (E2): LIDAR, camera and radar sensors with ROS-shaped readings

**Files:**
- Create: `crates/npc_perception/src/{lidar.rs,camera.rs,radar.rs,readings.rs}`
- Modify: `crates/npc_perception/src/{sensors.rs (new `Sensor` variants appended; builders `.lidar(..)`, `.camera(..)`, `.radar(..)`),system.rs (scan runs them),lib.rs}`, `crates/npc_framework/src/glue/perception_agent.rs` (observation sections), `content/testbed_content/src/actors.rs` (apply robot sensors)
- Test: `crates/npc_perception/tests/{lidar,camera,radar}.rs`

**Interfaces:**
- `SensorReadings` (snapshot-only `sensor_readings`): `lidar: Option<LaserScan>`, `camera: Vec<Detection>`, `radar: Vec<RadarTrack>`, plus the camera tracker map `tracks: BTreeMap<SimId, u32>` and `next_track: u32`.
- `LaserScan { angle_min_mdeg: i32, angle_increment_mdeg: i32, range_min_mm: i32, range_max_mm: i32, ranges_mm: Vec<i32> }` (ROS `sensor_msgs/LaserScan` in mm/millidegrees; a miss = `range_max_mm + 1`); a beam stops at the first blocking or occluding cell or a cell holding a perceivable entity (`SpatialGrid::trace_segment`); deterministic noise `±noise_ppm` from `sim_rand(seed, id, tick, purpose)`.
- `Detection { class: String, track_id: u32, bearing_mdeg: i32, distance_mm: i32, size_mm: i32, confidence_ppm: u32 }` (ROS `vision_msgs` style): perceivables in the cone and range with `line_of_sight`; confidence falls linearly from 990000 at 0 to 600000 at max range; `track_id` stable while the target stays in view (dropped after leaving view).
- `RadarTrack { bearing_mdeg: i32, range_mm: i32, radial_velocity_mm_per_tick: i32, amplitude_ppm: u32 }` (ROS `radar_msgs` style): bearing/range quantized to the sensor's resolution; radial velocity from `PrevSimPos → SimPos`; amplitude higher for moving targets; blocked by walls (line of sight).
- Camera and radar feed `PerceptionMemory` (new `SenseKind::{Camera, Radar}` appended), so affordances follow what the robot senses; LIDAR feeds no identities. Observation sections `lidar`, `camera`, `radar`: JSON = the readings; text = LIDAR nearest obstacle per 8 sectors, camera/radar one line per detection/track.

- [ ] **Step 1: Write failing tests**: lidar `ranges_hit_the_wall`, `misses_report_max_plus_one`, `noise_is_deterministic`; camera `detects_in_view_with_line_of_sight`, `track_ids_are_stable`, `occluded_targets_are_not_detected`; radar `moving_targets_have_radial_velocity`, `values_are_quantized`; `robot_observation_has_sensor_sections`; `existing_hashes_unchanged` (testbed/village at tick 600 equal recorded constants).
- [ ] **Step 2: Run** `cargo test -p npc_perception` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests, `assert_resume_equivalent` on a rover explorer world, and `cargo xtask verify --quick`.
- [ ] **Step 5: Commit** — `feat(perception): LIDAR, camera and radar sensors with ROS-shaped readings`

### Task 13 (E3): Robots in the view, in fog, in the workspace and the menu

**Files:**
- Modify: `content/testbed_view/src/{assets.rs (ModelKind gains procedural Rover/Sentinel models built from primitives: body, wheels, a sensor dome per sensor),visuals.rs (model from `ActorModel`)}`, `crates/npc_perception/src/fog.rs` (`reveal_cells` for lidar: cells along beams up to the hit; camera: cone with LOS; radar: the cells of its tracks), `content/testbed_content/src/explorer/workspace.rs` (`ActorSummary` from the preset), `apps/game_menu/src/explorer.rs` (New: preset picker with description; Resume rows show the preset), `apps/actor_testbed` / `npcctl serve` (`--preset`)
- Test: view, fog, workspace and menu unit tests

- [ ] **Step 1: Write failing tests**: `rover_uses_the_rover_model`, `lidar_reveals_geometry_only`, `agents_md_describes_the_robot`, `new_screen_lists_valid_presets_only`, `resume_row_shows_preset`.
- [ ] **Step 2–4: Implement, run, screenshot** a rover explorer world with fog on.
- [ ] **Step 5: Commit** — `feat: robots in the explorer (models, fog, workspace, menu)`

### Task 14 (E4): The actor editor

**Files:**
- Create: `apps/actor_testbed/src/actor_editor.rs`, `content/testbed_view/src/model_preview.rs`
- Modify: `apps/actor_testbed/src/main.rs` (`--actor-editor [--actors-dir DIR]`), `apps/game_menu/src/{screens.rs,launch.rs}` (Main gets **Actors** → launches the editor), `docs/agent-experiments.md`, `README.md`, `CHANGELOG.md`
- Test: unit tests in `actor_editor.rs`, game_menu tests

**Interfaces:**
- Consumes: Task E1 (`load_presets`, `validate`, `ActorPreset`), Task E3 models.
- Window: left a preset list (valid/invalid with reason) with **New**, **Duplicate**, **Save**, **Revert**; centre a **3D preview** (`model_preview.rs`: the preset's model on a turntable, lit, its sensors drawn as cones/rings at their ranges), a **model picker** under it; right a form: name, description, body, sheet (attributes/skills/traits with the point-buy budget shown live), speed, carry capacity, action checkboxes, sensors (add/remove; per-type parameter fields, including the reach vicinity radius, default 1.5 m), perception settings. Validation runs on every edit; Save is disabled while invalid and lists the reasons. Saving writes `<actors_dir>/<slug(name)>.json` atomically (never overwrites another preset with the same slug without confirmation).

- [ ] **Step 1: Write failing tests**: `editor_lists_presets_with_validity`, `new_preset_starts_from_human`, `editing_a_field_revalidates`, `save_writes_the_file`, `save_refuses_slug_collision_without_confirm`, `model_picker_changes_the_preview`, menu: `actors_entry_launches_the_editor`.
- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests; screenshots of the editor with Human and Rover selected; view them.
- [ ] **Step 5: Commit** — `feat: actor editor with 3D preview`

## Phase I: Agent registration (id, token, entering the world)

Spec: `docs/superpowers/specs/2026-10-26-agent-registration-design.md` (R1–R7, §6 builder answers). Runs after Phase E, before Phase G; Phase H's map tools are built session-bound on top of it.

### Task 30 (I1): The registration lock in the agent interface

**Files:**
- Create: `crates/npc_agent_io/src/registration.rs`
- Modify: `crates/npc_agent_io/src/{api.rs,remote.rs,session.rs,mcp.rs,lib.rs,observe.rs}`, `apps/npcctl/src/agent.rs` (`--agent-id`, `--token`), `docs/debugging.md`, `CHANGELOG.md`
- Test: `crates/npc_agent_io/tests/registration.rs`, `apps/npcctl/tests/agent.rs`

**Interfaces:**
- `RegistrationRequired` resource (absent = today: every existing scenario, test and doc unchanged). `AgentCredentials` resource: `BTreeMap<String /*agent_id*/, Credential { token_digest: [u8; 32], assigned: SimId, controller: String }>` plus `pub fn set_reload(fn(&mut World))` (the host's mtime re-read hook, called before each agent call). Sessions: `AgentSessions` (host memory, never saved), random session keys.
- Methods (only when `RegistrationRequired`): `agent/register {agent_id, token}` → `{session, agent_id, world}` | `bad_credentials` (+ `retry_after_ms` ≥ 1000 per agent id after a failure) ; `agent/enter_world {session}` → `{character: {name, body, description, senses, actions}, scene}` (text via a host hook, no unit number) ; `agent/take_control {session}` (the recorded attach command, Direct, controller = agent_id; refused `not_yours` for any other unit) ; `agent/release {session}`. Every other `agent/*` call needs `session`: before register `not_registered`; notes allowed after register (the session's agent folder); observe/act/act_and_wait/events/cancel need `take_control` first (`not_in_control`); `agent/list_units` returns only the own actor. A closed session (re-register, rotation) answers parked long-polls `unknown_session`. Tokens are compared in constant time and never logged, journaled, saved or echoed.
- MCP: tools `register {agent_id, token}`, `enter_world`, `take_control`, `release`, and the session-bound `observe`, `act`, `act_and_wait`, `events`, `cancel`, `notes_*` without `unit`/`controller`; `list_units` and `attach` dropped. The MCP server keeps the session and the credentials in memory and on `not_registered`/`unknown_session` re-registers once and retries.
- npcctl: `npcctl agent … --agent-id A --token T` registers first (dev only; not in AGENTS.md).

- [ ] **Step 1: Write failing tests**: `unlocked_worlds_are_unchanged` (no `RegistrationRequired`: today's calls work), `calls_before_register_are_refused`, `bad_token_is_refused_and_throttled`, `register_enter_take_control_act`, `take_control_of_another_unit_is_refused`, `reregister_replaces_the_session_and_closes_its_polls`, `rotated_token_stops_working_on_the_next_call` (reload hook), `token_never_appears_in_journal_or_logs`, `mcp_tools_are_session_bound`, `mcp_reregisters_after_a_restart`, npcctl `agent_with_token_registers_first`.
- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests and `cargo xtask verify --quick` (no replay or golden moves).
- [ ] **Step 5: Commit** — `feat(agent_io): agents register with an id and a token before they act`

### Task 31 (I2): The explorer as a registered world; dungeon-master briefing

**Files:**
- Modify: `content/testbed_content/src/explorer/{home.rs (agent_id + token generated at creation, stored in agent.json; `rotate_token`),host.rs (`RegistrationRequired`, fill `AgentCredentials` from agent.json, reload hook, enter_world text hook from the preset's ActorSummary + "You arrive in <world>.", detach the actor after build/load),workspace.rs + prompts/{agents.md,start.md,resume*.md,common.md} (AGENTS.md: id, token, register → enter_world → take_control, notebooks, rules, MCP only — no npcctl, no unit, no controller, no URL; the character section moves to enter_world; prompts become a dungeon master's briefing building on the Epoch 1 brief)}`, `apps/game_menu` (Resume detail and Detached screen: **New token** button → `rotate_token`, rewrites agent.json + AGENTS.md; a running world drops the old session on its next call), `apps/actor_testbed` + `apps/npcctl` (no behaviour change beyond the host), `docs/agent-experiments.md` §9, `CHANGELOG.md`
- Test: `content/testbed_content/tests/explorer_host.rs`, workspace tests, game_menu tests

**Interfaces:**
- `AgentMeta` gains `agent_id: String` (stable, `<slug>-<4 random>`) and `token: String` (≥ 128-bit from the OS RNG, hex); old `agent.json` files get both on first open (written back).
- The explorer actor is never attached after a build, a resume, a reset or a move (window ↔ detached): `take_control` is always the agent's own act.
- AGENTS.md contains the id and the token and nothing that names `npcctl`, a unit number or a controller (a test greps for them); the prompts never contain the token.

- [ ] **Step 1: Write failing tests**: `new_agent_gets_an_id_and_a_token`, `old_agent_json_gets_credentials_once`, `explorer_world_requires_registration`, `actor_is_unattached_after_build_resume_and_move`, `enter_world_describes_the_preset_without_its_unit`, `agents_md_has_credentials_and_no_npcctl_unit_or_controller`, `prompts_are_a_briefing_without_the_token`, `rotate_token_revokes_the_old_one` (live session refused on its next call), menu `new_token_button_rotates`.
- [ ] **Step 2: Run** — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests and `cargo xtask verify --quick`; hands-off: a detached world, `npcctl mcp` driven like an MCP client (register → enter_world → take_control → act_and_wait → notes_add), then Open in window and the next call re-registers transparently.
- [ ] **Step 5: Commit** — `feat(testbed): explorer worlds require registration; dungeon-master briefing`

## Phase G: World presets (mechanics and systems)

### Task 15 (G1): World presets as data

**Files:**
- Create: `content/testbed_content/src/worlds.rs`, `content/testbed_content/worlds/{plain_explorer,survival,night_shift}.json`
- Modify: `content/testbed_content/src/explorer/{home.rs (AgentMeta gains `world_preset: String`, default "plain_explorer"),host.rs (build_world applies the preset),settings.rs (`worlds_dir`)}`
- Test: `content/testbed_content/tests/worlds.rs`

**Interfaces:**
- Produces: `#[serde(deny_unknown_fields)] pub struct WorldPreset { pub name: String, pub description: String, pub needs: NeedsSpec, pub day_night: DayNightSpec, pub detection: Detection, pub perception: PerceptionRulesSpec, pub systems: SystemsSpec, pub inhabitants: InhabitantsSpec }`; `NeedsSpec { hunger: NeedSpec, thirst: NeedSpec, energy: NeedSpec, battery: NeedSpec }`, `NeedSpec { on: bool, rise_milli_per_tick: i32, low_milli: i32, out_milli: i32, low_speed_pct: u8 }`; `DayNightSpec { on: bool, cycle_ticks: u64, night_vision_pct: u8 }`; `PerceptionRulesSpec { forget_after_ticks: u64 (default 1200), memory_capacity: u32 (default 128) }` as `WorldPreset.perception` (Task 20's constants become these defaults); `SystemsSpec { crafting: bool, vehicles: bool, locked_doors: bool, progression: bool }`; `InhabitantsSpec { workers: u8, skeletons: u8 }`; `load_worlds(dir)`, `validate(&WorldPreset) -> Result<(), Vec<String>>` (rates 0..=1000, cycle 1200..=144000 ticks, pct 0..=100, inhabitants ≤ 8, forget_after 200..=12000 ticks, memory_capacity 16..=512); `world_doc(preset: &WorldPreset, actor: &ActorPreset) -> ScenarioDoc` (the explorer scenario with the preset's switches applied).
- Defaults in the shipped files: Plain explorer = every switch off except today's (locked store on, vehicles/crafting on as today, per-roll detection, no inhabitants beyond today's) — it must produce today's explorer world exactly; Survival = hunger/thirst/energy on with the village's rates, day/night on (village cycle); Night shift = day/night on with a short cycle and `night_vision_pct: 40`, per-roll detection.

- [ ] **Step 1: Write failing tests**: `shipped_worlds_validate`, `invalid_worlds_are_reported`, `plain_explorer_is_todays_world` (explorer state hash at tick 600 with Plain explorer + human equals today's, i.e. after Task 20's memory change), `systems_off_remove_their_things` (vehicles off → no cart/wagon spawned; locked_doors off → store unlocked).
- [ ] **Step 2–4: Implement, run, verify --quick.**
- [ ] **Step 5: Commit** — `feat(testbed): world presets`

### Task 16 (G2): Needs, day/night and inhabitants in the explorer world

**Files:**
- Modify: `content/testbed_content/src/{content.rs (register hunger/thirst/energy/battery `NeedDef`s through the facade, food/water/bed/charger smart objects with `Provides`),layout.rs (place sources when a need is on),tree.rs (NPC inhabitants' behaviour)}`, `crates/npc_framework/src/glue/*` only if a needs×agent glue is missing (observation section "needs" with levels and effects), `content/testbed_content/src/explorer/workspace.rs` (AGENTS.md lists active needs and rules)
- Test: `content/testbed_content/tests/worlds.rs`

**Interfaces:**
- Needs rise per tick for the played actor (humans: hunger/thirst/energy; robots: battery); low → `low_speed_pct` movement; out → collapsed/shut down (no actions accepted except "rest"/"charge" when at a bed/charger, reason "too exhausted"/"battery empty") until restored above `low_milli`. Eating/drinking/sleeping/charging are interact verbs on the placed sources. Day/night: vision and camera range × `night_vision_pct` at night; the observation's existing `night` flag reflects it. Inhabitants: workers wander and work, skeletons patrol; neither is played.
- New per-entity state is snapshot-only registered (Global Constraints); with Plain explorer nothing new is spawned or registered on entities.

- [ ] **Step 1: Write failing tests**: `hunger_rises_and_slows`, `collapse_until_rested`, `robot_battery_and_charger`, `night_shortens_vision`, `inhabitants_spawn_and_act`, `needs_section_in_observation`, `survival_world_resume_equivalent` (assert_resume_equivalent).
- [ ] **Step 2–4: Implement, run, verify --quick; screenshot a Survival world at night.**
- [ ] **Step 5: Commit** — `feat(testbed): needs, day/night and inhabitants from the world preset`

### Task 17 (G3): Worlds screen and New-agent choice in the menu

**Files:**
- Modify: `apps/game_menu/src/{screens.rs,explorer.rs,launch.rs}` (Main gets **Worlds**: list with validity, New/Duplicate/Edit/Save; form with every switch and number, defaults shown, validation live; New-agent screen picks actor preset + world preset; Resume rows show both), `apps/actor_testbed` + `npcctl serve` (`--world-preset NAME`)
- Test: game_menu tests

- [ ] **Step 1: Write failing tests**: `worlds_screen_lists_presets`, `editing_revalidates`, `save_writes_the_world_file`, `new_passes_both_presets`, `resume_row_shows_world_preset`.
- [ ] **Step 2–4: Implement, run; screenshots of the Worlds screen and New-agent screen.**
- [ ] **Step 5: Commit** — `feat(game_menu): world presets screen and choice`

## Phase H: The actor's map (landmarks)

Spec: `docs/superpowers/specs/2026-10-25-actor-map-and-memory-design.md` M2–M8. Runs after Phase G (the map switch is a world-preset system), before Phase F.

### Task 21 (H1): Map store, agent and player methods, CLI and MCP

Phase I applies: the agent map methods and MCP tools are session-bound like the other agent tools (no `unit`/`controller` in MCP; `session` required under `RegistrationRequired`).

**Files:**
- Create: `crates/npc_agent_io/src/map.rs`
- Modify: `crates/npc_agent_io/src/{notes.rs (share the file-format, stamp, mtime and atomic-write helpers; no behaviour change for notes),api.rs,remote.rs,session.rs,mcp.rs,lib.rs}`, `apps/npcctl/src/agent.rs` (+ player `npcctl map … --as player`), `docs/debugging.md` §4, `CHANGELOG.md`
- Test: `crates/npc_agent_io/tests/map.rs`, `apps/npcctl/tests/agent.rs`

**Interfaces:**
- Produces: `pub struct Landmark { pub id: u32, pub name: String, pub pos: [i32; 2], pub kind: Option<String>, pub thing: Option<SimId>, pub note: String, pub created: Stamp, pub updated: Stamp }`; `pub enum Place { Here, Thing(SimId), Pos([i32; 2]) }`; `pub struct NewLandmark { pub name: String, pub place: Place, pub kind: Option<String>, pub note: String }`; `pub struct LandmarkChange { pub name: Option<String>, pub kind: Option<Option<String>>, pub pos: Option<[i32; 2]>, pub note: Option<String> }`; `MapStore` (one per agent folder, inside `NotesStores`' folder mapping) with `list() -> Vec<Landmark>` (by id), `add(new, resolved_pos, thing, stamp, writer) -> Landmark`, `update(id, expected: Stamp, change, stamp, writer) -> Landmark` (stale → rejected like notes).
- Files: `map/<name>.md` in Task 24's Obsidian format and store machinery (frontmatter `type: landmark`, `name`, `pos: [x, z]`, `kind`, `thing: "#ID"`, stamps, `revision`; renamed when the name changes, found by `id`; the note as body). Owned file (never in `GENERATED`, never in the save).
- Methods: `agent/map/list|add|update` (controller check as `agent/notes/*`; `list` works for Observers) and `map/list|add|update` (player). `add` params `{unit, name, place: "here" | {thing: ID} | {pos: [x, z]}, kind?, note?}`. `Here` resolves to the unit's `SimPos`; `Thing` resolves through the unit's `PerceptionMemory` (sensed or remembered), else rejected `unknown_thing`. Limits: name ≤ 200 chars, one line; note ≤ 8192 bytes. Reasons: `no_map` (the world has no map; until Task 22 the map is on in every explorer world), `unknown_thing`, `unknown_landmark`, `stale`, `invalid`.
- CLI: `npcctl agent map list|add|update …` (`--here` | `--thing ID` | `--pos x,z`, `--kind`, `--note`, `--id`, `--expected-tick`, `--expected-date`). MCP tools `map_list`, `map_add`, `map_update` with every field in their schemas.

- [ ] **Step 1: Write failing tests**: `add_here_and_list`, `add_at_a_remembered_thing`, `unknown_thing_is_rejected`, `update_moves_and_renames_but_keeps_the_file`, `stale_update_is_rejected`, `hand_edit_is_picked_up`, `limits_are_enforced`, `player_and_agent_writers`, `map_writes_leave_the_hash_unchanged`, npcctl `map_add_list_update`, `mcp_lists_map_tools`.
- [ ] **Step 2: Run** `cargo test -p npc_agent_io --test map` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the tests and `cargo xtask verify --quick` — Expected: PASS / `"ok":true`.
- [ ] **Step 5: Commit** — `feat(agent_io): the actor's map of landmarks`

### Task 22 (H2): Using the map: observe, walking to a landmark, the world switch, AGENTS.md

**Files:**
- Modify: `crates/npc_agent_io/src/{observe.rs ("Your map:" text line and a `map` JSON array),act.rs or api.rs (resolve `landmark` in Move orders before submit)}`, `content/testbed_content/src/worlds.rs` + `worlds/*.json` (`SystemsSpec.map: bool`, on in every shipped preset), `content/testbed_content/src/explorer/{host.rs,workspace.rs}` + `prompts/{agents.md,common.md,resume*.md}`, `CHANGELOG.md`
- Test: `crates/npc_agent_io/tests/map.rs`, `content/testbed_content/tests/worlds.rs`, workspace tests

**Interfaces:**
- `observe` (text): a line `Your map: #3 North gate 42 m NE, #1 Well 12 m S, …` with up to 5 nearest landmarks (id, name, distance, direction in the existing observe convention); JSON: `map: [{id, name, pos, distance_mm, direction}]` (same 5). No line and no field when the world has no map or the map is empty.
- `agent/act` / `act_and_wait` / `steps`: an `order` of `{type: "Move", landmark: N}` (instead of `pos`) is rewritten to `{type: "Move", pos: <landmark N's pos now>}` before it is submitted; unknown id → rejected `unknown_landmark`. Recorded orders, replays and hashes only ever contain `pos`. Player RTS orders are unchanged.
- World switch: `SystemsSpec.map` (Task 15's struct gains `map: bool`, default true in every shipped preset); off → every map method rejects `no_map`, no observe line, and AGENTS.md has no map section.
- AGENTS.md: a "Your map" section (when to add a landmark, the three placements, walking to `landmark: N`, the CLI lines); the resume prompts list the map after memory.

- [ ] **Step 1: Write failing tests**: `observe_lists_nearest_landmarks`, `observe_has_no_map_line_without_a_map`, `move_to_a_landmark_arrives`, `move_to_unknown_landmark_is_rejected`, `recorded_order_has_the_position`, `map_off_rejects_no_map`, `agents_md_teaches_the_map` (and none when off), `resume_lists_map_after_memory`.
- [ ] **Step 2–4: Implement, run, verify --quick.**
- [ ] **Step 5: Commit** — `feat: observe and walk by the actor's map; map switch in world presets`

### Task 23 (H3): Map in the game: F7 Map tab and landmark markers

**Files:**
- Modify: `content/testbed_view/src/explorer_panel.rs` (F7 gains a **Map** tab: list, add — default place "here" for the played actor — edit name/kind/position/note, stale save handled like notes), `content/testbed_view/src/` (landmark markers: a labelled marker per landmark, read from the host-side map store, drawn under any fog), `CHANGELOG.md`
- Test: testbed_view tests

- [ ] **Step 1: Write failing tests**: `f7_map_tab_lists_and_adds`, `f7_map_stale_save_is_reported`, `markers_follow_the_store`, `markers_show_under_fog`.
- [ ] **Step 2–4: Implement, run, verify --quick; screenshot a world with landmarks under fog.**
- [ ] **Step 5: Commit** — `feat(testbed_view): map tab and landmark markers`

## Phase F: Docs, end-to-end, verify

### Task 18 (F1): Docs, end-to-end run, full verify

**Files:** `README.md`, `docs/agent-experiments.md`, `CLAUDE.md` crate map, `CHANGELOG.md`, `docs/_notes/notes-workspace-e2e.md`

- [ ] **Step 1: Docs** for notes, the workspace, detached mode, fog, actor presets/editor, world presets, memory span and the map.
- [ ] **Step 2: End to end** (an agent plays from the generated folder using only MCP tools under the generated settings — run Claude Code headless `claude -p` in the agent folder if available, else an automated agent following `AGENTS.md`): New with the rover preset, detached; the agent writes diary/memory/proposals through the tools; the dev adds a memory entry with `npcctl notes add`; Open in window (same port, same tick, fog shows the explored area); F7 shows all notes; close → keep running detached; Stop. Record in `docs/_notes/notes-workspace-e2e.md`, with the list of checks that still need a human.
- [ ] **Step 3: Run** `cargo xtask verify` — Expected: `"ok":true`.
- [ ] **Step 4: Commit** — `docs: notes, workspace, detached mode, fog of war and actor presets`
