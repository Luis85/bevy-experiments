# Agent notes, prepared agent workspace and detached explorer: design

Date: 2026-10-25. Status: draft for approval. Builds on
`2026-10-24-game-menu-and-ai-explorer-design.md` (the explorer, the menu, world snapshots), on the polish
pass that followed it, and on the "sense walls and openings" work (branch `claude/busy-mclaren-b26bd4`).
Implementation starts once both are merged into `main`.

## 1. Intent

Today an AI agent playing the explorer keeps its diary and proposals by writing markdown files itself,
using whatever file tools its AI tool has. That gives the agent broad file access, makes the formats
fragile, and leaves the game blind to what the agent writes until it re-reads the files.

The dev wants three changes:

1. **Notes are part of the game.** Every actor can have a **diary**, a **memory** (long-lasting facts it
   wants to keep) and **improvement proposals**. They are a native capability of the framework, like
   perception or inventory. An AI agent or a player lists, adds and updates entries **through the game**,
   and the game stores them. The game itself never writes into them for NPCs that nobody plays.
2. **A prepared agent folder.** Creating an agent creates a dedicated folder the agent works out of. It
   holds an `AGENTS.md` that explains the scenario and the available tools, every generated prompt, and a
   `.claude/settings.json` that grants only the permissions needed to interact with the world.
3. **Detached mode.** A world can run without a window, started from the menu or from a terminal. The
   notes work the same way there. The dev can **open a detached world in the window** to watch the
   agent: the world moves from detached into the window, and the agent carries on.

Why: the agent's persistent storage should be reliable and owned by the game; the agent should need no
file access at all; and the dev wants to run worlds without a window and still look in on them.

## 2. Goals

