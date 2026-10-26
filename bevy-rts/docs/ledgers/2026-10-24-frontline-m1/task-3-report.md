# M1 Task 3 report: terrain content format

**Status:** DONE_WITH_CONCERNS

**Commit:** 64d15be `feat(fl_content)!: terrain kind, PNG map layers through a sidecar hook, MoveClass in fl_content`
- Trailer: `Refs: ADR-0014`.
- No `Blessed:` trailer yet. Nothing is blessed; the lead adds the trailer with the bless commit.

**Base:** `.gate-base` is d66f685. While I worked, the controller added two commits to m1/phase-a:
- 5844337: bless Task 2.
- f1eff2f: plan doc.

My commit sits on f1eff2f.

## What I implemented

**`fl_content/src/terrain.rs`**
- `MoveClass { Foot, Wheeled, HalfTrack, Tracked }`. It derives `Ord` and `Hash`, and has `ALL` and `index()` (declaration order).
- `TerrainDef` (`*.terrain.ron`): rules-hashed (`IN_RULES_HASH`) and namespaced.
- Link checks:
  - `pass` must lie in [0, 1]; reported at `pass`.
  - A class listed twice in `blocks` is an error; reported at `blocks`.
- Crate-private `MAX_TERRAIN_KINDS = 255`.

**`fl_content/src/layers.rs`**
- `MapTerrain`, `MapLayers` (with `index`), `decode_layers` and `write_layers`.
- The decoder uses `Transformations::IDENTITY` and ignores the iCCP and text chunks (`set_ignore_iccp_chunk`, `set_ignore_text_chunk`).
- Accepted formats are exactly ADR-0014's:
  - terrain PNG: 8-bit RGB, or RGBA with every alpha 255;
  - height PNG: 8-bit gray (value v counts as v × 257) or 16-bit gray.
- Error order and messages:
  1. Format: `"<file>: unsupported PNG format: colour type <ColorType:?>, bit depth <n> (expected …)"`.
  2. Size: `"<file>: expected WxH pixels, found wxh"`.
  3. Legend lookup: `"<file>: pixel (px, py) colour #rrggbb is not in the legend"`.
  4. Alpha: an RGBA pixel whose alpha isn't 255.
- Pixel (px, py) is cell (px, cells_y − 1 − py).
- `write_layers` is the inverse: RGB8 for types and 16-bit gray for heights. Each palette id is written in its first legend colour.

**`builtin.rs`**
- `MapDef` gains `#[serde(default)] terrain` and `#[serde(skip_deserializing, default)] layers`.
- A RON file that sets `layers` gets serde's unknown-field error, which names `layers`.
- `MapDef::load_sidecars` decodes the PNGs next to the map file into a grid of `size_m / cell_m` cells.
  - It skips decoding when the size, the cell side or the legend's colours are invalid. The link pass reports those problems, so each is reported once.
- `MapDef::link` reports two errors, both at `terrain.legend[i]`:
  - a duplicate RGB, at the second entry;
  - a legend id that isn't a `TerrainDef`.

**`kinds.rs`**
- New trait hooks, both with default no-op bodies:
  - `ContentKind::load_sidecars(&mut self, _file) -> Result<(), String>`;
  - `RulesSection::link(&self, _ctx)`.
- Type-erased plumbing for both.

**`mods.rs` and `db.rs`**
- Sidecar hook:
  - The loader calls `load_sidecars` right after it parses each kind item.
  - An `Err` becomes `ContentError::at(file, <item id>, msg)`.
- Rules-section link pass:
  - Each rules section now remembers which `rules.ron` defined it (`RulesFiles`).
  - The link pass calls every section's `link` with scope `<Section>`, so errors appear at `"<Section>/<field>"`.
- Kind cap: `KindEntry.max_items` produces `"256 terrain kinds exceed the limit of 255"` at the 256th item's file and id.

**`registry.rs`**
- `with_builtins` registers `TerrainDef` with the 255 cap, through a private `register_kind_capped`.

**`fl_sim`**
- The `MoveClass` enum is removed from `units.rs`; `lib.rs` now has `pub use fl_content::MoveClass`.
- The `SimHash` impl stays in `units.rs`.
- The `trace_enum::<MoveClass>()` codec is unchanged, and so is the hash stream.

**`xtask/src/arch.rs`**
- `check_edges` flags a direct `png` dependency (normal, dev or build) in any crate whose role is not `Content`.
- Message: `"<crate> must not depend on png directly: only fl_content decodes PNG map layers (ADR-0014)"`.

