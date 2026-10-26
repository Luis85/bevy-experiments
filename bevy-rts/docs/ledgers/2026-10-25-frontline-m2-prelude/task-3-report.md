# Task 3 report: ADRs 0017–0022, refinements, spec revision 2.5

**Status:** DONE_WITH_CONCERNS (docs only, no cargo run). Commit `510f0ca` on `m2p/phase-a`.

## What I implemented

- **New ADRs** (MADR 4.0 from `template.md`, `status: "proposed"`, date 2026-10-25): each has Context, Decision
  Drivers, at least three Considered Options with pros and cons, Decision Outcome, Consequences, Confirmation (naming
  the plan's tests and gate rows) and More Information.
  - `docs/adr/0017-core-vocabulary.md`: conditions, stats and modifiers, tags, verbs, command buttons (C5).
  - `docs/adr/0018-unit-behaviour-and-entity-graph.md`: order queue, activity stack, `SimId` relationships, the rule
    for Hashed keys into Derived layouts (C7, R129).
  - `docs/adr/0019-sim-event-model.md`: Messages, hooks, limited observers, no entity disabling (C7).
  - `docs/adr/0020-content-load-phases-patches-provenance.md`: phases, the lossless tree, patches, path keys, origins,
    provenance ledger, deprecated keys (C6, C2).
  - `docs/adr/0021-golden-replay-versioning.md`: replay format 4, change classes, `SIM_VERSION` policy, MCRs, `fl_cli
    rebless`, gate rows (C8, M10, R146).
  - `docs/adr/0022-verification-from-m2.md`: specs and tracing, test author and holdout, statistics, mutants, review
    tiers and mini-gate, release determinism, gate time, journal, proptests (C1, C3, C4, C9, C10, I2, M17, R139,
    R140, R142).
- **Refinements** (`## Refinements (2026-10-25, M2 prelude)`, each with a "Status: proposed … R85" line and a
  Confirmation bullet): ADR-0002 (M3, R123), ADR-0006 (`migrate_with`, `LoadCtx::current_id`, core versions 3 and 4,
  relationship codecs, new prelude state), ADR-0007 (`MAX_COMMAND_UNITS` to `fl_protocol`, `core.no_map`, order kinds
  by name), ADR-0015 (lanes M5, stall rule M4, reload invalidation M1, `since_version` M8, immobile M7, one reset
  M12/M13).
- **`docs/adr/README.md`**: six rows (`proposed`); 0002, 0006, 0007, 0015 rows say "M2 prelude refinement 2026-10-25:
  proposed" with date 2026-10-25; the "Decisions still to write" paragraph now points at 0017–0022.
- **Spec** (`docs/superpowers/specs/2026-10-23-frontline-rts-design.md`): status line names 2.4 and 2.5; a
  `## Revision 2.5 (2026-10-25, M2 prelude contracts)` section above 2.4, one bullet per ADR and refinement in 2.4's
  `**Topic (ADR-…):**` format; §5.4's observer hazard line points at ADR-0019; §6.1's last line points at ADR-0020;
  §9.2 gains "Delivered by `docs/superpowers/plans/2026-10-25-frontline-m2-prelude.md` …"; §12.5's re-bless sentence
  now says re-blesses go through `fl_cli rebless` and follow R146.

Names were checked against the code: `Codec::{custom, migrate_from, fill_missing}`, `LoadCtx::{remap, saved_tables,
current_tables}` (`save/format.rs`), `RejectReason` and `MAX_COMMAND_UNITS`/`MAX_WAYPOINTS` (`commands.rs`),
`fl_client::orders::MAX_COMMAND_UNITS`, `ThinkSet::Orders`, `ResolveSet::{Effects, Stats}`, `LifecycleSet::Death`
(`schedule.rs`), `PathFollow.planned`, `PathState::Flow { key }`, `PathQueue.pending`, `BLOCKED_REPLAN_TICKS`,
`invalidate_paths`, `rebuild_derived`, `Mobility.speed`, `Owner(PlayerId)`, `FromRunner`/`ToRunner`, `DefRegistry`,
`LinkCtx`, `ModManifest` (`depends`, `load_after`), object `override`, ron 0.12's `handle_any_struct`, the existing
gate row names, and the classify tests `observers_are_not_allowed_in_the_sim_world` and
`relationships_are_not_allowed_in_the_sim_world`.

## Verification (Step 4)

```
$ git show --stat HEAD   # 12 files, all docs/adr/** or docs/superpowers/specs/**
$ grep -c '^status: "proposed"' docs/adr/001[7-9]-*.md docs/adr/002[0-2]-*.md
docs/adr/0017-core-vocabulary.md:1
docs/adr/0018-unit-behaviour-and-entity-graph.md:1
docs/adr/0019-sim-event-model.md:1
docs/adr/0020-content-load-phases-patches-provenance.md:1
docs/adr/0021-golden-replay-versioning.md:1
docs/adr/0022-verification-from-m2.md:1
```

The commit was made with `git commit -- <my 12 paths>`, so the other agents' uncommitted work in this worktree
(`OWNERS.toml`, `xtask/**`, `docs/design/**`) is untouched and not committed. No tests apply (docs only, no cargo).

## Files changed

`docs/adr/{0017,0018,0019,0020,0021,0022}-*.md` (new), `docs/adr/{0002,0006,0007,0015}-*.md`, `docs/adr/README.md`,
`docs/superpowers/specs/2026-10-23-frontline-rts-design.md`.

## Concerns (decisions I added to make the brief's contracts hold; please rule)

1. **ADR-0017: a modifier count bound.** The brief says the value bounds mean "no intermediate overflows `Fx`". That
   holds only if the number of modifiers is bounded: with `AddPct` up to 100 each, 21 of them on a stat at 10⁶ reach
   2.1 × 10⁹, past I32F32's limit. I added `MAX_MODIFIERS_PER_STAT` = 16 per unit and stat (`add_modifier` returns
   `Err` past it), with the arithmetic in the ADR (worst case 1.6 × 10⁹). Task 17 has no test for it; consider adding
   one (17th modifier rejected). Alternative: compute the buckets in `i128` and clamp before narrowing.
