### Task 4: GDD, mechanic template, M1 mechanic specs, MCR changelog

**Track:** docs (protected specs as `.new`) · **Refs:** ADR-0021, ADR-0022 · **Lane 1**, alongside Task 1 (R21) · No cargo commands.

**Files:**
- Create: `docs/design/GDD.md`, `docs/design/mechanic-template.md`, `docs/design/changelog-mechanics.md`
- Create as `.new` (protected): `docs/design/mechanics/{MOVE-speed,PATH-requests,FLOW-groups,ORDER-queue,STEER-separation}.md.new`

**Content decisions:**
- **`GDD.md`** (at most 300 lines): pillars and anti-goals (spec §1–2, condensed); a glossary (pass, terrain factor, profile, clearance, cluster, flow field, cold cost, order, activity, condition, modifier, MCR, change class); the `SimTick` phase and sub-set order (spec §5.6); the **mechanic registry** table (`ID | Name | Crate | Milestone | Status | Spec | Owner track`), one row per spec below; **§Guesses** (`G-CORE-1` `rules/StateHash.interval_ticks` = 20 from spec §5.4; `G-CORE-2` `rules/Spatial.cell_m` = 32; `G-ROSTER-1` every unit's `Hull.hp`; `G-ROSTER-2` every unit's `Mobility.turn_rate_deg_s`); the `SIM_VERSION` policy, change classes and the MCR and re-bless process (ADR-0021, R146); the statistical policy (ADR-0022); the review tiers and mini-gate (ADR-0022).
- **`mechanic-template.md`**: the format `spec-lint` parses (Task 9), exactly:
  - line 1 `# <AREA.NAME> — <title>`; line 2 `status: draft|approved|implemented|deprecated · spec-version: <n> · crate: <module path> · milestone: M<n>`; line 3 `Depends on: <ids or none> · Emits journal records: <kinds or none>`;
  - sections `## Intent`, `## Rules` (lines `- <AREA.NAME.N>  <EARS sentence>`), `## Formulas`, `## Parameters` (table `| Name | Path key | Unit | Default | Valid range | Provenance | Confidence |`; a path key may use `*` for one id or path segment), `## Acceptance criteria` (lines `- AC-<n> (<rule ids or ranges>): <criterion> → <crates/…/tests/….rs>::<test fn>`), `## Non-goals · Open questions · Guesses` (guess lines `- Guess G-<AREA>-<n>: <text>`), `## Changelog`.
- **The five M1 specs** describe M1 as built (status `implemented`, spec-version 1), from spec §7, ADR-0014, ADR-0015, the M1 plan and rulings R93–R137, with 5–10 rules each. Every acceptance criterion names an existing M1 test (read the test files; never invent a name). Guesses:
  - `MOVE.SPEED` (`MOVE-speed.md`, crate `fl_sim::movement`): speed formula with the `[min_speed_frac, 1]` clamp, class blocking, slope blocking, clearance, hull turning before moving. Guesses `G-MOVE-1` min_speed_frac, `G-MOVE-2` max_slope_m, `G-MOVE-3` terrain `pass` values, `G-MOVE-4` turn_in_place_deg, `G-MOVE-5` arrive_m.
  - `PATH.QUEUE` (`PATH-requests.md`, `fl_sim::path`): FIFO, budget, cold-cost charge, stale requests at charge 0, request-order application, partial and unreachable plans, the HPA* cost bound (R136). Guesses `G-PATH-1` cluster_cells, `G-PATH-2` budget_per_tick, `G-PATH-3` max_clearance, `G-PATH-4` snap_radius_cells, `G-PATH-5` path_terrain_factor.
  - `FLOW.GROUP` (`FLOW-groups.md`, `fl_sim::nav::flow`): group threshold, one request per group, outcome-neutral cache, tick-boundary eviction (R124). Guesses `G-FLOW-1` flow_min_group, `G-FLOW-2` flow_cache_max and flow_cache_hard_max.
  - `ORDER.QUEUE` (`ORDER-queue.md`, `fl_sim::orders`): replace versus Shift-queue, the 32-order cap and `core.queue_full`, stop, rotate facing (R105), follow (R106, R107), formation slots and spacing (R110). Guesses `G-ORDER-1` formation_max_spread_m, `G-ORDER-2` follow_distance_m, `G-ORDER-3` follow_repath_ticks.
  - `STEER.SEP` (`STEER-separation.md`, `fl_sim::steer`): pair distance and push, idle cadence and cap (R113), slide order (R114), movers push idle units (R115), stranded re-plan (R116). Guesses `G-STEER-1` separation_gain, `G-STEER-2` separation_margin_m, `G-STEER-3` idle_every, `G-STEER-4` max_push_frac.
  - Parameter rows use path keys under `rules/Movement.*`, `rules/Path.*`, `rules/Steering.*`, `terrain:*/pass`, `unit:*/components.Mobility.*`; the Provenance column is the class Task 8 assigns (`Guess`, `ResearchDerived`, `OursScaled`).
- **`changelog-mechanics.md`**: the MCR entry format (ADR-0021) and an empty list; Tasks 5, 13 and 15 add MCR-000, MCR-001 and MCR-002.

- [ ] **Step 1: Write `GDD.md`, `mechanic-template.md` and `changelog-mechanics.md`.**
- [ ] **Step 2: Write the five specs as `.md.new`.** Check each against the template by hand: unique rule ids, every rule covered by an acceptance criterion, every criterion's test exists (`grep -n "fn <name>" <file>`), every guess id unique. Task 9's `spec-lint` re-checks them mechanically.
- [ ] **Step 3: Verify.** `wc -l docs/design/GDD.md` is at most 300; `git status --short` lists only the files above.
- [ ] **Step 4: Bless and approve (lead).** The lead reads the specs (status `implemented` describes built behaviour, so the lead approves under the user's delegation and lists the five specs for ratification in Task 21) and runs `cargo xtask bless docs/design/mechanics --reason "M2P Task 4: M1 mechanic specs (backfill, C1)"`.
- [ ] **Step 5: Commit** `docs(design): GDD index, mechanic template, M1 mechanic specs, MCR changelog` with trailers `Refs: ADR-0021, ADR-0022` and `Blessed: …`.

