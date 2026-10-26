# Frontline — game design document

The index of Frontline's game design: what the game is for, the words its specs use, the order a sim tick runs in,
and the registry of mechanic specs. The design source is the spec (`docs/superpowers/specs/2026-10-23-frontline-rts-design.md`,
"the spec"); the decisions are the ADRs (`docs/adr/`). Each mechanic has one spec under `docs/design/mechanics/`, in the
format of `docs/design/mechanic-template.md`, checked by `cargo xtask spec-lint` and traced to tests by
`cargo xtask trace` (ADR-0022). Changes to rules go through `docs/design/changelog-mechanics.md` (ADR-0021).

## 1. Pillars

Condensed from spec §1–2.

- **A faithful WWII tactics base.** Iteration 1 reproduces the mechanics of the classic WWII RTS genre (terrain
  speed and blocking, firing cycles, hit ellipses, armour sides, penetration, supply, vision) from the rules written
  down in `docs/research/blitzkrieg-mechanics.md`, in milestones (spec §9). Later iterations add campaigns, tech
  trees, base building and colony management on top.
- **Extensible by construction.** Units, weapons, factions and maps are data (RON). Mechanics are plugins: a new one
  is a new feature crate pair, registered in one line, and never an edit to a central enum.
- **Deterministic and lockstep-ready.** A fixed 20 Hz tick whose only input is player commands, with `Fx`
  fixed-point and integer state, keyed randomness and ordered iteration, so replays, saves and lockstep multiplayer
  work without a rewrite.
- **Large scale.** 3,000 units (8–12k sim entities plus projectiles), 4 players, a 2 × 2 km map, at ≤ 15 ms per
  tick and ≥ 60 fps (G7). Scale is a design input, not a late optimisation.
- **A testbed first.** The first playable build is a skirmish map with debug tools (spawn, switch faction,
  overlays, pause and step, speed, hot reload), and every mechanic is built, tuned and tested on it.
- **Headless and agent-debuggable.** The sim runs with no window or GPU; journal records, counters, spans and debug
  queries say what happened and why (spec §15).

## 2. Anti-goals

Any of these is a failure even if the game works:

- Adding a mechanic means editing core crates or a central enum; unit types become a closed Rust enum.
- Sim logic reads render or input state, wall-clock time or floats, or iterates in an unordered way.
- Input or debug code changes unit state directly instead of issuing commands.
- Genre-specific assumptions are baked into the core ("no building produces units", "the only resource is supply").
- The original game's name or branding is used, or its code or data files are copied. Our values are our own:
  every one carries a provenance class (§6).

## 3. Glossary

- **Pass** (`TerrainDef.pass`, in [0, 1]): the fraction of road speed a terrain kind leaves a unit that escapes none
  of its slowdown. Roads and bridges pass 1; water and swamp pass 0 and block every class.
- **Terrain factor** (`Mobility.terrain_factor`, in [−1, 1]): how much of a terrain's slowdown a unit escapes; its
  speed fraction on a cell is `clamp(pass + (1 − pass) × terrain_factor, Movement.min_speed_frac, 1)`. Negative
  values slow a unit below the terrain's pass. Path costs use the per-class `Movement.path_terrain_factor` instead.
- **Movement class**: `Foot`, `Wheeled`, `HalfTrack` or `Tracked` (a closed enum, ADR-0014). Terrain blocking and
  slope limits are per class.
- **Profile** (`MoveProfile`): a movement class plus a size in cells, `ceil(footprint width / cell)` clamped to
  `1..=Path.max_clearance`. Navigation data (blocking, clearance, HPA* graph, flow fields) is kept per profile.
- **Clearance**: per class and cell, the largest `k ≤ Path.max_clearance` for which the `k × k` square anchored at
  the cell (toward +x and +y) is unblocked. A unit of size `s` may stand where the clearance at its anchor is ≥ `s`.
- **Cluster**: one `Path.cluster_cells`-square block of the grid in HPA*. Entrances on cluster borders are the
  abstract graph's nodes; a terrain change rebuilds only the clusters it touches.
- **Flow field**: for one destination cluster and one profile, every abstract node's cost to that cluster plus
  lazily built per-cluster tiles of cell directions. A group move of `Path.flow_min_group` or more same-profile
  units follows one field instead of planning one path each.
- **Cold cost**: the work a from-scratch computation of a path or group request does (cells and abstract nodes
  settled, snap probes, line-of-sight cells). Every request is charged its cold cost against
  `Path.budget_per_tick`, whatever a cache held, so caches never change outcomes (ADR-0015).
- **Order**: a player's instruction held on a unit (move, rotate, follow; Shift-queued up to 32). M1 keeps them in
  the closed `Order` enum of `MoveOrder`; the M2 prelude replaces it with an order queue of registered kinds
  (ADR-0018).
- **Activity**: one frame of a unit's activity stack, the step it is executing for its current order (drive, turn,
  follow); an order becomes current by pushing its root activity (ADR-0018, from Task 13).
- **Condition**: a named, counted token granted to a unit and tested by `requires` expressions (ADR-0017).
- **Modifier**: `(stat, op, value, source)` changing a unit's effective stat, applied in one fixed stacking order
  with clamps (ADR-0017).