2. **ADR-0017: the dirty set can't be `TransientReset`.** The plan names a TransientReset `"core.stats_dirty"`, but
   `Input::Housekeeping` resets it at the start of every tick (R72). A unit marked after `ResolveSet::Stats`, in
   `Lifecycle` (say a death rule removing an aura source's modifiers), would then lose its mark before the next
   recompute and keep a stale cache, and a save/load would give a different value (desync). The ADR makes the set
   `Class::TransientKept` (emptied only by `recompute_stats`) and adds the R84 rule that `effective_stat` evaluates on
   the spot for a marked unit, so no read sees a stale value. Task 17's implementer needs that.
3. **ADR-0017: smaller additions.** `-1,000,000 ≤ min` (the brief bounds only `max`), `Override` values within `[min,
   max]`, and several overrides apply in bucket order (the last wins). Task 17's `extreme_modifiers_stay_in_bounds`
   uses `Add −2,000,000` on a stat with `max` 10⁶, which `add_modifier` rejects under the brief's `[-max, max]`. The test
   must call the pure `evaluate` (which still can't overflow), or use a value in bounds.
4. **ADR-0021: class R and golden bytes.** Task 5's own commit is `Change-Class: R` and changes both goldens (format 4,
   hash streams identical). But its `refactor_class_cannot_change_goldens` test says "a commit with Change-Class: R
   touching a .replay → violation". The ADR allows golden *byte* changes under R/E only when the branch changes
   `REPLAY_FORMAT_VERSION` and the re-bless report shows identical hash streams. Task 5 should build that exemption
   into `class_violations` (or the test's fixture should leave the format unchanged).
5. **ADR-0019: observers stay rejected for now.** No prelude task adds observer support, and the R73 check rejects
   `Observer` entities. So the ADR says the first feature that needs one adds a `SimBuilder` method that spawns named
   observers at build time, with an order-independence test, as a contract change under this ADR.

## Fix round 1 (R151)

Commit `aac327d` `docs(adr): ADR-0017–0022 review fixes (R151)`. Docs only; no cargo.

**Rulings written in:**
1. ADR-0017: step 3 is `v *= max(0, 1 + Σ add_pct)`.
2. ADR-0017: a total bucket order `(key bytes, SimId with None first, op, value raw)`; `Modifiers.list` is kept sorted
   and `evaluate` sorts its input.
3. ADR-0017: `EffectiveStats` stores the base next to the effective value; `effective_stat` evaluates on the spot when
   the unit is marked or the base differs. The `TransientKept` dirty set stays as an optimisation.
4. ADR-0017 and ADR-0019: late effects go to the Hashed `EffectQueue` (`queue_effect`), drained first at the next
   tick's Effects slot, with a debug assert on a late `EffectRequest` message. ADR-0019 adds a general
   emission-window rule.
5. ADR-0018: the stepping rules. A pushed child ticks in the same tick; a parent resumes on the next tick; the next
   order begins in the same tick after a root `Done` (as M1); `MAX_ACTIVITY_STEPS_PER_TICK` = 4 with a debug assert;
   one cancel step per frame, then the new order begins in the same tick.
6. ADR-0021: the format exemption is per commit, and `golden-policy` checks that the hash streams are equal.
7. ADR-0020: coverage is checked against the final origins; staleness per mod against its own pre-patch documents
   (plus the leaves its patches write).
8. ADR-0020: `override` takes effect in pass `Default` at its mod's turn, discards earlier patches and has its own
   origins; later patches apply on top.
9. ADR-0022: holdout secrecy is best-effort (guard-read plus a Bash guard). `mini-gate --holdout` (lead) reports
   counts only; `trace` reports holdout coverage as counts.

**Minors 1–13:**
- (1) `recompute_stats` runs before `run_activities`.
- (2) `Modifiers` is removed when empty.
- (3) The cap stays (R149); a rejected `add_modifier` from an effect is journaled, never a panic.
- (4) `ConditionToken` is not Clone, is consumed by revoke and is saved by name; a double revoke is a debug assert.
- (5) A note on `Override` versus `Mul 0`.
- (6) An "M2 direct-fire gaps" section in ADR-0018: per-slot stats (`Modifier.slot`, `core.modifiers` v2); a second
  `ActivityStack` per weapon mount (justified: turret and hull lifetimes are independent); per-shot data in combat
  messages.
- (7) `MAX_ORDER_PARAM_BYTES` = 64 (`core.param_too_large`); `ActivityKindId` is saved and hashed by name.
- (8) The allowed observer buffers are named.
- (9) Class D needs an MCR and re-blesses the goldens; `needs_class` covers `scenarios/**` and `Cargo.{toml,lock}`;
  `content_changed` is defined.
- (10) The reroll goes to `seed_base + 100_000`.
- (11) Patching an inherited-only path fails, and a note says how to work around it; patch file order is byte-wise on
  mod-relative paths.
- (12) The chosen-option titles now match the Considered Options and the Pros/Cons headings in all six ADRs.
- (13) The spec bullet says ownership stays `Owner(PlayerId)`.

**Spec revision 2.5** bullets for 0017, 0018, 0020, 0021 and 0022 are updated to match. ADR-0006's "New state" bullet
adds `core.effect_queue` and the `TransientKept` dirty set.

**Plan amended** (`docs/superpowers/plans/2026-10-25-frontline-m2-prelude.md`):
- Task 5: `CommitFacts` with a per-commit `replay_format_changed`, plus `BranchFacts.refactor_goldens` (hash-stream
  equality via `fl_cli replay dump --json`) and the extended `needs_class`. New tests:
  `format_commit_may_rewrite_goldens`, `format_exemption_needs_equal_hash_streams`, `data_commit_needs_mcr`.
- Task 8: the coverage and staleness rules, with fixture mods under `crates/**/tests/fixtures/**` exempt unless they
  carry `provenance/`. New test: `fixture_mods_are_exempt_from_provenance`.
- Task 10: `override_discards_earlier_patches`, `patched_leaf_keeps_base_ledger_valid`.
- Task 9: trace output names no holdout test.
- Task 11: the Bash guard and `reroll_base`, with `bash_guard_denies_holdout_paths` and `reroll_base_is_disjoint`.
- Task 13: the stepping constants and rules, and `run_activities` placed after `recompute_stats` from Task 17.
  `activity_depth_is_capped` now pushes once per tick so it stays under the step budget. Six new tests.
- Task 14: `mini-gate --holdout` reports counts only (`mini_gate_hides_holdout_names`).
- Task 17: stat bounds, the cap, the sorted and auto-removed `Modifiers`, the base-carrying cache, the `TransientKept`
  dirty set with the on-read check, the `EffectQueue`, the system order, and the `extreme_modifiers_stay_in_bounds`
  note. Seven new tests.

**Verification:** the commit has 9 files, all under `docs/adr/**` or `docs/superpowers/{specs,plans}/**`. The
`status: "proposed"` grep still prints 1 for each of 0017–0022. Other agents' uncommitted files are untouched.

**Concerns:**
- I amended plan Tasks 9, 10, 11 and 14 as well as 5, 8, 13 and 17. Rulings 7–9 change their contracts, so leaving
  them out would have left the plan disagreeing with the ADRs.
- `EffectQueue` is present only while it holds entries. Otherwise adding a Hashed resource would change every state
  hash (the hash counts present Hashed resources), and Task 17 claims hash neutrality.
- `MAX_ACTIVITY_STEPS_PER_TICK` = 4 can change M1 timing when more than four already-satisfied orders are queued.
  Task 13's pose trace is the check; if it fails there, raise the bound rather than change the trace.

## Fix round 2 (R153)

Commit `3da2852` `docs(adr): ADR-0018/0021 review fixes round 2 (R153)`. Docs only; no cargo.

**Rulings written in:**
- New 1. ADR-0021: the hash-stream check runs per commit, comparing each golden at an R or E commit against that
  commit's parent, never the merge base against HEAD.
  - Plan Task 5: `BranchFacts.refactor_goldens` becomes `(sha, path, equal)`, computed per commit against its parent.
  - New test `later_reblesses_do_not_fail_the_format_commit`.
- New 2. ADR-0018: the transition budget is replaced.
  - `MAX_ORDERS_BEGUN_PER_TICK` = 32 (the queue cap) limits root orders begun per unit per tick.
  - Pushes and pops within a chain are bounded only by the depth cap. Cancel steps are exempt.
  - A `MAX_REPUSH_PER_TICK` = 4 loop guard asserts in debug builds.
  - Consequences: no M1 timing change.
  - Plan Task 13: the step-budget tests are replaced by `full_queue_of_instant_orders_finishes_in_one_tick` and the
    unit test `repush_guard_counts_per_parent`.
  - `activity_depth_is_capped` now pushes every tick (8 frames in one tick, then the 9th push fails), and its cancel
    steps trip nothing.
- New 3, EffectQueue (ADR-0017, plan Task 17):
  - An entry whose `Unit` target despawned is dropped, with journal `core.effect_dropped` and reason `"target_gone"`.
  - `MAX_EFFECT_QUEUE` = 4096: past it the newest entries are dropped with reason `"queue_full"`, counted in counter
    `effects.dropped`, plus a debug assert.
  - New tests: `queued_effect_on_despawned_target_is_dropped` and `effect_queue_is_bounded` (release builds).
- New 3, M2 gaps (ADR-0018), for the M2 planner to implement:
  - `StatDef.base` and `effective_stat` take `slot: Option<u8>` (`None` = hull).
  - The 16 cap applies per `(stat, slot)` and counts the `None` modifiers too.
  - `MAX_MOUNTS` = 8.
- New 3, holdout (ADR-0022, plan Task 14): `mini-gate --holdout` refuses unless `FL_ROLE=lead`. New test
  `mini_gate_holdout_needs_lead_role`.
- The spec revision 2.5 bullets for 0018 and 0021 are updated.
- The over-long lines in ADR-0022 and ADR-0018 are reflowed.

**Verification:** the commit has 6 files under `docs/adr/**` and `docs/superpowers/{specs,plans}/**`. The proposed
grep prints 1 for each of 0017–0022.

**Concerns:**
- The `MAX_REPUSH_PER_TICK` guard can't be reached through the behaviour traits. A parent ticks at most once per tick,
  because a child's `Done` ends the unit's turn, so the same kind can't be re-pushed onto one parent within a tick. The
  ADR says the assertion guards the implementation, and its test is a unit test of the counter.
- `core.effect_dropped` (journal) and `effects.dropped` (counter) are new observability names, not yet in the plan's
  Global Constraints observability list.