- G1. **Notes capability (framework).** Per actor: `diary`, `memory`, `proposals`. Operations: **list**,
  **add**, **update**. Entries have an id, a created and an updated sim tick and wall-clock date, a
  title and a body; proposals also have a `status` (`new`, `liked`, `pursuing`, `rejected`) and a
  `prompted_by` line. Only an AI agent (through the agent interface, as the unit's controller) or a
  player (through the game UI or the CLI) writes notes.
- G2. **Same in both modes.** The notes operations are agent methods served on the same channel as
  observe/act/events: BRP in the windowed game and the headless session server (`npcctl serve`).
  `npcctl agent notes …` subcommands and MCP tools wrap them.
- G3. **Stored as files in the agent's folder**, human-readable, outside the world save: `diary.md`,
  `memory.md`, `proposals/NNN-<slug>.md` (same header format as today). They survive
  "Fresh world, keep the diary" and a resume. The game writes them atomically (temp file + rename).
- G4. **In-game editor.** The F7 panel becomes an editor: tabs or sections for Diary, Memory and
  Proposals; list entries; add an entry; edit an entry's title/body; set a proposal's status. The prompt
  modal and RTS input rules from the explorer still apply (typing in the panel must not trigger game
  keys).
- G5. **Prepared agent folder.** On New (menu or CLI), the game creates the agent's folder with:
  - `AGENTS.md`: the scenario (who you are, the world, the rules), the available tools (observe, act,
    events, notes) with exact command lines for this agent, and where the prompts are;
  - `CLAUDE.md`: one line importing `AGENTS.md`, so Claude Code reads it;
  - `prompts/start.md` and `prompts/resume.md` (and the changed/rebuilt variants when they occur);
  - `.claude/settings.json`: permissions that allow only the game's MCP tools (from `.mcp.json`) and
    reading files inside the folder; everything else denied (no shell commands at all, no file writes,
    no web) — see §7;
  - `.mcp.json`: the game's MCP server (`npcctl mcp --url …`) ready to use;
  - the notes files (created empty) and the world save.
  The folder is rewritten when the port or paths change (e.g. a different port on resume), never the
  notes or the save.
- G6. **Start flow** (unchanged in spirit): open the game, create a world and agent, get the start
  prompt, copy it, open an AI tool pointed at the agent's folder, paste the prompt. The prompt and
  `AGENTS.md` tell the agent to use the game's notes tools, not files.
- G7. **Detached mode.**
  - From the menu: New and Resume offer **Start in window** and **Start detached**. Detached, the menu
    shows the prompt (with Copy), the URL, and a status line ("<agent> in <world> running detached on
    port N, tick T"), with **Open in window** and **Stop**.
  - From a terminal: one command starts a detached explorer world (new or resume) and prints the URL and
    the prompt file paths; it saves every 60 s and on stop (Ctrl+C or `npcctl session stop`).
  - **Open in window**: the detached world saves, stops, and the same world reopens in the game window
    from that save, on the same port. The agent's next command reaches the window. The world is exactly
    the same (snapshot resume). No prompt modal blocks the sim on this switch (the agent is already
    playing); a short note says where the world came from.
  - The reverse works too: a windowed explorer world can be moved to detached (menu or a key), and
    closing the window offers to keep it running detached.

- G8. **Fog of war (view), like Age of Empires.** A setting (menu Settings, default on for explorer
  worlds; a key toggles it while watching) makes the windowed view show the world as the played actor
  senses it:
  - **visible now** (what the actor currently senses: its vision cones with line of sight, per its
    sensors): drawn normally, with units;
  - **explored, not visible**: landscape and static things greyed, shown **as last seen** (walls, doors
    in their last-seen open/closed state, trees or stumps, rocks, workstations, items lying on the
    ground); **no units** there;
  - **never explored**: black.
  Wandering lifts the fog only where the actor's own senses reach. The explored area is kept per
  world and survives save/resume and moving between detached and window (my call: stored in the
  world save as a deterministic per-actor "explored cells" record updated from perception; it must not
  change any existing scenario's state hash — it exists only for actors that opt in, e.g. the
  explorer). The F1/F5/F6 debug overlays still show everything when the dev turns fog off.

- G9. **Choice of actor.** When creating an agent (menu New, or the CLI), the dev picks the actor the
  agent takes over. To start:
  - **Human**: a person like the villager/scout, with today's senses (vision cone with line of sight,
    hearing) and today's body (walks, picks up, opens doors, uses tools and workstations).
  - **Robot**, from **ready-made models** (no custom loadouts). A robot roams, picks things up and
    interacts with the world like a human (same verbs), but senses only through its sensor components.
    First models (my call; names and numbers the builder may tune):
    - **Rover**: 360° LIDAR + a forward camera.
    - **Sentinel**: radar + camera, good at spotting things that move.
  The choice is stored in `agent.json` and the world save, shown in the menu's Resume list, and
  described in `AGENTS.md` and the prompts (the agent learns what body and sensors it has).
- G10. **Sensors that behave like real ones** (both decided by the dev): each sensor is simulated like
  its real counterpart, and its data comes out in the shapes real robots use, so a later project can
  connect the same agent to physical hardware (driving a real robot is not part of this increment).
  - **LIDAR**: a planar scan; fixed angular resolution and max range; per beam the distance to the first
    thing it hits (walls, props, units, items), via integer grid ray casts; returns no identities, only
    ranges. Output shaped like ROS `sensor_msgs/LaserScan` (`angle_min`, `angle_increment`,
    `range_min`, `range_max`, `ranges[]`), in mm/millidegrees, with deterministic noise from `sim_rand`.
  - **Camera**: **detections only, no images** (decided). Objects in its field of view with line of
    sight, each with a class (kind), bearing, distance, apparent size and a confidence, plus a tracker
    id that stays stable while the object stays in view (a real detector + tracker does not know world
    ids). Shaped like ROS `vision_msgs` detections.
  - **Radar**: wide field of view, longer range, coarse bearing/range resolution; reports targets with
    radial velocity, strongest for things that move; blocked by walls. Shaped like ROS `radar_msgs`
    tracks.
  Sensors are sim-side (deterministic, integer, recorded like perception today); a robot's sensor
  readings feed its perception memory (so its observation, its "You can:" affordances and fog of war
  follow what its sensors report) and appear as their own sections in `agent/observe`, in JSON (the
  ROS-shaped data) and as a short text summary (e.g. nearest obstacle per sector for LIDAR).
  Fog of war for a robot lifts where its sensors reach (LIDAR coverage reveals geometry; camera and
  radar reveal what they detect).

- G11. **Actor presets and the actor editor.** Actors are **presets**: readable data files (my call: one
  JSON file per preset in a presets folder set in Settings, default `<agents folder>/../npc-actors` or a
  `actors/` folder beside the agents folder) that the dev creates, edits, duplicates and can share or
  edit by hand. The game ships **Human**, **Rover** and **Sentinel** as starting presets (G9's
  "ready-made models" become these presets; no per-agent custom loadouts — an agent picks a preset).
  A preset holds:
  - name, description, body (`human` or `robot`) and **3D model** (chosen from the available models);
  - stats and attributes (the character sheet: the rules' point-buy attributes, skills, traits);
  - capabilities: movement speed, carrying capacity, and which actions it may do (open doors, pick up,
    use tools/workstations, craft, drive, …);
  - sensors and perception: any mix of vision, hearing, LIDAR, camera, radar, each with its parameters
    (range, field of view, beam count/resolution, noise), plus perception settings (detection, memory).
  The **actor editor** is a window (my call: a mode of the Actor Testbed app, launched from a new
  "Actors" entry in the game menu, so it reuses the game's 3D models): a list of presets with New, Edit,
  Duplicate and Save; a form for every field above with validation (point-buy budget, sensible
  ranges); and a **3D preview** of the actor's model, slowly turning, with a picker to switch models.
  The menu's New-agent screen lists every valid preset. Robot models: the game builds simple robot
  models from primitives (body, wheels or tracks, a sensor dome per sensor) until real robot assets are
  added (my call; adding a glTF model later is data, not code).

- G12. **World presets: the mechanics and systems of an AI-agent world** (decided by the dev: each can be
  switched on/off and its numbers tuned from sensible defaults). Like actors, a **world preset** is a
  readable data file (a `worlds/` folder beside `actors/`), edited in a **Worlds** screen of the game
  menu and picked on the New-agent screen together with the actor preset. Shipped presets (my call):
  **Plain explorer** (today's world, nothing added), **Survival** (needs on, day/night on), **Night
  shift** (short days, dark nights, per-roll detection). A preset holds:
  - **Needs**: hunger, thirst, energy/exhaustion for humans; battery charge for robots. Per need:
    on/off, rise rate, thresholds and effects when it runs low and when it runs out (slower movement;
    then collapsing/shutting down until rested/charged). The world places what meets them (food items,
    a water source, beds, charging stations) when a need is on.
  - **Day and night**: on/off, cycle length, and how much darkness shortens vision (and camera range;
    LIDAR and radar are unaffected).
  - **Perception rules**: automatic or per-roll detection.
  - **World systems**: crafting, vehicles, locked doors and keys, skill progression — each on/off.
  - **Other inhabitants**: none, or a number of NPCs going about their business (workers, skeletons).
  Validation like actor presets (sensible ranges); an invalid preset is shown with its reasons and never
  used. The chosen world preset is stored in `agent.json` and the world save, described in `AGENTS.md`
  (the agent learns which needs and rules apply), and shown in the Resume list. Plain explorer must
  reproduce today's explorer world exactly.

- G13. **Fewer round trips for agents** (decided by the dev after an agent session where it always had to
  pull and observe). An LLM agent works in turns and cannot take pushed events mid-turn, so instead of
  push the interface cuts calls per step:
  - **Act and wait in one call:** run actions, wait until the actor is idle (or a named event kind
    fires, or a timeout), and return the act result, the events and a fresh observation together.
  - **What changed since you last looked:** an observation can carry only the changes since a given
    `seq` (things newly sensed or lost, inventory changes, order and interaction results) with a short
    "Since your last look: …" text line.
  - Both are available as agent methods, `npcctl` flags and MCP tools; `AGENTS.md` and the prompts
    teach the one-call loop. Claude Code's Monitor-based push is not used (it would reopen shell
    access, against the MCP-only settings).

## 3. Non-goals

- No live viewer of a world that stays detached (the dev chose "move into the window"; a networked
  viewer is a separate project).
- The game never writes notes for NPCs nobody plays; no automatic diaries for villagers.
- No deleting entries (list, add, update only). A proposal the dev dislikes gets `status: rejected`.
- Notes are not sim state: they are not in the state hash, the snapshot or replays, and writing them
  never changes what the world does.
- Configuration for AI tools other than Claude Code is limited to `AGENTS.md` (which many tools read)
  and `.mcp.json`; only Claude Code gets a permissions file.

## 4. Anti-goals

- The agent needs file-system write access to keep its notes.
- A note write changes the sim (hash, replays) or blocks a frame.
- Switching between detached and window loses or changes world state, or loses notes.
- The prepared `settings.json` lets the agent run arbitrary shell commands or edit game files.
- Two writers (the agent and the dev in the F7 editor) silently overwrite each other's edit.

## 5. How it fits the codebase (for the builder)

- **Notes capability:** host-side, not sim. A generic notes module (in `npc_agent_io`, or a small new
  crate behind the facade — the builder's call per ADR 0012) with a `NotesStore` resource: the host
  maps a controlled unit to a folder; operations read and write that folder's files. Agent methods (names
  the builder's call, e.g. `agent/notes/list|add|update`) are added beside `agent/observe` in
  `crates/npc_agent_io/src/api.rs` (`METHODS` + dispatch), exposed over BRP (`remote.rs`) and the
  headless session (`session.rs`), and in `npcctl agent notes …` and the MCP tool list (`mcp.rs`). The
  same `controller` check as `agent/act` applies to agent writes; the UI and CLI write as `player`.
  Concurrent edits: `update` takes the entry's `updated` stamp and is rejected if it changed since
  (optimistic concurrency), so the agent and the dev never overwrite each other silently.
- **File formats:** keep today's diary heading and proposal header formats (Task 6/7 parsers in
  `testbed_content::explorer::notes`) so existing agent folders keep working; add an id per entry (an
  HTML comment or a header line) and a `memory.md` with the same entry format as the diary.
- **Agent folder:** generated by `testbed_content::explorer` (templates under
  `content/testbed_content/prompts/` for `AGENTS.md` too). `settings.json` uses Claude Code's
  permission syntax (`permissions.allow` / `deny`); check the current Claude Code docs for the exact
  rule syntax for an absolute executable path and for MCP tools.
- **Detached:** explorer startup currently lives in `apps/actor_testbed/src/explorer.rs`. Move the
  window-free parts (build/resume/save/prompt files/folder) into `testbed_content::explorer` so both
  `actor_testbed` and a headless host can use them. The headless host is `npcctl serve` with explorer
  flags (the builder may choose `actor_testbed --detached` instead if that keeps npcctl render-free and
  simpler; npcctl must never render). The menu starts detached worlds as child processes like today
  and tracks them; "Open in window" stops the detached child (graceful stop → save) and starts the
  windowed one with `--resume` on the same port. Port handover: the window binds the port the detached
  world released; if it is taken in between, the window picks another and the menu says so (the prompt
  files are rewritten; the agent will see connection errors and should re-read `AGENTS.md`).
- **Prompts:** start/resume prompts and `AGENTS.md` describe the notes tools instead of file writes;
  the resume prompt tells the agent to list its memory and recent diary through the game first.

## 6. Proving it

- Notes: unit tests for each operation and both writers; optimistic-concurrency rejection; file format
  round trip; old-format folders read correctly; a test that note writes leave the state hash unchanged.
- Agent methods over BRP and the headless session; `npcctl agent notes` and MCP tools (like the
  existing agent CLI tests).
- Folder generation: files present, `CLAUDE.md` imports `AGENTS.md`, `settings.json` valid JSON with
  only the intended rules, `.mcp.json` valid; regenerated on port change without touching notes/save.
- Detached: start, save, stop, resume; "Open in window" keeps the tick and positions (snapshot) and the
  same port; notes identical before and after.
- End to end: an agent plays a world using only notes tools (no file access) and the generated
  settings, in Claude Code started in the agent folder; the dev opens it in the window midway.
- `cargo xtask verify` prints `"ok":true`; existing replays/goldens unchanged.

## 7. Decided by the dev

- `.claude/settings.json` is **MCP-only**: no shell commands allowed; the agent uses the game's MCP tools
  (observe, act, events, notes) and may read files in its folder. `AGENTS.md` still documents the
  `npcctl` command lines for AI tools without MCP. (The MCP tool schemas must therefore list every field
  the agent needs: interact, steps, brief, until_idle, dedupe, the notes operations.)
- Fog of war like Age of Empires (G8).
- "Open in window" moves the world (no live viewer of a detached world).
- Only AI agents and players write notes.
- The agent's actor is chosen at creation: human or a ready-made robot model (G9). Robots interact like
  humans. Sensors behave like real ones and output real-robot data shapes; the camera gives detections,
  never images; connecting to physical robots is a later project (G10).
- Actors are presets made in an actor editor with a 3D model preview and a model picker; capabilities
  cover movement, carrying, allowed actions, sensors and perception (G11).
- World mechanics are configurable per world: switches plus tunable numbers from sensible defaults,
  saved as world presets edited in the menu (G12).
- Claude Code configuration (checked against the Claude Code docs): `.claude/settings.json` with
  `permissions.defaultMode: "dontAsk"`, `allow: ["mcp__npc__*", "Read(./**)"]`,
  `deny: ["Bash", "PowerShell", "Edit", "Write", "NotebookEdit", "WebFetch", "WebSearch", "Monitor"]`,
  `blockReadsOutsideWorkingDirectories: true`; `.claude/settings.local.json` with
  `enabledMcpjsonServers: ["npc"]`; `.mcp.json` with server `npc` (stdio, the npcctl path with JSON-escaped
  backslashes, args `mcp --url <url>`); `CLAUDE.md` containing `@AGENTS.md`. Project allow rules apply
  only once the dev has trusted the folder in Claude Code; `AGENTS.md` and the prompt say so.

## 8. Calls I made (check while reading)

- Entries can be added and updated but not deleted; proposals get a status the dev sets.
- Notes live in files in the agent's folder, outside the world save; they are not sim state.
- `AGENTS.md` at the folder top with a `CLAUDE.md` that imports it; prompts in `prompts/`; a ready
  `.mcp.json`.
- Optimistic concurrency for updates (reject a stale update rather than overwrite).
- No prompt modal when a world moves into the window (the agent is already playing).
- Closing the window of a running explorer world offers to keep it running detached.
- **Writers:** agent writes go through agent methods and require the unit's controller (like
  `agent/act`); player writes (F7 editor in process, `npcctl notes … --as player` over BRP/session) use
  separate player methods with no controller, so an agent cannot pose as the player. Notes work with no
  controller attached. Only a player sets a proposal's `status`; `prompted_by` is optional. Limits:
  title ≤ 200 chars, body ≤ 8 KB (rejected beyond, with the limit in the message).
- **One owner:** the running game (window or detached) is the only writer; handovers are strictly
  stop → save → start. Before each operation the store re-reads a file whose mtime changed, so edits
  made by hand outside the game are picked up. Writes are atomic (temp + rename).
- **Ids and old folders:** entries without an id get one on first read and the file is written back
  (one-time migration); text the parser cannot place is kept verbatim and never dropped by an update.
- **Notes belong to the agent folder** (one agent, one actor), not to a unit id, so they survive a
  fresh world with a different unit id.
- **Generated vs owned files:** `AGENTS.md`, `CLAUDE.md`, `prompts/`, `.claude/settings.json` and
  `.mcp.json` are generated and rewritten on every start/resume (hand edits there are overwritten,
  and an old folder without them gets them on its next resume); `diary.md`, `memory.md`,
  `proposals/`, `agent.json` and the world save are never rewritten by generation.
- **Handover:** the snapshot already carries the controller attachment and queued actions; the event
  sequence restarts (new epoch), which the prompt already tells the agent to handle. The agent sees
  connection errors for the few seconds the window takes to open and retries. If the save at stop
  fails, "Open in window" aborts and the world keeps running detached. The window retries the same
  port for up to 10 s; if it stays taken, it does not move to another port (the agent's settings pin the
  URL): it exits with an error and the menu restarts the world detached from the save.
- **Detached processes are findable:** a running world writes `run.json` (pid, port, mode, started)
  into the agent folder and removes it on exit, so a new menu instance can show, stop or open it; a
  stale `run.json` (dead pid) is ignored.