- **Path key**: the name of one content value, `<doc>/<field path>` (`rules/Movement.arrive_m`,
  `terrain:base.forest/pass`); `*` in a spec's parameter table stands for one id or segment (ADR-0020).
- **Provenance class**: where a content value comes from: `Original`, `Manual`, `ResearchDerived`, `OursScaled`,
  `Guess` or `Tuned`, recorded per path key in the provenance ledger (C2, ADR-0020).
- **MCR** (mechanic change request): the `## MCR-nnn` entry in `docs/design/changelog-mechanics.md` that explains a
  rule change and predicts its effect before the code is written (ADR-0021).
- **Change class**: the `Change-Class:` commit trailer, `R|D|F|N|E|X` (§7).

## 4. The sim tick

`SimTick` runs eight phases in a fixed order, each split into hook sub-sets (spec §5.6, ADR-0005), 18 in all. Each
hook has a core slot, which runs first, and a feature slot after it; each phase ends with one explicit
`ApplyDeferred`. Features add systems only into the hooks.

| Phase | Hook sub-sets, in order | M1 core mechanic systems (core slot, in order; scale-mix systems left out) |
|---|---|---|
| `Input` | `Housekeeping`, `Validate`, `Apply` | Transient resets, flow-cache tick start (Housekeeping); `apply_commands` (Apply) |
| `Think` | `Orders`, `Behaviour` | `advance_orders`, `wander_system` (Orders); `process_path_queue`, `compute_steering` (Behaviour) |
| `Act` | `Move`, `Fire` | `prepare_flow_tiles`, `move_system` (Move) |
| `Resolve` | `Projectiles`, `Effects`, `Stats` | none |
| `Economy` | `Main` | none in iteration 1 |
| `Perceive` | `Spatial`, `Vision` | `track_spatial` (Spatial) |
| `Lifecycle` | `Death`, `Spawn`, `Despawn` | `spawn_funnel` (Spawn), `despawn_funnel` (Despawn) |
| `Publish` | `Snapshot`, `Events` | `publish_snapshot` (Snapshot) |

Outside the schedule, `step_tick` puts the tick's commands (sorted by `(player, seq)`) into the inbox, runs
`SimTick`, hashes if due (`StateHash.interval_ticks`), takes the outputs, clears trackers and advances the clock.
The phase list, the sub-sets and the system order are part of the determinism contract (system-order snapshot).

## 5. Mechanic registry

One row per spec; `spec-lint` checks that the table lists exactly the files under `docs/design/mechanics/`.

| ID | Name | Crate | Milestone | Status | Spec | Owner track |
|---|---|---|---|---|---|---|
| MOVE.SPEED | Terrain speed, blocking and the drive step | `fl_sim::movement` | M1 | approved (lead, R148) | [MOVE-speed.md](mechanics/MOVE-speed.md) | sim-core |
| PATH.QUEUE | Budgeted path requests and HPA* plans | `fl_sim::path` | M1 | approved (lead, R148) | [PATH-requests.md](mechanics/PATH-requests.md) | sim-core |
| FLOW.GROUP | Group moves on shared flow fields | `fl_sim::nav::flow` | M1 | approved (lead, R148) | [FLOW-groups.md](mechanics/FLOW-groups.md) | sim-core |
| ORDER.QUEUE | Move, rotate, follow and stop orders | `fl_sim::orders` | M1 | approved (lead, R148) | [ORDER-queue.md](mechanics/ORDER-queue.md) | sim-core |
| STEER.SEP | Separation steering | `fl_sim::steer` | M1 | approved (lead, R148) | [STEER-separation.md](mechanics/STEER-separation.md) | sim-core |

The five M1 specs are backfilled: they describe M1 as built, and the lead approved them under R148. They are on the
M2 prelude's list for the user's ratification (Task 21).

## 6. Guesses

Values with no source yet carry `Guess(id)` provenance. Guess ids are defined once, here or in a mechanic spec's
`## Non-goals · Open questions · Guesses` section; the ledger may name only defined ids. Engine values that the spec
fixes (rather than a source) are guesses too, with Medium confidence.

- Guess G-CORE-1: `rules/StateHash.interval_ticks` = 20, the state hash every 20 ticks (one second at 20 Hz), from
  spec §5.4; a lockstep desync is caught within a second. Replace with a measured trade-off between hash cost and
  desync detection latency once networking exists.
- Guess G-CORE-2: `rules/Spatial.cell_m` = 32, the side of a spatial-index bucket in metres, chosen without a
  measurement. Replace with a value measured on the scale mix (neighbour candidates per query against buckets
  visited per query).
- Guess G-ROSTER-1: every unit's `Hull.hp` (`unit:*/components.Hull.hp`) is invented: relative toughness only, until
  the damage model of M2 and a tuning sweep set it.
- Guess G-ROSTER-2: every unit's `Mobility.turn_rate_deg_s` (`unit:*/components.Mobility.turn_rate_deg_s`) is
  invented (infantry fastest, heavy tanks slowest), until measured against hull traverse data or tuned for feel.

