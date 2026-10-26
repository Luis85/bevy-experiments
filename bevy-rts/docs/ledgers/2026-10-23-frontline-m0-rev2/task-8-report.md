# Task 8 report: fl_content canonical rules hash, frozen registry, string tables

Worktree `C:\fl\t08`, branch `m0/t08-content`, base 32ec154. Commit: `e8903d4 refactor(fl_content)!: canonical postcard rules hash, frozen registry, string tables` (trailers `Refs: ADR-0007, ADR-0004`, Co-Authored-By).

## Implemented
- `hash.rs`: `pub const RULES_HASH_FORMAT: u32 = 1` (re-exported from lib). `rules_hash` = xxh64(seed 0) over `RULES_HASH_FORMAT.to_le_bytes()` ++ `postcard(Canonical { mods: [(id, version)] load order, units: [(id, name, tags, BTreeMap<&str, &dyn DefComponent>)] by id, kinds: [(ext, [postcard bytes per item, id order])] for IN_RULES_HASH kinds by ext, rules: [(name, postcard bytes)] by name })`. Fx/Angle go through their binary (raw integer) serde path.
- `registry.rs`: `frozen` flag, `freeze()`, `is_frozen()`; every `register_*` (component, kind, rules section, object kind) first asserts `"DefRegistry is frozen: register '<name>' in SimFeature::build"`. `string_tables()` → DEF_COMPONENTS / OBJECT_KINDS / CONTENT_KINDS / RULES_SECTIONS, each sorted. `fingerprint()` is now xxh64 over postcard(`string_tables()`) (the architect's suggestion; fl_sim fingerprint tests still pass).
- `db.rs`: `ContentDb::string_tables()` → `content.unit` (UNITS) in UnitTypeId order plus `content.<ext>` per registered kind in KindTable order. `same_id_sets` deferred to M1 per brief.
- `mods.rs`: `load_dirs` freezes the registry before parsing.
- `lib.rs`: `#![deny(clippy::float_arithmetic)]`.
- Flatten check: `grep -rn "serde(flatten" crates xtask` → no hits (only the new doc comment mentioning the rule).

### T5-review minors
- Link pass skipped on any inheritance failure: fixed. Added `inherit::resolve_partial` (pub(crate); `resolve_objects` now wraps it), `load_dirs` links the objects that did resolve, and concrete units whose chain failed go into `ContentDb::unresolved_units`, which `LinkCtx::unit_exists` treats as existing (so no duplicate "does not exist" errors). The link pass is still skipped only when the u16 unit cap is exceeded.
- Missing rules section error now names `<last-loaded mod dir>/rules.ron` (bare `rules.ron` only if no mod loaded).
- Hash-change tests for faction and rules section: added (fixtures `hash_faction_b`, `hash_rules_b`).
- Dep-cycle and load_after-only ordering tests: unit tests in `mods.rs` against `order_mods`.

## Tests
New: `hash::tests::rules_hash_sees_low_bits_at_large_magnitude`; load.rs `rules_hash_changes_with_faction_color`, `rules_hash_changes_with_rules_section`, `register_after_freeze_panics`, `load_freezes_the_registry`, `registry_string_tables_sorted`, `content_string_tables_in_id_order`, `link_pass_runs_when_an_inheritance_chain_fails`; mods.rs `load_after_alone_orders_mods`, `dependency_cycle_is_error`; `missing_rules_section_is_error` extended with the file assertion. Kept: stable/formatting/visuals/stat hash tests.

`cargo nextest run --profile agent -p fl_content` → `Summary [3.003s] 58 tests run: 58 passed, 0 skipped`.

## TDD evidence
- RED (hash): `cargo nextest run --profile agent -p fl_content --lib rules_hash_sees` on the old RON hash:
  ```
  FAIL hash::tests::rules_hash_sees_low_bits_at_large_magnitude
  panicked at crates\fl_content\src\hash.rs:98:9: assertion `left != right` failed
    left: 200646441103695631
   right: 200646441103695631
  ```
  Expected: the RON text writes Fx as f64, so 2^22+2^-32 and 2^22+2^-31 collapse to the same float.
- RED (rest): `cargo nextest run --profile agent -p fl_content` failed to compile: no method `freeze` / `is_frozen` / `string_tables` on `DefRegistry` / `ContentDb` (expected, new API).
- GREEN: same command → 58/58 passed.

## Gate (`cargo xtask gate task --track contract --track content`)
```
PASS fmt (3.4s)
PASS clippy-drift (3.4s)
PASS claude-md (0.0s)
PASS check-scope (0.4s)
PASS protected (0.4s)
WARN test-guard (1.5s): test-guard: 1 findings (reviewer must accept each)
    TEST-GUARD crates/fl_content/tests/load.rs:361 NewShouldPanic: #[should_panic(expected = "DefRegistry is frozen")]
PASS clippy (49.8s)
PASS arch-check (6.5s)
PASS test-headless (92.0s)
SKIP test-client: no client package touched since 32ec154 (.gate-base)
SKIP data-check: crates/fl_cli/src/data_check.rs does not exist yet
gate task: PASS in 158s
```
The test-guard WARN is the brief-mandated `#[should_panic(expected = "DefRegistry is frozen")]` on `register_after_freeze_panics`.

## Files changed
- `crates/fl_content/src/{hash,registry,db,mods,lib}.rs` (brief)
- `crates/fl_content/src/inherit.rs`, `crates/fl_content/src/link.rs` (outside the brief's list; needed for the T5 link-pass minor)
- `crates/fl_content/tests/load.rs`
- fixtures `tests/fixtures/{hash_faction_b,hash_rules_b}/` (brief) and `tests/fixtures/bad_inherit_link/` (new, for the link-pass minor)

## Concerns / deviations
1. The brief's test pair `Fx::from_raw((1<<53)|1)` vs `((1<<53)|2)` does NOT fail under the old RON hash: 2^21+2^-31 is exactly representable in f64, so the two values still differ as text. I kept that pair verbatim and added `(1<<54)|1` vs `(1<<54)|2` (both round to 2^22 in f64), which is what demonstrates RED.
2. Gate run with `--track contract --track content` (brief Step 5), not `--track contract` alone: `db.rs`, `lib.rs`, `inherit.rs`, `link.rs`, tests and fixtures are in the `content` track, so contract-only check-scope would fail.
3. Touched `inherit.rs` and `link.rs` and added fixture `bad_inherit_link`, all outside the brief's Files list, to fix the T5 link-pass minor that the controller asked for.
4. `Faction.color` (f32) stays in the rules-hashed kind, so a colour change changes the rules hash. The new test pins this; the architect's optional move to a presentation kind is not done.
