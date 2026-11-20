# Integrating Terra World into a game

Terra World is a Rust workspace with a Bevy adapter. Domain types, gameplay rules, source loading and networking are usable separately. The native demo is an example host; your game supplies its own camera, UI, assets, physics, AI and content policies.

## Choose the smallest dependency surface

| Requirement | Crate | Integration boundary |
|---|---|---|
| GPS, chunk addressing, calendar, sun, weather and actor descriptors | `terra-core` | Serializable types; no Bevy or HTTP dependency |
| Chunk loading, cache, real-source adapters and procedural fallback | `terra-stream` | `ChunkProvider` and bounded `ChunkStreamer` |
| Explicitly dated packs and political GeoJSON | `terra-history` | `HistoricalPack`, `HistoricalCatalog`, `PoliticsDataset` |
| Regional checkpoints, activation/retirement, complete export and online backup | `terra-persistence` | `RegionStore`, atomic generation, bounded dependency-closed working set |
| Construction, farming, factions, bodies, ecology, resources, territories, credits and logistics | `terra-sandbox` | Validated `Command` → state mutation / rejection; bounded living/ecology/economy advancement |
| Authoritative multiplayer / TCP reference transport | `terra-net` | Host environment and chunk-registration hooks |
| Scheduling, coordinates, environment, optional living population and renderer | `terra-bevy` | Plugins, resources and ECS components |

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

The geographic chunk grid covers the poles and antimeridian. It is not an XYZ/Web Mercator grid. The Terrarium provider translates the geographic grid to Mercator source tiles and explicitly reports source coverage limits. The default world requests 49 level-15 chunks at 65 × 65 elevation samples per tile. The optional [large-region preset](../configs/large-region.json) requests 121 chunks, with resident and queued ceilings of 144. That preset changes the source neighborhood, not the default or the living population limits. A plain `--live` demo invocation uses a smaller neighborhood unless a configuration or radius is supplied.

Streaming uses one configured source terrain level around the focus. The renderer adds distance tiers for both terrain interiors and feature geometry; every terrain border sample is retained and physical queries continue reading the full heightfield. This render-only LOD does not implement globe-wide adaptive source levels, clipmaps or an orbital camera.

### Height datums

Coordinates are geodetic, but DEM height samples retain their provider's vertical datum. The bundled Terrarium adapter records a warning about orthometric versus ellipsoidal height; no geoid conversion is performed. Games requiring survey-grade GPS altitude must supply that conversion in their provider.

## Load your own data

Implement `terra_stream::ChunkProvider` and return a validated `WorldChunk` with source attribution. The loader calls providers on bounded background workers. `TerraWorldPlugin::with_provider(config, Arc::new(provider))` installs a custom source with the same scheduling and integration path. See `crates/terra-bevy/examples/custom_provider.rs`.

Set `base_data_revision` to an immutable dataset revision. Do not change the dataset while retaining its revision ID. Provider work should honor timeouts and payload budgets; the generic trait cannot forcibly cancel arbitrary user code. Queued work, in-flight requests and resident chunks are bounded. Results for abandoned focus regions are discarded. CPU mesh construction uses bounded background jobs; GPU asset insertion stays on the main Bevy schedule.

The built-in `StreamConfig` supports explicit endpoints, request timeouts, response limits, disk-cache TTLs/budgets and an optional additional certificate file. It never disables TLS verification. Direct `ChunkStreamer` users can configure these lower-level settings. `WorldConfig` exposes a deliberately smaller set of common game policies. If you need a custom endpoint policy in the Bevy host, construct your own provider with that `StreamConfig`.

World chunks contain a heightfield, typed vector features, provenance records and warnings. New simulation layers can be separate Bevy resources/components keyed by `TileKey` and epoch, or additional game-owned provider payloads. The existing `FeatureKind` enum and mesh generator are concrete reference layers, not a universal parser for every possible public dataset.

### Import a raster heightmap

`WorldConfig.heightmap_manifest` selects a local georeferenced grayscale PNG.
The Bevy provider builder validates coverage at the configured focus and sets the
base revision from `PreparedHeightmapProvider::source_revision()`. Optional manifest
`features` imports an explicitly attributed, bounded GeoJSON layer. For custom
composition, construct `terra_stream::PreparedHeightmapProvider::open(path, resolution)`
and use it as a `ChunkProvider`. It accepts legacy single manifests and connected
sets with full seam/coverage validation. `HeightmapProvider` remains available
for single bundles. See [SOURCE_BUNDLES.md](SOURCE_BUNDLES.md).
The provider's `sample_height(position)` and `covers(key)` expose coverage for
host-side checks.

The demo accepts `--heightmap manifest.json` or `--config configs/heightmap.json`.
It can center an otherwise unspecified focus inside the raster and reduce radius
to complete covered tiles. The library host does not silently choose a different
scenario focus. Heightmaps cannot be combined with `live_data`, a historical
pack or an explicit historical epoch in one built-in `WorldConfig`.

