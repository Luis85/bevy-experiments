# Integrating Terra World into a game

Terra is a Rust workspace with a Bevy adapter. Domain types, gameplay rules, source loading and networking are usable separately. The desktop explorer is an example host; your game supplies its own camera, UI, assets, physics, AI and content policies.

## Choose the smallest dependency surface

| Requirement | Crate | Integration boundary |
|---|---|---|
| GPS, chunk addressing, calendar, sun, weather and actor descriptors | `terra-core` | Serializable types; no Bevy or HTTP dependency |
| Chunk loading, cache, real-source adapters and procedural fallback | `terra-stream` | `ChunkProvider` and bounded `ChunkStreamer` |
| Explicitly dated packs and political GeoJSON | `terra-history` | `HistoricalPack`, `HistoricalCatalog`, `PoliticsDataset` |
| Inventory, crafting, plots, homes, farming, resources, factions | `terra-sandbox` | Validated `Command` → state mutation / rejection |
| Authoritative multiplayer / TCP reference transport | `terra-net` | Host environment and chunk-registration hooks |
| Scheduling, coordinates, queries, optional meshes and lighting | `terra-bevy` | Plugins, resources and ECS components |

Add path dependencies from your game's Cargo manifest, or move the required crates into an existing workspace. These crates are not published to crates.io by this deliverable. Bevy is pinned to **0.20.0**; its ECS/render APIs must match your game's version.

```toml
[dependencies]
bevy = { version = "=0.20.0", default-features = false, features = ["std", "multi_threaded"] }
terra-core = { path = "../terra-world/crates/terra-core" }
terra-bevy = { path = "../terra-world/crates/terra-bevy", default-features = false }
```

The headless feature choice avoids a window and GPU renderer. Enable the default `terra-bevy` features and add `TerraRenderPlugin` when you want the example renderer. `DefaultPlugins` should be installed before the render plugin. A headless app uses `MinimalPlugins`.

```rust
use bevy::prelude::*;
use terra_bevy::{TerraWorldPlugin, WorldConfig};

fn main() {
    let config = WorldConfig {
        chunk_radius: 1,
        live_data: false,
        ..Default::default()
    };
    config.validate().expect("valid world configuration");
    App::new()
        .add_plugins(MinimalPlugins)
        .add_plugins(TerraWorldPlugin::new(config))
        .run();
}
```

Run the finite executable examples from the workspace root:

```bash
cargo run -p terra-bevy --no-default-features --example headless
cargo run -p terra-bevy --no-default-features --example custom_provider
```

## Coordinates and precision

Use `GeoPosition` for all persisted or networked positions: latitude/longitude in degrees, altitude in metres, all `f64`. `GeoPosition::new` validates finite values and latitude range, and normalizes longitude. `LocalFrame` converts through WGS84 Earth-centered coordinates. A `GeoAnchor` remains attached to its absolute coordinate when the renderer rebases its origin.

The local renderer uses **X east, Y up, Z south**. Actor velocity arrays use **east, north, up**. These are intentionally different conventions: do not pass a local `Vec3` directly as ENU velocity.

```rust
use terra_core::{GeoPosition, LocalFrame};

let position = GeoPosition::new(48.137, 11.575, 520.0)?;
let frame = LocalFrame::new(position);
let ten_metres_east = frame.to_geo([10.0, 0.0, 0.0])?;
# Ok::<(), terra_core::GeoError>(())
```

Authoritative coordinates stay in double precision. Bevy transforms are temporary local `f32` representations. Change `WorldRuntime.focus` to drive streaming; the plugin moves its local origin after the configured rebase distance. Never persist a Bevy `Transform` as a global location. Also never use local tangent-plane horizontal distance to decide whether an antipodal location is far away; use geographic surface distance.

The geographic chunk grid covers the poles and antimeridian. It is not an XYZ/Web Mercator grid. The Terrarium provider translates the geographic grid to Mercator source tiles and explicitly reports source coverage limits. The current renderer streams one configured level around the focus. It does not implement a globe-wide adaptive clipmap or orbital camera.

### Height datums

Coordinates are geodetic, but DEM height samples retain their provider's vertical datum. The bundled Terrarium adapter records a warning about orthometric versus ellipsoidal height; no geoid conversion is performed. Games requiring survey-grade GPS altitude must supply that conversion in their provider.

