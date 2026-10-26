# M1 Task 1 review: ADR-0014–0016, M1 refinements of ADR-0006/0007/0012, spec sync

Reviewed `0ae615d..1972d92` from `review-t1.diff`, which I read in two passes because of its size. This is a design review of docs only, so I ran no cargo commands.

### Spec Compliance

- ✅ **Spec compliant.**
  - Every decision in the brief is recorded with the brief's names, signatures and values:
    - ADR-0014: `docs/adr/0014-terrain-layers-and-movement-classes.md:44-87`
    - ADR-0015: `docs/adr/0015-…md:44-98`
    - ADR-0016: `docs/adr/0016-ai-controller-seam.md:34-55`
    - the refinement sections: 0006 diff:22-41, 0007 diff:53-75, 0012 diff:87-118
  - The README rows are added (diff:600-616).
  - All seven spec sections are edited: §5.3 diff:691, §5.5 diff:703-711, §5.7 diff:726-730, §6.2 diff:742-750, §6.4 diff:763-772, §8 diff:785-788, §11 diff:801.
  - Revision 2.3 sits above 2.2 (diff:643-678), and the status line names revisions 2.2 and 2.3 (diff:630-631).
  - The changed files are exactly the files the brief lists.
  - Each ADR has Context, Decision Drivers, at least two options with pros and cons, Consequences and Confirmation.
- ✅ **Names match the plan.**
  - Check: every backticked snake_case identifier on an added line exists in the M1 plan. The only misses are the crate names `bevy_asset` and `bevy_render`.
  - Check: I spot-checked 26 Confirmation test names, and each sits in the task the ADR attributes it to. Examples: `terrain_kind_added_changes_id_set` (plan:1325, Task 12), `terrain_view_from_base_testbed` (plan:1172, Task 17), `brp_debug_query_round_trip` (plan:1257, Task 18).
- ⚠️ **Spec §7** still has the speed formula without the clamp and the "destructible bridges" text. This is known; the lead is syncing it in the fix round.
- ⚠️ **Status:** the ADRs and refinements are still `proposed`; acceptance is Step 5, done by the lead.
- ⚠️ **Task 5's `speed_formula_matches_spec`** (plan:671) expects `(0.0, 0.0)` to be "clamped to 1/20" exactly.
  - `Fx` from RON rounds to the nearest raw unit (`fl_math/src/fx.rs:88-101`), so 0.05 parses to raw 214748365.
  - `Fx::from_ratio(1, 20)` truncates (fx.rs:63-73) to raw 214748364.
  - The test has to compare against the parsed `min_speed_frac`, not `from_ratio`. The other three exact cases do match: I computed them with `fixed`'s flooring multiply.

### Strengths

- **Profiles come from content, not from live units.** "Navigation data is kept per profile that the content's units use" (0014:69-70, plan `content_profiles`). That keeps `HpaGraphs` a pure function of Hashed state plus content, which the whole outcome-neutrality argument in ADR-0015 needs.
- **The cold-cost argument is stated precisely** (0015:48-53, 66-70):
  - the charge counts nodes plus LOS cells;
  - there is no path-result cache;
  - the flow cold cost is "the same whether built now or cached" (plan:996-997).
  - `flow_cache_cold` is named as a test oracle (Consequences).
- **Search tie-breaks and the entrance rule are pinned exactly as Tasks 6 and 7 state them**: `(f, h, y, x)`, `(f, h, node index)`, strictly lower g, the run rule with `start + (len − 1) / 2`.
- **The debug-query contract is complete and matches plan Task 2 word for word.** That covers the error strings, a duplicate panicking with both features named, the registry being External and frozen, dense ids never leaving the sim, and `Fx` sent as raw `i64`.
  - Check: `SimReply` already carries `serde_json::Value`, and `fl_view/tests/formats.rs:5-8` doesn't trace it, so `SimQuery::Debug { args: Value }` doesn't break the format snapshot.
