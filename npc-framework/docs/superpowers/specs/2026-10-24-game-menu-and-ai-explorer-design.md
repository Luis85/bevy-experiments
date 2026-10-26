# Game menu and the AI explorer testbed: design

Date: 2026-10-24. Status: draft for approval.

## 1. Intent

Two things in one increment, made for the game dev who builds this framework:

1. **A game menu.** One program to start. It lists everything playable: the village, the Actor Testbed's
   scenarios, the RTS units demo and the new AI explorer, plus a settings screen. Today these are three
   separate programs, and the dev has to remember command-line flags to start a given scenario.
2. **The AI explorer testbed.** A world with one actor in it, played by an AI agent the dev runs in a tool of
   their own choice (Claude Code, Codex, another chat). The game hands the dev a ready prompt to paste. The
   agent then becomes the actor: it explores out of curiosity, keeps a markdown diary as its memory, and writes
   improvement proposals for the game. The dev watches in the game window and reads the diary and proposals
   in game as they appear. A world can be resumed exactly where it was left.

Why: the dev wants to watch an AI live in the world, and wants the AI's view of the game (what confused it,
what it missed, what it would add) as a steady source of ideas. The proposals are written for the dev, who
decides which ones they like and pursue.

## 2. Goals

- G1. `cargo run -p game_menu` (name: builder's call) opens a window with a menu. From it the dev starts any
  entry without typing flags. Closing a started game returns to the menu.
- G2. A settings screen sets the **agents folder** (where every agent's files live). It persists between
  launches.
- G3. The AI explorer has a **New** / **Resume** step:
  - New: the dev names the agent and the world. Both fields are prefilled with random names the dev can edit.
  - Resume: the dev picks an existing agent and world from the agents folder.
- G4. Starting or resuming opens the world in the game window with a **modal** showing the agent prompt
  (start prompt or resume prompt), a **Copy** button that puts it on the clipboard, and **OK**, which closes the
  modal. The sim waits while the modal is open.
- G5. The prompt is self-contained and tool-agnostic. It tells the agent who it is (name, world), its role
  (a curious explorer), how to connect and act (the exact URL, unit id, controller name and an absolute path
  to `npcctl`, with MCP as the alternative), and how to keep its diary and proposals (exact paths and
  formats). On resume, it tells the agent to reread its diary and the proposals (including the statuses the
  dev changed) first.
- G6. Files per agent, in a folder named after the agent:

  ```
  <agents folder>/
    <agent name>/
      start_prompt.md       the prompt the modal showed on New
      resume_prompt.md      the prompt the modal shows on Resume (rewritten each resume)
      diary.md              written by the agent: its journal and memory
      proposals/            written by the agent: one markdown file per improvement proposal
      <world name>.world.json   the save (section 5)
  ```

  One agent has one world in this increment (see non-goals). Folder and file names are the names made
  filesystem-safe (a slug); the real names are kept inside the files.
- G7. **Resume restores the world exactly** as it was left: the actor's position and inventory, doors, items,
  the clock, the controller attachment. Not approximately.
- G8. An **in-game panel** (toggle key, like F5/F6) shows the diary entries and the proposals while the agent
  writes them. It picks up changes on disk within about two seconds.
- G9. The explorer world is **a mix**: the familiar compound (workshop, locked store, doors, key, tools, ore,
  trees, cart) in the middle of a larger open area with more to find outside the walls.

## 3. Non-goals

- No AI agent built into the game. The game never calls an LLM. The agent runs in the dev's own tool.
- No multiplayer agents: one actor, one agent per explorer world.
- One world per agent. An agent folder holds one world save. Several worlds per agent can come later.
- No in-game editing of proposals. The dev changes a proposal's status by editing its file. The panel only
  shows it. (My call; easy to add later.)
- No in-process world switching. The menu starts each game as its own process (section 4).
- No release packaging or installer. This is a dev-machine tool (BRP is a dev tool, `docs/debugging.md` §3.2).
- No new sim mechanics. The explorer world is built from the testbed's existing content kinds, plus new
  layout and placements (G9).

## 4. Anti-goals (a failure even if it technically works)

- The agent can cheat: the prompt must forbid `world.mutate_*`, `npc/command` and editing game files. The
  agent acts only through `agent/*` (validated, recorded actions).
- Resume restores a world that is subtly different from the one left, without saying so (section 5.3).
- The game's view or panels change sim results (golden rules 1 and 5; view rules: read-only on sim state).
- The prompt only works for one AI tool, or needs the dev to fill in ports, paths or ids by hand.
- The menu becomes the only way to start things: every existing app keeps its flags and works on its own.

## 5. How it fits the codebase

### 5.1 The menu: a launcher process (decided by me; the builder may revisit)

Each existing app builds its Bevy `App` around one scenario at startup (`apps/actor_testbed/src/main.rs`
`main`, `testbed_content::try_new_sim_app_with`). A windowed app cannot rebuild itself in process the way
`npcctl serve` does for `world/reset` (`crates/npc_agent_io/src/session.rs` `rebuild`). So the menu is a new
windowed app that **starts the chosen game as a child process** with the right flags, hides or minimises
itself while the child runs, and comes back when the child exits.

- Entries: AI explorer (New / Resume), Actor Testbed (every embedded scenario from
  `testbed_content::scenario`, listed by name with its one-line description from
  `content/testbed_content/README.md` or a new table), Village, RTS units demo, Settings, Quit.
- Finding the games: sibling executables in the menu's own target directory. If one is missing, the menu
  shows the `cargo build -p <name>` command and, when it runs from the workspace with cargo on the PATH,
  offers to build it with a "building…" state. (My call.) `npcctl` is needed for the prompt (G5), so the menu
  treats it like the other games.
- Settings: a JSON file in the per-user config directory (for example `%APPDATA%\npc-framework\settings.json`).
  The default agents folder is `<home>/npc-agents`. (My call.) The folder is a text field, validated as
  creatable. No native folder picker, to avoid a new dependency.
- UI: Bevy 0.20 `bevy_ui` with `ui_widgets` (the cheatsheet notes `Button`/`Interaction` are deprecated for
  `ui_widgets`). Clipboard: `bevy_clipboard`, already in the lockfile. Check both APIs in the cheatsheet or
  the registry source, per CLAUDE.md.
- Workspace step: a new crate that depends on `bevy` must be added to the `bevy` entry's `wrappers` in
  `deny.toml`, a protected file. Follow `docs/adding-a-host-app.md`: stop and ask the dev to make that
  one-line edit. The same goes for any change `arch_fitness.rs` would need.

### 5.2 The explorer mode in the Actor Testbed

The explorer runs in `actor_testbed`, started by the menu with new flags (names are the builder's call), for
example `--explorer <agent folder> --world <world name> [--resume]`. Without them the testbed behaves exactly
as today.

- **World (G9):** a new embedded scenario `explorer` with a new layout (for example `Layout::Frontier`): the
  existing compound centred in a larger grid (`GRID_CELLS` is 64 today, the compound 24×20 cells), with
  outside features made of existing kinds: groves of trees, ore rocks, scattered items (book, bread, logs, a
  scroll), and a ruin (walls with a gap, something inside worth finding). Furnished compound, store locked,
  per-roll detection. **Every explorer world uses the same hand-built map** (the dev's decision): only the
  seed (dice rolls) and the names differ, so diaries and proposals from different agents can be compared.
  The exact map is the builder's call; it should reward looking around (things out of sight from the start,
  a reason to find the store key).
- **The actor:** one scout-like unit, its scenario name being the agent's name (shown in the overlay and the
  F5 sheet). Its facing follows its movement (no `fixed_facing`). Pre-attached controller `explorer` in
  `Direct` mode, `on_idle: Idle`, through the scenario's `controllers` list, as `agent_village.json` does.
  While no agent acts, the actor stands still.
- **New vs resume:** New builds the `explorer` scenario with a fresh random seed and the chosen names. Resume
  rebuilds from the save (5.3).
- **Modal (G4):** a view-side UI overlay drawn by `testbed_view` (or a new view module), opened on startup in
  explorer mode. `SimControl` stays paused while it is open; OK unpauses. Copy uses the clipboard. The modal
  shows the URL and port it actually bound to (another game may hold 15702).
- **Prompts:** generated by the app from a template (section 6), written to `start_prompt.md` /
  `resume_prompt.md` whenever the modal opens, so they always match the current port, paths and names.
- **Panel (G8):** a view-only panel listing diary entries (split on `## ` headings, newest first) and
  proposals (title and status from each file's header). It polls file modification times about every
  second. It never touches sim state. A free function key, chosen by the builder (F7 if free), bound in the app.

### 5.3 Save and resume (G7): a world snapshot

The dev decided on **real state saving**, not replay from tick 0. Worlds may live for many hours and many
sessions, and a resume must not take longer the older the world gets. This is a **new generic framework
capability**, not explorer-only code. It goes in a core crate (`npc_kernel` or `npc_tools`, the builder's call
per ADR 0012) behind the facade, with its own ADR (CLAUDE.md workflow step 5; `.claude/rules/content.md`:
"propose the core change separately (spec + ADR)"). Any host app can use it. The village and the RTS demo
can adopt it later; this increment only wires it into the explorer.

- **What a snapshot holds:** everything that decides the next tick. That is every registered sim component
  (`register_sim_component` already requires `Reflect` and registers the type, so reflection-based
  serialization covers them), the sim resources (`register_sim_resource`), the clock, the id allocator, the
  behaviour-tree and action state, reservations, orders, controller attachments, the pending command queue,
  and whatever else the state hash and the next tick read. Derived caches (spatial grid, indexes) are
  rebuilt on load, not stored. The snapshot also records its format version, the content hash, the scenario
  document and seed it started from, and a header: agent name, world name, wall-clock dates, tick.
- **The invariant, and the test that proves it:** for any tick T, *save at T → load → run N ticks* gives the
  same state hash at every checkpoint as *run to T + N without saving*. Test it on the explorer, the full
  testbed scenario and the village, at several T (including mid-action, mid-walk, mid-craft, a door
  half-way, a unit mounted in a vehicle). A sim that loads equal but runs differently has a hidden piece of
  state the snapshot missed; this test is how the builder finds them.
- **Saving:** autosave every 60 s of wall time and on quit. Write a temp file, then rename it, so a crash
  never leaves half a save. File: `<world name>.world.json` (or a binary format if JSON is too big or slow,
  the builder's call). Saving must not change sim results (golden rule 5): it reads the world, between ticks.
- **Resuming:** build the app from the snapshot's scenario and seed (so content and statics exist), then
  replace the sim state with the snapshot, before the first rendered frame. The modal then shows the resume
  prompt.
- **When the save does not match the build** (format version or content hash differ: the dev changed the
  game), never resume silently. The modal says what differs and offers **two buttons**:
  - **Load anyway:** load what still fits. Unknown components and resources are skipped, new ones get their
    defaults, and every skipped or defaulted item is listed. The world continues, possibly drifted. The
    resume prompt tells the agent its world may have changed.
  - **Fresh world, keep the diary:** a new explorer world with the same agent name, world name and folder.
    The diary and proposals stay, the old save is kept next to it as `<world name>.world.<date>.json`, and
    the resume prompt tells the agent its world was rebuilt and that what it remembers may no longer be
    there.
- **Replay stays as a tool:** the explorer may still record its session commands (`--record`) for debugging
  and `npcctl replay` analysis, but resume never depends on them.

### 5.4 The determinism boundary

Names, random name generation, settings, file I/O, the modal, the panel and autosave timing are all
host/view side, and none of it enters sim state. The world seed is chosen from wall-clock entropy by the
app on New, and stored in the save. The sim only ever sees the seed and the scenario document.

## 6. The prompt

One template with start and resume variants, in a file the builder can edit without recompiling, if
convenient (for example embedded with `include_str!` from `content/testbed_content/prompts/`). It
condenses `docs/agent-experiments.md` §5 and the `/play-actor` loop, with every placeholder filled by the
game:

- **Who you are:** "You are {AGENT}, living in the world {WORLD}." Its character is a curious explorer: it
  wonders about its world, goes to see what's beyond the walls, tries things, and notes what it learns.
- **How the world works:** real time at about 20 ticks/s, the world keeps running while it thinks.
  Positions are mm `[x, z]`, +z is north, 1000 = 1 m. It only knows what its senses report.
- **How to act:** the absolute `npcctl` path, the URL, `--unit {ID} --controller explorer`, the
  observe → act → wait loop with the exact commands (observe `--format text --brief`, act by affordance id
  with `--expect-tick/--expect-seq`, `events --until-idle`), how to walk and how to interact. MCP as the
  alternative, with a ready `.mcp.json` snippet.
- **Rules:** act only through `agent/*`. Never `world.mutate_*` or `npc/command`, never edit game files. If an
  action is rejected, read why and adapt.
- **Diary** (`{DIR}/diary.md`): append-only, one `## ` entry per sitting or notable moment, starting with the
  sim tick, then what it did, saw, learned and wonders about next. It is the agent's only memory between
  sessions. Never rewrite old entries.
- **Proposals** (`{DIR}/proposals/NNN-<slug>.md`): one file per idea, with a header (`title`, `status: new`,
  `written` sim tick and date, `prompted by`: the moment in play that led to it), then the problem as the
  agent experienced it and the proposed change. Only the dev changes `status` (`liked`, `pursuing`,
  `rejected`). The agent never proposes an idea already there under another status.
- **Resume variant:** first read `diary.md` and every proposal (note the statuses the dev set), then observe,
  then continue.
- **Pacing:** no fixed goal and no fixed end; keep exploring until the dev stops it. Write to the diary at
  least every few actions.

## 7. Proving it

- Snapshot (core): the save → load → run invariant of 5.3 on the explorer, the full testbed and the
  village, at several ticks including mid-action states; a property test with random commands, like the
  village's `slow_random_commands_keep_invariants`, saving and loading at random ticks; a snapshot with a
  different format version or content hash is reported, never loaded silently; "load anyway" lists what
  it skipped.
- Headless tests (content side): the `explorer` scenario loads and its ids are stable; the prompt template
  has every placeholder filled (no `{` left) for the start, resume, "world may have changed" and "world
  rebuilt" variants.
- View: `/verify-visual` screenshots of the menu, the modal, and the panel with a sample diary and proposals.
- An end-to-end run: start the explorer from the menu, paste the prompt into Claude Code, and watch the
  agent explore, write diary entries and at least one proposal. Then quit, resume, and check the agent
  continues from its diary in the restored world.
- `cargo xtask verify` prints `"ok":true`. Existing replays, goldens and snapshots unchanged (the new
  scenario is new content, so existing scenario ids do not move).
- CHANGELOG line; docs: README "Quick start" gets the menu; `docs/agent-experiments.md` gets the explorer.

## 8. Open decisions

### 8.1 Decided by the dev

- A save that no longer matches the build: two buttons, "Load anyway" and "Fresh world, keep the diary" (5.3).
- Saving is a real world-state snapshot, so worlds can live long (5.3), not replay from tick 0.
- Every explorer world is the same hand-built map (5.2).

### 8.2 Calls I made (check while reading)

- The menu is a separate launcher process, not in-process world switching (5.1).
- The explorer lives in `actor_testbed` behind new flags, not in a new app (5.2).
- The snapshot is a generic core capability with its own ADR, wired only into the explorer in this
  increment (5.3).
- The world runs whenever its window is open (except behind the modal); it does not pause when the agent
  is idle or disconnected.
- Settings in a per-user config file; default agents folder `<home>/npc-agents`; a text field, no folder picker.
- One world per agent; proposals' status is edited in the file, not in game.
- The sim is paused while the modal is open; the actor stands still until the agent acts.
- Controller name `explorer`, `Direct` mode.
- No-cheating is a prompt rule, not enforced: BRP stays fully open in explorer mode, so the dev's own
  debugging and `/verify-visual` keep working. The prompt also forbids `npc/pause`, `npc/step` and speed
  changes. A stray `world.mutate_*` call is not detected; the snapshot simply saves the world as it then is
  (only a `--record` replay of that session would no longer verify).
- One game at a time: the menu tracks one child process and stays hidden while it runs. Explorer mode still
  binds a free port when 15702 is taken, and the prompt names the port it got.
