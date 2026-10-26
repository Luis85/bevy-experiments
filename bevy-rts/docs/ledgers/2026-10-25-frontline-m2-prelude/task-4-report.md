# Task 4 report: GDD, mechanic template, M1 mechanic specs, MCR changelog

**Status:** DONE_WITH_CONCERNS (one status-word question for the controller, below).
**Commit:** `d1a906e docs(design): GDD index, mechanic template, M1 mechanic specs, MCR changelog` (trailers
`Refs: ADR-0021, ADR-0022`, `Co-Authored-By`). No `Blessed:` trailer: Step 4 (bless) is the lead's.

## What I wrote

- `docs/design/GDD.md` (192 lines, limit 300): pillars and anti-goals (spec §1–2 condensed, no original-game branding),
  glossary (pass, terrain factor, movement class, profile, clearance, cluster, flow field, cold cost, order, activity,
  condition, modifier, path key, provenance class, MCR, change class), the `SimTick` phase table with all 18 hook
  sub-sets and the M1 core systems per hook (from the system-order snapshot), the mechanic registry (5 rows, links to
  `mechanics/<file>.md`, owner track sim-core), §Guesses (G-CORE-1, G-CORE-2, G-ROSTER-1, G-ROSTER-2 in the
  `- Guess G-…:` line shape), versioning (change classes, SIM_VERSION 2→3→4, MCR, golden-policy gate rules, R146
  re-bless), statistical policy, acceptance/holdout, review tiers and mini-gate (all from the plan's ADR-0021/0022
  text; those ADRs are not in the tree yet).
- `docs/design/mechanic-template.md`: the exact format of the brief (header lines 1–3, the seven sections, rule /
  parameter / AC / guess line shapes), plus the parse details Task 9 needs: id regex, EARS patterns, `*` in path keys,
  AC ranges `AREA.NAME.a-b`, status semantics, changelog line shape, and a skeleton.
- `docs/design/changelog-mechanics.md`: the MCR entry format (Why, Requirements, Predicted effect, SIM_VERSION,
  Change-Class, Re-bless) and an empty list naming MCR-000/001/002 as to come. The format example uses `MCR-nnn`, so
  no `## MCR-[0-9]{3}` heading exists yet.
- Five protected specs as `.md.new` (spec-version 1, milestone M1, written from the code, rules.ron, the terrain and
  unit files, and the tests):
  - `MOVE-speed.md.new` (MOVE.SPEED, `fl_sim::movement`): 10 rules, 23 ACs, 12 params; G-MOVE-1..5.
  - `PATH-requests.md.new` (PATH.QUEUE, `fl_sim::path`): 10 rules, 21 ACs, 6 params; G-PATH-1..5.
  - `FLOW-groups.md.new` (FLOW.GROUP, `fl_sim::nav::flow`): 10 rules, 20 ACs, 4 params; G-FLOW-1..2.
  - `ORDER-queue.md.new` (ORDER.QUEUE, `fl_sim::orders`): 10 rules, 23 ACs, 5 params; G-ORDER-1..3.
  - `STEER-separation.md.new` (STEER.SEP, `fl_sim::steer`): 9 rules, 10 ACs, 5 params; G-STEER-1..4.
  - Parameter rows use only path keys under `rules/Movement.*`, `rules/Path.*`, `rules/Steering.*`,
    `terrain:*/pass`, `terrain:*/blocks.*`, `unit:*/components.Mobility.*` and `unit:*/components.Hull.footprint.*`,
    with the Provenance and Confidence that Task 8's classification table assigns. Mixed-class patterns are avoided
    by giving each `Mobility` field its own row (`class` ResearchDerived, `road_speed_kmh`/`terrain_factor`
    OursScaled, `turn_rate_deg_s` Guess).
  - Shared parameters (`Path.max_clearance`, `Movement.min_speed_frac`, `Movement.arrive_m`,
    `Steering.separation_margin_m`, `Path.cluster_cells`) appear in more than one spec's table; each guess id is
    defined once only. Dependencies are acyclic: MOVE ← PATH, STEER; ORDER ← PATH, MOVE, STEER; FLOW ← PATH, MOVE,
    ORDER.

