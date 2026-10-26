# Data sources and operating the streaming layer

Terra separates geographic coordinates, imported source data and authored game
content. The default playground is deterministic and synthetic. Live network use
is explicit. Every terrain/map chunk carries `SourceRecord` provenance and
warnings; historical packs additionally declare time coverage and an immutable
source revision.

## Included adapters

| Layer | Source | Runtime behavior | Known scope |
|---|---|---|---|
| Elevation | Mapzen Terrain Tiles / AWS Open Data | Terrarium RGB PNGs reprojected into geographic height grids | Web Mercator coverage ±85.0511°; no polar provider or geoid conversion |
| Buildings, roads, POIs and land use | OpenStreetMap via Overpass | Tagged points/ways, waterways, bridges, roofs and functional metadata, clipped to chunks | No relation assembly, holes, complete routing graph or surveyed building interiors |
| Imported elevation | Local grayscale PNG + manifest | Georeferenced 8/16-bit samples resampled into complete geographic tiles | Explicit bounds/provenance; no NoData inference, datum conversion or semantic extraction |
| Current weather | Open-Meteo | Temperature, cloud fraction, precipitation rate and wind, with source time | Current model conditions; no historical replay or volumetric atmosphere |
| Country polygons | Natural Earth 1:110m | Local dated GeoJSON snapshot, original properties retained | Generalized atlas geometry and source-defined boundary policy |
| Offline world and historical examples | Terra authors | Reproducible generated terrain and authored semantics | Fictional content explicitly marked synthetic |

World tile addresses are equirectangular, with `2^level` tiles in both directions
covering the whole longitude/latitude range. Raster provider XYZ addresses use
Web Mercator. The elevation adapter performs the conversion; consumers must not
send a world key directly to an arbitrary XYZ service.

### Elevation and vertical datum

