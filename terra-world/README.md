# Terra World

**A reusable Rust and Bevy environment SDK with a playable geographic sandbox.**
Stream terrain and semantic features around real GPS positions, run a UTC world
clock and environmental model, attach actors, and persist player changes over
the original world. A native explorer, headless tools and an authoritative
multiplayer reference demonstrate the same library contracts.

The default playground is deterministic and synthetic, placed at real geographic
coordinates. Opt-in adapters load public elevation, OpenStreetMap features and
current weather. Explicitly dated local packs supply historical/scenario worlds;
the included era fixtures are synthetic examples. Every sourced chunk retains
provenance, attribution and fallback warnings.

This is an implemented foundation for games and simulation tools. It does not
claim to reconstruct every place and date on Earth, or to provide a full ecology,
fluid, traffic, geopolitical or rigid-body simulation.

## Start here

Run commands from this repository's root. `rust-toolchain.toml` pins Rust 1.99.1;
the workspace uses Bevy 0.20.0 and includes a lockfile. The desktop target needs a
graphical session, a working GPU backend and the platform development libraries
required by Bevy. The headless CLI does not need a window or GPU.

If the pinned toolchain is not installed yet, install it through Rustup:

```sh
rustup toolchain install 1.99.1 --profile minimal --component rustfmt
```

The native renderer in this workspace enables X11 on Linux. The Ubuntu CI build
installs these development libraries, in addition to a C/C++ build toolchain and
`pkg-config`:

```sh
sudo apt-get install build-essential pkg-config libudev-dev libx11-dev libxkbcommon-dev libvulkan-dev
```

You also need an active graphical session and a compatible graphics driver to
run the window. A first desktop build compiles the Bevy graphics stack and can
take substantially longer than a headless command. Windows and macOS native
builds have not been exercised in this deliverable; see the validation record
before assuming platform coverage.

### 1. Verify the complete gameplay loop without a window

```sh
cargo run -p terra-cli -- sandbox-demo --output saves/sandbox-demo.json
```

The JSON report walks through claiming land, gathering, crafting, building a
house and workbench, farming, harvesting, demolition, and a two-player faction
transfer with shared building permissions. The output save is validated and
reloaded, including exact geographic coordinate round trips.

### 2. Open the native world

```sh
cargo run -p terra-demo -- --camp-view
```

The offline scene starts near Munich with streamed synthetic terrain, world
features, starter resources, and reference actors. Ships are spawned only when
nearby loaded water is available. Scroll the left
panel for location, layer, time and political controls; scroll the right panel
for construction, farming, factions and saves. Press **Tab** to return to the
player, **O** for a landscape view, and **M** for the political atlas.

For a first house, claim the nearby plot, craft planks three times and stone
blocks four times, select **Place house**, and click free ground within 20 metres
of your player. [The gameplay guide](docs/GAMEPLAY.md) explains placement, shared
ownership, farming, saves and the full control scheme.

Local progress is saved explicitly with **F5** and restored with **F9**. Choosing
`--save saves/my-world.json` changes that path; it does not automatically load
the file on startup or save when the window closes. Restart with the same world
configuration, wait for the terrain, and press F9 to continue a saved settlement.

### 3. Inspect real data explicitly

```sh
cargo run -p terra-cli -- probe --live --no-osm --strict --output snapshots/munich.json
cargo run -p terra-demo -- --live --lat 48.137 --lon 11.575
```

The first command probes elevation and current weather while keeping the first
request small. Omit `--no-osm` to include map features. `--strict` fails on
provider errors instead of accepting synthetic fallback. The desktop reports
source/fallback status as it streams. Public endpoints have coverage and usage
limits; read [data sources and terms](docs/DATA.md) before configuring a game or
larger download.

## What is implemented

