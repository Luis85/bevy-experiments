# Task 2 review: contract additions (payloads, debug queries, PlayerView, AI seam)

Range 7eeb217..c855f94 (one commit), plus the four untracked `.new` artifacts. Reviewed in one pass from `review-t2.diff`; focused checks outside the diff are listed under "Checks run".

### Spec Compliance

- ✅ Spec compliant. Every file in the brief's Files list has its hunk (the diff stat covers all 17 files). Every interface in the brief exists with the specified signature, and all 12 brief tests exist with the specified assertions.
  - The payloads match ADR-0007's M1 refinement: `payloads.rs:27-42,121-137`, `deny_unknown_fields`, `NEEDS_UNITS`/`DEBUG_ONLY` defaults for rotate/follow, and `DEBUG_ONLY` + `!NEEDS_UNITS` for `debug.set_terrain`.
  - `SimQuery::Debug` / `SimReply::Debug` sit in already-`#[non_exhaustive]` enums (`fl_view/src/runner.rs`). `ToRunner` is untouched, which leaves R80 to Task 12.
  - The debug-query registry matches ADR-0015 (`debug_query.rs`, `builder.rs`). Details are under "Debug-query registry" below.
  - `PlayerView`, `AiController`, `NullAi` and `ai_controller` match ADR-0016 word for word (`fl_view/src/ai.rs:1-79`).
- One accepted deviation (Minor 5): `fl_cli query` takes the source as a positional argument. The brief's interface line asks for that, but its test text says `--scenario`. The positional form is right: it matches `inspect`/`save`.
- ⚠️ Cannot verify from this diff:
  - Command-name registration and handlers for `core.rotate`, `core.follow` and `debug.set_terrain` land in Tasks 5 and 9. This task only defines the payload types and names. The dense-id shift (`core.follow` < `core.move`) and its R73 remap are exercised then.
  - R71/R79 (`core.invalid_target` as `RejectReason::Feature`) aren't exercised here, because no handler rejects anything yet. The controller should check this at Tasks 5 and 9.
  - BRP `fl.debug_query` (Task 18). It will reach the registry through `ToRunner::Query`, which forwards every `SimQuery` (`fl_sim/src/runner/threaded.rs:51`).
  - The track mismatch (`--track sim-core`) and the 7eeb217 check-scope flag are known and accepted.

### Strengths

- The format guard is respected:
  - `FORMAT_VERSION` stays 1.
  - The payloads are standalone types, carried as postcard bytes inside `CommandKind`, so no existing traced type changes.
  - The guard explicitly passes added types (`crates/fl_protocol/tests/support/format_guard.rs:104-112`).
  - The `.new` snapshot only adds three lines.
- The registry is deterministic and contained:
  - It is a `StableRegistry<DebugQueryFn>` sorted on freeze (`crates/fl_sim/src/registry.rs:78-81`).
  - Its dense ids are never exported: `finish()` exports every registry except `debug_queries` (`builder.rs` finish hunk).
  - `DebugQueries` is classified External, so it is never hashed or saved.
  - Queries take `&World`, so read-only access is enforced by the type system.
  - `SimRunner::query` binds `let world = &self.world` (`runner/mod.rs:133`).
- `debug_query_does_not_change_state` checks both the state hash and `next_tick` (`crates/fl_sim/tests/debug_query.rs:137-155`).
- The error strings are pinned exactly, in both the sim tests and the CLI tests (`fl_cli/tests/agent.rs:582-631`, including the `{"error": …}` JSON shape and exit 1).
- A malformed `--args` is a usage error (exit 2) and is caught before the sim is built (`query_cmd.rs:24-28`).
- `PlayerView::from_snapshot` uses a checked shift, so team ≥ 8 sees nothing rather than overflowing (`ai.rs:30`).
- The test covers the own/visible split across teams, keeps sim_id order, and compares records by equality (`fl_view/tests/view.rs:208-232`).
- The gate's golden replays and testbed hash streams stay green (test-headless: 393/394, and the one failure is the unblessed snapshot). So the new External resource and its `core.debug_queries` entry in the resources string table change no hash stream.