The manifest declares geographic bounds, minimum/maximum height, immutable
revision and a complete `SourceRecord`. Eight- and sixteen-bit grayscale PNGs
are supported. Missing tiles remain unavailable; color imagery, unbounded
images and paths outside the manifest directory are rejected. Terrain height
alone does not imply land use, buildings or roads. See
[GRAPHICS_AND_MAPS.md](GRAPHICS_AND_MAPS.md) for the full schema and budgets.

## Schedule your systems

| Set | Schedule | Contract |
|---|---|---|
| `WorldSet::Stream` | `Update` | Poll completed chunks, update resident data, request weather, shift origin |
| `WorldSet::Sandbox` | `Update` | Apply a bounded batch of local gameplay intents, or send intents to the authoritative server |
| `WorldSet::Render` | `Update` | Integrate meshes and synchronize geographic visual transforms |
| `WorldSet::Simulate` | `FixedUpdate` | Advance calendar, environment-dependent growth and optional kinematic actors |

`Stream → Sandbox → Render` ordering is explicit. Place focus/camera input before `Stream`, environment consumers after it, and visual customizations after `Render`. The fixed schedule is a separate schedule; merely ordering an `Update` system against `WorldSet::Simulate` does not establish cross-schedule timing.

`TerraWorldPlugin` installs `Time<Fixed>::from_hz(config.fixed_hz)`, which sets the app-wide Bevy fixed timestep, including other systems using `FixedUpdate`. Coordinate `fixed_hz` with your game's physics rate, or install your preferred `Time<Fixed>` resource after adding the plugin. Terra's fixed systems consume that shared clock; the setting is not a private Terra-only timer.

The layer mask controls presentation and the optional reference systems. Disabling weather stops local weather updates and makes `WorldRuntime::effective_weather()` return calm, clear synthetic conditions for local growth/movement and rendering; the last source sample remains in `weather`. Disabling actors hides their reference visuals and pauses the legacy `GeoActor` sample controller. The living economy has its own `LivingSettings.simulate` switch; hiding actors does not erase bodies, stock or production. In multiplayer the host retains simulation authority. `day_night` controls solar lighting, and `seasons` controls the vegetation tint; these do not erase the calendar or source evidence.

`WorldRuntime.clock.time_scale` is simulation seconds per real second. Pausing or speeding up the environment affects crops and resource regrowth. Living food/water depletion and production use simulation time, while locomotion and exertion use fixed real simulation steps. Changing the sun's speed does not multiply walking/flight velocity. Setting time scale to zero freezes calendar-driven processes while allowing motion; `clock.paused` also stops the local living update. Live weather is a current model sample and is labeled separately from the accelerated simulation clock. Historical packs never fetch current weather as historical evidence; their weather is a labeled synthetic model unless your game provides an archival layer.

### Measure the host and client separately

`TerraWorldPlugin` installs `WorldDiagnostics`. `report()` returns a
`WorldTimingReport`; `stage(WorldStage::Ecology)` and the other stage variants
return `TimingSummary` values. Each window retains at most 240 samples and exposes
last, mean, p50, p95, maximum and sample counts. Stages cover streaming, base-world
advancement, ecology, living movement/production, settlement economy maintenance,
direct controlled motion and the regional checkpoint stage. Their measured boundaries are individual local
systems, not a complete server request or all game/render work.

The frame window records intervals between app updates, including waiting and
other systems; it is not GPU timestamp instrumentation. A network client records
its own frame/streaming work and does not manufacture host ecology or economy
samples. `WorldDiagnostics::reset()` explicitly starts a fresh measurement window;
it is separate from resetting an `EcologyScheduler` when replacing a world.
The reference demo exposes these readings in F12 and can export them with
`--telemetry FILE`. See [PERFORMANCE.md](PERFORMANCE.md) for interpretation,
reproduction commands and recorded native evidence.

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

`None` means coverage is not loaded or unavailable. Treat it as unknown; do not invent a zero-height surface. The bundled kinematic controller freezes ground/water actors without required surface data. It supports ground, water and air categories. The optional living adapter adds source-footprint, water, slope and authored-obstacle checks, local sliding and bounded routes. Legacy `GeoActor` remains kinematic. Registered transport adds four-wheel sampled
support, material/load-aware grip and visual wheel travel; see
[VEHICLE_DYNAMICS.md](VEHICLE_DYNAMICS.md). Physical suspension forces, hull
buoyancy, rotor aerodynamics and a general navigation mesh remain game work.

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

## Configure presentation and game-owned models

With the `render` feature, `terra_bevy::render::RenderQualitySettings` is a
resource independent of `WorldConfig`. Insert it before `TerraRenderPlugin`, or
change it at runtime. `RenderQuality` has `Performance`, `Balanced` (default) and
`High`, with `from_name()`, `label()` and `next()` helpers for a game's settings UI.

```rust,ignore
use terra_bevy::render::{RenderQuality, RenderQualitySettings};

app.insert_resource(RenderQualitySettings {
    quality: RenderQuality::Balanced,
    atmospheric_sky: true,
    terrain_lod: true,
});
```

