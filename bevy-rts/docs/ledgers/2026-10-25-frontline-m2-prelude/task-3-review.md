# Task 3 review: ADR-0017–0022, refinements, spec revision 2.5 (commit 510f0ca)

Reviewed as design: these texts become binding contracts for Tasks 5–21 and M2+. One pass over `review-t3.diff`
(1,295 lines, 12 files). File:line references are to the committed files (`docs/adr/00NN-*.md`, the spec).

### Spec Compliance

- ✅ Spec compliant. All six ADRs exist with MADR 4.0 front matter, `status: "proposed"` and every required section
  (Context, Decision Drivers, at least three Considered Options with pros and cons, Decision Outcome, Consequences,
  Confirmation, More Information). Each records the decisions in the brief. The four `## Refinements (2026-10-25, M2
  prelude)` sections, the six README rows plus the four refinement rows, and every spec edit are present: status line
  (spec:3–4), Revision 2.5 above 2.4 (spec:79–138), §5.4 observer line, §6.1 last line, §9.2 delivery line, §12.5
  R146. The diff has exactly the 12 files in the brief.
- Extras, all harmless and in line with the plan: ADR-0015 "One path reset (M12, M13)", ADR-0007 "Order kinds by
  name", ADR-0006 "New state of the prelude". The R149 additions (`MAX_MODIFIERS_PER_STAT`, a TransientKept dirty
  set read on the spot, `min ≥ −10⁶`, `Override` in `[min, max]`, the R/E golden exemption, observers still
  rejected) are recorded as ruled.
- ⚠️ Plan sync, which the controller must amend before dispatching these tasks (the plan text now disagrees with the
  ADRs it implements):
  - **Task 17** (plan:1185–1186, 1167): it still names a **TransientReset** `"core.stats_dirty"`. Its
    `effective_stat` is still "the cache, or the clamped base when not cached", with no on-the-spot evaluation for
    marked units (R149). `StatDef` states only `max ≤ 1,000,000`. `extreme_modifiers_stay_in_bounds` (plan:1193)
    still needs the note that it calls the pure `evaluate`.
  - **Task 5** (plan:358): `class_violations(commits)` has no input that carries the replay-format exemption, and
    `refactor_class_cannot_change_goldens` (plan:383) still says that every R commit touching a `.replay` is a
    violation (R149).
  - **Tasks 13 and 17** both put their system "first in `ThinkSet::Orders`" (plan:927, 1185). See Minor 1.
  - **Task 8**: whether fixture mods (`crates/fl_sim/tests/fixtures/**`, the `fl_cli` fixtures) need ledgers, or are
    exempt from the "every tunable leaf" rule, can't be told from these texts.

### Strengths

- Names and values checked against the code all hold:
  - `RejectReason::{NoValidUnits, QueueFull, Feature}` with codes `core.no_valid_units` and `core.queue_full`;
    `MAX_COMMAND_UNITS` = 256 and `MAX_WAYPOINTS` = 32 (`crates/fl_sim/src/commands.rs:102–160`).
  - `fl_client::orders::MAX_COMMAND_UNITS` (`orders.rs:26`); `BLOCKED_REPLAN_TICKS` = 10 (`movement.rs:307`);
    `rebuild_core` (`nav/mod.rs:161`).
  - `SimReply::Error(String)`, and the four enums already `#[non_exhaustive]` while `FromRunner` is not
    (`fl_view/src/runner.rs:36–143`).
  - `Codec::{custom, migrate_from, fill_missing}` and `LoadCtx::{remap, saved_tables, current_tables}`; the classify
    tests `observers_…` and `relationships_are_not_allowed_in_the_sim_world` (`tests/classify.rs:181, 192`).
  - The `ResolveSet::{Projectiles, Effects, Stats}` and `LifecycleSet::{Death, Spawn, Despawn}` order
    (`schedule.rs:120–150`).
- About 75 Confirmation test names in ADR-0017, 0018, 0020, 0021 and 0022, checked by grep against the plan: all
  exist.
- The overflow arithmetic in ADR-0017:73–77 is correct for `Fx` = I32F32. Its range is ±2³¹ ≈ 2.147 × 10⁹
  (`fl_math/src/fx.rs`); 10⁶ × 1,601 ≈ 1.6 × 10⁹ fits; `fixed`'s multiply widens before it narrows.
- The implementer's TransientKept finding (report concern 2) is right and important. Under R72 housekeeping, a mark
  made in `Lifecycle` would be lost, the cache would go stale, and a save/load would then desync from the live run.
  On-the-spot evaluation gives the cache R84 semantics.
