---
status: "accepted"
date: 2026-10-24
decision-makers: "user (via baseline upgrade), architect"
consulted: "research: architecture-best-practices.md N6/N7/C6/C7; workflow-and-verification.md §4.5; baseline-recommendations.md A6, A8"
---

# Mandatory state classification, versioned save format and the branch test

## Context and Problem Statement

Revision 1 hashed only "registered" components (§5.4); M0 code has `register_hashed`. A feature that forgets to
register a component causes desyncs the hash can't see, and the same component would be missing from saves. Resources
(tick, `SimId` allocator, seed, player stocks, vision grids) weren't covered. The design had no save/load at all, yet
campaigns, reconnects, late join and long testbed sessions need it, and retrofitting serialization onto a live sim is
how saves rot. `moonshine-save` (bevy 0.20) is reflection-based and states it has no versioning.

## Decision Drivers

* No sim state may escape the hash or the save.
* Saves must stay loadable across code changes (typed migrations).
* Save/load must be deterministic and therefore testable (Factorio "heavy mode").
* No Bevy `Entity` in persistent data.

## Considered Options

* Opt-in hashing registration; saves later (revision 1)
* Mandatory classification of every sim component and resource, with versioned postcard codecs, typed migrations, a
  round-trip test and a branch test
* Reflection-based saves (`moonshine-save`, `DynamicScene`)

## Decision Outcome

Chosen option: "Mandatory classification with versioned codecs and fitness tests".

- **Classes.** Every sim `Component` and `Resource` is registered through `SimBuilder` as exactly one of:
  - `Hashed { version }`: authoritative; hashed and saved with a versioned codec.
  - `Derived { rebuild }`: rebuilt from hashed state on load (spatial cell, effective-stat caches, `SimIndex`); not hashed.
  - `Transient`: cleared every tick; never hashed or saved (per-tick buffers, `SimCounters`, the journal).
  - `External` (resources only): fixed at build from the `MatchHeader` (content, config, cheats flag).
- **Startup check.** In debug builds and every test, `build_sim_app` walks `world.components()` and fails on any type
  from a sim crate (`fl_sim::`, `fl_feat_*`, not `*_client`) that is unclassified. `Local<T>` in sim systems counts as
  unclassified state and is banned unless registered.
- **Save layout** (postcard): `MatchHeader` (with string tables), then `{ tick, sim_id_allocator, pending_commands,
  resources[(name, version, bytes)], entities[SimId → [(component, version, bytes)]] }`, sorted by name and `SimId`.
- **Codecs** are registered with the class: `register_component::<C>("infantry.suppression", Class::Hashed { version: 1 })`.
- **Typed migrations:** keep `mod v1 { struct Suppression {…} }` and `impl From<v1::Suppression> for Suppression`.
  Migrations are kept forever, or until a save-break ADR.
- **Relationships are saved as `SimId`;** `Entity` never appears.
- **Load:** build a fresh sim from the header, spawn in `SimId` order through the funnel code (ADR-0005), restore
  resources and pending commands, run every `Derived` rebuild, remap IDs through the string tables (ADR-0004).
- **Fitness tests** (`fl_test_utils`): byte-identical `save(load(save(w))) == save(w)`, and the **branch test**: run A
  to tick M; run B to tick N < M, save, load fresh, run to M; hashes equal every tick. Loading re-creates archetype
  tables in a different order from the live run, so the branch test also catches table-order dependence.
- M0 builds the format, the registry and codecs for core state; each feature adds its codecs as it lands.

### Consequences

* Good, because the hash and saves see all sim state by construction.
* Good, because saves exist before any feature, so every feature is born save-aware.
* Good, because the branch test enforces the narrowed ordering rule (ADR-0005) mechanically.
* Bad, because every component needs a classification and, if hashed, `Serialize + Deserialize + SimHash` and a
  version. That is a few lines per type.
* Bad, because migrations accumulate; that is the price of loadable saves.

### Confirmation

* The startup classification check, run in every test binary.
* The round-trip and branch tests in `gate task` (smoke) and `gate pr` (several split points N).
* `serde-reflection` snapshots of every save codec; a format change without a version bump fails (ADR-0011).

## Pros and Cons of the Options

### Opt-in hashing, saves later

* Good, because it has the least upfront work.
* Bad, because forgotten registrations are invisible, and saves rot in a retrofit.

