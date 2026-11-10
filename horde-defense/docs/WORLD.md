# World campaign and geographic content

The world catalogue contains **242 selectable countries and territories**. Every
Natural Earth 1:50m administrative feature is included, including microstates
such as Vatican City, Monaco, San Marino and Liechtenstein. Display boundaries
use the lighter 1:110m geometry wherever available; otherwise 1:50m geometry
supplies the outline. A small-country list remains necessary because some
countries occupy less than a pixel on a full-world map.

Made with Natural Earth. The [source data is public domain](https://www.naturalearthdata.com/about/terms-of-use/).
Sources are the Natural Earth maintained [1:110m](https://github.com/nvkelso/natural-earth-vector/blob/master/geojson/ne_110m_admin_0_countries.geojson)
and [1:50m](https://github.com/nvkelso/natural-earth-vector/blob/master/geojson/ne_50m_admin_0_countries.geojson)
admin-0 country catalogues. SHA-256 source fingerprints are stored in
`world.json`. Population estimates retain their source year (predominantly
2019); they are scenario inputs, not a claim of current demographic accuracy.
Natural Earth's representation includes disputed regions and territories; map
inclusion, identifiers and campaign connections make no sovereignty claim.

## Configuring the campaign

For a development change, edit `crates/horde_core/src/world.json` and rebuild.
For runtime authoring, copy that JSON anywhere, edit it, and set `LASTLIGHT_WORLD`
to its file path before launching. The catalogue loads once at startup and a
bad override fails explicitly with its path and read/parse/validation error;
it never silently falls back to bundled defaults. Existing saves should use the
same configuration to preserve their local-stage accounting. The file is
deliberately separate from battle content.

```powershell
$env:LASTLIGHT_WORLD = "C:\Games\Lastlight\my-world.json"
cargo run --locked -p horde-defense
```
Every country entry exposes name, population,
geometry, biome, region/cultural setting, difficulty, modifiers, explanatory
boost/buff/debuff text, border/sea links and an optional exact `level_override`.
The default stage count is `ceil(population / levels_per_population)` clamped
to `min_levels..=max_levels`: 10 million residents per stage, 3–24 stages.
An override replaces the population formula. The global configuration and every
country are validated before the catalogue can be used.

Country biomes use an original authored scenario classification, and
`culture` supplies a regional location dressing such as reclaimed rail yards or
mountain service passes. These are broad themes, not researched individual
national architecture, clothing, music or language. Distinct national visual
asset packs are not bundled. Difficulty is an authored game value, unrelated to
real-world prosperity, conflict, safety, or assessments of national populations.

Modifiers have concrete gameplay fields: enemy health, enemy speed, reward
multiplier, additive build supply, personal weapon damage and NPC attack skill.
The starting country does not change these authored values, but the start clamp
below changes its effective difficulty, and with it the enemy health, enemy
speed and reward multipliers its deployments play (Austria 0.85 plays 0.95,
Bangladesh 1.60 plays 1.05). Encounter templates provide the five battlefield
roles; a country can contain more local stages than templates.

Country difficulty spans 0.85-1.60 in whole hundredths. The band follows the
country's size alone, through the stage count that population already sets,
never national traits: up to 3 stages 0.85-1.15, 4-9 stages 1.00-1.35, and 10
or more 1.20-1.60. Within its band a stable hash picks the value,
`(lo + fnv1a64("difficulty:" + id) % (hi - lo + 1)) / 100`, so Luxembourg is
0.92, Germany 1.04, Austria 0.85 and Bangladesh 1.60 (242 countries, 50
distinct values, mean 1.049). A deployment multiplies enemy health and reward
by the difficulty and enemy speed by its square root. The campaign's start
country plays clamped to 0.95-1.05 (`Country::effective_difficulty`, shown by
`Game::effective_country_difficulty` on the map and the briefing), so no first
defense opens far easier or harder than the rest; before a start is chosen
every country shows its clamped value. Every other country plays its raw value,
and the front order (`fronts_opened_by`), the recommended next front
(`Campaign::recommendation`, the easiest enterable uncleared country) and
pacing's routes read the raw value too. Nothing recommends a starting country.
The clamp is derived, never saved.

## Unlock and clear contract

A fresh campaign accepts any catalogue entry as its starting country exactly
once. Completing the local stages in order clears the country and unlocks all
of its connected neighbors. Cleared countries and previously unlocked bordering
countries remain visitable. Replaying an earlier stage does not increment the
country ledger or repeat its clear transaction. This state serializes alongside
campaign progression, never as a second independent save generation.

Shared border edges from the 1:50m geometry produce the land adjacency graph.
Explicit microstate borders supplement generalized cartography. Symmetric
**sea routes** connect islands and otherwise disconnected components, so a
campaign beginning anywhere can eventually reach every map region. Sea routes
are marked separately in `sea_neighbors` and are fictional game travel links;
they are not claims about international borders or maritime jurisdiction.

To reproduce the catalogue with only Python's standard library:

```powershell
python -I -B scripts/generate_world.py --check
python -I -B scripts/generate_world.py
python -I -B scripts/generate_world.py --rebuild --download
```

The default (`--from-existing`) is offline: it loads the committed
`world.json`, recomputes only each country's `difficulty` and writes the file
back as indented UTF-8 JSON; every other key, `stage_themes` and
`strategy_config` included, is carried over unchanged. `--check` writes nothing
and exits 1 when the output differs from the committed bytes. `--rebuild`
rebuilds the countries from the Natural Earth 110m and 50m files, given as two
positional paths or fetched with `--download`. It refuses source bytes that do
not match the sha256 pins in `sources` (exit 3; `--allow-source-drift` accepts
and re-pins them), a committed country id that vanishes or is renamed (exit 5;
ids are saved, so no flag accepts it) and any change to a country's stage
count (exit 4; `--allow-stage-count-change`). Committed countries keep their
order and a new one is appended after them. It keeps the top-level
configuration and each country's `level_override`, `stage_theme_ids` and any
other key the generator does not build. `--self-test` exercises every mode on
throwaway files. The rebuild computes adjacency before simplifying display
coordinates, checks connectivity during generation, and emits stable IDs and
sorted neighbors.
Change `difficulty` only through the generator, and review the diff of any
regeneration before committing it.

Tests cover source catalogue completeness, microstate presence, configuration
and overrides, arbitrary starting countries, neighbor gating, replay safety,
connected island routes, malformed content rejection, and save round trips.

## Strategic campaign configuration

`strategy_config` in the same JSON defines the progressive control expansion:
recruitment after one liberated country, parallel deployments after two, grand
strategy after four. Defaults permit four parallel battles, twelve roster
members and three guards per country, with a 100-coin recruitment base cost
that scales with the threat rank (see STRATEGY.md). Outbreak checks occur every
600 simulated seconds with a 35% configured chance and a
420-second response window. Guard suppression is configured at 30 percentage
points per assigned guard; the strategy module resolves staffing and threats.
All thresholds, timing, costs, limits and probabilities are editable data.

`WorldProgress::reopen` handles a lost country by reopening its final operation.
Clearing that operation liberates it again. Previously earned travel routes
remain unlocked; historical campaign first-clear rewards remain owned by the
campaign's separate transaction ledger. Reopening is idempotent, and its ledger
is included in save validation. Strategy simulation and guard behavior are
implemented separately from the geographical content and map renderer.


## Configurable local stage dressing

The world catalogue now contains `stage_themes`, ten authored scene records with `id`, `title`, `setting`, `weather`, `lighting` and `props`. Locations include service road, evacuation suburb, freight depot, industrial yard, quarantine checkpoint, farmland, railway, waterfront, market and highway maintenance. A country may set `stage_theme_ids` to a sequence of these IDs; omitted lists use a deterministic two-variant-per-encounter rotation. These records do not change encounter rewards or difficulty silently.

`Country::stage_theme(stage)` returns the validated record combined with that country's biome and regional cultural setting, plus a stable variation seed. The title and props are authoritative presentation tags; the renderer chooses how to depict them. The variation depends on country ID and local stage, never wall-clock time or future combat dice. Replaying a stage therefore keeps its location identity.

A runtime override can supply, for example, `"stage_theme_ids": ["waterfront", "market", "railway"]` on a country while editing the corresponding global scene records. These are original scenario locations and weather/lighting descriptions, not researched claims about individual national architecture.

Authoring validation requires a nonempty, connected graph of at most 512 countries/regions, with explicit symmetric border or sea connections. Every allowed starting country must reach the full configured campaign. Empty catalogues, invalid center coordinates, missing theme IDs, duplicate profiles and disconnected islands fail before deployment with a reason. Local stage counts are bounded at 256, preserving a finite country operation. The bundled 242-region geography, source fingerprints, populations and numerical combat modifiers are unchanged by this dressing pass.


External world and encounter files are limited to 32 MiB before parsing. World geometry is limited to 200,000 total points and 2,048 points per ring, preventing an accidental huge override from monopolizing the native map's triangulation cache. Country titles, regional strings, theme props and dossier modifier lists are bounded as well. The bundled world is about 1.1 MiB with 12,348 points and a largest ring of 556 points; none of its geometry was simplified or removed by this validation change.