## 7. Versioning: `SIM_VERSION`, change classes, MCRs and re-blesses

ADR-0021 and ruling R146.

- **Change classes.** Every commit that touches `crates/**`, `features/**` or `assets/**` carries
  `Change-Class: <c>` (merge commits are exempt; the `change-class` gate row enforces it):
  - `R` refactor: no behaviour change; every golden hash stream stays bit-identical.
  - `D` data tuning: content values only.
  - `F` formula or rule: changes what the sim computes; carries `MCR: MCR-nnn` and bumps `SIM_VERSION`.
  - `N` new mechanic: new behaviour behind new content or commands; existing goldens verify unchanged.
  - `E` engine or crate upgrade: like `R`, bit-identical goldens.
  - `X` experiment: a spike, probe or measurement.
- **`SIM_VERSION`** (`crates/fl_sim/src/build.rs`) names the rules the sim computes. Every class F change bumps it,
  at most once per task. It is 2 at the start of the M2 prelude, 3 after Task 13 (MCR-001) and 4 after Task 15
  (MCR-002). Class R and E changes never move it and keep the goldens identical.
- **MCR.** A class F change starts with its entry in `docs/design/changelog-mechanics.md`: why, the requirement ids
  it changes, the predicted effect and the `SIM_VERSION` change, written before the code.
- **Gate rules** (`golden-policy` row): a changed golden needs an `MCR:` trailer and a changed `SIM_VERSION`, replay
  format or content; a changed `SIM_VERSION` needs a changed golden; the trailer's MCR must have a changelog heading.
- **Re-bless** (R146). Golden replays change only through `fl_cli rebless --reason MCR-nnn`. It refuses (exit 1,
  writing nothing) when the changelog has no `## MCR-nnn` heading, or when any replay's semantic check fails on the
  new build. Otherwise it writes `<name>.replay.new` for every replay whose bytes change and a report,
  `target/rebless-report.md`: per replay the first divergent tick, the journal summary diff, the entity count before
  and after, and the semantic results. The lead blesses the `.new` files under the user's delegation (`cargo xtask
  bless … --reason "MCR-nnn: …"`), and the plan's final report lists every re-bless for the user's ratification.
  The re-bless report must confirm the MCR's prediction; an unpredicted difference is a finding, not a re-bless.
- **Specs follow.** An MCR that changes a requirement updates its mechanic spec (`spec-version` + 1, a changelog
  line naming the MCR) as a `.md.new` file in the same task.

## 8. Statistical test policy

ADR-0022 (C4), for every mechanic with randomness from M2 on.

- **Oracles** come from the spec (a formula, a probability, a bound) or from an `f64` reference model written from
  the spec in the test, never from running the code under test and recording what it does.
- **Rates** are checked with `fl_test_utils::stats::assert_rate_within_wilson` at α = 1e-4: the observed count
  must lie inside the Wilson score interval around the spec's probability.
- **Two tiers.** Formula tier: 10⁵ draws of the formula alone (`fl_sim::rolls` over fixed keys). Scenario tier:
  200–500 seeds of a whole scenario.
- **Seeds.** Gate runs use fixed seeds. Nightly sets `FL_SEED_BASE` to the number of days since 2026-01-01, so
  seeds rotate daily. A failing seed set may be rerolled once (`seed_base + 1`), and the reroll is recorded in the
  run's report; a second failure is a failure.
- **Proptests** use `fl_test_utils::proptest_policy` (R140, R142): a fixed ChaCha seed, no failure persistence, the
  case count from `PROPTEST_CASES` with a per-test default, random seeds only at nightly (`FL_PROPTEST_RANDOM=1`).
  A committed regression becomes a named test.
- **Journal.** Determinism tests never read the journal; a test that reads it opts in with
  `ScenarioBuilder::journal(true)` (R139).

## 9. Acceptance tests, review tiers and the mini-gate

ADR-0022 (C3, C10).

- **Acceptance and holdout tests.** The test-author writes acceptance tests (`tests/acceptance/`, every gate level)
  and holdout tests (`tests/holdout/`, built only with the `holdout` feature, run at nightly and milestone) from a
  spec alone, citing its rule ids with `// @req`. Holdout is at least 10 % of a spec's acceptance and holdout tests
  (milestone FAIL below 10 %, WARN above 20 %). Implementers never read holdout files; a protected test that
  contradicts its spec is reported as `BLOCKED: test X contradicts spec Y because …`.
- **Review tiers.** `cargo xtask review-tier` names the tier of a diff; the highest tier touched wins:
  - **P**, protected paths: the lead reviews and blesses, and lists the change for the user.
  - **A**, contract or sim core: a spec and determinism review plus a quality review.
  - **B**, client and tools: one combined review.
  - **C**, docs, and data with `data-check` green: one short review.
- **Mini-gate.** `cargo xtask mini-gate <AREA.NAME>` runs that spec's `spec-lint` and `trace`, then every test
  tagged with its rules (holdout included), so a milestone is a set of mechanics that each passed their own gate;
  it prints `mini-gate MOVE.SPEED: PASS` for `MOVE.SPEED`.