### Review questions answered

- **Payload codecs and versions (R70, ADR-0007):** consistent.
  - Payloads carry no per-type version. Adding one needs no bump under ADR-0007's M1 refinement, and the guard allows added types.
  - The `(u32, u32)` corners trace as `TupleArray{U32,2}`, and `terrain` is a content name (a `String`), never a dense id.
- **Debug-query registry:**
  - Name-keyed, sorted on freeze, frozen in `finish()`.
  - A duplicate name panics naming both features, through `StableRegistry`.
  - Read-only on sim state and not hashed.
  - Reachable from `SimRunner::query` (inline and threaded), from the CLI, and later from BRP through the runner channel.
- **fl_cli calling `fl_sim::debug_query` directly:** acceptable.
  - `Session` wraps an inline `SimRunner` (`crates/fl_cli/src/agent.rs:134,317`), and `run_to` returns between ticks. `SimRunner::query`'s `Debug` arm is a pure pass-through to the same function with the same strings. So the runner path adds no threading or tick-consistency guarantee that the CLI skips.
  - The one behavioural difference is panic handling: the threaded runner wraps queries in `catch_unwind` (`threaded.rs:51`), while the CLI lets a panic escape with exit 101. Both are failures, so this doesn't matter (see Minor 2).
  - The cost is one extra public item, `fl_sim::debug_query`. That follows the existing precedent of `inspect_world`/`inspect_entity`, which `fl_cli inspect` calls the same way (`crates/fl_cli/src/inspect.rs:8,29-33`).
- **PlayerView / AiController leakage:** none.
  - The types are `EntityRecord`, `SimSnapshot`, `CommandDraft` (fl_view) and `StringTables`, `PlayerId`, `TeamId`, `Tick` (fl_protocol). No `fl_sim` or Bevy types appear, and arch-check passed.
  - The trait matches ADR-0016 exactly (`Send + Sync + 'static`, `id`, `think(&mut self, &PlayerView, &StringTables) -> Vec<CommandDraft>`).
- **`#[should_panic]` (test-guard NewShouldPanic, `crates/fl_sim/tests/debug_query.rs:127`):** justified; accept.
  - The brief and ADR-0015's Confirmation mandate it.
  - Panicking at build on a duplicate name is the established `StableRegistry` contract for every registry (commands, streams, journal records), so there's no `Result` to assert on instead.
  - The `expected` substring is specific.
  - The `other → test` dependency makes the registration order deterministic, because `sort_features` breaks ties by id. Without the dependency, `other` would build first and reverse the names.

### Issues

#### Critical (Must Fix)

None.

#### Important (Should Fix)

None.

#### Minor (Nice to Have)

1. **The sortedness test can't fail.** `crates/fl_sim/tests/debug_query.rs:110`: `assert!(names.is_sorted())` passes even without sorting. `core.debug_queries` is registered first (in `SimBuilder::new`) and `test.echo` second, and that insertion order is already sorted.
   - The behaviour itself is correct (`registry.rs:78-81`); only this test is weak.
   - Fix: also register a query whose name sorts before `core.` (for example a feature `aaa` with `aaa.q`), so the test exercises the sort on freeze.
2. **The query panic contract is undocumented.** `crates/fl_sim/src/builder.rs:248-255`: `register_debug_query`'s docs don't say that a query must answer `Err` on bad args and never panic.
   - On the threaded runner a panicking query becomes `RunnerFailure`, which stops the match (`threaded.rs:51,63-67`). Args arrive as untrusted JSON from BRP and the CLI.
   - Fix: add one doc sentence here and in ADR-0015's query bullet, and carry the rule into the Task 5/7/8 dispatches (`core.passability`, `core.terrain_at`, `core.clusters`, `core.paths`).
3. **The CLI and runner query paths could drift.** `crates/fl_cli/src/query_cmd.rs:31` calls `fl_sim::debug_query` rather than the runner path. That's acceptable now. If `SimRunner::query`'s `Debug` arm ever gains behaviour (metadata, panic capture), the CLI will drift.
   - Fix: when `agent.rs` is next touched, add `Session::query(SimQuery) -> SimReply` over `self.runner.query` and route `query` through it, so both paths share one code path.