Profiles adjust shadow maps, antialiasing, atmospheric/environment lighting,
terrain detail and ground-cover limits. `atmospheric_sky: false` selects the
compatibility lighting path; Performance also disables atmospheric scattering.
`RenderCapabilities.atmospheric_sky` reports the adapter capability check.
`terrain_lod: false` keeps full terrain mesh interiors, with the corresponding
render cost. These switches do not change source heights, collision, population,
ownership or server simulation budgets.

`WorldDetailSettings` remains the requested nearby ground-cover/resource/boundary
policy. Its default requests 640 clusters within 96 m; quality limits cap that
request rather than increasing it. Performance caps ground cover at 320 clusters
within 64 m. See [GRAPHICS_AND_MAPS.md](GRAPHICS_AND_MAPS.md) for the complete
profile table, mesh budgets and the source-to-render distinction.

`ActorSceneOverrides` maps actor kinds and wildlife species to game-supplied
Bevy `WorldAsset` scenes. Species entries take precedence over the generic animal
entry. Register appearances in startup after the render plugin initializes the
resource, for example:

```rust
use bevy::{gltf::GltfAssetLabel, prelude::*};
use terra_bevy::render::{ActorSceneAppearance, ActorSceneOverrides};
use terra_core::ActorKind;

fn install_models(server: Res<AssetServer>, mut models: ResMut<ActorSceneOverrides>) {
    let mut human = ActorSceneAppearance::new(
        server.load(GltfAssetLabel::Scene(0).from_asset("models/human.glb")),
    );
    human.scale = 1.0;
    human.ground_offset_m = 0.0;
    human.yaw_offset_degrees = 0.0; // reference forward direction is local -Z
    models.set_kind(ActorKind::Human, human).expect("valid appearance");
}
```

The path names an asset supplied by your game; no production models are bundled
or downloaded by this API. Use `set_species`, `remove_kind` and `remove_species`
for more specific appearances and removal. The demo attaches `ActorSceneAnchor`
and `ProceduralActorMesh` to its living body visuals; another renderer can attach
those components to its own body roots to opt in. Root transforms and visibility
continue to follow geographic position, heading, first-person hiding and layers.

Loading, failed or meshless assets retain the procedural fallback. A ready
instance replaces it after `WorldInstanceReady`; removing an override restores
it and removes the old scene hierarchy. `ActorSceneInstance.body_id` identifies
the model child for a game's animation/equipment observer. Animation graphs,
clips, sockets, model licensing and GPU budgets remain game responsibilities;
changing a model does not change movement, collision or multiplayer authority.

### Validated animated packs

For a bounded authoring handoff, construct `ValidatedAssetPack::read(path)` and
add `AssetPackPlugin::new(pack)` **before** `DefaultPlugins` or `AssetPlugin`.
Bevy freezes the source registry when its asset plugin is built. The pack plugin
registers a private immutable in-memory source and installs its actor bindings
at startup. The native demo uses this same public path for its five original
animated wildlife models; `render::bundled_wildlife_pack()` exposes that example.

Preflight checks content hashes, the supported core-glTF profile, geometry,
hierarchy, skinning, every animation sampler and actual bounded PNG decoding.
Undeclared extension payloads are rejected too. Runtime clips use actual motion
and the full virtual-time delta, with pause-safe transitions. The procedural
actor remains until model dependencies and the named hierarchy are ready.
This route preserves character customization and world authority. It does not
add humanoid retargeting, animation graphs, equipment sockets or unrestricted
glTF support. Authoring steps, budgets and a validator command are in
[ASSET_PACKS.md](ASSET_PACKS.md).

## Add the living simulation

`TerraLivingPlugin` is opt-in for another game and is installed by the demo.
It consumes the existing `SandboxRuntime` and `ResidentChunks`; add it after
`TerraWorldPlugin`. The domain remains independent of Bevy.

```rust
use bevy::prelude::*;
use terra_bevy::{TerraLivingPlugin, TerraWorldPlugin, WorldConfig};
use terra_bevy::living::LivingSettings;

fn living_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(TerraWorldPlugin::new(WorldConfig::default()))
        .insert_resource(LivingSettings {
            seed_demo: true,
            simulate: true,
            collision: true,
        })
        .add_plugins(TerraLivingPlugin);
    app
}
```

`seed_demo` creates one authored settlement once sufficient nearby terrain is
resident, choosing suitable map features for facilities and roads where present.
It does not generate a historical population or a settlement for every chunk.
Set `seed_demo: false` before adding the plugin to install game-owned validated
living state instead. `simulate` controls local advancement; `collision` controls
the adapter's additional footprint/obstacle policy. A host that disables checks
must provide the intended replacement policy.

`SandboxRuntime.state.living` holds the authoritative bodies, accounts, shops,
production, routes and courier cargo. A player principal keeps one shared
inventory/wallet while its owned bodies have individual positions and needs.
`LivingAccount.active_unit` supplies the principal's physical interaction context.
`possessed` decides whether direct input controls that body. UI selection or a
camera mode must not itself rewrite ownership or inventory.

