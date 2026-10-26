# terra-history

Time-bounded world packs and political geometry for Terra World, without a Bevy or
network dependency. Load a validated local pack once, then query immutable chunks
from a streaming worker, a headless server, or an importer.

```rust
use terra_core::{GeoPosition, TileKey};
use terra_history::{HistoricalPack, PoliticsDataset, parse_date};

let epoch = parse_date("2000-01-01")?;
let position = GeoPosition::new(48.137, 11.575, 0.0)?;
let pack = HistoricalPack::load("data/synthetic-era-b/manifest.json")?;
let chunk = pack.chunk(TileKey::at(position, 15)?, epoch)?;
assert!(chunk.sources.iter().all(|source| source.synthetic));

let politics = PoliticsDataset::load("data/synthetic-politics-eras.geojson")?;
for territory in politics.at(position, epoch)? {
    println!("{}: {:?}", territory.name, territory.faction_id);
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

The example paths above are relative to this crate. The workspace documentation
uses paths relative to the repository root.

- [Historical pack and GeoJSON formats](../../docs/HISTORY.md)
- [Data sources, attribution, provider operation and limitations](../../docs/DATA.md)

Included real data: a Natural Earth 1:110m country snapshot captured on 2026-10-26,
with original source properties, public-domain provenance, and explicit date
scope. It is a current source capture, not a historical reconstruction.

Included authored data: two small fictional terrain/settlement eras and matching
fictional faction territories near the demo's coordinates. The dates exercise
temporal selection; all content remains explicitly synthetic.

`HistoricalCatalog` rejects ambiguous overlapping same-tile coverage. All
intervals are half-open. Missing tiles and dates outside a pack return errors;
current terrain and procedural content are never substituted by this crate.

Political queries preserve polygon holes and antimeridian crossings, return all
overlapping source claims, and keep gameplay faction links separate from country
and claimant properties. Import/export retains unknown feature properties. The
geometry validator checks structural validity and budgets; it does not repair
self-intersecting polygons or prove that an imported political claim is correct.