## Verification (no cargo, as required)

A throwaway Python checker (scratchpad, not committed) parsed the five specs and the GDD against the template:

```
FLOW-groups.md.new: FLOW.GROUP, 10 rules, 20 ACs, 4 params
MOVE-speed.md.new: MOVE.SPEED, 10 rules, 23 ACs, 12 params
ORDER-queue.md.new: ORDER.QUEUE, 10 rules, 23 ACs, 5 params
PATH-requests.md.new: PATH.QUEUE, 10 rules, 21 ACs, 6 params
STEER-separation.md.new: STEER.SEP, 9 rules, 10 ACs, 5 params
guesses: G-CORE-1..2, G-FLOW-1..2, G-MOVE-1..5, G-ORDER-1..3, G-PATH-1..5, G-ROSTER-1..2, G-STEER-1..4 (23, each defined once)
registry: MOVE-speed.md, PATH-requests.md, FLOW-groups.md, ORDER-queue.md, STEER-separation.md
OK: no errors
```

It checked line 1–3 shapes, the seven section headings in order, rule-id shape, uniqueness and spec prefix, 5–10
rules per spec, AC line shape and numbering, that every rule is covered by an AC, that every AC's file exists and
contains `fn <test>`, the 7-column parameter rows with a valid Provenance and Confidence, guess-id uniqueness across
specs and GDD, and that the GDD registry links every spec. I also read the body of every cited test to make each
criterion say what the test actually asserts. `wc -l docs/design/GDD.md` = 192. `git show --stat HEAD` lists only
the 8 files under `docs/design/`.

## Concerns

