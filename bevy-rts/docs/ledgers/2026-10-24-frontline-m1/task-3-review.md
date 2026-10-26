# M1 Task 3 review: terrain content format (commit 64d15be + untracked `.new`)

Reviewed in one pass over `review-t3.diff` (2422 lines). Out-of-diff checks, one per named risk, are listed under "Checks run".

### Spec Compliance

- ✅ Spec compliant. Every item in the brief's Interfaces block is present with the specified signature and attributes:
  - `MoveClass` with `Ord`/`Hash`/`ALL`/`index`: terrain.rs:14-36.
  - `TerrainDef` with its `pass` and `blocks` link checks: terrain.rs:38-72.
  - `MapTerrain`, `MapLayers::index`, `decode_layers` and `write_layers`: layers.rs.
  - `MapDef.terrain` (`#[serde(default)]`) and `MapDef.layers` (`#[serde(skip_deserializing, default)]`): builtin.rs:68-75.
  - `ContentKind::load_sidecars` (kinds.rs:27-32) and `RulesSection::link` (kinds.rs:38-41).
  - The loader calls the sidecar hook right after parsing (mods.rs:358-367) and runs the rules-section link pass (db.rs:189-191).
  - A 255 cap on terrain kinds (registry.rs:99, mods.rs:499-511).
  - The `png` arch rule (arch.rs:139-182).
  - The brief's 17 tests plus the arch unit test, all present (tests/terrain.rs, arch.rs:488-521).
- ADR-0014 PNG acceptance is exact:
  - The terrain predicate is `Rgb|Rgba && Eight` (layers.rs:~189). The height predicate is `Grayscale && (Eight|Sixteen)` (layers.rs:~215). Both are checked against `info.color_type/bit_depth`, which is the raw IHDR.
  - So indexed, gray-alpha, <8-bit, 16-bit RGB/RGBA, colour height and gray terrain PNGs are all rejected, with file, colour type and bit depth in the message (layers.rs:~103-109).
  - RGBA with alpha ≠ 255 is rejected (layers.rs:~196-202).
  - 8-bit height decodes as v×257, and 16-bit is read big-endian (layers.rs:~222-225).
  - Orientation is `cells_y − 1 − py` (layers.rs:~182).
  - A size mismatch and a colour missing from the legend produce the brief's exact messages.
  - A duplicate legend colour is an error at the second entry, in the link pass (builtin.rs:125-133). The sidecar skips decoding then, so it is reported once (test asserts `errs.len() == 1`).
  - A legend id that isn't a `TerrainDef` is an error at `terrain.legend[i]` (builtin.rs:134-136).
- Extras, all justified:
  - `proptest` dev-dependency: the brief demands a proptest but lists only `tempfile`.
  - `db.rs`: the link pass lives there; +10 lines, minimal.
  - `tests/load.rs`: 2 expected lists gain `terrain`/`content.terrain`. It is a required, mechanical change. Test-guard labels it "RemovedAssert", but it is a modified expectation, not a weakened one. **Accept.**
  - `png = "=0.18.1"`: exact pin, accepted by the lead.
- ⚠️ Cannot verify from the diff alone:
  - The gate rows the report attributes to controller commits (check-scope, protected) are known and accepted.
  - Gate time was 683 s against the 300 s budget. That is a process issue for the controller, not a code finding.

### Strengths

- **Decoding is integer-only and platform-independent.**
  - No float (`#![deny(clippy::float_arithmetic)]` still covers the crate), and no gamma.
  - `png` 0.18.1's `Decoder::new` defaults to `Transformations::IDENTITY` (checked in the registry source, decoder/mod.rs:138), so samples are raw.
  - Lookups use `BTreeMap`, palette order is a byte-wise `String` sort, and 16-bit samples are read with explicit `from_be_bytes`.
  - The decoder never applies gAMA/sRGB/cHRM (the crate has no colour management), and iCCP and text chunks are skipped outright.
- **The rules hash covers every pixel.**
  - hash.rs postcard-serializes every rules-hashed kind item.
  - `MapDef` serializes `terrain` (file names, `height_m`, legend) and `layers` (cells_x, cells_y, palette, every `types` and `heights` entry, with length prefixes).
  - The canonical tree's shape is unchanged (one more `kinds` entry), so `RULES_HASH_FORMAT` correctly stays 1.
- **The 255 cap is real and general.**
  - `max_items` is applied after duplicate removal, at the 256th id's file (mods.rs:499-511). The test checks 256 → error and 255 → OK (terrain.rs `too_many_terrain_kinds_is_error`).
  - `decode_layers` independently caps the palette at 255 (layers.rs:~148-153), so the `u8::try_from(...).expect` can't panic on a legend naming >255 unknown ids before the link pass.