| Area | Working behavior | Scope and extension seam |
|---|---|---|
| Global coordinates | WGS84 geodetic ↔ ECEF, local east/up/south frames, floating-origin rebasing, date-line/pole-aware geographic tiles | Authoritative coordinates stay `f64`; source vertical datums need explicit conversion for precise use |
| Streaming | Background loading, request and memory budgets, disk cache, retries/backoff, stale-result rejection, mesh integration budgets | Replace `ChunkProvider` with your prepared world packs or data service |
| Terrain and features | Heightfields, roads, buildings, water, forest/farm/grass polygons and trees | Reference geometry is simplified; no full OSM relation, road graph or city reconstruction |
| Environment | UTC clock, approximate solar direction, day/night, hemisphere seasons, weather samples, heuristic biomes | Weather is a sampled condition; offline weather and biome inference are illustrative |
| Actors | Human, animal, creature, vehicle, ship, helicopter, balloon and airplane kinds; GPS motion and environment queries | Add your own AI, physics, navigation, flight and marine systems |
| Sandbox | Plots, materials, recipes, structures, housing, damage/destruction, resources, wheat growth/harvest and persistent tree edits | Immutable source geometry plus a mutable gameplay overlay; configurable catalog |
| Factions | Invitations, leader/builder/member roles, atomic transfer of plots and contained assets, shared housing and workbenches | Gameplay permissions are separate from real political sovereignty |
| History and politics | Validated dated packs, date-scoped political GeoJSON, overlapping claims, atlas and authored territory drawing/export | Imported coverage is explicit; no invented historical reconstruction or autonomous border simulation |
| Multiplayer | Server-owned actors and commands, movement/reach/ownership checks, bounded AOI snapshots, replay protection, save/resume | Single-process TCP/LAN reference; production transport, account services and distributed simulation are host concerns |
| Persistence | Atomic JSON saves, strict reference/ownership checks, world/epoch identity in host save envelopes | Catalog/source changes require explicit migration; offline elapsed-time progress is not automatic |

## Workspace map

| Crate | Responsibility |
|---|---|
| [`terra-core`](crates/terra-core/README.md) | Pure geographic math, tiles, time, solar/seasons, semantic world types, weather and actor contracts |
| [`terra-stream`](crates/terra-stream/README.md) | Providers, deterministic offline content, cache and bounded asynchronous chunk lifecycle |
| `terra-history` | Immutable dated world packs and political polygons, provenance, containment and GeoJSON import/export |
| [`terra-sandbox`](crates/terra-sandbox/README.md) | Transactional gameplay commands, inventories, construction, farming, factions and persistent overlays |
| [`terra-net`](crates/terra-net/README.md) | Authoritative multiplayer library, server executable and command-line client |
| `terra-bevy` | Reusable world/environment/sandbox/network plugins and optional reference renderer |
| `terra-demo` | Native explorer, construction interface and political atlas |
| `terra-cli` | Headless data probes, deterministic simulation and complete sandbox demonstration |

Core, streaming, history and gameplay are independent of Bevy. The Bevy adapter's
`render` feature is optional, so games can use their own meshes, materials, UI and
physics, or run a headless simulation. [Integration contracts](docs/INTEGRATION.md)
cover schedules, resources, actor attachment and host responsibilities;
[architecture](docs/ARCHITECTURE.md) describes the data and authority boundaries.

## Historical and political scenarios

```sh
cargo run -p terra-demo -- --config configs/historical-1950.json --atlas
cargo run -p terra-demo -- --config configs/historical-1980.json --camp-view
```

These presets use two deliberately synthetic era packs and a dated political
fixture. They exercise different terrain/features and political validity
intervals without claiming to portray Munich or any country's actual history.
For your own data, use `--world-pack path/to/manifest.json --epoch YYYY-MM-DD`
and optionally `--politics path/to/borders.geojson`.

The **source epoch** selects immutable base data. The **simulation clock** can
then evolve from that starting point. Changing the season does not re-download
another historical terrain revision. Political features are queried against the
simulation date and show only claims whose declared interval covers that time.
An unavailable historical tile stays unavailable; it never falls through to a
current public API. See [historical formats and provenance](docs/HISTORY.md).

