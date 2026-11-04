# NPC World Viewer

A standalone TypeScript / ExcaliburJS observer beside the Bevy games. This increment projects NPC
Framework's **island village** into a pixel-art 2D browser world: terrain and shore, named workplaces,
resource patches, residents, ground items and the played character. Every moving position, need,
activity and inventory comes from exported state.
Person inspection also shows the current public NPC decision; workplaces list recipe
ingredients/outputs and resources show gathering duration and their next renewal tick.
These optional schema-1 fields preserve support for older exports.
Names in the map, directory and inspector use the same friendly goods vocabulary.
Carrying groups repeated stacks for reading; the received item IDs and counts are unchanged.

The opt-in **Living Island Village** adds twelve residents, finite deer, fiber and berry
patches, Warmth and equipped clothing. Its published market table shows current prices,
offered stock and purchasing demand, including conditional affordability caps. Run the host
below with `--scenario living_village`, or create an explorer with `--world-preset living_island`.
Both observers follow the same authoritative world. See the framework's
[living economy guide](../npc-framework/docs/living-island-economy.md) for production chains
and shared player/AI market actions.

New expanded Living worlds also publish cleared house/garden sites and trade-district
captions. These form quiet map context; real homestead and cottage entities supply
ownership, built-home state and the actual stages/positions of garden beds. Select a
site or cottage to inspect it. The browser sends no claim, building or farming actions.
Saved compact worlds retain their own terrain.
Fit follows the received land bounds rather than a fixed island size.

## Run the island showcase

Requirements: Node **22.12+** (tested with 24), npm, and the repository's Rust toolchain/native build
tools. Run these commands in two terminals, from the indicated directories.

Terminal 1 — `npc-framework/`:

```powershell
cargo run --locked -p npcctl -- serve --scenario ai_village --seed 7 --world-state-out target/world-state/island.json --world-state-interval-ms 500
```

Terminal 2 — `npc-world-viewer/`:

```powershell
npm ci
npm run dev
```

