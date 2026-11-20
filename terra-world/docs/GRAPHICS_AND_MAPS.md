# Terrain, map semantics and the reference renderer

Terra provides bounded atmospheric lighting, material detail and terrain level of
detail, introduced in 0.4, while keeping the renderer and data providers separate.
The native default requests a 7 × 7 neighborhood of level-15
geographic chunks and uses 65 × 65 elevation samples per tile. Live data should
use a smaller request radius or prepared regional packs: increasing view range
must not turn a public endpoint into a bulk-download backend.

## Height fields and materials

Terrain is built from the actual height samples, with shared indexed vertices,
continuous bilinear queries, smooth derivative normals, and generated tangents.
Near terrain uses the same source elevation samples that actors query. Distant
render interiors use the reduction described below while every border retains
its original samples; collision always reads the full source heightfield.
Generated 256 × 256 albedo and normal maps now include complete mip chains and
anisotropic filtering for close views without distant shimmer. Continuous
earth-centered noise provides macro color variation; relief exposes rock,
altitude/latitude determine illustrative snow, and source land use supplies
forest/field/grass tint. These materials are authored presentation, not satellite
imagery, measured soil properties, or climate evidence.

Forest, field and grass polygons affect the height-field material instead of
placing a flat triangulated cover across hills. Forest trees sample that terrain;
field rows are subdivided and grounded. Water areas are flat surfaces with their
recorded `ele` value when available, otherwise an inferred boundary elevation.
The offline pond has an authored basin in the synthetic elevation function.
Water has a cached mipmapped normal texture with a slow physical-time UV drift
and sky reflections when atmospheric environment lighting is enabled. These are
visual ripples; water does not simulate fluid volume, erosion or flood dynamics.

The updated procedural world includes connected roads, distinct small buildings,
market and drinking-water POIs, fields, orchard and forest. It has revision
`terra-procedural-v2`, new stable feature IDs, and explicit synthetic provenance.
Old base revisions are not silently treated as identical worlds.

## Imported PNG heightmaps

`terra_stream::HeightmapProvider::open(manifest_path, tile_resolution)` implements
`ChunkProvider`; it can be installed through `TerraWorldPlugin::with_provider` or
the demo's `--heightmap` path. The example is
`crates/terra-stream/data/heightmap-demo/manifest.json`.

The version-1 JSON manifest declares:

```json
{
  "schema_version": 1,
  "image": "height.png",
  "bounds": { "south": 48.12, "west": 11.54, "north": 48.16, "east": 11.61 },
  "min_height_m": 0,
  "max_height_m": 1500,
  "revision": "my-immutable-heightmap-v1",
  "source": {
    "provider": "My dataset",
    "license": "Explicit source license",
    "attribution": "Required dataset attribution",
    "url": "https://example.invalid/my-dataset",
    "retrieved_unix_seconds": 0,
    "synthetic": true
  }
}
```

The importer accepts 8-bit or 16-bit grayscale PNG, with north at the top. Black
maps to the minimum height and white to the maximum; values are bilinearly
resampled to each requested geographic tile. The example above shows the schema;
choose an extent containing the full requested tiles. Incomplete tile coverage
returns an explicit error. Dateline-crossing rasters need separate extents.

Input is bounded: manifest 64 KiB, encoded PNG 32 MiB, maximum dimensions 4096 ×
4096, decoder allocation limit 128 MiB. Image paths must remain inside the
manifest directory. Color imagery, missing provenance and invalid ranges are
rejected. Heights retain the declared source datum; no geoid conversion,
NoData inference or semantic extraction is performed.

The included 257 × 257 fixture is synthetic and contains no downloaded imagery.
Its generators are provided for reproducibility. An optional `features` member
adds a contained, independently attributed GeoJSON source. The new `manifest-v2.json`
contains synthetic woodland, fields, an orchard and trees; the original manifest
remains unchanged. New launcher worlds use the adjacent west/east set derived
from those same authored assets, with an exactly shared elevation seam.
`PreparedHeightmapProvider` accepts both single manifests and bounded connected
sets. See [SOURCE_BUNDLES.md](SOURCE_BUNDLES.md) for the complete
feature contract, budgets, content identity and native/host workflow.