Submit the same commands from an RTS interface, a controller or an AI system:

```rust
use bevy::prelude::*;
use terra_bevy::{SandboxIntent, SandboxQueue};
use terra_bevy::rules::{Command, LivingCommand};

fn possess_owned_body(mut queue: ResMut<SandboxQueue>) {
    // Obtain this ID from the player's validated owned-unit selection.
    let selected_unit_id = 50;
    queue.0.push_back(SandboxIntent::Command(Command::Living {
        action: LivingCommand::Possess { unit_id: selected_unit_id },
    }));
}
```

The example ID is illustrative; ownership is checked, and an invalid ID is
rejected. `SelectActive` changes RTS interaction context without cancelling the
body's order. `MoveUnits`, `StopUnits` and `Rest` validate an entire owned group
before changing it. RTS air destinations are horizontal terrain orders with
cruise clearance, while manual air input may include vertical velocity. `Release`
retains the active body. `TradeQuoted`, `Buy`, `Sell`, `Consume` and
`SetProduction` use that same context for reach and permissions.

For displayed personal trades, call
`SandboxState::personal_trade_quote(player, shop_id, item, amount, buy, catalog)`
to obtain the validated `PersonalTradeQuote`, retain its total with the rendered
seller/quantity/player/restore context, then enqueue
`LivingCommand::TradeQuoted { shop_id, item, amount, buy, quoted_total }`.
Authority rechecks the exact total, stock, funds, receiving limits and reach
before mutation. A changed quote rejects atomically and requires review; it is
not a price reservation. Legacy `Buy` and `Sell` remain available for embedders
that deliberately accept the current authority price at execution.

After a local possession is accepted, populate `LivingMotion` with host-generated
ENU velocity and heading. Its `active` flag enables direct motion for the
possessed body; the adapter clamps speed after current needs are advanced and
resolves motion against resident surfaces. In network mode send velocity intent
through the networking adapter instead; the client does not run local living
advancement or install speculative positions. The demo shows both paths in
`crates/terra-demo/src/camera.rs` and `control.rs`.

`seed_population` runs after `WorldSet::Stream` and before `WorldSet::Sandbox`.
`advance_population` runs in `FixedUpdate` after `WorldSet::Simulate`.
`LivingDiagnostics` exposes seeding status, population, moving bodies, rejected
steps, simulation time and the latest domain error. Domain-only hosts use
`seed_living_demo`, `ensure_living_player` and
`advance_living_with_environment(real_dt, simulation_dt, catalog, resolver)`.
The resolver must validate a step before returning its committed position;
delivery arrival checks happen after that resolution. A rejected movement must
not be treated as an arrival.

See [LIVING_WORLD.md](LIVING_WORLD.md) for prices, needs, production recipes,
finite-stock logistics, snapshot privacy and exact reference limits. Customize
those rules as game content; do not treat authored shops or NPCs as factual map
observations.

## Gameplay and content catalogs

### Ecology and settlement extension points

`SandboxState.ecology` owns habitat/species metadata and authored natural-resource references; animals themselves are `LivingUnit` bodies. `SandboxState.settlements` owns named claims, public stocks/treasuries, funded contracts, price anchors and player progression. These are serializable domain layers, independent of the renderer. Their default configuration is bounded regional gameplay, not a scientific census or global persistence service.

The installed `TerraLivingPlugin` adds idempotent nearby ecology after population seeding, including earlier living saves without ecology. Loaded source chunks register eligible habitat/resource definitions. During each authoritative fixed update it schedules ecology decisions before living movement/production, then settlement maintenance afterward. Physical decision cadence and movement use real step time; needs, population recovery, contracts and economy use calendar time. `LivingDiagnostics` reports last/peak combined step time, fixed-step count and copied ecology-work counters; those values are local samples, not replicated server timings.

`EcologyRuntime` owns the transient `EcologyScheduler`. Its default policy uses
a 160 m near-observer radius, two physical seconds between distant reconsiderations
and sixteen far decisions per behavior pass. Nearby or threatened wildlife takes
priority. The normal ecology behavior interval remains one second by default;
this does not turn decision-making into a 20 Hz process. Physical movement,
collision, needs, resource recovery, production and cargo arrivals continue on
their normal fixed steps. Far work rotates fairly rather than accumulating a
later movement jump.

A custom domain host can compose the same lifecycle:

```rust,ignore
use terra_sandbox::EcologyScheduler;

// Keep this in host runtime state, outside the update loop.
let mut ecology_scheduler = EcologyScheduler::default();

// On source registration / initial population setup:
state.register_chunk(&chunk);
state.resource_nodes.retain(|_, node| catalog.items.contains(&node.item));
state.register_ecology_chunk(&chunk, &catalog)?;
state.seed_ecology_near_with_environment(origin, &catalog, admit_ground)?;

// On each authoritative fixed update:
let observers: Vec<_> = state.living.units.values()
    .filter(|unit| unit.owner.is_some())
    .map(|unit| unit.position)
    .collect();
state.advance(sim_dt, &weather)?;
state.advance_ecology_scheduled_with_environment(
    real_dt, sim_dt, local_hour,
    &mut ecology_scheduler, &observers, admit_ground,
)?;
state.advance_living_with_environment(real_dt, sim_dt, &catalog, resolve_motion)?;
state.advance_settlements(sim_dt, &catalog)?;
```

