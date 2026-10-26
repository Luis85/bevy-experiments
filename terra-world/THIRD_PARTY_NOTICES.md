# Source and data notices

Terra World's original source code and authored example data are offered under either the MIT license or Apache License 2.0, at your option. The license files are included. Dependency licenses remain those of the respective Rust packages; the locked versions are listed in Cargo.lock. This source archive does not vendor third-party Rust crates or compiled runtime binaries.

## Desktop UI font

The desktop explorer embeds DejaVu Sans so geographic symbols, degree marks and
recipe arrows render offline. The unmodified font and its Bitstream Vera/DejaVu
license notice are in `crates/terra-demo/assets/`. The font's license is separate
from the Rust source code license.

## Bundled real political geometry

`crates/terra-history/data/natural-earth-countries-2026-10-26.geojson` contains Natural Earth's 1:110m admin-0 country polygons, retrieved on 2026-10-26. The original metadata and source URL are retained; Terra adds explicit provenance, capture-date coverage and canonical query fields. Natural Earth makes its map data public domain. Its default map follows its own de facto/disputed-boundary policy; the geometry is not a declaration of universal legal recognition or a historical reconstruction.

- Source: https://github.com/nvkelso/natural-earth-vector/blob/master/geojson/ne_110m_admin_0_countries.geojson
- Terms: https://www.naturalearthdata.com/about/terms-of-use/
- Boundary policy: https://www.naturalearthdata.com/about/disputed-boundaries-policy/

## Authored examples

The two small era packs and fictional political territories are **synthetic**. Their dates demonstrate temporal selection. They are not evidence about real 1950 or 1980 geography, boundaries, factions, buildings or farms. Their manifests and rendered source labels state this explicitly.

## Data requested only when enabled

No global terrain raster, OSM database or weather archive is redistributed in this source bundle. The live adapters fetch limited data only when requested, retain source records, and show synthetic fallbacks explicitly. Provider data terms and API-hosting terms are separate obligations. See docs/DATA.md for exact adapters, attribution and links, including OpenStreetMap ODbL and Open-Meteo CC BY 4.0 data / hosted-service terms.

The tiny provider parser fixtures and synthetic HTTP test payloads exercise formats; they are not a factual regional dataset. The live-probe verification record reports a few measurements and the observed provider availability on the run date.

Do not treat the source-code license as a blanket license for every external dataset a game may load.
