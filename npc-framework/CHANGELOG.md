# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project aims to follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html) once the `npc_framework` facade is published.

Bevy compatibility: `0.1.x` (unreleased) targets Bevy 0.20, Rust ≥ 1.97.1.

## [Unreleased]

### Security

- `cargo deny` ignores RUSTSEC-2026-0391 (zstd 0.13.3), RUSTSEC-2026-0398 and RUSTSEC-2026-0403 (zstd-safe 7.3.0), published 2026-10-26: all come in through Bevy 0.20's `bevy_image` (`zstd_c`); the fixes need zstd 0.14 / zstd-safe 8.2, which Bevy 0.20 cannot take, and the other backend (ruzstd) has RUSTSEC-2026-0344. The flawed paths are not reached (no reused context, no `seekable` feature, only trusted embedded current-format LUT frames). The dev's decision; drop the entries when Bevy moves to zstd 0.14.

### Changed (robot sensors fix round 1, plan Task 12)

- **npc_perception**: an idle robot reuses its traced LIDAR ranges (keyed by entity, position, facing, LIDAR,
  `SpatialGrid::revision` and the obstacle cells of its scan square) and only draws its noise again; the LIDAR
  scans half a stagger after the camera/radar pass (`is_lidar_due`; `SensorReadings::lidar_tick`), never in the
  same tick. `LidarObstacles` (set with `PerceptionAppExt::set_lidar_obstacles`, not hashed; default: every kind)
  says which perceivable things stop a beam in their cell. Camera track ids skip ids still in use.
- **testbed_content**: `world::lidar_obstacles()`: units, vehicles, trees, ore rocks, workstations and chests stop
  a beam; items do not, and a door only through its cell (closed: blocking; open: the beam passes).
- **npc_framework** (`perception_agent`): the LIDAR, camera and radar text lines come together right after `You
  perceive` (their sections carry only the JSON); `1 degree`, not `1 degrees`.

### Added (LIDAR, camera and radar with ROS-shaped readings, spec G10, plan Task 12)

- **npc_perception**: robot sensors, appended to `Sensor` (`Lidar`, `Camera`, `Radar`; builders `Sensors::lidar`,
  `camera`, `radar`; hash tags 3–5, so existing encodings are unchanged) and `SenseKind::{Camera, Radar}` (tags 4, 5).
  On an agent's scan tick the perception pass also runs them and writes `SensorReadings` (snapshot-only
  `sensor_readings`, added to agents with a robot sensor; never hashed): the LIDAR's `LaserScan` (ROS
  `sensor_msgs/LaserScan` in mm / millidegrees: per beam the distance to the first blocking or occluding cell or
  cell holding a perceivable thing, a miss = `range_max_mm + 1`, `±noise_ppm` noise from `sim_rand` purpose
  `LIDAR_NOISE_PURPOSE`; no identities, no memory), the camera's `CameraDetection`s (`vision_msgs` style: class =
  kind name, track id stable while in view, bearing + = left, distance, size, confidence 990000 → 600000 ppm over
  its range; cone + line of sight) and the radar's `RadarTrack`s (`radar_msgs` style: bearing and range rounded to
  its resolution, radial velocity from the range's change since its previous scan, amplitude higher for things that
  moved; line of sight). Camera and radar feed `PerceptionMemory` **automatically** (no detection check: a sensor
  reports what is there); priority vision, camera, radar, proximity, reach. Integer angle helpers (`atan2_mdeg`,
  `dir_ppm`, `bearing_mdeg`, `facing_angle_mdeg`, offline sine/tangent tables). A 360-beam, 20 m LIDAR marks the
  reachable square once per scan in a reused bitmap and walks it with the same supercover as `trace_segment`
  (proven equal by a unit test); nothing is allocated per scan once warm. Fog of war ignores the new sensors until
  Task 13.
- **npc_kernel**: `SpatialGrid::for_each_solid` (the blocking or occluding cells of a rectangle, read by row like
  `for_each_occluding`).
