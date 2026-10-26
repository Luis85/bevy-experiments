# M0 final fix wave A (sim contracts): review

Range `56e3425..040185a` (16 commits), diff package `review-fixa.diff`. I read the diff in four passes (protocol and
build.rs, classify/builder, commands/replay/save/step, then tests, snapshots and docs). Read-only: no edits and no
cargo runs. Outside the diff I made these focused checks: the committed snapshots, budgets and API listing against
their `.new` files; every `load_world`/`ReplayDriver::new`/`replay_app` call site; `SimIndex::insert`; Bevy 0.20
`World::bootstrap`; `fl_catalog::features()`; the git dir layout of `C:\fl\a`; the fl_sim build-script output under
`C:/t/fl`; and the gate and nextest logs.

## Spec Compliance

- ✅ Spec compliant. All 16 brief items have code, tests and docs:
  - **1 (I1):** `commands.rs` `remap_command_id(s)`. It is applied in `save/load.rs:122-123`, `replay.rs` (`replay_app`
    validates, `replay_inputs` remaps, `verify_replay` plays `replay_inputs`) and `fl_cli/src/agent.rs:208-213`.
    `fl_game/src/builder.rs:458` uses `loaded.pending`, so it is covered too. Unknown names give
    `LoadError::Command` / `ReplayError::Command`. Tests are in `save.rs` and `replay.rs` (the `aaa.noop` pattern).
  - **2 (I2/R70):** `header.rs` `features_not_newer` (ids order-free, saved version ≤ ours). `check_replay` and
    `check_handshake` are unchanged and strict, and `protocol.rs` pins both. The guard's `rose && !reads_version`
    branch fails a bump that has no migration (`format_guard_flags_bump_without_migration` covers no-reads, reads of
    the wrong version, `reads 1, 2` and `reads older`). Recorded in the ADR-0006/0007 refinements.
  - **3 (I3):** `check_tick` runs behind `cfg!(debug_assertions)` in `step.rs`. The watermark is set only in the
    debug branch of `build_sim_app`. `is_sim_type` splits on every non-path character, so generic arguments count,
    and every `bevy_ecs::…::Messages` counts. `ChildOf`, `Children` and `Observer` are checked by instance. The
    debug-only tests are `#[cfg(debug_assertions)]`.
  - **4 (I4/R72):** `Class::{Transient, TransientReset, transient(), TransientKept}`, one exclusive
    `reset_transients` in the Housekeeping core slot, and `clear_message_buffers` after the outputs are taken. The
    golden fixture is now `Class::transient()`.
  - **5 (I6):** `SimEntity(SimId)` has a private field and `pub(crate) new`. `Copy`/`Clone` is the only way left to
    duplicate one. A duplicate panics in `SimIndex::insert` ("another entity already has `id`"), so there is no silent
    public constructor path. `check_sim_entities` counts only classified components.
  - **6 (R71):** `#[non_exhaustive]`, `Feature(&'static str)` and `code()`. The event and journal carry the code.
  - **7 (Minor 4):** `fl_test_utils/tests/formats.rs` builds from `fl_catalog::features()` on the base mod.
    `Codec::custom` takes a `trace`.
  - **8 (Minor 7):** `SaveError`, and `SimReply::Error` from the runner, with a test.
  - **9 (Minor 9):** `crates/fl_sim/build.rs`, with a `"dev"` fallback when git fails.
  - **10:** `Stream` token, with `with_input` keeping the system names stable.
  - **11:** `allow_ambiguous_resource::<SimCounters>()` with the §15 comment.
  - **12:** `spatial.checked`.
  - **13:** `register_rebuild`, run in load order, plus the ADR text.
  - **14:** spec revision 2.2.
  - **15:** caps as code constants (accepted by R77).
  - **16 (T30):** the `#[cfg(debug_assertions)]` gate.
- Accepted out-of-scope items, as the brief and rulings expect: the `fl_cli` call sites (R75), and stale wording in
  `.claude/rules/sim.md` and `.claude/agents/reviewer.md` (the lead handles it).