### Mandatory classification with codecs

* Good, because it is complete, versioned and testable.
* Bad, because of per-type boilerplate.

### Reflection-based saves

* Good, because there are no per-type codecs.
* Bad, because there is no versioning, migration or validation, and reflection output isn't a stable format.

## More Information

* Spec §5.4 (classification, state hash), §5.8 (save and load), §12.3.
* Research: `architecture-best-practices.md` N6, N7, C6, C7; Factorio FFF #270 and the "heavy mode" wiki page.
* Related: ADR-0004 (string tables), ADR-0005 (spawn funnel), ADR-0012 (hot reload rebuild uses save/load).

## Refinements (2026-10-24)

From the final M0 review (`final-review-m0.md`); controller rulings R70, R72, R73, R74.

* **R70: save compatibility rests on the codecs.** `MatchHeader::check_save` requires the same feature ids, each
  saved at a version no newer than the build's (an older feature version loads), the same mods, and a save format
  version no newer than the build's. Per-component and per-resource codec versions and their typed migrations decide
  what decodes (`LoadError::NoMigration` otherwise). `check_replay` and `check_handshake` stay strict (ADR-0007).
  `codec_formats` heads each codec block with the older versions it reads (`<name>@<v> reads 1, 2:`, `reads older` for
  a `Codec::custom`), and the format guard fails a codec whose version rose without reading the committed version.
  The codec snapshot covers every catalog feature (`fl_test_utils/tests/formats.rs`), and `Codec::custom` takes the
  trace of the bytes it writes.
* **R72: the framework enforces `Transient`.** `Transient` means per-tick, and the sim makes it so: a core
  `Input::Housekeeping` system removes every `Transient` component from every entity and resets every `Transient`
  resource at the start of each tick. A `Transient` resource is `Class::transient()` (reset to `Default`),
  `Class::TransientReset(fn)`, or `Class::TransientKept("reason")` when its owner empties it at another point
  (`Journal`, `SnapshotOut`, `CommandInbox`, message buffers). `step_tick` clears every `add_message` buffer at the
  end of the tick: a message is read in the tick it was written, and state that must reach the next tick is `Hashed`.
* **R74 (Minor 15): one `Derived` rebuild per feature.** `Class::Derived` names no rebuild. A feature registers at
  most one with `SimBuilder::register_rebuild`, which rebuilds all of its `Derived` state in the order that state
  needs; `load_world` runs the rebuilds in feature load order (core first, each feature after its dependencies),
  never in type-name order. State kept by component hooks (`SimIndex`) needs none.
* **R73 (I3, I6): the classification check runs during the match too.** In debug builds `step_tick` re-runs
  `check_classification` whenever a tick registered new component or resource types, and asserts that no entity with
  a classified component lacks a `SimEntity`. A type counts as a sim type when any path segment of its name, generic
  arguments included, is a sim crate's, and every Bevy message buffer counts, so only `SimBuilder::add_message` adds
  one. `ChildOf`, `Children` and `Observer` are forbidden in the sim world. `SimEntity`'s field is private: only the
  spawn code and loading create one.
* **Saves between ticks never fail the match.** `save_world` returns `SaveError` (spawn or despawn requests
  waiting) instead of panicking, and the runner answers `SimQuery::Save` with `SimReply::Error`.
* **I1: pending commands are remapped by name.** `load_world` maps every pending command's type id through the
  save's `commands` string table (ADR-0004); a name the sim lacks is `LoadError::Command`.

## Refinements (2026-10-24, M1)

Status: accepted at the M1 Task 1 review (rulings R69, R84), with ADR-0014–0016. Controller rulings R78, R81.

* **R78: M0 saves load under M1.** A save codec may register
  `Codec::fill_missing(fn(&mut World) -> Result<(), String>)`. It fills a Hashed state name that is absent
  (Derived state has no codec: the Derived rebuilds make it) from a save whose owning feature version is older than the build's (`MatchHeader.features`). Fills run after every
  saved value is restored and before the Derived rebuilds, in feature load order and then by state name; an `Err` is
  `LoadError::Codec { name, message }`. Without a fill, or in a save of the same feature version, loading is
  unchanged (a missing Hashed resource stays absent, which saves of a match without a map rely on).
