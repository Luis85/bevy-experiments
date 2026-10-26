# Task 10b review: flow cache eviction at the tick boundary (R124), plus the ID-set rebuild re-plan

Base `bb29c46`, head `d27be13` (`f861480` + `d27be13`). Diff: `review-t10b.diff`, read in one pass, plus the `.new` files.

### Spec Compliance

- ✅ Spec compliant.
  - Behaviour 1, entry() never evicts: `crates/fl_sim/src/nav/flow.rs:232-257`. The `self.evict(key)` call is removed, so a miss only builds and inserts.
  - Behaviour 2, tick-start eviction skips entries read last tick: `flow.rs:218-229` calls `evict(tick − 1)`, and `flow.rs:280` (`len > max && last_used < previous`) only evicts entries last read before the previous tick. Entries read in the previous tick stay above the cap.
  - Behaviour 3, hard max: the rules value is `nav/mod.rs` `PathRules.flow_cache_hard_max`, and the link check `nav/mod.rs:82` requires `≥ flow_cache_max`. Base rules use 128 (4 × 32). It is enforced only in `evict` (`flow.rs:281`, `len <= hard_max`), and `evict`'s only caller is `begin_tick`. The memory bound is documented in `begin_tick`, the module docs and ADR-0015.
  - Behaviour 4, deterministic LRU order: `flow.rs:270-275` sorts `(Tick, FlowKey)`. `FlowKey` derives `Ord` (`path.rs:112`), and keys are unique, so `sort_unstable` is total. No hash map and no `Entity` are involved.
  - Behaviour 5, outcomes unchanged: covered by the 60-key overflow test (roomy, capped, overflowing and cold runs give equal hashes, charges and budget) and the 36-key test (hashes equal to a roomy run). I also checked the golden replay byte by byte (see Checks).
  - Behaviour 6, wander stagger: not done. It is reported with its reason, as the brief's "otherwise report and leave it" allows.
  - Tests: all four named tests exist (`tests/flow.rs:518,602,670,711`). RED evidence was captured for all four on code where eviction still ran inside `entry()`.
  - Carry (T12 Minor 2): `reload.rs:219` and `save/load.rs:184`, with RED/GREEN evidence.
  - Docs: R112 is corrected in ADR-0015, and "run at any point" is dropped.
- ⚠️ Cannot verify from the diff: `begin_tick` is not a dedicated system in the R46 hook slot. It is a lazy call that every flow reader makes first. I checked every caller (named risk: "a reader reads before tick-start eviction"): `path.rs:631` (before `cold_cost` and `direction`), `movement.rs:244` (`prepare_flow_tiles`) and `movement.rs:487` (`move_system`). `nav/mod.rs:206` `clear_profile` is invalidation, not a read. So today eviction never runs mid-tick. The controller should accept the lazy hook rather than a schedule change.

### Checks run

- **Golden replay `.new` is header-only. Verified byte-exact, not just trusted.** I took the committed `testbed_wander.replay` (6,533 B) and applied exactly four header edits:
  - build id `f4b34d9`→`bb29c46`;
  - the rules-hash varint `e9ad…8401`→`a7f5…940f`;
  - the `journal_records` count 8→9;
  - the string `\x13core.content_reload` inserted after `core.command_rejected`.

  The result equals `testbed_wander.replay.new` (6,552 B) exactly. The body (inputs, events, the 400 hashes) is unchanged.
- **The cache is never hashed or saved.** It is still `Class::Derived` (`nav/mod.rs:391`, unchanged). Eviction now leaves the same cache contents at the end of a tick whatever order the reads come in. Before, `move_system` iterated in query order and evicted between reads.
- **Every flow unit reads its field every tick** (`movement.rs:494-498`, every `PathState::Flow` unit in `move_system`). So "read in the previous tick" is exactly "live", and a live key is never evicted below `hard_max`.
- **Load path, rules hash after a value reload** (named risk: "save after a hot reload, then load under the same content, re-plans spuriously"). `reload.rs:135-137` writes the new `rules_hash` into the match header, so such a save loads with `content_changed == false`.
- **Perf numbers are internally consistent:**
  - `flow_thrash` totals: `abstract_nodes` 9,072 = `path.budget_spent` 9,072 = 36 × 252, so each field is built once and the charge equals the cold cost. `cells` 9,216 = 36 tiles × 256.
  - Per-tick maximum: 8,064 = 32 × 252 in the request tick, which gives budget 10,080 = ⌈8,064 × 1.25⌉, and cells 8,192 → 10,240.
  - A slow re-thrash under the per-tick budget would still change the exact snapshot totals, so it stays gated.
  - The bench (8.34 / 17.23 ms, machine not idle) uses wander-only single requests, and its counters are unchanged. The numbers are plausible, and G7 holds.
