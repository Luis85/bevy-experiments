# Task 3 re-review, fix round 1 (aac327d)

Read-only; one pass over review-t3-fix1.diff (ADRs 0006, 0017-0022, plan Tasks 5, 8, 9, 10, 11, 13, 14, 17, spec 2.5). No tests run (docs only).

## Finding verdicts
1. AddPct sign flip - ADDRESSED. ADR-0017 step 3 `max(0, 1 + sum add_pct)`; overflow bound restated as factor in [0, 1601]; plan test `add_pct_factor_never_negative`.
2. Total bucket order - ADDRESSED. (key bytes, SimId None first, op, value raw) matches R151; list kept sorted, evaluate sorts; plan `Modifiers`/`evaluate` signatures and `bucket_order_ignores_insertion_order` agree.
3. Stale cache vs direct base write - ADDRESSED. Cache stores base; effective_stat re-reads base via StatDef.base; dirty set is an optimisation; plan + `direct_base_write_is_never_stale`.
4. Late EffectRequest dropped - ADDRESSED. Hashed EffectQueue (queue_effect), drained first next tick, debug assert; ADR-0019 emission-window rule; ADR-0006 lists `core.effect_queue`; plan matches. Presence-only-while-non-empty is sound (absent state writes nothing; load restores absence).
5. Activity stepping - ADDRESSED in rules (child same tick, parent next tick, root Done then next order same tick, cancel = one step then new order same tick, constant 4, debug assert), but the budget arithmetic is wrong: see New 2.
6. Golden exemption - ADDRESSED for per-commit scoping (CommitFacts.replay_format_changed; change-class row). The hash-stream check is defective: see New 1.
7. Stale ledger under patches - ADDRESSED. Coverage vs final origins, staleness per mod vs own pre-patch docs + leaves its patches write; Task 8 text and `patched_leaf_keeps_base_ledger_valid` agree.
8. Whole-object override - ADDRESSED. Pass Default at its mod's turn, before that mod's patches, discards earlier patches, own origins, later/Final patches apply on top; `override_discards_earlier_patches`.
9. Holdout leaks - ADDRESSED as best-effort per R151: Bash guard, `mini-gate --holdout` counts only, trace counts only; plan Tasks 9/11/14 + tests agree. Residual (accepted): ancestor-rooted search/indirect paths, stated in the ADR.

Minors 1-13 - ADDRESSED (1 order named; 2 Modifiers removed when empty; 3 cap kept per R149, rejection journaled; 4 ConditionToken non-Clone/consumed/saved by name; 5 Override vs Mul 0 note; 6 "M2 direct-fire gaps" section; 7 MAX_ORDER_PARAM_BYTES=64 + ActivityKindId by name; 8 observer buffers named; 9 class D MCR, needs_class, content_changed; 10 reroll_base +100_000; 11 inherited-only Set fails + byte-wise file order; 12 titles match; 13 spec says Owner(PlayerId)). Plan amendments match ADR names/constants/signatures (MAX_ACTIVITY_STEPS_PER_TICK=4, MAX_ORDER_PARAM_BYTES=64, MAX_MODIFIERS_PER_STAT=16, queue_effect, reroll_base, CommitFacts).

M2 direct-fire section: concrete enough for the planner on the three decisions (Modifier.slot + core.modifiers v2; TurretActivities second stack stepped in ActSet::Fire (exists, schedule.rs:120); per-shot data in combat messages). Gaps listed as Minor 3 below.

## New breakage
1. Important - ADR-0021 golden-policy bullet and plan Task 5 `BranchFacts.refactor_goldens` compare merge base vs HEAD for "a golden changed by a class R or E commit". The same branch later re-blesses both goldens under class F (Task 13 MCR-001, Task 15 MCR-002), so HEAD's hash streams differ from the merge base's and the row reports a false violation at pr/milestone. Compare each R/E commit's golden against its parent (or restrict to goldens whose every changing commit is R/E).
2. Important - ADR-0018 stepping step 3 / Consequences, plan Task 13. Every push and pop counts, so one already-satisfied order costs 2 transitions: the budget of 4 admits two such orders per tick, not "more than four" as the Consequences say. Three consecutive instant orders (rotate, rotate, move) defer, changing M1 timing, and in debug builds trip the "an activity loops" assert on legitimate input. Also unstated whether cancel removals count: failing the 9th push cancels 8 frames, which would trip the assert in `activity_depth_is_capped`. Fix: exempt cancel steps, correct the Consequences arithmetic, and either size the bound from the M1 maximum chain (the pose trace is only a sample) or assert only when the budget is hit by pushes of a frame that ended in the same tick. The implementer's "raise the bound if Task 13's pose trace fails" is sound as a fallback but should not be the only check, since the assert is wrong for valid queues.
3. Minor - EffectQueue: no rule for a queued effect whose target despawned before the next tick, and no bound on queue growth (a chain that re-queues each tick is bounded only by content). Per-slot stats: `StatDef.base`, `effective_stat` and `EffectiveStats` have no slot parameter in the text; whether None-slot modifiers count toward the 16 cap per slot, and a bound on mounts (save decoder, as MAX_WAYPOINTS) are unstated. `mini-gate --holdout` "lead option" has no stated enforcement (hook role check).

## Out of scope
ADR-0022 line ~"a failing seed set may be rerolled once..." now exceeds the 120-column wrap used elsewhere (cosmetic).

## Verdict
Findings remain open: none of the nine Important, but two new Important (New 1, New 2) in the fix.
