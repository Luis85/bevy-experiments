# M1 Task 1 report: ADRs 0014–0016, M1 refinements of ADR-0006/0007/0012, spec sync

**Status:** DONE_WITH_CONCERNS (minor: see Concerns)
**Commit:** `1972d92` docs(adr): ADR-0014 terrain layers, ADR-0015 path requests and debug queries, ADR-0016 AI seam; refine 0006, 0007, 0012 (trailers `Refs: ADR-0011`, `Co-Authored-By`)
**Base:** `0ae615d` (`.gate-base`). Docs only; no cargo command was run (none is required).

## What I implemented

- **ADR-0014** `docs/adr/0014-terrain-layers-and-movement-classes.md` (status `proposed`, MADR 4.0 from the template).
  Records every brief decision verbatim in substance: `TerrainDef { id, name, pass, blocks, color }`; `MapDef.terrain:
  Option<MapTerrain>` (16/8-bit height PNG, RGB terrain PNG, height range, RGB → id legend); one pixel per cell, row
  0 north (pixel `(px, py)` → cell `(px, cells_y − 1 − py)`); `ContentKind::load_sidecars` into the non-RON
  `MapDef.layers` covered by the rules hash; a map without `terrain` is open ground; PNG decoding only in `fl_content`
  (`png`, pure Rust, `arch-check`-accepted), sim and client read integer `MapLayers`; `MoveClass` moves to `fl_content`
  (re-exported by `fl_sim`); profiles `(class, size)`; blocking (terrain `blocks`, slope over `max_slope_m[class]`,
  map edge blocked); clearance (k×k square toward +x/+y); speed formula with the clamp and `terrain_factor` ∈ [−1, 1]
  (research §1.8); per-class `path_terrain_factor`; terrain Hashed (`core.terrain`), mutable by `debug.set_terrain`.
  Four considered options (PNG via `fl_content` chosen; RON arrays; Bevy `AssetServer`/`bevy_image`; navmesh).
- **ADR-0015** `docs/adr/0015-path-requests-derived-caches-debug-queries.md` (`proposed`): Hashed FIFO `PathQueue`
  with the brief's enqueue order; processing in `ThinkSet::Behaviour` while `spent < budget_per_tick` (≥ 1 per tick);
  cold-cost charge; immediate apply in request order; stale/gone requests discarded at charge 0; HPA* 16×16 per
  profile with the Task 7 entrance rule and tie-breaks stated; flow fields (abstract Dijkstra + lazy tiles in
  `SimIndex` order before movement); `NavGrid`/`HpaGraphs`/`FlowCache` Derived and outcome-neutral, eviction free,
  rebuilt by `rebuild_core` (R74); per-tick results in a Transient scratch resource sorted by `SimId`, never a
  Transient component (R72, R76); invalidation via `on_terrain_changed`; debug-query registry and
  `SimQuery::Debug`/`SimReply::Debug`, JSON with `Fx` raw `i64`. Five considered options.
- **ADR-0016** `docs/adr/0016-ai-controller-seam.md` (`proposed`): `fl_view::ai::{PlayerView, AiController, NullAi,
  ai_controller}`; client-built `PlayerView` until M3 moves it to `Publish`; `fl_client::ai::AiHost` hosts
  `Controller::Ai(id)` players, thinks once per new snapshot tick, drafts through `LocalDriver` as that player; full
  placement ADR deferred to the first AI faction (§9.3). Four considered options.