- **RemovedAssert ×3** (`tests/flow.rs` at the old lines 482, 483 and 493): the replacements are at 60 keys (`:713-718`, `:724`). The old "≤ 2" is inverted by design (R124).

### Strengths

- The `evict` loop (`flow.rs:276-285`) is small and correct. Sorted by last read, it stops at the first entry it may keep, and it allocates only when the cache is over the cap.
- `flow_cache_live_keys_never_thrash` is strong:
  - It compares per-tick `flowfield.cells` against a roomy run, so it separates first-time tiles from rebuilds instead of asserting a bare 0.
  - It proves the test is not vacuous (more than 32 keys live on ≥ 100 ticks).
  - It also checks that the hashes are equal.
- The direct-API tests pin every branch:
  - the cap keeps last tick's reads;
  - stale entries go;
  - an evicted key comes back with the same cold cost;
  - LRU ties break by key order (`:650-660`);
  - the hard max cuts the cache to exactly 16 from the second tick on;
  - a tick without reads lets the cache drop to the cap.
- The load re-plan reuses the value reload's helper instead of a second copy. It has a guard test for unchanged content (`tests/reload.rs:646`), a twin-determinism check, and a journal assertion.
- The ADR text now states the real cost model and the memory bound.

### Issues

#### Critical (Must Fix)
None.

#### Important (Should Fix)
None.