- Hash neutrality for class N holds by construction: `compute_state_hash` writes nothing for absent components
  (`hash.rs:243–253`). So `Conditions` being removed at count 0 (ADR-0017:49–52) keeps Task 17 hash-neutral.
- ADR-0018's load-order reasoning for `Related<R>` (a source loads before its target, so the framework rebuilds it
  before feature rebuilds) and the M1-save path (`migrate_with` plus the `activity_stack` `fill_missing`) are
  coherent with `LoadCtx` as it exists.
- ADR-0020 explains why `ron::Value` fails (`handle_any_struct` turns a bare identifier into unit), and it gates the
  new tree on byte-identical rules hashes.

### Issues

#### Critical (Must Fix)

None.

#### Important (Should Fix)

1. **ADR-0017:68, 73 — the `AddPct` factor can go negative.** Each `AddPct` is bounded to `[−1, 100]`, but the step
   multiplies by `1 + Σ add_pct`. Two −60 % slows give a factor of −0.2, which flips the sign. On a stat with
   `min < 0`, the clamp then keeps a sign-flipped value, and on `move.speed` two slows read as "stopped". The
   contract is "fixed for good" (Consequences), so fix it now: `v *= max(0, 1 + Σ add_pct)`. Add a test with two −0.6
   modifiers.
2. **ADR-0017:71 — the order within a bucket is not total, so evaluation is not order-independent.**
   - Two modifiers with the same `(key, unit)` (one source adding two `Mul`s, or two `Override`s) fall back to their
     position in `Modifiers.list`. `Mul` rounding and "the last override wins" then depend on insertion history and
     on how `remove_modifiers_from` compacts the list. Today that history is hashed and preserved by the codec, but a
     later canonicalising codec or a `swap_remove` would silently change outcomes.
   - The comparison of `key` is not stated either: string bytes or a registry id. A dense id would let renumbering
     change outcomes.
   - Fix: define a total order, `(key bytes, unit with None first, op, value raw)`, or keep `Modifiers.list` sorted
     by it on insert, and say `evaluate` sorts its input.
3. **ADR-0017:78–85 — "a read never observes a stale value" rests on "every modifier or base change marks the
   unit", which nothing can enforce.**
   - `base` is an arbitrary `fn(EntityRef) -> Option<Fx>` over any component (here `Mobility.speed`), and a feature
     that writes that component directly (an M2 engine-damage rule, say) doesn't mark. A modified unit then reads a
     stale cache in the live run but a fresh value after a load (`rebuild_core`): a branch-test desync.
   - A value reload is covered, because `apply_content_values` runs `rebuild_derived` (`reload.rs:131`).
   - Fix: cache `(base, effective)` and re-read `base` on every `effective_stat`, evaluating when it differs (cheap,
     and it makes the cache pure). Or rule that stat base components change only at spawn, through a marking API, or
     by reload.
4. **ADR-0017:100–102 and ADR-0019:41–45 — late `EffectRequest`s are silently dropped.** Effects apply in
   `ResolveSet::Effects`, and messages are cleared at the end of the tick (R72). A request emitted later than the
   applier in that tick is never applied, with no error. Later emitters include the feature slot of `Effects`,
   `ResolveSet::Stats`, `LifecycleSet::Death` (a death explosion or ammo cook-off in M2), and an effect's own
   `apply` (chained effects). Fix: state the emission window (writers run before `ResolveSet::Effects`' core slot,
   or the applier drains until empty for chains, with a bound), say what a later emitter does (Hashed pending state
   for the next tick), and add a debug check that the buffer is empty at the clear.
5. **ADR-0018:57–59, 64–70 — the activity stepping rule is under-specified for feature kinds.** Four points:
   - **A pushed child:** does it tick in the same tick as the `Push`? That changes timing by one tick.
   - **A child that ends:** after it returns `Done`, is the parent ticked again in the same tick? "Pops done frames
     until a frame continues" implies yes.
   - **No bound on steps:** nothing limits steps per unit per tick. A parent that re-pushes a child that finishes at
     once ("push drive if out of range"; the drive is `Done` immediately) loops forever inside one tick. That is a
     deterministic freeze, which the depth cap of 8 does not stop.
   - **The `Canceling` state:** its meaning is unclear when `give_order` "cancels every frame top-down … and begins
     the new one at once". Either cancel is immediate (then `Canceling` is unused) or frames stay to finish
     cancelling (then the new root sits above them).
   - Fix: define these rules, plus a per-unit step budget per tick (exceeding it fails the order). The core kinds'
     answers are already fixed by the pose trace, so this only writes them down.