The [AWS Terrain Tiles registry](https://registry.opendata.aws/terrain-tiles/)
describes the hosted data. The [Tilezen format specification](https://github.com/tilezen/joerd/blob/master/docs/formats.md)
defines Terrarium heights as `R * 256 + G + B / 256 - 32768` metres. The adapter
checks dimensions, bounds decompression allocations, rejects implausible/NoData
values, and resamples the source pixels consistently at geographic tile edges.
Source-tile edges use replicated boundary pixels during bilinear sampling; this
is an acknowledged half-pixel approximation, not a continuous geodesic raster
resampler.

Height products have contributing datasets and vertical references. Terra
currently retains their heights without applying a geoid model. Its WGS84
geodetic coordinates use ellipsoidal altitude, so rendering those heights in a
local frame is an approximation recorded in chunk warnings. Use a provider with
explicit datum conversion for applications requiring survey-quality altitude.

The [Tilezen attribution manifest](https://github.com/tilezen/joerd/blob/master/docs/attribution.md)
contains contributing-source credits and terms. The runtime keeps that link with
each sourced chunk. A shipping title must include the applicable full credits;
the code's MIT/Apache license does not replace elevation-source obligations.

### OpenStreetMap geometry and public Overpass

OSM imports retain source IDs and tags. Supported semantics include building
footprints/parts, tagged dimensions and roof forms, roads with physical widths,
lane/surface/bridge attributes, trees, point amenities/shops, water areas and
linear waterways, forest/grass/field polygons, orchards and vineyards. Geometry
is normalized to typed `WorldFeature` values; other properties remain available
to game-specific systems.

The renderer uses supported roof/material/land-use tags to produce illustrative
geometry. The living adapter can use roads for its bounded graph and nearby
shop, water, farm, woodlot, mill or bakery tags to locate authored facilities.
These tags do not prove that a business is open or that its simulated stock,
workers, money or output reflects the real place. Relations, complex polygon
holes, every roof type, interiors and complete transport topology remain outside
the importer. An absent feature does not establish real-world absence. The exact
supported map/render matrix is in [GRAPHICS_AND_MAPS.md](GRAPHICS_AND_MAPS.md).

[OpenStreetMap's copyright page](https://www.openstreetmap.org/copyright)
documents ODbL and attribution. Preserve `© OpenStreetMap contributors` with the
copyright link in a product using that data, and assess the obligations for
redistributing transformed or combined databases.

The [Overpass public-instance manual](https://dev.overpass-api.de/overpass-doc/en/preface/commons.html)
describes shared capacity and explicitly identifies whole-world bbox scraping
and reliance on the public instances as an application backend as problematic.
Use the built-in adapter for small development probes. Production streaming
should use prepared regional extracts, your own instance, or a service with an
appropriate agreement. A whole-world playground does not require millions of
clients to query a shared public Overpass server.

Terra permits only detail levels 13 and above for these queries. Within a process
it allows one Overpass request in flight and waits at least two seconds after
completion before the next request. Defaults cap a session at 256 requests and
256 MiB of attempted response data. Failed requests also consume the budgets.
These caps limit accidental load; they are not a provider service guarantee.

### Local PNG heightmaps

Use `--heightmap path/to/manifest.json` in the demo or install
`HeightmapProvider` through the reusable provider API. A ready-to-run synthetic
example is available with:

```sh
cargo run -p terra-demo -- --config configs/heightmap.json
```

The manifest supplies south/west/north/east bounds, a linear minimum/maximum
height range, an immutable revision and a complete source/attribution record.
North is the PNG's top row. Black and white map to the declared range, and
bilinear sampling produces the requested tile grid. The image must be 8- or
16-bit grayscale. Source height datum remains the caller's responsibility.

Manifest size is limited to 64 KiB, encoded PNG size to 32 MiB, dimensions to
4096 × 4096 and decoder allocations to 128 MiB. The image path must stay within
the manifest directory. Coverage must contain an entire requested tile. Raster
extents crossing the date line should be split; color-image interpretation,
NoData inference and terrain-feature extraction are not implemented.

The included 257 × 257 image, source declaration and generator are authored
fixtures. They demonstrate relief, streaming and controls without claiming to
be a real DEM of Munich. A heightmap-only provider emits no sourced buildings or
vegetation; the sample living facilities and roads are explicitly authored.
Compose providers if your game also has a licensed vector dataset for that area.

### Weather semantics and terms

The [Open-Meteo API documentation](https://open-meteo.com/en/docs) specifies current
model variables, units and aggregation intervals. The adapter requests metres
per second for wind and Unix timestamps, validates expected units, and converts
interval precipitation accumulation to millimetres per hour. Cloud cover is
normalized to a fraction between zero and one. Preserve the returned timestamp:
it describes the model sample, not the accelerated simulation clock or a promise
of direct on-site sensor measurement.

The free hosted API and data license are different contracts. See
[Open-Meteo pricing](https://open-meteo.com/en/pricing) for the hosted API's
non-commercial free tier and commercial service terms. The data attribution
requires credit under CC BY 4.0. Display a link to Open-Meteo where weather is
presented, and keep the model timestamp visible in diagnostics.

Weather caches for ten minutes in 0.05° coordinate cells by default. This is a
sampling approximation; moving inside a cell does not produce an independent
measurement. There is no atmospheric fluid solver, exact seasonal ecology model,
or automatic historical weather download in this adapter.

## Bundled Natural Earth capture

`crates/terra-history/data/natural-earth-countries-2026-10-26.geojson` contains
177 features from the project's official maintained
[1:110m country GeoJSON](https://github.com/nvkelso/natural-earth-vector/blob/master/geojson/ne_110m_admin_0_countries.geojson),
captured on 2026-10-26. The raw retrieved file's SHA-256 is
`6866c877d39cba9c357620878839b336d569f8c662d3cfab4cb1dbe2d39c977f`.
The wrapper preserves original feature properties and geometry and adds the
canonical Terra metadata/properties. Its revision ID includes the raw hash.

Natural Earth declares these maps [public domain](https://www.naturalearthdata.com/about/terms-of-use/).
The included attribution says “Made with Natural Earth” and links to its
[boundary policy](https://www.naturalearthdata.com/about/disputed-boundaries-policy/).
This country file follows the source's de facto presentation and does not bundle
every auxiliary disputed-area or claim line. Empty canonical claimant lists are
explicitly described as incomplete; source status and worldview fields remain
available in the original property map.

The wrapper starts at Unix second `1792972800`, the capture date. That is an
ingestion convention, not an asserted real-world effective date. The importer
refuses earlier requested dates. An open end makes this captured snapshot usable
for a current example, without guaranteeing that it remains geopolitically
current. Use dated evidence for historical scenarios, as described in
[HISTORY.md](HISTORY.md).

## Background loading, failures and cache

`ChunkStreamer` uses a bounded standard-library worker pool. HTTP, disk access,
JSON/PNG decoding and full chunk validation happen on workers. `poll(n)` consumes
at most `min(n, 64)` completions; the frame thread never waits for an HTTP request
or worker join. Desired keys are deduplicated and prioritized. A changed focus
drops obsolete queued work, evicts unwanted residents and rejects stale results.
Existing HTTP requests finish with configured deadlines.

The lower-level `StreamConfig` defaults remain 49 resident tiles, 2 in flight,
64 pending, 33 × 33 terrain samples, 8 MiB per HTTP body and 256 MiB on disk. The
native `WorldConfig` overrides this to a 49-tile requested neighborhood, 64
resident capacity and 65 × 65 grids. A plain `--live` launch selects radius one
and 33 × 33 grids unless a config or explicit radius was supplied. The supplied
`configs/live.json` also keeps that smaller footprint.

The reference renderer independently bounds mesh jobs and geometry detail; the
living layer bounds bodies, sites, routes and replicated views. Hosts must still
choose GPU, actor and collider budgets for their target hardware. Custom providers must obey
their own resource bounds; `ChunkProvider` is trusted application code.

The disk cache stores a schema marker, retrieval time, payload length and a
corruption checksum, with atomic writes. It manages only its own
`terra-v1/*.cache` files and removes oldest entries to satisfy the byte budget.
The checksum detects accidental corruption and is not authentication. Provider
URLs, tile keys, source options and schema versions enter cache keys. Assembled
semantic chunks use `chunk-v4` keys in this version so earlier parser output is
not mistaken for the expanded schema. Stable
terrain/map entries default to a seven-day lifetime; source timestamps also
limit assembled-chunk reuse. Recombining cached raster tiles preserves their
oldest retrieval timestamp instead of presenting the assembly as a fresh fetch.

In live mode, an OSM failure leaves the semantic layer absent. When
`allow_fallback` is enabled, terrain failures may substitute a clearly marked
synthetic heightfield. A chunk may therefore have mixed source status: inspect
each `SourceRecord`, not just whether any real source exists. Degraded chunks are
not persisted as successful live results. Set `allow_fallback: false` for strict
imports or provider validation. Weather-provider errors are returned to callers;
any application fallback must remain visibly synthetic.

Failures receive a bounded 2–60 second retry delay at the chunk scheduler. GET
requests retry transient connection/timeout/server errors once; HTTP429 is
surfaced rather than immediately retried. Configurable endpoint URLs support
organization-hosted services. `ca_cert_path` can add an explicit PEM CA bundle;
otherwise `SSL_CERT_FILE` is honored when present. Certificate verification stays
enabled. Client credentials and private service configuration belong outside
checked-in world packs.

## Verification record and remaining work

The provider test suite uses local deterministic fixtures and a loopback HTTP
service to exercise real request, decode, clipping, unit conversion, provenance
and cache paths without relying on public uptime. It also covers geographic
seams, invalid/polar data, cache corruption and expiration, bounded queues,
duplicate suppression and stale completion rejection. Concurrent loopback calls
also check that Overpass spacing extends past the full response body and that
the process request budget rejects further queries before contacting the API.

A manual strict live probe on 2026-10-26 at 48.137, 11.575 loaded sourced terrain
with a 519.889–530.212 metre sample range and a non-synthetic Open-Meteo current
sample. The public Overpass POST timed out in this execution environment; a tiny
GET diagnostic returned HTTP406. Its local HTTP/fixture integration passed, but
this run does not claim a successful live OSM fetch. One later v0.2 probe to the
Private.coffee alternate also timed out at 20.206 seconds; no repeated public
retry loop was used. Its record is
[`overpass-v2-2026-10-26.json`](../crates/terra-stream/verification/overpass-v2-2026-10-26.json). Service availability and
current data are not guaranteed by a previously successful probe.

To grow this into a production world dataset, add a reproducible offline importer
for regional source extracts, polygon relation assembly, topology and biome
classification; publish versioned chunks and metadata through controlled object
storage/CDN; reconcile data licenses and attributions for the specific sources;
and make all multiplayer peers agree on the same source revision and weather
state. Save player structures, factions and world edits separately from imported
source truth.
