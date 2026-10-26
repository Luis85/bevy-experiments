# Actor map (landmarks) and longer perception memory: design

Date: 2026-10-25. Status: draft for approval. Extends
`2026-10-25-agent-notes-and-workspace-design.md` (notes, agent folder, fog of war, actor and world presets)
and is implemented as part of its plan.

## 1. Intent

The dev watched an AI agent explore: it travelled from one end of the world to the other, and by the
time it arrived it had forgotten everything it had discovered on the way. Two things are missing:

1. **Memory that lasts a journey.** Today a thing that leaves the actor's senses is forgotten 10 s after
   it was last sensed (`npc_perception::DEFAULT_FORGET_AFTER = 200` ticks), and at most 32 things are
   kept (`DEFAULT_MEMORY_CAPACITY`). The explorer world uses these defaults. The dev decided: **actors
   remember things for up to a minute.**
2. **A map.** For anything worth keeping longer than that, the actor keeps a **map of landmarks**. The
   agent wanders and decides for itself whether something it found is worth adding. Landmarks help it
   find things later: it lists its map and walks to a landmark by name instead of remembering
   coordinates.

Why: an explorer that forgets its journey cannot build on what it found; the notes (diary, memory) hold
words, but not places the agent can walk back to.

## 2. Goals

- M1. **Perception memory of one minute** in explorer worlds: a thing no longer sensed is remembered
  (with confidence fading as today) until 60 s after it was last sensed (`forget_after_ticks = 1200` at
  20 Hz). Capacity rises so a minute of travel is not evicted early (my call: 128 things as the target;
  the builder may lower it to no less than 64 if perf budgets need it, and says so).
  `PerceptionConfig` is a world-wide resource, so this applies to every perceiving actor in an explorer
  world (the agent's actor and, later, its inhabitants) — consistent with "actors remember things for up
  to a minute". It is set where an explorer world is built (the window-free host), never in the shared
  testbed scenarios, the village or the framework defaults. When world presets arrive (G12), it becomes
  a tunable "memory" number in their perception rules, default 60 s. Actor presets (G11) do not override
  it (my call: one value per world keeps the hash and the presets simple).
  Old explorer saves resume with the new value (my call: the dev wants the longer memory now); the host
  applies it after loading, and the resume must still succeed.
- M2. **Map capability (framework, like notes).** Per actor: a map of **landmarks**. Operations
  **list**, **add**, **update**, for the AI agent (as the unit's controller) and the player (UI, CLI),
  with the same writer rules as notes (only agents and players write; the game never adds landmarks for
  NPCs nobody plays; no delete; optimistic concurrency on update).
- M3. **A landmark** holds: id, **name** (one line, ≤ 200 chars), **position** (`[x, z]` mm), an optional
  **kind** (free word: gate, well, grove, store, …), an optional **thing** it marks (`#ID`), a **note**
  (≤ 8 KB), and created/updated stamps (sim tick + wall-clock date, like notes).
  Where it can be placed (my call): at the actor's own position (`here`), at a thing the actor currently
  senses or remembers (its last known position at that moment), or at an explicit position. The
  position is fixed once stored: a landmark does not follow the thing it marks. `update` may change the
  name, kind, position and note (not the id or the thing).
- M4. **Stored as one markdown file per landmark** in `map/` in the agent folder, in the Obsidian format
  of `2026-10-25-notes-as-obsidian-files-design.md` (superseding the earlier `NNN-<slug>.md` header
  format): file name = the landmark's name, renamed when the name changes; YAML frontmatter with `id`,
  `type: landmark`, `name`, `pos`, `kind`, `thing`, stamps; the note as body. The landmark id (used by
  `landmark: N`) lives in the frontmatter and never changes. Owned files: never rewritten by workspace
  generation, never in the world save, not sim state; written atomically; hand edits outside the game
  are picked up (mtime) like notes.
- M5. **Using the map.** `observe` shows a short "Your map:" line with the nearest landmarks (my call: up
  to 5, each with id, name, distance and direction in the same convention as the existing observe
  lines), and the JSON observation carries them as a structured array; `map_list` gives the full list
  with positions. A walk can target a landmark: in the agent's `act`/`act_and_wait` (MCP, CLI, BRP,
  session) a `Move` order may name `landmark: N` (N = landmark id) instead of `pos`. The agent layer
  resolves it to the landmark's position at that moment before the order is submitted, so recorded
  orders, replays and the hash only ever see positions; an unknown id is rejected (`unknown_landmark`).
  Player RTS orders are unchanged.
- M6. **Same in both modes and all channels:** agent methods (beside `agent/notes/*`), player methods,
  `npcctl agent map …` / `npcctl map … --as player`, MCP tools `map_list`, `map_add`, `map_update`.
  `AGENTS.md` and the prompts teach it ("add a landmark when you find something you may want to come
  back to"; the resume prompt lists the map after memory).
- M7. **In the game UI:** the F7 notes editor gets a **Map** tab (list, add, edit). In the windowed view
  landmarks are drawn as labelled markers on the ground. A landmark is the actor's own knowledge, so its
  marker shows wherever it is, under grey or black fog too. A detached world has no view; its map is the
  files.
- M8. **A world system, on for AI-agent worlds.** The map is a switch in world presets (G12 "world
  systems"), on in every shipped explorer preset. When it is off, the map methods reject with reason
  `no_map`, `observe` has no map line and `AGENTS.md` does not mention it. The map is built after world
  presets in the plan, so the switch lands with it; until then it does not exist.

## 3. Non-goals

- No automatic landmarks; NPCs nobody plays have no map entries.
- No path planning to unexplored places beyond today's `Move` (the walk to a landmark is an ordinary
  Move to its position).
- No deleting landmarks (rename or update the note, e.g. "gone").
- The map is not sim state: not in the hash, snapshot or replays; writing it never changes the world.

## 4. Anti-goals

- The longer memory changes the village, any non-explorer scenario, or an existing replay or golden
  without the re-baseline procedure.
- The agent needs file access to keep its map.
- A landmark write blocks a frame or changes the sim.

## 5. How it fits (for the builder)

- Memory: `PerceptionConfig { forget_after_ticks, memory_capacity }` is a world resource (inserted by
  `content/testbed_content/src/scenario.rs`); the explorer host overrides it after the world is built or
  loaded. `memory_capacity` also caps how many things one scan senses (nearest first), so check the
  perception perf budgets and tests. `PerceptionConfig` is hashed: the explorer world's hash changes. If
  a replay or golden covers an explorer world, stage a re-baseline (`docs/rebaselining.md`) and ask.
  The memory change is a small task early in the plan (the dev is hitting it now), ahead of the map.
- Map store: reuse the notes store machinery in `crates/npc_agent_io` (file format helpers, stamps,
  revision suffix, mtime re-read, atomic writes, `NotesDirs` folder mapping) for a `map` book with its
  own header fields; methods in `api.rs` beside `agent/notes/*`; BRP and session; `npcctl`; MCP.
- Landmark from a thing: resolve `#ID` through the actor's perception memory (what it senses or
  remembers), never through world truth; an unknown id is rejected (`unknown_thing`).
- `landmark: N` in a Move order: resolved in the agent layer before submit (rejected `unknown_landmark`).
- View: markers in `testbed_view` read the map store (host-side resource), never sim state.

## 6. Proving it

- Memory: a test that an explorer actor still remembers a thing 59 s after losing sight of it and has
  forgotten it after 61 s; the village and existing replays/goldens unchanged.
- Map: operations, both writers, stale update rejected, file round trip, hand edit picked up, `no_map`
  when off, `here`/thing/explicit placement, unknown thing rejected, `landmark: N` Move reaches the
  landmark, map writes leave the hash unchanged.
- MCP/CLI/BRP/session like the notes tests; `AGENTS.md` names the map tools.
- `cargo xtask verify` prints `"ok":true`.

## 7. Decided by the dev

- The agent decides what is worth adding to its map while it wanders.
- Landmarks are persisted as one markdown file each, in a `map` folder in the agent folder.
- The map is on for the AI-agent world.
- Actors remember things for up to a minute.

## 8. Calls I made (check while reading)

- One minute of memory for every perceiving actor in an explorer world (a world setting, later a world
  preset number), not per actor preset; capacity 128 (no less than 64 if perf needs it). Framework
  defaults, village and other testbed scenarios unchanged. Old explorer saves resume with the new value.
- Placement: `here`, a sensed/remembered thing, or an explicit position; the position stays fixed;
  update changes name, kind, position, note. File names follow the name (Obsidian design).
- No delete; optimistic concurrency like notes; same writer rules.
- `observe` shows up to 5 nearest landmarks (text and JSON); the agent's Move can target
  `landmark: N` (the id), resolved before submit; player orders unchanged.
- F7 gets a Map tab; landmarks drawn as labelled markers, shown under any fog.
- The map is a world-preset switch, on in all shipped explorer presets; it is built after world presets.
