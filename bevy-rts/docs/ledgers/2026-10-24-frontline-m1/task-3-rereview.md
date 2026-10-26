# M1 Task 3 re-review, fix round 1 (dc0620e)

## Finding Verdicts
- Important 1 (unbounded grid aborts loader) - ADDRESSED. builtin.rs `grid()` is the one check (n > 4096 -> error at `cell_m`, cell <= 0 early return); `link` and `load_sidecars` both use it. layers.rs `decode` re-checks 1..=4096 per side before any file access, allocates `types` after the terrain PNG header/size check and `heights` after the height PNG's; `read_png` allocates only after the size check. No overflow in `grid()` (i64 raw division). Tests: `grid_over_4096_cells_per_side_is_error`, `decode_layers_rejects_oversized_grids`.
- Minor 1 (paths not confined; R90) - ADDRESSED. `check_png_name` (layers.rs) accepts only Normal/CurDir components with >= 1 Normal; enforced in link, load_sidecars (skip), decode, write_layers. Windows: drive prefix, UNC, verbatim, root-relative and `..\x` all parse as Prefix/RootDir/ParentDir and are rejected; the test covers `..`, `sub/../..`, absolute drive path, empty, and subdir/`./`.
- Minor 2 (sidecar error location) - ADDRESSED. Hook returns `(field, msg)`; mods.rs reports at `<id>/terrain.terrain_png|height_png`; tests assert the paths.
- Minor 3 (duplicated size rules) - ADDRESSED. `cells()` removed, shared `grid()`.
- Minor 4 (asymmetric serialize) - ADDRESSED. Doc warning on `MapDef.layers`.
- Minor 5 (implicit IDENTITY) - ADDRESSED. `set_transformations(IDENTITY)` in read_png.
- Minor 6 (coverage) - ADDRESSED. Rules-hash test now flips terrain pixel, height sample, legend colour; height PNG gray-alpha 8/16 and indexed added.
- Minor 7 (arch test comment) - ADDRESSED. Reworded; checks fl_content and fl_sim.
- Minor 8 (report order) - ADDRESSED (report corrected).
- Minor 9 (write_png doc) - ADDRESSED.

Test-guard replacements (3 asserts): all stronger.
- terrain.rs rules-hash: `base == base` retained, `ne` now for 3 variants instead of 1 (superset).
- arch.rs: old `check_tree("fl_content","png v0.18.1")==0` became the same plus `fdeflate` line, over fl_content and fl_sim (superset).

## New Breakage in the Fix Diff
- Minor: builtin.rs:111 the cap message contains a run of ~26 literal spaces ("at most 4096                          cells per side"), apparently a lost `\` line continuation. Cosmetic; tests match only "8192 cells"/"4096". Report claims a clean message.
- Minor: `check_png_name` is platform-dependent (std Path). On Linux/macOS `..\x` and `C:\x` are single Normal components and accepted (not an escape there, but a name valid on one OS fails on Windows). Optionally also reject `\` and `:` in names for cross-platform determinism of "does this mod load".
- Minor (non-blocking): symlinks inside the map dir are not confined; out of R90 scope.
- Minor: doc comments on `MapTerrain.height_png/terrain_png` (layers.rs) exceed the line width (rustfmt does not wrap comments; cosmetic).

## Out-of-Scope Observations
None.

## Verdict
**Fix round:** All findings addressed, no new Critical/Important breakage.

## Bless
`xtask/snapshots/api/fl_content.txt.new`: YES. The new `pub(crate)` items (`MAX_MAP_CELLS`, `check_png_name`, `png_names`, `decode`, `FieldError`) and private `grid()` add no public API; the only delta against round 0 is the `load_sidecars` return type `Result<(), (String, String)>` on the trait and each kind, matching the diff.