4. **The team ≥ 8 path is untested.** `crates/fl_view/src/ai.rs:30`: the `checked_shl` branch has no test.
   - Fix: add one assertion to `player_view_splits_own_and_visible`: `TeamId(8)` gives an empty `visible` and still a full `own`.
5. **The plan's test text disagrees with the implementation.** Plan and brief test text (Task 2 Step 1, and ADR-0015 if it is quoted there) say `query --scenario base.testbed`; the implementation and tests use a positional source (`fl_cli/tests/agent.rs:582-631`).
   - Fix: sync the plan text. No code change.
6. **The gate is over budget.** The reported gate output carries `WARN gate task took 423s (budget 300s)` (api 124s, data-check 105s, test-headless 153s). That's process noise rather than code, and probably comes from cold `cargo xtask api` rustdoc runs.
   - Fix: none needed in this task. The controller should watch it, because from Task 4 the Stop hook enforces the gate.

### Checks run (outside the diff, one per named risk)

- **Risk: the listing isn't sorted.** Checked `crates/fl_sim/src/registry.rs:4-24,78-81,153`: `freeze` sorts the keys, and `iter` yields in dense-id (name) order.
- **Risk: the threaded runner doesn't forward `Debug`.** Checked `crates/fl_sim/src/runner/threaded.rs:40-70`: every `ToRunner::Query` goes to `SimRunner::query`, under `catch_unwind`.
- **Risk: the CLI bypasses runner guarantees.** Checked `crates/fl_cli/src/agent.rs:134,290-319` (`Session` wraps an inline `SimRunner`; `run_to` stops between ticks; `world()` is `runner.world()`) and `crates/fl_cli/src/inspect.rs:8,29-33` (the same direct-call precedent).
- **Risk: the format guard rejects added types.** Checked `crates/fl_protocol/tests/support/format_guard.rs:100-135`: only types present in both the committed and the current snapshot are compared.
- **Risk: the `.new` files are stale after the Task 15 merge (HEAD is now d66f685).** `git diff --stat c855f94 d66f685 -- crates/fl_view crates/fl_protocol crates/fl_sim xtask/snapshots` is empty, so the `.new` files are still valid.
- **Risk: a `.new` file removes or changes existing entries.** Scanned the appended `.new` diffs for removed lines. The only one is the insta metadata `assertion_line: 60` → `63` in the protocol snapshot. The three API diffs are additions only.

### Bless recommendations

- **`crates/fl_protocol/tests/snapshots/formats__protocol_formats.snap.new`: BLESS.**
  - It adds exactly `DebugSetTerrainCmd`, `FollowCmd` and `RotateCmd`, and `FORMAT_VERSION` stays 1, as ADR-0007's M1 refinement allows.
  - No existing type line changed. The `assertion_line` change is insta metadata only.
- **`xtask/snapshots/api/fl_protocol.txt.new`: BLESS.** Additions only: the three payload structs, their `CommandPayload` impls and their root re-exports.
- **`xtask/snapshots/api/fl_view.txt.new`: BLESS.** Additions only: `fl_view::ai::{PlayerView, AiController, NullAi, ai_controller}` plus their re-exports, `SimQuery::Debug{name,args}` and `SimReply::Debug(Value)`. They match ADR-0015 and ADR-0016.
- **`xtask/snapshots/api/fl_sim.txt.new`: BLESS.**
  - Additions only: `DebugQueries` (and the Component/Resource impls Bevy derives), `DebugQueries::names`, `SimBuilder::register_debug_query`, `DebugQueryFn` and `debug_query`.
  - `debug_query` goes beyond the brief's interface list. Accept it, on the `inspect_world` precedent, and name it in the bless reason.

### Assessment

**Task quality:** Approved

**Reasoning:** The contracts match ADR-0007 (M1), ADR-0015 and ADR-0016 exactly. The registry is name-keyed, sorted, frozen, External and read-only by type. The tests pin behaviour and error strings. All six findings are minor (test strength, a missing doc contract on query panics, and plan-text sync).