`terra-sources inspect/export/verify/install` now provides a bounded offline
handoff for those prepared files. Exported directories preserve the original
source identity and relative manifest paths, carry every file hash plus declared
extent/provenance, and report complete/missing tiles at an explicit level.
Verification runs the real raster/semantic/seam importer again; installation
requires a new destination. The authored connected valley is the working
roundtrip example in [SOURCE_BUNDLES.md](SOURCE_BUNDLES.md). Dense grayscale
NoData masks, automatic source downloads and global licensed coverage remain
outside this adapter.

## OpenStreetMap attributes used

| Source data | Applied behavior |
|---|---|
| `height`, `building:levels`, `min_height` | Building vertical dimensions, with explicitly marked defaults when absent |
| `roof:shape` | Flat roofs; gabled, hipped, pyramidal or skillion treatment on four-corner footprints |
| `roof:height`, `roof:colour`, `roof:material` | Roof rise and material palette |
| `building:colour`, `building:material` | Facade palette; windows and trim are authored illustrative detail |
| `shop`, `amenity`, `craft`, `tourism`, `name` | Preserved functional metadata and point markers; available to the living-world adapter |
| `highway`, `width`, `lanes`, `surface` | Bounded road widths, paved/unpaved appearance, shoulders and lane marks |
| `bridge`, `layer` | Illustrative raised deck, rails and supports using a shared movement-support query |
| `natural=water`, `waterway` | Closed lakes/ponds and distinct linear river/stream/canal ribbons |
| `landuse`, `crop`, `leaf_type` | Forest, meadow, field and orchard presentation, and tree variants |
| Remaining source tags | Preserved in `WorldFeature.properties` for game-specific systems |

Lengths support metres, centimetres and explicitly suffixed feet. A traffic
restriction such as `maxwidth` is not treated as the physical road width. The
provider keeps inferred width/height labels separate from measured tags.
Building footprints describe physical form; an active shop requires a functional
`shop`/`amenity` tag rather than inferring commerce from a building type.

Sources for tag interpretation:

- https://wiki.openstreetmap.org/wiki/Key:building
- https://wiki.openstreetmap.org/wiki/Key:roof:shape
- https://wiki.openstreetmap.org/wiki/Key:width

The importer still does not reconstruct multipolygon relations, every possible
roof type, underground floors, building interiors, or a complete routable OSM
transport graph. Named POIs do not establish that a real business is currently
open, and simulated stock, workers and transactions are authored gameplay data.

## Render and simulation budgets

Chunk mesh generation stays on at most two background jobs; main-thread mesh
integration uses the configured budget. Source geometry is batched by layer.
Feature distance tiers use distance to tile bounds, keeping a nearby tile edge
detailed even when its center is farther away. The initial thresholds are 550
and 1500 metres; hysteresis uses 450–650 and 1300–1700 metre bands. Pending mesh
jobs retain their target tier through those bands, avoiding repeated cancellation
while the camera moves. Changed tiers rebuild through the same two-worker queue;
stale results are discarded before replacing visible geometry. Each mesh job also
retains its actual immutable source page. Completion requires the same resident
source instance, so an evicted/reloaded tile cannot accept geometry from its
previous page; the replacement is queued through the same bounded worker policy.

| Detail | Trees per chunk | Building windows | Agricultural rows | Road detail |
|---|---:|---:|---:|---|
| Near | 160, full crowns | Up to 144 per building | Up to 900 segments | 8 m subdivisions, shoulders, lane marks, bridge rails |
| Middle | 48, simplified crowns | Up to 12 per building | Up to 120 segments | 16 m subdivisions and reduced trim |
| Far | 24, silhouettes | None | Land-use material only | 30 m subdivisions, road/bridge surface |

With the default terrain LOD setting, near meshes keep the full source grid;
middle/far mesh interiors use sample strides of two/four. Every outer edge still
keeps every original source sample. Coarse cells fan around an original center
sample and retain fine border segments, so neighboring tiers have matching
geometry without skirts or hidden cracks. Non-power-of-two grid tails are
merged to avoid T-junctions. The High profile keeps middle terrain at full detail
and uses stride two only for distant interiors. Disabling `terrain_lod` restores
the full render grid at every tier. The underlying resident 65 × 65 heightfield
remains untouched: distance detail never changes collision, height queries or
source semantics. Road subdivision is
also capped at 512 segments per feature; individual layer builders stop accepting
features after their vertex budget. Mapped tree nodes reserve part of the tree
allowance; remaining samples are shared among forest and orchard polygons rather
than all being consumed by the first polygon. Orchard trees use planted rows;
recorded leaf types and grain crops affect illustrative geometry and colors.
Shared actor archetype/variant meshes avoid
per-actor asset creation. An executable regression builds the default 49-chunk
source neighborhood and requires fewer than two million vertices, including
terrain. This is a bounded reference budget, not a planet-scale performance or
frame-rate claim.

