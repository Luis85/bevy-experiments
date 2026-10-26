### Task 10b: Flow cache eviction at the tick boundary (R124)

**Track:** sim-core + perf + docs (ADR-0015 text) · **Refs:** ADR-0015, R84, R86, R112 (corrected), R124, R126 · **Lane 1** (after Task 12; added by the controller after the Task 13 review)

**Why:** Task 13's `flow_thrash` scenario (36 live flow keys, `flow_cache_max` 32) rebuilds ~91 flow integrations and tiles every tick; a 60-key draft on the 2 km map cost ~350 ms/tick. Cause: `FlowCache::entry()` (crates/fl_sim/src/nav/flow.rs ~228, 238–254) evicts least-recently-used entries while the tick is still reading, so it evicts exactly the keys needed next. Those read-miss rebuilds are uncharged (R86), so the path budget does not bound them. R112's claim that thrash is bounded by `flow_cache_max` was wrong.

**Files:**
- Modify: `crates/fl_sim/src/nav/flow.rs` (eviction), `crates/fl_sim/src/path.rs` or the system that owns tick-start housekeeping for the flow cache (whichever exists; keep it in the core feature's hook slot, R46), `crates/fl_sim/tests/flow.rs` (tests), `docs/adr/0015-path-requests-derived-caches-debug-queries.md` (correct the R84/R112 cost wording).
- Regenerate (as `.new`, never bless): `crates/fl_test_utils/tests/snapshots/perf_counters__flow_thrash.snap.new`, `docs/perf/budgets.ron.new` (flow_thrash budget replaced by the real post-fix level; remove the "temporary ceiling" note), plus any other perf snapshot/system-order/API `.new` the change produces.

**Behaviour (decided):**
1. `FlowCache::entry()` never evicts. Reads during a tick only insert (a miss still recomputes, R84).
2. At the start of each tick (before any flow reads), evict least-recently-used entries down to `flow_cache_max`, considering only entries **not read in the previous tick**. Entries read last tick stay, even when that leaves the cache above the cap.
3. Hard ceiling: `flow_cache_hard_max = 4 × flow_cache_max` (rules value, range-checked in the link pass, ≥ flow_cache_max). If the cache would exceed it, evict LRU entries not read *this* tick at the next tick start; never evict mid-tick. Document the memory bound.
4. LRU order is deterministic: last-read tick, then key order (`FlowKey` Ord). No HashMap, no Entity order.
5. Outcome-neutral (R84): unit positions/hashes are identical cold vs warm vs post-load vs overflow, before and after this change — the golden replay must not change (if it does, stop and report: that would mean eviction changed outcomes).
6. If the 5000-unit tick-0 burst (R126: all idle units get wander orders at tick 0, 4,881 requests, drains by tick 52) is driven by a synchronised wander start, stagger the wander start by `SimId` (e.g. first wander tick = `sim_id % wander_period`) — only if it changes no behaviour test semantics; otherwise report and leave it.

**Tests (each must fail before the fix, capture RED):**
- `flow_cache_live_keys_never_thrash`: 36 live keys, cap 32 → `flowfield.abstract_nodes` and tile rebuilds are 0 every tick after the request tick (over ≥100 ticks).
- `flow_cache_evicts_only_at_tick_start`: a key read this tick is never evicted before the next tick start, even when the cache is over the cap.
- `flow_cache_hard_max_bounds_memory`: with 200 distinct one-off keys, the cache size never exceeds `flow_cache_hard_max` at any tick start.
- `flow_cache_overflow_does_not_change_outcomes` (existing) still passes; extend it to 60 keys.
- Re-run `perf_counters` scenarios; the `flow_thrash` snapshot should drop to near-zero rebuilds after the first tick.

**Verification:** `cargo nextest run -p fl_sim -p fl_test_utils -p fl_catalog -p fl_cli` in full; `cargo xtask gate task --track contract,sim-core,perf,docs`. Report the `flow_thrash` per-tick `flowfield.abstract_nodes` max before/after and a release `fl_cli bench` at 3000/5000 units.

**Commit:** `fix(fl_sim): flow cache evicts only at the tick boundary (R124)` with `Refs: ADR-0015` and `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