The snippet is schematic: reuse the host's closures by reference or create each closure as appropriate. Supply a validated `WeatherSample`; base `advance` owns crop growth and resource-node regrowth, so ecology advancement alone does not replenish renewable patches. `admit_ground` must return a grounded point only when the relevant surface is resident, dry, within slope policy and clear of source/player/facility footprints. Resolve terrain height before applying altitude-sensitive placement checks. `resolve_motion` must still validate every accepted step; a valid habitat is not permission to walk through a building.

Observers must come from trusted owned-body state. The local Bevy host uses all
owned bodies; a network host should restrict them to connected principals, as
the reference server does. A spectator camera is not an authority source for
near-tier decisions. Existing player positions still define active regions, and
indexed threat checks allow a nearby person or vehicle to bypass the far budget.
Owned animals retain their game/player orders.

Call `ecology_scheduler.reset()` whenever a host replaces or loads a world, even
if its saved calendar matches the old world. Clock rewind also resets it
automatically. The Bevy load path resets both the scheduler and copied
`LivingDiagnostics` counters; the server resets its scheduler on restore.
Do **not** reset for a floating render-origin rebase: geographic positions and
the fairness cursor remain valid. The legacy
`advance_ecology_with_environment` remains available as a full-pass option for
hosts managing their own decision policy. Do not run both advancement methods
for the same step. See [SCALING.md](SCALING.md) for configuration validation,
latency bounds, counters and the finite probe.

Configure `EcologyConfig` before registration/seeding. Species, resources, capacities, activity hours and recovery intervals are data; unsupported custom-catalog goods or unavailable tools do not create impossible harvest tasks. `seed_demo: false` lets a game install its own population. Setting `ecology.config.enabled = false` disables new ecological registration/advancement; it does not silently delete saved bodies or stock. Configure contract duration through `SettlementWorld.contract_duration_seconds`. Tier balances are reference policy in the settlement module, while its readiness helpers keep UI requirements aligned with authority.

Submit `Command::Settlement { action: SettlementCommand::... }` for founding, contribution, funding, upgrade and contract actions. These use the same principal, active body, queue, replay rules and inventory as existing commands. `check_settlement_land`, `check_settlement_interact`, `settlement_plot_faction` and `check_settlement_transfer` are the reusable permission gates for host-specific extensions. Current membership determines access; retaining an original creator ID is not a bypass.

The renderer exposes `WorldDetailSettings` for ground cover, natural resource models and settlement boundaries. Wildlife poses are bounded cached meshes driven by authoritative behavior/velocity, and resource models reflect real remaining stock. A game's production renderer may consume the same data without adopting those meshes.

Read [ECOLOGY.md](ECOLOGY.md), [SETTLEMENTS_AND_ECONOMY.md](SETTLEMENTS_AND_ECONOMY.md) and [MMORPG_FOUNDATION.md](MMORPG_FOUNDATION.md) for limits, rules and production extension work.

Queue a `SandboxIntent::Command` through `SandboxQueue` from the UI or other game systems. In local mode the Bevy adapter validates loaded physical support, water, slope and source/authored occupied footprints for plots, structures and crops, then delegates inventory, reach, owned-overlay overlap and permissions to `terra-sandbox`. `sandbox::placement_error` is shared with the demo preview so feedback and command admission agree. In network mode intents go to the server and mutate no local authoritative inventory.

The rules crate does not fetch external terrain or query a host physics world. It provides bounded loaded-graph routing and body-contact admission; hosts still supply environmental placement and movement checks. Hosts using `SandboxState::apply` directly must apply their own environmental placement checks. Definition consistency and ownership are validated when restoring state with a catalog. Register players with the built-in starter inventory or the custom-inventory API when using a custom catalog. See [GAMEPLAY.md](GAMEPLAY.md) and the [catalog example](../crates/terra-sandbox/catalog.example.json).

Source geometry is an immutable layer. Player structures, crops, claims, inventories and resource edits are a separate mutable overlay. Stable source IDs keep gathered trees from reappearing on chunk reload. Whole sourced buildings/roads/water are not destructible through the bundled command API: clipped source geometry cannot establish ownership of the complete upstream object. Player-built structures have hit points and can be destroyed by permitted owners/builders. Arbitrary terrain digging and physical fracture are future host-specific systems.

Faction membership controls actual plots, houses, farms and resources. A political country's polygon is source information; it never automatically grants build permissions or claims political sovereignty for a gameplay faction. Authored scenario territories may carry a faction ID as a link. See [HISTORY.md](HISTORY.md).

## Reuse geographic interest work