Open [the local viewer](http://127.0.0.1:5173). Either process may start first. The viewer waits for a
missing file and reconnects automatically. NPCs run their existing autonomous economy; the visitor
starts idle. To stop the simulation gracefully, run `cargo run --locked -p npcctl -- session stop`
in another terminal in `npc-framework/`. Stop the viewer with Ctrl+C. If the default simulation port
is occupied, choose `--port N` for the host and `--url http://127.0.0.1:N` for its CLI clients.

An existing detached island explorer can publish the same file by adding these flags to its usual
`npcctl serve --explorer-home ... --agent ... --resume` command. Stop the active host before resuming;
do not start a duplicate host for an owned world. The native Bevy `actor_testbed` window accepts the
same export flags in scenario, explorer and resume modes. Export publishes initially and between native
frames, including while paused. No native window is required for the headless showcase above.

### Watch Herp in native 3D and pixel art together

Start one native explorer host from `npc-framework/` instead of the headless host above:

```powershell
cargo run --locked --release -p actor_testbed -- --explorer-home C:/NPC/herp-stream --agent Herp --world Island --preset human --world-preset island_village --world-state-out ../target/world-state/herp.json --world-state-interval-ms 500
```

Then, from `npc-world-viewer/`, follow that explicit output path:

```powershell
npm run dev -- --state ../target/world-state/herp.json
```

Both presentations now watch the same simulation and Herp's public dialogue/action feedback. The browser
remains read-only; native controls and the agent's game tools operate the one host. Do not start another
`npcctl serve` for the same explorer. For a later session, stop the owned host first, replace the native
creation flags with `--explorer-home C:/NPC/herp-stream --agent Herp --resume`, and keep the export flags.
The shared `../target/` path above resolves to the monorepo's ignored build-output directory from either
project. Custom paths must match the native output and viewer `--state` arguments.

### Configuration

```powershell
# Relative --state paths resolve from the shell's current directory.
npm run dev -- --state C:/path/to/world-state.json --port 5180
# WORLD_STATE_FILE and PORT are supported too.
$env:WORLD_STATE_FILE = 'C:/path/to/world-state.json'
npm run dev
```

The producer defaults to 500 ms; valid intervals are 50–60000 ms. Export is off unless
`--world-state-out` is supplied. Parent directories are created and the file is atomically replaced,
including while paused. Failed updates warn and retry. Use a dedicated projection path, separate
from saves, records or preset files; one producer per path. The final file can lag stop by one interval.

The Node process reads every 250 ms (following rename/replacement on Windows); the browser polls
every 500 ms. Snapshots older than five seconds are marked stale. With a producer interval above
five seconds, that status is expected between exports. Missing, invalid or partially written JSON
retains the last valid view and reports the problem. No CORS relaxation or direct browser access to
the simulation's RPC server is needed.

## Explore

- Drag the map or focus it and use arrow keys to pan. Scroll, `+` / `-`, or the buttons to zoom.
- **Fit island** (`0` with map focus) resets the camera. **Follow player** follows the current played
  SimId, including a successor after a new life. Manual pan/directory navigation releases follow.
- Click an entity or use the keyboard-accessible directory to inspect its latest state. Escape with
  map focus clears selection. Search and People/Places filters narrow the directory.
- **Labels** toggles names. People show names when selected or played, keeping workplaces readable.
- The inspector shows needs, health, coins, home coordinates, inventory, resident assignments and
  resource quantities. These controls change only the view; they send no gameplay commands.
- Market demand at the current quote is separated from lower-priced bids; conditional caps are
  displayed as context rather than promises that a trade has already happened.
- Homestead inspection shows the public owner, whether a cottage exists, crop stages and any
  supplied planting/watering/ready ticks. Bed graphics follow received stages; the browser never
  advances growth while the simulation is paused or the file is stale.
- Public spoken dialogue and action summaries appear in pixel bubbles, with speech pointers and
  thought dots. Herp and selected actors have priority when space is crowded; select a person to read
  both current records in the inspector. Action summaries contain no private reasoning or notebooks.
  Feedback expires against the received simulation tick, so a stale feed retains its marked historical view.

All artwork is original code-drawn pixel art with nearest-neighbor rendering. The TypeScript app
does not import the native game's 3D assets. The title uses self-hosted Silkscreen; see [credits](THIRD_PARTY.md).

## Build and verify

```powershell
npm test
npm run check:state -- ../npc-framework/target/world-state/island.json
npm run build
npm start                     # serves dist/ and follows the same configured file
```

`npm start -- --state FILE --port N` accepts the same options. The local Node server is required
in both modes: a browser cannot watch arbitrary disk paths itself. It binds only to `127.0.0.1`;
remote hosting and authentication are outside this increment. Keep the lockfile and use `npm ci`.

Rust checks run in `npc-framework/`: `cargo test --locked -p testbed_content --test world_projection`,
`cargo test --locked -p npcctl --bin npcctl`, then `cargo xtask verify`. Tests check simulation hash
equivalence, file replacement, invalid JSON/schema, missing files, recovery, resets and despawns.
Browser evidence and gate results: `../npc-framework/docs/_notes/2026-10-29-world-viewer.md`.

## Architecture

See [the JSON contract](docs/world-state.md).

| Source | Responsibility |
|---|---|
| `../npc-framework/content/testbed_content/src/projection.rs` | Read-only content adapter |
| `../npc-framework/apps/npcctl/src/projection.rs` | Configurable publication and atomic replacement |
| `server/feed.ts` | Validated file reads and last-good-state retention |
| `server/main.ts` | Loopback GET endpoint and Vite/static frontend hosting |
| `src/state.ts` | Runtime schema, types and stable-ID reconciliation |
| `src/world.ts` | Excalibur actors, camera and local selection |
| `src/art.ts` | Cached pixel-art graphics; terrain comes from the producer |
| `src/main.ts` | Polling, live status, directory and inspector |

No root Cargo workspace, shared lockfile or Rust dependency on TypeScript is introduced. Later
increments can add adapters/graphics for other games without moving their simulations into the viewer.
