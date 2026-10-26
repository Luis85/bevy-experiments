---
status: "accepted"
date: 2026-10-25
decision-makers: "controller (accepts under the user's delegation, R85), lead"
consulted: "M2 prelude planner (planner-m2p-report.md; spec-gap resolutions 1 and 2 accepted as R148); research: architecture-best-practices.md N10, C11; workflow-and-verification.md §3.3, §6.7; baseline-recommendations.md C2, C6"
informed: "M2 prelude implementers (Tasks 7, 8, 9, 10, 12)"
---

# Content load phases over a lossless RON tree; patches, per-path origins and the provenance ledger

## Context and Problem Statement

`fl_content::load_dirs` deserializes each RON file straight into typed defs, then resolves inheritance and links. Spec
§9.2 C6 asks for explicit phases (discover → parse → migrate → patch in passes → inherit → typed deserialize → feature
post-process hooks → link → freeze) so that two mods can change one unit without whole-object overrides, old content
formats migrate, and features normalise data after loading. C2 asks that every value carry its provenance in a closed
vocabulary, enforced by `data-check`. Patching and migration need an untyped tree, but ron 0.12's `deserialize_any`
reports a bare identifier as unit (`handle_any_struct` → `visit_unit`), so neither `ron::Value` nor
`serde_json::Value` keeps enum variants such as `class: Tracked`: a tree built that way loses data. The rules hash
(ADR-0007) must not change for content that no patch touches.

## Decision Drivers

* Two mods touching one value compose in a defined order, and the result names who set it (research N10: RimWorld
  patches, Factorio's data stages).
* Every error names the file, the path and, for patches, the op.
* The typed result of unpatched content is unchanged: byte-identical rules hashes (R148).
* Provenance is checkable by tools and never changes the rules hash.
* Agents and tools address values by one path syntax (ledger, mechanic specs, tuning tables, errors).

## Considered Options

* Own lossless tree, phases, patches and ledger
* `ron::Value` or `serde_json::Value`
* Whole-object overrides, provenance as comments

## Decision Outcome

Chosen option: "Own lossless tree, phases, patches and ledger", because it is
the only option whose tree keeps every value our content writes, so patches and migrations can work on it without
changing the typed result.

- **Phases**, each with path-aware errors:
  1. **Discover** mods and order them (`depends`, `load_after`, ties by id).
  2. **Parse** each file into a `ContentTree` `Node` with spans (`parse_ron`): the RON subset our content uses
     (comments, named and unnamed structs and tuples, lists, maps, strings with escapes, integers, floats,
     booleans, `Some`/`None`, `()`, bare identifiers, trailing commas). Anything else is an error naming file, line
     and column. `to_ron` prints a tree that `parse_ron` reads back equal.
  3. **Migrate** each document from its mod's `mod.ron` `format_version` (default 1; current
     `CONTENT_FORMAT_VERSION` 1) to the current one, by registered tree migrations (`DefRegistry::register_migration`,
     one per source version, run in order). A mod newer than the build is an error.
  4. **Patch** with `<mod>/patches/*.patch.ron` files (`Patch(target, phase, ops)`, target `rules`, `unit:<id>` or
     `terrain:<id>`). Ops: `Set` (the path must exist), `Merge` (into a struct or map), `Remove` (the path must
     exist), `Add` (append to a sequence, or insert a missing key or component), `AddTag` (unless present) and
     `If(cond, ops)` with `cond ∈ {Exists(path), ModLoaded(id), Not(cond)}`. Pass `Default` runs every patch, then
     pass `Final`; within a pass, mod load order, then file name order, then op order. "File name order" is the
     byte-wise order of the patch file's path relative to its mod root, with `/` separators, so it is the same on
     every OS. An error names the patch file, the op index and the path. Patch values for enum variants may be
     written as identifiers or strings.
     - Patches apply to documents before inheritance, so a patch on a template reaches its children. For the same
       reason, a path that a child only inherits does not exist in the child's document: `Set` on it fails. A mod
       patches the template, or `Add`s the whole component to the child (which then replaces the inherited one).
     - **Whole-object `override`** (an object file with `override: true`, R151) takes effect in this phase, in pass
       `Default` at its mod's turn in load order, before that mod's own patches. It replaces the earlier mod's
       document whole and discards every patch already applied to that object, and all its leaves get the
       override's origin (its mod and file). Patches that come later (that mod's own, later mods', and every `Final`
       patch) apply on top of it.
  5. **Inherit** per component on trees: a child's component replaces the parent's whole, `remove` drops one.
  6. **Typed deserialize** through the registry seeds over `TreeDeserializer` (strict: `deny_unknown_fields`, errors
     carry the field path and span).
  7. **Feature post-process hooks** (`DefRegistry::register_post_process`, `SimBuilder::register_content_post`), run
     in feature id order; their changes are content and enter the rules hash.
  8. **Link** (cross-references, `requires` and class expressions, ADR-0017; range checks, R104), **freeze**
     (sorted ids, string tables) and the **rules hash** (ADR-0007, unchanged encoding).
