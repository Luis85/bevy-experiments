# Task 4 report: `fl_content` def-component registry, object parsing, inheritance

**Status:** DONE_WITH_CONCERNS (the concerns are minor and listed at the end)
**Commit:** `7f675ab feat(fl_content): def-component registry, object parsing, inheritance`

## What I implemented
- `error.rs`: `ContentError { file, line_col, path, message }`. It displays as `file[:line:col][: path]: message`, leaving out the parts that are missing. `ContentErrors(pub Vec<ContentError>)` prints one error per line. Both implement `std::error::Error`, and there is `From<ContentError> for ContentErrors`.
- `def.rs`: `DefComponent` (validate / instantiate / as_any) with `erased_serde::serialize_trait_object!`, and `SpawnCtx { content, unit, player, pos, heading }`.
- `link.rs`: a minimal `LinkCtx<'a> { file, scope, errors }`.
  - `pub fn new(file, scope, &mut errors)`.
  - `error(path, msg)` pushes a `ContentError` whose path is `"{scope}.{path}"`, or the scope alone when `path` is empty.
  - Task 5 is expected to extend or replace the constructor.
- `registry.rs`: `DefRegistry` (`#[derive(Resource, Default)]`).
  - Fields: `components: BTreeMap<&'static str, ComponentEntry{deserialize: DeserializeFn}>`, `type_names: BTreeMap<TypeId, &str>`, `object_kinds`, `content_kinds` and `rules_sections`. The last three are `BTreeSet<&'static str>`; the content-kind and rules-section sets are empty placeholders for Task 5.
  - `DeserializeFn = for<'de> fn(&mut dyn erased_serde::Deserializer<'de>) -> Result<Arc<dyn DefComponent>, erased_serde::Error>`.
  - `register_component`: panics on a duplicate name, and on a duplicate type (the message names the type).
  - `register_object_kind`: panics on a duplicate.
  - Also: `component_name<T>` and `fingerprint()`, which is xxh64 with seed 0 over `"component:X\nobject:Y\nkind:Z\nrules:W\n"`, each group sorted.
  - Extra helper: `object_kind(ext) -> Option<&'static str>`. Task 5 needs it to map a file suffix to the `'static` kind string.
- `object.rs`: `ObjectDef` and `parse_object`.
  - Uses `ron::Options::from_str_seed` with a hand-written `DeserializeSeed`/`Visitor` (`ObjectSeed`).
    - The object is read through `deserialize_any`, so ron treats `Unit(...)` as a map and ignores the struct name.
    - Unknown top-level fields, duplicate fields and a missing `id` are errors.
  - `components` uses `ComponentsSeed`, a map visitor:
    - It looks each name up in the registry. An unknown name gives `unknown component 'X' (registered: …)`; a duplicate gives `duplicate component 'X'`.
    - Each value is deserialized through `<dyn erased_serde::Deserializer>::erase(d)` and the entry's fn pointer.
  - When parsing fails:
    - `line_col` comes from `SpannedError.span.start`.
    - `path` is `"{id}/components.{name}"`, `"{id}/{field}"` or `"{id}"`, depending on how far the parser got (the id is only known if it was parsed before the error).
  - In RON, `name` is a bare string (`name: "x"`) and `inherits` is `Some("…")`, as in the brief's schema.
- `inherit.rs`: `ResolvedObject` (with `get<T>` and `has`), `Resolved { concrete (sorted by id), abstract_ids }` and `resolve_objects`.
  - Resolution walks each chain once and memoizes it.
  - At each level it applies `remove` to the inherited set first (removing a component that is not there is an error), then inserts the child's components, replacing them whole.
  - Tags are a `BTreeSet` union along the chain, so they come out sorted and deduplicated.
  - All errors are collected, each with path `{id}/inherits` or `{id}/remove`:
    - duplicate ids, naming both files
    - a missing parent
    - a cycle, reported once as `inheritance cycle: a -> b -> a`
    - a parent of a different kind
  - Descendants of an object that failed are skipped silently, so the root cause is reported only once.
- `db.rs` (ruling R2): a minimal `ContentDb { registry, units }` with the accessors `units()`, `unit(UnitTypeId)` and `registry()`, matching Task 5's interface. It has no constructor yet; Task 5's `load_dirs` will build it.