`render::RenderDiagnostics` (an alias of `RenderTelemetry`) exposes integrated
source `chunks` and `vertices`, plus `pending_meshes` and `building_meshes`. It also reports actual
`terrain_vertices`, corresponding full-resolution `terrain_full_vertices`, and
`lod_chunks` in near/middle/far order.
Capture tools can wait for every resident chunk to be integrated and both queues
to empty. The vertex count includes source terrain/features; independently cached
living actors and player-built structures have separate presentation budgets.

Validated actor packs additionally use aggregate scene admission through
`ActorSceneBudget`. Quality presets bound instances, expanded geometry,
primitives, animated nodes/channels, retained texture-slot pixels and estimated
payload. Selection favors the visible active body, owned bodies and stable
nearby distance bands. Deferred actors keep the procedural mesh. Lazy loads and
outgoing model dependencies remain charged through retirement, so camera
movement cannot continually acquire new uncharged models. Diagnostics expose
selected and reserved totals, deferred bodies and transition peaks. These
counts/byte estimates are not measured GPU time or actual allocator memory;
see [ASSET_PACKS.md](ASSET_PACKS.md) for exact presets, formulas and scope.

All eight actor kinds now have distinct authored silhouettes. Human heads and
limbs, animal bodies, wheels, cabins, aircraft and balloons use rounded bounded
geometry. Market stalls and production sites have separate reusable meshes.
Material colors are converted from authored sRGB values to the linear vertex
colors expected by Bevy's PBR pipeline. Fog distance follows the resident span
and weather; sun, ambient light and vegetation tint retain day/night and season
configuration. Detailed animation rigs, photographic assets and physically
accurate vehicle dynamics are separate extension points.


## Wildlife and resource presentation in 0.3

Wildlife visuals use the authoritative `EcologyWorld.animals` metadata and the
existing `LivingUnit` position/velocity. Deer, boar, hare, fox and ibex have
separate silhouettes, proportions, ears, tails and species features. A shared
palette contains eight locomotion samples plus standing, grazing, resting and
drinking poses. The native renderer prepares all twelve poses for each visible
species once and switches mesh handles while animating. It does not generate
new geometry per animal per frame. Pose timing uses physical time and freezes
with the world pause state. First-person possession still hides the controlled
body. These are bounded authored animation samples, not skeletal motion capture
or a full animation-graph system.

Natural-resource visuals follow actual `ResourceNode.remaining` and `capacity`.
Timber piles, stone/iron outcrops, berry bushes, flax, freshwater sources and
fishing spots use full, reduced and depleted stages. A depleted resource keeps a
stump, bare plant or exhausted outcrop so the player can recognize the location.
Normal quantity updates do not rebuild geometry until a stage boundary changes.
The renderer shows only resident terrain and at most 128 nearby resource nodes.
The geometry palette is bounded to seven types, three stages and three variants.
These resource models are illustrative game content; a simulated iron deposit
is not a geological survey assertion.

Courier trucks now have a separate bed silhouette. Cargo meshes appear only
while the authoritative delivery is in its drop-off or returning phase: timber/planks,
barrels or crates travel with the actual carrier. Uncollected and delivered
stock is not shown as cargo. Facility stocks remain authoritative simulation
state, not decorative containers.

## Ground detail, territorial overlays and reuse

`terra_bevy::render::WorldDetailSettings` controls optional ground cover,
resource presentation and gameplay-territory boundaries independently of the
simulation. The default close-range grass/flower cover is one batched mesh with
640 clusters within 96 metres. Hard limits are 2,048 clusters and a 180-metre
radius. Cover samples loaded terrain, avoids water, mapped roads and buildings,
authored roads, facilities and player structures, and drapes each blade's base to
the heightfield. It refreshes after 24 metres of focus movement or a relevant
source/geometry change, at most once per 0.8 seconds. Grass is decorative ground
cover; it is not an inventory resource or additional ecological population.

