# Planner report: M2 prelude plan

Plan: `docs/superpowers/plans/2026-10-25-frontline-m2-prelude.md` (commit `be12963` on `m1/phase-a`, 1,402 lines). Docs only: no code changed, no cargo run.

## Shape

- **21 tasks.** Task 1 is M17 (gate-time trim) plus M16 (test-guard allowlist). Task 2 is M11 (tag `m1` fixture, frozen `mod v2` MoveOrder, and a pose trace of `testbed_paths`). Both run first, in parallel, before anything touches `move_order`.
- **ADRs.** Task 3 writes ADR-0017 to ADR-0022, all `proposed`. They cover the core vocabulary, unit behaviour and the entity graph, the event model, content phases with patches and provenance, golden versioning, and verification from M2. It also refines ADR-0002, 0006, 0007 and 0015 and adds spec revision 2.5. The controller accepts them under R85.
- **Docs.** Task 4 writes the GDD, the mechanic template, five backfilled M1 mechanic specs (MOVE.SPEED, PATH.QUEUE, FLOW.GROUP, ORDER.QUEUE, STEER.SEP; protected `.new`) and the MCR changelog.
- **Topology:** two builder lanes, plus docs-only Tasks 3 and 4 running alongside Task 1 (R21).
  - Lane 1 (`C:\fl\a`, `m2p/phase-a` from tag `m1`): 1 → 5 → 6 → 9 → 11 → 13 → 15 → 17 → 21.
  - Lane 2 (`C:\fl\m2pNN`, shared build dir per R31): 2 → 7 → 8 → 10 → 12 → 14 → 16 → 18 → 19 → 20.
  - `main` is fast-forwarded with `gate pr` green after Tasks 6, 13, 17 and 21.
  - Shared append points (the gate `steps()` table, subcommand enums, `lib.rs` re-exports, `SimBuilder` methods, table constants) are resolved by the controller.
- **Version and re-bless sequence:**
  - `SIM_VERSION` goes 2 → 3 in Task 13 (MCR-001) and → 4 in Task 15 (MCR-002).
  - MCR-000 (Task 5) re-records the goldens for the format change only; their hash streams don't change.
  - Every re-bless goes through `fl_cli rebless` and onto the final ratification list (R146).
- **Outcome-neutral proofs.**
  - Refactor-class tasks (5, 6, 7, 10, 12, 16, 17, 19) must keep the goldens verifying without a re-bless.
  - Task 13 (C7) must keep the pose trace from Task 2 identical, both for a fresh run and for a continuation of the m1 save.

## Spec gaps (resolved in the plan; listed in "Builder's calls")

1. C6: ron 0.12's `deserialize_any` drops bare identifiers, so a generic value tree loses enum variants. The plan adds its own lossless RON-subset tree, accepted only if every rules hash comes out byte-identical.
2. C2: the closed vocabulary has no class for values the spec fixes (hash interval, spatial cell). The plan makes them `Guess`, with ids in GDD §Guesses.
3. C3: holdout files can't be kept secret mechanically while R65/R82 dispatch general-purpose agents. The plan adds a `guard-read` hook for role agents, a brief rule, and a `holdout` Cargo feature that keeps holdout out of implementer builds.
4. C8: "re-bless needs the user" conflicts with the user's delegation. Encoded as R146: the lead re-blesses with an MCR, and the re-bless is listed for ratification.
5. C9: cargo-mutants has no global time cap, and killing an in-place run is unsafe. The plan uses a copy-mode xtask wrapper with its own build dir. The controller runs it per task at review, not in the Stop hook.
6. C5: no command-button API exists before M6. The plan adds the declaration contract, `requires` evaluation and a `core.conditions` view channel; drawing stays in M6.
7. C11: `VisibilityRange` measures camera distance, but the orthographic zoom doesn't change it. The plan ties the rig's distance to the view height.
8. R77: C7 was to add an Orders rules section. `MAX_WAYPOINTS` stays a constant because it bounds the save decoder. `MAX_COMMAND_UNITS` moves to `fl_protocol`.
9. C7: the order queue keeps the state name `core.move_order`, since there is no codec rename mechanism.
10. C10: "per-mechanic mini-gate" is undefined. The plan defines it as `cargo xtask mini-gate <AREA.NAME>`, built from the trace map.
11. C4: the source of the nightly seed rotation is unspecified. The plan sets `FL_SEED_BASE` to the number of days since 2026-01-01.
12. M17: "budget overrun FAIL at nightly" is ambiguous. The plan adds a nightly `task-time` row, level budgets that warn, and a timing log.
13. Workflow stage 1 says "human approves" each mechanic spec. The lead approves the five backfilled M1 specs, and they are listed for ratification.

## Assumptions

- Tag `m1` exists (the controller creates it at M1 close) before Task 2. Task 2 uses `fl_cli save` on the golden `testbed_paths.replay` at tick 310, which has queued orders, a follower and a flow group.
- Already closed in the M1 fix wave, so not replanned: I3, I4, M2, M9, M15.
- Prelude scope stops short of some items:
  - Unit scope only for modifiers.
  - Trigger verbs get registries, but nothing runs them yet.
  - The R115, R127 and R130 crowd work, R136 densification and M6 resumable flow stay parked with explicit triggers. Task 19 does R98, R125 and the Task 7b buffer and early-exit work, which must keep plans identical.
- The perf baseline becomes `docs/perf/M2P.json` (Task 21). It replaces R143's `M1.json` as the reference for M2's 20 % rule.
- The milestone-style check is `gate milestone --milestone M2P`.
- No downloads, no toolchain changes. The only manifest edit is `[profile.release-check]` (Task 6).