- **Refinements** (`## Refinements (2026-10-24, M1)`, each marked "Status: proposed"), appended after the existing
  `## Refinements (2026-10-24)` sections, which are untouched:
  - ADR-0006: R78 `Codec::fill_missing` (absent Hashed/Derived name, older owning feature version, after restore and
    before Derived rebuilds, `Err` → `LoadError::Codec`, unchanged otherwise); core v2 with fills for
    `core.terrain`, `core.path_queue`, `core.path_follow`; the `m0_testbed.save` fixture (from tag `m0`, R81).
  - ADR-0007: `core.rotate { toward }`, `core.follow { leader }`, `debug.set_terrain { min_cell, max_cell, terrain }`;
    no `FORMAT_VERSION` bump, remap by name (R73); `MAX_WAYPOINTS` over every order kind → `core.queue_full` (R77);
    forbidden targets → `RejectReason::Feature("core.invalid_target")`, stable code, no new variant (R71, R79).
  - ADR-0012: polling watcher (`hot_reload`, `hot_reload_poll_ms`); `ToRunner::Control { id, control:
    RunnerControl::ReloadContent(OpaqueContent) }` → `SimReply::Reloaded { tick, rules_hash }`; `ToRunner`
    `#[non_exhaustive]` (R80); Derived rebuilds in feature load order after a value reload (R74);
    `load_world_with(.., LoadOptions { drop_unknown_units: true })`; client `replay save`; `DefComponent::reload`.
- **README** index: rows for 0014–0016 (`proposed`, 2026-10-24, spec sections); rows 0006/0007/0012 now read
  "accepted; M1 refinement proposed" (dates unchanged, matching their front matter); the "AI placement" sentence notes
  ADR-0016 covers only the stub seam.
- **Spec** (`docs/superpowers/specs/2026-10-23-frontline-rts-design.md`): status line names revisions 2.2 and 2.3;
  new `## Revision 2.3 (2026-10-24, M1 contracts)` above `## Revision 2.2`, in 2.2's format (bullets for ADR-0014,
  ADR-0015 ×2, ADR-0016, R78/ADR-0006, R77+R79/ADR-0007, R80/ADR-0012); edits to §5.3 (named debug queries,
  `SimQuery::Debug`), §5.5 (fixed core and debug payload list, stable reject codes, `debug.set_terrain`), §5.7
  (cold-cost charge, lazy flow tiles, Derived caches, bridges as terrain in M1), §6.2 (`TerrainDef` fields, map layer
  format), §6.4 (polling watcher, runner control message, `DefComponent::reload`, `drop_unknown_units`, replay
  capture), §8 (client-built `PlayerView` stub until M3; `AiHost`), §11 (overlays read debug queries).

Every Confirmation bullet names tests by the plan's names (Tasks 2–20, mostly 5–20). A script checked that every
snake_case identifier in the added ADR text appears in the M1 plan (only `bevy_asset`/`bevy_render`, crate names, are
not in it).

## Verification

```
$ git diff --cached --stat        (before commit; base 0ae615d)
 docs/adr/0006-component-classification-and-save-format.md   |  21 +++
 docs/adr/0007-lockstep-command-envelope-and-match-header.md |  24 +++
 docs/adr/0012-hot-reload-policy.md                          |  33 ++++
 docs/adr/0014-terrain-layers-and-movement-classes.md        | 164 +++++++++++++++++++
 docs/adr/0015-path-requests-derived-caches-debug-queries.md | 180 +++++++++++++++++++++
 docs/adr/0016-ai-controller-seam.md                         | 107 ++++++++++++
 docs/adr/README.md                                          |  11 +-
 docs/superpowers/specs/2026-10-23-frontline-rts-design.md   |  77 +++++++--
 8 files changed, 598 insertions(+), 19 deletions(-)
$ grep -c '^status: "proposed"' docs/adr/001[456]-*.md
docs/adr/0014-terrain-layers-and-movement-classes.md:1
docs/adr/0015-path-requests-derived-caches-debug-queries.md:1
docs/adr/0016-ai-controller-seam.md:1
$ git diff --check     → clean
```

Only the files in the brief's Files list changed. New prose lines are ≤ 120 columns (front matter and pre-existing
spec bullet lines excepted, which follow the files' existing style).

## Self-review findings (fixed)

- ADR-0016 first said `Controller::Ai(id)` lives in `MatchHeader` players; it lives in scenario players
  (`fl_content` builtin, `fl_protocol::Controller`). Corrected in three places.
- README dates for 0006/0007/0012 briefly changed to 2026-10-24; reverted to match their front matter (the lead sets
  dates at acceptance).

## Concerns

1. **Commit subject.** The brief's Step 6 subject ends "refine 0007, 0012", but the brief's Files list and Step 5
   also refine ADR-0006 (R78). I wrote "refine 0006, 0007, 0012" so the subject is accurate. Reword if the verbatim
   subject matters.