6. **ADR-0021:59–60, 80–81 — the R/E golden exemption is branch-scoped and only half checked.**
   - "unless the branch changes the replay format": once Task 5 lands on `m2p/phase-a`, every later R or E commit on
     that branch (Tasks 6, 7, 10, 16 …) may change golden bytes unflagged until `main` is fast-forwarded. The
     condition "the re-bless report shows every hash stream identical" is not checked by any row.
   - Plan Task 5's `class_violations(commits)` has no input that could carry the condition.
   - Fix: scope it per commit (that same commit changes `REPLAY_FORMAT_VERSION`), and have `golden-policy` compare
     the old and new goldens' hash streams (`Replay::from_bytes` reads format 3 and 4) when the class is R or E.
7. **ADR-0020:78–82 — the stale-ledger rule breaks under patches.** "An entry naming no tunable leaf of M is an
   error" is judged against final origins. Once mod B patches a leaf that `base` set, the leaf's origin is B, so
   `base`'s own ledger entry becomes "stale provenance key". `base`'s `data-check` then fails whenever B is loaded,
   which is exactly the case Review Focus 2 and `later_mod_patch_wins_and_owns_provenance` exercise. Fix: judge
   staleness against the leaves of M's own pre-patch documents, and judge coverage ("no provenance for") against the
   final origins.
8. **ADR-0020:44–66, 101 — whole-object `override` has no place in the phase list.**
   - The existing `is_override` (`fl_content/src/object.rs:26`) still works, but nothing says when it resolves
     relative to patch and inherit.
   - Undefined: which document a patch targets when a later mod overrides the object, whether an earlier mod's
     patches survive the override, and what origins an overridden object's leaves get.
   - This is the "two mods touching one unit" contract. Fix: place override resolution explicitly (say, after parse
     and before patch, so patches apply to the winning document), and state the origins.
9. **ADR-0022:56–63, 84–85, 51–53 — holdout secrecy leaks through the tools this ADR defines.**
   - A `guard-read` on `Read`/`Grep`/`Glob` paths can't stop a `Grep` or `Glob` rooted at an ancestor (`crates/`),
     whose results include holdout files. Nor can it stop Bash `rg`/`cat`/`git show`.
   - `cargo xtask mini-gate` runs holdout tests and prints their names and assertion output to whoever runs it.
   - `cargo xtask trace` writes holdout test names to `target/trace.md`.
   - The ADR admits the hook is not mechanical, but these leaks come from its own tooling. Fix:
     - add an `.ignore`/`.rgignore` with `**/tests/holdout/`, so ripgrep-backed `Grep`/`Glob` skip it;
     - extend the Bash guard to holdout paths;
     - let implementer roles run `mini-gate` without holdout (or only see its pass or fail);
     - redact holdout names from `trace` output for them.

#### Minor (Nice to Have)

1. **ADR-0017:80 vs ADR-0018:65.** Both `recompute_stats` and `run_activities` are "first" in `ThinkSet::Orders`. On-
   the-spot evaluation makes the order outcome-neutral, but name it (`recompute_stats`, then `run_activities`).
2. **ADR-0017:63–64.** Say that `Modifiers` is removed when its list empties, as `Conditions` is (49–52). Otherwise a
   once-modified unit hashes differently from its twin ("absent state writes nothing").