- **Independent tests pin orientation and byte order.**
  - `layers_are_north_up` and `heights_accept_16_and_8_bit` use a raw PNG writer, not `write_layers`, so the proptest round-trip (write_layers ∘ decode_layers) can't hide a symmetric flip or endianness bug.
- **The ancillary-chunk test is real.** It hand-builds valid gAMA/sRGB/iCCP chunks (including a correct zlib stored block for iCCP) rather than asserting a no-op.
- **The `fl_sim` API is compatible.**
  - `pub use fl_content::MoveClass` keeps `fl_sim::MoveClass` (lib.rs:93-95).
  - Variant order is unchanged, so serde variant indices (the postcard codec, `trace_enum`) and the explicit `SimHash` mapping (units.rs:280-287: Foot 0 … Tracked 3) are unchanged.
  - The derives added (`Ord`, `PartialOrd`, `Hash`) are additive.
- **The arch rule is placed correctly.** It is checked before `denied_direct`/member filtering, so it fires for normal, dev and build edges of any non-`Content` role (arch.rs:169-178). Its test ran RED against the old code (report RED 2) and asserts the exact message for fl_sim, fl_cli, fl_client and fl_test_utils (dev).

### Issues

#### Critical (Must Fix)

None.

#### Important (Should Fix)

1. **A plausible content typo aborts the loader instead of producing a ContentError.**
   - Where: builtin.rs:80-92 (`cells`), layers.rs:~168-174 (`decode_layers` allocation) and layers.rs:~121-124 (`read_png` buffer).
   - What's wrong:
     - `cell_m` has no lower bound: link only requires > 0 and an exact divisor (builtin.rs:147-160).
     - `decode_layers` allocates `types` and `heights` (`vec![0; cells_x*cells_y]`) before it opens either PNG, so before the size check.
     - Example: a map with `terrain` and `cell_m: 0.04` instead of `4.0` on a 4096 m side gives 102400² ≈ 1e10 cells. That is ~10 GB + 20 GB of allocation before the "expected 102400x102400 pixels, found 1024x1024" error can be produced.
     - `cell_m: 0.0001` gives ~1.6e15 cells, which hits `handle_alloc_error`, a process abort.
     - Whether the 1e10 case errors or aborts depends on the OS's overcommit and pagefile, so the load outcome itself becomes machine-dependent.
     - A crafted IHDR whose dimensions equal such a `cells` also drives `read_png`'s `vec![0; output_buffer_size]` before any IDAT is read.
   - Why it matters: content loading must fail with a located error, never abort. This is new with this task: maps without `terrain` never reach this path.
   - Fix:
     - Bound the grid: a max cells per side, or `cell_m ≥` some minimum. Check it in `MapDef::link` (at `cell_m`) and in `cells()`, so the sidecar skips.
     - Move the `MapLayers` allocation after the terrain PNG's header/size check.
     - Add a test: a map with a tiny `cell_m` and terrain → error at `cell_m`, no decode.

#### Minor (Nice to Have)