- **Rules-hash coverage of the layers is real.**
  - Check: `fl_content/src/hash.rs:65-75` hashes the postcard bytes of every rules-hashed kind item.
  - `MapDef.layers` is `skip_deserializing` but still serialized, so every pixel counts, as 0014:58-60 claims.
- **The PNG crate placement is sound.**
  - Check: `xtask/src/arch.rs:238-244` constrains only `bevy*` crates for the Content role and the Sim allowlist, so `png` and its dependencies pass.
- **The R78 fill is well specified.**
  - Ordering: after restore, before rebuilds, then load order, then name.
  - The `core.terrain` map-matching rule is explicit, including the "several maps" error.
  - Check: `MapInfo` has no map id (`fl_sim/src/map.rs:11-17`), which justifies matching by size and cell.
  - Check: `ReplayEvent::ContentReload` already exists (`fl_sim/src/replay.rs:47`), so ADR-0012's replay claim needs no format change.
  - Check: `RejectReason::UnknownContent` already has the code `core.unknown_content` (`fl_sim/src/commands.rs:139`), as ADR-0007's refinement assumes.
- **ADR-0016 keeps sim types out of the seam.** `PlayerView` is made of `fl_view` `EntityRecord`s, `think` takes `StringTables` and returns `CommandDraft`s, and nothing names an `fl_sim` type.
  - Check: `fl_view/src/driver.rs:29-31` stamps per `draft.player`, so "through `LocalDriver` as that player" works with today's driver.
  - The implementer's self-correction is right: `Controller::Ai` lives in scenario players (`fl_content/src/builtin.rs:122`), not in `MatchHeader`.

### Issues

#### Critical (Must Fix)

None.

#### Important (Should Fix)

1. **`docs/adr/0015-path-requests-derived-caches-debug-queries.md:66-70`: "Eviction is therefore free to be any deterministic policy" is false as the contract stands.**
   - **Why it's wrong.** Cold-cost charging makes caches neutral only for the *charge*. They are also neutral for *behaviour* only if no reader can ever see a miss. Under the plan's Task 10 contract a reader can:
     - `FlowCache::direction` is read-only and returns `None` "when the tile is missing" (plan:1001).
     - LRU eviction past `flow_cache_max` (32) by "(last-used tick, then key)" runs during `ensure_tile`.
     - `prepare_flow_tiles` ensures tiles for every Flow unit before `move_system` reads them.
   - **Failure case.** More than 32 distinct `(destination cluster, profile)` keys in use in one tick, which the Task 13 scale scenario can reach.
     - Some keys ensured early in `prepare_flow_tiles` are evicted before `move_system` reads them.
     - Which keys are evicted depends on what the cache held at the start of the tick: warm, cold (`flow_cache_cold`), or empty right after a load.
     - So `cache_does_not_change_outcomes` and `branch_mid_flow_follow` can diverge. `move_system`'s behaviour on `None` is also unspecified (probably Idle followed by a Hashed re-request).
   - **Fix.** Add the missing invariant to ADR-0015: a cache read never observes absence. Either of these works:
     - (a) Eviction runs only at one fixed point (the start of `prepare_flow_tiles`), only removes keys that no live `Flow` unit references, and `flow_cache_max` is a soft cap.
     - (b) A miss at read time computes the value, which is pure.
   - **Test.** Add a Confirmation item: more than `flow_cache_max` concurrent group keys, run with `flow_cache_cold` both true and false, must give equal hash streams.

#### Minor (Nice to Have)

1. **0014:52-63: the PNG format is not exhaustive.** It should state:
   - Colour types and bit depths outside 16/8-bit gray and RGB8/RGBA8 (for example indexed/PLTE, which optimizers and editors emit, gray+alpha, 1/2/4-bit, RGB16) are a content error naming the file and the type found.
   - Samples are raw, with no gAMA, sRGB or iCCP conversion, so legend matching cannot depend on the decoder.
   - Two legend entries with the same RGB are an error.
   - The palette is capped at 255 entries, because `types: Vec<u8>` and `TerrainGrid::OPEN = u8::MAX` (plan:603). That cap also bounds the `debug.set_terrain` appends at 0014:82-83.