- Breaking commits use `!` and every one carries a `Refs: ADR-NNNN` trailer (checked with `git log` trailers).
- ⚠️ Cannot verify from the diff: whether Cargo reruns `fl_sim`'s build script on every invocation in this checkout
  (Important 1 below). The controller should run `cargo check -p fl_sim -v` twice. The second run must say
  `Fresh fl_sim`, not `Dirty … the file '…/packed-refs' is missing`.

## Determinism checks

- **Golden replay untouched.** `git diff --stat 56e3425..040185a` touches neither
  `crates/fl_test_utils/tests/replays/testbed_wander.replay` nor `fl_test_utils/tests/golden.rs`. The only "golden"
  file in the range is `fl_sim/tests/golden.rs`: the fixture reclassification, plus the `Stream` closure capture,
  which leaves the draw key unchanged.
- **The Transient housekeeping cannot reach the hash:**
  - `reset_transients` touches only resources classified Transient (never hashed) and components classified
    Transient (never hashed).
  - Core's resets (`SimEvents`, `SimCounters`, `CommandLog`) do the same work as the five removed clear and rotate
    systems.
  - `try_resource_scope` moves only resource entities, which no sim query matches.
- **The end-of-tick clear runs after `compute_state_hash` and after the output is built** (`step.rs`). Message
  buffers are `TransientKept`, so not hashed. `Messages::clear` keeps `message_count`, so `MessageReader` cursors
  stay valid.
- **Behaviour is unchanged in M0.** The three perf-counter `.new` snapshots differ from the committed ones only by
  the added `spatial.checked` line. `spawn.entities`, `despawn.entities`, `publish.*`, `spatial.relocations` and
  `spatial.query_candidates` are identical, which shows the same spawns, despawns, movement and order over 200 ticks
  at 500, 3000 and 5000 units.
- **The debug-only `check_tick` changes no world state** except `ClassifiedUpTo`. That resource is
  `init_resource`d in every build profile and is Transient, so debug and release hashes cannot diverge.
- **Ordering.** Five unordered Housekeeping systems became one exclusive system. `ShuffleProbe` in `determinism.rs`
  restores a non-vacuous shuffle check, which is justified.

## Strengths

- The remap design is careful. An id the recorded table never had maps to `u16::MAX`, so a recorded `UnknownType`
  rejection replays as the same rejection instead of failing the load (`commands.rs` `remap_command_id`).
- The format guard extension is small and correct. Its tests pin each branch, including a bump that reads only an
  intermediate version.
- The alloc-budget increase is fully explained, and it is not hiding a regression. Raw per tick it is +656 B and +2
  allocations (the budgets are `ceil(max × 1.25)`: 18030→18850 is 14424→15080 raw, and 64→67 is 51→53). That is
  exactly one std `BTreeMap` root split: a 280 B leaf plus a 376 B internal node for `&'static str → u64` with
  capacity 11. `SimCounters` goes from 11 keys to 12. The report's measurement (budgets unchanged without the counter)
  agrees.
- `check_sim_entities` avoids allocating when it passes (empty `collect`s), so debug-build alloc budgets stay
  meaningful.
- Moving the codec blocks loses nothing. Apart from the new `reads older` labels, the bodies are identical, except
  `core.unit_type`'s root, which changed from `UnitType → UnitTypeId → U16` to `U16`. That is a pure trace
  correction: `encode_unit_type` writes `unit.0.0` as a bare `u16`, and postcard newtypes are transparent.
- `failed_save_query_replies_with_an_error_and_the_match_runs_on` proves that the runner survives a failed save, not
  just that an error comes back.

## Issues

### Critical (Must Fix)

None.

### Important (Should Fix)