`GeoSpatialIndex<K>` provides a conservative geographic broad phase followed by
exact surface-distance checks. It handles date-line and polar queries and
ignores altitude for interest selection; reach, collision and ownership retain
their separate rules. Traffic and wildlife threat queries use this surface in
the domain implementation.

For multiple recipient snapshots, build a `SandboxInterestIndex` once after all
world updates and reuse it throughout that immutable broadcast:

```rust,ignore
use terra_sandbox::SandboxInterestIndex;

let index = SandboxInterestIndex::new(&state)?;
for player_id in connected_players {
    let view = state.snapshot_for_indexed(player_id, interest_radius_m, &index)?;
    // Encode through the authoritative host's per-recipient replication path.
}
```

The index borrows the source state; it cannot coexist with mutation of that state
while still in use, and an index from another world is rejected. Rebuild after
movement, commands, removal or world replacement. Never persist it as gameplay
state. `LivingInterestIndex` and `LivingWorld::snapshot_indexed` expose the
standalone living equivalent. Original scan APIs remain available.

Indexed views preserve recipient privacy, distant owned bodies, deterministic
ordering, truncation and byte budgets. They do not make partial client replicas
into full writable worlds. Building an index for one small view can cost more
than scanning; the benefit is reusing selection across a populated state and
multiple recipients. [SCALING.md](SCALING.md) includes the reproducible snapshot
comparison and distinguishes its separate 100,000-point index experiment from
fully simulated population capacity.

## Persistence and networking

The desktop `LocalWorldSave` envelope stores the overlay, factions, player identity, living units/needs/orders, wallets, shops, production/cargo, simulation clock and edited politics. It validates world revision, seed, terrain level/resolution, frozen data epoch and political base ID before restoring. A save from another era is rejected without changing the current world. Writes use a same-directory temporary file and atomic replacement. The pure `SandboxState` JSON, desktop envelope and server save are distinct formats; they are not interchangeable.

For a direct `--save` session, F5/F9 explicitly save and restore; `save_path` is a destination rather than an autoload policy. Managed portable-JSON worlds created in the launcher additionally autoload their saved state and save on clean exit. Setting `WorldConfig.region_store` opts into database resume, automatic checkpoints, F5 checkpoint and F9 restore. `RegionalPersistence` reports residency, generation, latency and errors; source registration and commands hydrate before complete-world lookups, then commit and retire at a batch boundary. Dormant capacity remains reserved. Whole-world save envelopes reject partial active states; `RegionStore::export` reconstructs a complete save. See [REGIONAL_PERSISTENCE.md](REGIONAL_PERSISTENCE.md) for the exact global/dormant policy, APIs, startup/exit ordering and backup/recovery. Network clients leave persistence to the host.

A successful explicit local restore retires the remaining `SandboxQueue` intents
prepared against the previous world, including quoted trades and control changes.
The native demo also retires its older UI/control action backlogs and pending
interaction, drag, order and placement state when the player or restore serial
changes. Initial startup and ordinary state revisions retain input. Review the
restored player and offers before issuing new actions. A failed restore does not
invoke this barrier; regional persistence failures retain their existing world-pause
and recovery policy. Network sequence/replay handling is independent.

`TerraWorldPlugin` installs the regional adapter automatically; configure the
path before adding the plugin. No renderer or additional persistence plugin is
required. `RegionalPersistence::export_world` returns a complete validated
`LocalWorldSave`, including uncheckpointed active changes; call
`check_output_path` before writing it. `RegionalPersistence::backup_to` creates a new SQLite backup
of the latest committed state, including WAL contents, with bounded copy retries,
complete copied-snapshot validation and atomic publication.
`terra_persistence::RegionStore::backup_with_options` adds a receipt for the exact
copied generation; `terra_persistence::RegionStore::open_read_only` is the
nonmutating maintenance entry point. The local CLI has no
automatic JSON-to-database conversion; existing JSON worlds can continue in
explicit F5/F9 mode or be migrated explicitly with `terra-worldctl import-local`.
Ordinary hydration and checkpoints use one bounded `RegionWorker` job. Main
thread submission clones the bounded state, polling never waits for disk, and
authority pauses until the validated result installs. Host mutation success is
released only after its commit. Startup, explicit restore/export and final flush
remain deliberately blocking boundaries. See [OPERATIONS.md](OPERATIONS.md)
for verification, backup and migration commands.

Source packs and the current simulation date are separate concepts. A scenario can start from a 1950 dataset and then simulate decades of player changes. Advancing its clock does not quietly replace buildings with modern source data. Switching the frozen source epoch requires an explicit world restart/reload with an appropriate pack and overlay policy.

Multiplayer uses an authoritative reference server, geographic interest, ordered commands, movement validation, replay rejection and bounded framed TCP. The reference network uses **wire protocol 11**; use matching host and client executables, since equal protocol numbers alone do not certify arbitrary mixed versions. The host shares a principal-actor index per actor broadcast and a sandbox interest index per sandbox broadcast. Mutable sandbox interest follows the player's active authoritative body rather than a free camera.

