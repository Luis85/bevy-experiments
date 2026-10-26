# M0 final fix wave A — sim contracts (lane 1, worktree C:\fl\a, branch m0/phase-a)

Tracks: contract, sim-core, docs (`cargo xtask check-scope --track contract,sim-core,docs`; gate base 56e3425 is in `.gate-base`).
Source of findings: C:\fl\a\.superpowers\sdd\2026-10-23-frontline-m0-rev2\final-review-m0.md (read the cited sections; each has file:line, failure scenario, fix). Rulings R70–R74 in progress.md (same dir) are binding.
Another builder works in parallel in C:\fl\fixb on xtask/, OWNERS.toml, .claude/, CLAUDE.md, clippy templates, fl_cli, fl_client, fl_game, fl_observe, debug.ron, docs/debugging.md, docs/perf/**, fl_test_utils/src/perf.rs and fl_test_utils/tests/system_order.rs. Do NOT touch those paths.

Fix, TDD (failing test first) for each behavioural item:
1. I1 — remap pending-command `kind.type_id` by name on load (`LoadCtx::remap(tables::COMMANDS, id)`, unknown name → LoadError) and in `replay_app`/`verify_replay` from the recorded header's tables. Test: a save with a pending command loads into a sim with an extra command name sorting first (reuse the `"aaa.noop"` fixture pattern); same for a replay.
2. I2 / R70 — `check_save`: same feature ids, save feature version ≤ build's; codec versions + migrations decide. `check_replay`/`check_handshake` stay strict. Format guard (crates/fl_protocol/tests/support/format_guard.rs and its users) fails when a codec version rises without a migration covering the committed version (expose the migration list in `codec_formats`). Tests: load a save whose header has an older feature version; guard rejects bump-without-migration. Record as "Refinements (2026-10-24)" in ADR-0006 (and ADR-0007 if it states handshake rules).
3. I3 — under `debug_assertions` re-run `check_classification` in `step_tick` when `world.components().len()` grew; `is_sim_type` considers every path segment incl. generic args; whitelist only `Messages<M>` registered via `SimBuilder::add_message`; reject `ChildOf`/`Children`/`Observer` in the sim world. Tests for late registration and the `app_mut().add_message` bypass.
4. I4 / R72 — framework-enforced Transient: Housekeeping removes Transient components; Transient resources reset via Default (or a registered reset fn) unless opted out with a named reason (Journal, SnapshotOut, …); all `add_message` buffers cleared at end of `step_tick`. Fix the `"aaa.draws"` golden fixture misuse (classify it correctly; if golden output changes, write `.new` and report — never bless). Tests.
5. I6 — `SimEntity` field private (crate-private ctor, `pub fn id()`); under `debug_assertions` assert in `step_tick` that no entity with a classified component lacks `SimEntity` (resources are entities in 0.20 — don't count raw entities). Test.
6. I7 / R71 — `RejectReason` `#[non_exhaustive]` + `Feature(&'static str)`; `SimEvent::CommandRejected` carries a stable code (e.g. `core.late`; feature codes verbatim), not `format!("{:?}")`.
7. Minor 4 — the codec-format snapshot covers every catalog feature (move the codec section to fl_test_utils using `fl_catalog::features()` — a new test file, NOT tests/system_order.rs or src/perf.rs) and `Codec::custom` requires its own trace. Snapshot changes → `.new`, report, do not bless.
8. Minor 7 — `save_world` returns `Result`; `SimQuery::Save` replies `SimReply::Error` instead of killing the runner. Test. (If fl_client/fl_game call `save_world` directly and stop compiling, make the minimal call-site change and list it in the report.)
9. Minor 9 — real `build_id()`: `crates/fl_sim/build.rs` with `git rev-parse --short HEAD` + dirty flag (fall back to "dev" without git); `cargo:rerun-if-changed` on .git HEAD and the ref (worktrees: resolve via `git rev-parse --git-dir`).
10. Minor 10 — `register_stream` returns a `Stream` token; `rolls` takes the token. Update all call sites in sim crates.
11. Minor 11 — `allow_ambiguous_resource::<SimCounters>()` (counter adds commute), with a comment citing spec §15.
12. Minor 12 — count `spatial.checked` in `track_spatial`. If this changes a budget/perf snapshot, write `.new` and report.
13. Minor 15 — contract: Derived rebuild = one rebuild fn per feature, run in feature load order; make the code match if it currently runs in name order. Record in the ADR covering save/load.
14. Minor 16 — spec: add a "Revision 2.2 (2026-10-24)" refinements section, one line each: `MatchHeader.features` is topological load order; R70; R71; R72; item 13.
15. Minor 17 — cap `MoveOrder.waypoints` and `PlayerCommand.units` (core rules constants: 32 waypoints, 256 units; over-cap → reject with a stable code). Tests.
16. T30 — gate `unclassified_sim_type_fails_build` with `#[cfg(debug_assertions)]` (the check is debug-only).

Breaking contract changes: commit type with `!`, `Refs: ADR-NNNN` trailer. State hashes / golden replay must stay identical unless a fix necessarily changes them — then explain why and write `.new` artifacts for the lead.
Done when: `cargo xtask gate pr` PASS (put the summary in the report), clippy -D warnings, fmt. Commit per item group, conventional messages ending `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
