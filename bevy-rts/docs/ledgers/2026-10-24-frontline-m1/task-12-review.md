# Task 12 review — hot reload, sim side (53ddd6e..4e9d225)

Reviewed in one pass over `review-t12.diff` (2626 lines, incl. the three API `.new` diffs). Focused checks outside the diff are each named below with the risk they answer.

### Spec Compliance

- ✅ Spec compliant. Every file in the brief's Files list has its hunk (fl_content db/def/tests, fl_view runner/lib/tests, fl_sim units/lib/runner mod+threaded/save mod+load, new reload.rs and tests/reload.rs, fl_client tests debug/view). All 13 named fl_sim tests, the 3 fl_content tests and `opaque_content_eq_is_identity` are there. Two tests are extra and useful: `reload_refused_for_another_feature_set` and `reload_changing_profile_discards_queued_request`. The report says "the brief's 14" fl_sim tests, but the brief names 13; that is a counting slip in the report, not a missing test.
  - Brief order is followed exactly: cheats → registry → ID sets (no change on refusal) → profiles captured → Content + header rules hash replaced → def reload in SimIndex then component-name order → `rebuild_derived` → profile re-plan → journal (`crates/fl_sim/src/reload.rs:104-155`).
  - `HullDef::reload`: the ratio is kept in u64 integer math (`units.rs:84-93`), so there is no float and no overflow, and 0 stays 0. `MobilityDef::reload` replaces `Mobility` only (`units.rs:218`).
  - `ToRunner` is `#[non_exhaustive]` and gains `Control`, as R80 requires. `RunnerControl` is non-exhaustive. `OpaqueContent`'s Debug and PartialEq match the brief. `SimReply::Reloaded` is appended last, so existing postcard indices are kept.
  - `load_world_with`/`LoadOptions`/`Loaded.dropped`: the drop happens before any spawn, so no SimEntity is created for a dropped unit (`save/load.rs:149,188-221`). `load_world` delegates with the default options.
  - Terrain palette error is `"unknown terrain '<id>'"` under `core.terrain` (`terrain.rs:226`).
  - TerrainGrid is never written.
- ⚠️ Cannot verify from the diff (cross-task, for the controller):
  - **"Never in a lockstep match" (ADR-0012).** The sim gates only on `cheats_allowed`, and a lockstep match can have cheats on: `check_handshake` only requires the flag to be equal on both peers (`fl_protocol/src/header.rs:116`). A `Control` sent to one peer's runner would desync it. Carry to Task 19: the client must refuse a reload while a `LockstepDriver`/`NetSession` drives, with a test.
  - **Replay event.** `ReplayEvent::ContentReload { tick, rules_hash }` already exists (`replay.rs:44-48`). Task 19 should build it from `SimReply::Reloaded { tick, rules_hash }`, not from the `core.content_reload` journal record, because the journal is only written when `journal.enabled()` (`reload.rs:140`). `replay-verify` reporting "non-reproducible" is also Task 19.

### Checks run (risk → what I checked)