1. **`crates/fl_sim/build.rs:51-58` emits `rerun-if-changed` for paths that may not exist.** In `C:\fl\a`,
   `C:/Projects/bevy-experiments/.git/packed-refs` does not exist (checked), and the build-script output under
   `C:/t/fl/c5/…/build/fl_sim-*/output` lists it. A branch whose ref is only packed has no loose
   `<common>/refs/heads/<branch>` either.
   - **Why it matters:** Cargo's own FAQ ("Why is Cargo rebuilding my code?", in the local toolchain docs at
     `share/doc/rust/html/cargo/faq.html`) names this exact case: a `rerun-if-changed` on a file that doesn't exist
     makes Cargo keep running the build script, and the stated fix is to avoid printing it. So the build script
     reruns on every cargo invocation and rebuilds `fl_sim` and every crate that depends on it, across every lane's
     dev loop and the gate. sccache softens this, but does not remove the relinks and test rebuilds. The ⚠️ check
     above confirms the effect in this checkout.
   - **Fix:** emit each path only if it exists. Always watch `HEAD`. Watch the loose ref if present, else
     `packed-refs` if present. If neither exists, watch the `<common>/refs/heads` directory, which Cargo scans
     recursively.

### Minor (Nice to Have)

1. **The dirty flag and the ADR wording.**
   - **ADR overclaims.** `build.rs:39-46` computes `-dirty` only when `HEAD` or the ref moves. ADR-0007's refinement
     says "`-dirty` when tracked files differ from `HEAD`", but edits made after a commit build as the clean revision.
     So two peers with different uncommitted code share one `build_id`, and the handshake check is weaker than stated.
     Fix: reword the ADR, or also watch `<git-dir>/index` (this catches staged changes only).
   - **Older git.** `--path-format=absolute` needs git ≥ 2.31. On older git the ref watch is skipped silently, and
     the build id goes stale after commits.
   - **False positives.** `diff-index` without an index refresh reports stat-only changes as dirty.
2. **`classify.rs:926` and `:933`: the panic strings lost their `\` line continuations.** The messages contain runs
   of 18 and 14 literal spaces ("…with SimBuilder):                  [names]"). Restore the continuations.
3. **`commands.rs` `remap_command_id`: the doc and the code disagree.** The doc says "# Errors: the saved table is
   missing", but a missing `commands` table maps every id to `u16::MAX`, and the command is silently rejected as
   `UnknownType`. Either return `Err` when the table is absent and the input has commands, or fix the doc.
4. **`ChildOf`, `Children` and `Observer` are re-checked only when the component count grows** (`classify.rs`
   `check_tick`). If a forbidden type is already registered at build (for example any system with
   `Query<&ChildOf>`) and first instanced mid-match, nothing catches it. `forbidden_in_world` costs 3 lookups and an
   archetype scan, so run it every tick next to `check_sim_entities`.
5. **`fl_math::Rolls::new` and `stream_key` stay public, so the `Stream` token can be bypassed** (`rng.rs`). A feature
   can still draw from an unregistered key; `scale.rs` does this itself for placement, in setup code. The module doc's
   claim that "every stream a system draws from is registered" overstates the guarantee. Fix: a clippy
   `disallowed-methods` entry for `Rolls::new` in sim and feature crates outside `fl_sim::rng`. That is a lane-B path
   (clippy templates), or reword the doc.
6. **`Class::Derived` no longer forces a rebuild** (`classify.rs` `register_component`/`register_resource`, and
   `register_rebuild`). A feature that classifies Derived state and forgets `register_rebuild` loads with a stale or
   empty cache, and only a branch test would notice. Consider a freeze-time panic when a feature has a Derived type
   that is not hook-maintained but registered no rebuild, with an explicit marker for the hook-maintained case.
7. **The end-of-tick clear silently drops a `SpawnRequest`/`DespawnRequest` written after the Lifecycle funnels**
   (`classify.rs` `clear_message_buffers`, `spawn.rs` doc). Such a request used to be delivered the next tick, or
   asserted at save time. The funnels `drain()` these two buffers, so a non-empty one at clear time means a lost
   spawn. A `debug_assert!` there would keep the old protection.
8. **`save/mod.rs` `SaveError` is not `#[non_exhaustive]`,** although `RejectReason` is under the same post-M0
   contract-freeze policy.