## Load your own data

Implement `terra_stream::ChunkProvider` and return a validated `WorldChunk` with source attribution. The loader calls providers on bounded background workers. `TerraWorldPlugin::with_provider(config, Arc::new(provider))` installs a custom source with the same scheduling and integration path. See `crates/terra-bevy/examples/custom_provider.rs`.

Set `base_data_revision` to an immutable dataset revision. Do not change the dataset while retaining its revision ID. Provider work should honor timeouts and payload budgets; the generic trait cannot forcibly cancel arbitrary user code. Queued work, in-flight requests and resident chunks are bounded. Results for abandoned focus regions are discarded. CPU mesh construction uses bounded background jobs; GPU asset insertion stays on the main Bevy schedule.

The built-in `StreamConfig` supports explicit endpoints, request timeouts, response limits, disk-cache TTLs/budgets and an optional additional certificate file. It never disables TLS verification. Direct `ChunkStreamer` users can configure these lower-level settings. `WorldConfig` exposes a deliberately smaller set of common game policies. If you need a custom endpoint policy in the Bevy host, construct your own provider with that `StreamConfig`.

World chunks contain a heightfield, typed vector features, provenance records and warnings. New simulation layers can be separate Bevy resources/components keyed by `TileKey` and epoch, or additional game-owned provider payloads. The existing `FeatureKind` enum and mesh generator are concrete reference layers, not a universal parser for every possible public dataset.

## Schedule your systems

| Set | Schedule | Contract |
|---|---|---|
| `WorldSet::Stream` | `Update` | Poll completed chunks, update resident data, request weather, shift origin |
| `WorldSet::Sandbox` | `Update` | Apply a bounded batch of local gameplay intents, or send intents to the authoritative server |
| `WorldSet::Render` | `Update` | Integrate meshes and synchronize geographic visual transforms |
| `WorldSet::Simulate` | `FixedUpdate` | Advance calendar, environment-dependent growth and optional kinematic actors |

`Stream → Sandbox → Render` ordering is explicit. Place focus/camera input before `Stream`, environment consumers after it, and visual customizations after `Render`. The fixed schedule is a separate schedule; merely ordering an `Update` system against `WorldSet::Simulate` does not establish cross-schedule timing.

`TerraWorldPlugin` installs `Time<Fixed>::from_hz(config.fixed_hz)`, which sets the app-wide Bevy fixed timestep, including other systems using `FixedUpdate`. Coordinate `fixed_hz` with your game's physics rate, or install your preferred `Time<Fixed>` resource after adding the plugin. Terra's fixed systems consume that shared clock; the setting is not a private Terra-only timer.

The layer mask controls presentation and the optional reference systems. Disabling weather stops local weather updates and makes `WorldRuntime::effective_weather()` return calm, clear synthetic conditions for local growth/movement and rendering; the last source sample remains in `weather`. Disabling actors hides them and pauses the local sample controller. In multiplayer the host retains simulation authority. `day_night` controls solar lighting, and `seasons` controls the vegetation tint; these do not erase the calendar or source evidence.

`WorldRuntime.clock.time_scale` is simulation seconds per real second. Pausing or speeding up the environment affects crops and resource regrowth. Locomotion uses fixed real simulation steps so changing the sun's speed does not multiply walking/flight velocity. Live weather is a current model sample and is labeled separately from the accelerated simulation clock. Historical packs never fetch current weather as historical evidence; their weather is a labeled synthetic model unless your game provides an archival layer.

## Query terrain and environment

```rust
use bevy::prelude::*;
use terra_bevy::{ResidentChunks, WorldEnvironment, WorldRuntime};

fn inspect_environment(chunks: Res<ResidentChunks>, world: Res<WorldRuntime>) {
    if let Some(sample) = chunks.sample_environment(world.focus, &world.effective_weather()) {
        // sample.terrain_height_m, slope_degrees, water, weather, sourced_terrain
        // Use these to drive a game-owned physics/controller/AI implementation.
    }
}
```

`None` means coverage is not loaded or unavailable. Treat it as unknown; do not invent a zero-height surface. The bundled kinematic controller freezes ground/water actors without required surface data. It supports ground, water and air categories, but does not simulate vehicle suspension, hull buoyancy, helicopter rotors, aerodynamics, collisions, navigation or animal behavior.