The actor and sandbox streams have independent revision baselines. New sessions
receive keyframes; ordinary updates use entity deltas when those encode smaller
than a full view. Periodic keyframes and a bounded resync request recover missing
baselines. Reconstruction validates counts, identities, domain references and
the 240 KiB complete sandbox-view budget before committing a new baseline; the
wire frame ceiling remains 256 KiB. Removal from a recipient's interest set is a
replica change, not an authoritative deletion command.

`NetClient::poll()` reconstructs wire updates into the existing
`ServerMessage::Snapshot` and `SandboxSnapshot` variants. Keep application-level
manifest and gameplay validation on that path; do not apply raw `ActorUpdate` or
`SandboxUpdate` messages directly to game state. The current protocol also returns private
`MotionState` snapshots. A custom client can feed those and its successfully sent
input frames into `terra_net::prediction::ClientPrediction`; the result belongs
only to the owned body's display pose. It never replaces inventory, commands,
collision authority or a save. See [CLIENT_PREDICTION.md](CLIENT_PREDICTION.md)
for sequencing, stale-input and terrain-admission requirements.
`replication_stats()` measures
encoded payloads and equivalent complete-view bytes, `transport_stats()` reports
actual socket traffic, and `resync_pending()` exposes baseline recovery. These
are distinct measurements; payload reduction is not a CPU-speed claim.

Matching manifests prevent a client from joining with a different seed,
historical era or political revision. Native remote visuals interpolate
authoritative positions and headings; the owned possessed display also uses
bounded input replay from completed-step acknowledgements. The optional TLS
transport uses verified CA/hostname checks and
exact `terra-world/11` ALPN, with no plaintext fallback. Use `Server::bind_tls`,
`NetClient::connect_tls`, or `TerraNetworkPlugin::with_tls`; see
[NETWORK_REPLICATION.md](NETWORK_REPLICATION.md) for the exact configuration.
Non-loopback plaintext requires an explicit opt-in. Bounded owner prediction/reconciliation is display-only. There is no public account service or cross-process shard coordination. An Internet game still needs authentication, abuse controls and
operational deployment. Never put resume capability files in source control.

See [MULTIPLAYER.md](MULTIPLAYER.md) for startup, session identities, save/restart
and host integration, and [NETWORK_REPLICATION.md](NETWORK_REPLICATION.md) for
the delta/resync contract and recorded sixteen-client loopback workload. Its
4,096 generic kinematic records are a simpler representation than full living
actors; the report does not establish MMORPG concurrency. Use
[PERFORMANCE.md](PERFORMANCE.md) to read local/native timings alongside those
network counters.

### Headless source-surface policy

`terra_net::ResidentSurface` is a reusable, bounded memory-only surface adapter
for the reference host. Construct it with the tile level, then forward validated
`register_chunk` and `evict_chunk` events. It exposes terrain height, slope/water
samples, source-building footprints, placement admission and movement resolution.
It retains at most 512 chunks. Unknown resident data rejects movement/placement
rather than guessing a clear path.

`terra_net::WorldEnvironment` provides default `register_chunk`, `evict_chunk`,
`resolve_movement(kind, from, candidate)` and
`allows_placement(position, radius_m, footprint)` hooks. Defaults preserve the
simple terrain/coverage-only library contract. The reference CLI overrides them
with `ResidentSurface`: ground actors reject water and steep slopes, road bridges
supply physical decks, source footprints are swept, ships require water and air
bodies retain terrain clearance. Dynamic authored structures/facilities are
checked separately by the server. These conservative policies are useful for a
shared demo, but a host still owns full collision volumes, navigation and physics.

Model-sized vehicle sweeps call
`WorldEnvironment::vehicle_support_with_clearance(position, body_height_m)`.
Its default delegates to the existing `vehicle_support(position)`, preserving
custom providers' earlier support policy. A provider with roof/underpass
colliders should override the height-aware method. `ResidentSurface` does so
using the model height and terrain-relative source `min_height`; a generic
point-height result alone does not establish headroom. See
[CONTACTS_AND_ROUTING.md](CONTACTS_AND_ROUTING.md).

## Embed driving and the native session flow

Transport rules are renderer-independent and re-exported at crate root. Import
`terra_sandbox::{TransportCommand, CompanyCommand, TransportWorld}` (or the
corresponding `terra_bevy::rules` types). Call
`SandboxState::ensure_transport_demo(&catalog)` after creating the living world,
or author bounded `TransportWorld` records for your game. Commands enter through
`Command::Transport { action: TransportCommand::… }`; the same atomic rules
handle native, host and test clients. `set_character_profile` accepts a validated
name and bounded appearance palette before population is seeded. See
[TRANSPORT.md](TRANSPORT.md) for the complete contracts and conservation rules.