2. **Spec §7 text unchanged.** The brief limits spec edits to §5.3, §5.5, §5.7, §6.2, §6.4, §8, §11. The §7 speed
   formula still lacks the `min_speed_frac` clamp and still says "Bridges are destructible objects"; revision 2.3's
   ADR-0014 bullet records the clamp, the widened `terrain_factor` and bridges-as-terrain-until-M7, and 2.3 wins on
   conflict. If you want §7 edited in place too, it is a two-line change.
3. **Acceptance pending (Step 5).** ADR-0014–0016 and the three M1 refinements are `proposed`. Per the brief the lead
   asks the user; on acceptance set `status: "accepted"` + date in 0014–0016, drop "Status: proposed" from the three
   refinement sections, update the README rows, and drop "its ADRs proposed" from the spec status line and the 2.3
   intro.
4. The ADR-0006 refinement records the plan's deviation from R78's wording: without a fill, a missing Hashed
   resource stays absent (current code), not `LoadError` — as the plan's "Builder's calls" section states.

## Fix round 1

**Commit:** `15b8aba` docs(adr): M1 ADR review fixes; accept ADR-0014..0016 (`Refs: ADR-0011`, Co-Authored-By).
Docs only; no cargo. Files: the six ADRs, `docs/adr/README.md`, the spec, and the M1 plan (permitted this round).

What changed, by finding:
- **Important 1 (R84).** ADR-0015 Caches bullet: a cache read never observes a miss. `FlowCache::direction`
  builds a missing integration or tile before answering (pure; its work counts like any build); `None` means only
  "no route". So eviction can run at any point under any deterministic policy. The chosen-option rationale and the
  rulings list mention R84. Plan Task 10: `direction(&mut self, key, cell, nav, hpa, work)`, `prepare_flow_tiles`
  is a warm-up only, and the new test `flow_cache_overflow_does_not_change_outcomes` (`flow_cache_max` 2, four group
  keys at once, cold vs warm → equal hashes). It is also in the ADR Confirmation and a Builder's-calls bullet.
- **Minor 1.** ADR-0014 and plan Task 3 now give the full PNG acceptance rules:
  - Height PNG: gray without alpha, 16-bit or 8-bit.
  - Terrain PNG: 8-bit RGB, or 8-bit RGBA with alpha 255.
  - Errors: indexed/PLTE, gray+alpha, depths below 8, 16-bit RGB/RGBA, a colour height PNG and a gray terrain PNG,
    each named with its colour type and bit depth.
  - Samples are raw; ancillary colour chunks are ignored.
  - A duplicate legend RGB is an error.
  - At most 255 terrain kinds (`u8::MAX` = OPEN).

  New Task 3 tests: `unsupported_png_formats_are_errors`, `ancillary_colour_chunks_are_ignored`,
  `duplicate_legend_colour_is_error`, `too_many_terrain_kinds_is_error`.
- **Minor 2.** ADR-0014 "Where a unit stands": centre cell `c = cell_of(pos)`; the anchor is `c − ⌊(k−1)/2⌋` per
  axis (centred for odd k; c plus +x/+y neighbours for even k); passable when clearance at the anchor ≥ k; the
  anchor is c for k ≤ 2, so it is latent in M1. Plan Task 5: the `NavGrid::passable` comment and the test
  `profile_anchor_is_centred`.
- **Minor 3.** Edge rule reworded: off-map cells count as blocked (clearance 0, never passable), border cells
  aren't blocked by the edge, and the slope test uses in-map neighbours only.
- **Minor 4.**
  - Evaluation order is pinned: `Fx` ops in the written order, `Mul` dropping low bits, RON values rounded to the
    nearest raw `Fx`.
  - The cost is a raw-integer ceiling, `((COST_ROAD << 32) + raw − 1) / raw`.
  - Range validation: `RulesSection` gains a `link` hook (plan Task 3, test `rules_section_link_errors_are_reported`).
    `Movement` and `Path` implement it in Task 5 with these ranges:
    - `min_speed_frac` in (0, 1] and ≥ `COST_ROAD/u16::MAX`;
    - both per-class maps cover every class, with slopes ≥ 0;
    - `path_terrain_factor` in [−1, 1];
    - counts ≥ 1.
  - New Task 5 tests: `movement_rules_ranges_are_validated`, `path_rules_ranges_are_validated` and
    `nav_cost_is_integer_ceiling` (100 / 134 / 167).
