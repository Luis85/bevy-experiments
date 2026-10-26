# M1 Task 1 re-review, fix round 1 (1972d92..15b8aba)

## Finding verdicts

- Important 1 (flow cache eviction visible to reads): ADDRESSED. ADR-0015 Caches bullet makes "a cache read never observes a miss (R84)" the invariant; plan Task 10 `direction(&mut self, .., nav, hpa, work)` rebuilds on miss, `prepare_flow_tiles` is a warm-up only, and `flow_cache_overflow_does_not_change_outcomes` (flow_cache_max 2, four keys, cold vs warm) is in the plan (~1057) and the ADR Confirmation. The rebuild is not charged to the path budget.
- Minor 1 (PNG format not exhaustive): ADDRESSED. ADR-0014 pins accepted formats, errors naming file/type/depth, raw samples, duplicate legend RGB, 255-kind cap; plan Task 3 and the four new tests match. 16-bit gray stays accepted (accepted by lead).
- Minor 2 (anchor cell undefined): ADDRESSED. "Where a unit stands": anchor = c - floor((k-1)/2); plan `NavGrid::passable` comment and `profile_anchor_is_centred` agree (size 3 at (5,5) -> clearance(4,4)).
- Minor 3 (edge wording): ADDRESSED. Off-map cells blocked (clearance 0); border cells not blocked by the edge; slope uses in-map neighbours.
- Minor 4 (evaluation and ranges): ADDRESSED. Evaluation order and raw-integer ceiling pinned; `RulesSection::link` hook and ranges in ADR and plan Tasks 3/5; tests `nav_cost_is_integer_ceiling` (100/134/167, arithmetic checked), `movement_rules_ranges_are_validated`, `path_rules_ranges_are_validated`. Range tests live in Task 5 (accepted by lead). `LinkCtx` is public, so a section impl outside `fl_content` can use it.
- Minor 5 (enqueue sources/orderings): ADDRESSED. ADR-0015 queue bullet lists command handlers, `advance_orders`, `wander_system`, fallback singles, with in-command order and the group stale rule; plan Task 10 repeats them.
- Minor 6 (ADR-0012 reload cases): ADDRESSED. All three cases in ADR-0012 and plan Task 12/19 with tests (`value_reload_keeps_terrain_grid`, `rebuild_with_vanished_terrain_is_load_error`, `reload_changing_profile_replans`, `failed_rebuild_keeps_match_running`).
- Minor 7 (0016 think cadence): ADDRESSED. "at most once per frame, in a frame where the newest snapshot's tick advanced".
- Minor 8 (arch-check over-claim): ADDRESSED. Real rule owned by Task 3 (`xtask/src/arch.rs`, test `png_direct_dep_only_in_content`); xtask is on the contract track, which Task 3's gate already includes.
- Minor 9 (formatting): ADDRESSED. Only lines over 120 chars left in 0012/0014/0015/0016 are the pre-existing front-matter `consulted:` lines (0014:5, 0015:5); stray break fixed.
- Speed-test note (Task 5 `speed_formula_matches_spec`): ADDRESSED. Compares to the RON-parsed min_speed_frac (raw 214748365), never `Fx::from_ratio`; ADR-0014 says the same.
- Spec §7 sync (known item): ADDRESSED (clamp, [-1,1], bridges as terrain cells).

## New breakage in the fix diff

- Minor, plan Task 3 interface block (diff ~486-490): the new `RulesSection::link` and "at most 255 TerrainDefs" comment lines were inserted between the `load_sidecars` line and its continuation comment ("an Err becomes a ContentError on `_file`"), which is now orphaned under the registry line. Move the continuation up or the new lines below.
- Minor, spec Revision 2.3 ADR-0014 bullet still lists "the map edge" as a blocking source; ADR-0014 now says border cells are not blocked and only off-map cells count as blocked. Reword for consistency.
- Minor, plan Task 10 / ADR-0015: `flowfield.cells` / `flowfield.abstract_nodes` counters now include on-read rebuilds, so they depend on cache state (cold vs warm, or right after a load). Hashes stay neutral, but no test or ADR text says counters are exempt from cache-neutrality; say so, or keep read-time rebuild work out of those counters, so branch and perf comparisons don't surprise.

No Critical or Important.

## Out-of-scope observations

None.

## Verdict

**Fix round:** All findings addressed, no new Critical/Important breakage.