**Dependencies**
- `Cargo.toml`: `png = "=0.18.1"`, an exact pin as the dispatch asked (the brief says `"0.18"`). It was already in the lock, so nothing was downloaded.
- `fl_content`: `png.workspace`, plus dev-dependencies `proptest` and `tempfile`.
- `Cargo.lock`: only those three dependency names were added to `fl_content`'s entry.

## Tests

**`crates/fl_content/tests/terrain.rs`**
- All 17 tests from the brief, plus `move_class_order_and_index`.
- Mods are written into temp dirs; no binary fixture is committed.

**`xtask` unit test `png_direct_dep_only_in_content`**
- `png` in `fl_content` passes.
- `png` in `fl_sim`, `fl_cli`, `fl_client` and `fl_test_utils` (dev) is flagged.
- `check_tree` doesn't flag a transitive `png`.

### TDD evidence

**RED 1:** `cargo nextest run --profile agent -p fl_content --test terrain`
```
error[E0432]: unresolved imports `fl_content::MapLayers`, `fl_content::MapTerrain`, `fl_content::MoveClass`, `fl_content::TerrainDef`, `fl_content::decode_layers`, `fl_content::write_layers`
error[E0407]: method `link` is not a member of trait `RulesSection`
error[E0609]: no field `layers` on type `&MapDef`   (x3)
error[E0609]: no field `terrain` on type `&MapDef`  (x2)
```
Expected: none of these API items existed yet.

**RED 2:** `cargo nextest run --profile agent -p xtask -E 'test(/png_direct/)'`
```
assertion `left == right` failed: fl_sim -> png (None)
  left: []
 right: ["fl_sim must not depend on png directly: only fl_content decodes PNG map layers (ADR-0014)"]
```
Expected: there was no png rule yet.

**GREEN**
| Command | Result |
|---|---|
| `cargo nextest run --profile agent -p fl_content` | 76 tests run: 76 passed (re-run after the final edit) |
| `cargo nextest run --profile agent -p xtask -E 'test(/png_direct/)'` | 1 passed |
| Step 4: `cargo nextest run --profile agent -p fl_content -p fl_sim -p fl_cli -p fl_catalog` | 336 tests run: 336 passed |
| `cargo nextest run --profile agent -p fl_test_utils` | 22/23 passed. The one failure is `golden_replays_verify`, `Err(Header(RulesHash))`, which the brief expects. |

My first GREEN run found a bug in my own test: the RGBA case ran while the 4-bit height PNG from the previous case was still on disk. I fixed the test.

### Other checks
- `cargo deny check`: advisories ok, bans ok, licenses ok, sources ok.
- `cargo xtask arch-check`: `arch-check: OK (12 packages)`.
- `cargo clippy -p fl_content --all-targets -- -D warnings`: clean.
- `cargo fmt --check`: clean.

## Gate after commit 64d15be

Command: `cargo xtask gate task --track contract --track content --track sim-core`. The output is appended at the end of this report.

Result: `gate task: FAIL (check-scope, protected, test-headless, api) in 683s`. None of these failures comes from a defect in this task:

| Row | Cause |
|---|---|
| check-scope | The only out-of-track file is `docs/superpowers/plans/2026-10-24-frontline-m1.md`, from controller commit f1eff2f (after `.gate-base` d66f685). |
| protected | All 4 files come from controller commit 5844337 (the Task 2 bless). This task changes no protected file. |
| test-headless | The only failure is `golden_replays_verify` (rules hash). The re-recorded `.replay.new` is waiting for bless. |
| api | `fl_content` and `fl_sim` changed. Their `.txt.new` files are written and waiting for bless. |

**test-guard WARN:** `crates/fl_content/tests/load.rs` needed two expected lists updated, because the new built-in kind `terrain` joins them:
- the registry's `content_kinds` list;
- the `content.*` string-table list.

This is a mechanical change to a non-protected test outside my Files list. The reviewer must accept it.

All other rows pass: fmt, clippy-drift, claude-md, clippy, arch-check, test-client and data-check.

After the bless, every row should pass except check-scope and protected, which the controller's own commits trigger.

## Protected artifacts for the lead

Suggested bless reason: `M1 Task 3: MoveClass moved to fl_content; MapDef gains terrain layers (rules hash)`.

