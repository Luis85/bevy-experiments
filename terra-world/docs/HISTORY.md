# Historical worlds, political boundaries and scenario territories

Terra can start a simulation from a dated local world pack and overlay dated
political geometry. Data selection is explicit: changing a sky clock does not
turn today's buildings, weather, or borders into historical evidence.

## Included data and example dates

All paths below are relative to the repository root.

| Data | File | Coverage |
|---|---|---|
| Real current country snapshot | `crates/terra-history/data/natural-earth-countries-2026-10-26.geojson` | Captured 2026-10-26; no assertion about earlier eras |
| Fictional rural settlement, era A | `crates/terra-history/data/synthetic-era-a/manifest.json` | 1950-01-01 inclusive to 1980-01-01 exclusive |
| Fictional expanded settlement, era B | `crates/terra-history/data/synthetic-era-b/manifest.json` | 1980-01-01 inclusive to 2020-01-01 exclusive |
| Fictional cooperative territories | `crates/terra-history/data/synthetic-politics-eras.geojson` | Both example eras; shared border changes at 1980-01-01 |

Both world packs cover 25 level-15 geographic tiles around latitude 48.137,
longitude 11.575. Each tile has a 17×17 height grid. Era B has wider roads and more
buildings. The landscape, settlements and factions are authored examples. These
files do not reconstruct Munich in 1950, 1980 or 2000.

Useful Unix timestamps, all at UTC midnight:

| Date | Unix seconds |
|---|---:|
| 1950-01-01 | -631152000 |
| 1980-01-01 | 315532800 |
| 2000-01-01 | 946684800 |
| 2020-01-01 | 1577836800 |
| 2026-10-26 | 1792972800 |

`parse_date` accepts a signed Unix-seconds string or `YYYY-MM-DD`. It validates
Gregorian calendar dates in years 0001 through 9999 and interprets dates as UTC
midnight. It does not infer locale, historical calendar conversion, uncertainty,
or the time zone in which an event occurred. Convert such evidence deliberately
in the importer and describe the choice in the provenance.

## World pack format

A pack is a directory containing a JSON manifest and canonical `WorldChunk` JSON
files. One manifest describes one validity interval. Use adjacent packs for
different revisions of the same tile.

```json
{
  "schema_version": 1,
  "id": "my-scenario-v1-sha256-CONTENT_HASH",
  "title": "Author-defined example era",
  "valid_from_unix": 315532800,
  "valid_until_unix": 1577836800,
  "source_url": "urn:example:authored-scenario:v1",
  "license": "MIT OR Apache-2.0",
  "attribution": "Authored fictional example; no historical accuracy claimed",
  "synthetic": true,
  "chunk_files": ["15-17437-7620.json"]
}
```

`valid_from_unix` is inclusive; `valid_until_unix` is exclusive. A null end means
that the imported dataset imposes no end date. This is an author declaration,
not proof that the source remains accurate indefinitely.

The loader checks schema version, finite geometry, height dimensions, nonempty
source provenance, unique tile keys, and the declared synthetic status. A pack
marked non-synthetic cannot contain a synthetic source. Paths must stay inside
the manifest directory, including after symbolic-link resolution. Absolute
paths, parent traversal, duplicate file names and duplicate tile keys fail.
Only regular files are accepted.

Limits are 512 chunk files, 256 KiB for the manifest, and 32 MiB for the entire
pack's JSON input. These deliberately make the first importer predictable.
Large deployments should use an indexed regional package service through
`ChunkProvider`; do not turn the bounded pack loader into an unbounded planet
JSON reader.

The existing `WorldChunk` schema is the canonical interchange format. Heights are
row-major, north to south and west to east, including shared boundary samples.
World tile keys use a full-globe equirectangular grid; they are not Web Mercator
XYZ addresses. Geographic positions contain latitude, longitude and altitude in
metres. Record the height source's vertical datum and any conversion or
approximation in each chunk's sources and warnings.