* **Core becomes version 2 in M1** (`CoreFeature::version()`, unchanged through M1). It registers fills for
  `core.terrain` (a `TerrainGrid` from the content map whose size and cell match the saved `MapInfo`: that map's
  layers, or open ground when no matching map has layers; an error when several maps with layers match; none without
  a `MapInfo`), `core.path_queue` (empty) and `core.path_follow` (`Idle` on every unit with `Mobility`, in `SimIndex`
  order, so a loaded unit equals a live one). Changed codecs, such as `core.move_order` v2, migrate as before (R70).
* **Evidence.** M0 saves therefore load under M1, as the protected `m0_testbed.save` fixture shows: a testbed save at
  tick 40 generated from tag `m0` (R81), loaded under M1 by `crates/fl_test_utils/tests/m0_save.rs`.
* **Confirmation.** `fill_missing_runs_for_an_older_feature_save`, `fill_missing_skipped_for_the_same_version`,
  `fill_error_is_a_codec_load_error` (Task 5, `fl_sim/tests/save.rs`); `m0_testbed_save_loads_under_m1` (Tasks 5
  and 8) and `m0_save_round_trips_after_load` (Task 5); `move_order_v1_migrates` (Task 8).

## Refinements (2026-10-25, M2 prelude)

Status: proposed (M2 prelude Task 3); the controller accepts it at that task's review under the user's delegation
(R85), and the plan's final report lists it for ratification. With ADR-0017 and ADR-0018; controller rulings R83,
R148.

* **`Codec::migrate_with(version, fn(Old, &LoadCtx) -> Result<T, String>)`** for a migration that needs the string
  tables, such as one that turns a closed enum into registered kind ids: it reads bytes saved at `version` as postcard
  `Old` and converts them with the saved and current tables at hand; an `Err` is `LoadError::Codec { name, message }`.
  `migrate_from` (`Old: Into<T>`) stays for migrations that need no tables. Both follow the R70 rules: one migration
  per older version, and the format guard fails a codec whose version rose without reading the committed one.
* **`LoadCtx::current_id(table, name) -> Option<u32>`**: the current dense id of `name` in `table`, next to the
  existing `remap`, `saved_tables` and `current_tables`.
* **Core version 3** (Task 13, `SIM_VERSION` 3, MCR-001): `core.move_order` becomes codec v3 (`OrderQueue`, a
  `Codec::custom` saving kind ids remapped through the `"orders"` table), migrated from the frozen
  `units::v2::MoveOrder` with `migrate_with`; `core.activity_stack` (v1) registers a `fill_missing` that gives each
  unit with a current order that order's root activity (`OrderBehaviour::begin`), or an empty stack (ADR-0018). The
  evidence is the protected `m1_testbed.save` fixture, generated from tag `m1` (Task 2, R83).
* **Core version 4** (Task 15, `SIM_VERSION` 4, MCR-002): `core.path_queue` v2 (two lanes; v1's `pending` migrates
  to the command lane, the background lane empty) and `core.path_follow` v2 (`moved`, migrated from v1's `planned`)
  (ADR-0015 M2 prelude refinement).
* **Relationship codecs save `SimId`s** (ADR-0018): `RelatedTo<R>` is Hashed under `R::NAME` with the target's
  `SimId`; `Related<R>` is Derived and rebuilt by the framework after a load, once the hashed state is restored and
  before the feature rebuilds, because a source can load before its target.
* **New state of the prelude** (ADR-0017): `core.conditions` and `core.modifiers` (Hashed; condition and stat ids
  saved and hashed by name), `core.effect_queue` (Hashed, present only while it holds late effects, which it saves by
  effect name), `core.effective_stats` (Derived, storing each stat's base next to its effective value) and
  `core.tags` (Derived), both rebuilt by core's one rebuild (R74); `core.stats_dirty` is `TransientKept`, emptied
  only by the recompute (R149).
* **Confirmation.** `m1_testbed_save_loads`, `m1_save_continues_with_m1_pose_trace` (Task 2);
  `m1_save_migrates_to_order_queue`, `activity_stack_round_trips`, `relations_survive_save_and_load` (Task 13);
  `path_queue_v1_migrates_to_command_lane`, `path_follow_v1_migrates_moved` (Task 15);
  `effective_stats_rebuild_after_load` (Task 17); the `formats__save_codecs` snapshot (`protected`).
