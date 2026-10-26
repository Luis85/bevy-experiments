# Task 5 report: `fl_content` kinds, mods, link pass, rules sections, rules hash, `ContentDb`

**Status:** DONE_WITH_CONCERNS (the concerns are small design calls, listed at the end)
**Commit:** `24d0421 feat(fl_content): mods, kinds, link pass, rules sections, rules hash`

## What I implemented
- **`kinds.rs`**
  - `ContentKind`, `RulesSection` and `KindTable<K>` exactly as the brief specifies. `KindTable` keeps its items sorted by id and looks them up with a binary search; `index_of` returns the position as a `u32`.
  - Crate-private type erasure:
    - `ErasedKind` (id / link / into_any) and `ErasedKindTable` (as_any / erased_items) are blanket-implemented for every `K: ContentKind`.
    - `ErasedRules` is blanket-implemented for every `S: RulesSection`.
    - Both use `erased_serde::serialize_trait_object!`, so the hash can serialize them.
- **`builtin.rs`**
  - `Faction`, `MapDef`, `ScenarioDef` (with `ScenarioPlayer` and `ScenarioUnit`), `VisualDef` and `Placeholder`, field for field as in the brief. All use `deny_unknown_fields`.
  - `VisualDef.model` is `#[serde(default)]`, so it can be left out.
  - Each kind's link checks:
    - Faction: every roster unit exists and is concrete.
    - Map: 0 < side ≤ 4096 m on both axes, `cell_m` > 0, and each side is an exact multiple of `cell_m` (compared on the raw `Fx` values).
    - Scenario:
      - the map exists
      - every faction exists
      - every unit exists and is concrete
      - `player < players.len()`
      - `0 ≤ pos < size` on both axes
    - Visual: the unit exists (concrete or abstract).
- **`registry.rs`**
  - New functions: `with_builtins()`, `register_kind::<K>()` and `register_rules_section::<S>(name)`.
  - They panic on a duplicate extension or name, on a duplicate rules type, and on an extension that is already used as an object kind (and the other way round).
  - `fingerprint()` produces the same canonical text format as before.
- **`link.rs`**
  - `LinkCtx` now holds `&ContentDb`, and adds `unit_exists`, `unit_is_abstract` and `kind_get::<K>`.
  - `new` is now `pub(crate)`.
  - In error paths, an item scope is joined to its path with `/` (`a.red/roster[1]`); a component scope is joined with `.` (`v.zero/components.Hull.hp`).
- **`rules.rs`**: parses `Rules({ "Section": (…) })` with a `DeserializeSeed`. ron checks the struct name. An unknown or duplicate section is an error that carries the file, line and the section name as its path.
- **`mods.rs`**
  - Types: `ModManifest`, `CreditEntry` (the license is checked while deserializing, so the error carries a line) and `ALLOWED_LICENSES`.
  - `load_dirs` runs the brief's algorithm in order:
    1. Read the manifests. A missing `mod.ron` and a duplicate mod id are errors.
    2. Sort the mods topologically, breaking ties by id with Kahn's algorithm. A missing `depends` target is an error, a missing `load_after` target is ignored, and a cycle is an error.
    3. Walk each mod's files recursively, sorted by relative path:
       - `mod.ron`, `rules.ron` and `credits.ron` count only at the mod root.
       - `*.<ext>.ron` is routed to an object kind or a content kind.
       - Any other `.ron` file gives the error `unknown content file type (… registered kind: …)`.
       - Non-RON files are ignored.
       - A file that is not valid UTF-8 is an error.
    4. Check ids:
       - An object `override` replaces an object with that id from an earlier mod. Overriding an id from the same mod, or an id that no earlier mod defines, is an error.
       - A duplicate id is an error that names both files.
       - The namespace check skips overrides, and skips kinds with `NAMESPACED = false`.
    5. Resolve inheritance, then intern the units. More than `u16::MAX` units is an error.
    6. Run the link pass.
    7. Check that every registered rules section is present.
    8. Emit the missing-visual warnings, which are also logged with `tracing::warn!`.
    9. Compute the rules hash.
  - All errors are collected before returning.