- **npc_framework** (`perception_agent`): observation sections `lidar`, `camera` and `radar` (the readings as
  JSON; text: nearest obstacle per 8 compass sectors, one line per detection / track, capped at 8 with a per-class
  / moving-still summary). `fog_agent::{SECTORS, sector_of}` are public (the glue's compass). `per_roll` never
  rolls for camera or radar (own dice purposes if ever asked).
- **testbed_content** (`actors`): the rover (LIDAR 20 m / 360° / 360 beams + camera 15 m / 35°) and the sentinel
  (radar 30 m / 60° + camera 15 m / 35°), both with reach 1.5 m, are valid and get their sensors from
  `apply_preset`; `ROBOT_SENSORS_LATER` is gone; at most one `lidar`, `camera` and `radar` per preset. No hash of a
  world without robots moved (testbed, village, the human explorer world: `existing_hashes_unchanged`).

### Changed (quitting the game stops every process it started, plan Task 29)

- **game_menu**: Quit (Main's button, the window's close button, or Ctrl+C on the menu's console through Bevy's
  Ctrl+C handler) now stops every process the menu started or adopted before it exits, in parallel on worker threads,
  on a new **Quitting...** screen that lists each one and its state: tracked detached worlds get `session/stop` (they
  save first; an unanswered stop is asked again within the wait, for a world still taking its port over after F9)
  and up to 15 s to exit (one that saved but still runs is then killed); an explorer window gets `app/quit {pid}` (it
  saves; once it answered it gets up to 10 s more) and is killed through the menu's child handle when it has not quit
  in time; the Village, testbed scenarios and the RTS units demo have nothing to save and are killed at once (ruling
  R17); a world still starting is killed after 15 s; a build or a save check is killed at once (`QuitWait`). Every
  kill removes the `run.json` it leaves behind. An adopted world (a `run.json` pid) is killed only after
  `run::kill_adopted` checked the pid is still its `npcctl`/`actor_testbed` and did not start after the `run.json`; a
  reused pid is never killed (the world is forgotten, its stale `run.json` removed). An Open in window or Stop in flight
  is waited for first (nothing opens or restarts after it). A world that cannot be stopped keeps the menu on that
  screen with the world and the reason (refused: its save failed; or no answer: `StopError`), **Quit anyway** (kill;
  the save stays at the last autosave, `run.json` removed) and **Back** (it keeps running; not after Ctrl+C). This supersedes spec G7's "a detached world outlives the menu" for the menu's Quit;
  Quit is no longer disabled while a Stop or Open in window runs (`BUSY_QUIT` is gone). Processes the menu did not
  start or adopt, and `npcctl mcp` servers (the AI tool's), are never touched.
- **npc_view_helpers** (`remote`, feature `remote`): `app/quit {pid?}` (`install_app_quit`, `AppQuitPlugin`,
  `app_quit`, `quit_when_due`, `QUIT_DELAY`): answers `{quitting, pid}`, then the app exits 0 200 ms later through an
  ordinary `AppExit`, so its exit systems run; a pid that is not the app's is refused. Registered by apps only.
- **actor_testbed**, **village**: register `app/quit` with their BRP. An explorer window's `app/quit` takes the
  close dialog's "No" path: save, remove `run.json`, exit 0.
- **testbed_content** (`explorer::host`): `remote_call(port, method, params, timeout)` (`session_call` uses it),
  `remote_call_checked` (`RemoteError::Refused` / `NoAnswer`), `parse_rfc3339` (the inverse of `rfc3339`).

### Added (actor presets as data, spec G9–G11, plan Task 11)

- **testbed_content** (`actors`): actor presets, one JSON file per preset in the actors folder (`Settings::actors_dir`,
  default `npc-actors` beside the agents folder; the built-in `human`, `rover` and `sentinel` are copied into a folder
  that has no preset). `ActorPreset { name, description, body, model, sheet, speed_mm_per_tick, carry_capacity,
  actions, sensors, perception }` (unknown fields and sensor types refused); `load_presets`, `find_preset`,
  `validate` (one reason per problem: the sheet's traits, skills and limits within a 100-point budget, speed, carrying
  capacity, sensor ranges 1..=50 000 mm, reach 0..=5 000 mm and at most one, fields of view, beams, known actions
  and models, the world's detection check), `apply_preset` (sheet, speed, carrying capacity, vision/hearing, `ReachSense`,
  `AllowedActions`, `ActorModel`; both new components snapshot-only, never hashed). Until Task 12 the robot sensors
  (LIDAR, camera, radar) are refused ("this sensor arrives with the robot sensors (Task E2)"), so the rover and the
  sentinel show as invalid. A scenario actor may name a preset (`"preset": "<id>"`, applied at spawn).
- **testbed_content** (`explorer`): the explorer's actor is built from the chosen preset (`HostArgs.preset`, default
  `human`; `HostArgs.actors_dir`); an unknown or invalid preset is refused with its reasons before anything is
  written. `agent.json` stores the preset id (`AgentMeta.preset`, `human` for older folders) and the save header
  the preset itself (`preset`, `actor_preset`), so a resume and `world/reset` rebuild the same actor even after the
  file changed. A resume keys the preset by the save (a save from before presets is the built-in human); an
  `agent.json` that disagrees is corrected and the host says so (`ExplorerSession.notes`, also printed to stderr);
  resuming with another `--preset` is an error. `--fresh-world` reads the preset file again (an edit applies) and,
  when the file is gone, uses the old save's copy with a note. The preset's `reach` replaces the fixed 1.5 m
  (`EXPLORER_REACH_MM` stays the built-ins' value); every preset keeps `RevealsFog`. `AGENTS.md`'s "Your character"
  comes from the preset (`ActorSummary::from_preset`): the human renders as before, its description now the preset's
  sentence followed by speed and load. The `human` preset's world has today's content and state hashes.
- **npc_agent_io**: verb gates (`register_verb_gate`, `verb_gate`, not in the content hash): a refused verb's
  affordances say why (`why_not`) and are marked structural (`Affordance::structural`, a new unserialized flag that
  `structurally_impossible` honours whatever the reason's words), so `brief` hides them; `agent/act` rejects the
  verb (`not_allowed`). The testbed's gate refuses an actor's known actions outside its `AllowedActions` ("this actor
  cannot pick_up").
- **npc_framework** (`glue::command_agent`): `order_verb` names the verb an order makes a unit do: `Interact` its
  verb; with `vehicles`, `Mount` `mount`, `Unload` `unload`, and `Move`/`AttackMove`/`Patrol`/`Follow` `drive_to`
  for a vehicle or a unit in a driver seat (`ByCrew` or `Direct`). An `order` or `interact` act is refused
  (`not_allowed`) when a verb gate refuses that verb to an ordered unit or, for orders to another unit, to the acting
  unit itself.
- **actor_testbed** / **npcctl serve**: `--preset ID` (New) and `--actors-dir DIR`; **game_menu** passes the
  settings' `actors_dir`, when set, as `--actors-dir` to every explorer host it starts (window and detached).

### Added (searched ground in observe, spec F4, plan Task 28, proposal 004)

- **npc_framework** (`glue::fog_agent`): the perception section of an actor with an `ExploredCells` record carries
  `searched {cell_mm: 10000, radius_mm: 60000, within_radius_pct, map_pct, sectors: [{dir, explored, total}; 8]}`
  and the text line `Searched: 34% within 60 m, 12% of the map; least searched: north-east, east.`. The record
  is coarsened to 10 m cells (explored when at least half of the fine cells are); sectors are N..NW (`+z` north) with
  ties in that order (sectors of fewer than 2 coarse cells, edge slivers, are never named); `map_pct` is the explored share of the whole grid (no start position is stored). `cell_mm` is the real coarse edge; a cell of more than 400 fine cells is sampled (at most 20 x 20 tests). Read-only, computed per observe, never
  hashed: no replay, golden or state hash moves.
- **testbed_content**: `AGENTS.md` says that observations show how much nearby ground the actor has searched.

### Added (fog of war in the window, spec G8, plan Task 10)

- **testbed_view** (`fog`): the Actor Testbed window draws the fog of war of the `RevealsFog` actor (the explorer),
  like Age of Empires: never-explored ground black, explored ground not visible now greyed, visible ground clear.
  A mask plane over the ground holds one texel per grid cell (`mask_texel`: black, dark grey at alpha 200, clear;
  linearly filtered), rewritten only when a cheap hash of the explored and visible bits changes (checked once per
  sim tick). Statics in ground not visible now are drawn dimmed **as last seen** (`SeenStatics`: a door seen open
  stays open, a tree seen standing stays standing, an item where it was seen) by swapping their meshes to cached
  darkened materials; a static never seen is not drawn; wall runs are fogged piece by piece by their cells, and the
  view-only dressing by its cell. **Units and vehicles are drawn only while the actor senses them** (Ruling R14:
  `sensed` in its `PerceptionMemory`, never by the fog cells); the actor and the vehicle it rides always. Pure
  functions `unit_look`, `static_look`, `area_look`, `mask_texels`, `mask_hash`. Read-only on the sim: no state
  hash, replay or golden changes.
- **F8** toggles fog; resource `FogOfWar { enabled }`. **actor_testbed `--fog on|off`** (default on in explorer
  mode, off elsewhere; without a `RevealsFog` actor everything shows anyway). The F1/F5/F6/F7 panels are not fogged.
- **Settings** `fog_of_war: bool` (default true; older files load as true) in
  `testbed_content::explorer::settings`; the game menu's Settings screen has a **Fog of war** checkbox (saved at
  once) and explorer windows (New, Resume, Open in window) get `--fog on|off`; detached worlds have no view.
  `launch::save_settings` takes the fog flag.
- **Review fixes (Task 10 round 1).** `npc_view_helpers::ViewHidden` (new, generic): sim entities the view hides;
  the crew and path gizmos skip them, they cannot be clicked, box-selected or right-click picked, and a selected
  one leaves `Selected` / `CommandSelection` (so no selection ring or order gizmo follows it into the fog). The fog
  fills it once per tick with every visual whose look is hidden (units it does not sense, statics never seen).
  **Ruling R15:** the F1 item markers follow the item's look under fog (none when hidden, at the last-seen spot
  when drawn as last seen); the rest of F1 and F5/F6/F7 stay unfogged. A mesh whose material was not loaded when
  it should have been dimmed is retried until it is. A headless test builds and runs the whole
  `TestbedViewPlugin` (`content/testbed_view/tests/plugin.rs`).

### Added (fog of war, sim side, spec G8, plan Task 9)

- **npc_perception** (`fog`): opt-in marker `RevealsFog` brings `ExploredCells { width, bits }`, `VisibleCells { bits }`
  (one bit per grid cell, `u64` words over `GridBounds`) and `SeenStatics(BTreeMap<SimId, SeenState { kind, pos, open,
  felled, footprint }>)`. On the actor's scan ticks, right after the perception systems (`fog_system`), visible =
  the cells its vision cones (`reveal_cells`: centre in range and cone, with `line_of_sight`) and its `ReachSense`
  (`reveal_reach_cells`) reach, plus its own cell; explored |= visible; statics standing in visible cells are recorded
  as seen now, and records whose cell is seen empty are dropped. A record made for another grid size starts over.
  All four are snapshot-only (`reveals_fog`, `explored_cells`, `visible_cells`, `seen_statics`, ADR 0013): the
  explored ground survives save/resume, and no state, content hash, replay or golden changes.
- **`FogStatics`** (`PerceptionAppExt::set_fog_statics`, not hashed): content says which `Perceivable` kind bits are
  static, the kind for `GridOccluder` entities without `Perceivable` (wall runs), and `fn(EntityRef) -> Option<bool>`
  readers for "open" and "felled". Default: nothing recorded.
- **Testbed**: `world::fog_statics()` (doors with their open state, trees felled or standing, ore, stations, items,
  chests; walls as the new `kinds::WALL`, `kinds::FOG_STATICS`), registered by `TestbedPlugin`. **Explorer worlds**:
  the actor gets `RevealsFog` in the host's `apply_senses` (New, Fresh, Resume, reset); the shared scenarios are
  unchanged.
- **Fog cost (review fix round)**: the fog update is its own exclusive system, `fog_system` in the new
  `NpcFogSet` (`NpcSet::Sense`, after `NpcPerceptionSet`), with its own perf mechanic `fog` (tag
  `npc_perception::fog`) and a budget row `fog` 55 + 0 µs (approved by the dev), so `perception`
  keeps its budget. Fog is per cell: seen from the centre of the actor's cell, with lines of sight traced once per
  range into a table and the scan area's occluders read once (`SpatialGrid::for_each_occluding`, new); a scan
  reuses the visible cells while the actor's cell, facing, sensors and the grid's cells (`SpatialGrid::revision`,
  new, not state) are unchanged, and touches only statics that can be in view, allocating nothing once warm.
  Release explorer world, perception p95: windowed 132 µs before, 38–46 µs after (fog p95 16 µs idle, 38 µs
  walking); headless idle 62 → 6.5 µs (fog 2.1), walking 79 → 8.1 µs (fog 13.6).
- `ExploredCells` records the grid's `min` too (`serde` default: older records still load) and a record that does
  not fit the loaded grid starts over at the load: a `warn!`, `FogResets` and, in explorer worlds, a line in the
  load report's `skipped`. `VisibleCells::fits`; `is_visible` is false for bits of another size. `SeenState` keeps
  a wall run's `footprint`, so a wall seen gone at any of its cells is dropped.

### Added (reach sense; vehicles and units are remembered, spec F3, plan Task 27)

- **npc_perception**: opt-in `ReachSense { radius_mm }` (next to `Sensors`): every `Perceivable` thing within the
  radius, whatever the facing, with the same grid line of sight as vision (never through a wall), on the agent's scan
  ticks into `PerceptionMemory` as `SenseKind::Reach` (`"reach"`, hash tag 3; vision, then proximity win). It is
  registered snapshot-only (`reach_sense`, ADR 0013): no state- or content-hash entry, so worlds without it keep every
  hash. `perception_json` shows it. **per_roll** rolls reach with the proximity modifier (dice index sense 3).
- **Perception text** (`perception_agent`): a `Within reach: chest #54 (behind you), forge #15.` line for units with
  a reach sense (sensed things within reach, nearest first; `behind you` / `beside you` for one only reach senses),
  JSON `reach_mm` and `within_reach`.
- **Explorer worlds**: the actor gets `ReachSense { radius_mm: 1500 }` (`EXPLORER_REACH_MM`; also on resume of an
  older save and on reset), and chests become `Perceivable` (new kind bit `kinds::CONTAINER`), so a chest in reach is
  sensed and remembered (proposals 007, 011); in explorer worlds chests are perceivable to every NPC, not only the
  agent's actor, and a chest spawned at runtime stays imperceptible until the next resume or reset. AGENTS.md's senses
  list "reach: 1.5 m all around, through nothing solid". A missing explorer actor is now an error, not a world
  without reach.

### Fixed (proposal 009)

- **"You remember"** lists the 24 most recently seen things (`REMEMBER_TEXT_LIMIT`) plus every vehicle and unit
  (a `Brain`, `Commandable` or `Vehicle`, whatever its age), then `; and N more (40 tree, 12 item)` by kind name
  (count descending, then name); the JSON `remembered` stays complete. It used to stop after the eight lowest
  `SimId`s ("and N more"), so vehicles and units, spawned after the world and so with the highest ids, were hidden
  whenever more was remembered. Memory itself never dropped them by kind; the one-minute rule still applies.
- **"You perceive"** follows the same rule: the 8 nearest (`PERCEIVE_TEXT_LIMIT`, was the 8 lowest ids) plus every
  vehicle and unit, then the per-kind summary.

### Added (the menu runs detached worlds, spec G7, plan Task 8)

- **game_menu**: New has **Start in window** and **Start detached**; each Resume row starts in the window and has
  **Start detached** (both after the save check; the mismatch dialog keeps the choice). Detached runs `npcctl serve
  --explorer-home .. --agent ..` (its own console and process group: it outlives the menu) and shows the new
  **Detached** screen: "<agent> in <world> running detached on port N, tick T" (`session/info` every 2 s on a
  background thread), the URL, the prompt without its frontmatter with **Copy** (the shared clipboard module), **Open
  in window**, **Stop** and Back. Stop and Open in window run on a worker thread: `session/stop` (the world saves
  first), then the exit is waited for up to 15 s. Stop returns to Resume ("<agent> stopped at tick T and was
  saved."). Open in window aborts on a refused stop (a failed save) or a process still running, and the world keeps
  running detached; a process that exited non-zero or wrote no newer save is restarted detached from its last save
  on the same port; else `actor_testbed --resume --exact-port N` opens it, and a window that exits 4 or 2 restarts it
  detached on `N` with the reason. The Resume list adopts live `run.json` worlds ("running detached on port N - Open /
  Stop"; one running in a window is disabled). `--detached AGENT` opens an agent's Detached screen. The menu never
  kills a detached world; it only stops it over `session/stop`.
- **actor_testbed** (explorer mode): closing the window asks **"Keep it running detached?"** (`close_when_requested`
  is off there): Yes saves, starts `npcctl serve --explorer-home .. --agent .. --resume --port N --wait-port` on the
  window's port (detached from the window), prints `{"detached":{"pid","port","agent","world"}}` for the menu (which
  then shows the Detached screen) and exits; No saves and exits; Escape / Cancel goes back. The game's input does not
  run while it is open and no text field keeps the focus (Y / N / Escape answer it). **F9** moves the world to
  detached without asking. `--close-dialog` opens the dialog at start (visual checks).
- **`npcctl serve --wait-port`** (with `--resume --port N`): a handover from the closing window: waits up to 10 s, in
  one budget, for `N` to come free and for the window (its `run.json` pid) to exit; never another port (exit **4**,
  "port N is still in use"). The window spawns the detached host before it exits, so the host must wait: this flag is
  that wait (the window cannot drop its BRP server and still spawn after `App::run`).
- `testbed_content::explorer::host`: the clock-free handover waits `wait_for_port_until` / `wait_for_handover_until`
  (the caller passes "is the budget spent?": actor_testbed and npcctl own the clock, this crate reads none),
  `Handover`, `running_elsewhere` and `HANDOVER_WAIT` (moved from actor_testbed), plus `detached_resume_args`,
  `detach_command`, `session_call` (the menu's JSON-RPC to a detached world), `DetachedLine`, `latest_prompt`,
  `prompt_text`, `pid_dead` (a confident "gone" only) and `RunFile::read_any`.
- Review fixes: the Resume screen asks the OS about `run.json` pids on a thread ("checking whether it runs..." until
  it answers) and deletes a `run.json` whose pid is clearly dead; a Stop or Open in window that aborts from a Resume
  row shows its reason on the world's Detached screen; Quit (and the menu window's close button) waits while a Stop
  or Open in window is in flight; Stop of a found (adopted) world counts as clean only with a newer save; a failed
  move to detached puts the window's earlier pause state back; the "moved to detached" line never panics on a closed
  stdout; the prompt modal is frozen while the close dialog is open.
- Known limits: a detached world the menu started keeps its stdout/stderr piped to the menu (no log file yet), so
  after the menu quits its later log lines are lost and its final stdout line may fail (it has saved by then). A
  window-to-detached handover whose `npcctl serve --wait-port` cannot take the port (still taken after 10 s) fails
  silently: the window has exited and saved, nothing runs, and the save is intact (Resume it).

### Added (detached explorer worlds, spec G7, plan Task 7)

- **`npcctl serve --explorer-home DIR --agent NAME [--world W | --resume [--load-anyway | --fresh-world]]
  [--preset human] [--port N] [--paused]`** hosts an explorer world without a window (npcctl never renders):
  built or resumed by `testbed_content::explorer::host`, served with the session's agent methods. It prints
  `{"listening","port","agent","world","prompt"}`, writes the agent folder's workspace for its URL and
  `run.json` (mode `detached`), autosaves every 60 s of wall time and saves on `session stop` or Ctrl+C (the new
  `ctrlc` dependency of npcctl), then removes `run.json` and exits 0. A failed save on `session stop` refuses the
  stop (the world keeps running). Without `--port` it takes the first port in 15702..=15721 whose next port is
  free too; a second host for a running agent is refused. `world/reset` rebuilds the explorer world with its
  notes resources (`host::reset_world`); `world/load_scenario` is refused there.
- **`npc_agent_io::session`**: `Session::start_app(app, options)` / `start_app_on(app, server, options)` host a
  world the host built (no `--record`); `SessionOptions` gains `on_frame` (after every frame and step slice),
  `on_stop` (a final save; an `Err` refuses `session/stop`), `rebuild` (`world/reset` of such a session) and `stop`
  (an `AtomicBool` a Ctrl+C handler sets). `SessionOptions` is no longer `Clone` (it holds the hooks).
- **`actor_testbed --explorer-home .. --resume --exact-port N`**: moves a stopped detached world into the window
  on exactly port `N`, retried for up to 10 s, never another port (exit **4**, "port N is still in use"); no prompt
  modal on this path, the sim runs and a 5 s notice says "moved here from detached (port N)". Every explorer window
  writes `run.json` (mode `window`) and removes it on exit. `pick_port` / `PORT_SPAN` moved to
  `testbed_content::explorer::host` (re-exported by actor_testbed).
- Review fixes: once a stop is accepted, the session runs nothing more: calls queued behind `session/stop`,
  parked long-polls and steps are answered "the session is stopping" (`session::STOPPING`), so the final save is
  the world's last state; `on_stop` extras never replace `stopped` / `record`; an app finished without a driver
  is refused with a message. The window refuses an agent whose `run.json` names another live process (exit 2,
  "agent X is already running (pid P, mode M, port N)"); `--exact-port` waits within its 10 s for the detached
  host to exit first. A detached `world/reset` keeps the old save as `<world>.world.<date>.json`. `npcctl serve`
  exits 3 for a save from another build, as `actor_testbed` does. ctrlc's `termination` feature is on (SIGTERM /
  SIGHUP on Unix; on Windows closing the console window is not a graceful stop and can lose up to the last 60 s
  autosave interval: use `session stop` or Ctrl+C). A reset whose save fails and whose backup cannot be put back
  says so and names the backup.

### Fixed (item and crafting feedback, agent proposals 002, 005, 006, 008)

- **A tool yield says where it went** (proposal 005): `use_tool_on` yields are always placed at the actor's feet (a
  sim rule, unchanged; they never went into the pack). `JournalKind::Interacted` gains `dropped: [{item, kind,
  stack, pos: [x, z]}]` (`npc_kernel::journal::DroppedItem`; left out of the JSON when empty) for a success that put
  new items on the ground, and `reason` then says why the pack could not take them (`"pack would carry 30950 of
  30000 g"`, `npc_interaction::items::PackFit::refusal`). Event lines and the `changes` text read "use_tool_on #42
  success, 2 × log dropped at (24.08 m, 1.10 m): pack would carry 30950 of 30000 g" (`observe::dropped_text`). The
  `use_tool_on` offer shows where the yield lands and the load after picking it up: "yields 2 × log at your feet
  (pack 24950 -> 30950 of 30000 g, too heavy to pick up)" (or ", no free slot to pick up";
  `glue::interaction_agent::yield_load`). Journal and observation only: computed while the journal records.