The atlas can display imported countries/regions/disputed claims and draw
additional authored scenario territories. A territory's optional faction link
is descriptive. Construction rights come from the sandbox's owned plots and
roles, so drawing a large polygon does not confiscate other players' property.
The default local explorer also loads the bundled current Natural Earth country
snapshot when available. That generalized atlas layer is dated 2026-10-26; it is
separate from the synthetic terrain and from the fictional historical fixtures.

## Multiplayer

Start the reference server and two native clients in separate terminals:

```sh
cargo run -p terra-net --bin terra-server -- --save saves/server.json
cargo run -p terra-demo -- --connect 127.0.0.1:7777 --session saves/alice-session.json --camp-view
cargo run -p terra-demo -- --connect 127.0.0.1:7777 --session saves/bob-session.json --camp-view
```

Each client receives its own player ID. Arrow keys move the authoritative player;
WASD moves the spectator camera. Gameplay commands go to the server, which checks
identity, command sequence, quantity, position and permission before changing
state. All peers must use matching source revisions and data epochs. Give each
player a separate `--session` file and reuse that file to resume their identity.

For a resumable command-line session:

```sh
cargo run -p terra-net --bin terra-client -- --seconds 60 --session saves/alice.json
cargo run -p terra-net --bin terra-server -- --load saves/server.json --save saves/server.json
```

Stop the first server before restarting it. The session file carries the player's
resume capability; reuse it with the same server. The server saves periodically
and on a clean shutdown. Read [multiplayer operation](docs/MULTIPLAYER.md) for
desktop resume support, historical sessions, command JSON, identity handling and
the reference transport's limits.

## Configure and reuse

`WorldConfig` controls focus coordinates, seed, tile level/radius, resident and
request budgets, terrain resolution, layers, day/night, seasons, time scale,
floating-origin distance, public-data adapters, source packs and save paths.
Presets in [`configs`](configs) give concrete starting points.

```sh
cargo run -p terra-demo -- --config configs/offline.json
cargo run -p terra-cli -- probe --lat -33.9249 --lon 18.4241 --level 16
cargo run -p terra-cli -- simulate --steps 120 --dt 0.05
```

The sandbox catalog is JSON data. Change item IDs, recipe inputs/outputs,
blueprint costs/footprints and crop timing/yields through `Catalog::from_json`.
[`catalog.example.json`](crates/terra-sandbox/catalog.example.json) matches the
built-in content. A custom game can register its own trusted starter inventory
through `add_player_with_inventory`; catalog-aware restore checks existing
asset parameters before accepting a changed ruleset.

For large worlds, prepare immutable regional chunks and host them through your
own distribution service. Player overlays, inventories and actor state are a
separate authoritative persistence layer. The reference's public adapters and
in-process maps are useful development components, with explicit interfaces for
larger storage, spatial indexing and simulation partitioning.

## Verification and project notes

```sh
cargo test
cargo test -p terra-bevy --no-default-features
cargo check -p terra-demo
```

The default workspace test targets are headless. Tests cover geographic edge
cases, provider decoding and cache behavior, chunk budgets, temporal coverage,
transactional commands, ownership, persistence and real loopback multiplayer.
Bevy tests exercise the actual plugin with a manual clock and no renderer.
The [validation record](docs/VALIDATION.md) separates checks actually run from
hardware/network-dependent verification, and the [product review](docs/PRODUCT_REVIEW.md)
records the final end-to-end assessment and remaining scope.

Code is available under MIT or Apache-2.0. Public datasets retain their own
licenses and attribution requirements; see [third-party notices](THIRD_PARTY_NOTICES.md)
and [data documentation](docs/DATA.md). Synthetic fixtures are explicitly marked
as such and contain no real downloaded OSM extract.