## Tests
`crates/fl_content/tests/objects.rs` has 21 tests, and `link.rs` has 1 unit test. The two test-only components, `TestHull {hp: u32}` registered as "Hull" and `TestSpeed {kmh: Fx}` registered as "Speed", are both `deny_unknown_fields`. The fixtures are in `tests/fixtures/objects/` (`pz3`, `unknown_component`, `unknown_field` `.unit.ron`).
- The brief's list:
  - parses_registered_components
  - unknown_component_is_error_with_line: also checks that the line is 5, the path, and the `file:5:` display
  - unknown_field_is_error
  - child_replaces_parent_component_wholesale
  - remove_drops_inherited_component
  - three_level_chain_resolves
  - abstract_objects_are_not_output
  - tags_are_unioned_sorted
  - inheritance_cycle_is_error: exactly one error
  - missing_parent_is_error
  - removing_absent_component_is_error
- Extras:
  - optional_fields_default, missing_id_is_error, unknown_top_level_field_is_error, duplicate_component_is_error
  - name_falls_back_to_id, duplicate_id_is_error, errors_display_one_per_line
  - registry_names_and_fingerprint, duplicate_component_name_panics, duplicate_component_type_panics

## TDD evidence
- **RED:** `cargo test -p fl_content --test objects` failed to compile: `error[E0432]: unresolved imports fl_content::ContentError, fl_content::DefComponent, fl_content::DefRegistry, … parse_object, resolve_objects`, and `cannot find type Resolved / ContentErrors`. This was expected because the crate had only an empty `lib.rs`.
- **GREEN:** `cargo test -p fl_content` printed `test result: ok. 21 passed; 0 failed` for objects and `ok. 1 passed` for the lib unit tests.

Sample errors, from a scratch test that I deleted afterwards:
```
…\unknown_component.unit.ron:5:10: test.bad.armour/components.Armour: unknown component 'Armour' (registered: Hull, Speed)
…\unknown_field.unit.ron:4:25: test.bad.colour/components.Hull: Unexpected field named `colour`, expected `hp` instead
i.ron:1:44: x/components.Speed: value NaN is not a finite number with magnitude below 2^31, so it cannot be an Fx
inline.unit.ron: t.a/inherits: inheritance cycle: t.a -> t.b -> t.a
```

## Gate (run from C:\fl\a before committing)
- `cargo fmt --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo test --workspace`: all pass (fl_content 21+1, fl_math 46+4, fl_protocol 4, …), with no failures and no warnings.
- `bash scripts/check_deps.sh`: `deps OK`.

## Files
- `crates/fl_content/src/{lib,error,def,link,registry,object,inherit,db}.rs`
- `crates/fl_content/tests/objects.rs`
- `crates/fl_content/tests/fixtures/objects/{pz3,unknown_component,unknown_field}.unit.ron`

## Decisions and concerns
1. **Name inheritance.** A resolved `name` is the nearest `name` along the chain, falling back to the id. The brief only says "falls back to id". I chose this because everything else inherits too. If own-name-or-id is preferred instead, it is a one-line change in `inherit.rs`, and the name assertion in `three_level_chain_resolves` would change with it.
2. **The struct name is ignored.** Because the object is read with `deserialize_any`, any struct name is accepted: `Building(id: …)` parses in a `*.unit.ron` file, and so do a bare `(…)` and `{ "id": … }`. This matches "ron ignores the struct name". `ron`'s `deserialize_struct` would reject a name that doesn't match, and the kind name is not available as a `'static` capitalized string. `parse_object` also does not check that `kind` is registered.
3. **Heads-up for Task 5's rules hash.** serde implements `Serialize` for `Arc<T>` only with its `rc` feature, which the workspace does not enable. So `BTreeMap<String, Arc<dyn DefComponent>>` will not serialize directly; serialize `&**arc` per entry, or enable `serde/rc`, which is a Cargo.toml change.
4. **`LinkCtx::new` is public** so that it is not dead code and can be unit-tested. Task 5 may change its signature freely.
5. A component error's inner message is converted to text when it crosses the erased-serde boundary (`de::Error::custom`), so ron reports it as `Error::Message`. The location and path are still accurate.