- **A craft names every input and tool** (proposal 006): `npc_crafting::can_craft` refuses a craft with missing
  inputs or tools with the new `CraftError::Missing { needs: [CraftNeed {key, tool, need, have}] }`: every recipe
  input and tool with what the crafter has, the missing ones first ("needs 1 plank (has 0), 1 ingot (has 2), tool
  hammer (has 1)"; `tool:` prefixes are dropped in text, `npc_crafting::tool_name`). It used to name only the first
  gap (`MissingTool` / `MissingInput`, still used by the completion check). The plan of a craft that can start is
  unchanged. The `craft:<recipe>` label says what it consumes and uses ("Craft forge_axe at workstation #14:
  consumes 1 ingot, 1 plank; uses hammer").
- **Craft odds, and defaulted skills on the sheet** (proposal 002): a craft line carries its roll's odds like a tool
  use (`check`, and "[Carpentry 7 (default DX-5) vs 3d6: ~16%, low odds]": the effective skill plus the recipe's
  modifier and the planned tools' quality; `glue::interaction_agent::odds::skill_check`). With `progression`, the
  `sheet` section lists a skill the unit has rolled without buying it at its default, in the new `used_defaults`
  (`[{"name","level","default"}]`) and on the Skills line ("Carpentry 7 (default DX-5)").
- **A full pack says what to drop** (proposal 008): a refusal for the actor's full inventory (`pick_up`,
  `unequip`, and `equip` into a taken slot) names the lightest items that free a slot, by weight then `SimId`, at
  most three: "inventory is full (8 slots): drop 1 of [plank #66, ingot #89] to make room", "main_hand holds #17
  (unequip it first; inventory is full (8 slots): drop 1 of [..] to make room)"
  (`npc_interaction::items::with_room_hint`), in `why_not`, the action rejection, the `Interact` command refusal
  and the journal. The verbs' own reasons stay the bare rule, because an `Interact` order keeps its failure reason
  in hashed state. "9 of 8 slots" was not a counting bug: equipped items take no slot, and the agent counted the
  main-hand item from "You carry". The load line now says so when something is equipped ("8 of 8 slots used;
  equipped items (pickaxe #17 in main_hand) take no slot, but unequipping one needs a free slot"), and the
  `inventory` section has `"equipped_use_slots": false`.

No hashed state, recorded command, replay or golden changes. `JournalKind::Interacted` has a new field, so code
that builds it needs `dropped: Vec::new()`.

### Fixed (movement and vehicle feedback, agent proposals 001, 003, 010)

- **`drive_to` from the driver seat of a `Direct` vehicle** (proposal 010): the testbed's wagon is `Direct` (a unit of
  its own), and `steered_vehicle` accepted only `ByCrew` driver seats (`npc_vehicles::driven_vehicle`), so a unit
  shown "Mounted {role: Driver, seat: 0}" was refused "riding #30 but not in its driver seat". A unit in the driver
  seat of a `Direct` vehicle now drives it (the recorded `DriveTo`, in `Advisor` and `Direct` mode; busy "driving
  vehicle #30 to (x, z)" while it moves) unless the vehicle is on autopilot (`glue::vehicles_agent::autopilot`:
  another controller, its own behaviour tree, or with `command` its orders drive it); then `drive_to` says so and how
  to take control ("wagon #30 is on autopilot: its own behaviour drives it; attach a controller to #30 in Direct mode
  to take control"). The `vehicle` section reads "self-propelled, steered from its driver seat" or "on autopilot"
  instead of "moving on its own". Passengers keep "riding #N but not in its driver seat". The driver seat of a
  `Direct` vehicle now also offers `unload@<vehicle>` (the `unload` handler uses the same `steered_vehicle`), so
  that unit can get everyone off. The `drive_to` label and the busy reasons speak metres: "Drive to (4.50 m,
  0.50 m)", "driving vehicle #4 to (4.50 m, 0.50 m)", "driving to (..)", "blocked on the way to (..)".
- **A blocked walk says how far it got and why** (proposal 001): `JournalKind::OrderFailed` gains the optional
  `nearest_reachable: [x, z]` (mm) and `blocker` (`npc_kernel::spatial::Blocker`: `{"kind":"Wall","cell":[x,z],
  "pos":[x,z]}`, `{"kind":"Door","id":12,"locked":true}`, `{"kind":"OutOfBounds"}`), filled by `npc_command` for
  `reason: "blocked"` from the new read-only `npc_nav::explain_blocked` (its own flood fill, no nav budget or queue
  state; computed only while the journal records). It floods with the planner's rules first (a door the agent may
  open is passable), so behind an unlocked outer door and a locked inner one it names the inner door and a point in
  the hallway; only when the goal is reachable under those rules does a strict pass (closed doors block) name the
  door the walk stalled at. Its work is capped at `npc_nav::EXPLAIN_CELL_BUDGET` (2^18) flooded cells
  (`explain_blocked_within` takes another budget), and at most `npc_command::BLOCKED_REPORTS_PER_TICK` (8) reports
  are built per tick (`BlockedReports`, not hashed; units in `SimId` order); past either cap a failure carries only
  its `reason`. Doors are named through the new `NavAppExt::register_cell_blocker`
  (`npc_interaction::door::door_blocker`). Event lines and the `changes` text read "Move failed: blocked by locked
  door #12; nearest reachable (4.50 m, 3.50 m)"; `changes.orders[]` carries the two fields. Both are left out of the
  JSON when `None`, so other events are unchanged; `JournalKind::order_failed(order, reason)` builds one without them.
- **Move orders speak metres** (proposal 003): `describe_order` labels, `order` results, the `command` section and
  the busy reason say "move to (0.00 m, -0.03 m)" (`observe::metres_pos`). A `pos` (or `points` entry) that is not
  integer millimetres is refused with "positions are integer millimetres: 6.5 m is 6500" (`api::POS_HINT`): the
  act's own `pos` and a step's `pos` as invalid params, one in `args` as `invalid_args`.

### Changed (notes as Obsidian files)

- **One markdown file per notes entry** (spec `2026-10-25-notes-as-obsidian-files-design.md`): `NotesStore` keeps
  the diary, memory and proposals as `diary/YYYY-MM-DD <title>.md`, `memory/<title>.md` and `proposals/<title>.md`
  in the agent folder, each with a flat YAML frontmatter (`id`, `type`, `title`, `aliases`, `tags: [npc/<type>]`,
  `agent`, `world`, `author` (last writer), `created`, `created_tick`, `updated`, `updated_tick`, `revision`;
  proposals add `status` and `prompted_by`), so the folder opens as an Obsidian vault. File names are made safe
  (`notes::safe_name`: `* " \ / < > : | ? # ^ [ ]` dropped, spaces collapsed, ends trimmed, Windows reserved names
  (`CON`, `NUL`, `COM0`-`COM9`, `COM¹`-`COM³`, `LPT` alike, `CONIN$`, `CONOUT$`) get `_`, 120 characters, a trailing
  `.md` of the title dropped, `Memory 12` when nothing is left), a taken name gets ` (2)`, ` (3)`; a title change
  renames the file (temp file, rename, then the old file removed) and adds the old title to `aliases`. Entries are
  found by `id`, so a file renamed or edited by hand (in Obsidian) is the same entry; subfolders and non-`.md` files
  are ignored; unknown properties and untouched lines survive an update byte for byte. A file with no or broken
  frontmatter, or a second file with the same `id` (the newest wins: `updated` when both are UTC timestamps, then
  `updated_tick`, `revision`, the modification time), is listed in the new `NotesStore::unreadable` / `notes::Unreadable` and never written; the notes `list` results (methods, MCP
  `notes_list`, `npcctl`) gain the additive `unreadable: [{file, reason}]`. A hand-made file with frontmatter but no
  `id` is adopted (the next id, written back once). New ids are above a high-water mark of every id seen in the
  folder (entries, earlier reads, the `id:` lines of unreadable files), so no id is handed out twice. The
  frontmatter reader/writer is hand-written (`notes::frontmatter`; no YAML crate). The API is unchanged: the `updated` stamp keeps its
  `{tick, date}` shape with `#rN` in `date`, rebuilt from `updated_tick`, `updated` and `revision`.
- **Old folders migrate once**: a `diary.md` / `memory.md` without a `.bak` becomes one file per entry (ids, titles,
  stamps and text kept; text before the first entry becomes an entry `Kept from diary.md`) and is renamed
  `diary.md.bak` / `memory.md.bak`; entries whose id already has a file are skipped, so a rerun is safe. A header-block
  proposal (no frontmatter, but a `title:`, `status:` or `written:` header line or an `NNN-` name; any other file
  without frontmatter is unreadable) is rewritten under its readable name (unknown header lines become properties) and the old
  file removed. Old free-form dates are kept verbatim, quoted.
- **Owners**: `notes::NotesOwner {agent, world}` fills the `agent` / `world` properties; `NotesStore::open_as(dir,
  owner)` and `NotesStores::set_owner(dir, owner)` (the testbed's `insert_notes` sets it from the agent's
  `agent.json` / session), so the generic store never reads content files.
- **Testbed**: new agent folders get empty `diary/`, `memory/` and `proposals/` (no `diary.md`); `AgentDir::diary()`
  is the folder, `AgentDir::memory()` is new. `AGENTS.md` and `prompts/*.md` start with a frontmatter (`type:
  agent-guide` / `prompt`, `agent`, `world`, `generated: true`; no date, so an unchanged folder regenerates byte for
  byte; `workspace::{generated_frontmatter, agents_md_file, prompt_file}`); `CLAUDE.md` stays exactly `@AGENTS.md`
  and the prompt text the modal shows and copies has no frontmatter. The F7 editor lists the unreadable files of the
  shown book with their reason.

### Changed (explorer memory, spec M1)

- **Explorer actors remember for a minute**: explorer worlds set `PerceptionConfig` to `forget_after_ticks = EXPLORER_FORGET_AFTER` (1200, 60 s) and `memory_capacity = EXPLORER_MEMORY_CAPACITY` (128) on every start path, old saves included (the host applies them after loading). Other scenarios, the village and the framework defaults are unchanged; the `perception_config` hash of explorer worlds changes (no replay or golden covers them).
### Changed (explorer prompts: an Epoch 1 mission)

- **The AI explorer's prompts are a game master's mission brief** (`content/testbed_content/prompts/`): Epoch 1,
  survive, learn, explore and map the world, find out how it works and keep what you learn in your notebooks.
  `AGENTS.md` shrinks from a manual to a character sheet: mission, body, how the agent reaches the world, its three
  notebooks (memory is its map and its manual), the rules, and `npcctl agent --help` for tools without MCP. It no
  longer teaches the one-call loop, tool fields, coordinates or the walls and openings lines; the MCP tools'
  descriptions are the agent's reference.

### Added (prepared agent folder, spec G5)

- **`testbed_content::explorer::workspace`**: `write_workspace(dir, kind, vars)` writes the agent folder's generated
  files atomically (`GENERATED`: `AGENTS.md` from the new `prompts/agents.md`, `CLAUDE.md` = `@AGENTS.md`,
  `prompts/<kind>.md`, `.claude/settings.json` (MCP-only: allow `mcp__npc__*` and `Read(./**)`, deny shell, edits
  and web, `dontAsk`, no reads outside the folder), `.claude/settings.local.json` (enables `npc`) and `.mcp.json`
  (`npcctl mcp --url URL --controller explorer`, the Windows path JSON-escaped)); it never touches the notes,
  `agent.json`, saves or `run.json`, and deletes the old `start_prompt.md` / `resume_prompt.md`. `WorkspaceVars`,
  `ActorSummary` (`human_scout()` until actor presets exist). `AGENTS.md` teaches the one-call loop (`act_and_wait`,
  `observe` with `since_seq`), the notes tools and, for tools without MCP, the `npcctl` lines. The start/resume
  prompts shrink to who you are, read `AGENTS.md`, (resume) list memory, diary and proposals with `notes_list`,
  observe; they no longer tell the agent to write files. `AgentDir::prompt(kind)` (`prompts/<kind>.md`) replaces
  `start_prompt()` / `resume_prompt()`; actor_testbed writes its prompt there (the whole workspace: Task B2).
  Template filling is now one pass, so a name that looks like a placeholder stays as typed.
- **The explorer writes the prepared folder on every start**: actor_testbed's explorer calls `write_workspace` (actor
  `ActorSummary::human_scout()`) on every New, Resume, Load anyway and Fresh world, so an older folder gets its
  `AGENTS.md`, `CLAUDE.md`, `prompts/`, `.claude/` settings and `.mcp.json` on its next resume and a changed port is
  rewritten everywhere; the notes, `agent.json` and the saves stay untouched. Its own `write_prompts` / `prompt_path`
  are gone; the modal's file is `prompts/<kind>.md`. A folder that cannot be written no longer stops the start: it
  is logged and named in the modal's notes (`ExplorerPrompt::write_error`). The menu's New screen names the folder
  ("After Start, open your AI tool in <agents folder>/<slug(agent)> and paste the prompt.", following the typed name;
  `launch::agent_folder_line`).

### Added (fewer round trips for agents, spec G13)

- **`agent/act_and_wait`** (BRP and `npcctl serve`): every `agent/act` field plus `until_idle` (default true),
  `kinds`, `timeout_ms` (default 30000, clamped to 120000) and `observe: {format, brief}`; acts at once, waits until
  the unit is idle (or, with `kinds`, the first such event; without `until_idle`, the first matching event) or the
  timeout, and returns `{act, events, observation}` (`events`: the `agent/events` result for the seqs after the act,
  `timed_out` at the timeout; `observation`: `agent/observe` with `changes` since the act). A rejected act answers at
  once. A parked poll in the session and a watching method on BRP, where it **needs a `nonce`** (invalid params
  without one): state is keyed by the params, nonce included, and every finished call (also a rejected act or
  `timeout_ms: 0`) keeps its answer as a tombstone for 60 s after its last run, since `bevy_remote` re-runs an answered
  watcher until the HTTP side drops it; re-runs get that answer and never act twice. It never blocks a frame. API:
  `api::act_and_wait_start`, `act_wait_ready`, `act_wait_result`, `ActWait`. Host-side: the same commands as
  `agent/act`, so the state hash is the same.
- **`agent/observe {since_seq}`**: adds `changes` (`sensed_new`, `lost`, `inventory_gained`, `inventory_lost`,
  `orders`, `interactions`, `actions`, `truncated` when the ring buffer already dropped events after `since_seq`),
  built from the agent event buffer only (`observe::changes`), and in text a first line "Since your last look: .."
  or "Nothing changed since your last look." (`observe::since_line`). A `since_seq` beyond the latest seq is invalid
  params (the world was rebuilt), as in `agent/events`.
- CLI: `npcctl agent act .. --wait [--kinds K,..] [--timeout-ms N]` (`--format text`: an act line, a wait line and
  the observation text; a rejection exits 1) and `npcctl agent observe .. --since-seq S`. MCP: tool `act_and_wait`
  (every `act` field plus the wait fields; text form like the CLI's, `observe.format` default text) and
  `observe.since_seq`. The HTTP client and server wait `act_and_wait`'s default/cap plus the usual grace. The CLI's
  `nonce` is now unique per call (process id plus wall clock), not per process.

### Added (agent notes)

- **`npc_agent_io::notes`**: a host-side `NotesStore` per agent folder with three books, `diary.md`, `memory.md` and
  `proposals/NNN-<slug>.md` (`list` / `add` / `update`; ids per book; title <= 200 chars, body <= 8192 bytes). Updates
  carry the `updated` stamp the writer read and are rejected as stale ("changed since you read it") when it moved
  (`updated` is a version: a same-tick write or a hand edit gets a `#rN` revision on the date);
  only `Writer::Player` sets a proposal's status. Old explorer folders (`## tick T - title` headings, proposals
  without `id:`) get ids on first read, written back once; text before the first heading, unknown header lines,
  BOM and CRLF are tolerated and untouched entries are written back byte for byte. Files changed on disk are re-read
  before each operation; writes are atomic. Not sim state: nothing is hashed or snapshotted.
- **Notes methods** (BRP and `npcctl serve`): `agent/notes/list|add|update` for the unit's controller (the `agent/act`
  check, now `api::check_controller`: `not_attached`, `not_controller`, `read_only`; `list` is a read and works for an
  Observer) and `notes/list|add|update` for
  the player (no controller; only they set a proposal's `status`). The host maps units to agent folders in
  `NotesDirs` and dates stamps with `NotesClock`; one `NotesStores` resource holds a store per folder. A stale update
  is the rejection `reason: "stale"` with `current`. CLI `npcctl agent notes ..` and `npcctl notes ..`; MCP tools
  `notes_list`, `notes_add`, `notes_update` (`npcctl mcp --controller`, default `explorer`), and the MCP schemas
  now list `act.steps`, `observe.brief` and `events.until_idle|min_events|dedupe`. `mcp::serve`, `handle` and
  `call_tool` take the default controller. MCP tool errors keep the method's `data.reason` (text and
  `structuredContent.error.reason`), and the `act` tool says how to walk to a thing and use it (`interact`).

- **F7 notes editor** (`testbed_view`, explorer mode): the F7 panel edits the explorer's notes through the same
  `NotesStores` as the notes methods, as `Writer::Player`: tabs Diary, Memory, Proposals (Tab cycles), a scrollable
  list newest first with ids, created/updated stamps, bodies and statuses; **Add** / **Edit** open a title and a
  multi-line body `TextInput`, **Save** passes the `updated` stamp read when editing began. A stale save writes
  nothing: the form keeps the typed text, shows the entry's current title and body above the fields ("Now in the
  file:") and says "#N was changed meanwhile - its current text is shown above; Save again to replace it with
  yours, or Cancel"; only that second Save may replace it (stale again if it changed once more). Status buttons
  per proposal ("changed meanwhile - reloaded" when stale); the player's own status click on the proposal open in
  the form keeps the form current. Re-read every 1 s.
  While a panel field has focus `ViewSystems::Input` is gated (`editor_typing`), Tab moves between the fields, and
  copy/cut/paste use the OS clipboard; clicks and the wheel over the panel do not reach the world. `actor_testbed`'s
  explorer maps its actor to the agent folder (`NotesDirs`) and dates notes with the wall clock (`NotesClock`).
  `testbed_content::explorer::notes` (the old diary/proposal parsers) is gone: `read_lossy` moved to `home`, and
  `clipboard::paste_text` is shared by the menu and the editor.

### Fixed (profiler numbers and sim tick cost; `docs/_notes/explorer-perf-budgets.md`)

- **`NpcSimTick` runs on Bevy's `SingleThreadedExecutor`** (set by the kernel): with `bevy_ecs/multi_threaded` (every
  windowed game) the default executor cost ~0.45 ms per tick more than the work of its small, mostly exclusive
  systems and made each one measure ~3x slower. Explorer tick 0.59 → 0.12 ms, village_n 500 0.87 → 0.45 ms
  (release, profiler on). System order is unchanged (ambiguities are errors): no state hash changes.