3. **ADR-0017:74 — `MAX_MODIFIERS_PER_STAT` (R149).** `add_modifier`'s `Err` at the 17th modifier has no defined
   caller behaviour (`EffectVerb::apply` returns `()`, so the modifier is silently dropped). Evaluating the buckets on
   the raw `i128` and clamping before narrowing (the implementer's own alternative) removes the cap and the failure
   mode. The controller decides, since R149 ruled the cap.
4. **ADR-0017:49–52 — the `ConditionToken`.** As plain `(unit, condition)` data, a double revoke silently cancels
   another feature's grant. A token kept in Hashed state needs its condition saved by name (say so).
5. **ADR-0017:66 — `Override`.** It is "replace the base", not "force the final value": `Override 0` plus any `Add`
   still moves. For M2 track breaks, `Mul 0` is the right tool; a note keeps features from misusing `Override`. "The
   last override wins" by lexicographic key is arbitrary but deterministic.
6. **M2 direct-fire expressibility.** It is mostly carried: orders `core.attack`/`attack_ground`/`stand_ground` as
   kinds with `Unit`/`Point`/`None` targets; "move into range, then fire" as the stack; track break as condition
   plus `Mul 0`; wrecks through the funnel with sources stripped; ammo and the firing cycle as Hashed feature state;
   armour facing and penetration as def components or stats in bounds (mm, HP and metres are all ≤ 10⁶). Gaps worth
   one line each in ADR-0017 and ADR-0018:
   - **Per-weapon stats:** unit scope with code-registered stat names can't hold them for multi-weapon units. Name
     the pattern: unit-level factor stats applied by the weapon system, or weapon sub-entities with a parent scope.
   - **Concurrent weapons:** turret tracking and firing on the move run alongside the hull's activity, so they live
     in feature systems outside the single stack, not in it (the "composite orders" consequence at ADR-0018:105
     overstates this).
   - **Effect payload:** `EffectRequest` has no dynamic payload (impact point, angle, shell), so damage and
     penetration travel as combat messages (ADR-0019), not as effects.
7. **ADR-0018:46–47, 55–56.** `MAX_WAYPOINTS` is justified because it bounds the save decoder, yet
   `QueuedOrder.param` and `Activity.param: Vec<u8>` are unbounded. Add `MAX_ORDER_PARAM_BYTES` (checked at handler
   entry and in the codec). Also state that `ActivityKindId` is saved and hashed by name, as `OrderKindId` is.
8. **ADR-0019:48–51.** "Push into a buffer that its consumer sorts" contradicts "never writes a resource". Name the
   allowed buffer (a Message whose reader sorts, or a named Transient resource).
9. **ADR-0021:53–60, 64–67.**
   - Every class D change alters the rules hash in every base-mod golden's header (`check_replay` requires an equal
     `rules_hash`, `fl_protocol/src/header.rs:71–85`). So D commits always re-bless both goldens and need an `MCR:`
     under `golden-policy`, but the MCR section speaks only of F. Say so.
   - `needs_class` (crates, features, assets) misses root `Cargo.toml`/`Cargo.lock` (class E) and root `scenarios/**`,
     and `content_changed` is undefined.
10. **ADR-0022:64–69.** A reroll to `seed_base + 1` is the next night's seed set, so a reroll that passes today
    guarantees that tomorrow's run repeats it. Reroll to a disjoint base (for example `seed_base + 100_000`).
11. **ADR-0020:53–60.**
    - A `Set` on a field of a component that a child only inherits fails ("the path must exist" before inheritance).
      Document that a mod patches the template, or `Add`s the whole component on the child.
    - Define "file name order" as a byte-wise order of the mod-relative path, independent of the OS.
12. **MADR polish.** The chosen-option titles don't match the Considered Options titles in all six ADRs (for
    example ADR-0018:40 vs 34).
13. **Spec revision 2.5, ADR-0018 bullet (spec:97–103).** §9.2 C7 lists "ownership" among the custom relationships,
    while ADR-0018 keeps `Owner(PlayerId)`. Revision 2.5 overrides older text only by what it says, so the bullet
    should state that ownership stays `Owner(PlayerId)`.

### Checks run (read-only)

Each named risk got one focused check:

- **Fx range and multiply:** `crates/fl_math/src/fx.rs`.
- **Hash neutrality of absent components:** `crates/fl_sim/src/hash.rs:243–330`.
- **`Class::TransientKept` exists for resources only:** `classify.rs:49–70`.
- **Hook order and the dropped-message window:** `schedule.rs:100–150`.
- **Reload rebuilds derived state:** `reload.rs:110–170`.
- **The header requires equal sim version and rules hash:** `header.rs:67–94`.
- **Runner enums and `SimReply::Error`:** `fl_view/src/runner.rs`.
- **Codes and constants:** `commands.rs:95–165`, `movement.rs:307`.
- **Codec and `LoadCtx` API:** `classify.rs:97–226`, `save/format.rs:61–110`.
- **Manifest and override fields:** `fl_content/src/{mods,object}.rs`.
- **Classify test names:** `tests/classify.rs:181, 192`.
- **Every Confirmation test name against the plan:** a grep loop.
- **Plan Tasks 5, 7, 11–17 interfaces against the ADR text.**
- **No cargo:** docs only, so no tests apply.

### Assessment

**Task quality:** Needs fixes

**Reasoning:** The ADRs record every decision in the brief, the names match the code, and the overflow and
dirty-set reasoning is sound. But several binding rules have gaps that later tasks would turn into code or that M2
would trip over: the `AddPct` sign flip, the non-total modifier order, base-change staleness, dropped late effects,
the activity stepping rules, the branch-scoped golden exemption, stale ledgers under patches, the place of
`override`, and holdout leaks through `mini-gate` and `trace`. Each fix is a few sentences in the ADR, plus the plan
amendments listed under ⚠️.