- **Tick boundary.** `SimClock.tick` names the next tick between ticks (`clock.rs` doc). `step_tick` drains the Journal with `take_sorted` at the end of that tick, so the record lands in tick T's output. `reload_journals_content_reload` asserts tick 4 after reloading at 4. The threaded `serve` loop handles messages one at a time, so a Control always runs between two Ticks (`threaded.rs:48-68`).
- **Deterministic iteration order.** `ResolvedObject.components` is a `BTreeMap` (`fl_content/src/object.rs:27`). The units come from SimIndex iteration. Replans run in SimIndex order (`reload.rs:134-136`). No HashMap, f32/f64, Instant or RNG in any added line (grep of the diff's `+` lines).
- **Dense-id safety of `same_id_sets`.** It compares the ordered tables; units are stored sorted (`db.rs` `unit_id` uses binary search), so "same set" means "same dense ids", and `id_set_diff` is empty exactly when the sets match.
- **Rule values baked in at build time and missed by a reload.** Only `build.rs:172` reads rules at build, for `HashInterval`. That resource is `Class::External` (`feature.rs:173`), so it is not hashed. All other rules are read live or rebuilt: spatial and nav go through `rebuild_core`, and `install_nav` builds the HPA graphs and the flow cache (`nav/mod.rs:104-128`).
- **Rebuild reads map layers from the new content (ADR case 1).** `rebuild_nav` builds from the Hashed `TerrainGrid` and `MapInfo`, plus the content's terrain kinds and rules (`nav/mod.rs:85-96`). No map pixel reaches the match. `value_reload_keeps_terrain_grid` pins this.
- **Stale profile in a queued request (ADR case 3).** `plan_for` plans with the unit's current profile (`path.rs:536`), not the one stored in the request, and a request whose unit is not `Waiting` on it is stale at charge 0 (`path.rs:509-533`). So a re-planned unit's old request cannot plan with an old profile. Pinned by `reload_changing_profile_discards_queued_request`.
- **Hashed FlowKeys.** The only Hashed FlowKey is in `PathState::Flow { key }`. `PathQueue` group requests hold no key (`path.rs:113-180`). Flow units whose profile changed are re-planned; the cache is Derived and emptied by `install_nav`.
- **Dropped SimIds referenced elsewhere.** Path requests for a missing unit resolve as stale (`path.rs:7`, `:509-533`), and a Follow whose leader is gone completes (orders `Reason::Completed`). Dropped ids are below `sim_id_next`, so the allocator never reissues them.
- **Lenient load used by lockstep or replay.** The only callers of `load_world_with` and `drop_unknown_units` are `load_world` (with the strict default) and the tests (`git grep` at 4e9d225). `replay_app` and `verify_replay` don't use it.
- **Rules hash change detected.** Header `rules_hash` is replaced, and `reload_journals_content_reload` asserts that the header names the new hash. `SimQuery::Header` reads `MatchHeaderRes`.

### Strengths

- The atomicity claim holds: all three refusal checks come before the first mutation, and each refusal test asserts the state hash (and the `Content` Arc) are unchanged.
- The re-plan pass compares each unit's profile before and after the reload, and runs after `rebuild_derived`. It handles Waiting, Following and Flow. The tests use a group of 6 on a flow field, a truck whose profile doesn't change and which keeps `Following` (the negative control), a budget-starved queue, and HpaGraphs compared with a fresh build. The mutation check reported for the re-plan condition is credible.
- `rescale_hp` is integer-exact, has guards for a zero old maximum and for overflow, and has a unit test.
- `rebuild_drops_vanished_unit_types` checks that two independent loads run on with equal hash streams, that survivors keep their SimIds and poses, and that the dropped ids stay unused.
- Per-process, per-call scratch dirs fix the cross-process nextest collision properly.

### Issues

#### Critical (Must Fix)
None.

#### Important (Should Fix)
None.

#### Minor (Nice to Have)

1. **Flow units after a `Path.cluster_cells` reload (`reload.rs:211`).** Only a profile change triggers a re-plan. A value reload of `Path.cluster_cells` changes the cluster layout, so a unit whose profile is unchanged keeps a Hashed `FlowKey.cluster` index that now names another region. It heals: `Arrived` without line of sight, or `None`, goes back to Idle and re-requests, and `Arrived` with a clear line drives straight to its own goal (`movement.rs:506-515`). The route is suboptimal for a while, but the state stays deterministic. Fix: when `old.rules::<PathRules>().cluster_cells != new`, treat every Flow unit as changed. Add a test.
2. **The ID-set rebuild path does not re-plan.** After `load_world_with(drop)` under content where a footprint also changed, a loaded `PathState::Flow` can name a profile the rebuilt graphs lack. It heals on the next tick (`hpa.graph(profile)` is `None` → `to_idle` → re-request with the current profile). Strictly, though, the ADR line "no Hashed path state names a profile the rebuilt graphs lack" holds only on the value path. Carry to Task 19: reuse the re-plan helper after a rebuild load (no old profile is known, so re-plan Flow units whose key profile ≠ the current profile).
3. **`rescale_hp` can exceed the new maximum (`units.rs:91`).** If hp > old_max (a debug edit), the result can be above the new max. Clamp it to `new_max`. Repeated reloads also drift downward because of floor rounding (99/100 → /50 → /100 gives 98). That follows the brief's formula, so document it rather than change it.
4. **`reload_is_deterministic` (`tests/reload.rs:395`) shares one `Arc<ContentDb>` between both runs.** Load the edited content twice, independently, so the test actually models "identical across peers" (each peer loads its own files).
5. **`reload_refused_without_cheats` (`tests/reload.rs:281`) does not assert that the header `rules_hash` is unchanged.** The state hash may not cover the header. Add that assertion, also to the IdSetsDiffer and RegistryMismatch refusals.
6. **`ReloadError` is not `#[non_exhaustive]`.** Other pub contract errors such as `SaveError` are, and refusals will grow (for example a lockstep refusal). Adding the attribute is free now and breaking after the freeze. If it is adopted, regenerate `fl_sim.txt.new` before bless.
7. **`UNIT_TYPE` coupling (`save/load.rs:182`).** See ruling 4: add a guard test.
8. **`drop_unknown_units` is not gated on the save's `cheats_allowed`.** It is opt-in and has no non-test caller today, but ADR-0012 makes the rebuild a testbed path. Either refuse it for a save without cheats or say so in the `LoadOptions` docs, so a future late-join (M8) path cannot pick it up by accident.
9. **Gate time was 524 s against a 300 s budget, and the implementer ran nextest builds while the gate was running.** Both are known (see the ledger's "Watch"). Future dispatches should say not to build during a gate.

### Recommended rulings

1. **Out-of-list edits: accept all four** (precedent R108, R118).
   - `fl_client/tests/terrain.rs` is a third exhaustive fake runner the brief missed, and it fails to compile without the arm.
   - `feature.rs`'s one `reload::build` line registers the journal kind, the same pattern as `movement::build`.
   - The `terrain.rs` message is the text the brief mandates.
   - The `orders.rs` change is docs only.
2. **`HashInterval` not refreshed: accept.** It is `Class::External` and not hashed, so it has no determinism effect, and a `RunConfig` override can't be told apart from the rule at reload time. Document in the `apply_content_values` docs that `StateHash.interval_ticks` applies from the next match start or load, like map layers. No code change.
3. **A def component removed from a unit def stays on live units: accept for M1.** Only Selectable, Hull and Mobility exist, and the save/load rebuild also keeps the saved components, so both paths agree and stay deterministic. Add a doc note now. Add a `DefComponent` removal hook (or a re-spawn) when the first stateful feature def lands (M2 weapons/ammo).
4. **The lenient load decodes `core.unit_type` v1 directly: accept,** with a guard test in this fix round. The test asserts that the registered codec version of `"core.unit_type"` equals `UNIT_TYPE.1`, so a v2 bump fails loudly in CI, not only through the strict-path error.
5. **The threaded runner skips unknown `ToRunner` kinds: accept.** It is unreachable today: fl_view and fl_sim build together, every current variant has an arm, and the wildcard exists only because `#[non_exhaustive]` requires it. So no client can hang on it now. Park this: the next commit that adds a `ToRunner` variant also adds `ToRunner::reply_id(&self) -> Option<u64>` in fl_view (an exhaustive match in the defining crate), so `serve` answers `SimReply::Error("unsupported runner message")` to an unknown kind that carries an id. Task 19 should check that client waits on replies are bounded.

### Bless (per `.new`)

- `xtask/snapshots/api/fl_content.txt.new`: **bless**. It is additive: `same_id_sets`, `id_set_diff`, and `DefComponent::reload` with a default body.
- `xtask/snapshots/api/fl_view.txt.new`: **bless**. `ToRunner` becomes `#[non_exhaustive]` (R80, ADR-0012 refinement). The additions are `Control`, `RunnerControl`, `OpaqueContent` and `SimReply::Reloaded`, appended last.
- `xtask/snapshots/api/fl_sim.txt.new`: **bless**. It is additive. If Minor 6 (`ReloadError` `#[non_exhaustive]`) is taken, regenerate and bless the new file instead.
- Bless reason: `M1 Task 12: hot reload runner control, lenient load`.

### Assessment

**Task quality:** Approved

**Reasoning:** The value-reload path is deterministic. It runs between ticks in SimIndex and BTreeMap order with integer HP math, refuses atomically, rebuilds Derived nav through `install_nav`, and re-plans exactly the units whose profile changed. All three ADR-0012 refinement cases have tests. What remains is minor hardening, plus Task 19 carries for the lockstep refusal, the replay event and re-planning after a rebuild.
