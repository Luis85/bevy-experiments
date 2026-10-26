# terra-stream

Bounded background chunk loading for a geographic world. The crate is independent
of Bevy and can be reused in import tools, authoritative servers and tests.

## Start locally

```rust
use terra_core::GeoPosition;
use terra_stream::{ChunkStreamer, StreamConfig, StreamEvent};

let mut world = ChunkStreamer::new(StreamConfig::default())?;
world.set_focus(GeoPosition::new(48.137, 11.575, 0.0)?, 15, 2)?;
// Call once per frame. Network, decoding and cache I/O run on background workers.
for event in world.poll(2) {
    match event {
        StreamEvent::Loaded(chunk) => { /* create/update this chunk's render data */ }
        StreamEvent::Evicted(key) => { /* free its mesh/collider/instance buffers */ }
        StreamEvent::Failed { key, error } => { /* surface diagnostics */ }
    }
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

The default mode creates deterministic synthetic terrain and semantic content at
real geographic coordinates. Synthetic content is always marked in `sources` and
`warnings`. It is a development playground, not a reconstruction of that place.

`StreamConfig { live: true, ..Default::default() }` enables the public-data
adapters. `load_chunk` and `fetch_weather` also work synchronously in import tools,
but must not be called from a render-frame system. `ChunkProvider` plus
`ChunkStreamer::with_provider` is the extension interface for local packs,
organization data services or additional datasets.

## Implemented providers

| Layer | Source | Implemented behavior | Explicit limits |
|---|---|---|---|
| Height field | Mapzen Terrain Tiles on AWS | Terrarium PNG decode, geographic resampling, stable shared tile samples, raw-tile and result caching | Web Mercator only, ±85.0511°; source zoom capped at 15; ≤12 source images per world chunk; no geoid conversion |
| Semantic features | OpenStreetMap via Overpass | Buildings, roads, trees, closed water/forest/farmland/grass polygons; OSM IDs and tags preserved | No relation assembly, polygon holes, river-centreline buffering, routable graph, tunnel/bridge topology or complete city meshing |
| Current weather | Open-Meteo | Temperature, cloud fraction, precipitation rate, wind speed/direction, provider timestamp | Current wall-clock model conditions; no historical replay, full forecast series, atmosphere volume or physical fluid simulation |
| Offline fallback | Terra generator | Seamless synthetic surface, roads, ponds, farm/forest patches, houses and trees | Illustrative geometry, not real location data |

In live mode missing OSM content stays missing. No fabricated roads, buildings or
trees are overlaid and labelled as real data. A terrain failure can use a clearly
marked synthetic heightfield when `allow_fallback` is true. Set it to false for
strict import/verification workflows. Degraded chunks are not persisted as
successful cached real data, so a later load can recover.

The elevation provider has mixed contributing datasets and height references.
The implementation reports the absence of a geoid transformation when placing
those heights in a WGS84 local frame. Do not use the demo mesh as an exact vertical
datum conversion, aviation terrain clearance model or survey product.

## Lifecycle and budgets

1. Submit desired tile keys in priority order, or use `set_focus` to sort a nearby
   area by wrapped tile distance.
2. Deduplicate desired keys and truncate to `max_resident`; keep at most
   `max_queued` waiting and `max_in_flight` submitted requests.
3. Workers perform provider access, payload checks and decoding away from the
   application thread.
4. `poll(n)` integrates at most `min(n, 64)` completions and reports evictions.
5. A changed desired area discards queued work, evicts obsolete resident chunks
   and rejects obsolete completions. Active HTTP requests finish within their
   configured timeout; they are not forcibly aborted.
6. Failed requests receive bounded exponential backoff of 2–60 seconds.

Resident storage is a desired-area cache, not a predictive global cache. The disk
cache provides cross-visit reuse and removes oldest entries when its byte budget
is exceeded. Its versioned binary envelope contains timestamp, length and an
integrity checksum; writes are atomic. Invalid/expired entries are ignored. Only
its own `terra-v1/*.cache` files are managed. There is no global database scan or
disk I/O on `poll`.

The default budgets are 49 resident chunks, 2 in flight, 64 pending, a 33×33 height
grid, 8 MiB per response and a 256 MiB disk cache. Tile count is bounded; the
renderer must separately budget GPU meshes, colliders and vegetation instances.

Overpass is serialized across this process with at least two seconds between
completed requests, a default 256-request session budget, and a 256 MiB session
download cap. Rejected/error requests consume these budgets as well. Stable terrain/map results cache for seven
days; weather caches for ten minutes in 0.05° cells. Weather values are from the
provider wall-clock timestamp; accelerated game time does not create forecasts
for that simulated date. HTTP GET retries only transient connection, timeout and
server errors once. HTTP 429 is surfaced rather than immediately retried.

## Production distribution

Public APIs are suitable for a small development probe. The reusable architecture
is designed to replace them with prepared world packs or a controlled data
service: ingest regional extracts, validate geometry, record source licenses and
dates, convert to canonical chunks, store immutable revisions, serve through a
CDN, then retain mutable gameplay edits separately. Multiple players should
consume one server-authoritative source revision and weather state. They should
not independently query public APIs every frame.

## Provider terms and sources

Checked against source documentation on 2026-10-26:

- [AWS Open Data: Terrain Tiles](https://registry.opendata.aws/terrain-tiles/)
  describes the global tiled elevation dataset and points to source attribution.
- [Tilezen format specification](https://github.com/tilezen/joerd/blob/master/docs/formats.md)
  documents the Terrarium RGB equation and Mercator raster formats.
- [Tilezen source attribution](https://github.com/tilezen/joerd/blob/master/docs/attribution.md)
  is the authoritative contributing-dataset credit list. The code retains its URL
  in every sourced terrain chunk; a production game's credits should include the
  applicable complete attribution.
- [Overpass public-instance documentation](https://wiki.openstreetmap.org/wiki/Overpass_API)
  explains that free public servers are intended for small projects and recommends
  self-hosting or regional extracts for heavier use.
- [OpenStreetMap copyright and license](https://www.openstreetmap.org/copyright)
  describes ODbL attribution and database licensing. Code licensing does not
  replace data licensing.
- [Open-Meteo forecast API](https://open-meteo.com/en/docs)
  defines the current variables and units.
- [Open-Meteo pricing/terms](https://open-meteo.com/en/pricing)
  distinguishes its non-commercial free hosted API from commercial plans.
  Its [data](https://open-meteo.com/) are CC BY 4.0 with attribution; the hosted
  endpoint's use terms and its server source-code license are separate matters.

Parser fixtures are synthetic and intentionally do not contain a downloaded OSM
extract. Tests cover cross-chunk seams, polar behavior, deterministic offline
loads, exact Terrarium decoding, OSM validity, weather units, cache corruption and
eviction, completion budgets, duplicate suppression and stale-request rejection.
The loopback HTTP integration also checks the shared Overpass request gate under
concurrent loads, spacing after complete response bodies, and exhausted session
budgets. See [the data guide](../../docs/DATA.md) for operation and the recorded
live-provider verification result, and [the historical data guide](../../docs/HISTORY.md)
for custom dated pack integration.