1. **Status word.** The brief says status `implemented` (built behaviour); your dispatch says
   `status: approved (lead, R148)`. I followed the dispatch: line 2 reads `status: approved (lead, R148) · …` and the
   GDD registry's Status column says the same. To keep it parseable the template allows an optional
   `(<approver>, <ruling>)` note after the state word. If you want `implemented` instead, it is a one-word change in
   the five line-2s, the five GDD rows and nothing else (both states are traced, so Task 9's behaviour is the same).
2. **Test gaps found while backfilling** (listed as open questions in the specs, candidates for the test-author;
   no AC claims more than its test shows): MOVE.SPEED.8's `arrive_m`-but-not-a-step branch; terrain `pass` range
   rejection; a unit reaching `Failed` through the queue (PATH.QUEUE.6) and the `failed` pop (ORDER.QUEUE.4); the
   step-candidate order of R114 (STEER.SEP.6) and the push cap itself (STEER.SEP.4).
3. **Code vs spec §7 difference noted, not changed:** M1 applies turn-in-place (MOVE.SPEED.6) to every class, Foot
   included; spec §7 names only tracked and wheeled units. Recorded as an open question in MOVE.SPEED.
4. `Path.snap_radius_cells` has no link-pass range check in M1 (written as "any u32 (not range-checked in M1)").
   R104 asks every content value reaching arithmetic to be range-checked; flagging for whoever owns `PathRules`.
5. Guess texts state what each value is and what would replace it; rationales are only what the code and data
   show (no invented sources). ADR-0021/0022 were not in the tree when I wrote, so the GDD's §7–§9 follow the plan's
   Task 3 text for them; if Task 3's wording differs, the GDD should follow the ADRs.

## Files

- `C:\fl\a\docs\design\GDD.md`
- `C:\fl\a\docs\design\mechanic-template.md`
- `C:\fl\a\docs\design\changelog-mechanics.md`
- `C:\fl\a\docs\design\mechanics\{MOVE-speed,PATH-requests,FLOW-groups,ORDER-queue,STEER-separation}.md.new`

## Fix round 1

Against `task-4-review.md` (Important 1, Minors 1–6; Minor 7 left as asked). Docs only, no cargo.

- **Important 1.** MOVE.SPEED.6 now says the sim "shall take no drive step" while the unit turns in place, and
  that its separation push still applies (STEER.SEP.7); the `aim` formula says the same.
- **Minor 1.** The template gains an **open** acceptance criterion, `- AC-<n> (<ids>): <criterion> → open: <why>`:
  spec-lint accepts it, it names no test, so no `@req` tag is backfilled and `trace` reports the rule untraced
  (an approved spec then fails `trace` until a test lands). STEER.SEP: AC-6 no longer lists the cap; the push cap
  (STEER.SEP.4) and the candidate order (now STEER.SEP.7) have open criteria. The same rule applied where a split
  left a branch no test isolates: MOVE.SPEED.9 (`arrive_m` stop), FLOW.GROUP.11 and .12 (the two arrival
  branches), ORDER.QUEUE.12 (follower with an `Idle` path plans in the same tick). Each spec's open questions say so.
- **Minor 2.** STEER.SEP AC-11 (was AC-8): "`Idle` with no corners by tick 20".
- **Minor 3.** One behaviour per rule, subject "the sim shall", renumbered with every reference updated:
  - MOVE.SPEED: .8 → .8 corner advance, .9 `arrive_m` stop; old .9 → .10 map clamp, .11 open ground without a grid;
    old .10 → .12 (12 rules).
  - FLOW.GROUP: old .1 → .1 group, .2 singles below the threshold, .3 no group when queued; old .7 → .9 look-ahead,
    .10 never off the field, .11 arrival with sight, .12 arrival without; others shifted (15 rules).
  - ORDER.QUEUE: old .1 → .1 replace, .2 append, .3 journal, .4 clamp (R104); old .6 → .9 offset, .10
    `core.invalid_target`, .11 self dropped; old .7 → .12 `Idle` plans now, .13 no re-plan while waiting or before
    the cadence, .14 drift thresholds; others shifted (17 rules).
  - STEER.SEP: old .6 → .6 stay when nothing fits, .7 candidate order, .8 `move.blocked`; others shifted (11 rules).
  - Cross-references updated: MOVE.SPEED.6 → STEER.SEP.7, G-STEER-2 → ORDER.QUEUE.17, ORDER open question → .7.
- **Minor 4.** The template lists the Valid range forms (interval or bound, optionally `and` one more bound; map or
  sequence `… for each entry; one per …`; enum list; `any <type>`). Every row now uses one; `snap_radius_cells` is
  `any \`u32\`` with the no-check note moved below the table.
- **Minor 5.** The template states a key may appear in several specs' tables, owned by one; copies write
  `<name> (owned by <SPEC.ID>)` and agree with the owner in every column. Marked: `max_clearance` (PATH.QUEUE) and
  footprint width (STEER.SEP) in MOVE; `min_speed_frac` (MOVE.SPEED) in PATH; `cluster_cells` (PATH.QUEUE) in FLOW;
  `arrive_m` (MOVE.SPEED) and `separation_margin_m` (STEER.SEP) in ORDER. MOVE's `min_speed_frac` range now equals
  PATH's copy exactly.
- **Minor 6.** GDD §7 re-bless now follows ADR-0021: refuses (exit 1, writing nothing) without the heading or on a
  failing semantic check; writes `<name>.replay.new` for changed replays and `target/rebless-report.md` with the
  first divergent tick, journal summary diff, entity count before and after, and semantic results. §9 mini-gate
  follows ADR-0022 and prints `mini-gate MOVE.SPEED: PASS`.

**Verification.** The scratchpad checker (extended to accept open criteria) passes every check on all five specs
(line shapes, sections, unique prefixed ids, every rule covered, every named test's `fn` exists, parameter rows,
unique guess ids, GDD registry) except my own 5–10 rules limit: MOVE 12, FLOW 15, ORDER 17, STEER 11, PATH 10.
Open criteria: MOVE.SPEED.9, FLOW.GROUP.11–12, ORDER.QUEUE.12, STEER.SEP.4, STEER.SEP.7. A grep of every rule-id
mention outside rule and AC lines shows no stale number. `GDD.md` 194 lines.

**Concerns.**
1. The brief's "5–10 rules each" no longer holds for four specs, because of the split you asked for.
2. Six rules now have open criteria, so Task 9's expected `trace: <n> rules, 0 untraced` will report 6 untraced
   (and fail for these approved specs) until the test-author adds those tests or Task 9 scopes them out.
