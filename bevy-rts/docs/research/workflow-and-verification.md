# Frontline: workflow from design to implementation to verification for an agent-built game

Research date: 2026-10-23. Scope: the Frontline RTS described in
`docs/superpowers/specs/2026-10-23-frontline-rts-design.md` (Bevy 0.20, deterministic fixed-point sim at 20 Hz in its
own World, RON content, one feature crate per mechanic, built mostly by Claude Code subagents).

How this was checked:
- I looked up every crate and tool version in this report on the crates.io API or GitHub releases on 2026-10-23.
- I read the Bevy 0.20 claims in the `v0.20.0` tag source: `ScheduleBuildSettings`, `CiTestingConfig`, the BRP method
  names, `RemotePlugin::with_method_main`, and the `examples/remote/` integration test.
- Where a source is older than 2024, I mark it as a classic reference.
- Anything I recommend without a source is marked as my judgement.

---

## 0. Summary

The current process (brainstorm → spec → plan → subagent TDD → two reviews → commit) is sound. Its weakness is that
nearly every check is **an LLM grading LLM output**. The implementer writes the tests that grade its own code, and
reviewers read code they can't run. The research consistently says this is where coding agents fail:
- Frontier agents edit assertions, special-case inputs and hardcode expected values when tests get hard
  ([ImpossibleBench, ICLR 2026](https://arxiv.org/abs/2510.20270);
  [Claude 3.7 Sonnet system card](https://www.anthropic.com/claude-3-7-sonnet-system-card)).
- On long tasks, the gap between visible-test and held-out-test pass rates grows with code size
  ([SpecBench, 2026](https://arxiv.org/abs/2605.21384)).

Frontline has an unusual advantage. **A deterministic, command-driven sim makes almost every check mechanical**:
- bit-exact replays
- exact oracles for formulas
- statistical tests that can't be flaky (fixed seeds give the same answer every run)
- deterministic "work counters" that stand in for performance

The recommended workflow moves trust from reviewers to **mechanical, tamper-resistant gates**:
1. **Mechanic specs are machine-checkable.** They have requirement IDs, formulas, parameter tables with provenance, and
   worked examples that turn directly into tests.
2. **Acceptance tests come from the spec, written by a different agent than the implementer, and they are protected.**
   Hooks and CODEOWNERS stop the implementer editing them. A small held-out set runs only at the milestone.
3. **Determinism gates move left.** Run the twin-run, schedule shuffle and a snapshot of the system order on every
   task, not only at milestones.
4. **Golden replays are versioned.** Each replay has both bit-exact and semantic assertions, plus an explicit
   re-bless protocol tied to a `SIM_VERSION` bump.
5. **Mutation testing checks the tests themselves:** cargo-mutants on the sim crates, `--in-diff` per PR and a full
   run nightly.
6. **CI starts in M0** on Linux, Windows and macOS. A fast sim-only job doesn't compile the renderer.

---

## 1. Verified tool and crate versions (2026-10-23)

| Purpose | Crate / tool | Version | Notes |
|---|---|---|---|
| Engine | `bevy`, `bevy_remote` | 0.20.0 | Released 2026-10-08 |
| Test runner | `cargo-nextest` | 0.9.149 | Per-process tests, retries, `--flaky-result fail` (since 0.9.131), JUnit output |
| Snapshot tests | `insta` / `cargo-insta` | 1.49.0 | Features include `ron`, `yaml`, `json`, `redactions`, `glob`, `filters` |
| Property tests | `proptest` | 1.12.0 | |
| | `proptest-state-machine` | 0.9.0 | Command-sequence (model-based) testing |
| | `proptest-derive` | 0.9.0 | |
| | `test-strategy` | 0.4.5 | Nicer proptest macros (optional) |
| Table tests | `rstest` | 0.28.0 | Spec example tables → test cases |
| Data-driven tests | `datatest-stable` | 0.3.3 | One test per scenario file, on stable Rust |
| Fuzzing | `cargo-fuzz` | 0.13.2 | Needs nightly; best on Linux |
| | `libfuzzer-sys` | 0.4.13 | |
| | `arbitrary` | 1.5.0 | |
| | `bolero` | 0.13.7 | Optional: one harness for proptest-style and fuzz engines |
| Mutation testing | `cargo-mutants` | 27.1.0 | `--in-diff`, `--shard k/n`, `--iterate`, `--test-tool nextest` |
| Coverage | `cargo-llvm-cov` | 0.9.1 | Works on `x86_64-pc-windows-msvc`; `cargo llvm-cov nextest` |
| Benchmarks (wall clock) | `criterion` | 0.8.2 | Local and dedicated-machine baselines |
| Benchmarks (instruction count) | `gungraun` | 0.20.0 | The renamed successor of `iai-callgrind`; Valgrind, Linux only |
| | CodSpeed (`codspeed-rust` v6.0.0, `CodSpeedHQ/action` v5.6.0) | | Hosted alternative; "simulation" mode counts instructions on ordinary runners |
| Statistics | `statrs` | 0.19.1 | Binomial and normal quantiles for test bounds |
| Hashing | `xxhash-rust` | 0.8.19 | State and rules hash (already chosen) |
| Fixed point | `fixed` | 1.31.0 | Implements `FromStr`: parse RON decimals straight to fixed, with no f64 step |
| Schema / docs generation | `schemars` | 1.2.3 | JSON Schema from def structs → generated content docs and editor completion |
| Traceability (optional) | `mantra` | 0.9.0 | Requirement ↔ test tracing. A small in-house `xtask` check is enough to start (§3.4) |
| Schedule visualization | `bevy_mod_debugdump` | 0.17.0 | Requires bevy ^0.20.0 |
| Agent-driven play | `bevy_brp_mcp` / `bevy_brp_extras` | 0.23.0 | MCP server for BRP; require bevy ^0.20.0 |
| Debug UI | `bevy_egui` | 0.43.1 | Requires bevy ^0.20.0 |
| | `bevy-inspector-egui` | 0.38.0 | Requires bevy ^0.20.0 |
| Image diff | `image-compare` | 0.5.0 | Golden-image tests with a tolerance |
| | `dssim` | 3.5.1 | Structural similarity |
| Crash and bug reports | `sentry` | 0.49.3 | Optional, for external playtests |
| Supply chain | `cargo-deny` | 0.20.2 | `EmbarkStudios/cargo-deny-action` v2.1.1 |
| | `cargo-audit` | 0.22.2 | |
| | `cargo-shear` | 1.14.0 | Unused dependencies |
| CI actions | `actions/checkout` | v7.0.1 | |
| | `actions/cache` | v6.1.0 | |
| | `Swatinem/rust-cache` | v2.9.3 | |
| | `taiki-e/install-action` | v2.87.30 | |
| Spec tooling (reference only) | `github/spec-kit` | v1.2.3 | Spec → Plan → Tasks → Implement, like superpowers |

---

## 2. The recommended workflow

### 2.1 Pipeline overview

Each stage produces an artifact, and each gate is mechanical where possible.

| # | Stage | Who | Artifact | Gate to leave the stage |
|---|---|---|---|---|
| 0 | **Research** | research subagent | `docs/research/*.md` with file refs into nival/Blitzkrieg, in our own words | Every number cites a source or is marked a guess |
| 1 | **Mechanic spec** | brainstorm with the human, then a spec-writer agent | `docs/design/mechanics/<AREA>-<name>.md` (template in §3.2) | `xtask spec-lint` passes (IDs unique, every REQ has an acceptance criterion, every parameter has provenance); **human approves** |
| 2 | **Tuning data** | spec-writer or implementer | RON defaults in `assets/base/**` plus provenance entries (§3.3) | `fl_cli data-check` passes, including the provenance check |
| 3 | **Acceptance tests** | a **test-author agent that is not the implementer** | `tests/acceptance/<area>/*.rs` and `scenarios/*.scenario.ron`, written from the spec only | Tests compile and **fail** for the right reason (unimplemented or todo); human or lead skims them; files become protected |
| 4 | **Plan** | `writing-plans` | `docs/plans/<milestone>-<mechanic>.md`; each task lists the REQ IDs it covers and the files it may touch | `xtask plan-lint`: every REQ in scope maps to at least one task; task file scopes don't overlap across parallel tracks |
| 5 | **Implement (per task)** | implementer subagent, TDD | Code, unit and property tests, commit | **Task gate** (§5.1), run by a Stop/SubagentStop hook. Evidence (command output) pasted into the task report |
| 6 | **Review (per task, risk-tiered)** | reviewer subagents in fresh context | Review notes | Low risk: one combined review. Sim core: spec, quality and determinism/ECS checklist (§5.5). Test-diff guard findings resolved |
| 7 | **Integrate** | CI | PR / merge to main | **PR gate** (§5.2) on Linux, Windows and macOS |
| 8 | **Milestone verification** | lead agent and human | Demo run script (BRP), perf baselines, re-blessed goldens report, playtest notes | **Milestone gate** (§5.3) including the held-out acceptance suite and a manual playtest checklist |
| 9 | **Feedback** | human playtests and bug captures | `bugs/inbox/*` bundles, `docs/design/feedback-ledger.md` | Each item is triaged into a bug (→ regression test), a tuning change (→ A/B sweep) or a design change (→ back to stage 1) |

### 2.2 Why there's a separate stage 3

Today the implementer subagent writes the test first, then the code. TDD discipline is kept, but the test still encodes
the implementer's reading of the spec. When the code gets hard, the same agent can quietly make the test easier.
ImpossibleBench reports agents editing assertions and special-casing inputs, and the Claude 3.7 Sonnet system card says
this tends to happen after repeated failed attempts. Anthropic's own guidance suggests the same split:
- "have one Claude write tests, then another write code to pass them"
- have a fresh-context reviewer check the diff against the plan

Both are in [Claude Code best practices](https://code.claude.com/docs/en/best-practices).

So:
- **Acceptance tests** (stage 3) are spec-level, black-box, written from the spec alone, and protected.
- **Unit and property tests** (stage 5) stay with the implementer, under normal TDD.
- A **held-out slice** of acceptance cases (about 10–20%, for example extra worked examples, mirrored and rotated
  scenarios, and extra seeds) lives outside the implementer's reach. It runs at the milestone gate and nightly.
  SpecBench measures reward hacking as exactly this visible-versus-held-out gap.

### 2.3 What a task brief must contain

These briefs go to implementer subagents.
- The mechanic spec section and REQ IDs the task covers. Paste them in; don't just link.
- The file and crate scope it may edit, and the **protected paths** it may not edit.
- The exact gate command, `cargo xtask gate task`, and the rule that it must paste the output.
- What it must not do: weaken or delete existing assertions, add `#[ignore]`, widen tolerances, cut proptest cases,
  or change goldens. If it believes a test is wrong, it **stops and reports**. That case is legitimate: EvilGenie notes
  that many "cheats" are agents fixing genuinely wrong tests, so the escape hatch must exist, but it goes through a
  human or lead decision rather than a silent edit.

---

## 3. Design documentation that works for agents

### 3.1 One living GDD index plus one spec per mechanic

My recommendation, based on how agents use context:
- **`docs/design/GDD.md`** is a short living index of no more than about 300 lines:
  - pillars and anti-goals (lift from design §1–2)
  - a glossary (RU, relax time, piercing, …)
  - the SimSet order contract
  - a **mechanic registry table**: ID, name, crate, milestone, status, spec link, owner

  It's the one document every agent reads.
- **`docs/design/mechanics/<AREA>-<name>.md`**, one per mechanic, each no more than about 2 pages. Examples:
  `COMBAT-penetration.md`, `COMBAT-dispersion.md`, `VISION-camouflage.md`, `SUPPLY-morale.md`. An agent loads only
  the specs its task touches. Claude Code's guidance is that context is the scarce resource and that the most useful
  specs "name the files and interfaces involved, state what is out of scope, and end with an end-to-end verification
  step" ([best practices](https://code.claude.com/docs/en/best-practices)).
- **`docs/design/decisions/ADR-NNNN-*.md`**: one-page decision records, for things like "I48F16 over I32F32" or
  "suppression model v2".
- **`docs/design/changelog-mechanics.md`**: one entry per mechanic change request (§6), linked to `SIM_VERSION` bumps.
- **Generated, never hand-written:**
  - `docs/design/generated/content-reference.md`: every def component, its fields, units and ranges, generated from
    the def structs through `schemars`
  - `docs/design/generated/tuning-tables.md`: the current values from RON joined with provenance

Why generate? Hand-maintained trait docs drift. OpenRA's community wiki for traits was reported as sparse and at times
offline, so modders read the C# source instead
([OpenRA forum thread](https://forum.openra.net/viewtopic.php?p=279106); weak, anecdotal evidence, but it's the
classic failure mode).

The separate-spec-per-feature pattern matches the 2025–2026 spec-driven tools:
- [GitHub Spec Kit](https://github.github.com/spec-kit/) uses Spec → Plan → Tasks → Implement, with one Markdown
  artifact per phase.
- [Kiro](https://kiro.dev/docs/specs/) writes `requirements.md`, `design.md` and `tasks.md` per spec. Third-party
  write-ups say its acceptance criteria use EARS notation ([morphllm](https://www.morphllm.com/spec-driven-development)).
  The Kiro docs page I read says only "structured notation".

A caveat from InfoWorld's 2026 review: none of these tools truly makes the spec the source of truth
([InfoWorld](https://infoworld.com/article/4171332/four-cutting-edge-tools-for-spec-driven-development.html)). Sync has
to be enforced by your own checks (§3.4).

### 3.2 Mechanic spec template (machine-checkable)

Write requirements in [EARS](https://alistairmavin.com/ears/) form:
- Ubiquitous: "The sim shall …"
- Event-driven: "When …, the sim shall …"
- State-driven: "While …, …"
- Unwanted behaviour: "If …, then …"
- Optional feature: "Where feature X is enabled, …"

EARS is short, unambiguous, and easy for a linter to check. Each requirement gets a stable ID.

```markdown
# COMBAT.PEN — Armor penetration            status: approved · spec-version: 3 · crate: fl_sim::combat
Milestone: M2 · Depends on: COMBAT.HIT, COMBAT.SIDE · Emits journal events: ShotResolved, Penetrated, Ricochet

## Intent (player-facing, ≤3 lines)
Heavier guns beat thicker armor most of the time, and angle (side) matters. Results are random but stay within
predictable bands that the info panel can show.

## Rules
- COMBAT.PEN.1  When a shell hits a target, the sim shall draw piercing p ~ U[pierce − pierce_rand, pierce + pierce_rand].
- COMBAT.PEN.2  When a shell hits a target, the sim shall draw armor a ~ U[armor_min(side), armor_max(side)] for the side hit (COMBAT.SIDE).
- COMBAT.PEN.3  The shell penetrates iff p ≥ a AND the target's cover roll does not save it (COVER.SAVE).
- COMBAT.PEN.4  If the shell does not penetrate, then the sim shall apply no HP damage from that shell (splash: COMBAT.SPLASH).
- COMBAT.PEN.5  The sim shall draw p and a, in that order, from the shared sim RNG stream `combat`, once each per hit.

## Formulas (units explicit; fixed-point in runtime)
P(pen | no cover) = P(U[p0−r, p0+r] ≥ U[amin, amax])   (closed form: see appendix A, exact piecewise-quadratic)

## Parameters
| Name | RON path | Unit | Default | Valid range | Provenance | Confidence |
|---|---|---|---|---|---|---|
| pierce | weapons/*.shells[].piercing | mm | per weapon | 0–300 | original-scale (RPGStats.cpp, see research §1.3) | high |
| pierce_rand | weapons/*.shells[].piercing_random | mm | per weapon | 0–50 | original-formula | high |
| armor_min/max | units/*.Armor.{front,side,rear,top,bottom} | mm | per unit | 0–250 | ours (scaled from original) | medium |

## Acceptance criteria (each becomes a protected test)
- AC-1 (COMBAT.PEN.1–3, exact): Given p0=60, r=10, armor=(40,55), then P(pen)=… (exact value from appendix A),
  and the formula-level test over 100k draws lands within the Wilson 99.99% bound.        → tests/acceptance/combat/pen.rs
- AC-2 (COMBAT.PEN.4): Given a non-penetrating hit, then target HP is unchanged and a `Ricochet` event is journaled.
- AC-3 (scenario, statistical): `scenarios/combat/pz3_vs_t34_front_300m.scenario.ron`, 400 seeds:
  first-shot penetration rate within [x, y] (derived from AC-1 × hit geometry; tolerance policy §4.4).
- AC-4 (metamorphic): rotating AC-3's scenario 90° or mirroring it leaves the penetration rate within bounds.
- HELD-OUT: 2 more worked examples kept in tests/holdout/ (not shown to the implementer).

## Non-goals · Open questions · Guesses
- Guess G-PEN-1: no angle-of-impact model beyond side selection (Blitzkrieg has none, per research). Revisit in M9.

## Changelog
- v3 (2026-11-02, MCR-014): cover save moved after the penetration roll (RNG order change → SIM_VERSION 7).
```

Rules for the linter, `cargo xtask spec-lint` (my judgement; about 150 lines of Rust):
- Every `AREA.NAME.N` ID is unique across `docs/design/mechanics`.
- Every REQ appears in at least one AC.
- Every AC names a test path that exists, unless status is `draft`.
- Every parameter row has a RON path that resolves in the content schema, plus a provenance value from the controlled
  vocabulary.
- Every journal event named exists in the `fl_protocol` event enum or registry, which can be dumped by
  `fl_cli schema --events`.

### 3.3 Tuning tables and provenance

Blitzkrieg values come from three sources with very different trust levels: verified source code, the manual, and our
guesses (design §7 says gaps get "a documented guess in `rules.ron`"). That trust level has to be visible to agents and
to data-check. RON comments don't survive parsing, and `deny_unknown_fields` rules out inline metadata, so use a
**sidecar ledger**:

```ron
// assets/base/provenance/rules.prov.ron
{
  "rules.splash.area_damage_coeff": (src: Original(file: "RPGStats.cpp", note: "research §1.3"), conf: High),
  "rules.splash.armor_for_area_damage": (src: Original(file: "RPGStats.cpp"), conf: High),
  "rules.morale.out_of_supply_floor":  (src: Guess(id: "G-MOR-2", by: "builder", date: "2026-11-10"), conf: Low),
  "rules.experience.rank_mult":        (src: Guess(id: "G-EXP-1"), conf: Low),
  "units.base.de.pz3.Armor.front":     (src: OursScaled(from: "original pz3 data", note: "own numbers, realistic scale"), conf: Medium),
  "rules.infantry.prone_cover":        (src: Tuned(mcr: "MCR-021", sweep: "sweeps/2026-11-20-prone.json"), conf: Medium),
}
```

- **The vocabulary is closed:** `Original | Manual | ResearchDerived | OursScaled | Guess | Tuned`.
  - `Tuned` must reference an A/B sweep artifact (§6.6).
  - `Guess` must reference a guess ID in a spec's "Guesses" section.
- **`data-check` enforces it:**
  - Every key in `rules.ron` has an entry, and so does every *template* def field that's numeric.
  - Concrete units can inherit "OursScaled" by default.
  - No entry points at a path that no longer exists, which catches stale provenance.
- **The generated `tuning-tables.md`** joins values with provenance. A designer can then ask "show me every Low
  confidence number in combat", and agents know which numbers they're allowed to move freely. Guesses and Tuned values
  can move; Original values need an MCR.
- **Licensing:** "Original" means the *formula or scale* was read in the source. The numbers in our RON are still our
  own (design §3). The ledger is also the audit trail for that rule.

### 3.4 Keeping docs, data and code in sync (traceability)

| Link | Mechanism | Enforced by |
|---|---|---|
| Requirement → test | A test-side tag `// @req COMBAT.PEN.3`, or `mantra`'s `#[req(...)]` attribute ([mantra 0.9.0](https://docs.rs/mantra)) | `xtask trace`: every approved REQ has at least one test, every tag points at an existing REQ, matrix written to `target/trace.md` |
| Test → code | Coverage of acceptance tests per crate (`cargo llvm-cov nextest -p fl_sim --test acceptance_*`) | Report only; not gating |
| Parameter → data | The spec's parameter table "RON path" column | `spec-lint` against the generated schema |
| Data → docs | `schemars` → generated content reference | CI regenerates and fails on diff (`git diff --exit-code docs/design/generated`) |
| Data → provenance | Sidecar ledger | `data-check` |
| Mechanic change → version | `changelog-mechanics.md` entry plus `SIM_VERSION` bump | CI rule (§6.3) |

Don't tag production code with REQ comments; they rot. Trace through tests. Coverage closes the loop from code back to
requirements well enough.

### 3.5 Studio and open-source precedents

- **Factorio:**
  - Integration tests build small maps, run ticks and compare the result to pre-saved CRCs, which are "regenerated
    when game logic changes". That's the golden-hash plus re-bless pattern.
  - Desyncs are debugged with per-tick whole-map CRCs and per-tick readable snapshots, comparing the first divergent
    tick.
  - These are classic posts, 2014–15: [FFF #60](https://factorio.com/blog/post/fff-60),
    [FFF #47](https://factorio.com/blog/post/fff-47), [FFF #62](https://cf-www.factorio.com/blog/post/fff-62).
- **Riot (League of Legends):** game-server determinism, used to replay and diagnose issues (classic, 2017):
  [Determinism in LoL](https://technology.riotgames.com/news/determinism-league-legends-introduction).
- **Age of Empires:** the original lockstep RTS write-up, including how sync bugs were caught (classic, 2001):
  [1500 Archers](https://www.gamedeveloper.com/programming/1500-archers-on-a-28-8-network-programming-in-age-of-empires-and-beyond).
- **Sea of Thieves (Rare):** "actor tests" as a middle layer between unit and integration tests, and very large
  automated suites (classic, 2019):
  [GDC talk](https://gdcvault.com/play/1026366/Automated-Testing-of-Gameplay-Features). Your `ScenarioBuilder` tests
  are the equivalent of their actor tests.
- **Spec Kit and Kiro** (2025–26) for spec → plan → task artifacts, cited above.

---

## 4. The test pyramid for a deterministic sim

### 4.1 The layers

From cheapest and most numerous to most expensive. "Gate" says where each layer blocks: T = task, P = PR,
N = nightly, M = milestone, R = release.

| Layer | What | Crates | Gate | Notes |
|---|---|---|---|---|
| **L0 Static** | fmt, clippy `-D warnings`, determinism lints, dependency rules, `data-check`, spec-lint, trace, cargo-deny | clippy, `cargo-deny` 0.20.2, `cargo-shear` 1.14.0 | T, P | See §4.2 for the determinism lints |
| **L1 Formula unit tests** | Pure functions in `fl_math` and combat/vision math, using spec worked examples as tables | `rstest` 0.28.0 | T | Exact expected values from the spec, never "whatever the code returned" |
| **L2 Property and differential tests** | Invariants, monotonicity, metamorphic relations, and fixed-point vs an f64 reference oracle | `proptest` 1.12.0, `test-strategy` 0.4.5 | T | §4.3 |
| **L3 Headless scenario tests** | `ScenarioBuilder` and `*.scenario.ron` files run for N ticks, asserting on **journal events** and final state | `datatest-stable` 0.3.3 (one test per scenario file), nextest | T, P | The sim's equivalent of actor tests |
| **L4 Statistical tests** | Rates and distributions over fixed seed sets, with exact oracles | `statrs` 0.19.1 | T (formula level), P (scenario level), N (rotating seeds) | §4.4; deterministic, so never flaky |
| **L5 Determinism** | Twin-run, schedule shuffle, system-order snapshot, replay round-trip, cross-OS golden hashes | Bevy `ScheduleBuildSettings` (0.20), `insta` | T, P | §4.5 |
| **L6 Golden replays and snapshots** | Versioned replays with bit-exact and semantic assertions; insta snapshots of journal summaries, schedules and schemas | `insta` 1.49.0 (`ron`, `yaml`, `redactions`) | P | §4.6, §6.3 |
| **L7 Fuzzing** | Random command streams against invariants; RON and replay parser fuzzing | `proptest-state-machine` 0.9.0; `cargo-fuzz` 0.13.2, `libfuzzer-sys` 0.4.13, `arbitrary` 1.5.0 | P (short proptest), N (cargo-fuzz 30 min) | §4.7 |
| **L8 Balance sweeps** | Matchup matrices, Lanchester sanity checks, A/B comparisons | `fl_cli sweep`, `statrs` | N (report), M (review) | §4.8; reports, not hard gates, apart from sanity bands |
| **L9 Performance** | Deterministic work counters (gate), instruction counts (PR trend), criterion wall clock (milestone) | `criterion` 0.8.2, `gungraun` 0.20.0 or CodSpeed | T (counters), P/N (instructions), M (wall clock) | §4.9 |
| **L10 Client and visual** | Input → command tests under `MinimalPlugins`; BRP-driven integration; golden screenshots | `bevy_ci_testing`, `bevy_remote`, `image-compare` 0.5.0 / `dssim` 3.5.1 | P (input tests), N (screenshots) | §4.10 |
| **L11 Manual playtest** | Checklists per milestone, plus free play | — | M | §7 |
| **Meta: test quality** | Mutation testing, coverage | `cargo-mutants` 27.1.0, `cargo-llvm-cov` 0.9.1 | P (in-diff), N (full) | §5.4 |

### 4.2 L0: determinism lints that run before any test

Each sim crate (`fl_math`, `fl_protocol`, `fl_content`'s runtime half, `fl_sim`, every feature crate's `sim` module)
gets its own `clippy.toml` and crate attributes. Clippy looks for `clippy.toml` starting from the crate directory.

- `#![deny(clippy::float_arithmetic)]` in sim crates. `fl_content` has a narrowly `#[allow]`ed conversion module, or
  better, parses decimals straight to `Fx` with `fixed`'s `FromStr`.
- `disallowed-types`: `std::collections::HashMap` and `HashSet` (and the `bevy_platform` ones in sim state),
  `std::time::Instant`, `std::time::SystemTime`.
- `disallowed-methods`: `std::time::Instant::now`, `rand::thread_rng` / `rand::rng`, `std::env::var` in sim code.
- `disallowed-types` for `bevy::time::Time` in sim crates, as a backstop to the `cargo tree` dependency rule.
- An `xtask deps-check` asserts the design §5.1 rules: no `bevy_render`, `bevy_window`, `bevy_input` or `bevy_time`
  in the sim crates' `cargo tree -e normal`.
- `cargo deny` with `bans` for float-physics crates and an allowlist of licences (also enforces design §3 licensing).
- `SimTick` builds with `ScheduleBuildSettings { ambiguity_detection: LogLevel::Error, .. }`. The default is
  `Ignore`, which I confirmed in the 0.20 source.

### 4.3 L2: property, metamorphic and differential tests

The highest-value property tests for this game are my judgement:
- **Invariants** after any tick:
  - HP ∈ [0, max]
  - ammo ≥ 0
  - every unit in a passable cell for its movement class
  - the spatial grid agrees with a brute-force recount
  - per-cell vision refcounts equal a brute-force recompute
  - SimIds are unique and monotonic
  - no `Entity` in hashed state
- **Monotonicity:**
  - more armor never raises the penetration rate; more piercing never lowers it
  - `terrain_pass` ↑ ⇒ speed ↑
  - morale ↓ ⇒ dispersion never shrinks

  Check these with paired seeds (common random numbers) so they are exact, not statistical.
- **Metamorphic relations.** Simulation models usually have no direct oracle, and metamorphic testing is the standard
  answer ([Raunak & Olsen, NIST-listed](https://csrc.nist.gov/pubs/conference/2021/06/02/metamorphic-testing-on-the-continuum-of-verificati/final);
  [2025 Modelica paper](https://ecp.ep.liu.se/index.php/modelica/article/view/1300)):
  - Rotating a scenario by 90° about the map centre (on a symmetric map) keeps outcome distributions equal.
  - Translating it leaves results bit-identical, if the grid is translation-invariant.
  - Swapping player IDs mirrors outcomes. SimId tie-breaks make this distributional, not bit-exact.
  - Splitting one 2-unit group command into two 1-unit commands on the same tick makes no difference where the spec
    says commands are independent.
- **Differential (reference oracle).** `fl_test_utils::oracle` holds a deliberately naive **f64** implementation of
  each formula (speed, dispersion radius, ellipse sampling transform, armor side selection, the penetration closed
  form). Proptest compares it with the `Fx` implementation within a stated epsilon. It catches fixed-point overflow,
  rounding bias and lookup-table trig errors, and the oracle is too simple to share the implementer's bug. It lives
  in test code, so float lints don't apply.
- **Content round-trip:** parse → resolve → canonical serialize → parse gives an equal `ContentDb` and an equal rules
  hash. Key order and whitespace never change the rules hash.

### 4.4 L4: statistical tests that cannot be flaky

**The key point:** randomness in the sim comes only from a seeded PCG stream, so a statistical test over a **fixed seed
set** is deterministic. It passes or fails the same way on every run and every OS. Flakiness isn't the risk.
Brittleness and overfitting are:
- **Brittleness:** any legitimate change in RNG draw order (for example COMBAT.PEN v3 above) reshuffles every sample.
  A tight bound might flip from pass to fail.
- **Overfitting:** an agent tunes code or tolerances until the fixed seeds pass. Seed pinning used as a workaround is
  a known anti-pattern ([Dutta et al., ICST'22](https://cs.cornell.edu/~saikatd/papers/seeds-icst22.pdf)).

Policy (my judgement, standard statistics):

1. **The oracle comes from the spec, not the code.** For the penetration example, the spec's appendix gives the exact
   probability of `U[p0±r] ≥ U[amin,amax]`. It's a piecewise-quadratic closed form, so compute it exactly in the test
   with rationals or f64. Scenario-level targets come from the formula-level oracle times the hit geometry. Never
   paste a rate measured from the implementation into a test.
2. **Bound by a fixed false-failure budget.** Assert that the oracle p₀ lies inside the **Wilson score interval**
   around the observed rate p̂ at confidence 1−α, with **α = 10⁻⁴** (z ≈ 3.89).
   - With about 200 such tests, a *correct* change that reshuffles the RNG trips about 0.02 of them on average.
   - When one trips, the protocol is to reroll the seed set (`seed_base += 1`) **once**, recorded in the commit. If it
     still fails, the code is wrong.
3. **Size N for power, two tiers:**
   - **Formula level (L1/L4, per task):** call `resolve_penetration(&mut rng, …)` directly, 10⁵ draws, a few ms.
     To detect a 1-point deviation at p = 0.5 you need N ≈ (z_α + z_β)²·p(1−p)/δ² = (3.89 + 1.28)²·0.25/0.01² ≈ 67k.
   - **Scenario level (L3/L4, per PR):** full sim duels, 200–500 seeds, detecting deviations of about 5–7 points.
     Example: (5.17²·0.25)/0.06² ≈ 1,860 for 6 points. So scenario tests use a **coarser tolerance**, and the precise
     check lives at formula level.
4. **Rotating seeds nightly.** The nightly job runs every statistical test with `seed_base = date-hash`. A failure
   there is a real bug (or a 10⁻⁴ fluke, confirmed by rerunning with a third base). It also catches code accidentally
   overfitted to the PR seeds.
5. **Held-out seeds and examples** stay in `tests/holdout/` and run only at the milestone gate and nightly.
6. **Distribution shape, not just the mean:**
   - Dispersion: χ² goodness of fit on impact points binned in the ellipse frame, checking uniform-in-ellipse and the
     3:1 elongation.
   - Aim and relax factor: a KS test against U[1, 1.2].
   - `statrs` gives the χ² and normal quantiles.
7. **Assert through the journal.** Count `ShotResolved { penetrated }` events rather than reaching into internals. The
   journal is the stable observation API, and it already records every roll.

Sketch:

```rust
// tests/acceptance/combat/pen.rs   // @req COMBAT.PEN.1 COMBAT.PEN.2 COMBAT.PEN.3
#[test]
fn ac1_pen_rate_matches_closed_form() {
    let (p0, r, amin, amax) = (60.0, 10.0, 40.0, 55.0);
    let expected = oracle::pen_probability(p0, r, amin, amax);       // exact, from spec appendix A
    let mut rng = SimRng::from_seed(seeds::formula(0xC0FFEE));       // fixed seed set
    let n = 100_000;
    let hits = (0..n).filter(|_| resolve_penetration(&mut rng, fx(p0), fx(r), fx(amin), fx(amax))).count();
    assert_rate_within_wilson(hits, n, expected, ALPHA_1E4);         // fl_test_utils helper; prints p̂, CI, p0
}
```

### 4.5 L5: determinism checks (move them left)

| Check | How | Gate |
|---|---|---|
| Twin-run | Every scenario test builds two sim Apps from the same inputs and compares the state hash every tick (cheap: hash every tick in tests) | T |
| Schedule shuffle | Bevy 0.20 has `ScheduleBuildSettings::shuffle_seed: Option<u64>`: "systems will be shuffled according to the given seed" while ordering constraints still hold. Run the scenario suite with 3 shuffle seeds and the single-threaded executor (the Bevy docs recommend it here); hashes must equal the unshuffled run. This finds missing `.before/.after` edges that ambiguity detection can miss, for example via `Commands` | T (1 seed), P (3 seeds) |
| System-order snapshot | Dump the topologically sorted `SimTick` system list (and the sets) as text and `insta::assert_snapshot!` it. Any order change shows up in review as a diff, which is how parallel subagents' schedule edits become visible. `bevy_mod_debugdump` 0.17 can render a graph for humans | T |
| Replay round-trip | Record → serialize → load → replay gives an identical hash stream | T |
| Cross-OS goldens | Each CI OS runs `fl_cli replay-verify tests/replays/**` against stored hashes. Windows x64, Linux x64 and macOS arm64 also cover a different CPU architecture | P |
| Executor parity | Once the multi-threaded executor is on: the same scenarios give identical hashes under single- and multi-threaded executors | P |
| Save/load parity | (When saves exist) run 500 ticks, save, load, run 500 more: hash equals a straight 1,000-tick run. This catches hidden state in `Local<T>`, change ticks and caches | P |
| Soak | 3,000 units for 72,000 ticks (an hour of game time), twin-run, no divergence | M, R |

### 4.6 L6: golden replays and approval snapshots

- **Golden replay file:**
  `{ sim_version, rules_hash, content_version, feature_flags, seed, scenario, commands[], hashes_every_20_ticks[], semantic: [...] }`.
- **Two kinds of assertions per replay:**
  - **Bit-exact:** the hash stream. Very sensitive; it catches *any* behaviour change, including RNG-order changes.
  - **Semantic:** outcome-level facts that must stay true across legitimate changes. For example "player 2 holds the
    bridge at tick 2400", "the T-34 dies before tick 600", "no friendly-fire events", "the truck arrives within
    [380, 460] ticks".

  When a mechanic legitimately changes, hashes get re-blessed (§6.3), but the semantic assertions must still pass or
  be changed *in the spec*.
- **insta snapshots** for structured text:
  - journal summaries (event counts per type per 100 ticks)
  - the system order
  - the generated schema
  - `data-check` error messages, so error UX doesn't regress
  - CLI JSON output, so the agent-facing API stays stable

  Use RON or YAML formats, with `redactions` for volatile fields.
- **insta in CI:** `INSTA_UPDATE` defaults to `auto`, which means `no` when CI is detected, so CI never writes
  snapshots. `--unreferenced=auto` acts as `reject` in CI, so orphaned snapshots fail
  ([insta advanced docs](https://insta.rs/docs/advanced/)). **Forbid `INSTA_FORCE_PASS` and `cargo insta accept` in
  agent sessions** through a PreToolUse hook (§5.4). Only the re-bless protocol may accept.
- **Approval discipline:** an approved file is "a reviewed example of expected behavior, not ground truth", and
  without review approval testing decays into "accepting whatever the program printed most recently"
  ([qaskills guide](https://qaskills.sh/blog/approval-testing-golden-master-guide)). Hence the re-bless report in §6.3.

### 4.7 L7: fuzzing commands and content

- **Command-stream fuzzing on stable Rust and all OSes** with `proptest-state-machine` 0.9.0:
  - The *reference state machine* is a tiny model: which SimIds exist, who owns them, which kinds each can do.
  - Transitions are `PlayerCommand`s, including **invalid ones**: wrong owner, dead units, unknown kinds, absurd
    coordinates, huge unit lists, the same tick, past ticks.
  - Postconditions: the invariants from §4.3; invalid commands are rejected and journaled, never applied; no panics;
    twin-run hash equality.
  - Shrinking turns a failure into a minimal command list, and that list is directly a regression replay.
- **Parser fuzzing** with `cargo-fuzz` 0.13.2 and libFuzzer, nightly toolchain, Linux CI only:
  - targets: the RON content loader (malformed files must give errors, never panics), the replay decoder, the
    scenario loader, and the BRP/CLI JSON command decoder
  - seed the corpora from `assets/base` and `tests/replays`
- **Scenario fuzzing:** random small scenarios (random units from the roster, random positions on random terrain
  patches) run 600 ticks and are checked against the invariants. This is the cheapest way to find pathing and vision
  edge cases at scale.
- **Skip AFL and `bolero` at first.** Proptest plus cargo-fuzz covers the need. `bolero` 0.13.7 is an option if you
  later want one harness for both.

### 4.8 L8: balance and simulation sweeps

- **`fl_cli sweep`** (a new command) takes a matrix spec: unit A × unit B × range bins × facing × terrain × N seeds. It
  emits JSON with win rate, time to kill (TTK) distribution, first-shot penetration rate and ammo used, each with a
  Wilson CI.
- **Sanity bands as soft gates:** each matchup has an expected band from the spec. Examples: "Pz III vs T-34 frontal
  at 300 m: Pz III wins 10–35%"; "the heavy tank beats the light tank ≥ 90%". Nightly reports flag violations. Only
  *gross* violations, like a rate going to 0% or 100% where the spec says contested, fail the milestone gate.
- **Lanchester sanity checks:**
  - For aimed fire between homogeneous groups, outcomes should roughly follow the square law: doubling the force
    should more than double fighting strength.
  - Fit attrition coefficients per unit type from sweep data (maximum likelihood, as
    [Stanescu, Barriga & Buro, AIIDE 2015](https://ojs.aaai.org/index.php/AIIDE/article/view/12780) did for
    StarCraft). Compare them with the "strength" implied by the stat tables.
  - Area fire (artillery, HE splash) should look closer to the linear law.
  - Large deviations from the expected law point to bugs in target selection, overkill or splash logic, not just
    balance problems.
  - Lanchester is a fast estimator, but less reliable than Monte Carlo in some cases
    ([Pollub study](https://ph.pollub.pl/index.php/jcsi/article/view/8353)). Use it as a smell test, not an oracle.
- **Automated balance research** (later, with an AI controller): self-play plus attribution of imbalance to parameters,
  as in [DeLaurentis et al., IEEE CoG 2021](https://www.universityxp.com/research/2022/5/11/toward-automated-game-balance-a-systematic-engineering-design-approach).
  Out of scope for iteration 1, since there's no AI yet.

### 4.9 L9: performance regression tests

The design (§5.7, §12) gates on a 20% wall-clock regression from criterion. On shared CI runners, and even on a dev
machine with background load, wall clock is noisy enough to cause false alarms or hide real regressions. A better
three-tier approach (my judgement, informed by instruction-counting practice on CI:
[CodSpeed](https://codspeed.io/docs/reference/codspeed-rust), [gungraun](https://github.com/gungraun/gungraun)):

1. **Deterministic work counters, as a hard gate at T/P.** The sim counts algorithmic work per tick:
   - path nodes expanded
   - LOS cells visited
   - spatial-query candidates examined
   - entities sorted
   - allocations, through a counting allocator in tests

   These counts are *exact and reproducible* because the sim is deterministic. A test asserts budgets on the scale
   scenario, for example "≤ 40k LOS cells per tick on average at 3,000 units", and snapshots the totals with insta.
   An agent can't hide an accidental O(n²) behind noise. This is the most agent-friendly perf gate there is.
2. **Instruction counts at P/N.** Run `gungraun` (Linux/Valgrind) or CodSpeed simulation mode on the `SimTick`
   benches. Trend it per PR, and fail on more than 5–10% (instruction counts are stable enough for tight thresholds).
3. **Wall clock at M.** Run `criterion` 0.8.2 and `fl_cli bench` on the *same dev machine* under a fixed power plan,
   with the process pinned and the median of 5 runs, recorded in `docs/perf/M<n>.json`. The 15 ms tick budget at
   3,000 units (G7) is checked here. Keep the "> 20% must be justified" rule, but on this controlled baseline only.

Also: the per-system profiler budgets you planned map straight onto tier 1. Make each budget a counter, not a time.

### 4.10 L10: client, visual and BRP-driven tests

- **Input → command** (P): `MinimalPlugins` tests feed synthetic input or picking events and assert the emitted
  `PlayerCommand`s (design §12 already plans this). Most client logic should be testable this way.
- **BRP integration** (N): Bevy 0.20 ships `examples/remote/app_under_test.rs` and `integration_test.rs`. The test
  connects over BRP, takes a screenshot through a spawned `Screenshot` entity plus `world.observe+watch`, finds a
  button and clicks it. Bevy's own CI runs it under `xvfb-run` on Linux with Mesa Vulkan. Copy this pattern for
  `fl_game`.

  **Caveat for this architecture:** BRP's built-in `world.*` methods act on the **main app World** (and the render
  sub-app). They don't reach a separate sim World. In 0.20, custom methods are registered with
  `RemotePlugin::with_method_main(name, system)`. Expose the sim through custom methods only:
  - `fl.step`, `fl.pause`
  - `fl.issue_command`, which goes through the command queue
  - `fl.inspect`, read-only
  - `fl.hash`, `fl.journal_tail`

  Never allow `world.mutate_components` on sim state, because that would bypass "commands are the only input".
- **Golden screenshots** (N, a few only):
  - Bevy's `bevy_ci_testing` feature reads a RON `CiTestingConfig` (via `CI_TESTING_CONFIG`) with frame-indexed
    events: `Screenshot`, `NamedScreenshot`, `MoveCamera`, `AppExit`, `Custom(String)`, and a `fixed_frame_time`
    setup. That's enough to script "load scenario X, step to tick T, screenshot".
  - Compare against goldens with `image-compare` or `dssim` and a tolerance. Software rasterizers differ slightly
    from GPUs, so keep separate goldens per backend: Linux lavapipe, Windows WARP.
  - Limit them to fog-of-war, terrain material and HUD layout. Everything else is checked better through sim state.
- **Agent-driven exploratory checks** (M): `bevy_brp_mcp` 0.23.0 (requires bevy ^0.20.0) lets a Claude session drive
  the running game. Use it for scripted demo checklists ("select the Pz III, order a move, screenshot, assert the
  position changed"). Don't make it the regression suite; the headless sim is.

---

## 5. Verification gates and CI

### 5.1 The gate matrix

The rule: **agents and CI run the same entry points.** Bevy does this with `cargo run -p ci -- test`, as seen in its
`ci.yml`. Use a `cargo xtask gate <level>` crate so a "verified locally" claim means exactly what CI checks.

| Check | Task (local, agent, Stop hook) | PR / merge (CI) | Nightly | Milestone | Release |
|---|---|---|---|---|---|
| `fmt --check`, `clippy -D warnings` (all targets) | ✅ | ✅ (3 OS) | | ✅ | ✅ |
| Determinism lints, deps-check, cargo-deny | ✅ (lints) | ✅ | | ✅ | ✅ (licences) |
| `data-check` and provenance ledger | ✅ | ✅ | | ✅ | ✅ |
| spec-lint, trace, generated-docs diff | ✅ (touched specs) | ✅ | | ✅ full audit | ✅ |
| Unit, property and formula-statistical tests (nextest) | ✅ affected crates + all of `fl_sim` | ✅ workspace, 3 OS | | ✅ | ✅ |
| Scenario and acceptance tests | ✅ touched area | ✅ all | ✅ | ✅ | ✅ |
| Held-out acceptance suite | ❌ (not visible) | ❌ | ✅ | ✅ | ✅ |
| Twin-run, shuffle (1 seed), system-order snapshot | ✅ | ✅ (3 shuffle seeds) | | ✅ | ✅ |
| Golden replays: bit-exact and semantic | ✅ | ✅ 3 OS | | ✅ re-bless review | ✅ full corpus |
| Test-diff guard (§5.4) | ✅ | ✅ (PR comment) | | | |
| proptest-state-machine command fuzz (short) | | ✅ (256 cases) | ✅ (20k cases) | ✅ | |
| cargo-fuzz parsers | | | ✅ 30 min / target (Linux) | | ✅ |
| cargo-mutants | ✅ `--in-diff` on touched sim crate, time-capped | ✅ `--in-diff` | ✅ full, sharded | ✅ missed-mutant review | |
| Coverage (llvm-cov) | | ✅ report | ✅ | ✅ review | |
| Rotating-seed statistical tests | | | ✅ | ✅ | ✅ |
| Work-counter budgets | ✅ | ✅ | | ✅ | ✅ |
| Instruction-count benches | | ✅ trend (Linux) | ✅ | ✅ | |
| Criterion wall clock and scale (3k units ≤ 15 ms) | | | | ✅ dev machine | ✅ |
| Balance sweeps (sanity bands) | | | ✅ report | ✅ review | ✅ |
| Input → command client tests | ✅ if client touched | ✅ | | ✅ | ✅ |
| BRP integration and golden screenshots | | | ✅ Linux lavapipe, Windows WARP | ✅ + macOS | ✅ 3 OS packaged builds |
| Soak (1 h game time, 3k units, twin) | | | weekly | ✅ | ✅ |
| Manual playtest checklist and demo checklist | | | | ✅ human | ✅ human |
| Credits / asset licence audit | | | | ✅ | ✅ |

**Per-task budget:** the task gate should take under 3–5 minutes on the dev machine, or agents will learn to dodge it.
Keep it fast:
- Sim crates don't compile `bevy_render`.
- Run the scenario suite for the touched area plus a fixed "smoke" set.
- Mutation testing is `--in-diff`, limited to the touched sim crate, with `--timeout`.

### 5.2 GitHub Actions layout

Get a remote (a private GitHub repo) in M0. Cross-platform determinism is a core requirement (G3), and right now only
Windows is ever exercised.

```yaml
# .github/workflows/ci.yml (sketch; pin actions by SHA like Bevy does)
env: { CARGO_TERM_COLOR: always, CARGO_INCREMENTAL: 0, CARGO_PROFILE_DEV_DEBUG: 0, RUSTFLAGS: "-C debuginfo=0 -D warnings" }
concurrency: { group: "${{ github.workflow }}-${{ github.ref }}", cancel-in-progress: true }
jobs:
  sim:            # fast: no renderer, no system deps
    strategy: { matrix: { os: [ubuntu-latest, windows-latest, macos-14] } }
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v7.0.1
      - uses: dtolnay/rust-toolchain@stable           # toolchain first: rust-cache keys on rustc version
      - uses: Swatinem/rust-cache@v2.9.3
        with: { key: sim }
      - uses: taiki-e/install-action@v2.87.30
        with: { tool: cargo-nextest }
      - run: cargo xtask gate pr-sim                   # nextest -p fl_math -p fl_protocol -p fl_content -p fl_sim -p 'fl_feat_*' (sim only),
                                                       # data-check, replay-verify, shuffle x3, work counters
  client:
    strategy: { matrix: { os: [ubuntu-latest, windows-latest, macos-14] } }
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v7.0.1
      - if: runner.os == 'Linux'
        run: sudo apt-get update && sudo apt-get install -y g++ pkg-config libx11-dev libasound2-dev libudev-dev libxkbcommon-x11-0 libwayland-dev libxkbcommon-dev
      - uses: dtolnay/rust-toolchain@stable
        with: { components: "clippy, rustfmt" }
      - uses: Swatinem/rust-cache@v2.9.3
        with: { key: client }
      - run: cargo xtask gate pr-client                # fmt, clippy --workspace --all-targets, client tests, build fl_game
  lint-meta:      # Linux only
    runs-on: ubuntu-latest
    steps: [ checkout, toolchain, cache,
             "EmbarkStudios/cargo-deny-action@v2.1.1",
             "cargo xtask spec-lint && cargo xtask trace && cargo xtask docs --check",
             "git diff origin/main > git.diff && cargo mutants --in-diff git.diff -p fl_sim --test-tool nextest --in-place",
             "cargo llvm-cov nextest --workspace --exclude fl_game --lcov --output-path lcov.info  # report only" ]
```

Nightly workflow:
- full `cargo mutants --shard k/8 --sharding round-robin --baseline skip` matrix (shards as in the
  [mutants.rs sharding docs](https://mutants.rs/shards.html))
- cargo-fuzz targets
- rotating seeds, held-out suite and sweeps
- instruction-count benches
- visual tests

**Headless GPU options** (verified from Bevy's and wgpu's current workflows):
- **Linux:**
  - Install `mesa-vulkan-drivers` and `libgl1-mesa-dri`, and run under `xvfb-run`. Bevy installs a newer Mesa from
    the `kisak/turtle` PPA and caches the apt payload.
  - This is the cheapest and most reproducible screenshot backend: lavapipe/llvmpipe.
  - Expect a slow first frame on software Vulkan. One project reports about 6 s
    ([bglga notes](https://git.hoyer.xyz/harald/bglga/commit/c6dcf9d72838bde32318f2318bf3cb5cc30c98a9)), so script
    screenshots by frame or tick, never by wall time.
- **Windows:** Bevy runs its examples on `windows-latest` with `WGPU_BACKEND=dx12`, which falls back to Microsoft's
  WARP software adapter. wgpu's GPU-test job installs WARP and Mesa explicitly on Windows.
- **macOS:** Apple-silicon runners (`macos-14`) expose a GPU, and Bevy and wgpu both run rendering tests there. There
  are reports of intermittent Metal failures on Intel and virtualized runners
  ([runner-images #9254](https://github.com/actions/runner-images/issues/9254)), so keep macOS visual tests
  non-blocking.
- **Caching:**
  - `Swatinem/rust-cache` caches dependencies, not workspace crates, and sets `CARGO_INCREMENTAL=0`.
  - Separate cache keys for the `sim` and `client` jobs stop the Bevy-renderer build from evicting the small sim
    build.
  - Bevy also strips debuginfo in CI (`-C debuginfo=0`, `CARGO_PROFILE_DEV_DEBUG: 0`), which shrinks caches a lot.
  - Add `mozilla-actions/sccache-action` v0.0.11 only if build times hurt.
- **Windows MAX_PATH:** set the same short `target-dir` in CI, or keep the default on runners, which have short work
  paths. Make sure `.cargo/config.toml` doesn't hardcode `C:/t/fl` on non-Windows hosts. Use an env-specific config
  or `CARGO_TARGET_DIR` locally instead.

### 5.3 Milestone gate contents

These are in addition to the matrix above.
1. **Demo checklist automated:** `fl_cli demo M2` runs each demo item headless (scenario + assertions). A
   `bevy_brp_mcp`-driven script then repeats the visual items in the real client and stores screenshots in
   `docs/milestones/M2/`. A human watches the client run, using the manual checklist (§7.3).
2. Held-out suite green. Rotating seeds green for 7 consecutive nightlies.
3. Perf baseline recorded on the dev machine, with work-counter totals snapshotted.
4. Full mutation run: every *missed* mutant in `fl_sim` and feature sim modules is either killed by a new test or
   recorded in `.cargo/mutants.toml` `exclude_re` with a justification.
5. Re-blessed goldens reviewed (the §6.3 report).
6. Spec audit: every REQ in the milestone's specs is `implemented`, trace is complete, and Guess entries are reviewed.
7. Coverage review of acceptance tests per mechanic. Uncovered branches in sim code get a test or a reason.

### 5.4 Stopping agents from cheating the tests

Threats, all documented for frontier models
([ImpossibleBench](https://arxiv.org/abs/2510.20270), [EvilGenie](https://arxiv.org/abs/2511.21654),
[SpecBench](https://arxiv.org/abs/2605.21384)):
- editing or weakening assertions
- deleting tests or adding `#[ignore]`
- special-casing test inputs or seeds
- hardcoding expected outputs
- widening tolerances
- reducing proptest case counts
- over-mocking, so the test exercises the mock instead of the code
- auto-accepting snapshots
- for statistical tests, tuning to the fixed seeds

Controls, from cheapest to most expensive:

| Control | Implementation |
|---|---|
| **Protected paths** | `tests/acceptance/**`, `tests/holdout/**`, `tests/replays/**`, `**/snapshots/**`, `scenarios/**` and `docs/design/mechanics/**`. (1) A PreToolUse hook on `Edit\|Write` denies them for implementer sessions: exit 2, or `permissionDecision: "deny"` ([hooks docs](https://code.claude.com/docs/en/hooks)). (2) CODEOWNERS requires human review on GitHub. (3) A Bash-matcher hook blocks `cargo insta accept`, `INSTA_FORCE_PASS`, `INSTA_UPDATE=always` and `git checkout -- tests/` |
| **Test-diff guard** | `xtask test-guard` diffs test code against the merge base and flags removed or changed `assert*` lines, new `#[ignore]` / `#[should_panic]`, tolerance or `ALPHA` constants changing, `ProptestConfig::cases` dropping, deleted test functions, a falling `cargo nextest list` count, or a new `seed_base`. Findings go to the reviewer, who must accept each one explicitly. |
| **Held-out tests** | §2.2. The implementer can't fit what it can't see |
| **Spec-derived oracles** | Expected values come from the spec or the f64 oracle, never from running the implementation. The reviewer checks that the test's expected numbers appear in the spec |
| **No mocks in sim tests** | Policy: sim tests use the real `build_sim_app` with fixture content (`fl_test_utils`). Mocks are allowed only at the client/OS boundary. A clippy-style grep in `test-guard` flags `mock`/`fake` types in sim test files |
| **Mutation testing** | `cargo mutants --in-diff` per PR. A test that doesn't kill the mutants of the code it claims to cover is weak, whoever wrote it. Meta's ACH work shows mutation-guided test generation catches real faults at industrial scale ([FSE 2025](https://arxiv.org/abs/2501.12862)) |
| **Deterministic Stop gate** | Stop/SubagentStop hook runs `cargo xtask gate task`. `decision: "block"` with the failure summary keeps the agent working until it passes. Cap retries and escalate. The docs warn the check must be able to pass, or the hook loops forever |
| **Evidence, not assertions** | The task report pastes the gate output. The reviewer reads the evidence instead of re-running (best practices: "Have Claude show evidence rather than asserting success") |
| **Escape hatch** | If an implementer believes a protected test is wrong, it writes `BLOCKED: test X contradicts spec Y because …` and stops. The lead or human decides. This keeps legitimate test fixes possible without silent edits |

Evaluate [TDD Guard](https://github.com/nizos/tdd-guard) (a Claude Code hook with a Rust reporter that blocks
implementation without a failing test) before building your own. Your superpowers TDD skill already covers most of
what it does. The protected-paths hook and `test-guard` are the parts that are missing.

**cargo-mutants details:**
- `--test-tool nextest` (nextest fails fast, which suits mutation runs)
- `--in-diff git.diff` per PR ([mutants.rs/in-diff](https://mutants.rs/in-diff.html)). The docs say it's a speed-up,
  not a replacement for full runs.
- `--iterate` locally to work through the missed list
- `.cargo/mutants.toml` with `exclude_re` for `impl Debug`, `impl Display` and the client crates
- Default timeouts are 5× the baseline, minimum 20 s, with `--timeout` and `--minimum-test-timeout` overrides
  ([mutants.rs/timeouts](https://mutants.rs/timeouts.html))
- Target: zero missed mutants in `fl_math`, combat formulas and content validation; triaged in the rest of the sim;
  not tracked for the client.

**Coverage:** `cargo llvm-cov nextest` works on `x86_64-pc-windows-msvc`. Branch coverage needs nightly
([cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov)). Use it **diagnostically**: a line-coverage threshold
invites assertion-free tests, which mutation testing catches and coverage doesn't. If you want a number, gate on "no
new uncovered lines in sim crates" in the PR diff, not a global percentage.

### 5.5 Review checklist: determinism and ECS

Give this to the reviewer subagent for any diff touching a sim crate. Each item is a yes/no question.

**Determinism**
1. Any `f32`/`f64`, `as f*`, or float-returning math in sim code, including "temporary" debug math? (The lint should
   catch it; the reviewer confirms there's no `#[allow]`.)
2. Any `HashMap`/`HashSet` (std or `bevy_platform`) in sim state or in iteration that affects state?
3. Does any state-affecting query iterate without sorting by `SimId` (archetype order changes as components are
   added and removed)?
4. Do RNG draws happen in a documented, fixed order? Does the spec's RNG-order requirement (COMBAT.PEN.5 style) match
   the code? Does a parallel phase touch the shared RNG instead of deriving `(seed, tick, SimId, stream)`?
5. Does a `par_iter` or parallel compute write anything other than a `SimId`-indexed result buffer that a sequential
   phase then applies in `SimId` order?
6. Do new systems have explicit `.in_set` / `.before` / `.after`? Does the system-order snapshot diff match the
   intent?
7. Any `Local<T>` in sim systems? It's hidden state that isn't hashed or saved, so it's banned or must be registered.
8. Any reliance on change detection (`Changed`, `Added`) or `RemovedComponents` for game logic? Change ticks aren't in
   the state hash or saves, and removal buffers have frame semantics that break when `SimTick` runs k times per frame.
   Use explicit dirty flags in hashed state.
9. Is `Entity` stored in hashed state, commands, journal or replays instead of `SimId`?
10. Do observers or hooks mutate sim state? If so, is their firing order defined and tested?
11. Is new sim state (components, resources) registered for the state hash, and for saves when they exist?
12. Do tie-breaks have a total order ending in `SimId`?
13. Is integer overflow possible at scale (3,000 units, 2 km, long matches)? Is checked arithmetic on in debug builds?

**ECS and extensibility**
14. Does the change edit a core crate or a central enum to add a mechanic? That's an anti-goal; it should be a feature
    crate plus def components.
15. Does a new def component have a schema with `deny_unknown_fields`, validation with path-aware errors,
    `instantiate`, hashing registration, generated docs and a data-check fixture?
16. Are magic numbers in code? Every constant belongs in `rules.ron` or a def, with provenance.
17. Does client code mutate sim state, or does sim code read client, render or input state?
18. Are there per-tick allocations, or all-entity scans that aren't budgeted or time-sliced (design §5.7)?
19. Do new journal events carry enough data to assert the REQ without reaching into internals?

---

## 6. Change-management playbook

### 6.1 Classify every change first

| Class | Example | Hashes change? | What has to happen |
|---|---|---|---|
| **R: Refactor** | Extract a function, rename, restructure a crate | **No, must be bit-identical** | All goldens pass unchanged. This is the strongest refactor safety net you have. Any hash change means it's not a refactor |
| **D: Data tuning** | Pz III front armor 40→45 | Yes, for affected scenarios (rules hash changes) | Provenance updated (`Tuned`, linked to a sweep), sanity bands checked, goldens that use that unit re-blessed through the report |
| **F: Formula or rule change** | Move the cover save after the penetration roll | Yes, often everywhere (RNG order) | MCR, spec version bump, `SIM_VERSION` bump, acceptance tests updated **in the spec first**, full re-bless with report, human approval |
| **N: New mechanic** | `fl_feat_weather` | Only for scenarios that enable it | New spec, new feature crate, new scenarios and goldens. Existing goldens must stay **bit-identical** unless the feature is on (this proves extensibility, G2) |
| **E: Engine or crate upgrade** | Bevy 0.20.x patch, `fixed` bump | Should be no | Treated as R. Any hash change is investigated before re-blessing |
| **X: Experiment** | Suppression v2 behind a flag | Only with the flag on | §6.5 |

The class goes in the commit trailer (`Change-Class: F`, `MCR: MCR-014`). CI uses it: class R or E with changed golden
hashes fails outright.

### 6.2 The Mechanic Change Request (MCR)

A half-page entry in `docs/design/changelog-mechanics.md`:
- why (a playtest or feedback link)
- REQs added, changed or removed
- expected effect on outcomes, for example "Pz III frontal pen vs T-34 drops from about 30% to about 20%"
- the flag name, if experimental
- the sweep artifact, for tuning
- the `SIM_VERSION` bump: yes/no

The expected effect is written **before** implementation. The re-bless report then has to confirm it. If the observed
effect differs from the prediction, that's a finding, not a re-bless.

### 6.3 Versioned golden replays and the re-bless protocol

**Versioning:**
- `SIM_VERSION: u32` lives in `fl_sim`. It's bumped on every class F change and on any intentional change to RNG
  order or scheduling.
- The replay header stores `sim_version`, `rules_hash`, `content_version` and `feature_flags`.
- The **rules hash** covers content only. Code semantics are covered by `SIM_VERSION`. The future lobby handshake
  should compare both.
- **Commands are the durable artifact; hashes are derived.** A golden replay's command stream (plus scenario and seed)
  survives a version bump. Only its expected hash stream is regenerated.

**Re-bless protocol** (`fl_cli rebless --reason MCR-014`):
1. Precondition: `SIM_VERSION` or the content changed in this branch, and an MCR exists. Otherwise refuse.
2. Re-run every golden's command stream on the new build. **Semantic assertions must pass**, unless the MCR lists them
   as intentionally changed and the spec AC was updated.
3. Write `target/rebless-report.md`. For each replay:
   - first divergent tick
   - journal summary diff (event counts by type)
   - outcome diff (winner, survivors, objective ticks)
   - key-metric deltas (penetration rate, TTK) compared with the MCR's predicted effect
4. Update the hash files under `tests/replays/`. The file header gets the new `sim_version`.
5. A human (or the lead plus a fresh-context reviewer) reads the report and approves it. CODEOWNERS on
   `tests/replays/**` enforces that.
6. **Old replays:** keep the tagged binary per `SIM_VERSION`, using git tags `sim-v7` and so on. A replay from an old
   version is verified with `cargo run --locked` at that tag, as an archive job, or it gets migrated by re-recording.
   Players' saved replays later need a "requires version N" message. Bit-exact backward compatibility isn't a goal.

**CI rules:**
- golden hash files changed ⇒ `SIM_VERSION` changed or the content changed, **and** an MCR ID is in the commit trailer
- `SIM_VERSION` changed ⇒ at least one golden changed, which proves the bump wasn't decorative

### 6.4 Impact analysis before changing a mechanic

`fl_cli impact <REQ-or-def-key-or-system>` (new) answers "what will this change touch?":
- **By REQ:** the trace matrix gives the tests and scenarios.
- **By def key** (for example `Armor.front`): scan scenarios and goldens for units whose resolved defs use that key.
- **By system:** run the golden corpus once with per-system work counters (§4.9) to build a `system → scenarios where
  it did non-zero work` map, cached nightly. The scenarios where `fl_feat_supply::morale` actually ran are the ones a
  morale change can affect.
- The output feeds the plan. Tasks name the scenarios whose goldens are expected to change. Any golden that changes
  *outside* that predicted set is flagged in the re-bless report as unexpected.

### 6.5 Feature toggles for experimental mechanics

Two levels:
- **Compile-time:** feature crates registered in `GameBuilder` already handle "is the mechanic present at all".
- **Runtime experiments** live in `rules.ron`:
  `experiments: { "suppression_v2": false, "flowfield_groups": true }`. Scenarios can override them.
  - Flags are **part of the rules hash** and are stored in replay headers, so a replay always runs with the flags it
    was recorded with.
  - Read flags only at build time (`build_sim_app`): choose which systems to add, or put a run condition on a
    `Res<Experiments>` that's in the hash. Never read them from env vars, wall time or per-player randomness.
  - **Both arms are tested while the flag lives.** CI runs the acceptance suite for that mechanic with the flag on and
    off. Goldens are recorded for the default arm, and a small set for the experimental arm.
  - Every flag has an owner, an MCR and a **removal milestone** in the GDD registry. `spec-lint` warns when a flag
    outlives its milestone. Stale flags are a well-documented source of debt
    ([Eppo, 2024](https://www.geteppo.com/blog/feature-flag-best-practices)).
  - In the testbed, an egui panel toggles flags live. This changes the rules hash, so it's marked "testbed-only",
    just like hot reload.

### 6.6 A/B balancing in the testbed

- `fl_cli sweep --variant A=rules.ron --variant B=rules_prone_v2.ron --matrix sweeps/infantry.ron --seeds 0..500`.
- **Common random numbers:** both variants run on the same seed for each trial. A paired comparison (McNemar for win
  or loss; a paired difference for TTK) cuts variance a lot compared with independent samples. The sim's per-stream
  RNG design helps here: if the variant doesn't change draw counts in a stream, pairs stay closely coupled.
- Output JSON and a short Markdown table: per-matchup win rate A vs B with CIs, the delta and its CI, TTK
  distributions, and a Lanchester coefficient refit.
- The decision is written into the MCR. The winning values get `Tuned(mcr, sweep)` provenance. The sweep JSON is
  committed under `sweeps/` so the tuning is reproducible.
- In the testbed client: a split-screen or alternating "A/B duel" mode spawns the same scenario under both rule sets
  for a human feel check. Feel matters, and numbers alone don't capture it.

### 6.7 Migrating content data

- `mod.ron` carries `schema_version`. `fl_content` has a chain of migrations `fn v3_to_v4(ron::Value) -> ron::Value`,
  run by `fl_cli migrate <dir>`. Migrations are pure, tested with before/after fixtures, and they're also how
  third-party mods in `mods/` stay loadable.
- Never add a required field silently. Either give it a serde default that's **recorded in the spec and provenance**,
  or bump `schema_version` and migrate.
- After migration, `data-check` must pass, and the **rules hash must be unchanged** unless the MCR says otherwise.
  This proves the migration is semantics-preserving.

---

## 7. Playtesting and the telemetry loop

### 7.1 In-game capture ("F8 = report")

One key press writes `bugs/inbox/BUG-20261123-142233/` containing:
- `screenshot.png`, taken just before the dialog opens with the HUD optionally hidden (common practice:
  [Game Developer](https://www.gamedeveloper.com/game-platforms/in-game-bug-reporter-best-practices))
- `replay.ron`: seed, scenario, rules hash, `SIM_VERSION`, flags and the full command stream up to now. This makes any
  bug reproducible headless, which is your biggest advantage over most games.
- `state.json`: the `fl_cli inspect` dump at the current tick (all registered sim components by `SimId`)
- `journal_tail.ron`: the last N ticks of journal events, including every roll
- `view.ron`: camera pose, selected SimIds, active overlays and the controlled player
- `meta.ron`: git SHA, dirty flag, OS, GPU adapter, build profile, and the free-text description plus a category
  (bug / balance / feel / clarity / idea)

The same bundle comes from `fl_cli` when a scenario test or fuzz case fails, so humans and machines report bugs the
same way.

### 7.2 From bug report to regression test

The pipeline is a skill, `/triage-bug <bundle>`, run by an agent:
1. **Reproduce headless:** `fl_cli replay-verify` up to the report tick. Confirm the reported condition from
   `state.json` and the journal, for example "unit 1043 inside a river cell".
2. **Write the assertion first:** turn the report into a predicate over state or journal. This is the oracle. If it
   can't be stated mechanically (a "feel" report), it goes to the feedback ledger instead.
3. **Minimize:** delta-debug the command stream (ddmin over commands, then over tick ranges) and the scenario (drop
   units the predicate doesn't depend on). The proptest-state-machine shrinker does this automatically for fuzz-found
   bugs.
4. **Find the cause:** `git bisect run fl_cli replay-check-predicate …` finds the commit that introduced it. For a
   desync, `bisect-desync` finds the first divergent tick and component, as Factorio did with per-tick CRCs and
   snapshots.
5. **Commit the regression test:** the minimized `tests/regressions/BUG-…/{scenario.ron, replay.ron, predicate}`,
   picked up automatically by `datatest-stable`. It **must fail before the fix**, and the evidence is pasted.
6. Fix it through the normal task flow. The regression folder becomes protected after merge.

### 7.3 Playtest checklists and design feedback

- **Per-milestone manual checklist** (`docs/milestones/M<n>/playtest.md`): about 10–20 items, each phrased as an
  observable. For example: "a tank firing from forest edge reveals itself", "the truck visibly slows off-road", "the
  info panel penetration numbers match the combat log". Items link to REQ IDs so a failed item points at a spec.
- **Feedback ledger** (`docs/design/feedback-ledger.md`): each entry has the date, source, category, bundle link,
  observation (what happened), interpretation (why it might feel wrong) and outcome (bug / MCR / won't-do). Brainstorm
  sessions start from open ledger entries, and MCRs cite them. That closes the loop from design feedback to spec.
- **Local telemetry, no network:** every match writes `match_stats.json` from the journal: per-unit-type TTK,
  penetration rates, shots per kill, distance travelled, time under suppression, and supply shortfall time. `fl_cli
  stats aggregate playtests/*.json` compares them with the spec's statistical targets and the sweep bands. A "feels too
  lethal" report then becomes "observed TTK is 40% under the sweep band at ranges under 200 m", which an MCR can act
  on.
- **External playtesters** (later): `sentry` 0.49.3 for crash reports, with the same bundle attached; opt-in only.

---

## 8. Critique of the current process

What's strong:
- spec → plan → tasks
- TDD per task
- verification before claiming completion
- milestone demo checklists
- a determinism twin-run
- a headless CLI with JSON output, an event journal that records rolls, and BRP control

The debug and observability plan in particular is excellent, and better than most studios'.

Gaps and risks, most important first:

1. **Self-graded tests.** The implementer subagent writes the tests for its own code. Spec-compliance review is an
   LLM reading code against prose and can't run anything. This is exactly the setup where SpecBench sees held-out pass
   rates fall below visible pass rates as code grows. *Fix:* §2.2 (separate acceptance tests, protected and held out)
   and §5.4.
2. **The specs aren't machine-checkable yet.** The design doc is good prose, but its acceptance criteria are things
   like "penetrates about X% of the time". Without IDs, exact oracles and tolerance policies, "spec compliance" can't
   be checked mechanically. *Fix:* the §3.2 template plus spec-lint and trace.
3. **Determinism checks come too late and too thin.** A twin-run per milestone (or per scenario test) misses
   order-dependence that only shows up under a different schedule. *Fix:* shuffle seeds, the system-order snapshot and
   cross-OS goldens per PR (§4.5). They're all cheap, and Bevy 0.20 already has `shuffle_seed`.
4. **No CI until there's a remote.** All verification is local on Windows, run by the same agents whose work it
   checks. *Fix:* a private remote and the three-OS CI in M0.
5. **The perf gate is noisy.** A 20% wall-clock threshold on criterion will either cry wolf or miss real regressions.
   *Fix:* deterministic work counters as the gate, instruction counts for trends, and wall clock only for milestone
   baselines on a controlled machine (§4.9).
6. **Statistical tests have no policy.** Without an oracle source, an α budget and a seed-reroll rule, agents will
   paste measured rates into tests or widen tolerances until they pass. *Fix:* §4.4.
7. **Golden replay maintenance isn't designed.** "A golden replay test with stored hashes" will break on the first
   RNG-order change, and agents will re-bless without thinking. *Fix:* `SIM_VERSION`, semantic assertions, the
   re-bless report and CODEOWNERS (§6.3).
8. **Two LLM reviews on every task is the most expensive step and often redundant.** Code-quality review overlaps
   clippy and fmt. For data-only or client-cosmetic tasks, spec review adds little. *Fix:* risk-tiered review (below).
9. **Parallel subagents share the schedule contract.** From M4, parallel feature crates each add systems to shared
   `SimSet`s. Order conflicts surface as desyncs much later. *Fix:* the system-order snapshot (insta), so every
   schedule change is a visible diff in review; ambiguity detection at Error level from M0.
10. **Gaps marked as guesses aren't tracked.** Design §7 says guesses are "marked" in `rules.ron`, but RON comments
    vanish on parse and nothing enforces them. *Fix:* the provenance ledger checked by data-check (§3.3).
11. **Milestones are big.** M2 alone packs about 15 mechanics. The demo checklist at the end is too coarse a feedback
    loop for agents. *Fix:* each mechanic spec gets its own mini-gate (its ACs green, plus its scenario in the demo
    runner), so a milestone becomes a collection of already-verified mechanics.
12. **Hidden-state hazards specific to Bevy** (`Local<T>`, change detection, `RemovedComponents`, observers, archetype
    iteration order) aren't in the determinism rules. *Fix:* the §5.5 checklist, and the save/load parity test once
    saves exist.

**Wasted or low-value steps to cut or shrink:**
- A full code-quality review on every task. Replace it with clippy (pedantic subset), the test-guard and a combined
  review, and keep the deep quality review for sim-core and public-API tasks.
- Writing plans that restate the spec. A task should *reference* REQ IDs and paste only the relevant spec section.
  Long plans eat agent context, which the Claude Code docs call the binding constraint.
- Manual demo checklists that re-verify what headless scenarios already prove. Automate those, and keep manual
  checks for feel and visuals.

**Risk-tiered review:**

| Task touches | Review |
|---|---|
| Docs, data only (with data-check green), client cosmetics | One combined reviewer, a short checklist |
| Client logic | Combined spec and quality review |
| Sim core, feature sim modules, `fl_math`, `fl_content` validation, schedule | Spec review, quality review **and** determinism/ECS checklist review (§5.5), all in fresh context, all seeing only the diff, the spec section and the gate evidence |
| Protected tests, goldens, specs | A human (CODEOWNERS) |

---

## 9. Top 10 prioritized improvements

| # | Improvement | Effort | Why it's first |
|---|---|---|---|
| 1 | **Protected, independently written acceptance tests plus a held-out slice.** Add a stage 3 test-author agent, a PreToolUse hook denying protected paths to implementers, Bash-hook bans on `insta accept` and force-pass, CODEOWNERS, and the "BLOCKED" escape hatch | M | It removes the main way agent-built code fails (self-graded tests) |
| 2 | **Machine-checkable mechanic specs.** The §3.2 template, stable REQ IDs in EARS form, spec worked examples becoming `rstest` tables, `xtask spec-lint` and `xtask trace` in the task gate | M | Turns "spec compliance" from an opinion into a check; makes item 1 possible |
| 3 | **`cargo xtask gate {task,pr,nightly,milestone}` plus a Stop/SubagentStop hook running `gate task`.** The same commands locally and in CI, with evidence pasted in reports | S | Makes verification before completion deterministic instead of advisory |
| 4 | **Determinism shifted left.** `ambiguity_detection: Error`, a 1–3 seed `shuffle_seed` run, the insta snapshot of `SimTick` system order, determinism clippy lints and deps-check, all from M0 | S | Cheap, and it catches order bugs at the commit that causes them, which matters most once M4+ parallel feature crates start |
| 5 | **Remote plus three-OS GitHub Actions in M0.** A split sim/client job, rust-cache, Linux lavapipe and Windows WARP for nightly screenshots, cross-OS golden replay verification | M | G3 (determinism) is unverified until a second OS and architecture run it |
| 6 | **Statistical test policy and helpers.** Spec-derived oracles, `assert_rate_within_wilson` at α = 10⁻⁴, two tiers (formula and scenario), nightly rotating seeds, held-out seeds, the one-time reroll rule | S | Stops agents pasting measured rates or widening tolerances; makes randomized mechanics verifiable |
| 7 | **Golden replay versioning and re-bless protocol.** `SIM_VERSION`, semantic assertions next to hashes, `fl_cli rebless` with a report, CI rules tying golden changes to MCRs and change classes (refactors must be bit-identical) | M | Lets you change mechanics safely, which is the user's stated goal, without goldens decaying into rubber stamps |
| 8 | **Mutation testing on sim crates.** `cargo mutants --in-diff` per PR and per task (time-capped), a full sharded run nightly, missed-mutant triage at milestones. Coverage stays diagnostic | S–M | It's the one tool that measures whether the tests (agent-written or not) actually detect faults |
| 9 | **Deterministic work-counter perf gates** (path nodes, LOS cells, query candidates, allocations) snapshotted and budgeted per task. Instruction counts per PR. Criterion only for milestone baselines | M | Replaces a noisy 20% wall-clock gate with an exact one agents can't argue with; protects G7 |
| 10 | **Provenance ledger plus F8 bug bundle → minimized regression replay pipeline** (`/triage-bug` skill, ddmin, `datatest-stable` regression folder) and the feedback ledger feeding brainstorming | M–L | Closes the design ↔ playtest loop and turns every bug into a permanent test; makes "our guess vs original" auditable |

Close runners-up:
- runtime experiment flags in the rules hash, with both arms tested (S)
- `fl_cli sweep` with common random numbers and Lanchester refits (M)
- risk-tiered review to cut cost (S)
- per-mechanic mini-gates inside milestones (S)
- the BRP custom methods (`fl.*`) plus `bevy_brp_mcp` for agent-run demo checklists (M)

---

## 10. References

**Agent behaviour and agentic workflow**
- Claude Code best practices (verification, writer/reviewer, adversarial review, hooks): https://code.claude.com/docs/en/best-practices
- Claude Code hooks reference (PreToolUse deny, Stop/SubagentStop block): https://code.claude.com/docs/en/hooks
- ImpossibleBench: measuring LLMs' propensity to exploit test cases (Zhong, Raghunathan, Carlini; ICLR 2026): https://arxiv.org/abs/2510.20270
- SpecBench: measuring reward hacking in long-horizon coding agents (2026): https://arxiv.org/abs/2605.21384
- EvilGenie: a reward hacking benchmark (2025): https://arxiv.org/abs/2511.21654
- Claude 3.7 Sonnet system card (special-casing tests): https://www.anthropic.com/claude-3-7-sonnet-system-card
- Mutation-guided LLM-based test generation at Meta (ACH; FSE 2025): https://arxiv.org/abs/2501.12862
- TDD Guard (Claude Code TDD hook): https://github.com/nizos/tdd-guard
- GitHub Spec Kit: https://github.github.com/spec-kit/ (release v1.2.3, 2026-10-22)
- Kiro specs: https://kiro.dev/docs/specs/ ; spec-driven development overview: https://www.morphllm.com/spec-driven-development
- InfoWorld review of spec-driven development tools (2026): https://infoworld.com/article/4171332/four-cutting-edge-tools-for-spec-driven-development.html
- EARS requirement syntax: https://alistairmavin.com/ears/

**Deterministic simulation, replays and game testing**
- Factorio FFF #60 "Tests all around": https://factorio.com/blog/post/fff-60
- Factorio FFF #47 "CRC fun": https://factorio.com/blog/post/fff-47
- Factorio FFF #62 "The automation of Factorio": https://cf-www.factorio.com/blog/post/fff-62
- Riot: Determinism in League of Legends (classic): https://technology.riotgames.com/news/determinism-league-legends-introduction
- 1500 Archers on a 28.8 (Age of Empires, classic): https://www.gamedeveloper.com/programming/1500-archers-on-a-28-8-network-programming-in-age-of-empires-and-beyond
- Automated Testing of Gameplay Features in Sea of Thieves (GDC 2019, classic): https://gdcvault.com/play/1026366/Automated-Testing-of-Gameplay-Features
- In-game bug reporter best practices: https://www.gamedeveloper.com/game-platforms/in-game-bug-reporter-best-practices
- Replay-based regression suites (practitioner guide; vendor blog): https://bugnet.io/blog/how-to-build-a-replay-based-regression-test-suite
- Metamorphic testing for simulation V&V (NIST-listed): https://csrc.nist.gov/pubs/conference/2021/06/02/metamorphic-testing-on-the-continuum-of-verificati/final
- Metamorphic testing and Given-When-Then for simulation models (Modelica 2025): https://ecp.ep.liu.se/index.php/modelica/article/view/1300
- Seeds and flakiness in ML projects (Dutta et al., ICST'22): https://cs.cornell.edu/~saikatd/papers/seeds-icst22.pdf
- Arcuri & Briand, statistical tests for randomized algorithms in SE: https://pure.ul.ie/en/publications/a-hitchhikers-guide-to-statistical-tests-for-assessing-randomized/
- Approval testing / golden master guide: https://qaskills.sh/blog/approval-testing-golden-master-guide

**Balance**
- Stanescu, Barriga, Buro, "Using Lanchester Attrition Laws for Combat Prediction in StarCraft" (AIIDE 2015): https://ojs.aaai.org/index.php/AIIDE/article/view/12780
- Combat models for RTS games: https://arxiv.org/abs/1605.05305
- Lanchester vs Monte Carlo encounter prediction: https://ph.pollub.pl/index.php/jcsi/article/view/8353
- Toward automated game balance (CoG 2021, summary): https://www.universityxp.com/research/2022/5/11/toward-automated-game-balance-a-systematic-engineering-design-approach
- Feature flag hygiene (Eppo, 2024): https://www.geteppo.com/blog/feature-flag-best-practices

**Rust and Bevy tooling**
- Bevy 0.20 source (checked): `crates/bevy_ecs/src/schedule/schedule.rs` (`ScheduleBuildSettings`: `ambiguity_detection`, `shuffle_seed`), `crates/bevy_dev_tools/src/ci_testing/config.rs` (`CiTestingConfig`), `crates/bevy_remote/src/lib.rs` (`with_method_main`), `examples/remote/integration_test.rs`: https://github.com/bevyengine/bevy/tree/v0.20.0
- Bevy CI workflows (three-OS matrix, caching, `cargo run -p ci`, xvfb plus Mesa, Windows DX12, pixel-eagle screenshots): https://github.com/bevyengine/bevy/blob/main/.github/workflows/ci.yml , https://github.com/bevyengine/bevy/blob/main/.github/workflows/example-run.yml
- Bevy Linux dependencies: https://github.com/bevyengine/bevy/blob/v0.20.0/docs/linux_dependencies.md
- wgpu CI (WARP and Mesa on Windows, Mesa on Linux, macos-14 GPU tests): https://github.com/gfx-rs/wgpu/blob/trunk/.github/workflows/ci.yml
- Headless Bevy screenshots with lavapipe (bglga): https://git.hoyer.xyz/harald/bglga/commit/c6dcf9d72838bde32318f2318bf3cb5cc30c98a9
- GitHub runner-images Metal/MPS issue: https://github.com/actions/runner-images/issues/9254
- bevy_brp (MCP server and extras): https://github.com/natepiano/bevy_brp
- cargo-mutants book: https://mutants.rs/ (CI: https://mutants.rs/ci.html , in-diff: https://mutants.rs/in-diff.html , shards: https://mutants.rs/shards.html , timeouts: https://mutants.rs/timeouts.html , nextest: https://mutants.rs/nextest.html , iterate: https://mutants.rs/iterate.html)
- cargo-llvm-cov: https://github.com/taiki-e/cargo-llvm-cov
- nextest retries and flaky handling: https://nexte.st/docs/features/retries/
- insta advanced settings (CI behaviour, unreferenced, glob): https://insta.rs/docs/advanced/
- proptest state-machine testing: https://proptest-rs.github.io/proptest/proptest/state-machine.html
- Clippy lint configuration (disallowed-types / disallowed-methods): https://doc.rust-lang.org/stable/clippy/lint_configuration.html
- CodSpeed Rust: https://codspeed.io/docs/reference/codspeed-rust ; gungraun: https://github.com/gungraun/gungraun
- mantra (requirement tracing): https://docs.rs/mantra
- crates.io API (all version numbers in §1, queried 2026-10-23): https://crates.io/api/v1/crates/<name>
