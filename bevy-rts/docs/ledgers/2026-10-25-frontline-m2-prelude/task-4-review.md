# Task 4 review (d1a906e)

### Spec Compliance

- ✅ Spec compliant. All 8 files present (GDD 192 lines, template, changelog, five `.md.new`); header/section/line shapes match the brief; guess ids G-CORE-1..2, G-ROSTER-1..2, G-MOVE-1..5, G-PATH-1..5, G-FLOW-1..2, G-ORDER-1..3, G-STEER-1..4 all defined once; rule counts 9-10 per spec (brief: 5-10 — within); all 97 AC test names resolve (`fn <name>` grep over every AC, 0 missing).
- Deviation from brief, accepted: status word is `approved (lead, R148)` not `implemented` (R150 in progress.md); template allows the note.
- ⚠️ Cannot verify from diff: parameter Provenance/Confidence columns vs the Task 8 ledger (`Guess`/`ResearchDerived`/`OursScaled` per key) and `spec-lint` parsing of `(lead, R148)` and of `*` in `terrain:*/blocks.*`; the registry links `.md` files that exist only as `.md.new` until bless. Task 9 must be checked against these.

### Strengths

- Formulas verified against code and data: `speed_factor` operation order (grid.rs:85), km/h conversion (units.rs:22-35), clearance recurrence and `passable` anchor (grid.rs:256-263, 332-342), `max_step` 4-neighbour slope rule, edge cost / octile / cell cost (astar.rs:53-82, grid.rs:91), `nearest_passable` 8r probe count (astar.rs:375), R136 bound constant 16 = 2*7+2 (hpa.rs:17-25), budget loop with at-least-one (path.rs:479-487), stale at charge 0 (path.rs:517-531), group key/charge/singles (path.rs:591-671), eviction order and hard max (flow.rs:288-312), steering push/cap/coincident rule (steer.rs:166-231), step candidates and BLOCKED_REPLAN_TICKS (movement.rs:307, 373-405, 610-615), follow offset/goal/re-plan rule (commands.rs:775-835, orders.rs:290-318), `formation_targets`/`space_slots` (orders.rs:327-427), queue cap and reject order (commands.rs:561-569). Defaults and ranges all match `assets/base/rules.ron`, the eight `terrain/*.terrain.ron` files (pass, blocks) and the unit roster (speed 5-60, turn 55-180, factor 0.35-0.8, footprint 0.8-3.6 wide). GDD hook list (18 hooks, system placement) matches schedule.rs/builder registrations; SIM_VERSION 2 matches build.rs:24.
- ≥10 test bodies read against their AC text (kmh conversion, move_command_reaches_target tick 200, truck_slows_down_off_road, hull_rotates_before_moving, no_request_starves bound, flow_units_reach_their_slots, space_slots_pinned, formation_targets_pinned, step_never_cuts_a_blocked_corner, mover_pushes_an_idle_unit_aside): all say what the AC says, except the points below.
- Honest open-questions list for untested branches; GDD §7-§9 agree with ADR-0021/0022 on classes, SIM_VERSION 2→3→4, gate rows, rebless refusal, Wilson alpha, seeds, tiers P/A/B/C, holdout 10-20 %. No Blitzkrieg values or branding copied.

### Issues

#### Critical (Must Fix)
None.

#### Important (Should Fix)
1. MOVE-speed.md.new MOVE.SPEED.6: "shall leave the unit where it is (it turns in place)" is not what the code does. While turning, `step_delta` is zero but `take_step` still applies the separation push (movement.rs:586-606, candidates `whole = step + push`), so a turning unit is pushed. Reword to "shall take no drive step" (and say the push still applies, STEER.SEP.6).

#### Minor (Nice to Have)
1. STEER-separation.md.new AC-6 lists STEER.SEP.4 (push cap), but `mover_pushes_an_idle_unit_aside` (steering.rs:392) never measures the cap; it only shows an idle tank moved >1 m. The same holds for STEER.SEP.6's candidate order (AC-4/5). Both are disclosed as open questions, but the AC mapping makes `trace` report them covered. Prefer a dedicated test (Task 15) and drop STEER.SEP.4 from AC-6 once it exists.
2. STEER-separation.md.new AC-8 says "dropped to Idle after 10 ticks"; `fast_unit_never_hops_a_one_cell_wall` (steering.rs:455-479) asserts only `Idle` after 20 ticks, not the 10-tick threshold (`blocked_step_replans_with_backoff` covers the cadence).
3. Non-atomic rules weaken per-rule tracing: ORDER.QUEUE.1, .6, .7; FLOW.GROUP.1, .7; MOVE.SPEED.8 each bundle 2-4 behaviours in one sentence. ORDER.QUEUE.6 and .7 break the "the sim shall" EARS subject ("the command shall be rejected").
4. mechanic-template.md says Valid range is interval notation, but rows use prose (`≥ 0, one entry per class`, `Foot, Wheeled, ...`, `any u32 (not range-checked in M1)`). Say free text is allowed for enum/map keys, or Task 9 lint may choke.
5. Same path key repeated across specs (`Path.max_clearance` MOVE+PATH, `Movement.arrive_m` MOVE+ORDER, `Steering.separation_margin_m` ORDER+STEER, `Movement.min_speed_frac` MOVE+PATH, `Path.cluster_cells` PATH+FLOW); the template does not say this is allowed or that Provenance/Valid range must agree across copies. State it so spec-lint does not flag duplicates.
6. GDD.md:215-217 states the mini-gate prints `PASS (<n> tests)`; ADR-0022 gives only `mini-gate MOVE.SPEED: PASS`. GDD.md:174-178 lists the rebless report as "first divergent tick, journal summary diff, outcome diff"; ADR-0021 says first divergent tick, journal summary diff, entity count before/after, semantic results. Align wording with the ADRs.
7. GDD.md:40 cites `docs/research/blitzkrieg-mechanics.md` by path while §2 forbids the original name. The file exists, so this is only a naming-consistency point.
8. Registry (GDD.md:129-133) links `mechanics/*.md`, which exist only as `.md.new` until the lead blesses; fine if bless lands in the same commit, otherwise spec-lint's "registry lists exactly the spec files" fails in between.

### Assessment

**Task quality:** Approved (after fixing Important 1 as a one-line wording change; the rest can ride with Task 9/15)

**Reasoning:** Formulas, defaults, ranges and 97 test references check out against the code, rules.ron and terrain data, and the GDD matches ADR-0021/0022. One rule (MOVE.SPEED.6) misdescribes the push during a turn, and a few ACs claim slightly more than their tests assert.