Named gameplay settlements are displayed by a fine dashed boundary that samples
resident heights and a non-solid ownership flag. Neighborhood, village and city
claims have one, two and three pennants respectively. Owner/permission colors
match the existing plot language; names, tier requirements and governance remain
in the Territory inspector. Boundary segments outside loaded terrain are
omitted until that terrain becomes resident. These overlays are separate from
imported political borders and do not create buildings or collision volumes.
The `settlement_boundaries` switch disables them, and the existing Buildings
layer visibility also applies.

Housing-plot outlines also follow the loaded heightfield at every ribbon corner;
they do not stay at the plot center's elevation on a hillside. Plot rings use
64–512 bounded segments and preserve their existing permission colors and width.
Plots and settlement boundaries share coverage admission: a segment is omitted
if a source page between its endpoints is missing, including across the date
line. Loaded page identities invalidate the cached plot mesh when coverage or
source heightfields change, without hashing full terrain arrays each frame.
Claim positions, radii and ownership remain authoritative gameplay data; these
height adjustments affect presentation only.

The material field now uses continuous earth-centered coordinates with double
precision before color calculation, retaining continuity across the date line.
Steeper land exposes rock tones, near forest crowns have finer silhouettes,
and terrain detail has stronger variation. Cooler, balanced ambient lighting,
closer shadow cascades and less intrusive near fog retain relief and material
contrast. The native camera explicitly uses ACES fitted tonemapping and a
controlled day/night exposure range. Photographic texture sets, high-resolution
scanned assets, physically simulated water and expensive volumetric effects
remain replaceable presentation choices for a hosting game.


## Presentation profiles in 0.4

`render::RenderQualitySettings` is a reusable Bevy resource, separate from world
simulation policy. It exposes `quality`, `atmospheric_sky` and `terrain_lod`.
`RenderQuality::{Performance, Balanced, High}` provides `next()`, `label()` and
`from_name()` helpers for game settings. Changing profiles applies camera/light
settings once and queues affected terrain through the existing bounded workers;
textures and actor/resource palettes are not regenerated every frame.

| Setting | Performance | Balanced (default) | High |
|---|---:|---:|---:|
| Shadow map edge | 1,024 | 2,048 | 4,096 |
| Maximum shadow distance | 600 m | 1,200 m | 2,000 m |
| Multisample antialiasing | Off | 4× | 4× |
| Atmospheric sky / generated environment map | Off | Lookup sky / 32 × 32 faces | Lookup sky / 64 × 64 faces |
| Sky-view LUT | None | 128 × 64 | 256 × 128 |
| Aerial-view LUT | None | 16³ | 32³ |
| Ground-cluster upper bound | 320 | 1,024 | 2,048 |
| Terrain strides, near/middle/far | 1 / 2 / 4 | 1 / 2 / 4 | 1 / 1 / 2 |
| Bloom | Off | Off | Restrained natural bloom |

The existing `WorldDetailSettings` remains the requested ground-cover policy;
profile limits cap rather than inflate it. Performance also caps its radius at
64 m. The default request remains 640 clusters within 96 m on Balanced/High.
Balanced uses Bevy's default 4× multisampling because 2× is optional on common
adapters; it does not assume that an intermediate sample count is supported.

The deterministic 49-chunk source-geometry regression fixture measures
1,620,763 vertices across terrain and features, with 6 near, 20 middle and 23 far
chunks. Terrain uses 87,121 vertices against 207,025 original height samples:
a 57.9% render-only reduction. Every original edge sample is retained, while
collision and terrain sampling keep the full source grids. These are CPU mesh
counts for the seeded fixture, not a GPU frame-rate benchmark or an MMORPG
concurrency measurement.

The atmospheric path uses Bevy's Earth scattering medium, the world's solar
position and a visible solar disk. Generated environment lighting supplies
ambient diffuse and reflections, making shaded vegetation and water readable
without a flat global fill. Atmosphere placement follows floating-origin rebases
and geodetic height. Clear-air scattering is illustrative; the weather sample
attenuates sunlight and changes distance haze but does not generate physically
simulated cloud volumes.

`RenderCapabilities.atmospheric_sky` records the adapter check for compute
shaders, RGBA16 float storage textures and nonzero storage-texture limits. An
unsupported adapter automatically uses the directional/ambient compatibility
path. `atmospheric_sky: false` explicitly selects that path on any profile.
Performance is the conservative option for software rendering. Profiles are
cost controls, not promises of frame rate on unmeasured hardware.