**`xtask/snapshots/api/fl_content.txt.new`.** It adds:
- MoveClass, TerrainDef, MapTerrain, MapLayers (with `index`);
- decode_layers, write_layers;
- MapDef.terrain and MapDef.layers;
- ContentKind::load_sidecars, plus its default on every built-in kind;
- RulesSection::link.

The full diff is appended at the end of this report.

**`xtask/snapshots/api/fl_sim.txt.new`.** It changes:
- `pub use fl_sim::MoveClass` replaces the enum;
- field types now print as `fl_content::terrain::MoveClass`;
- the SimHash impl line moves.

The full diff is appended at the end of this report.

**`crates/fl_test_utils/tests/replays/testbed_wander.replay.new`.** I compared it with the old replay using `fl_cli replay dump --json`:
- All 400 per-tick state hashes are identical.
- Header changes:
  - `rules_hash`: 13586675111922715448 → 4288563367055306475.
  - `string_tables.content_kinds` gains `terrain`, and there is a new empty table `content.terrain`.
  - `build_id` differs.
  - `sim_resources` gains `core.classified_up_to` and `core.debug_queries`. These come from earlier M0 and Task 2 work, not from this task.

## Files changed
- `Cargo.toml`, `Cargo.lock`
- `crates/fl_content/Cargo.toml`
- `crates/fl_content/src/{builtin,db,kinds,lib,mods,registry}.rs`
- New: `crates/fl_content/src/{terrain,layers}.rs`
- New: `crates/fl_content/tests/terrain.rs`
- `crates/fl_content/tests/load.rs` (mechanical)
- `crates/fl_sim/src/{units,lib}.rs`
- `xtask/src/arch.rs`

## Concerns and self-review notes

1. **Edits outside the Files list.**
   - `tests/load.rs`: the expected-list update described under the gate table; test-guard flags it.
   - `db.rs`: holds the rules-section link pass and the `RulesFiles` map, because the link pass lives there.
   - `proptest` as a `fl_content` dev-dependency: the brief requires a proptest round-trip but lists only `tempfile`. Proptest is a workspace dependency already in the lock, so nothing was downloaded.

2. **The `png` pin is exact (`=0.18.1`).** I followed the dispatch rather than the brief's `"0.18"`.

3. **I chose the path for sidecar errors.** An `Err` from `load_sidecars` is reported at path `<item id>` on the map file, because the brief only says "a ContentError on `_file`". The message names the PNG file, plus the pixel where there is one.

4. **The 255-kind cap is a general mechanism.** It is a per-kind `max_items`, set privately in `with_builtins`, so it adds no public API. Over the cap, the link pass still runs; it is skipped only over the unit cap.

5. **Decoding is skipped on a size or legend error.** When `load_sidecars` skips decoding for an invalid size or a duplicate legend colour, `layers` stays `None`. The load fails anyway on the link error.

6. **Gate time.** The gate took 618 s and 683 s, against a 300 s budget.

---

## Appendix A: gate output after commit 64d15be (trimmed)

```
PASS fmt (4.6s)
PASS clippy-drift (5.6s)
PASS claude-md (0.0s)
FAIL check-scope (0.6s)
    1 of 24 changed files are outside track(s) contract, content, sim-core:
      docs/superpowers/plans/2026-10-24-frontline-m1.md → docs
FAIL protected (0.5s)
    4 violations among 4 changed protected files:
      crates/fl_protocol/tests/snapshots/formats__protocol_formats.snap: changed protected path; only the lead promotes it with `cargo xtask bless`
      xtask/snapshots/api/fl_protocol.txt: changed protected path; only the lead promotes it with `cargo xtask bless`
      xtask/snapshots/api/fl_sim.txt: changed protected path; only the lead promotes it with `cargo xtask bless`
      xtask/snapshots/api/fl_view.txt: changed protected path; only the lead promotes it with `cargo xtask bless`
WARN test-guard (2.1s): test-guard: 1 findings (reviewer must accept each)
    TEST-GUARD crates/fl_content/tests/load.rs:387 RemovedAssert: assert_eq!( names(tables::CONTENT_KINDS), ["faction", "map", "scenario", "visual"] );
PASS clippy (46.7s)
PASS arch-check (8.4s)
FAIL test-headless (299.4s)
         Summary [  93.408s] 563 tests run: 562 passed, 1 failed, 2 skipped
        thread 'golden_replays_verify' (27796) panicked at crates\fl_test_utils\tests\golden.rs:43:18:
        C:\fl\a\crates\fl_test_utils\tests/replays\testbed_wander.replay does not verify: Err(Header(RulesHash))
    error: test run failed
    (full log: target\xtask\test-headless.log)
PASS test-client (124.9s)
FAIL api (123.5s)
    api: 2 of 5 crates changed or new
      fl_content: changed
      fl_sim: changed
    run `cargo xtask api` to write the .txt.new files for the lead to bless
PASS data-check (65.0s)
WARN gate task took 683s (budget 300s)
gate task: FAIL (check-scope, protected, test-headless, api) in 683s
```