1. **Sidecar file names aren't confined to the map's directory.** layers.rs:~178 and ~211 do `dir.join(&terrain.terrain_png)`.
   - An absolute path or a `..` path silently reads files outside the mod (another mod's or an arbitrary PNG).
   - ADR-0014 and the brief say "next to the map file", and hot reload (Task 12) will need to know which files to watch.
   - Fix: reject absolute paths and paths with `..` (or with any separator) at `terrain.height_png`/`terrain.terrain_png` in link.
2. **Sidecar errors are located only at the item id.** mods.rs:360 uses `ContentError::at(&file, item.id(), msg)`, so the path is `t.map`, while every other map error is at `t.map/<field>`.
   - The message does name the PNG path and pixel, so the location is usable, but it is coarser than the link-pass convention.
   - Fix: have `load_sidecars` return the field (`terrain.terrain_png`/`terrain.height_png`) with the message, or prefix it.
3. **`cells()` duplicates the link pass's size/cell rules.** builtin.rs:80-92 restates builtin.rs:138-160.
   - If one is edited without the other, the sidecar either decodes a map the link pass rejects (double report) or skips one it accepts (silently no layers, and the load succeeds).
   - Fix: share one helper that both call.
4. **`MapDef` is serialize-asymmetric.** Serialize writes `layers`; Deserialize skips it.
   - Any future non-self-describing round-trip (postcard/bincode of a `MapDef`, for example in a save or a content snapshot) would misalign silently.
   - Today `MapDef` is deserialized only from RON (checked: no other deserialization site), and the design is brief-mandated.
   - Fix: a doc warning on the field is enough.
5. **The `IDENTITY` transformation is implicit.** layers.rs:~95 relies on `png`'s default.
   - The report claims "uses Transformations::IDENTITY", but the code never sets it.
   - It is safe under the `=0.18.1` pin, but one line (`decoder.set_transformations(png::Transformations::IDENTITY)`) makes the ADR's "no palette expansion or bit-depth transformation" survive a version bump.
6. **Test coverage gaps:**
   - `rules_hash_changes_with_one_pixel` flips only a terrain pixel, not a height pixel or a legend entry.
   - `unsupported_png_formats_are_errors` doesn't try a height PNG that is gray-alpha, indexed, or 16-bit gray+alpha. The predicates are strict, so the risk is low.
7. **The arch test comment doesn't match what it checks.** arch.rs:518-519 says "Bevy's image stack in the client" but checks `check_tree("fl_content", ...)`. Either check `fl_client`'s tree or reword the comment.
8. **Report inaccuracy (no code impact).** The report lists error order as format → size → legend → alpha; per pixel, the code checks alpha before the legend (layers.rs:~196-209).
9. **Test helper doc mismatch.** In tests/terrain.rs, `write_png`'s doc says "An 8-bit PNG", but it takes any depth.

### Checks run (one per named risk)

- **png default transformations.** Read `png-0.18.1/src/decoder/mod.rs`. `new`, `new_with_limits` and `new_with_options` all set `Transformations::IDENTITY`. `output_buffer_size` and `next_frame` use the raw colour type/depth under IDENTITY, and interlaced images are deinterlaced into the full buffer. So `line_size` and sample offsets are correct.
- **Rules-hash coverage.** `git show 64d15be:crates/fl_content/src/hash.rs`: every rules-hashed kind item is postcard-serialized whole, so `MapDef.layers` is included.
- **Other `MapDef` (de)serialization sites (risk: skip_deserializing misalignment).** `git grep MapDef` finds only `kind::<MapDef>()` reads (fl_sim/scenario.rs, fl_game/builder.rs, tests). There is no serde round-trip.
- **`MoveClass` SimHash and codec stability.** units.rs:280-287 has an explicit discriminant mapping; `trace_enum::<MoveClass>()` (units.rs:352) sees the same name and variant order.
- **`CONTENT_KINDS` index use (risk: "visual" shifting from index 3 to 4 reaching hashed state).** It is used only by `DefRegistry::string_tables` (registry.rs:282). Nothing in the sim reads it.
- **Golden replay is header-only.** Byte-compared `testbed_wander.replay` with `.replay.new`:
  - The last 4870 bytes, everything after the header's `sim_resources` name list (commands and the per-tick hash stream), are byte-identical.
  - The +81 bytes and the differences all sit in the header: rules hash, `content_kinds` + `content.terrain` tables, build id, and the pre-existing `core.classified_up_to`/`core.debug_queries` resource-name drift from earlier tasks.
- **Can sim-visible state change?** No, reasoning from the diff:
  - The testbed map has no `terrain`, so `terrain`/`layers` stay `None`.
  - `fl_sim` reads only `size_m`/`cell_m` from `MapDef`.
  - `MoveClass` hash and codec are identical.
  - The `RulesSection::link` default is a no-op.
  - No sim system reads a content-kind table index.

### Assessment

**Task quality:** Needs fixes

**Reasoning:** The format, decoding and hashing are correct, deterministic and well tested, and the `MoveClass` move and the arch rule are sound. One Important robustness gap remains: an unbounded grid size is allocated before the PNG size check, so a `cell_m` typo can abort the loader instead of reporting a ContentError. It is a small, local fix with one test.

### Bless recommendation

- `xtask/snapshots/api/fl_content.txt.new`: **bless.** It is purely additive and matches the brief's Interfaces exactly (MoveClass, TerrainDef, MapTerrain, MapLayers, decode/write_layers, MapDef.terrain/layers, load_sidecars, RulesSection::link). The Important fix shouldn't change public API, unless it adds a pub constant; if it does, re-run `cargo xtask api`.
- `xtask/snapshots/api/fl_sim.txt.new`: **bless.** The enum is replaced by `pub use`, field types print the `fl_content` path, and the SimHash impl line moves. There is no semantic API change.
- `crates/fl_test_utils/tests/replays/testbed_wander.replay.new`: **bless.** It is header-only (byte-verified suffix plus reasoning above). The fix for Important-1 won't change the testbed's rules hash unless it adds a new rules-hashed field; re-record if it does.
- Reason: `M1 Task 3: MoveClass moved to fl_content; MapDef gains terrain layers (rules hash)`.