- **`hash.rs`**: xxh64 with seed 0 over one canonical text with these lines:
  - `mod:{id}@{version}` for each mod, in load order.
  - `unit:` followed by the `ron::to_string` of `(id, name, tags, components)` for each concrete unit, sorted by id.
  - `{ext}:` followed by the RON for each item of every kind with `IN_RULES_HASH`, ordered by extension and then id.
  - `rules:{name}:` followed by the RON for each section, ordered by name.
- **`db.rs`**
  - `ContentDb` with every accessor in the interface, plus `load_scenario_file`. It parses a standalone file and runs `ScenarioDef::link` against the db.
  - `kind::<K>()` and `rules::<S>()` panic with the type name when the type is not registered.
- **Ruling R8:** I did *not* enable `serde/rc`. The hash serializes `BTreeMap<&str, &dyn DefComponent>`, built from the `Arc` map. The output is byte-identical to serializing the `Arc` map directly, so `Cargo.toml` and `Cargo.lock` are unchanged.
- **Ruling R9:**
  1. `remove: [...]` is parsed by a `RemoveSeed`. An unknown name gives `unknown component 'X' (registered: …)` with the file, the line of that element, and the path `{id}/remove`.
  2. `parse_object` rejects an unregistered `kind` with `unknown object kind 'X'`.

## Tests
- `tests/load.rs` has 24 tests.
  - Every test the brief lists:
    - loads_two_mods_in_dependency_order, override_replaces_unit_from_earlier_mod
    - duplicate_id_without_override_is_error (names both files), override_without_target_is_error, id_must_carry_mod_prefix
    - unknown_file_kind_is_error, mod_errors_are_aggregated (≥3 errors: missing dependency, unknown component with `tank.unit.ron:4:`, unknown file kind)
    - missing_rules_section_is_error, credits_reject_nc_license (exactly 1 error, at `credits.ron:3:`)
    - scenario_rejects_abstract_unit, scenario_rejects_unit_outside_map, faction_roster_must_exist
    - unit_ids_interned_in_sorted_order
    - rules_hash_stable_across_loads (the same hash whatever order the dirs are passed in), rules_hash_changes_with_stat, rules_hash_ignores_visuals, rules_hash_ignores_formatting
    - missing_visual_is_warning_not_error
  - Extras: missing_manifest_and_duplicate_mod_are_errors, credits_and_rules_are_loaded, map_size_must_be_multiple_of_cell, component_validate_runs_in_link_pass, kinds_are_loaded_into_sorted_tables (this one also covers `Controller` `Human`, `Ai(..)` and `None` in RON, and every accessor), load_scenario_file_links_against_db (good and bad files).
- `tests/objects.rs` gains 2 tests, for a total of 23: remove_unknown_component_is_error_with_line (line 5, path, display) and parse_object_rejects_unregistered_kind.
- The lib unit test `error_paths_are_scoped` was updated for the new `LinkCtx`, and now also covers the `/` join for item scopes.
- Fixtures are in `crates/fl_content/tests/fixtures/`:
  - `mod_a` and `mod_b`
  - `bad_{duplicate,override,prefix,unknown_kind,aggregate,no_rules,credits,scenario_abstract,scenario_outside,faction_roster,map,no_manifest,validate}`
  - `hash_{base,stat,visual,format}`
  - `standalone/{ok,bad}.scenario.ron`