## Appendix B: fl_sim API diff (`diff fl_sim.txt fl_sim.txt.new`)

```
1a2
> pub use fl_sim::MoveClass
164,183d164
< pub enum fl_sim::MoveClass
< pub fl_sim::MoveClass::Foot
< pub fl_sim::MoveClass::HalfTrack
< pub fl_sim::MoveClass::Tracked
< pub fl_sim::MoveClass::Wheeled
< impl core::clone::Clone for fl_sim::MoveClass
< pub fn fl_sim::MoveClass::clone(&self) -> Self
< impl core::cmp::Eq for fl_sim::MoveClass
< impl core::cmp::PartialEq for fl_sim::MoveClass
< pub fn fl_sim::MoveClass::eq(&self, &Self) -> bool
< impl core::fmt::Debug for fl_sim::MoveClass
< pub fn fl_sim::MoveClass::fmt(&self, &mut core::fmt::Formatter<'_>) -> core::fmt::Result
< impl core::marker::Copy for fl_sim::MoveClass
< impl core::marker::StructuralPartialEq for fl_sim::MoveClass
< impl fl_sim::SimHash for fl_sim::MoveClass
< pub fn fl_sim::MoveClass::sim_hash(&self, &mut fl_sim::StateHasher)
< impl serde_core::ser::Serialize for fl_sim::MoveClass
< pub fn fl_sim::MoveClass::serialize<__S>(&self, __S) -> core::result::Result<<__S as serde_core::ser::Serializer>::Ok, <__S as serde_core::ser::Serializer>::Error> where __S: serde_core::ser::Serializer
< impl<'de> serde_core::de::Deserialize<'de> for fl_sim::MoveClass
< pub fn fl_sim::MoveClass::deserialize<__D>(__D) -> core::result::Result<Self, <__D as serde_core::de::Deserializer>::Error> where __D: serde_core::de::Deserializer<'de>
823c804
< pub fl_sim::Mobility::class: fl_sim::MoveClass
---
> pub fl_sim::Mobility::class: fl_content::terrain::MoveClass
849c830
< pub fl_sim::MobilityDef::class: fl_sim::MoveClass
---
> pub fl_sim::MobilityDef::class: fl_content::terrain::MoveClass
1587a1569,1570
> impl fl_sim::SimHash for fl_content::terrain::MoveClass
> pub fn fl_content::terrain::MoveClass::sim_hash(&self, &mut fl_sim::StateHasher)
1618,1619d1600
< impl fl_sim::SimHash for fl_sim::MoveClass
< pub fn fl_sim::MoveClass::sim_hash(&self, &mut fl_sim::StateHasher)
```

## Appendix C: fl_content API diff (`diff fl_content.txt fl_content.txt.new`)

