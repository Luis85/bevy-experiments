# Elevation and semantic world bundles

Terra can import a georeferenced grayscale PNG together with an explicit
GeoJSON feature layer. The same `HeightmapProvider` supplies height samples,
normalized features, attribution and source identity to native play or a host.
It does not infer roads, land use or buildings from pixel brightness.

The bundled `manifest-v2.json` adds original forests, fields, an orchard and
specimen trees to the existing synthetic elevation. New launcher heightmap
worlds use the connected west/east bundle set derived from it, documented below.
`manifest.json`, `manifest-v2.json` and the original raster are preserved so
compatible older saves keep their source identity. Neither fixture describes
observed terrain or land use in Munich.

## Play and connect

From the repository root:

```sh
cargo run -p terra-demo -- --heightmap crates/terra-stream/data/heightmap-demo/manifest-v2.json
cargo run -p terra-demo -- --config configs/heightmap-multiplayer.json --connect 127.0.0.1:7777 --session saves/sessions/valley-driver.json
```

The native Multiplayer form accepts `configs/heightmap-multiplayer.json` in
**Local source configuration JSON**. Leave the field blank for the default
procedural host. Use the same bundle, seed, chunk level, height resolution and
political revision as the host. Paths inside a `WorldConfig` use the application's
working directory; the included commands run from the repository root.
Files referenced inside a heightmap manifest use that manifest's directory.

The host's `--heightmap` option is documented in [MULTIPLAYER.md](MULTIPLAYER.md).
Source files must already be present on each machine. The gameplay connection
checks identity; it does not download the bundle. Incomplete tiles are unavailable
and do not receive synthetic replacement terrain. Fixed local bundles cannot be
combined with live adapters or a historical pack in one built-in configuration.

## Manifest extension

Keep the existing version-one raster manifest fields. Add this optional member:

```json
{
  "features": {
    "geojson": "features.geojson",
    "source": {
      "provider": "My explicit feature dataset",
      "license": "The actual source license",
      "attribution": "The attribution required by that source",
      "url": "https://example.invalid/my-features",
      "retrieved_unix_seconds": 0,
      "synthetic": false
    }
  }
}
```

This fragment belongs inside a complete manifest, whose raster schema is in
[GRAPHICS_AND_MAPS.md](GRAPHICS_AND_MAPS.md). Both sources require provider,
license, attribution and URL. Set `synthetic` truthfully. A feature source can
have a different license and origin from its elevation source; both records are
retained on every produced chunk.

## Feature contract