#### Minor (Nice to Have)
1. **The tick-start hook is a convention, not a structure (`flow.rs:205-217`).** A future flow reader that skips `begin_tick` reads with a stale `now`. Its entries then get an old `last_used` and can be evicted at the next start although they were read this tick. The outcome is still neutral, but the thrash comes back silently. Either add a `debug_assert` in readers (pass the tick to `direction`/`cold_cost`), or move `begin_tick` into one core Housekeeping system later (it would change the system-order snapshot). Documented as is, so this is acceptable.
2. **`evict` allocates and sorts every tick while the live set is above the cap (`flow.rs:270-275`).** In the 36-key steady state that is about 36 × 16 B per tick. It is deterministic and counted in `alloc.*`. A scratch `Vec` kept in the `Derived` cache would remove it, matching Task 11's "allocates nothing per tick" for flow prep.
3. **`max == 0` turns off `hard_max` too (`flow.rs:267`).** The early return happens before the hard-max check. Rules can't produce this (the link pass requires `max ≥ 1`), but `FlowCache::new(0, h, _)` silently ignores `h`. Document it on `new`, or check the hard max first.
4. **`begin_tick` is now `pub` API (`fl_sim.txt.new`) only so tests can drive ticks.** A client or debug caller could evict out of band. That is harmless, being outcome-neutral, but consider `#[doc(hidden)]` before the API freeze.
5. **`rules_edited` (`tests/flow.rs:440`) duplicates `rules_with` (`:75`).** Make `rules_with(from, to)` call `rules_edited(&[(from, to)])`.
6. **The rewritten overflow test lost the old scenario's ponds.** Outcome neutrality under overflow is now shown on open grass only. Obstacle structure is still covered cold vs warm by `cache_does_not_change_outcomes`.
7. **The load re-plan covers only `Flow` units (`reload.rs:219-226`).** After a content-changed load, a `Following` or `Waiting` unit whose footprint changed keeps its corners or queued request. This is outside T12 Minor 2's scope. Movement checks passability, and queued requests use the current profile, so it heals. A one-line note in the `load_world_with` docs would close it.
8. **`rebuild_replans_flow_units` (`tests/reload.rs:559`) asserts the tanks drive on, but not that they regroup onto one shared field.** One assertion after a few ticks (all of #3–#8 in `Flow` with the same key) would show that the re-request rebuilds the group move, not six single paths.
9. **`docs/perf/budgets.ron.new` is CRLF (the recorder, pre-existing since Task 13).** Normalise on bless.

### Assessment

**Task quality:** Approved

**Reasoning:** Eviction now runs only at the tick boundary, as the brief decided: it is deterministic, enforced only at a tick start, and outcome-neutral. The golden replay changes only in its header (byte-verified), the four new tests failed on code that still evicted inside `entry()`, and the perf numbers add up. The remaining items are hardening and tidiness.

### Recommended rulings

1. **`Path.flow_cache_hard_max` as a rules value: accept.**
   - The brief decided it, `flow_cache_max` is already a rules value, and it needs the same range check.
   - The only cost is a one-time header-only golden re-record, verified byte-exact.
   - Do not switch to a code constant. That would hide a tuning value from content and give the same result.
   - Bless reason: say "header-only (rules hash, build id, T12 journal string); hash stream identical".
2. **Out-of-list edits: accept (as R109 and Task 13 F).**
   - `assets/base/rules.ron` and the five fixture `rules.ron` are forced by strict `PathRules` (no serde default). `bad_mod` stays bad for its own reason.
   - `build.rs` is mechanical.
   - `reload.rs`, `save/load.rs` and `tests/{reload,terrain}.rs` are the ledgered T12 Minor 2 carry plus the link test.
   - `save/load.rs` is a contract path: the gate ran with `contract`, which is fine.
3. **Load-time re-plan on every load: correct and deterministic. Accept.**
   - It depends only on `save.header.rules_hash` versus the content's rules hash and iterates in `SimIndex` order, so peers loading under the same content behave identically.
   - Under the save's own content nothing changes: a flow key's profile equals its members' profile, because groups split per profile. This holds also after a value reload, which rewrites the header hash (`reload.rs:137`). The guard test and the unchanged save, branch and M0 tests confirm it.
   - Re-planning everyone under changed content is the right conservative choice, because a save does not record `cluster_cells` or the profiles. Lockstep and late-join loads always share content, so they never hit it.
   - Take Minor 8 (regroup assertion) with the next lane-1 fix touch.
4. **Wander stagger not added: accept, park.**
   - It changes the wander orders, so the testbed hash stream and the golden replay would change, which behaviour 5 forbids.
   - R126 already shows the burst drains (by tick 52) and stays within the per-tick path budget.
   - If wanted, schedule it as its own gameplay change with a deliberate golden re-record (C-list), not as part of a cache fix.
5. **Three test-guard RemovedAsserts: accept all three.**
   - `keys.len() == 4` → `live_keys == 60`.
   - `cache.len() <= 2` → `cache.len() == 60`, inverted on purpose by R124.
   - `flow_charges.len() == 4` → `== 60`.

   The replacements are at least as strong.

### Bless per `.new`

- `crates/fl_test_utils/tests/replays/testbed_wander.replay.new`: **bless**. Header-only, byte-verified: build id, rules hash, journal string count, `core.content_reload`.
- `crates/fl_test_utils/tests/snapshots/perf_counters__flow_thrash.snap.new`: **bless**. Only `flowfield.abstract_nodes` 4,571,532→9,072 and `flowfield.cells` 4,644,096→9,216 change.
- `docs/perf/budgets.ron.new`: **bless**. It drops the temporary-ceiling note and sets `flow_thrash` `abstract_nodes` 10,080, `cells` 10,240, `alloc.bytes` 601,132. Nothing else changes. Normalise the CRLF line endings.
- `xtask/snapshots/api/fl_sim.txt.new`: **bless**. It adds `begin_tick` and `PathRules::flow_cache_hard_max`, and `FlowCache::new` gains an argument. If Minor 4 (`#[doc(hidden)]`) is taken first, regenerate it before blessing.