```
1a2,28
> pub enum fl_content::MoveClass
> pub fl_content::MoveClass::Foot
> pub fl_content::MoveClass::HalfTrack
> pub fl_content::MoveClass::Tracked
> pub fl_content::MoveClass::Wheeled
> impl fl_content::MoveClass
> pub const fl_content::MoveClass::ALL: [fl_content::MoveClass; 4]
> pub fn fl_content::MoveClass::index(self) -> usize
> impl core::clone::Clone for fl_content::MoveClass
> pub fn fl_content::MoveClass::clone(&self) -> Self
> impl core::cmp::Eq for fl_content::MoveClass
> impl core::cmp::Ord for fl_content::MoveClass
> pub fn fl_content::MoveClass::cmp(&self, &Self) -> core::cmp::Ordering
> impl core::cmp::PartialEq for fl_content::MoveClass
> pub fn fl_content::MoveClass::eq(&self, &Self) -> bool
> impl core::cmp::PartialOrd for fl_content::MoveClass
> pub fn fl_content::MoveClass::partial_cmp(&self, &Self) -> core::option::Option<core::cmp::Ordering>
> impl core::fmt::Debug for fl_content::MoveClass
> pub fn fl_content::MoveClass::fmt(&self, &mut core::fmt::Formatter<'_>) -> core::fmt::Result
> impl core::hash::Hash for fl_content::MoveClass
> pub fn fl_content::MoveClass::hash<__H: core::hash::Hasher>(&self, &mut __H)
> impl core::marker::Copy for fl_content::MoveClass
> impl core::marker::StructuralPartialEq for fl_content::MoveClass
> impl serde_core::ser::Serialize for fl_content::MoveClass
> pub fn fl_content::MoveClass::serialize<__S>(&self, __S) -> core::result::Result<<__S as serde_core::ser::Serializer>::Ok, <__S as serde_core::ser::Serializer>::Error> where __S: serde_core::ser::Serializer
> impl<'de> serde_core::de::Deserialize<'de> for fl_content::MoveClass
> pub fn fl_content::MoveClass::deserialize<__D>(__D) -> core::result::Result<Self, <__D as serde_core::de::Deserializer>::Error> where __D: serde_core::de::Deserializer<'de>
131a159
> pub fn fl_content::Faction::load_sidecars(&mut self, &std::path::Path) -> core::result::Result<(), alloc::string::String>
154a183
> pub fl_content::MapDef::layers: core::option::Option<fl_content::MapLayers>
156a186
> pub fl_content::MapDef::terrain: core::option::Option<fl_content::MapTerrain>
169a200
> pub fn fl_content::MapDef::load_sidecars(&mut self, &std::path::Path) -> core::result::Result<(), alloc::string::String>
173a205,242
> pub struct fl_content::MapLayers
> pub fl_content::MapLayers::cells_x: u32
> pub fl_content::MapLayers::cells_y: u32
> pub fl_content::MapLayers::heights: alloc::vec::Vec<u16>
> pub fl_content::MapLayers::palette: alloc::vec::Vec<alloc::string::String>
> pub fl_content::MapLayers::types: alloc::vec::Vec<u8>
> impl fl_content::MapLayers
> pub fn fl_content::MapLayers::index(&self, u32, u32) -> usize
> impl core::clone::Clone for fl_content::MapLayers
> pub fn fl_content::MapLayers::clone(&self) -> Self
> impl core::cmp::Eq for fl_content::MapLayers
> impl core::cmp::PartialEq for fl_content::MapLayers
> pub fn fl_content::MapLayers::eq(&self, &Self) -> bool
> impl core::default::Default for fl_content::MapLayers
> pub fn fl_content::MapLayers::default() -> Self
> impl core::fmt::Debug for fl_content::MapLayers
> pub fn fl_content::MapLayers::fmt(&self, &mut core::fmt::Formatter<'_>) -> core::fmt::Result
> impl core::marker::StructuralPartialEq for fl_content::MapLayers
> impl serde_core::ser::Serialize for fl_content::MapLayers
> pub fn fl_content::MapLayers::serialize<__S>(&self, __S) -> core::result::Result<<__S as serde_core::ser::Serializer>::Ok, <__S as serde_core::ser::Serializer>::Error> where __S: serde_core::ser::Serializer
> impl<'de> serde_core::de::Deserialize<'de> for fl_content::MapLayers
> pub fn fl_content::MapLayers::deserialize<__D>(__D) -> core::result::Result<Self, <__D as serde_core::de::Deserializer>::Error> where __D: serde_core::de::Deserializer<'de>
> pub struct fl_content::MapTerrain
> pub fl_content::MapTerrain::height_m: (fl_math::fx::Fx, fl_math::fx::Fx)
> pub fl_content::MapTerrain::height_png: alloc::string::String
> pub fl_content::MapTerrain::legend: alloc::vec::Vec<((u8, u8, u8), alloc::string::String)>
> pub fl_content::MapTerrain::terrain_png: alloc::string::String
> impl core::clone::Clone for fl_content::MapTerrain
> pub fn fl_content::MapTerrain::clone(&self) -> Self
> impl core::cmp::PartialEq for fl_content::MapTerrain
> pub fn fl_content::MapTerrain::eq(&self, &Self) -> bool
> impl core::fmt::Debug for fl_content::MapTerrain
> pub fn fl_content::MapTerrain::fmt(&self, &mut core::fmt::Formatter<'_>) -> core::fmt::Result
> impl core::marker::StructuralPartialEq for fl_content::MapTerrain
> impl serde_core::ser::Serialize for fl_content::MapTerrain
> pub fn fl_content::MapTerrain::serialize<__S>(&self, __S) -> core::result::Result<<__S as serde_core::ser::Serializer>::Ok, <__S as serde_core::ser::Serializer>::Error> where __S: serde_core::ser::Serializer
> impl<'de> serde_core::de::Deserialize<'de> for fl_content::MapTerrain
> pub fn fl_content::MapTerrain::deserialize<__D>(__D) -> core::result::Result<Self, <__D as serde_core::de::Deserializer>::Error> where __D: serde_core::de::Deserializer<'de>
244a314
> pub fn fl_content::ScenarioDef::load_sidecars(&mut self, &std::path::Path) -> core::result::Result<(), alloc::string::String>
285a356,379
> pub struct fl_content::TerrainDef
> pub fl_content::TerrainDef::blocks: alloc::vec::Vec<fl_content::MoveClass>
> pub fl_content::TerrainDef::color: (f32, f32, f32)
> pub fl_content::TerrainDef::id: alloc::string::String
> pub fl_content::TerrainDef::name: alloc::string::String
> pub fl_content::TerrainDef::pass: fl_math::fx::Fx
> impl core::clone::Clone for fl_content::TerrainDef
> pub fn fl_content::TerrainDef::clone(&self) -> Self
> impl core::cmp::PartialEq for fl_content::TerrainDef
> pub fn fl_content::TerrainDef::eq(&self, &Self) -> bool
> impl core::fmt::Debug for fl_content::TerrainDef
> pub fn fl_content::TerrainDef::fmt(&self, &mut core::fmt::Formatter<'_>) -> core::fmt::Result
> impl core::marker::StructuralPartialEq for fl_content::TerrainDef
> impl fl_content::ContentKind for fl_content::TerrainDef
> pub const fl_content::TerrainDef::EXT: &'static str
> pub const fl_content::TerrainDef::IN_RULES_HASH: bool
> pub const fl_content::TerrainDef::NAMESPACED: bool
> pub fn fl_content::TerrainDef::id(&self) -> &str
> pub fn fl_content::TerrainDef::link(&self, &mut fl_content::LinkCtx<'_>)
> pub fn fl_content::TerrainDef::load_sidecars(&mut self, &std::path::Path) -> core::result::Result<(), alloc::string::String>
> impl serde_core::ser::Serialize for fl_content::TerrainDef
> pub fn fl_content::TerrainDef::serialize<__S>(&self, __S) -> core::result::Result<<__S as serde_core::ser::Serializer>::Ok, <__S as serde_core::ser::Serializer>::Error> where __S: serde_core::ser::Serializer
> impl<'de> serde_core::de::Deserialize<'de> for fl_content::TerrainDef
> pub fn fl_content::TerrainDef::deserialize<__D>(__D) -> core::result::Result<Self, <__D as serde_core::de::Deserializer>::Error> where __D: serde_core::de::Deserializer<'de>
304a399
> pub fn fl_content::VisualDef::load_sidecars(&mut self, &std::path::Path) -> core::result::Result<(), alloc::string::String>
316a412
> pub fn fl_content::ContentKind::load_sidecars(&mut self, &std::path::Path) -> core::result::Result<(), alloc::string::String>
322a419
> pub fn fl_content::Faction::load_sidecars(&mut self, &std::path::Path) -> core::result::Result<(), alloc::string::String>
328a426
> pub fn fl_content::MapDef::load_sidecars(&mut self, &std::path::Path) -> core::result::Result<(), alloc::string::String>
334a433,440
> pub fn fl_content::ScenarioDef::load_sidecars(&mut self, &std::path::Path) -> core::result::Result<(), alloc::string::String>
> impl fl_content::ContentKind for fl_content::TerrainDef
> pub const fl_content::TerrainDef::EXT: &'static str
> pub const fl_content::TerrainDef::IN_RULES_HASH: bool
> pub const fl_content::TerrainDef::NAMESPACED: bool
> pub fn fl_content::TerrainDef::id(&self) -> &str
> pub fn fl_content::TerrainDef::link(&self, &mut fl_content::LinkCtx<'_>)
> pub fn fl_content::TerrainDef::load_sidecars(&mut self, &std::path::Path) -> core::result::Result<(), alloc::string::String>
340a447
> pub fn fl_content::VisualDef::load_sidecars(&mut self, &std::path::Path) -> core::result::Result<(), alloc::string::String>
345a453,454
> pub fn fl_content::RulesSection::link(&self, &mut fl_content::LinkCtx<'_>)
> pub fn fl_content::decode_layers(&fl_content::MapTerrain, &std::path::Path, (u32, u32)) -> core::result::Result<fl_content::MapLayers, alloc::string::String>
348a458
> pub fn fl_content::write_layers(&fl_content::MapTerrain, &std::path::Path, &fl_content::MapLayers) -> core::io::error::Result<()>
```