To give your own object a persistent geographic anchor:

```rust
use bevy::prelude::*;
use terra_bevy::GeoAnchor;
use terra_core::GeoPosition;

fn spawn_anchor(mut commands: Commands) {
    commands.spawn((
        GeoAnchor(GeoPosition::new(48.137, 11.575, 520.0).unwrap()),
        Transform::default(),
        // Add your game's meshes/materials/components here.
    ));
}
```

Do not attach both a game-owned physics transform writer and `GeoActor`'s sample movement controller to the same object without an explicit reconciliation system. `GeoActor` is optional. Add `LayerVisual(Layer::Actors)` if you want the sample visibility controls to apply to custom visuals.

## Gameplay and content catalogs

Queue a `SandboxIntent::Command` through `SandboxQueue` from the UI or other game systems. In local mode the Bevy adapter validates terrain support for plots, structures and crops, then delegates inventory, reach, collision footprint and permissions to `terra-sandbox`. In network mode intents go to the server and mutate no local authoritative inventory.

The pure rules crate does not query terrain, physics or pathfinding. Hosts using `SandboxState::apply` directly must apply their own environmental placement checks. Definition consistency and ownership are validated when restoring state with a catalog. Register players with the built-in starter inventory or the custom-inventory API when using a custom catalog. See `docs/GAMEPLAY.md` and `crates/terra-sandbox/catalog.example.json`.

Source geometry is an immutable layer. Player structures, crops, claims, inventories and resource edits are a separate mutable overlay. Stable source IDs keep gathered trees from reappearing on chunk reload. Whole sourced buildings/roads/water are not destructible through the bundled command API: clipped source geometry cannot establish ownership of the complete upstream object. Player-built structures have hit points and can be destroyed by permitted owners/builders. Arbitrary terrain digging and physical fracture are future host-specific systems.

Faction membership controls actual plots, houses, farms and resources. A political country's polygon is source information; it never automatically grants build permissions or claims political sovereignty for a gameplay faction. Authored scenario territories may carry a faction ID as a link. See `docs/HISTORY.md`.

## Persistence and networking

The desktop `LocalWorldSave` envelope stores the overlay, factions, player identity, simulation clock and edited politics. It validates world revision, seed, terrain level/resolution, frozen data epoch and political base ID before restoring. A save from another era is rejected without changing the current world. Writes use a same-directory temporary file and atomic replacement. The pure `SandboxState` JSON, desktop envelope and server save are distinct formats; they are not interchangeable.

The example desktop saves and loads only on explicit F5/F9 actions. `save_path` is a destination, not an autoload or autosave policy. A hosting game can queue `SandboxIntent::Save` and `SandboxIntent::Load` at its own lifecycle boundaries; network sessions leave persistence to the authoritative server.

Source packs and the current simulation date are separate concepts. A scenario can start from a 1950 dataset and then simulate decades of player changes. Advancing its clock does not quietly replace buildings with modern source data. Switching the frozen source epoch requires an explicit world restart/reload with an appropriate pack and overlay policy.

Multiplayer uses an authoritative reference server, interest-filtered snapshots, ordered commands, movement validation, replay rejection and bounded framed TCP. Matching manifests prevent a client from accidentally joining with a different seed, historical era or political revision. The reference transport has no TLS, public account system, cross-process shard coordination or prediction/interpolation. Keep it on trusted local networks or replace the transport/session boundary for an Internet game. Never put resume capability files in source control.

See `docs/MULTIPLAYER.md` for startup, session identities, save/restart and host integration.

## Production expansion order

1. Select a regional vertical slice and pin licensed source revisions, coordinate/height transformations and update policies.
2. Build an offline ingestion pipeline and host immutable tiles on your own object store/CDN. Public demo APIs are not a global game backend.
3. Add adaptive terrain LOD, terrain/feature collision, navigation, physics, real assets and asynchronous disk-backed overlays.
4. Replace the reference session transport with authenticated encrypted connections; add client prediction, interpolation and load testing.
5. Add region ownership/sharding, durable databases, migration policies, backups and observability before lifting the reference capacity limits.
6. Add specialized historical, climate, vegetation, agriculture, traffic or social models as evidence-backed layers with explicit uncertainty and provenance.

The implementation is a tested foundation and a working native example. It is not a complete scientific digital twin, a planet-scale production service, or a finished survival game.