9. **`tests/hash.rs` `hash_independent_of_spawn_order` no longer spawns in the opposite order.** Both worlds now
   allocate #1 then #2, so the name claims more than the test checks. Keep the order case as a unit test inside
   `fl_sim`, where `SimEntity::new` is reachable, or rename the test. test-guard did not flag this.
10. **The `SimCounters` alloc budget will grow with the number of counter keys** (no action for M0).
    `BTreeMap::clear` frees the nodes every tick, so each root split costs allocations every tick. The M1 perf work
    could zero values in place instead.

test-guard (33 findings, `C:/t/fl/fixa-gate-pr.log:10-43`): accept all. Each is a replacement, a debug-only gate on a
debug-only check, a new `should_panic` with a specific message, or an assert moved to `fl_test_utils/tests/formats.rs`.
The nextest log has no warnings.

## Bless recommendation per `.new` artifact

| Artifact | Diff vs committed | Recommendation |
|---|---|---|
| `crates/fl_sim/tests/snapshots/formats__save_codec_formats.snap.new` | `SAVE_FORMAT_VERSION`/`REPLAY_FORMAT_VERSION` sections byte-identical; only the codec blocks are removed (moved) | Bless |
| `crates/fl_test_utils/tests/snapshots/formats__save_codecs.snap.new` | New file. Blocks equal the old fl_sim ones except the `reads older` labels on `core.players`/`core.unit_type` and the `core.unit_type` root `U16` (a trace correction, same bytes) | Bless (it becomes the guard's first baseline) |
| `crates/fl_test_utils/tests/snapshots/system_order__sim_tick.snap.new` | 5 Housekeeping lines (`rotate_messages`×2, `clear_command_log`, `clear_counters`, `clear_sim_events`) → `fl_sim::classify::reset_transients`; everything else identical | Bless |
| `crates/fl_test_utils/tests/snapshots/perf_counters__scale_500.snap.new` | Adds `"spatial.checked": 336000` (= 1680 entities × 200 ticks) only | Bless |
| `crates/fl_test_utils/tests/snapshots/perf_counters__scale_3000.snap.new` | Adds `"spatial.checked": 2000000` (= 10000 × 200) only | Bless |
| `crates/fl_test_utils/tests/snapshots/perf_counters__scale_5000.snap.new` | Adds `"spatial.checked": 3336000` (= 16680 × 200) only | Bless |
| `docs/perf/budgets.ron.new` | Adds `spatial.checked` budgets (12500/2100/20850 = ceil(max × 1.25)). Alloc +820 B/+3 in each scenario, which is exactly one BTreeMap root split | Bless |
| `xtask/snapshots/api/fl_sim.txt.new` | 58 lines, all from this wave's contract changes: `Class` variants and `transient()`, `Derived` without a field, `RejectReason` non_exhaustive, `Feature`/`QueueFull`/`TooManyUnits`/`code()`, `LoadError::Command`, `ReplayError::Command`, `SaveError`, `Codec::custom` with a trace, `register_rebuild`, `register_stream → Stream`, `SimEntity(_)` and `id()`, `clear()`s, `Stream`, `MAX_*`, `replay_inputs`, `rolls(…, Stream)`, `save_world → Result`, `trace_type`. Nothing unexplained | Bless |

## Assessment

**Task quality:** Needs fixes. The only blocker is Important 1. It is a few lines in `build.rs`, but it can stall
every build if it is real. The Minor items can be parked.

**Reasoning:** The contract changes are correct and well tested, and determinism is preserved. The golden replay is
untouched, and the counter totals are identical except for the new counter. The remaining risk is the build script's
`rerun-if-changed` on paths that do not exist, which may rebuild `fl_sim` on every cargo run until it is confirmed or
fixed.