2. **0014:73-74: clearance anchors the k×k square at the cell and extends toward +x/+y, but neither the ADR nor the plan says which cell a size-k unit's position maps to.**
   - Using `cell_of(pos)` as the anchor offsets wide units' clearance toward +x/+y by up to (k−1) cells, which is asymmetric wall clearance.
   - Every base unit is size 1 (width ≤ 3.6 m at 4 m cells), so this is latent in M1, but `t.wide_tank` (size 2) is exercised.
   - Fix: define the anchor (for example `cell_of(pos − (k−1)·cell/2)`) or accept the asymmetry explicitly.
3. **0014:71-72: "The map edge counts as blocked" reads as if border cells are blocked.** The plan's clearance recurrence treats cells *outside* the map as 0 (plan:704). Reword to say that.
4. **0014:75-80 and 85-86: the speed and cost formula is given, but not its integer evaluation or the rules' valid ranges.** Pin these:
   - `pass + (ONE − pass) × factor` with `Fx`'s flooring `Mul`, then the clamp; cost = integer ceil of `Fx(COST_ROAD) / factor`.
   - Validation: `min_speed_frac` must be in `[COST_ROAD/u16::MAX, 1]`, otherwise the cost divides by zero or overflows `u16`.
   - `max_slope_m` and `path_terrain_factor` must cover all of `MoveClass::ALL`, otherwise indexing panics.
   - `path_terrain_factor` must be in [−1, 1], and `max_clearance` and `cluster_cells` must be ≥ 1.
   - Content is strict, and nothing in the ADR or the plan says to validate these.
5. **0015:44-47: the list of enqueue sources is incomplete.** Two more exist:
   - `wander_system`, a separate chained system (plan:871-875);
   - the Task 10 fallback "a Single request at the back" from inside `process_path_queue` (plan:1010).

   Two orderings are also unstated:
   - the order of group and single requests within one command;
   - when a Group request is stale (presumably when no member still waits on it).

   State the general rule: fixed system order, each system in SimIndex order, plus the group rules.
6. **The ADR-0012 refinement (diff:101-110) doesn't cover these reload cases:**
   - Map-pixel edits never reach a running match: `TerrainGrid` is Hashed, and a rebuild loads it from the save, while the rules hash still changes. The ADR should say so.
   - A save whose `TerrainGrid` palette names a terrain kind that no longer exists after an ID-set rebuild has undefined behaviour. It should be a load error, so the match keeps running.
   - A value reload that changes a `Footprint` width across a cell boundary leaves a Hashed `PathState::Flow { key.profile }` or a queued request naming a profile that `HpaGraphs` may no longer have. Either re-idle such units or require graph lookups to tolerate it.
7. **0016:51: "thinks once per new snapshot tick" contradicts the plan's test.** Plan:1201/1207 say one think per *frame* in which the tick advanced (`ai_thinks_once_per_new_tick`: three `TickOutput`s give one think). Reword the ADR.
8. **0014:121: the Confirmation over-claims.** `arch-check` doesn't check "`png` is a direct dependency of `fl_content` only"; `arch.rs` has no `png` rule. Either name a real check (a clippy `disallowed-types`/`disallowed-methods` on `png::` in the sim and client profiles, or an arch-check rule) or call it a review item.
9. **Formatting:**
   - Two added prose lines exceed 120 columns: 0012:89 at 188 and 0016:17 at 139. This contradicts the report's "≤ 120 columns" claim.
   - 0014:15-16 has a stray reflow break ("terrain type"⏎"and lists").

### Assessment

**Task quality:** Needs fixes

**Reasoning:** The ADRs are thorough, consistent with the plan's names, values and test names, and correct about the code they lean on. But ADR-0015's central determinism claim, that eviction may be any deterministic policy, doesn't hold while `FlowCache::direction` can see a tile that was evicted in the same tick. That invariant has to be in the binding contract before Task 10 implements it.