Sample messages, taken from a scratch print test that I then deleted (fixture prefix trimmed):
```
bad_duplicate\units\two.unit.ron: d.same: duplicate id 'd.same' (first defined in bad_duplicate\units\one.unit.ron)
bad_override\units\ghost.unit.ron: a.ghost/override: override: true, but no earlier mod defines 'a.ghost' to replace
bad_prefix\units\wrong.unit.ron: q.wrong: id 'q.wrong' must start with its mod's prefix 'p.'
bad_aggregate\mod.ron: depends: mod 'g' depends on mod 'missing', which is not loaded
bad_aggregate\foo.weapon.ron: unknown content file type (expected mod.ron, rules.ron, credits.ron or *.<kind>.ron for a registered kind: faction, map, scenario, unit, visual)
bad_aggregate\units\tank.unit.ron:4:10: g.tank/components.Armour: unknown component 'Armour' (registered: Hull, Speed)
rules.ron: Test: missing rules section 'Test': no mod's rules.ron defines it
bad_credits\credits.ron:3:61: license 'CC-BY-NC-4.0' is not allowed (allowed: CC0-1.0, CC-BY-3.0, CC-BY-4.0, OFL-1.1)
bad_scenario_outside\outside.scenario.ron: s.outside/units[0].pos: (1000, 10) is outside map 'a.flat' (size 1000 x 1000 m)
bad_map\odd.map.ron: m.odd/size_m: y side 5000 m must be in (0, 4096] m
bad_validate\units\zero.unit.ron: v.zero/components.Hull.hp: must be positive
```

## TDD evidence
- **RED:**
  - `cargo test -p fl_content` failed to compile `tests/load.rs` with `error[E0432]: unresolved imports fl_content::Faction, fl_content::MapDef, fl_content::Placeholder, fl_content::RulesSection, fl_content::ScenarioDef, fl_content::VisualDef, fl_content::load_dirs`. This was expected: none of the new API existed yet.
  - `cargo test -p fl_content --test objects` gave `21 passed; 2 failed` (remove_unknown_component_is_error_with_line, parse_object_rejects_unregistered_kind). This was expected because the old parser accepted both inputs.
- **GREEN:** `cargo test -p fl_content` gave lib `1 passed`, load `24 passed`, objects `23 passed`, all with `0 failed`.

## Gate (run from C:\fl\a before committing)
- `cargo fmt --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test --workspace`: 102 passed, 0 failed, and no warnings.
- `bash scripts/check_deps.sh`: `deps OK`.

## Files
- New: `crates/fl_content/src/{kinds,builtin,mods,rules,hash}.rs`, `crates/fl_content/tests/load.rs`, `crates/fl_content/tests/fixtures/**`
- Modified: `crates/fl_content/src/{lib,db,error,link,object,registry}.rs`, `crates/fl_content/tests/objects.rs`

## Self-review fixes made before committing
- Too many units used to make the link pass panic in `unit_id`. Now the link pass is skipped when the cap is exceeded, and the error is attributed to the file of the first unit over the cap.
- Removed an unused per-section file path in the rules map.
- Python on Windows had written CRLF line endings into three files; I converted them back to LF.

## Decisions and concerns
1. **The link pass is skipped when inheritance resolution fails.** `resolve_objects` returns no partial result, so linking would only report every unit as missing. Parse and id errors in other files do not block the link pass, so a single typo can also cause follow-on link errors, for example a roster entry naming the unit that failed to parse. I accepted that to keep "collect all errors".
2. **Kinds have no `override`.** A duplicate kind id across mods, including a second `VisualDef` for the same unit from a later mod, is an error. If later mods need to restyle another mod's visuals, kinds will need an override mechanism.
3. **RON struct names are the Rust names:** `Faction(…)`, `MapDef(…)`, `ScenarioDef(…)`, `VisualDef(…)`. ron checks the name if one is written; it may also be left out (`(…)`). Asset authors in later tasks must use these names, or no name.
4. **Only `"unit"` objects are interned and exposed.** Concrete objects of any other registered object kind are resolved and validated, but `ContentDb` has no accessor for them yet.
5. **The missing-rules-section error has no real file.** It uses the bare file name `rules.ron`, because no single file is at fault.
6. **`ALLOWED_LICENSES` is public** (an addition to the interface), so the credits screen and data-check can show it.