- **Per-tick `world.query()` rebuilt a `QueryState` in 14 exclusive systems** (perception, think, nav plan, command
  engine, the four vehicle systems, interaction completions and doors, crafting completions, the `ExternalAction`
  passes): ~1.5 µs each, plus one `par_for_each` tracing span per build with `bevy/trace`. They take cached
  `&mut QueryState` / `Local` params now. Perception gathers the perceivables only on a tick where an agent scans
  or a noise was heard.
- **`npc_perf`**: a `mechanic` span inside a timed span of the same mechanic is no longer counted twice (vehicles,
  perception, interaction, crafting and progression were doubled). New `BreachLog` (`Warn`, `WarnOnce`, `Debug`) and
  `set_breach_interval`; `NpcPerfPlugin` defaults to `WarnOnce` and `perf_log_filter` adds `npc_perf=info`, so
  normal play logs one line per slow mechanic instead of one every 5 s (`actor_testbed --perf-warn` restores it).
  Bevy's `Main::run_main` is tagged `frame` (inclusive, no budget) instead of `engine`.
- **`actor_testbed` profiled none of its sim systems**: they were added before `LogPlugin` set the subscriber, so
  their spans stayed disabled and F4 showed only the hand-written `mechanic` spans. New
  `perf::install_log_plugin_now()` installs the subscriber first (`DefaultPlugins.disable::<LogPlugin>()`).
- **The testbed's sim systems were `untagged`**: new `testbed_content::perf_tags` (`walk_cancel_system`,
  `walk_arrival_system` → `actions.move`; `idle_system` → `actions.idle`, like the village's), added by
  `actor_testbed` and `npcctl bench`; `TestbedPlugin` registers the `actions.idle` budget (10 + 2, the village's row:
  `TESTBED_BUDGETS`). No sim change.
- **World snapshots lost `GridBlocker` / `GridOccluder` without spatial hashing**: they were saved only when a
  capability called `register_spatial_hash`, so a host with `SpatialPlugin` alone lost them on load and the
  post-load re-sync cleared their cells. `SpatialPlugin` now always saves them (`grid_blocker`, `grid_occluder`).
  No content-hash or replay change (ADR 0013 addendum).
- **A driving unit counted as idle for agents**: nothing registered a vehicles busy predicate, so `agent/events
  {until_idle}` and `agent/act_and_wait` answered while an `Advisor` driver's vehicle (the `DriveTo` command) or a
  `Direct` vehicle was still on its way. New busy predicate `npc_vehicles` (`glue::vehicles_agent::vehicles_busy`):
  busy while `DriveVehicle` runs, or while a `Direct` vehicle's own intent is `Moving` / `Blocked`. Agent tooling
  only: no sim or hash change.
- **The agent HTTP server could lose its refusals** (`npc_agent_io::http`, the flaky `caps_concurrent_connections`):
  a refused connection was closed without reading the client's request, and on Windows a close with unread input,
  or a request arriving after the close, resets the connection, which can destroy the `503`/`4xx` before the client
  reads it (16 of 330 loaded runs got status 0 and no body; 0 of 325 after the fix). A refusal now reads the
  connection until the client closes it (at most 1 s, `REFUSE_LINGER`), and the `503`s are answered by their own
  thread (a queue of 64, dropped beyond) so the listener never waits on a client. The test also counted 200 retries
  (~4 s) for a free slot; it now watches the server's slot count with a 60 s clock deadline and a 1 h request
  deadline, so a freed slot can only come from a close (slots return 6–470 ms after close under load). Private test
  seam `HttpServer::bind_counted`; no public API change.
- **Docs**: per-component and per-system reference pages (`docs/entities/components/`, `docs/entities/systems/`);
  `disable::<StraightLineMover>()` is shown on `NpcPlugins::new().build()`; README F7 (notes editor) and profiler
  log-plugin text updated.
- **Schedule warning**: `flush_per_roll_log` was put in `NpcSet::Sense` and `NpcPerceptionSet` (already in
  `Sense`); the redundant edge warning at start-up is gone.
- **Budgets** (`NPC_BUDGET_TABLE`, fixed µs + µs per agent): `vehicles` 10 → 30 + 2, `perception` 20 → 50 + 5, `nav`
  20 → 30 + 2, `interaction` 15 → 25 + 2, from the windowed explorer p95 (~1.5× headroom); `bench/baseline.json`
  unchanged.

### Changed (explorer and snapshot polish)

- **`testbed_content::explorer::clipboard`**: one std-only module for the OS clipboard (`copy`, `paste`), with a
  5 s timeout per tool (a hung tool is killed) and `wl-paste --type text/plain`; the modal and the menu switch to it
  next. `home::name_problem` explains an unusable agent name (`create_agent` refuses one), `backup_save` never
  overwrites a backup (`-2`, `-3`, ..). Prompt: the "npcctl not built" note has its own line, the events step keeps
  `next_seq`, `--kinds` lists the craft events, and every path uses forward slashes.
- **Snapshots**: `write_atomic` syncs the temp file before the rename, `snapshot_check` prints the entity diff for a
  saved-vs-never-saved divergence too, a test pins the load order the `brain` and `spatial_grid` loaders rely on, and
  a test pins that `Reservations` round-trips hash-equal. No state hash changes.
- **In-game explorer**: the prompt modal copies through the shared `clipboard::copy` (its own clipboard code is
  gone); a copy that has not reported after 7 s, or whose thread panicked, shows the failure note and Copy works
  again; a "scroll for more - Copy takes all of it" hint shows while the prompt is taller than its scroll area.
  The F7 panel reads the files in the frame it opens, and a file it saw but could not read (a sharing violation
  while the agent writes) keeps the old text and is retried at the next poll instead of being cached
  (`notes::try_read_lossy`, `try_read_proposals`). `actor_testbed` explorer mode: `pick_port` needs `p + 1` free
  too (the render server), `--remote-port N` is the first port tried (`N..=N+19`), a port taken before BRP could
  bind it exits 2 ("busy: try again") instead of advertising a dead URL, `--world` with `--resume` is a usage
  error, and a failed tick-0 save removes the new agent folder so the name can be used again.