---

# Fix round 1 (review task-3-review.md; rulings R89, R90)

**Commit:** dc0620e `fix(fl_content)!: bound map grids, confine layer paths, locate sidecar errors at the field` (`Refs: ADR-0014`).
**Gate base:** 2f579f6.

## Changes

- **Important 1 / R89: map grids are bounded.**
  - Crate-private `MAX_MAP_CELLS = 4096` per side, in `layers.rs`.
  - One shared check, `MapDef::grid() -> Result<(u32, u32), Vec<(field, msg)>>`, now holds the size and cell-side rules that were split across `link` and `cells()`. This fixes Minor 3.
    - `link` reports each error it returns, at `cell_m` or `size_m`.
    - `load_sidecars` decodes only when it returns `Ok`.
    - The message is e.g. `x side 8 m is 8192 cells of 0.0009765625 m; at most 4096 cells per side`.
  - `decode_layers` also rejects grids outside `1..=4096` per side before it touches a file.
  - `types` is allocated only after the terrain PNG's header matched the grid size, and `heights` only after the height PNG's header did. `read_png`'s own buffer is allocated after its size check, which was already the case.
- **Minor 1 / R90: layer paths are confined.**
  - `check_png_name`: every path component must be `Normal` or `CurDir`, with at least one `Normal`. Absolute paths, drive prefixes, `..` and the empty string are rejected.
  - Where it is enforced:
    - `MapDef::link`, as an error at `terrain.height_png` or `terrain.terrain_png`;
    - `load_sidecars`, which skips decoding so the error is reported once;
    - `decode_layers` (error prefixed with the field);
    - `write_layers` (`InvalidInput`).
  - Subdirectories below the map's directory still work, e.g. `layers/t.png` and `./layers/h.png`.
