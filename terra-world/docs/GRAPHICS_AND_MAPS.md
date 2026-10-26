# Terrain, map semantics and the reference renderer

Terra 0.2 expands the visible regional world while keeping the renderer and data
providers separate. The native default requests a 7 × 7 neighborhood of level-15
geographic chunks and uses 65 × 65 elevation samples per tile. Live data should
use a smaller request radius or prepared regional packs: increasing view range
must not turn a public endpoint into a bulk-download backend.

## Height fields and materials

Terrain is built from the actual height samples, with shared indexed vertices,
continuous bilinear queries, smooth derivative normals, and generated tangents.
The render mesh uses the same source elevation grid that actors query. The
renderer adds a small authored ground-grain color/normal texture for close views.
This texture, elevation/slope/latitude palette, and land-use tint are illustrative
materials, not satellite imagery, measured soil properties, or climate evidence.

Forest, field and grass polygons affect the height-field material instead of
placing a flat triangulated cover across hills. Forest trees sample that terrain;
field rows are subdivided and grounded. Water areas are flat surfaces with their
recorded `ele` value when available, otherwise an inferred boundary elevation.
The offline pond has an authored basin in the synthetic elevation function.
Water does not simulate fluid volume, erosion, waves or flood dynamics.

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
Its generator is provided for reproducibility. Source semantics can be composed
with the heightmap through a custom provider; the heightmap provider itself
returns only terrain and provenance.

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
stale results are discarded before replacing visible geometry.

| Detail | Trees per chunk | Building windows | Agricultural rows | Road detail |
|---|---:|---:|---:|---|
| Near | 160, full crowns | Up to 144 per building | Up to 900 segments | 8 m subdivisions, shoulders, lane marks, bridge rails |
| Middle | 48, simplified crowns | Up to 12 per building | Up to 120 segments | 16 m subdivisions and reduced trim |
| Far | 24, silhouettes | None | Land-use material only | 30 m subdivisions, road/bridge surface |

Terrain retains the full 65 × 65 sample grid at every tier, so distance detail
never changes collision, height queries or source semantics. Road subdivision is
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
source `chunks` and `vertices`, plus `pending_meshes` and `building_meshes`.
Capture tools can wait for every resident chunk to be integrated and both queues
to empty. The vertex count includes source terrain/features; independently cached
living actors and player-built structures have separate presentation budgets.

All eight actor kinds now have distinct authored silhouettes. Human heads and
limbs, animal bodies, wheels, cabins, aircraft and balloons use rounded bounded
geometry. Market stalls and production sites have separate reusable meshes.
Material colors are converted from authored sRGB values to the linear vertex
colors expected by Bevy's PBR pipeline. Fog distance follows the resident span
and weather; sun, ambient light and vegetation tint retain day/night and season
configuration. Detailed animation rigs, photographic assets and physically
accurate vehicle dynamics are separate extension points.