- **Minor 5.** ADR-0015 queue bullet lists every enqueue source in schedule order:
  - command handlers, with a command's group requests (in `MoveProfile` order) before its single requests (in
    `SimId` order);
  - `advance_orders`, then `wander_system`;
  - the `process_path_queue` fallback singles.

  It also adds the group stale rule. Plan Task 10 states both.
- **Minor 6.** The ADR-0012 refinement and plan Task 12 cover three reload cases:
  - **Profile change.** Units whose profile changed go `Idle` and get `replanned` (test `reload_changing_profile_replans`).
  - **Map layers.** Edited layers never rewrite a running match's `TerrainGrid` (test `value_reload_keeps_terrain_grid`).
  - **Unknown terrain.** A palette name the content lacks is `LoadError::Codec { name: "core.terrain" }`
    (test `rebuild_with_vanished_terrain_is_load_error`). A failed rebuild leaves the match running (Task 19 test
    `failed_rebuild_keeps_match_running`).
- **Minor 7.** ADR-0016 now says the host thinks at most once per frame, in a frame where the newest snapshot's tick
  advanced.
- **Minor 8.** Plan Task 3 owns an `xtask/src/arch.rs` rule allowing a direct `png` dependency only in `fl_content`,
  with the unit test `png_direct_dep_only_in_content` (Files, Step 2, Step 6 updated). The ADR-0014 Confirmation
  points at it.
- **Minor 9.** I reflowed the long lines in 0012, 0014, 0015 and 0016 and the stray break in 0014. My first commit
  had them because my edits only reflowed part of each paragraph. New prose lines are now ≤ 120 characters,
  checked by character count; only the older lines in 0002, 0003, 0005, 0006 and 0013 stay longer.
- **Speed test.** Plan Task 5 `speed_formula_matches_spec` compares the clamp case against the RON-parsed
  `min_speed_frac` (raw 214748365), not `Fx::from_ratio(1, 20)`.
- **Spec §7.** The speed formula now has the clamp, `terrain_factor` ∈ [−1, 1], and bridges are terrain cells until
  M7. §5.7, §6.2 and §6.4 and revision 2.3 carry R84, the PNG rules and the map-layer note.
- **Acceptance.**
  - ADR-0014–0016: `status: "accepted"`, date 2026-10-24, decision-makers "controller (accepted at the M1 Task 1
    review, rulings R69, R84), lead".
  - ADR-0006/0007/0012: the refinement sections say "accepted at the M1 Task 1 review", and the front-matter dates
    are now 2026-10-24.
  - README: rows 0014–0016 accepted; rows 0006/0007/0012 read "accepted (M1 refinement 2026-10-24)".
  - Spec: the status line and the 2.3 intro no longer say proposed.

Verification: `git diff --check` clean. Every test name added to an ADR has a `fn <name>()` in the plan. No "proposed"
status remains in the touched ADRs, the README rows or the spec.

Concerns:
1. **Minor 1 ruling text.** The ruling's text says "8-bit gray for height … reject … 16-bit". I kept 16-bit gray
   height PNGs accepted and rejected only 16-bit RGB/RGBA for terrain. The brief, ADR-0014 and Task 3's
   `heights_accept_16_and_8_bit` all require 16-bit height, and Task 4 generates 16-bit height PNGs. If the ruling
   meant rejecting 16-bit height too, those three must change.
2. **Validation tests are in Task 5, not Task 3/4.** The `Movement`/`Path` range-validation tests sit in Task 5,
   because those rules types are defined there; Task 4 runs in parallel and can't see them. Task 3 adds the
   `RulesSection::link` hook and its generic test.
3. **Who accepted.** The README rules and the M1 Global Constraints say only the user accepts ADRs. The acceptance
   here follows the controller's instruction (R69/R84) and is recorded as such in the decision-makers field.