- **Game menu**: Ctrl+C / Ctrl+X / Ctrl+V in a text field use the shared `clipboard::copy` / `paste` off the
  frame (its own clipboard code is gone; Cut still removes the selection). A game that exits non-zero shows
  "<game> exited with code N: <its last stderr line>" (stderr is piped through a thread that still copies every
  line to the menu's terminal). `--check-save` exiting neither 0 nor 1 reports "the save check failed (exit N):
  <line>" on the Resume screen instead of the mismatch dialog; a check is killed after 30 s ("the save check
  timed out") or when the Checking screen is left, and an in-flight check blocks a second launch like a running
  game. The Building screen has **Cancel** (kills cargo, back to Missing); closing the menu kills a build too.
  Settings' Save creates no folder when there is no settings file to write; New checks both names with
  `home::name_problem` (inline reason). The menu renders reactively (low-power winit modes: a frame on window
  events, 2 per second while a text field has the focus for the caret, polling only while a child, a clipboard
  tool or `--screenshot` needs it) instead of continuously. **Build** falls back to the workspace the menu was
  compiled in when none is above its folder (`CARGO_TARGET_DIR` outside the workspace).

### Added (explorer senses walls)

- **`walls` observation section** (`testbed_content::walls`, registered by `TestbedAgentPlugin`): the wall runs
  (`Prop { Wall }` footprints and the `SpatialGrid` static layer's blocking cells) and openings (a gap of at most
  4 cells between two runs on one line; a `Prop { Doorway }` in it makes it a `door` with its state and testbed
  name, e.g. `gate`) within the unit's vision range, sorted by distance, with compass directions; no field of
  view or line of sight. Text: "Walls within 15.00 m: .." and "Openings within 15.00 m: the gate (closed door
  #10) .., 3.04 m south". Integer geometry, read-only (no state or content hash change). The explorer prompt
  (`prompts/common.md`) now says walls and openings are sensed (proposal 003, finding P3 of the explorer E2E run).

### Added (game menu and AI explorer)

- **`apps/game_menu`**: `cargo run -p game_menu` opens a menu (Explore with an AI agent, Actor Testbed with one
  button per embedded scenario, Village, RTS units demo, Settings, Quit) that starts the chosen game as a child
  process with its flags, hides its window while the game runs (polling `try_wait`, low-power updates) and comes
  back when it exits, showing a non-zero exit code on the main screen. A game not built next to the menu shows its
  `cargo build -p <name>` command and a **Build** button (when `cargo` is on `PATH` and the workspace is found)
  with a "building..." state; Launch and Build ignore a second activation while a child runs. Settings edits the
  agents folder in a Bevy `TextInput` (validated inline, created on Save, saved to the per-user settings file or
  `--settings FILE`); the explorer entry is disabled until the folder is valid. Ctrl+V / Cmd+V in every menu text
  field pastes from the OS clipboard (`powershell Get-Clipboard`, `pbpaste`, `wl-paste`/`xclip`/`xsel` on a
  thread, first line for single-line fields) in place of Bevy's in-app paste. `--screen NAME --screenshot PATH
  --after-secs S` and `--resume AGENT` (presses Resume at start) for visual checks.
  `testbed_content::scenario::embedded_scenarios()` lists every embedded scenario with a one-line description.
- **Explorer New / Resume screens**: New has the agent and world names prefilled with `random_name` (wall-clock
  seed), a **Random** button per field, and **Start** (or Enter in a field), disabled with an inline reason while
  a name is blank or the agent's folder exists ("an agent with this name exists: resume it instead"). Resume lists
  the agents folder's agents with their world and last save date (from the save header), "no save yet" ones
  disabled; it runs `actor_testbed --check-save` off the frame ("Checking ..." screen) and starts the game when it
  fits, else shows "This world was saved by another version of the game: <error>" with **Load anyway** /
  **Fresh world, keep the diary** (an unreadable save: the error and Fresh world only).
- **Explorer mode in `actor_testbed`**: `--explorer-home DIR --agent NAME [--world NAME] [--resume
  [--load-anyway | --fresh-world]] [--check-save]`. New creates the agent folder and an `explorer` world (actor
  named after the agent, wall-clock seed, saved at tick 0); Resume rebuilds from the save header's scenario and
  seed and loads the snapshot before the first frame; a save from another build exits 3 without a choice;
  `--check-save` prints `{"ok":..}` JSON and exits 0/1 without a window. The sim starts paused. Explorer mode
  picks the first free BRP port in 15702..=15721 (exit 2 when none, or without `debug-tools`), writes
  `start_prompt.md` / `resume_prompt.md` into the agent folder, exposes the prompt in an `ExplorerPrompt`
  resource, and saves every 60 s (wall clock, after the driver set) and on exit, also when the window is closed
  (the final save runs in `OnAppExitSystems`, after the window's exit); `--record` / `--commands` are refused
  with `--resume`.
- **Explorer prompt modal**: `testbed_view::{ExplorerModalPlugin, PromptModal, PromptModalClosed}` show a centred
  panel (max 900 px, scrollable monospace 13 px prompt, notes) with **Copy** (pipes the text into the OS copy tool
  on a background thread: `clip.exe` fed UTF-16LE+BOM, `pbcopy`, or `wl-copy`/`xclip`/`xsel`; "Copied" for 2 s
  only when the tool exits 0, else a note naming the prompt file) and **OK**; the game's input
  (`ViewSystems::Input`: command UI, order/camera keys, speed keys) waits while it is open. Explorer mode shows the
  start/resume prompt there and unpauses the sim on OK. No new crates and no Bevy `system_clipboard` feature (its
  `clipboard-win` dependency is BSL-1.0, which `deny.toml` does not allow).
- **F7 explorer panel** (`testbed_view`, `ExplorerFiles`, `--panels explorer`; explorer mode only): the agent's
  proposals first (`[status] title`, the 12 newest, then "(+N more)"), then its 5 newest diary entries, each cut
  to 280 characters; re-read within ~2 s of a change on disk; the box is capped at 85% of the window height.
- **Explorer prompt templates**: `testbed_content::explorer::prompt::{PromptKind, PromptVars, render_prompt}`
  render the paste-into-an-AI prompt (start and three resume variants) from `content/testbed_content/prompts/*.md`.
  The common part says what a paused world looks like (wait rather than repeat actions), that walls are not sensed
  and a far Move paths through doors and gates, and how to recover a lost controller; the plain resume says the
  save may be up to a minute older than the diary, so trust the senses. Examples use `VERB@ID`, not real ids.
- **Explorer agent folders**: `testbed_content::explorer::{names, home, notes, settings}` give random agent/world
  names, per-agent folders (`create_agent`, `list_agents`, `slug`), diary/proposal parsing and the agents-folder
  `Settings` (std + serde only). A leading BOM in `agent.json`, `settings.json`, the diary or a proposal is
  tolerated; the Windows read-only attribute on a folder (not enforced there) does not reject it.
- **`testbed:explorer`**: a new embedded scenario on the `frontier` layout (the compound plus groves, ore, a ruin
  and scattered items outside, on a 128-cell grid) with one `scout` under a `Direct` `explorer` controller.
  Existing scenarios are unchanged (64-cell grid, same spawn order).
- **Docs**: README quick start starts with `cargo run -p game_menu` and shows the AI explorer
  (`docs/img/explorer.jpg`); `docs/agent-experiments.md` §9 "The AI explorer" (the flow, the folder layout, the
  prompt variants, resume, **Load anyway** / **Fresh world, keep the diary**); the explorer support code in
  `content/testbed_content/README.md`; `CLAUDE.md` crate map (`apps/game_menu`, snapshots in `npc_kernel`).
  An end-to-end run with its findings: `docs/_notes/explorer-e2e.md`.

### Added (world snapshots, ADR 0013)

- **World snapshots** (`npc_kernel::snapshot`): `SnapshotRegistry` (save/load per component and
  resource by stable name, rebuild functions), `SnapshotAppExt`, `save(world) -> WorldSnapshot` and
  `load(world, &snap, LoadMode::{Strict, Lenient}) -> LoadReport` (version and content-hash checks, skipped
  and defaulted names). `register_sim_component[_with]` also registers the component (bound `+ DeserializeOwned`,
  `_with` also `+ Serialize`); `SimHashResource::snapshot_fns` opts a resource in; `SimClock` and
  `SimIdAllocator` derive serde and are saved as `sim_clock` / `sim_id_allocator`. `Journal::clear`. Snapshot
  entries are not in the content hash: state hashes, content hashes and replays are unchanged.
- **Every tick-deciding piece of state is in world snapshots**: the command queue,
  `Brain` (by tree name), `BrainState`/`ActiveLeaf`/`Blackboard`/`BrainStats`/`ThinkRequest`, the spatial grid
  (static and dynamic layers, unreported changes; index rebuilt, one re-sync on the next update), the nav queue
  (requests, paused A* search and its scratch stamps), pending noises, `Provides`/`Approach`/`Reservations`,
  `PauseDecay`, `MoveIntent`/`MoveStatus`/`ExternalMover` (saved even when not hashed), `HostPosition`, agent
  controllers/queues and the agent id counters, the village places/villager/`VillageIds` and `TestbedIds`;
  `DoorCells` is rebuilt from the saved synced cells (doors removed after the saved sync are cleared next tick),
  and `key_replan_pending` carries `Inventory`/`Equipment` changes made after `key_replan_system` ran. `load`
  settles change detection (loaded components look unchanged to every system; rebuild functions run after it),
  never re-hands a loaded `SimId`, and range-checks ids before changing anything.
  `npc_tools::snapshot_check::assert_resume_equivalent` proves save → load → run equals an uninterrupted run
  (three runs: never saved, saved, loaded); the testbed, every embedded scenario, mid-walk/craft/key/mounted
  saves and the village pass. **Changed:** snapshot entries take explicit names
  (`register_snapshot_component_named`, `register_snapshot_resource_named`, new `register_snapshot_component_with`;
  the type-name variants are gone); re-registering the same type under the same name is a no-op. State hashes,
  content hashes, replays and goldens are unchanged.
- **Snapshot files**: `npc_kernel::snapshot::{write_atomic, read, check}` (re-exported as
  `npc_framework::snapshot`; `check` makes `load`'s version and content-hash checks without loading) and
  `npcctl snapshot save | check` (`docs/debugging.md` 2.6b).

### Changed

- **Testbed `doors` and `agent` scenarios**: the scout `eye` no longer has `fixed_facing`, so its vision cone
  follows its movement like its model does (it started facing +Z and stayed there). No replay, golden or
  snapshot uses either scenario; `perception` keeps its fixed facings.

### Added (FX4: facade/API gaps from the fresh-agent extensions, `docs/_notes/fresh-agent-{a,b}-friction.md`)

All additive: apps that use none of it keep their state hash, content hash, journal and replays byte for byte.

- **Custom orders**: `UnitOrder::Custom { name, args: JsonObject }` (`UnitOrder::custom_with(name, &args)`,
  `order.custom()`, `order.journal_name()`), run by `CommandAppExt::register_custom_order(name, OrderExecutor)`
  (content hash `order_custom`); the journal's `order` field is the name. **Per-verb `Interact` executors**:
  `register_interact_verb_executor(verb, name, exec)` (content hash `order_verb`), consulted before the
  `"Interact"` kind executor, so game verbs coexist with the interaction glue (`OrderExecutors::for_order`).
- **`IssueOrder::skip_invalid`** (`.skip_invalid(true)`, JSON `"skip_invalid":true`, not serialized when false):
  units that fail a per-unit check are skipped and journaled `OrderFailed { reason: "skipped: .." }`;
  `IssueOrder::partition`, `submit_order_report` → `IssueReceipt { tick, seq, units, skipped }`, `issue_to`.
- **Approach engine** `npc_command::approach`: `ApproachSpec { reach: Reach::{Point, Area}, goal, tolerance_mm }`
  (`to_point`, `to_footprint`), `approach_step` → `InReach`/`Walking`/`Blocked`, `approach_then`. The command ×
  interaction glue's walk into reach now runs on it (same behaviour: testbed replays unchanged).
- **Footprint queries** (`npc_kernel::spatial`): `SpatialGrid::footprint_area`, `footprint_cell_rect`,
  `cells_around(pos, footprint, ring)`, `nearest_free_cell_around`, `nearest_blocker_by_edge(world, from, max_mm,
  filter)` → `BlockerHit`, `area_dist2`.
- **`command_ui`**: `CommandUiAppExt` (`add_right_click_order`, `add_context_order` and `_subset` variants; any
  plugin order; the interaction glue uses it), subset resolvers returning `ClickOrder { units, order }` (old
  resolver signatures unchanged), `right_click_order_for`, `context_order_for`, `issue_to_units`; right-click
  orders go out with `skip_invalid` (`SkipInvalidOrders`, default on); `PickTarget` marks visuals that right
  clicks pick but selection does not.
- **Journal**: `record_effect(world, unit, kind, &payload)` and `JournalKind::effect(kind, &payload)`; the prelude
  now exports `JournalKind`, `JsonObject`, `set_journal`, `record_effect`, the approach and receipt types, the
  footprint queries, `free_for`/`free_slots`/`try_reserve_slot` and (with `perf`) `PerfAppExt`. `CommandConfig`
  fields are documented.
- **`npc_jobs`**: multi-slot reservations `free_slots` (also `Reservations::free_slots`), `try_reserve_slot`,
  `Reservations::slot_of`; the village's `available_food_for` uses `free_slots`.
- **Perf budgets from content**: `PerfAppExt::register_budget(mechanic, fixed_us, per_agent_us)` into the app's
  `PerfBudgetTable`, `NpcPerfPlugin::budget(..)` / `budgets_with`, `PerfBudgets::default_npc_with`,
  `add_rows`, `default_npc_for_world` (used by `npcctl bench`). `actions.eat/work/sleep/idle` moved out of
  `NPC_BUDGET_TABLE` into `village_content::VILLAGE_BUDGETS` (same effective budgets for the village).
- **`npcctl run --all-needs`**: adds `needs_all` (every registered need by key, every entity with `Needs`, any
  content); the default summary is unchanged.

### Changed (FX4: docs, skills and test hygiene from the same friction logs; no oracle changed)

- **One re-baseline procedure**: `docs/rebaselining.md` (stage → evidence → user approval → swap, with exact
  commands, the list of checks that fail on a re-baseline, the evidence note template and the swap commands);
  `scripts/rebaseline/stage_replays.sh` re-records corpus files into `tests/replays.next/`. CLAUDE.md,
  `debugging.md` §8.2, `.claude/rules/{content,sim}.md` and the skills point at it.
- **SimId stability** documented (`.claude/rules/content.md`, `/add-mechanic`, `rebaselining.md` §5); tests look
  ids up (`VillageIds`, `TestbedIds`, `next_sim_id`, the agent scenario loaded in process) instead of hard-coding
  them; `corpus_equivalence.rs` (village, testbed) accepts a documented id remap
  `tests/replays.next/<name>.idmap.json`.
- `golden_inspect_unit5` split into one test per snapshot (`_t600`, `_t1800`, `_t4000`; snapshot files
  unchanged), so one failure no longer hides the others.
- `npcctl` tests kill spawned `serve`/`mcp` processes on drop (no leaked server holding the exe).
- `/add-mechanic` and `/add-unit-type` rewritten (what "content only" touches, perf tag + budget, need-id order,
  `SetNeed` timing, multi-slot reservations, tree branch vs order executor, the executor lifecycle, the RTS
  order → autonomous → idle pattern, `.chain()` for action systems); a small-scenario harness
  `apps/rts_units_demo/tests/small_scenario.rs`; `using-npc-framework.md` §4.1/§4.3 (need-id order, `SetNeed`
  timing, what shares `NpcSet::Actions`/`Locomotion`); gate timings in CLAUDE.md.

### Fixed (FX1: final simulation review, `docs/_notes/final-review-sim.md`)

- **Nav queue starvation**: a paused head search restarts on `GridChanged` only when a changed cell is one it
  opened or is next to one it expanded (`Search::touches`); one request's work over all restarts is capped at
  `SEARCH_LIMIT_BUDGETS` (8) × `NavBudget` (at least the grid's cell count, so an uninterrupted search is never
  capped), past which it fails with the new `PathFailReason::SearchLimit` and the requests behind it are served.
- **Nav agents with their own `GridBlocker`** (e.g. `Direct` vehicles) plan and move: their own footprint is open
  to their search, smoothing and mover (`SpatialGrid::cell_without_one_blocker`), and their own cell changes do
  not make them replan.
- **Interaction leave hook** (`npc_interaction.items`): a holder that leaves the sim drops its items where it stood
  (`SimId` order, journaled `ItemMoved`, back in the grid); a carried item that leaves is removed from its holder's
  inventory/equipment. `OnLeaveFn` now also gets the leaving `Entity` (components still readable).
- **`close`** checks the doorway against live `SimPos`, not the start-of-tick grid index.
- **Crafting** completes with the claimed items: `Crafting { tools, inputs: (id, units) }`, checked by
  `check_claimed` (a shrunk claimed input makes the craft `invalid`), instead of re-picking inputs and tools.
- **Commands inside a tick**: `submit` inside `NpcSimTick` (after the tick's commands applied) is `Unavailable`
  without `at_tick`; `submit_deferred` queues for the next tick; the running tick is `TickInPast`
  (`CommandQueue::applied_through`). `CommandLog` keeps each command's own `(tick, seq)`: no key collisions.
- **Formations**: spacing is validated (`1..=POS_LIMIT`) for `IssueOrder` and `CreateSquad`; world slots are
  clamped to ±`POS_LIMIT`. A squad that leaves the sim no longer leaves `MemberOf(dead)` on its members.
- **`per_roll` dice key** (`detect_key`): unique per observer and tick (noises carry `Detection::noise`
  ordinals; targets 2^30 apart and sourceless noises no longer share dice); unambiguous cases keep their old dice.
- **Hashing**: pending `Messages<Noise>` (`pending_noises`, new name: changes the content hash of perception
  apps), the paused nav search's whole progress (`nav_queue`; unchanged encoding while no search is paused),
  crafting's claimed tools/inputs; `Brain.tree` is hashed only with `npc_kernel::hash::register_brain_hash` (opt-in:
  by default it would change every village hash).
- **Limits**: `PostCraftOrder` at most `MAX_CRAFT_ORDERS` (64) open orders per workstation; footprints are
  clamped to the grid's size (`footprint_cells`, `footprint_indices`).

### Security (FX2: tooling, transport and guardrails after the final review)

- **Session server** (`npc_agent_io::http`, `npcctl serve`): refuses requests with an `Origin` header (403), a
  `Host` other than `127.0.0.1|localhost|[::1]:PORT` (421) or a non-JSON `Content-Type` (415); optional bearer
  token (`serve --token`, clients and `mcp --token`, 401); limits: 64 KiB headers, a 10 s request deadline, 64
  connections, 1 MiB bodies, 64 calls per batch, an empty batch is a `-32600` error. `world/load_scenario` takes
  the scenario inline only (no `path`). Transport errors name the URL.
- **Client file paths** (`npc_tools::out_path`, one checked function): `npc/record/stop {path}` and
  `npc/screenshot {path}` are relative, confined under the output root (`--out-dir`, default
  `target/npc-out`), extension forced, no overwrite unless `overwrite: true`.
- **BRP off by default in release builds** of `village` / `actor_testbed` (`--remote` turns it on); the shared
  remote setup moved to `npc_view_helpers::remote` (`RemoteArgs`, `install_remote`, `npc/screenshot`,
  `record_from_startup`). `bevy_remote` 0.20 cannot check `Origin`/`Host`: documented residual risk
  (`docs/debugging.md` §3.2).
- **Hook** `protect_goldens.sh`: trailing dots/spaces, `\\?\` prefixes, NTFS streams and 8.3 names folded;
  `git checkout|restore|reset|stash|apply ..` (and patch contents), PowerShell `ri|cpi|mi|rni`, `[IO.File]::`,
  `python -c` writes, `cd` + relative paths and deleting a directory that holds oracles are blocked; copying *from*
  an oracle is allowed; `xtask/src/main.rs` and the CI workflow are protected. Docs: launch Claude Code in
  `npc-framework/` (CLAUDE.md, README, debugging.md §8.2).

### Changed (FX2)

- Session host `npc/step` runs long steps in 50-tick slices, serving requests and long-polls in between.
- MCP `events` sends a per-call `nonce`; BRP watching requests with identical params get separate state.
- CI: `verify` is the only fmt/clippy/test/deny job; clippy on feature combinations, an MSRV (1.97.1) job.
- `cargo xtask verify` (full) fails on an empty replay corpus; stale "Task P2" texts fixed.
- npcctl's empty `debug-tools` feature removed; scenario lookup is file-first in `npcctl` and `actor_testbed`;
  `npc_agent_io::scenario::attach_controllers` is public and used by the testbed app and content.
- Removed `build.log` and the `docs/_notes/{npcctl,rpc}-reference.md` pointer stubs (their content is in
  `docs/debugging.md`); `scripts/p3/capture.sh` handles testbed replays and session records.

### Changed (FX3: agent interface after the first fresh-agent play-test)

- **Perception gate for affordances**: with a perception provider, `agent/observe` offers only affordances whose
  target the unit perceives, remembers, carries or rides (central filter in `npc_agent_io`; new registries
  `register_known` / `register_possessions` / `register_describer` / `register_unit_info` / `register_busy`), with
  no hidden radius for known targets (`why_not: "too far: 12.0 m .."`). Text: item kinds and positions for
  percepts, no "remembered" own items, a night line, tool targets' yield and uses, skill-check odds
  (`check`, `[Mining 13 vs 3d6: ~83%]`), the auto-spend vs bank explained; recent events drop perception rolls
  and keep the agent's own outcomes; `observe {brief}` hides structurally impossible affordances.
- **Acts**: atomic multi-step `agent/act {steps}` (all or nothing; plans' commands are validated before any is
  submitted), the `interact` verb (walk into reach and act, one act), wrongly typed args are `invalid_args`,
  staleness counts only relevant events (listed in the rejection, `interrupting`) and `stale_ticks` defaults to
  600, acts skipped at apply time are journaled (`ExternalAction {status:"rejected"}` + `ActionRejected`) and the
  queue limit counts same-tick acts; `execute_orders` ends `done` when the order queue runs dry.
- **Events**: `epoch` on every response, `since_seq` beyond the latest seq is an error, `oldest_seq` / `dropped`,
  `until_idle`, `min_events`, `dedupe`, unknown kinds rejected with the valid list, binary-searched scans;
  `list_units` rows carry the `owner` player. Journal JSON: `Roll.target` is `target_number` (`target` still read).
  CLI: `observe --brief`, `act --interact VERB@ID | --steps JSON` (a note when `--queue` is used with `order`),
  `events --until-idle --min-events --dedupe`.

### Added (RTS demo: engineers repair walls)

- `apps/rts_units_demo`: walls carry a hashed `WallHealth` (three start damaged); idle engineers repair the nearest
  damaged wall within 12 m (guard `rts_damaged_wall_near` → action `RtsRepair`: walk to a free cell next to it,
  then +4 hp/tick, journal `Effect {type: "WallRepaired"}`); the order `Interact { verb: "repair", target }`
  (executor `rts_units_demo.repair` for kind `Interact`) and R + right click (`RightClickKind::Custom
  ("rts.repair")`) send engineers anywhere; damaged walls are tinted; the headless summary has `damaged_walls`.
  Content only: no facade change. Tests: `apps/rts_units_demo/tests/repair.rs`.

### Added (village: thirst and the well; behaviour change, re-baseline staged)

- **Thirst + well** (content only, `village_content` / `village_view`): need `Thirst` (decays 45/tick awake,
  paused asleep like energy), a `Well` smart object per plot (`PlaceKind::Well`, `Provides Thirst` 2500/tick,
  `Approach`, 2 drinking slots via `Reservations` and `jobs::free_slots`; SimId 6 in `village`, spawned after
  the plot so the older SimIds stay, so runtime spawns start at 7), actions `ReserveWell` / `DrinkWater` (`Drink` hashed component, `Activity::Drink`, slot
  released in the tick the drink completes), conditions `thirst_urgent_well` / `thirsty_well`, guards
  `UrgentDrink` (below `UrgentEat`, abort lower) and `CasualDrink` (below `NightSleep`), the agent verb `drink`,
  perf mechanic `actions.drink` (budget row in `VILLAGE_BUDGETS`, like eat's), a well model and a thirst bar
  in the F1 overlay; `npcctl run --all-needs` reports `thirst`. Test: `content/village_content/tests/thirst.rs`.
  New names change the content hash: the village replays and goldens are re-staged (`tests/replays.next/`
  with `village_spawn_despawn.idmap.json` (6 → 7), `.snap.new`), evidence in
  `docs/_notes/thirst-merge-rebaseline-evidence.md` (the pre-merge branch note: `thirst-well-evidence.md`).

### Changed (wave H2: portability and API gaps from the RTS demo)

- **`command_ui` right-click order kind** (additive): `RightClickMode { kind, once }` (host-set from its own
  hotkeys; `held` / `once`), `RightClickKind::{Move (default: the context order), AttackMove, Patrol,
  Custom(name)}`, custom kinds by name in `RightClickOrders` (`RightClickOrderFn(&World, &RightClick)`),
  `PatrolDraft` (Shift+right click collects patrol points, a plain click issues `Patrol`, leaving the mode
  issues the draft; drawn as gizmos), the pure `right_click_order` / `patrol_click`, and the system set
  `NpcCommandUiSet` (hosts setting the mode run `.before` it). `apps/rts_units_demo` no longer unbinds the
  plugin's right click: A / P set the mode, S / H use `issue_to_selection`.
- **Public picking and selection**: `npc_view_helpers::{pick, PICK_HEIGHT, PICK_RADIUS}`,
  `command_ui::{pick_unit, select_units, select_in_rect, unit_screen_positions, issue_to_selection}` and the
  public `DragBox` (the drag in progress or a scripted marquee, drawn by the plugin). The RTS demo's
  screenshot mode uses them instead of its own picker and marquee; the testbed's `--select` uses
  `select_units`.
- **Profiler setup in two calls**: `npc_framework::perf` is now a module (`npc_perf::*` re-exported as
  before) adding, with `view-helpers`, `log_plugin()` / `perf_layer` / `perf_log_filter()` and
  `NpcPerfPlugin` (`tag`, `tags_with`, `budgets`, `end_frame`; default tags, budgets scaled for the build
  profile, `PerfHandle`, `end_frame_system` in `Last`), implemented in `npc_view_helpers::perf_setup`.
  `apps/village`, `apps/actor_testbed` and `apps/rts_units_demo` drop their ~20-line `LogPlugin
  { custom_layer }` modules.
- **Perception scan** (same results, cheaper): candidates outside every vision cone and outside
  proximity are dropped before the nearest-first sort, line of sight is traced only for visited cone
  candidates, the scan returns a sorted `Vec`, agents no longer clone `Sensors` every tick, hearing finds
  listeners by binary search. 200-agent dense crowd (`npc_perception` `tests/perf.rs`, release, same
  machine): stagger 1 ~2.50 → ~1.87 ms/tick, stagger 4 ~0.72-0.79 → ~0.56-0.59 ms/tick, identical
  percept counts; the `("perception", 20, 5)` budget (1.02 ms at 200 agents) is unchanged.
- **Key after a failed path**: `npc_nav::retry_failed_path(world, agent)` re-queues one agent's failed
  request; `npc_interaction`'s `door::key_replan_system` (in `NpcInteractionSet::Complete`, after the
  completions) calls it for a `NavAgent` + `Interactor` whose inventory/equipment changed and who now
  carries the key of a locked door (a per-agent pass cost, so no `GridChanged` came). Tests
  `failed_path_replans_when_the_key_is_picked_up`, `retry_failed_path_requeues_one_failed_request`.
- **Prelude** (additive): `state_hash`, `content_hash`, and with `command` `create_squad` (the RTS demo no
  longer reaches into `kernel::hash` / `kernel::registry` / `command`).
- `docs/adding-a-host-app.md`: the `deny.toml` `bevy`-wrappers rule for new host apps (a protected file:
  ask the user) and an open proposal. No oracle changed: village and testbed replays, goldens and the RTS
  demo's headless summary (seed 1 / 200 units: state `2091ecc0bdb3c7b6`, all arrived at 1043) are byte-identical.

### Added (wave H1: CI, documentation, agent skills)

- **CI** (`.github/workflows/npc-framework.yml` at the repository root; `working-directory: npc-framework`, runs on
  changes under `npc-framework/**`): `lint` (fmt + clippy `-D warnings`), `test` (nextest + doc tests), `verify`
  (`cargo xtask verify` on ubuntu-latest and windows-latest, with cargo-nextest and cargo-deny installed), `deny`
  (cargo-deny-action), `features` (`cargo hack check --each-feature -p npc_framework --no-dev-deps`, plus the 23
  capability-glue and view-helper feature pairs alone; a full powerset would be 2^15 builds), `determinism` (on
  both OSes: `npcctl replay verify` over `tests/replays/*.json`, `npcctl run --seed 1 --ticks 7200`,
  `--scenario testbed --ticks 4800`, `rts_units_demo --headless-ticks 1500`; hashes uploaded as artifacts) and
  `determinism-compare` (fails when Linux and Windows disagree). Bevy's Linux libraries (ALSA, udev, X11,
  xkbcommon, Wayland) are installed where the workspace builds; cargo is cached (`Swatinem/rust-cache`).
- **`docs/debugging.md`**: the debugging guide (every npcctl subcommand and BRP/agent/world/session method with
  examples and result shapes, error codes, all 39 journal kinds grouped by crate, mechanics and budgets, Tracy and
  Chrome traces, `cargo xtask verify`, the hooks, and recipes: why did unit X do Y, reproduce and bisect a desync,
  the slowest mechanic, the live game with curl, a screenshot at tick N, a recorded agent session).
  `docs/_notes/npcctl-reference.md` and `rpc-reference.md` are now pointers to it.
- **`docs/using-npc-framework.md`**: integrating the framework into another Bevy game (the dependency and
  features, `NpcPlugins` and every `with_*`, a complete unit type registered by name, the host-owned schedule,
  tick and movement seam, Relaxed mode, every capability with a snippet and its hooks, the glue table,
  determinism rules, view helpers incl. `RightClickMode` and the perf setup, versions). Its snippets were
  compile-checked and run in a scratch crate against the facade.
- **`docs/agent-experiments.md`**: running AI-agent experiments (headless `serve` or the windowed testbed, attach
  modes, the real-time observe → act → events loop, staleness and idle policy, a prompt template, MCP setup for
  Claude Code, analysing and verifying a recorded session, a worked key → store → pickaxe → ore example).
- **Skill `/play-actor <unit> <goal>`** (`.claude/skills/play-actor/SKILL.md`); `/add-mechanic`,
  `/add-unit-type`, `/verify-visual`, `/debug-desync` and `/why` updated for the content crates, the testbed, the
  RTS demo, capability kinds and the CI determinism job.
- README rewritten as a front page (screenshots, features per capability, quick start, crate map, guides);
  `CLAUDE.md` lists every crate and app and points to the guides.

### Added (wave G2a: the windowed Actor Testbed)

- **`content/testbed_view`** (presentation only; never writes sim state or inserts components on sim entities):
  Dungeon wall runs tiled from `wall` pieces; doors from `wall_doorway` squeezed along the wall (`DOOR_SX` 0.5) so the
  leaf fits the sim's 1 m door cell, the leaf re-parented onto its own hinge pivot (non-uniform scale without
  shear), swinging smoothly with `Door.open`; trees that turn into stumps when their `ToolTarget` is used up, the ore
  rock shrinking with its uses, workbench / anvil / forge, chest; items lying on the ground, equipped tools in the
  holder's hand bone, other carried items hidden; the cart (turned handle-first) and the siege-ram wagon with
  re-pivoted, spinning wheels; Knight / Rogue (player 1) and skeletons (player 2) with faction rings, animation
  from sim state (`Pose`: idle, walk, work, interact, craft, pick up, ride) and riders seated on their vehicle;
  RTS camera; F1 overlay (pose, interaction, crafting, ride, needs, tree path, order queue, equipment,
  inventory), **F5** character sheet (attributes, secondary, traits, skills with effective level, points, XP,
  level, policy), **F6** perception panel and gizmos (cone, line-of-sight rays, memory markers, last noise); the
  RTS command UI (`CommandUiPlugin`) with the glue's item / door / vehicle resolvers plus `testbed.use_tool`
  (tree, ore → `use_tool_on`), Z/X/C/V formations, H/B/U hold / stop / unload.
- **`apps/actor_testbed`**: `--scenario NAME|FILE`, `--seed`, `--speed`, `--paused`, `--player N`, `--select
  ID[,ID..]`, `--camera X,Z,DIST[,YAW,PITCH]`, `--commands FILE` (submitted when each tick comes up),
  `--screenshot PATH --after-secs S`, `--panels journal,perf,sheet,perception`, `--journal`, `--record`,
  `--remote-port` / `--no-remote` / render port flags; BRP `npc/*` + `npc/screenshot` + **`AgentRemotePlugin`**
  (`agent/*`, `world/state` against the window). Speed keys Space / `.` / `-` `=` (digits are control groups).
  The windowed sim matches the headless one (testbed seed 1, tick 117: the same state hash).
- **`npc_view_helpers::command_ui`**: `CommandFormation` (the formation group moves carry; hosts bind keys) and
  `order_command_with` (additive).
- **`apps/village`**: `AgentRemotePlugin` behind `debug-tools` (read-only there: no `AgentIoPlugin`, the content
  hash is unchanged; `tests/rpc.rs` `rpc_agent_methods_read_the_village`).
- **arch_fitness** (tightening): `testbed_view` is a `VIEW` crate, `testbed_content` must stay headless, canaries
  `actor_testbed → testbed_view → bevy_render`.
- README "Actor Testbed" section, `docs/img/testbed_{overview,panels,door}.jpg`.

### Added (wave G2b: Actor Testbed glue scenarios, npcctl testbed support, testbed corpus)

- **npcctl** takes the testbed in `run`, `inspect`, `journal`, `replay record|verify`, `diverge` and `bench`:
  `--scenario testbed` / `testbed:<name>` (an embedded scenario; a bare `smithy` is qualified) or a scenario file
  (`*.json`, built by its content's `npc_agent_io` loader). A factory registry (`apps/npcctl/src/sim.rs`,
  `CONTENTS`) resolves names; the village is last and takes every other name, so village output, replay headers
  and errors are byte-identical (the `golden__run_seed1_7200` snapshot and the village corpus are unchanged).
  Additive: `run` adds `content_summary` (absent for the village; testbed: units, doors, vehicles, ground items)
  and counts every brain's aborts/leaf starts for other contents; `inspect` prints the kernel's installed
  inspector for other contents; `bench --scenario S` (default `village_n`; fixed-size scenarios run once);
  `--seed` defaults to 1 (a scenario file: its own seed). `replay record --scenario FILE` prints a session record.
  `replay verify` dispatches: a session record re-runs from its document (as `verify-session`), a replay by the
  content claiming its `scenario` name. `diverge` checks both files are of the same content.
- **`testbed_content`**: `replay` (`try_new_replay_app`: the testbed's replay `AppFactory` for `testbed` /
  `testbed:<name>`, `ScenarioInfo.name` qualified; `verify`, `try_diverge`, `is_testbed`, `qualified`),
  `summary::summary_json`, scenario `smithy.json` (smith + joiner with the `player` policy + apprentice); books
  and scrolls `Teaches` (Carpentry / Smithing, max 12, 200 ticks), `smith`-template units are a `Teacher` of
  Smithing (100 ticks) (`content::TEACHES` / `TEACHERS`). Components only (already registered by the glue): the
  testbed content hash is unchanged by them; the testbed state hash changes (no earlier testbed oracle existed).
- **Tests**: `content/testbed_content/tests/glue.rs` (cart via its driver + `Unload`, `Mount` orders, the wagon
  with crew, queued `Interact` orders key → store → pickaxe, mine → smelt → forge with quality from Smithing,
  crafting XP → level-up → AI spend, Player policy + `SpendPoints`, study and teaching gated by level, a scripted
  `Direct` agent doing key → store → pickaxe → ore by `observe`/`act` only with `why_not` checks and its session
  replayed without the agent, an Advisor ordering a squad in formation, faction ownership rejections);
  `content/testbed_content/tests/corpus.rs`; `apps/npcctl/tests/testbed.rs` (CLI + insta golden
  `testbed__run_testbed_4800`).
- **Corpus** (new oracle files; existing ones untouched): `tests/replays/testbed_default_2days.json` (4800
  ticks), `testbed_orders_craft.json` (`testbed:smithy`, 2400 ticks, 18 commands), `testbed_vehicles.json`
  (`testbed:vehicles`, 1200 ticks, 8 commands), `testbed_agent_session.json` (session record, 600 ticks, 16
  commands). The village corpus test skips other contents' files; `cargo xtask verify`'s replays step counts
  files per kind (village / testbed / session).

### Added (wave G2c: `apps/rts_units_demo`, the portability proof)

- **`apps/rts_units_demo`**: a self-contained RTS host depending only on `npc_framework` (`nav`, `command`,
  `view-helpers`, `perf`) + `bevy`, no `content/*` crate. Its own content in the app (`sim.rs`): unit types
  `Infantry` / `Engineer` registered by name (trees `rts_infantry` / `rts_engineer`: `has_unit_order` →
  `ExecuteOrder` over the `RtsIdle` leaf; components `rts_unit`, `rts_wall`, `rts_idling`; kind namer), a 96 × 96 m
  map with `GridBlocker` walls, 200 `NavAgent` units of 2 players in 4 squads, a scripted opening (squads cross the
  map around the walls; `AttackMove` + a shift-queued `Move`). Host (`host.rs`): `Time<Fixed>` at 60 Hz with its own
  physics-like system in `FixedUpdate`; the sim at 10 Hz `run_in(FixedUpdate)`. View (`view.rs`): capsule/cube
  visuals per player and squad, RTS camera, HUD, `command_ui` selection and control groups, right click with
  A (`AttackMove`) / P (`Patrol`) / Shift (queue), Z/X/C/V formations (`CommandFormation`), S/H, F2/F3/F4.
  Flags `--units`, `--seed`, `--idle`, `--screenshot PATH --after-secs S`, `--headless-ticks N` (JSON summary:
  arrived, ticks, `all_arrived_at`, state/content hash, wall time). Screenshots `screenshots/rts_demo_*.png`,
  `docs/img/rts_demo.jpg`; README "Portability" with the findings.
- Tests: `apps/rts_units_demo/tests/demo.rs` (`rts_demo_orders_move_squads`: all 200 arrive by tick 1500,
  identical summary and state hash across two runs; `headless_flag_prints_the_summary`;
  `host_fixed_timestep_is_untouched_in_demo`); arch_fitness `rts_demo_uses_only_facade` (direct workspace deps ⊆
  {`npc_framework`}, no content/app crate anywhere in the unified closure, dev deps included).
- **Facade prelude** (additive): `GridBounds`.

### Added (actor capabilities F4: facade glue for command, vehicles, progression, crafting)

- **Glue modules** (`npc_framework::glue`, ADR 0011/0012; each behind both facade features, its own plugin, added by
  `NpcPlugins` when both capabilities are on; none is registered otherwise):
  - `command_interaction` (`CommandInteractionPlugin`): order executor `npc_interaction.interact` for
    `Interact { verb, target }` (walk into the verb's reach through `MoveIntent`/nav, then `begin`; `OrderDone` on
    success, `OrderFailed` with the verb's own reason; location-independent checks fail before walking; queued
    *pick up key → unlock → open* works); hashed `InteractOrder`; finished hook `npc_command.interact_order`; with
    `view-helpers` the `ContextOrders` resolvers `npc_interaction.item` (→ `pick_up`) and `npc_interaction.door`
    (→ `open` / `unlock`); shared engine `interaction_order_step`.
  - `command_vehicles` (`CommandVehiclesPlugin`): redirect `npc_vehicles.crew` (a `ByCrew` vehicle's orders go to its
    `Commandable` driver), executor `npc_vehicles.drive` for `Move`/`AttackMove` (a driver steers the vehicle's
    `MoveIntent`; a vehicle ordered itself goes through `command_vehicle_to`, i.e. its AI driver's `DriveGoal`;
    riders fail "dismount first"; `Direct` vehicles and everyone else use the built-in move), `npc_vehicles.mount`
    (walk into `mount_reach_mm`, then `mount`), `npc_vehicles.unload`; with `view-helpers` resolver
    `npc_vehicles.mount`. Content gives a `Direct` vehicle `Commandable` + `Owner` (it is a unit); a `ByCrew` vehicle
    needs them too (orders name it), its driver `Commandable` to receive them.
  - `crafting_interaction` (`CraftingInteractionPlugin`, feature `crafting`): interaction busy predicate
    `npc_crafting`, so no interaction starts while a unit crafts.
  - `progression_crafting` (`ProgressionCraftingPlugin`): craft hook `npc_progression` (`award_xp(.., XP_CRAFT)` on
    success, `grant_practice` of the recipe's duration).
  - `progression_interaction` (`ProgressionInteractionPlugin`): verbs `study` (target with `Teaches { skill,
    max_level, session_ticks }`, e.g. a book; `grant_study` capped by the book) and `teach` (actor with
    `Teacher { skills, session_ticks }` and a sheet, target a student with `Experience`; gated like
    `grant_teaching`); components hashed as `teaches` / `teacher` (they live in the glue, so `npc_progression`'s
    content hash is unchanged and it stays off `npc_interaction`).
  - `command_agent` (`CommandAgentPlugin`): section `command`, actions `order {order, units, queue, formation}`
    (validated `IssueOrder` for any owned units/squads, Direct and Advisor) and `execute_orders` (Direct units with
    a `Brain` queue the `ExecuteOrder` leaf).
  - `vehicles_agent` (`VehiclesAgentPlugin`): `mount@v` / `dismount@v` affordances (interaction verbs, `why_not`),
    `drive_to {pos}` (Direct driver: `DriveVehicle` leaf; else the recorded `DriveTo` command), `unload`, section
    `vehicle`.
  - `progression_agent` (`ProgressionAgentPlugin`): section `progression` (XP, level, next level, learning).
  - `crafting_agent` (`CraftingAgentPlugin`): `craft:<recipe>` affordances per nearby workstation with
    `CraftError` `why_not` (handlers per recipe and `craft {recipe}`; Direct: `Craft` leaf, Advisor: `Craft`
    command), section `crafting`.
- **`npc_interaction`** (additive): `InteractionListeners` with busy predicates (`add_busy_check`, consulted by
  `busy_with`, so `begin`, the `Interact` command and affordances) and finished hooks (`add_finished_hook`, called
  when an interaction ends: success/failed/invalid/cancelled), by name; names join the content hash only when
  registered (none by default).
- **`npc_agent_io`**: `SIGNIFICANT_KINDS` lists `OrderIssued/Started/Done/Failed`, `Mounted`, `Dismounted`,
  `VehicleStalled`, `XpGained`, `LevelUp`, `SkillImproved`, `CraftStarted/Done/Failed`.
- **Architecture fitness**: rows for `npc_progression` (rules only among capabilities) and `npc_crafting`
  (interaction + rules only), both in `CORE`, facade as-built rules and canaries for `progression` / `crafting`.
- Tests: `crates/npc_framework/tests/glue_command.rs`, `glue_learning.rs`, `glue_agent.rs`.

### Added (actor capabilities F3: `npc_progression` and `npc_crafting`)

- **`npc_rules` roll feed** (additive): `RollCounters.feed: RollFeed { enabled, records }` with `RollRecord { tick,
  unit, skill, outcome, purpose }`, filled by every roll (`roll_against` and up, `roll_with`) only when enabled;
  `RollCounters::drain_feed`. Off by default (nothing changes without a consumer); never fed from the journal.
- **`npc_progression`** (opt-in, facade feature `progression`, `ProgressionPlugin`; `NpcPlugins::with_progression()`):
  - `Experience { xp, level }` (requires `Learning` and `SpendPolicy`, default `Ai`); XP sources by name
    (`XpSourceDef`, defaults `roll`/`craft`/`job`/`discovery`); roll XP from the feed (successes of registered skills:
    `5 + clamp(16 - target, 0, 10) + 2 * difficulty step + 5 on a crit`); `award_xp(world, unit, amount, source)`.
  - Default curve: triangular, base 100 (levels at 100, 300, 600, 1000 .. total XP), 5 banked points per level, max
    level 50. `SpendPolicy::Ai` spends at once, one skill point at a time on the highest `uses / (points + 1)`;
    `Player` banks for `SpendPoints`.
  - Learning: practice (30 ticks per roll, 300 per point, cap attribute + 2), study (200 per point, cap attribute + 4
    and the book's max), teaching (100 per point, teacher ≥ student + 1, cap the teacher's level): `grant_practice`,
    `grant_study`, `grant_teaching` (the `study`/`teach` verbs are facade glue, wave F4).
  - Journal `XpGained`, `LevelUp`, `SkillImproved`; perf mechanic `progression`; inspect `experience_json`; state hash
    (`experience`, `learning`, `spend_policy`, `progression_config`) only with the plugin.
- **`npc_crafting`** (opt-in, facade feature `crafting`, `CraftingPlugin`; `NpcPlugins::with_crafting()`):
  - `RecipeDef` by name (inputs by kind or tag from the inventory, tools carried or equipped, workstation kinds,
    skill + modifier, duration, outputs, xp), validated at freeze (`validate_recipe`); `Workstation { kinds }`.
  - One path for the `Craft` leaf (recipe, or the next order on the workstation), the `Craft` command and orders:
    claims the workstation (capacity 1) and the items, rolls `npc_rules::roll_skill` (purpose `craft.<recipe>`) at the
    end; quality `min(margin / 2, 5)`, +2 on a critical success; a failure keeps the inputs; a critical failure
    consumes them and damages the tools (`Durability` −10, or −1 quality without it); every attempt wears tools by 1.
  - `CraftingOrders` + `PostCraftOrder` command, `crafting_jobs` (job board), `next_order_for`; completion hooks by
    name with `CraftCompleted` (the progression glue is wave F4); `CraftError` reasons usable as `why_not`.
  - Journal `CraftStarted`, `CraftDone`, `CraftFailed`; perf mechanic `crafting`; inspect `crafting_json`; state hash
    (`workstation`, `crafting_orders`, `durability`, `crafting`, `craft_leaf`, `crafting_config`) only with the plugin.
    Village oracles unchanged (the village adds neither plugin).

### Added (actor capabilities F2: `npc_vehicles`, seats, mounting, ByCrew/Direct vehicles)

- **`npc_vehicles`** (opt-in, facade feature `vehicles`, `VehiclesPlugin`; `NpcPlugins::with_vehicles()`, which
  also turns on interaction; ADR 0012: depends on `npc_interaction` only among the capabilities):
  - `Vehicle { seats: [Seat { role: Driver | Passenger | Gunner, required, offset }], speed_mm_per_tick, control:
    ByCrew | Direct }`; `MountedIn { vehicle, seat }` on the actor and `Occupants` (one `SeatSlot` per seat:
    `Free` / `Claimed` / `Taken`) on the vehicle, both by `SimId`, changed only by the crate.
  - Mounting is the interaction verbs `mount` / `dismount` (reserved by `npc_interaction`, registered here): one
    path for the `Interact` leaf/command, affordances and the new commands `Mount { unit, vehicle, role? }`,
    `Dismount { unit }`, `Unload { vehicle }`. Seat = requested role (strict), else `SeatPreference`, else first
    free; seats are claimed per mount (lower `SimId` wins a tie, the loser is cancelled with the reason); reach
    `VehicleConfig::mount_reach_mm`. Dismount/unload place actors on free cells next to the vehicle (ring search,
    nearest first, never through walls or wall corners, unoccupied cells preferred).
  - While mounted: `SimPos` = vehicle + seat offset every tick (after the movers), the actor's own `MoveIntent`
    is dropped, BT conditions `Mounted`, `MountedAsDriver`, `HasDriveGoal`.
  - `ByCrew`: the driver's `DriveVehicle` action writes the vehicle's `MoveIntent` toward its `DriveGoal`, its
    standing order (`SimCommand::Order` to the driver) or blackboard slot `goal_slot`; without a driver (or with a
    required seat empty) the intent is dropped, `MoveStatus::Blocked`, journal `VehicleStalled`. `Direct`: the
    vehicle moves through locomotion/nav like any agent with its crew aboard. Glue hooks:
    `command_vehicle_to`, `driver_of`, `can_move`, `set_drive_goal`, `mount`/`dismount`/`unload`.
  - Journal kinds `Mounted`, `Dismounted`, `VehicleStalled` (NAMES, journal panel); perf mechanic `vehicles`
    (tag + budget); inspect `vehicle_json`; crew gizmo `npc_view_helpers::vehicle_gizmos` (feature `vehicles`);
    state hash (`vehicle`, `occupants`, `mounted_in`, `seat_preference`, `drive_goal`, `driving`,
    `vehicle_config`, plus locomotion state) only with the plugin. Village oracles unchanged (the village does not
    add the plugin).

### Added (actor capabilities F1: `npc_command`, RTS orders, queues, formations, squads)

- **`npc_command`** (opt-in, facade feature `command`, `CommandPlugin`; `NpcPlugins::with_command()`):
  - `Owner(PlayerId)` and `Commandable { speed_mm_per_tick, arrive_mm }` on units; every order is the recorded
    content command `IssueOrder { player, units, order, queue, formation? }`, validated against ownership on submit
    and again at its tick. `UnitOrder`: `Move`, `AttackMove` (tagged move), `Stop`, `Hold`, `Follow { target }`,
    `Patrol { points }`, and the data-only `Interact { verb, target }`, `Mount { vehicle }`, `Unload` (rejected
    unless an `OrderExecutor` is registered for the kind by name: capability glue).
  - Per-unit `OrderQueue` (current `ActiveOrder` with progress + shift-queue): `queue: false` replaces (journal
    `OrderFailed { reason: "replaced" }`), `queue: true` appends. Journal kinds `OrderIssued`, `OrderStarted`,
    `OrderDone`, `OrderFailed` (in `JournalKind::NAMES`, journal panel).
  - Execution in `NpcCommandSet::Execute` (after `Sense`, before `Think`) in `SimId` order through the locomotion
    seam; `Brain` units run orders via the BT condition `has_unit_order` + leaf `ExecuteOrder` (aborting pauses).
    Hooks by name: `register_order_executor(kind, name, ..)`, `register_order_redirect(name, ..)`.
  - Group moves: integer formation slots (`Line`, `Box`, `Wedge`, spacing in mm) facing centroid → target, blocked
    slots relocated to the nearest free cell (deterministic), one leader whose nav path followers reuse shifted by
    their offset. `Squad { members, formation }` + `MemberOf`, `CreateSquad` / `DisbandSquad` commands.
  - Perf mechanic `command` (tag + budget); inspect `command_json`; state hash (`command_owner`, `commandable`,
    `order_queue`, `executing_order`, `squad`, `squad_member`, `command_config`, plus locomotion and spatial
    state) only with the plugin. Village oracles unchanged (the village does not add the plugin).
- `npc_view_helpers` feature `command`: opt-in `CommandUiPlugin` (click/box/shift selection, Ctrl+1-9 control
  groups, right-click context orders with `ContextOrders` resolvers by name, order/waypoint gizmos), configurable
  `CommandBindings`; it only emits `SimCommand`s.

### Added (actor capabilities E1: `npc_interaction`, items, inventory, doors, tools, verbs)

- **`npc_interaction`** (opt-in, facade feature `interaction`, `InteractionPlugin`; `NpcPlugins::with_interaction()`):
  - `Item { kind, stack, quality }` entities (in the world with `SimPos`, or carried: `CarriedBy { holder, slot }`),
    `ItemKindDef` by name (weight g, stacking, tags such as `"tool:axe"` / `"key:gate"`, equip slot);
    `Inventory { slots, max_weight_g, items }` and `Equipment { slots }` as sorted `SimId` lists; `Interactor`.
  - `InteractionDef` by name (`register_interaction` / `override_interaction`): offer filter, preconditions with
    reasons, duration (fixed or per target), optional skill check, effects, target reservation, noise, item arg.
    Built-ins `pick_up`, `drop`, `give`, `open`, `close`, `lock`, `unlock`, `equip`, `unequip`, `use_tool_on`
    (`ToolTarget { required_tag, yields, duration, check, remaining }`); `mount`/`dismount` reserved for vehicles.
  - One path: the `Interact` BT leaf (`ParamValue::Name(verb)`, target/item from blackboard slots 7/6), the
    `Interact { unit, verb, target, item? }` content command and doors on nav paths all call `begin`; completions
    run in `NpcInteractionSet::Complete` in `SimId` order. Hooks by name: skill check (none: checks pass) and noise.
  - `Door { open, locked_by, key }`: the door cell's static grid data (tag bit 0, blocking + occluding while closed,
    extra cost 1 while locked); nav pass-cost fn (closed door the agent may open: extra cost + path marker; locked
    without the key: blocked); a `NavAgent` + `Interactor` at a marked door unlocks (with the key) and opens it.
  - Journal kinds `Interacted { verb, target, item, outcome, reason }`, `ItemMoved`, `DoorChanged` (in
    `JournalKind::NAMES`, journal panel); perf mechanic `interaction` (tag + budget); inspect `interaction_json`;
    state hash (`item`, `inventory`, `equipment`, `carried_by`, `interactor`, `door`, `interactable`,
    `tool_target`, `interacting`, `interact_leaf`, `interaction_config`, the spatial static layer) only with the plugin.
- **`npc_framework::glue`** (ADR 0011): `interaction_rules` (`InteractionRulesPlugin`: checks are `roll_skill`
  with purpose `interaction.<verb>`; a failed roll spends the time, no effect), `interaction_perception`
  (`InteractionPerceptionPlugin`: door and tool noises become `Noise`), `interaction_agent`
  (`InteractionAgentPlugin`: affordances for every verb with `why_not`, handlers (Direct: the leaf; Advisor: the
  command), section `inventory`). Village oracles untouched (the village adds no interaction plugin).

### Added (actor capabilities E2: glue between the capability crates, ADR 0011)

- **`npc_framework::glue`** (facade modules, each compiled only when both features are on; capability
  crates still never depend on each other):
  - `perception_rules` (`perception` + `rules`): `PerceptionRulesPlugin { activate }` registers detection
    check `"per_roll"`: 3d6 under Per + distance band + sense + target modifiers by name
    (`PerRollAppExt::register_per_roll_modifier`; built-in `"stealthy"` reads the new hashed `Stealthy
    { penalty }`), tuned by the hashed `PerRollConfig` (no sheet: automatic; spotted targets tracked
    without re-rolls unless `reroll_sensed`). Dice keyed `(seed, observer, tick, "perception.detect",
    target·4+sense)`; every roll journaled as `Roll { skill: "Per", purpose: "perception.detect" }`.
  - `perception_agent` (`perception` + `agent-io`): `PerceptionAgentPlugin`, perception provider
    `perception` (`sensed` / `remembered` with last-seen ticks and ages, `last_noise`): `agent/observe`
    shows only what the unit perceives and reports `omniscient: false`; `world/state` still shows all.
  - `rules_agent` (`rules` + `agent-io`): `RulesAgentPlugin`, section `sheet` (attributes, secondary,
    traits, bought skills with effective levels, points), affordance/handler `spend_points {purchase}`
    (Direct and Advisor; `why_not` without points / for observers), registers `SpendPoints`.
- **`NpcPlugins`**: opt-in `with_perception()`, `with_rules()`, `with_agent_io()`,
  `with_per_roll_detection()`; they add the capability plugins and the glue of every pair that is on.
- **`npc_rules`**: recorded command `SpendPoints { unit, purchase }` (validated at submit and apply; not
  registered by `RulesPlugin`, so its content hash is unchanged).
- **`npc_tools`**: feature `rules` (forwarded by the facade's `rules`): BRP `npc/sheet {unit}` with `remote`.
- Docs: ADR 0011, CLAUDE.md crate map rows, `.claude/rules/sim.md` paths (new core crates and the glue),
  README architecture. Village oracles untouched (the village adds no glue plugin).

### Added (actor capabilities D3: `npc_agent_io`, external agent control in real time)

- **`npc_agent_io`** (opt-in, facade feature `agent-io`, `AgentIoPlugin`): controllers attached by recorded
  commands (`AttachController` / `DetachController`, modes `Direct` / `Advisor` / `Observer`, idle policy
  `Idle` / `Autopilot`); in `Direct` mode a reserved top-priority `ExternalControl` guard (injected into
  every tree at freeze time through the new kernel `TreeTransforms` hook) runs an `ExternalAction` leaf over
  a per-unit queue of steps that name registered actions (`AgentAct` / `AgentCancel` / `AgentClear`);
  provider registries by name (`AgentIoAppExt`: observation sections, perception, affordances with
  `why_not`, action handlers); observations as JSON (`omniscient: true` until a perception provider exists)
  and deterministic text; staleness guard (`expect_tick` / `expect_seq`, `AgentIoConfig`); per-controller
  latency stats; an event buffer harvested from the journal. Methods `agent/attach|detach|observe|act|
  cancel|clear|events|stats|list_units`, `world/state|load_scenario|reset`, `session/info|stop`. Hosts: a
  headless real-time session (std JSON-RPC over HTTP, BRP wire format; `agent/events` long-polls without
  blocking the loop, ADR 0010) and, with feature `remote`, BRP (`AgentRemotePlugin`, watching methods).
  Versioned scenario files with per-content loaders; session records that verify without the agent.
  Clients: `RpcClient`, a stdio MCP server, the `greedy` reference controller
  (`examples/greedy_controller`). Journal kinds `ControllerAttached`, `ControllerDetached`,
  `ExternalAction`, `ActionRejected`; perf mechanic `agent_io`. Without the plugin nothing changes (village
  oracles untouched); with it the content hash changes.
- **Village:** `village_content::agent` (`VillageAgentPlugin`: `needs` / `village` sections, `walk_to`,
  `eat`, `sleep`, `work` affordances and handlers; the `village` scenario loader),
  `try_new_sim_app_with`, `scenarios/agent_village.json`.
- **npcctl:** `serve --scenario s.json [--tick-hz N] [--record f] [--port N] [--seed N]`, `agent
  attach|detach|observe|act|cancel|clear|events|stats|list-units`, `world load-scenario|reset|state`,
  `session info|stop` (`--url`, `--format json|text`), `mcp --url`, `replay verify-session`.
- **npc_tools:** `RemoteAppExt::add_remote_watching_method`; re-exports `BrpError`, `BrpResult`,
  `error_codes`.

### Added (actor capabilities D1: `npc_perception`)

- **`npc_perception`** (opt-in, facade feature `perception`, `PerceptionPlugin`): `Sensors` (vision cone with an
  integer half angle checked by dot products against a cos² table, no runtime trig; hearing; proximity),
  `Facing` (follows movement unless `FixedFacing`), `Perceivable { kinds }`, `Noise` stimuli (`emit_noise` /
  `EmitNoise`, no-ops without the plugin), line of sight over occluding grid cells (`trace_segment`), bounded
  `PerceptionMemory` (forget after N ticks, oldest evicted), staggered scans `(sim_id + tick) % N` in
  `NpcSet::Sense` (`NpcPerceptionSet`), `DetectionChecks` hook by name (default `automatic`), conditions
  `sees_kind` / `remembers_kind` / `heard_noise` (+ generic `sees::<M>` etc.), `PerceptionFacts` blackboard
  mirror, `perception_json` inspect section. Journal kinds `Perceived` / `Lost`, perf mechanic `perception`,
  vision-cone / LOS gizmos in `npc_view_helpers` (feature `perception`). Without the plugin nothing is
  registered (village unchanged); adding it changes the content hash.

### Added (actor capabilities D2: `npc_rules`)

- **`npc_rules`** (opt-in, facade feature `rules`, `RulesPlugin`): point-buy `CharacterSheet` (ST/DX/IQ/HT,
  derived HP/Will/Per/FP/Basic Speed in quarters/Basic Move, traits, skill points, banked points); `SkillDef`,
  `TraitDef`, `SheetTemplate` registered by stable name (`RulesAppExt`, checked at freeze); a pluggable
  `RuleSet` resource (own default cost tables, doubling-then-linear skill curve, difficulty offsets, crit
  thresholds, default chain, dice and resolution functions); 3d6 roll-under `roll_skill` / `roll_stat` /
  `roll_against`, `quick_contest`, `resisted_roll` with a counter-based dice key `(seed, unit, tick, purpose,
  roll index)`; validated `spend_points`; `SheetView` JSON in inspect. Journal kind `Roll` (`Critical` in the
  kernel), perf mechanic `rules`. Without the plugin nothing is registered (village unchanged).

### Changed (actor capabilities C2: tooling follow-ups, eat claim, zstd)

- **Profiler:** budgets are checked only once a mechanic has `min_samples` samples since the last reset
  (`DEFAULT_MIN_SAMPLES` 60, `PerfCollector::set_min_samples`; `over_budget` waits too);
  `PerfBudgets::scaled_for_profile()` (×10 with `debug_assertions`) is what the village uses. `report()`
  selects percentiles in O(window) instead of sorting; `report_top` / `report_limited` summarise only the
  rows kept; the F4 panel caches its report and refreshes it at most every 0.5 s.
- **BRP ports:** `NpcRemotePlugin { port, render_port: RenderPort }` probes both ports and logs a taken one
  (with its number) instead of failing silently (`RemoteStatus`); the render-app server follows the main
  port (`port + 1`) or is off. Village flags `--render-remote-port N`, `--no-render-remote`.
- **Journal:** `JournalKind::NAMES` / `parse_name` (npcctl uses them); `with_journal_from_build` and
  `npc_tools::new_app_journaled` keep the scenario's tick-0 `Spawn` entries; `npcctl journal` uses it by
  default (`--journal-from-build false` for the old output), `inspect` on request. Library default unchanged.
- **One replay recorder:** `npc_kernel::replay::Recorder` (aligned or live cadence), re-exported as
  `npc_tools::recorder`, behind `replay::record`, `npcctl run --record` / `replay record` and BRP
  `npc/record/*` (`village --record`). Recorded replays are byte-identical to before.
- **Deferred command validation:** `submit_deferred` validates at apply time only (skips recorded in
  `SkippedCommands`); npcctl command files use it, so a command can target a unit spawned earlier in the
  same tick. Rejections still exit 2 with the same JSON.
- **zstd:** the windowed apps decode zstd KTX2 LUTs with Bevy's `zstd_c` (C `zstd`) instead of `ruzstd`;
  Bevy's default features are spelled out in `village`/`village_view` without `zstd_rust`, the libraries
  depend on Bevy without default features. `ruzstd` is gone from the graph and `deny.toml` no longer
  ignores RUSTSEC-2026-0344.
- **Eat claim (behaviour change, re-baseline staged):** `eat_system` releases the stockpile claim in the
  tick the meal completes, with its `TakeFood`, instead of when the leaf stops a tick later
  (`docs/_notes/c2-eat-claim-evidence.md`).

### Added (actor capabilities C1: spatial grid, locomotion seam, `npc_nav`)

- **Spatial grid** in the kernel (`npc_kernel::spatial`, `SpatialPlugin` in `NpcPlugins`): integer cells
  (default 1 m, 128 × 128), static `CellInfo` (blocking/occluding/tags + extra cost) merged with
  `GridBlocker`/`GridOccluder` footprints, a `SimId` index updated in `NpcSet::Inputs`, sorted
  `in_radius`/`in_box` queries, a conservative integer `trace_segment`, and `GridChanged` messages. Not hashed
  unless a capability calls `register_spatial_hash`.
- **Locomotion seam** (`npc_locomotion`, facade feature `locomotion`, default): `MoveIntent`/`MoveStatus`,
  `NpcSet::Locomotion` (between Actions and Effects) with `NpcMoverSet`, and the `StraightLineMover` in
  `NpcPlugins`. The village's `MoveToDest`/`FollowOrder` now write a `MoveIntent` and end on `MoveStatus`;
  the village stays byte-identical (corpus and goldens unchanged; the intent is not hashed unless
  `register_locomotion_hash` is called).
- **Pathfinding** (`npc_nav`, facade feature `nav`, opt-in `NavPlugin`/`GridPathMover` + `NavAgent` per
  unit): deterministic grid A* (10/14, no corner cutting, `(f, h, cell)` ties), LOS smoothing, a per-tick
  node budget (`NavBudget`, 4000) served in `(tick, SimId)` order with carry-over, replanning on
  `GridChanged` or `Blocked`, a per-tag `PassCost` hook with path markers (doors, wave E), journal
  `PathFound`/`PathFailed`, perf mechanics `spatial`, `locomotion`, `nav`, `nav.astar`, and a path gizmo
  (`npc_view_helpers`, feature `nav`). New scenario `village_nav` (wall per plot, `NavAgent` villagers).

### Changed (baseline hardening P3: crate split, stable-name registration)

- **Crate split** behind the facade `npc_framework` (`NpcPlugins`, `NpcAppExt`, `prelude`; features `tools`,
  `remote`, `perf`, `view-helpers`): `npc_sim` became `npc_kernel` + `npc_needs` + `npc_jobs` + the example
  content `content/village_content`; `npc_view` became the generic `npc_view_helpers` + `content/village_view`;
  `npc_remote` became `npc_tools` (feature `remote`), which also holds the CLI helpers and bench harness.
- **Registration by stable name** (ADR 0009): actions, conditions, scorers, trees, needs (`NeedDef`), sim
  components (`SimHash`), hashed resources and content commands; ids frozen sorted by name in
  `Plugin::finish`; trees reference names; typed leaf params (`ParamValue`); content hash in replays
  (format version 2; v1 files still load).
- **State hash** is registry-driven (registered components in name order). The encoding changed once;
  journals, inspect snapshots and run summaries are byte-identical before and after
  (`docs/_notes/p3-rebaseline-evidence.md`).
- **Commands**: `SpawnVillager` and `SetFood` are content commands (`ContentCommand`), same JSON on the wire.
- **Host-safe driver** (ADR 0008): `NpcPlugins::new().tick_hz(n).run_in(schedule)` with its own accumulator
  (`SimTime`); never writes `Time<Fixed>`; pause/speed/step in the kernel's `SimControl`, keys in
  `apps/village`; `DeterminismMode::{Strict, Relaxed}` with a `HostPosition` adapter; public labels
  `NpcSimTick`/`NpcSet`.
- `arch_fitness` covers the new crates (incl. an as-built feature resolver and a facade-only rule for content).


### Added

- **Behaviour-tree runtime (`npc_ai`)**: flat serializable `TreeDef` with a content hash, shared by all
  agents; per-agent `BrainState`/`Blackboard`/`ActiveLeaf`; `Selector`, `Sequence`, `Guard` with
  `Abort::LowerPriority`, `Leaf`, `UtilitySelector`; event-driven think for dirty agents only; actions as
  `SparseSet` components ticked by ordinary systems (ADRs 0001, 0005).
- **Deterministic headless sim (`npc_sim`)**: `SimTick` schedule at 20 Hz with ambiguity detection = Error,
  integer state (mm positions, milli-unit needs), `SimId` ordering, counter-based RNG, FNV state hash
  (ADRs 0002–0004).
- **Villager content (iteration 1)**: hunger and energy needs with hysteresis, smart objects (house, bed,
  stockpile, field) with `Provides`/`Approach`, `Reservations`, a food economy loop, day/night cycle, and
  the villager tree (urgent eat/sleep, forage, follow order, night sleep, casual eat, day work, wander).
- **Presentation (`npc_view`, `apps/village`)**: KayKit CC0 assets, separate visual entities (`VisualOf`),
  interpolated movement, animation from `Activity`, day/night lighting, RTS camera, F1 debug overlay,
  `--screenshot PATH --after-secs S`.
- **Debug tooling**: decision/event journal (JSON Lines, ring buffer, filters), `SimCommand` input layer
  with `GoTo`/`Clear` orders, replays (record, verify, diverge with per-entity diffs), inspect snapshots;
  all of it never changes sim results (ADR 0006).
- **`npcctl` headless CLI**: `run`, `inspect`, `journal`, `replay record|verify`, `diverge`, `bench`, JSON
  on stdout with exit codes 0/1/2.
- **BRP remote interface (`npc_remote`)**: `npc/pause|resume|step|speed|state|unit|command|journal|perf|
  perf/reset|record/start|record/stop|screenshot|methods` on `127.0.0.1:15702` (feature `debug-tools`).
- **Profiler (`npc_perf`)**: per-system timing via a tracing layer, mechanic tags, rolling p50/p95/max,
  per-mechanic budgets with rate-limited warnings, F3 journal and F4 perf panels, `bench/baseline.json`;
  optional `tracy` and `chrome-trace` features.
- **Agent workflow**: `CLAUDE.md`; path-scoped rules in `.claude/rules/` (sim determinism, view
  presentation-only, content registration); project skills `/add-mechanic`, `/add-unit-type`,
  `/verify-visual`, `/debug-desync`, `/why`; Claude Code hooks: a PreToolUse guard that protects goldens,
  snapshots, replays, baselines, lint config and the hooks unless the user creates
  `.claude/rebaseline-approved`, and a Stop hook that runs `cargo xtask verify --quick` and blocks while it
  fails (tests in `xtask/tests/hooks.rs`).
- **Architecture decision records** `docs/adr/0001`–`0008`, and this changelog.
- In progress (baseline hardening, `docs/superpowers/plans/2026-10-23-baseline-hardening.md`): the
  locomotion seam (ADR 0008) and `apps/rts_units_demo` (P4), CI (P5).
- Actor capabilities G1 (Actor Testbed assets + content): `content/testbed_content` (walled compound with every
  capability on; item kinds, skills, traits, sheet templates, recipes, unit trees with the order branch,
  `TestbedDrive`, `testbed` observation section, scenario loader `content: "testbed"` + 9 embedded scenario files,
  `try_new_sim_app`, headless scenario tests; `npcctl serve --scenario testbed`), CC0 testbed assets in
  `assets/testbed/` (POSIX `scripts/fetch_assets.sh`, `assets/CREDITS.md`, `docs/_notes/testbed-assets-measured.md`).
  New content only: village oracles unchanged.