Terrain-backed hosts must hydrate complete regional command dependencies before
vehicle placement, then use `SandboxState::exit_vehicle_with_ground` for
dismount and `SandboxState::deliver_vehicle_with_ground` for `BuyVehicle` or
`RentVehicle`. Their callbacks provide actual resident terrain/source support;
they must reject missing, wet, too-steep or obstructed samples without shifting
horizontal coordinates. The shared planners admit complete footprints and
occupancy before the transaction. Plain domain `apply` retains its explicit
level-ground convenience assumption. The supplied native and network adapters
already route these commands through the grounded APIs.

Feed normalized `terra_core::DriveInput` to the authority. `step_vehicle` handles
the shared kinematic state; `drive_actor` integrates a candidate geographic pose.
Your adapter must validate terrain coverage and collision before committing that
pose, then call `record_vehicle_motion` once to charge fuel, mileage and wear.
The supplied Bevy and host adapters implement this order. Use
`SandboxState::vehicle_speed_limit_m_s` to include licenses, mechanical condition
and the present driver's needs. Do not charge manual motion again in the normal
autonomous simulation step. No input must produce a safe brake intent, not a
frozen moving body. Unknown support coverage must not produce collision damage.

The native `terra-demo` owns menu/navigation, character preview and presentation;
those policies do not leak into the reusable domain. Running without arguments
opens the menu. Explicit simulation flags retain direct launch. Character and
world profiles, source selection, resume destinations, TLS settings and
save-to-menu lifecycle are described in [GAME_FLOW.md](GAME_FLOW.md). This flow
can serve as a reference when a host game supplies its own UI.

## Production expansion order

1. Select a regional vertical slice and pin licensed source revisions, coordinate/height transformations and update policies.
2. Build an offline ingestion pipeline and host immutable tiles on your own object store/CDN. Public demo APIs are not a global game backend.
3. Extend the existing terrain/feature mesh LOD and quality profiles with adaptive source levels or clipmaps, complete collision volumes, navigation, physics and production assets.
4. Build public account enrollment/recovery and abuse controls on the verified TLS boundary. Trusted principal suspension, capability rotation and resume-only admission are implemented; extend the bounded owner prediction and remote interpolation with realistic latency/loss testing and game-specific combat or physics reconciliation.
5. Extend the bounded storage worker beyond its current immutable environmental dependency certificates with migration policies and measured transaction budgets. Current global ledgers and dependent cold bodies still validate together. Disconnected-owner dormancy and finite multi-client churn probes are implemented; measure regional authority handoff and sustained representative residency before lifting reference capacity limits. Existing worldctl backup/verify/import/export tools cover one authority.
6. Add specialized historical, climate, vegetation, agriculture, traffic or social models as evidence-backed layers with explicit uncertainty and provenance.

The implementation is a tested foundation and a working native example. It is not a complete scientific digital twin, a planet-scale production service, or a finished survival game.


## Current integration contracts

Company actions enter through `Command::Transport { action:
TransportCommand::Company { action: CompanyCommand::... } }`. The domain owns
cash, wages, assignment, finite stock/queue reservation, public fare or private
shop-purchase escrow, reserved private arrival/refund space, physical delivery
proof and current service/retail quotes. Read the owner's `TransportView.company` for private
management; public vehicle appearance contains no company ledger. UI selection
is presentation state and is never authorization. See [COMPANIES.md](COMPANIES.md)
for each action, bound and recovery rule.

For supplied living bodies, `LivingContacts` provides the shared spatial broad
phase and core sweep admission. Local manual control, the reference host and
fixed-step autonomy use it after terrain/source checks. A custom physics engine
may replace the reference stop/yield policy, but it must retain authority and
source-coverage checks. `vehicle_route_plan` exposes a bounded directed plan and distinguishes connected,
permitted off-road and unavailable routes. One-way/access restrictions must not
fall back to a direct shortcut. `pedestrian_route_between` remains a separate
reference path. Persisted physical-time intersection admission coordinates
automatic road traffic; manual driving has final terrain/body admission and
advisory road law. These are not surveyed sidewalks or a complete traffic code. See
[CONTACTS_AND_ROUTING.md](CONTACTS_AND_ROUTING.md).

The host calls `set_player_dormant(player, true)` for offline principals and
hydrates all required dependencies before reactivation. An application embedding
`SandboxState` directly must provide that lifecycle deliberately: a retained
resume capability is not evidence of an active socket. Dormancy freezes owned
needs/orders, transport commitments and payroll rather than simulating offline
catch-up. Complete world exports still require the regional store; a partial
resident view must never be serialized as a complete authority save.

`ValidatedRegionPartition` is an immutable, catalog-pinned certificate for a
checked cold partition. Dependency-closed unchanged environmental groups can
reuse certified validation, while current global state and actual dependent cold
records are validated together. Untrusted restore validates the complete world.
One worker, one outstanding snapshot and revision/write
barriers remain. Do not infer distributed ownership from region partitioning.

The demo's keyboard/gamepad editor and native process handoff are client policy.
They send the same bounded motion/command intents as the default bindings and do
not add a network privilege. Games may replace the entire input, camera and UI
layer without changing company, persistence or transport rules.