```rust
use terra_core::{GeoPosition, TileKey};
use terra_history::{HistoricalCatalog, HistoricalPack, parse_date};

let earlier = HistoricalPack::load(
    "crates/terra-history/data/synthetic-era-a/manifest.json"
)?;
let later = HistoricalPack::load(
    "crates/terra-history/data/synthetic-era-b/manifest.json"
)?;
let catalog = HistoricalCatalog::new(vec![earlier, later])?;
let key = TileKey::at(GeoPosition::new(48.137, 11.575, 0.0)?, 15)?;
let chunk = catalog.chunk(key, parse_date("2000-01-01")?)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

`HistoricalPack::keys` enumerates coverage; `chunk_ref` avoids a payload clone;
`terrain_height` returns `None` beyond coverage. `HistoricalCatalog` rejects
duplicate revision IDs and same-tile temporal overlaps. It never chooses a
preferred historical source silently. Missing data yields an error or `None`,
without requesting today's public APIs or generating substitute terrain.

## Source epoch and simulation clock

The integrated world configuration uses `historical_pack` and an explicit
`data_epoch_unix`. Its historical provider loads the pack once and pins all
chunk queries to that epoch. The simulation clock can advance the game after
this starting point; doing so does not fetch a later survey or replace the
terrain beneath players. Restart or deliberately build a transition system to
change the base revision. The core catalog API supports explicit era selection
for applications that implement such transitions.

The authoritative server can require a pack ID, exact source epoch and political
dataset revision. All peers must use the same data. Keep source revisions
immutable: a versioned ID or content hash belongs in the manifest, and editing
any chunk requires a new revision. The loader checks payload validity; it does
not cryptographically verify a user-supplied hash embedded in an arbitrary ID.
The bundled example IDs include a hash of their filenames and chunk bytes.

Use `live_data: false` for historical configurations. The current Open-Meteo
adapter is a wall-clock current-weather adapter. Historical replay needs a dated
weather series supplied by the scenario, not the current endpoint.

## Political GeoJSON

The political importer accepts a GeoJSON `FeatureCollection` containing Polygon
or MultiPolygon geometry and a required top-level `terra` metadata member:

```json
{
  "type": "FeatureCollection",
  "terra": {
    "schema_version": 1,
    "id": "fictional-territories-v1",
    "title": "Fictional scenario territories",
    "valid_from": "1980-01-01",
    "valid_until": "2020-01-01",
    "source_url": "urn:example:fictional-territories:v1",
    "license": "MIT OR Apache-2.0",
    "attribution": "Authored gameplay example, not real political evidence",
    "synthetic": true
  },
  "features": [{
    "type": "Feature",
    "properties": {
      "id": "territory-a",
      "name": "Fictional cooperative",
      "kind": "scenario_territory",
      "country": "",
      "claimants": [],
      "status": "Fictional scenario",
      "faction_id": 7
    },
    "geometry": {
      "type": "Polygon",
      "coordinates": [[[11.57,48.13],[11.58,48.13],[11.58,48.14],[11.57,48.14],[11.57,48.13]]]
    }
  }]
}
```

GeoJSON uses **longitude, latitude**, with an optional third altitude coordinate.
Rings must be closed. Ring zero is the exterior; additional rings are holes.
Multipolygons keep all component polygons and holes. Longitudes must be in
[-180, 180], latitudes in [-90, 90]. Point queries unwrap antimeridian crossings;
view queries accept bounds crossing the dateline (`west > east`). Hole boundaries
are excluded by the containment query; an exterior boundary is included, so two
adjacent territories can both match exactly on their shared boundary.

Canonical feature properties are `id`, `name`, `kind`, `country`, `claimants`,
`status`, `faction_id`, `valid_from_unix` and `valid_until_unix`. Date-string aliases
`valid_from` and `valid_until` are accepted. Absent/null feature dates inherit the
dataset range. Explicit feature ranges must stay inside it. Supported kinds are
`country`, `region`, `disputed_area` and `scenario_territory`. IDs must be unique
within a dataset; use era-specific IDs when multiple versions are present.

`country` is the source's identity code, not an assertion that every provider uses
ISO codes. `claimants` is an explicit list supplied by that source. Empty claims
do not mean that a place is undisputed. `faction_id` is an optional gameplay link;
assigning a faction does not rewrite the country, control status, or source
claims. `PoliticsDataset::at` returns every overlapping matching feature. Games
decide how to display conflicting claims and must not infer ownership from
iteration order.

`visible` performs conservative bounding-box culling. `to_geojson` validates
edited data, retains original feature properties, preserves rings, holes,
altitudes, dates and provenance, and writes canonical fields from their current
public values. Unknown feature properties survive a round trip. Metadata outside
the documented schema and arbitrary GeoJSON foreign members are not preserved.

Import limits are 32 MiB, 8,192 features, 500,000 total vertices, and 4,096 polygons
per feature/rings per polygon. The validator checks closed finite rings and
nonzero area. It does not repair self-intersections, validate every hole's
topology, perform spherical geodesic polygon operations, or establish the
historical accuracy of an author's claim. Prepare complex data in a geographic
import pipeline before distributing a scenario.

## Real historical sources and evidence

The bundled Natural Earth file uses the publisher's current 1:110m country
geometry. The capture date is explicitly an ingestion boundary for this example,
not the date on which every border became effective. Its geometry is generalized
for an atlas and is unsuitable for parcel-scale ownership. Source properties
including Natural Earth's boundary/status fields remain attached. The included
country file is not the project's complete auxiliary disputed-boundary/claim
dataset. See its [disputed boundaries policy](https://www.naturalearthdata.com/about/disputed-boundaries-policy/).

For actual historical country coverage, ETH's [CShapes 2.0](https://icr.ethz.ch/data/cshapes/)
documents independent states and dependent territories from 1886 through 2019,
with CShapes-Europe extending to 1816. Its license is CC BY-NC-SA 4.0. It is not
bundled or treated as a freely reusable commercial-game default; obtain suitable
rights for the intended use and retain its coding system and citation metadata.

[OpenStreetMap full history](https://wiki.openstreetmap.org/wiki/Planet.osm/full)
contains past database versions, including deleted objects subject to the
publisher's documented exclusions. An edit timestamp describes when the database
changed. It does not, by itself, establish when a building was erected or a road
existed. OSM history, historical surveys, aerial imagery and authored
reconstructions can support a dated importer, but each must retain its own dates,
coverage, uncertainty and licensing. Terra supplies the runtime contract; it
does not include a complete evidence-backed historical Earth database.