- **Path keys.** `<doc>/<field path>`, where `doc` is `rules` or `<kind>:<id>` (`unit:base.de.pz3`,
  `terrain:base.forest`) and the field path is dot-separated, map keys as written and sequence indices as numbers:
  `unit:base.de.pz3/components.Hull.footprint.0`, `rules/Movement.min_speed_frac` (`path_key`). Patches, ledgers,
  mechanic specs (ADR-0022), tuning tables and errors all use them.
- **Origins.** Every leaf of every document (abstract objects, kinds, rules) records the mod and file that last set it
  (`ContentDb::origins`); a patch op sets the origin of the leaves it writes to the patch's mod and file, and an
  inherited leaf keeps its parent's origin (under the parent's key).
- **Provenance ledger (C2).** `<mod>/provenance/*.prov.ron` maps path keys to `(src, conf)`:
  - `src ∈ {Original(file, note), Manual(section), ResearchDerived(doc), OursScaled(from), Guess(id), Tuned(mcr,
    sweep)}`, a closed vocabulary; `conf ∈ {High, Medium, Low}`. `Guess.id` matches `G-[A-Z]+-[0-9]+`, and
    `Tuned.mcr` matches `MCR-[0-9]{3}`.
  - **Tunable leaves** are every leaf of `rules.ron`, the number, boolean and identifier leaves under `components.` of
    objects, and the leaves a kind lists in `ContentKind::TUNABLE` (`TerrainDef`: `pass`, `blocks`).
  - **Coverage** is judged against the final origins: every tunable leaf whose final origin is mod M needs an entry
    in M's ledger ("no provenance for <key>"). A patching mod's ledger covers the leaves its patches set.
  - **Staleness** is judged per mod against that mod's own documents before the patch phase (R151): an entry in M's
    ledger must name a tunable leaf of M's own parsed and migrated documents, or a leaf one of M's patches writes;
    otherwise it is an error ("stale provenance key <key>"). So when mod B patches a leaf that `base` set, `base`'s
    entry stays valid (it describes `base`'s value), B's entry covers the final value, and loading B never breaks
    `base`'s `data-check`.
  - Test fixture mods under `crates/**/tests/fixtures/**` are exempt from the ledger rules (R151): they are inputs
    to loader tests, not shipped content. A fixture that tests the ledger itself (Task 8) opts in by carrying a
    `provenance/` directory.
  - Ledgers are not content: they are read per mod, never enter the rules hash, and never reach the sim.
  - `Guess` ids are defined in a mechanic spec's Guesses section or in `docs/design/GDD.md` §Guesses; values the spec
    fixes (`StateHash.interval_ticks`, `Spatial.cell_m`) are `Guess` with Medium confidence (R148). `spec-lint`
    checks every id (ADR-0022).
  - `fl_cli tuning-tables` writes `docs/design/generated/tuning-tables.md` from the ledger, and `data-check
    --tuning-tables` fails when that file is out of date.
- **Deprecated keys.** `DefRegistry::deprecate_field(component, old, new)` renames an old key during migrate, with a
  `data-check` warning ("deprecated key '<old>' in <file>: use '<new>'"), as the README's deprecation policy asks.

### Consequences

* Good, because mods compose by patching paths, in an order anyone can read off the mod list, and the origins say
  which mod and file set every value.
* Good, because the typed pipeline is proven unchanged by byte-identical rules hashes before any patch exists.
* Good, because provenance is mechanical: a value without an entry, or an entry without a value, fails `data-check`.
* Bad, because we own a RON-subset parser and printer, and a RON feature our content starts using must be added to
  it first (an error names the unsupported syntax).
* Bad, because every tunable value needs a ledger entry, which is bookkeeping for each new def field.
* Neutral, because whole-object `override` stays available for a mod that wants to replace an object.

### Confirmation

* Tree pipeline (Task 7, `fl_content/tests/tree.rs`): `identifiers_survive_parsing`, `floats_parse_like_ron`,
  `printer_round_trips`, `unsupported_syntax_names_position`, `typed_errors_name_field_path`,
  `tree_load_matches_typed_load` (byte-identical rules hashes for the base mod and every fixture mod),
  `origins_cover_every_leaf`, `inherited_leaf_keeps_parent_origin`.
* Provenance (Task 8, `fl_content/tests/provenance.rs`, `fl_cli/tests/tuning_tables.rs`): `base_ledger_is_complete`,
  `missing_ledger_entry_is_error`, `stale_ledger_key_is_error`, `closed_vocabulary`, `bad_guess_and_mcr_ids_are_errors`,
  `ledger_is_not_hashed`, `tuning_tables_lists_every_key`, `tuning_tables_check_detects_drift`; Task 9's
  `unknown_guess_id_is_lint_error` and `param_key_must_match_ledger`.
* Override and provenance under patches (R151, Tasks 8 and 10): `override_discards_earlier_patches`,
  `patched_leaf_keeps_base_ledger_valid`, `fixture_mods_are_exempt_from_provenance`.
* Migrate, patch and post-process (Task 10, `fl_content/tests/patch.rs`): `set_changes_one_field`,
  `later_mod_patch_wins_and_owns_provenance`, `final_pass_runs_after_default`, `template_patch_flows_to_children`,
  `merge_remove_add_addtag`, `if_mod_loaded_skips_absent_mod`, `patch_on_missing_path_names_file_and_op`,
  `patch_on_missing_target_is_error`, `enum_patch_values`, `old_format_is_migrated`, `newer_format_is_error`,
  `deprecated_field_warns`, `post_process_runs_in_feature_order`, `base_content_unchanged`.
* Gate rows: `data-check` (with `--tuning-tables docs/design/generated/tuning-tables.md`), `spec-lint`, `api` (the
  `fl_content` and `fl_sim` snapshots), and the golden replays verifying unchanged through Tasks 7, 8 and 10.

## Pros and Cons of the Options

### Own lossless tree, phases, patches and ledger

* Good, because identifiers, struct names and spans survive, so patches, migrations and errors work on real content.
* Good, because the ledger is separate from content, so provenance edits never move a hash.
* Bad, because of a parser and printer to maintain, and the proof burden that the typed result is unchanged.

### `ron::Value` or `serde_json::Value`

* Good, because there is no parser to write.
* Bad, because a bare identifier becomes unit, so `class: Tracked` and every other enum unit variant is lost, and the
  typed result after a round trip differs from the file.
* Bad, because neither carries spans, so errors lose their line.

### Whole-object overrides, provenance as comments

* Good, because it is today's model, with no new code.
* Bad, because two mods touching one unit conflict and the later one silently drops the other's changes (research
  N10).
* Bad, because comments cannot be checked, so provenance drifts from the data (the `// guess` comments of M1).

## More Information

* Spec §6.1 (def components, strict parsing), §6.4 (mods), §9.2 C2 and C6; the README's deprecation policy (content
  keys).
* Research: `architecture-best-practices.md` N10 (RimWorld PatchOperations, Factorio data stages, Recoil
  `unitdefs_post`), C11; `workflow-and-verification.md` §3.3 (tuning tables and provenance), §6.7 (migrating content).
* Related: ADR-0004 (string tables), ADR-0007 (canonical rules hash), ADR-0012 (hot reload reruns the same phases),
  ADR-0017 (`requires`, class expressions and verbs linked in phase 8), ADR-0022 (mechanic specs use path keys).
* Rulings: R148 (lossless tree accepted only with byte-identical rules hashes; spec-fixed values classed `Guess`),
  R151 (staleness per mod, the place of `override`, fixture mods exempt).
* Revisit: if content needs RON syntax the subset lacks, or if mod ordering needs more than `depends` and
  `load_after`.
