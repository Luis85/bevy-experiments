# Task 13 review: scale mix on the 2 km terrain with pathing; M1 counter budgets; bench --flat

Base 53ddd6e, head 70a9b3b (worktree C:\fl\t13), diff `review-t13.diff` read once in full (3 passes: fl_cli/fl_game, fl_sim, fl_test_utils + `.new`). Diff-line refs are `diff L<n>`; source refs are worktree paths.

### Spec Compliance

- ✅ Spec compliant against the brief, file by file: `scale.rs` (`map`, `SCALE_MAP`, `on_terrain`, size check before any world change, 16 redraws from the same `placement` Rolls under the existing scoped `#[expect]`, then `nearest_passable`: diff L918-934, L983-1020, `crates/fl_sim/src/scale.rs:397`), `replay.rs` (v3, diff L707-709), `tests/replay.rs` (literal only), `scale_mix.rs` (all 5 brief tests, diff L1192-1320), `perf.rs` + `perf_counters.rs` (3 scale tests on terrain + `path_budget_holds_at_scale`, diff L2018-2084), `bench.rs`/`tests/bench.rs` (`--flat`, `map` column, `#[serde(default)]` so M0.json parses as open, diff L105-108, L462-487), `builder.rs`/`tests/builder.rs` (stress on terrain through `TerrainView::from_content`, one code path, diff L567-595). No `flowfield.*` budget in `scale_*` (brief "Budgets" rule held).
- ❌ Carried R117 only partly met: `choke_crowd` never measures `move.blocked` (0 in all 200 ticks), so it is budgeted only in `scale_3000`/`scale_5000` (2 and 3). Justified in the report; needs a ruling (D below). The test doc says the opposite: `crates/fl_test_utils/tests/perf_counters.rs:117` "`move.blocked` and `steer.pairs` budgeted".
- Extra (out of list, all required by the brief's interface or mechanical): `scenario.rs` `MapSizeMismatch` (diff L1111-1113), `fl_sim/src/lib.rs` re-export, `fl_test_utils/src/lib.rs` re-exports, `fl_cli/src/main.rs --flat` (diff L338-341). Accept (R108/R118 precedent).
- Carries present with numbers: R98/R112 profile (`process_path_queue` 7th, 0.065 ms at 3000), Task 10 group-move + >32-key scenarios, R86 (every `CounterSim` builds a fresh sim: `from_scale`/`ascii`, diff L1615-1660), R100 (not budgeted, terrain never edited), G7 (≤ 8.1 ms worst at 3000).
- ⚠️ Cannot verify from diff: (1) whether the `path.queue_len` 1031 burst at 5000 is a synchronised re-request (R116 stranded re-plan every 10 ticks, or wander timers) — lane 1 should name its source; (2) wall-clock numbers (taken under load, not reproducible here).

Checks run:
- Golden: byte-compared `testbed_wander.replay` vs `.new`: both 6533 bytes, 8 bytes differ = byte 0 (2→3) + 7 chars of the build id (`4a92672`→`70a9b3b`). Header-only, as the report says. Only one committed replay exists (`Glob **/*.replay`), and it is a `Scenario` start.
- Budget arithmetic: every budget I spot-checked is `ceil(max × 1.25)` of the report's maxima (scale_3000 `path.budget_spent` 2721→3402, `path.nodes_expanded` 1410→1763, `alloc.bytes` 154,952→193,690; scale_5000 `path.queue_len` 1031→1289, `path.requests` 108→135, `path.budget_spent` 8876→11095; flow_thrash `flowfield.abstract_nodes` 22,932→28,665, `flowfield.cells` 23,296→29,120; group_move 8444→10555; choke `steer.pairs` 12,190→15,238). The report's numbers match the `.new` files.
- Bench vs M0.json: flat quiet 0.525/4.929/11.688 vs 0.537/4.431/9.335 = −2.2 / +11.2 / +25.2 %; repeat-5 0.678/6.658/13.862 = +26 / +50 / +48.5 %. Matches the report.
- Named risk "eviction inside `entry()`" (A): read `crates/fl_sim/src/nav/flow.rs:206-254`. `entry()` builds a missing key, then `evict(key)` drops the smallest `(last_used, key)` while `len > max`. `last_used` is a tick, so mid-tick the entries not yet read this tick (`last_used = now − 1`) are the oldest, and those are the next ones the cyclic per-unit read order needs. That is the classic LRU-on-cyclic-access failure: every read past the 32nd key misses, and each eviction drops the integration and every tile. Confirmed.
- Named risk "per-request buffers" (B): `crates/fl_sim/src/nav/hpa.rs:649-654` (`g`/`parent`/`closed` over all nodes + `BinaryHeap` per request) and `crates/fl_sim/src/nav/astar.rs:216-221`. These are the R98 buffers, as claimed.
- Determinism: all scenarios seeded and fixed (`on_terrain` seed 1, ASCII sims seed 0, commands in `BTreeMap<Tick,_>` with per-player seq, diff L1689-1720). Placement on the open map never redraws, so the flat stream is M0's (`place` returns the first draw when it is passable, `crates/fl_sim/src/scale.rs:397`). `scale_mix_twin_run` compares 200 hashes.
- test-guard: both RemovedAssert rewrites keep or strengthen the assertion (builder.rs diff L627→628-635: none → `Some((500,500))`; scale_mix.rs diff L1202→1203: per-tick hash count). Accept.

### Strengths

- Terrain placement is correct and cheap: draws first, borrowing `NavGrid`, then spawning. Redraws come from the same stream and no new `Stream` is added. Errors are checked before the world changes, and a test pins that (`scale_map_size_must_match` asserts no `MapInfo`).
- `scale_units_start_on_passable_cells` asserts the map has water (non-vacuous). `scale_pathing_reaches_targets` checks every tick, not just the end.
- `path_budget_holds_at_scale` checks both the R98 overshoot rule and that `path.budget_spent` equals the sum of the journaled charges, and it asserts that planning happened (> 50 of 100 ticks).
- The implementer made the thrash scenario actually thrash (member-major spawn → cyclic access) and ran a 32-key control. Without that, A would have stayed hidden.
- Bench: the old rows default to `"open"`, and a map mismatch against the baseline warns, mirroring the build-type warning.

### Issues

#### Critical (Must Fix)
None in the diff.

#### Important (Should Fix)
1. **Flow-cache thrash (pre-existing, Task 10 code, surfaced here; blocks M1 close).** `crates/fl_sim/src/nav/flow.rs:228,238-254`: with more live keys than `flow_cache_max`, every read misses (36 keys: ~91 integrations + ~91 tiles per tick for 216 units; 60 keys on 2 km: ~350 ms/tick, 16 MB/tick). Under R112 read-miss rebuilds are uncharged, so the ADR-0015 path budget does not bound this. R112's wording "thrash bounded by flow_cache_max" is false: the cost grows with reads per tick. In a 4-player match, AI plus R88 splitting and per-profile groups can reach 32 live destinations. At 350 ms/tick a 50 ms lockstep tick stalls 7×. Fix: see A.
2. **The M0-comparable flat workload is no longer counter-gated (plan-mandated).** `docs/perf/budgets.ron.new` replaces the flat `scale_*` budgets with terrain ones, and `scale_3000 alloc.bytes` goes 19,030 → 193,690. A future non-path alloc regression of up to ~170 KB/tick would now pass. That undoes R102's guarantee ("don't bless raised alloc budgets" on an unchanged workload). Fix: keep one flat scenario (`flat_3000` = `ScaleScenario::with_units(3000)`) in `perf_counters.rs` and re-record it (it should land at today's M0-era numbers).
3. **`flow_thrash` budget/snapshot encode defect 1 as the accepted level.** `budgets.ron.new` `flow_thrash.flowfield.abstract_nodes: 28665`, `.cells: 29120`. If blessed as-is it is only a ceiling, not a check. The A fix must re-record it, and it should drop to roughly 0 per tick in steady state.

#### Minor (Nice to Have)
1. Stale test docs: `crates/fl_test_utils/tests/perf_counters.rs:110` says "60 live flow fields" (it is 36, `THRASH_GROUPS`); `:117` says `move.blocked` is budgeted in choke_crowd (it is not).
2. Very small budgets are fragile: `move.blocked` 2/3, `path.requests` 3 (choke), `path.queue_len` 5/13. `ceil(1.25 × small)` leaves +1. That is deterministic, but any movement or pathing change will need a re-record. Accept.
3. `path.queue_len` at 5000 is a burst, not a backlog: total 6705 / 200 ticks ≈ 33.5 mean vs max 1031 (a ~10-tick drain at ~100 requests/tick). The report says "backs up". No test asserts that the queue drains.
4. `perf.rs` grew by ~370 net lines and now mixes scenario construction (`CounterSim`, ASCII maps, `Orders`) with budget plumbing. Split `CounterSim` into `perf/scenarios.rs` when next touched.
5. `scale_mix.rs` `units_on_blocked_cells` (diff L1218-1235) repeats the profile/cell lookup that `perf.rs` `units_by` also does. Test-only.
6. `bench --baseline` still compares across maps after the warning (`crates/fl_cli/src/bench.rs:570`). That is acceptable for a diagnostic; consider refusing unless `--flat` is set.
7. Gate took 444 s (budget 300 s); the three new scenarios add to `test-headless`. This adds to the existing gate-time watch item (ledger line 76).

### Rulings / fixes A–F

- **A, flow-cache thrash: blocks M1, but not Task 13.** Lane 1, `crates/fl_sim/src/nav/flow.rs` (`entry`/`evict`/`begin_tick`) plus a test next to Task 10's R84 cold/warm test.
  - Rule: `entry()` never evicts. `begin_tick(t)` evicts by LRU `(last_used, key)` down to `flow_cache_max`, but only entries with `last_used < t − 1`. Entries read in the previous tick are the live set: kept even above the cap (pinning without refcounts; deterministic; outcome-neutral, so no hash or golden change).
  - Optional hard ceiling `Path.flow_cache_hard_max` (e.g. 4 × max) so memory stays bounded; LRU applies over the live set only past it.
  - Test: 36 keys, after the request tick `flowfield.abstract_nodes` = 0 per tick (the implementer's 32-key control already shows this shape).
  - Then re-record `flow_thrash` (budget + snapshot) and amend the R112 cost wording.
- **B, terrain allocs 8–19× M0: not blocking; R98 stays parked** (trigger not met: 500 cells per side < 1024; `process_path_queue` 7th, ~1.4 %).
  - Ruling: the raised `alloc.*` budgets are accepted for the terrain scenarios (new workload, not a regression of an old one), on condition that Important 2 lands (flat scenario gated at M0-level allocs).
  - Cheap follow-up for lane 1 when it next touches `nav/hpa.rs` / `nav/astar.rs`: reuse `g`/`parent`/`closed`/heap via a `Transient` `PathScratch` with generation stamps (R102 pattern), then re-record the terrain `alloc.*` budgets lower.
- **C, 5000-unit queue: not blocking.** It drains by design (FIFO, budgeted, ADR-0015), and 5000 is not the G7 size. Lane 1 should name the burst source. If it is synchronised (R116's 10-tick re-plan or aligned wander timers), stagger by `SimId % period` (deterministic). Mention it in the M1 report.
- **D, `move.blocked` in choke: amend R117.** `choke_crowd` budgets `steer.pairs` plus the path/flow counters; `move.blocked` is gated where it occurs (`scale_3000`/`scale_5000`). Do not emit zero-valued counters now: that would put `move.blocked: 0` budgets in every scenario. Carry a jamming choke variant to C7 crowd tuning (opposing crowds head-on through the gap, or the jam window after ~tick 470). Fix the stale docs (Minor 1) in the T13 fix round.
- **E, bench vs M0.json: the justification is adequate for the quiet run, but an idle rerun is needed before M1.json.** The quiet deltas are explained by new M1 per-tick systems: +0.49 ms at 3000 vs `compute_steering` 1.07 ms; +2.35 ms at 5000 vs 3.13 ms. The loaded +26–50 % is noise, not evidence. M0.json itself was recorded under load, so it is a weak baseline.
  - Before M1.json (ADR-0010 tier 3): idle machine, fixed power plan, the `m0`-tag binary and the M1 `--flat` binary interleaved, median of 5.
  - Write the 5000-unit +>20 % justification with a per-system attribution into the M1.json note.
  - Not a Task 13 blocker.
- **F, scope/contract: accept.** `replay.rs` changes because `ReplayStart::Scale(ScaleScenario)` embeds the struct. The new `map: Option<String>` changes the non-self-describing layout, so a v2 Scale replay would misdecode; the bump follows R62.
  - No migration is needed: R70 keeps `check_replay` strict, `from_bytes` reads only the current version, and replays are build-bound.
  - The only committed replay is a `Scenario` start, re-recorded header-only (verified).
  - The brief's Step 7 command omitted `--track contract` (plan defect). Gate with `--track sim-core --track tools --track perf --track client --track contract` (OK per report). The out-of-list edits are accepted.

### Bless per `.new`

| artifact | recommendation |
|---|---|
| `crates/fl_sim/tests/snapshots/formats__save_codec_formats.snap.new` | bless now |
| `xtask/snapshots/api/fl_sim.txt.new` | bless now |
| `crates/fl_test_utils/tests/replays/testbed_wander.replay.new` | bless now (header-only, verified) |
| `perf_counters__scale_500/3000/5000.snap.new` | bless now (totals; A/B fixes do not touch them) |
| `perf_counters__group_move.snap.new`, `perf_counters__choke_crowd.snap.new` | bless now (≤ 12 live keys, 1 key: unaffected by A) |
| `perf_counters__flow_thrash.snap.new` | bless with the T13 fix round as the documented pre-fix baseline; the A fix re-records it |
| `docs/perf/budgets.ron.new` | re-record after fixes: T13 fix round adds `flat_3000` (Important 2); `flow_thrash` enters at today's level as a temporary ceiling and the A fix re-records it |

### Assessment

**Task quality:** Needs fixes

**Reasoning:** The scenario, bench and budget work is correct, deterministic and matches the report's numbers. However, swapping in terrain budgets drops the flat (M0) alloc gate, and the docs misstate what choke_crowd and flow_thrash gate. The flow-cache thrash it found is a real pre-existing defect that must be fixed in lane 1 before M1 closes.