- **Minor 2: sidecar errors are reported at the field.**
  - The hook's signature changed: `ContentKind::load_sidecars(&mut self, &Path) -> Result<(), (String, String)>`, an error being `(field, message)`.
  - The loader reports it at `<id>/<field>` (`<id>` when the field is empty), e.g. `t.map/terrain.terrain_png` or `t.map/terrain.height_png`.
  - `decode_layers` (public) still returns a `String`, now prefixed with the `MapTerrain` field (`terrain_png: …`, `legend[1]: …`).
- **Minor 4:** a doc warning on `MapDef.layers` explains that its serialization is asymmetric and that a `MapDef` must never be stored in postcard and read back.
- **Minor 5:** the decoder calls `decoder.set_transformations(png::Transformations::IDENTITY)` explicitly.
- **Minor 6: more test coverage.**
  - `rules_hash_changes_with_one_pixel` now checks four variants against a base mod: identical (equal hash), one terrain pixel, one height sample and one legend colour (each a different hash).
  - `unsupported_png_formats_are_errors` adds height PNGs that are gray-alpha 8, gray-alpha 16 and indexed 8.
- **Minor 7:** the arch test comment is reworded. The test now checks that the tree rules accept `png` and `fdeflate` in both the `fl_content` and `fl_sim` trees.
- **Minor 8 (report correction):** my first report gave the error order as format → size → legend → alpha. In fact, each pixel's alpha is checked before its legend lookup.
- **Minor 9:** the test helper `write_png`'s doc now says it writes any colour type and depth.

New tests:
- `grid_over_4096_cells_per_side_is_error`:
  - An 8 m side with `cell_m: 0.0009765625` (an exact divisor, 8192 cells) gives two errors, both at `t.map/cell_m`, and no PNG error, so no decode ran.
  - `cell_m: 0.0001` gives an error at `t.map/cell_m`.
  - A 4096-cell side loads.
- `decode_layers_rejects_oversized_grids`: grids (4097, 1), (1, 4097) and (0, 1).
- `png_paths_must_stay_next_to_the_map`:
  - `../t.png`, `sub/../../t.png`, an absolute height path and `""` are each one error at the field, and `decode_layers` refuses them too.
  - A subdirectory works.