The material maps are generated once, use no external downloads, and occupy less
than 800 KiB of uncompressed RGBA texels including all mip levels. Terrain mip
normals are renormalized. Normal-map water ripples freeze when world time is
paused; physical animation speed is independent of calendar acceleration.
Near conifers now use separate bounded bough volumes instead of concentric
solid cones, without alpha-card overdraw. Source buildings retain their original
footprints/heights and add bounded illustrative ground-floor frames, plinths
and cornices. Production rigs, motion capture, interiors and photographic
material libraries remain game-owned content.

Primary API references used for this integration:

- https://docs.rs/bevy/0.20.0/bevy/light/struct.Atmosphere.html
- https://docs.rs/bevy/0.20.0/bevy/pbr/struct.AtmosphereSettings.html
- https://docs.rs/bevy/0.20.0/bevy/light/struct.AtmosphereEnvironmentMapLight.html
- https://bevy.org/examples/3d-rendering/atmosphere/

## Game-supplied actor models

`render::ActorSceneOverrides` lets a hosting game replace reference meshes with
its own Bevy `WorldAsset` scenes. In Bevy 0.20 a glTF scene label loads a
`Handle<WorldAsset>`. This example installs a game-owned human model and a deer
model during startup, after `TerraRenderPlugin` has initialized the registry:

```rust
use bevy::{gltf::GltfAssetLabel, prelude::*};
use terra_bevy::{
    render::{ActorSceneAppearance, ActorSceneOverrides},
    rules::AnimalSpecies,
};
use terra_core::ActorKind;

fn install_models(server: Res<AssetServer>, mut models: ResMut<ActorSceneOverrides>) {
    let mut human = ActorSceneAppearance::new(
        server.load(GltfAssetLabel::Scene(0).from_asset("models/human.glb")),
    );
    human.scale = 1.0;              // metres per imported unit
    human.ground_offset_m = 0.0;    // local feet/wheel alignment
    human.yaw_offset_degrees = 0.0; // reference forward direction is local -Z
    models.set_kind(ActorKind::Human, human).expect("valid model placement");
    models.set_species(
        AnimalSpecies::Deer,
        ActorSceneAppearance::new(
            server.load(GltfAssetLabel::Scene(0).from_asset("models/deer.glb")),
        ),
    ).expect("valid model placement");
}
```

This direct hook does not validate or download the supplied model. The demo's
separate validated asset pack supplies the original animated wildlife example.
The hook supports the
eight actor kinds and five wildlife species, with species entries taking
precedence. The demo attaches `ActorSceneAnchor` and `ProceduralActorMesh` to
each authoritative body visual. A hosting renderer using its own body entities
can attach those same components to opt in. Grounded geographic transforms,
heading, first-person hiding, layer visibility and body identity stay on that
root; the model receives only its explicit local scale, offset and orientation.

A missing, still-loading, dependency-failed or meshless scene retains the
procedural mesh. The renderer creates one child instance per body only when the
resolved model changes, waits for Bevy's actual `WorldInstanceReady`, then
removes the fallback mesh. Removing an override or unloading its asset restores
the fallback and despawns the old scene hierarchy. Readiness is inspected once
per distinct model per update, not once per individual actor. Imported nodes
marked `Visible` become `Inherited` so a hidden possessed body remains hidden;
intentionally hidden model parts remain hidden.

`ActorSceneInstance` on the model child exposes `body_id` for a game's
`WorldInstanceReady` observer. Animation graphs, clips, skinning quality, sockets,
equipment and model licensing belong to the game. Direct overrides without
validated pack metadata have game-owned geometry/memory limits; their instances
are explicitly reported as unmetered by pack diagnostics. The
demo suspends its reference body bobbing while a supplied model is active.
Changing an appearance does not change collision, movement, inventory, ownership
or multiplayer authority. The reference cargo overlay is separate; a game with
different truck proportions should attach cargo to its own model socket.

## Failed mesh builds (0.20)

A disconnected worker or poisoned receiver releases its tracked slot, retains
the dirty page and retries through the same two-job queue. Per-source/revision/LOD
backoff starts at 0.5 seconds and doubles to at most 30 seconds using monotonic
real time, including while simulation time is paused. A changed source, edit
revision or desired LOD resets that failure context; eviction discards it.
Success clears active retry state and replaces geometry; a failure preserves
existing geometry. F12 and opt-in local reports expose active retry pages,
cumulative observed failures and the last failure message. Last failure is
historical even after successful recovery. This does not preempt hung worker
threads or qualify frame/GPU performance. Broad edit invalidation remains WP03.