Input is an [RFC 7946 GeoJSON](https://datatracker.ietf.org/doc/html/rfc7946)
`FeatureCollection` with WGS84 longitude/latitude coordinates. Every feature
requires a stable string or numeric `id`, a properties object and supported
geometry. Numeric and boolean properties become text tags; null properties are
omitted. Nested property values are rejected. Unknown scalar tags remain available
to game systems.

| Feature | Accepted geometry | Classification and dimension |
|---|---|---|
| Building | Polygon or MultiPolygon | `building` tag, or `terra:kind: "building"`; explicit `height`, otherwise bounded levels × 3 m or a 6 m default |
| Road | LineString or MultiLineString | `highway`, or `terra:kind: "road"`; explicit `width`, otherwise an authored class default |
| Waterway | LineString or MultiLineString | `waterway`, or `terra:kind: "waterway"`; explicit `width` or a 3 m default |
| Water | Polygon or MultiPolygon | `natural: "water"`, `waterway: "riverbank"`, or `terra:kind: "water"` |
| Forest | Polygon or MultiPolygon | `landuse: "forest"`, `natural: "wood"`, or `terra:kind: "forest"` |
| Farm | Polygon or MultiPolygon | farmland, farmyard, orchard or vineyard `landuse`; or `terra:kind: "farm"` |
| Grass | Polygon or MultiPolygon | grass/meadow `landuse`, `natural: "grassland"`, or `terra:kind: "grass"` |
| Tree | Point or MultiPoint | `natural: "tree"`, or `terra:kind: "tree"` |
| Facility/landmark | Point or MultiPoint | shop, amenity, craft or tourism tags; or `terra:kind: "point_of_interest"` |

`terra:kind` is an explicit override and must match the geometry. Dimensions use
the supported map-length parser; unknown dimensions are labeled as defaults,
not measurements. Road surface tags, roof/building tags, crop and tree tags are
retained for the existing world systems. In the normalized `WorldFeature`,
`height_m` holds width for roads and waterways, as in the existing provider API.

Multi-geometries split into stable parts, retaining `source_id`. Polygons must
have one closed, nonzero, simple exterior. Holes, GeometryCollection, legacy CRS,
null/empty geometry, consecutive duplicate positions, duplicate source IDs and
coordinates outside the raster extent are rejected with an import error.
Prepare a compatible export explicitly; do not remove holes by filling them.
This subset is not a complete general-purpose GIS topology importer.

Clipping can split a concave exterior into disconnected pieces. If the existing
single-ring representation would produce a touching or crossing ring, that tile
is rejected with a diagnostic requiring explicit simple exported parts. It is
never filled across the gap. Boundary-only intersections with zero area are
omitted. Normalized `geometry` tags are `point`, `line` or `polygon`; external
`source_id` remains stable across tile fragments.

Features are filtered by their bounds and clipped to requested tiles. Every
resulting vertex samples that bundle's elevation. Optional source coordinate
altitude is validated but not used; elevation comes from the raster. No geoid,
NoData, bridge-deck or building-foundation reconstruction is inferred. The
existing road/building metadata policies still apply to elevated structures.

## Bounds and identity

| Input | Limit |
|---|---:|
| Manifest | 64 KiB |
| Encoded raster | 32 MiB; 4096 × 4096 pixels; 128 MiB decoder allocation |
| GeoJSON | 8 MiB |
| Source features | 2,048 |
| Normalized parts / clipped parts per tile | 8,192 each |
| Normalized vertices / clipped vertices per tile | 131,072 each |
| Positions per line or ring | 1,024 |
| Properties per feature | 64; key 128 bytes, value 2,048 bytes |
| Expanded normalized properties / clipped properties per tile | 65,536 entries and 8 MiB of key/value text each, including generated metadata |
| Stable source ID | 128 printable bytes |

Raster and GeoJSON paths must be relative and remain within the manifest
directory after canonical resolution. Import validates the entire bounded input
before it becomes a provider. Tile loading uses a bounded feature-bounds scan;
this is a regional file adapter, not a planetary spatial database.
Multipart expansion reserves property budgets before cloning attribute maps.
Tile clipping first produces geometry-only fragments and reserves the complete
attribute budget before attaching source tags, preventing a small zigzag line
or MultiPoint feature from multiplying a large property map without bound.

Use **`HeightmapProvider::source_revision()`** for a save or multiplayer manifest.
For a semantic bundle it combines the declared revision with a BLAKE3 digest of
the canonical manifest, PNG bytes and GeoJSON bytes. Editing the actual bundle
invalidates its source identity even if the declared revision is unchanged.
Byte changes in GeoJSON are deliberately significant. Raster-only legacy
manifests retain their declared revision for save compatibility; publishers
must still give changed legacy raster data a new revision.

Do not replace the revision of an existing saved world to force a new bundle
onto its persistent changes. Create a new world or implement an explicit,
validated game-owned migration. Domain save schema one remains unchanged.

## Verification

`heightmap.rs` tests cover shared raster edges, absent coverage, clipped and
grounded semantic geometry, independent provenance, content identity changes,
geometry mismatches, duplicate IDs, holes and escaped paths. The companion
`heightmap_features.rs` tests cover multipart and clipped-property expansion
limits and disconnected concave clipping. Native/host
admission tests reject changed sources. The release [validation record](VALIDATION.md)
names the exact final test run and observed native landscape; earlier screenshots
are retained as historical evidence.

## Connected prepared bundles

The common `PreparedHeightmapProvider::open(path, resolution)` entry point now
accepts either an existing single manifest or a **connected set**:

```json
{
  "schema_version": 1,
  "kind": "heightmap_set",
  "revision": "my-region-v1",
  "bundles": ["west/manifest.json", "east/manifest.json"],
  "seam_tolerance_m": 0.05
}
```

Every child remains a complete attributed single-bundle manifest. Collection
paths are relative to the collection file, cannot escape that directory after
canonical resolution, cannot repeat a file, and cannot contain another set.
Rasters must form one connected group through shared edges. Corner-only contact,
disconnected islands and overlapping interiors fail before play begins. Shared
coordinates must match exactly; an epsilon is not used to fill geographic gaps.

All elevation knots from **both** rasters along each shared edge are checked,
including the overlap endpoints. Their piecewise-linear edge difference must
remain within the declared tolerance. This catches a narrow spike that a few
uniform probes would miss. The tolerance defaults to 0.05 m and is bounded to
0–2 m; it is a declared acceptance threshold, not automatic terrain repair.
Correct mismatched source elevations/datum in your preparation pipeline.

A tile can span several bundles. Coverage is checked over their exact rectangle
union, including small holes and concave boundaries. Checking only the union's
bounding box or the requested height-sample points would incorrectly invent
terrain in those gaps. Incomplete tiles fail with a coverage error. The provider
never inserts procedural fallback. Shared sample coordinates choose the same
canonical child regardless of request order, preserving identical generated
chunk edges.

Collection identity is a digest of the declared revision, seam policy and every
child's canonical manifest content and exact PNG/GeoJSON bytes, including
raster-only children. Reordering the list or changing manifest formatting/key
order has no effect; changing canonical content or payload bytes does. Legacy single
manifests still retain their established `source_revision()` behavior. Child
features have collection-safe namespaced IDs while preserving their exact
external `source_id` and adding `bundle_revision` provenance. This is a new
source identity, so an old single-bundle save is not silently relabeled as a set.

The collection accepts up to 32 children, with at most 16,777,216 decoded height
samples and 32 MiB of normalized feature text across them. Individual input
limits still apply. Each generated tile retains the existing combined limits of
8,192 feature parts, 131,072 vertices, 65,536 properties and 8 MiB of property
text. Children are bounded in memory; this is prepared regional coverage, not an
unlimited on-demand atlas/CDN.

For a new-world default, `PreparedHeightmapProvider::covered_focus(level)` first
tries the existing child-center choices. If none covers a complete tile, it
checks candidate rows and columns adjacent to bundle boundaries, bounded by the
32-child contract, and admits a result only through exact union coverage. This
also finds a jointly covered tile between offset strips whose individual centers
are unusable. It does not scan every tile in a large bounding box, fill holes or
move an explicitly configured or saved focus.

### Included connected valley

New **Heightmap valley** worlds use an original two-bundle demonstration:

```sh
cargo run -p terra-demo -- --config configs/connected-heightmap.json
cargo run -p terra-net --bin terra-server -- --heightmap crates/terra-stream/data/heightmap-connected/manifest.json
cargo run -p terra-demo -- --config configs/connected-heightmap-multiplayer.json --connect 127.0.0.1:7777
```

The local configuration enables regional storage. The multiplayer configuration
leaves authority to the host. `data/heightmap-connected/generate.py` derives
adjacent west/east elevation rasters from the original authored valley, retains
one shared sample column, and explicitly clips its forest/field/tree semantics.
The 81 covered tiles cross the source boundary and retain full attribution.
The old heightmap manifests and rasters are unchanged for compatible saved worlds.

### Persistent feature identifiers

The gameplay store requires identifiers of at most 128 ASCII identifier bytes.
An otherwise valid 128-byte external GeoJSON ID could exceed that contract after
part and clipping suffixes; spaces or Unicode could also make the feature
unavailable to gameplay. The importer now hashes such IDs into a deterministic
bounded identifier while retaining the exact external ID in `source_id`.
Ordinary previously valid short identifiers remain byte-for-byte unchanged.

## Inspect, distribute and install an immutable prepared source

`terra-sources`, included in `terra-stream`, is an offline developer handoff for
the same importer used by native play and the host. It validates declared
extent, raster/feature admission, connectedness and seams, then inventories every
referenced file. It does not fetch geographic data or change world authority.
The existing authored west/east valley is a complete working example:

```sh
cargo run -p terra-stream --bin terra-sources -- inspect crates/terra-stream/data/heightmap-connected/manifest.json --level 15 --resolution 65 --require-complete
cargo run -p terra-stream --bin terra-sources -- export crates/terra-stream/data/heightmap-connected/manifest.json /tmp/terra-valley-package --level 15 --resolution 65 --require-complete
cargo run -p terra-stream --bin terra-sources -- verify /tmp/terra-valley-package --require-complete
cargo run -p terra-stream --bin terra-sources -- install /tmp/terra-valley-package /tmp/terra-valley-installed --require-complete
```

Choose new destination names when repeating the example. Destination parents
must exist; existing directories, files and partial exports are never
overwritten. This example distributes the original two synthetic bundles and
their seven referenced source files. Source identity, exact manifest/PNG/GeoJSON
bytes, per-layer licensing, attribution, URLs and synthetic flags survive the
move. Development scripts and unrelated neighboring files are not copied. Point
a host/client source configuration at the installed `manifest.json` only after
verification succeeds. Gameplay connections still do not download packages.

`inspect` prints a `SourceInspection` JSON object with `source_identity`,
`declared_bounds`, bundle descriptions/provenance, file sizes and BLAKE3 digests,
and a coverage report. `export`, `verify` and `install` return
`{ "package_identity": "blake3:…", "inspection": { … } }`. The package digest
identifies the **exact canonical index bytes**, which bind every member hash.
It is separate from the source identity used by saves and multiplayer admission.
Publish that package digest through the same trusted channel as the release:

```sh
terra-sources verify RECEIVED_DIRECTORY --expect-package blake3:DIGEST_FROM_EXPORT --expect-source SOURCE_ID_FROM_EXPORT
terra-sources install RECEIVED_DIRECTORY NEW_DIRECTORY --expect-package blake3:DIGEST_FROM_EXPORT --expect-source SOURCE_ID_FROM_EXPORT
```

Replace those digest/identity placeholders with the export result. A mismatch
fails before a destination is created. Hashes establish content integrity; they
are not publisher signatures and do not prove that a license or geography claim
is legally or scientifically correct. A receiving developer must establish the
publisher and actual redistribution rights. The package retains each source's
declared provenance rather than supplying invented attribution.

### Coverage and NoData scope

The default requested extent is the source's navigation bounding rectangle.
`--bounds west,south,east,north` requests another non-wrapping rectangle. Every
tile **intersecting** it at `--level` is checked through exact provider coverage;
a tile only partly covered at an outer edge is reported missing. The report
contains `requested_tiles`, `fully_covered_tiles`, the complete `missing_tiles`
list and `complete`. A concave set's bounding rectangle is not treated as covered.
The authored valley's level-15 extent is tile aligned and contains 81 complete
tiles. Other valid inputs can legitimately report `complete: false`.

Inspection succeeds for a valid partial source and reports those gaps.
`--require-complete` makes any missing requested tile a command failure and,
for export/install, prevents publication. This policy is opt-in because a
developer may deliberately distribute a smaller region. `--resolution` controls
the importer validation configuration, defaults to 65 and remains in 2–257.
Coverage level is 0–20. A request must contain at most 65,536 intersecting tiles;
larger requests fail before allocating a global tile list. Use a smaller extent
or coarser level, without describing that coarse inspection as complete
fine-resolution coverage.

The admitted PNG profile remains **dense grayscale**: every sample is elevation.
There is no NoData mask or sentinel-value inference. The report explicitly says
so. Exporters must remove unsupported NoData and represent genuinely missing
coverage with omitted/split rectangular sources; do not turn an unknown sentinel
into an invented height. Separate disconnected regions need separate prepared
packages or a deliberately implemented game-owned provider. None of these checks
establish surveyed accuracy, worldwide coverage or historical completeness.

### Admission, publication and updates

The portable workflow adds a 256 MiB aggregate encoded-member limit, at most 97
files, and an 8 MiB index limit to existing importer bounds. Member paths use
portable ASCII names bounded to 1,024 bytes/16 components. Traversal, absolute paths, symbolic links,
special files, platform-invalid characters, reserved publication/portable filenames and case-insensitive name collisions
are rejected. Received packages reject unindexed entries. Keep input and output
directories quiescent during the operation; this is not an adversarial
concurrent-filesystem sandbox.

Export checks source hashes before and after provider validation, copies into
an exclusively created directory, hashes copied bytes and reopens the copied
provider. Its original source identity, extents, provenance and coverage must
match exactly after path rebasing. A fully written and synced temporary index is
renamed to `source-package.json` **last**. A directory without that complete
index is not an admitted package. Install performs received-index/hash/source
checks and the same copied-provider checks. An unsuccessful operation removes
only the newly created destination; it does not replace existing data. This is
a publication marker, not a claim of power-loss-atomic directory replication.

Keep immutable releases in separate directories. New semantic bytes change the
source identity; changed legacy raster-only data still requires the publisher
to assign a new revision. Package hashes additionally bind legacy raster bytes.
Neither tool changes a saved revision, merges incompatible source extents, edits
configurations or migrates saved worlds. Choose a new world or an explicitly
validated game migration when authority must change.

SDK callers can use `source_distribution::{inspect_prepared_source,
export_source_package, verify_source_package, install_source_package}` and
`SourceInspectionOptions`. Tests cover the authored roundtrip, legacy identity,
concave/edge gaps, bounded enumeration, forged report metadata, altered member
bytes, path/link admission, trusted-identity mismatches and no-overwrite behavior.