- Field locations added to `layer_size_mismatch_is_error` (terrain and height) and `unknown_legend_colour_is_error`.

## TDD evidence

**RED:** `cargo nextest run --profile agent -p fl_content --test terrain`, with the tests above and the old code:
```
FAIL fl_content::terrain layer_size_mismatch_is_error        no error at "t.map/terrain.terrain_png" containing "expected 4x3 pixels, found 4x2"
FAIL fl_content::terrain decode_layers_rejects_oversized_grids   (no "4096" error)
FAIL fl_content::terrain grid_over_4096_cells_per_side_is_error  no error at "t.map/cell_m" containing "8192 cells"
FAIL fl_content::terrain unknown_legend_colour_is_error      no error at "t.map/terrain.terrain_png" containing "colour #ff00ff"
FAIL fl_content::terrain png_paths_must_stay_next_to_the_map no error at "t.map/terrain.terrain_png" containing "relative"
Summary 21 tests run: 16 passed, 5 failed
```
The extended rules-hash test and the new height formats passed on the old code: they add coverage, they don't test new behaviour.

**GREEN:**
- `cargo nextest run --profile agent -p fl_content`: 79 tests run, 79 passed.
- `cargo nextest run --profile agent -p xtask -E 'test(/png_direct/)'`: 1 passed.
- `cargo clippy -p fl_content -p xtask --all-targets -- -D warnings`: clean.

## Protected artifacts

- `xtask/snapshots/api/fl_content.txt.new`: regenerated. Compared with the round-0 `.new`, the only change is the `load_sidecars` return type, `Result<(), String>` → `Result<(), (String, String)>`, on the trait and on every kind (12 lines). No new public items.
- `xtask/snapshots/api/fl_sim.txt.new`: byte-identical to round 0.
- `crates/fl_test_utils/tests/replays/testbed_wander.replay.new`: **not changed.** The fix changes nothing that enters the rules hash for base content. `fl_cli replay-verify testbed_wander.replay.new` gives `verified: 400 ticks`, with only a build-id warning.
- All three are untracked and unblessed. The bless reason is unchanged.

## Gate after dc0620e

Command: `cargo xtask gate task --track contract --track content --track sim-core`.

```
PASS fmt (8.6s)
PASS clippy-drift (5.2s)
PASS claude-md (0.0s)
PASS check-scope (0.6s)
PASS protected (0.5s)
WARN test-guard (5.3s): test-guard: 3 findings (reviewer must accept each)
    TEST-GUARD crates/fl_content/tests/terrain.rs:395 RemovedAssert: assert_eq!(hash(false), hash(false));
    TEST-GUARD crates/fl_content/tests/terrain.rs:396 RemovedAssert: assert_ne!(hash(false), hash(true));
    TEST-GUARD xtask/src/arch.rs:519 RemovedAssert: assert_eq!(check_tree("fl_content", "png v0.18.1").len(), 0);
PASS clippy (130.6s)
PASS arch-check (15.4s)
FAIL test-headless (233.4s)
         Summary [  16.383s] 195 tests run: 194 passed, 1 failed, 229 skipped
        thread 'golden_replays_verify' (47116) panicked at crates\fl_test_utils\tests\golden.rs:43:18:
        C:\fl\a\crates\fl_test_utils\tests/replays\testbed_wander.replay does not verify: Err(Header(RulesHash))
    error: test run failed
    (full log: target\xtask\test-headless.log)
FAIL api (59.5s)
    api: 2 of 5 crates changed or new
      fl_content: changed
      fl_sim: changed
    run `cargo xtask api` to write the .txt.new files for the lead to bless
PASS data-check (18.2s)
WARN gate task took 481s (budget 300s)
gate task: FAIL (test-headless, api) in 481s
```

Why each non-PASS row is expected:
- **test-headless:** only `golden_replays_verify` fails, waiting for the bless of `testbed_wander.replay.new`.
- **api:** waiting for the bless of the two `.txt.new` files.
- **test-guard: the reviewer must accept 3 findings.** None weakens a check:
  - terrain.rs:395 and :396: the boolean `hash(false)`/`hash(true)` asserts were replaced by the four-variant `Flip` checks.
  - arch.rs:519: the single `check_tree("fl_content", "png v0.18.1")` assert was replaced by the same check over `png` + `fdeflate` for `fl_content` and `fl_sim`.
- check-scope and protected pass this round. The gate took 481 s against a 300 s budget.
