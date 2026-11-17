# Next iterations: Last Light (horde-defense)

Status: **proposal, no direction chosen** (2026-11-17). Written after the production pass merged into local `main` as `713bd877`. The production pass's plan of record stays [2026-11-05-production-pass.md](2026-11-05-production-pass.md) ("the production plan" below). Nothing in this file is built or measured unless it says so.

## Contents

1. [Purpose and status](#1-purpose-and-status)
2. [What was checked on 2026-11-17](#2-what-was-checked-on-2026-11-17)
3. [Binding constraints carried from the production pass](#3-binding-constraints-carried-from-the-production-pass)
4. [Direction A: late-game difficulty](#4-direction-a-late-game-difficulty)
5. [Direction B: review-led polish](#5-direction-b-review-led-polish)
6. [Direction C: new content](#6-direction-c-new-content)
7. [Direction D: open technical work](#7-direction-d-open-technical-work)
8. [Conflicts and dependencies between directions](#8-conflicts-and-dependencies-between-directions)
9. [Recommended order](#9-recommended-order)
10. [Decisions needed from the user](#10-decisions-needed-from-the-user)
11. [Pending human checks](#11-pending-human-checks)

---

## 1. Purpose and status

On 2026-11-17 the user asked to document four improvement directions for the next iterations: late-game difficulty, review-led polish, new content and open technical work. This file is that record. No direction is chosen yet. Section 9 recommends an order, and section 10 lists what the user must decide before each direction starts.

Sources: a read-only research pass of four researchers (one per direction) and a critic, run on 2026-11-17. Every critic correction is applied below, duplicates are merged, and the critic's missing items are added where a spot check held. Every citation was checked against the tree at `713bd877`. Claims that could not be confirmed were dropped.

Notation. A § number (§2.5, §12.10) refers to the production plan; "section N" refers to this file. "design-enemy-balance" is [docs/plans/production-pass/design-enemy-balance.md](production-pass/design-enemy-balance.md) and "design-progression-tree" is [docs/plans/production-pass/design-progression-tree.md](production-pass/design-progression-tree.md). The production plan's §6 conventions are written "convention C1" (serde default), "convention C6" (append-only) and "convention C8" (keyed tags), so they do not read as candidates C1, C6 and C8.

Baseline at HEAD `713bd877`:
- `horde-defense/` is byte-identical to `d7877b3a`, the last production-pass commit. The merge only brought in npc-framework commits from `main`.
- `RULESET_VERSION` 5 (`crates/horde_core/src/sim/persistence.rs:55`), `SAVE_VERSION` 7 (`crates/horde_core/src/sim/migrations.rs:8`).
- 14 of 16 `pacing_targets` rows pass. G6.11 and G6.12 are red and accepted for the production pass (R60; production plan §12.3).
- The §10 capture ledger is 56 of 56 fixed (production plan §12.5). The 11 human checks are pending (section 11).

Entry format. Each candidate has an id (A1, B1, ...) and these fields: **Problem** with its evidence, **Change**, **Files**, **Cost** (S, M, L), **Risk**, **Moves** (pins, pacing rows, gates, captures) and **Verify**. Some entries also have **Evidence** and **Critic correction(s)**; a critic correction binds the design.

## 2. What was checked on 2026-11-17

**The saved WIP commits from 2026-11-04.** Two branches held work that an earlier worktree cleanup kept:
- `a5750e50` on `claude/overhaul-camp-flow`: one camp navigation row, a shared header on the upgrade home, one "Return to active defense" route, and retreat as the single withdraw.
- `a16ae8db` on `claude/overhaul-core-tree`: rustfmt-only reformatting in a different line-breaking style (it both splits and joins lines; the workspace has no `rustfmt.toml`).

Both were checked against `main`. Every change is already in `main` in a more developed form: `NavEntry` and `camp_entries` (`crates/horde_game/src/ui/navigation.rs:139`), `Screen::shows_header` without a tab (`ui/screen.rs:44`), the Upgrades screen, hold-to-retreat, and the test `retreat_is_the_one_visible_withdraw_and_management_keeps_its_tools` (`ui/tests/battle_tests.rs:757`). Nothing was ported.

**Cleanup.** Both branches were deleted on 2026-11-17, along with the empty `worktree-wf_*` branches (40 by the cleanup run's count; none remain). The commit objects `a5750e50` and `a16ae8db` still resolve locally.

**Uncommitted edits.** This worktree was clean. The only uncommitted edits the production plan names (the A6-18 change, §12.6) are committed as `ef778e28`, an ancestor of HEAD. The main checkout and the other worktrees of this repository were not inspected for uncommitted edits.

**The merge.** The production pass is merged into local `main` as `713bd877` (R64; parents `a058087d` and `d7877b3a`), and nothing is pushed. The R64 merge is no longer open work.

## 3. Binding constraints carried from the production pass

These hold for every candidate below unless the user changes them. Sources are in the production plan and `docs/plans/production-pass/acceptance.md`.

| Constraint | Rule | Source |
|---|---|---|
| Append-only ids | Ids never change; content, Command, EventKind, BossRank and hash tags are append-only. New records go at the end. | ISO-3; §6 "Conventions", convention C6 |
| `SAVE_VERSION` 7 frozen | A new saved field is serde-defaulted to a legacy-neutral value and skipped at it (convention C1). `save_v7.json` is never re-cut and must load, validate and resave byte-identically (`v7_fixture_loads_validates_and_resaves_byte_identically`). A field that cannot meet this stops the work and the user is asked. | §6 "The freeze", convention C1 |
| Rules only through `Game::apply` | Rules change through typed Commands with typed errors; no widget IO. Review fixtures stage through Commands. | ISO-5 |
| Re-bless protocol | A digest-moving commit sets `PROVISIONAL` with a cause row; one re-bless commit per wave folds it, with a reason per digest; a `RULESET_VERSION` bump names the rules. | §6 "Provisional-pin and re-bless protocol (ruling P3)"; ISO-6 |
| Balance needs before and after | Every balance change has pacing and smoke before and after. | ISO-7 |
| No screen shake | `field_rect_never_offsets_on_damage_or_explosion` | ISO-9 |
| Audio warnings keep visual twins | Every warning sound has a visual twin in the same frame. | G8.11 |
| Presentation never touches core RNG | Presentation and keyed rolls never draw `Attempt.rng` or the strategy RNG; keyed rolls use `hash::keyed` with append-only tags. | ISO-8; convention C8 |
| Reduced effects covers every new effect | The reduced MOTION gates apply; the block criterion covers flash and pulse effects only (R63). | ISO-10 |
| No AI music, no new ImageGen sheets | Every music slot stays `Track::Deferred`. New audio comes from the Rust synthesizer with provenance. New enemy looks reuse the 8 sprites with tint and scale. | §2.1 decision 9; §9.3 "FU-AUD-MUSIC" |
| Root files untouched, PowerShell 5.1 | No root-repo edit (R6); scripts run under Windows PowerShell 5.1. | ISO-1, ISO-2 |
| Skill node additions | Decision 19 allowed at most 8 new nodes (ids 170-177), all used. More nodes need a new user decision. | §2.1 decision 19 |

## 4. Direction A: late-game difficulty

**Why the late campaign gets easy.** From the code and the W4-04 measurements (production plan §9.6, "W4-04..n tuning loop, resumed on the repriced tree"):
1. Threat only rises. The rank is `Campaign.victories` (`crates/horde_core/src/campaign/progression.rs:534-541`), and "Defeat and withdrawal do not raise it" (`docs/PROGRESSION_SCALING.md:7`). The survivor's power stops growing at about run 30. So a steeper enemy curve gives a wall, not a band: with late health 0.25 the median campaign stops at rank 29 and loses 98-100% of its runs from run 46.
2. The survivor is never hit. On expert-deu the median damage dealt to the survivor is 0 from run 11, with no defeat after run 20, while the rank reaches 51 at run 60. Every enemy lever in the W4-04 table left 32-33 windows out and a 46-50 streak inside the G6.17 band.
3. Some defender power is free with rank and multiplies the tree. The snapshot grows `fortification_multiplier` (1 + 0.025 r, 2.25 at rank 50) and `supply_bonus` (min(r, 40)) with every victory (`crates/horde_core/src/progression.rs:696-699`). `structure_bonus` multiplies the structure rank term, fortification and the engineering multiplier (`sim/npc.rs:453-461`). The four engineering nodes multiply together (`skill_catalog.rs:543-546`; 1.25 x 1.2 x 1.2 x 1.25 = 2.25 in `skills.json`). The bot places a trap on every lane first (`sim/pacing.rs:782-809`).
4. The early end of G6.11 fails the other way. On `goal6-novice` the offset-0 window reads 30.5%, and every window from offset 18 reads 0.0% (§2.5 row "G6.11 and G6.12 stay red on the goal-6 measurement profile"). A fix has to lower the early defeat share and add late defeats.

The traps-cause-overkill reading in point 3 is an inference. It is not measured; A1 measures it.

R60 froze balance data, bots and acceptance rows for the production pass. A1 and A6 change no rule. A2 to A5 change rules or data and need the user to lift that freeze, plus a re-bless.

### A1. Measure the late overkill before tuning (harness only)

- **Problem.** The W4-04 loop tuned without knowing what kills the late enemies or how far each run is from losing. `CombatTelemetry` already records `defender_damage`, `trap_damage`, `turret_damage`, `structure_damage_taken`, `contact_attacks`, `peak_enemies` and `skipped_spawns` (`crates/horde_core/src/sim/telemetry.rs:12-32`), but `PacingRun` (`sim/pacing.rs:457`) carries none of them, and the CSV header has no column for them (`docs/BALANCE_AND_RUN_STATS.md:88`). The solo bot also always enters the easiest enterable country (`sim/pacing.rs:1160`, `min_by` difficulty), so the country difficulty band and the biome mix are barely sampled late (critic addition).
- **Change.** (1) Copy those telemetry fields into `PacingRun` and the `--csv` columns. (2) Add a headroom probe to `examples/pacing`: for runs 11, 20, 30, 40, 50 and 60 of each seed, clone the game at the camp step, deploy the same stage under Custom balance with `enemy_health` and `spawn_rate` scaled together (0.25-3.0 each, `balance.rs:111`), and binary-search the smallest factor that loses the run. Print the median per 5-run bucket. (3) Add a route variant (random or hardest enterable country) next to the easiest-country route.
- **Files.** `crates/horde_core/src/sim/pacing.rs`, `crates/horde_core/examples/pacing.rs`, `tests/pacing_harness.rs`, `docs/BALANCE_AND_RUN_STATS.md`.
- **Cost.** S-M. **Risk.** Low: harness code, plays only through `Game::apply` and `tick`, probes on clones. Keep the probe out of `pacing_targets` for runtime.
- **Moves.** No rule, data, pin or row. The CSV gains columns.
- **Verify.** `tests/pacing_harness.rs` pins the new columns on a synthetic report. `cargo run --locked --release -p horde_core --example pacing -- 60 --csv --profile goal6-novice --seeds 20` prints the damage shares and the break factor per bucket.

### A2. Streak heat with relief: a feedback rule

- **Problem.** G6.11 and G6.12 are statements about streaks, and threat never falls. A rule driven by the outcome streak can turn the wall into a band. The production plan already listed "a rank rule that eases after a defeat" as an option (§2.5, both G6.11 rows); the user chose other options then.
- **Evidence.** `Campaign.run_history` is saved (`crates/horde_core/src/campaign.rs:53`; pushed at `campaign/ledger.rs:22`). `RunRecord` holds `worker_id`, `recruit_id` and `outcome` (`stats.rs:143-152`), so a streak can be derived without a new saved field. `Campaign::progression_for` is the one fold point for deploy, the briefing preview and `defender_power` (`campaign/progression.rs:651-689`).
- **Change.** A new optional `streak_heat` block in `progression.json` (serde default: none). For main runs only, streak = consecutive Victory records since the last non-Victory; heat = clamp(streak - `from_streak`, 0, `max_steps`). Fold heat only into terms that boss health does not read: `arrival_multiplier`, `elite_rate_bp` and the spawn-rate floors. Regular `enemy_health` is not one of them: boss HP is `base x boss_health_multiplier x modifiers.enemy_health` (`sim/director.rs:280`; the same order in `encounter_health_bound`, `sim.rs:1916`), and `modifiers.enemy_health` takes the snapshot's `enemy_health` (`sim/lifecycle.rs:240`). Heating health therefore needs the validator widened (see the corrections below). After `relief_from_defeats` consecutive defeats, apply relief. The briefing names "Horde alert <heat>" and relief as "The horde regroups"; any sting keeps a visual twin.
- **Calibration.** The equilibrium defeat share is about 1 / (1 + mean streak), so a mean streak of 3-9 lands in the 10-25% band of G6.11. Starting values to measure, not to ship: `from_streak` 4, `relief_from_defeats` 2, and a relief floor at the validator bounds. Size the per-step terms from A1's break factor so that about 10 heat steps beat the late overkill. To keep winning from feeling punished, pair heat with a reward term or present it as the horde adapting.
- **Critic corrections that bind the design.**
  - The snapshot validator bounds `boss_health_multiplier` to 1.0-2.0 (`progression.rs:895`), and `fold_boss_rank` clamps it there (`:840`). Dividing it by the heat factor is a schema violation, not a judgement call. Either heat only terms the boss does not read, or widen the validator (a change to saved-snapshot bounds).
  - The validator also requires `enemy_health` in [1.0, 4096] (`:880`) and `arrival_multiplier` in [0.5, 8.0] (`:879`). Relief must clamp at those floors, or act only through spawn rate and elites.
  - `progression_for(mode, rank, lanes, front)` takes no worker input (`campaign/progression.rs:651-656`). "Main runs only" needs a new parameter or a separate call path.
  - Withdrawal is an `Outcome` (`sim.rs:895-899`). The rule must say whether a withdrawal breaks a streak or counts toward relief, or players can farm relief by withdrawing.
- **Files.** `progression.json`, `src/progression.rs`, `src/campaign/progression.rs`, `sim/stage_preview.rs`, `sim/power.rs`, horde_game briefing and world panel, `docs/PROGRESSION_SCALING.md`, `design-enemy-balance.md` §1.
- **Cost.** L. **Risk.** Medium-high. It adds a threat axis that design-enemy-balance §1 does not have (it rejects player power and liberation count as axes; outcome history was not considered). Players can feel punished for winning.
- **Moves.** Every solo pacing sweep from the first long streak; `RULESET_VERSION` 5 -> 6. GOLDEN runs from fresh campaigns, and the progressed pins are staged with `c.victories = rank` and no history (`tests/support/mod.rs:290`), so no pin move is expected; confirm with `tests/determinism.rs` and use P3 if one moves. Rows at risk: G6.15 (Hard 13 against 13.5), G6.17 (arrival heat adds enemies during the boss fight; boss health if the validator is widened), G5.9, G5.10, G3.10, G3.12. Save schema unchanged.
- **Verify.** Unit tests: heat 0 on empty history, on worker records and after a defeat; relief after `relief_from_defeats` defeats; Easy, Normal and Hard get identical heat; validator bounds hold; boss HP unchanged by heat. `tests/threat_guard.rs` stays green. Release `pacing_targets`: G6.11 and G6.12 on `goal6-novice`, and the 14 green rows still pass.

### A3. Cap the free rank-indexed defender power

- **Problem.** Each victory raises the enemy curve and also hands the defender `fortification_multiplier` (structure durability and damage at `sim.rs:1673` and `sim/structures.rs:62-72`, ordnance at `sim/manual.rs:263`) and `supply_bonus` (`sim/lifecycle.rs:274`). The W4-04 loop tried only enemy-side keys.
- **Evidence.** `progression.json`: `fortification_per_rank` 0.025, `supply_per_rank` 1, `max_supply_bonus` 40. The shrapnel trap costs 4 supply (`content.json:2700`). Level supply is 30 / 33 / 36 / 40 / 45 by template, the template is `stage % 5` (`sim.rs:1606`), and 195 of 242 countries have 3 stages (`world.rs:229-238`, `levels_per_population` 1e7, minimum 3). So most late runs play templates 0-2 with 30-36 base supply. (Critic correction: the earlier "45 supply pays for 17 traps" used the wrong template.)
- **Change.** Add `fortification_knee_rank` (serde default: no knee) so the multiplier is 1 + 0.025 min(r, knee); ship a knee of at least 13 (12 keeps the progressed pins; below about 13 puts G6.15 at risk). Lower `max_supply_bonus` from 40 to 15-20; with base 30-36 that leaves about 11-14 traps before preset supply. Size the cut from A1's numbers. Optionally make the engineering multipliers add instead of multiply (`skill_catalog.rs:543-546`), only if A1 shows structures dominate. Leave `item_level_rank_step` alone: G3.11 sits exactly on its bound (6 / 9 = 0.67, R66).
- **Files.** `progression.json`, `src/progression.rs`, possibly `skill_catalog.rs` and `campaign/tree_value.rs`, `docs/PROGRESSION_SCALING.md`.
- **Cost.** S (data and one key) to M (with the engineering change). **Risk.** Medium. Alone it repeats W4-04 (no band). Its value is that it shrinks the heat A2 needs. Boss time-to-kill rises as turret and ordnance damage falls.
- **Moves.** The progressed pins run at rank 12, so a knee of 12 or more keeps them. Pacing late rows; `RULESET_VERSION` bump. Rows at risk: G6.17, G6.15 (only with a knee under about 13).
- **Verify.** Unit pins for the knee and the legacy default; `tests/determinism.rs` (0 moves with knee >= 12); A1's break factor per bucket; the release suite.

### A4. Something that leaks past the trap line

- **Problem.** With 10 trap-rank nodes, rank fortification and the engineering product, a trap kills most regular enemies that reach it, and it gains a charge per structure rank (`content.rs:352-355`; the shipped trap authors `charges_per_rank` 1). Regular enemies have no trap-specific defence (`enemy_armor_vs`, `sim.rs:2047`).
- **Change.** Pick one: (a) an appended elite affix that takes reduced trap damage and spends no charge, weighted into elites from about rank 14. `EliteAffix::offered_to` (`src/enemy_catalog.rs:271`) has no rank gate today, so (a) needs C5's `min_rank` field. (b) An appended archetype that disarms the nearest trap or wall on contact, placed in wave tiers 9-11. Tier 9 starts at rank 16 and the progressed pins run at rank 12 (tier 7), so the pins hold. Either needs a small deterministic rule in horde_core and draws nothing at spawn (`docs/ENEMY_COMPOSITION.md:209`). Direction C's burrower, late affixes and composite archetypes attack the same problem; see section 8 for the trap stance.
- **Files.** `enemy_catalog.json` (append), `src/enemy_catalog.rs`, `sim/structures.rs`, `docs/ENEMY_COMPOSITION.md`, horde_game enemy visuals and inspector; for (b) an `ENEMY_VOICES` row in `crates/horde_audio/src/voicing.rs`.
- **Cost.** M-L. **Risk.** Medium. The premise is an inference; confirm with A1's trap share first. A new archetype must be spawned and killed in every sweep (G6.16).
- **Moves.** A new affix in the pool moves the progressed pins (rank 12 rolls elites) unless it is rank-gated above 12 through C5's `min_rank`. `RULESET_VERSION` bump. Rows: G6.16, G6.14, G6.15, the G1 perf gates; for (b) `every_catalog_archetype_has_explicit_voice_row_with_existing_files`.
- **Verify.** Unit tests for the effect; `goal6_archetype_coverage`; A1 shows the trap share falling and non-zero survivor damage late; perf progressed-peak and stress.

### A5. One pressure-floors table past rank 28

- **Problem.** Past rank 28 most enemy terms are flat. `pressure_floors` has 29 rows (ranks 0-28) and holds its last row after that (start 0.50, rate cap 2.00, boss health 21,200, tier 11). In the snapshot, batch is (1 + r/3).min(4), flat from rank 9; population is (72 + 2r).min(145), flat from rank 37; speed and arrival grow to `max_rank` 50 (`progression.rs:681-700`). Past rank 50 health and damage keep growing, and item level grows to its cap of 30 at rank 58 (`item_level` uses the unbounded rank, `progression.rs:690`). (Critic correction to the earlier "all stop at 50" list.)
- **Change.** Edit rows 14-28 to steepen `rate_cap` and `start_rate` from rank 14, and append rows 29+. This is a value change to existing data plus an append, not append-only. Keep the boss floor slope (G6.17) and the `wave_tier` column (G6.14) unless C1 changes it. Direction C1 also appends rows 29+ for new tiers: write one table for both (section 8).
- **Files.** `progression.json`, `docs/PROGRESSION_SCALING.md`.
- **Cost.** S. **Risk.** Medium. More arrivals raise adds during the boss fight (W4-04: `arrival_per_rank` 0.04 pushed TTK p90 to 99.7 s). Low value without A2.
- **Moves.** Late pacing rows; `RULESET_VERSION` bump. Progressed pins unchanged if the slope starts at rank 14 or later.
- **Verify.** `every_floor_tier_exists_in_enemy_catalog` and the floor validation; `snapshot_wave_tiers_at_10_20_40_are_7_10_11`; A1's break factor roughly flat over runs 20-60.

### A6. Show the streak and offer to raise the stakes (presentation only)

- **Problem.** R60 leaves the late game to the human difficulty playtest, but a player who wins 40 in a row gets no signal. Hard already raises threat growth (1.25), elites (150%), the ramp (150%) and boss phases (+1), with reward 1.15 (`balance.json`). `Command::SetBalance` and `Command::ApplyPreset` exist (`command.rs:44`, `:86`) and act from the next deployment.
- **Change.** In horde_game only: show the current win streak and the defeat share of the last 20 runs in Results and the camp. After about 8 straight victories, show a dismissible camp card that offers Hard and issues the existing Command through `Game::apply`. The dismissal lasts for the session (no save field).
- **Files.** horde_game camp and Results panels, `docs/PLAYTEST_GUIDE.md` (record the streak and preset changes in the difficulty playtest).
- **Cost.** S-M. **Risk.** Low; the card can feel naggy, so show it once per streak. Reads `run_history` only.
- **Moves.** Nothing in horde_core. The bots never see the card, so G6.11 and G6.12 stay red until a rule lands.
- **Verify.** UI tests for trigger and dismissal; a capture of the card; `tests/determinism.rs` 0 moves.

## 5. Direction B: review-led polish

Presentation only: no horde_core change, no pin, no save field. The open polish is in three places: the evidence the production pass left partial (production plan §10.5, §12.5), the four human checks that judge a feel (damage pop, text placement, sprite upscale, camera), and one toast defect found in code. The closing triage ledger (715 items) is not committed, so nothing here mines it.

### B1. HEAD capture baseline and the two carried judgements

Merged with the technical direction's capture-run candidate.

- **Problem.** No capture run exists after the W7-11 set at `d53cfe0f`. Production plan §12.5 names four code commits since then, but `git log d53cfe0f..HEAD -- horde-defense/crates horde-defense/assets` lists ten: `62a406d4`, `ac71fcf7`, `fe14fe4f`, `d789418a`, `e73ae33c` (the closing-ui1 items, §12.4), `400c8efb`, `b33f03fe`, `27d763cb`, `a0278a08` and `ef778e28`. Some change pixels on purpose: `fe14fe4f` puts the defender picker on Camp home's Next defense card (`ui/home.rs`, `ui/chrome.rs`), `d789418a` names the defender in the NPC decision label (`ui/battle/field.rs`, `inspector.rs`), and `62a406d4` edits the Recruit modal (`ui/modal.rs`). `a0278a08` moves the frame schedules to the single-threaded executor, and `b33f03fe` changes which glyph sizes are warmed. Also open: no recorded judgement of the W2-37 phase hold or of the W2-41 compact Firearms rows (§12.5; §10.5 "Not evidenced by this review").
- **Change.** Build once at HEAD; run `capture_review.ps1` (normal and reduced) and `compare_captures.py` against `target/production-after/closing/w7-11`. Expect changed stills in at least camp-home, the camp callout and the takeover and observer battle scenes, and possibly the Recruit modal: read and judge each diff, and log a CF row only for a defect. Run `capture_motion.ps1 -Scene boss-phase` (both sizes and modes) and storm, then `measure_field_luminance.py`. Record the W2-37 judgement from the boss-phase frames (the 90-tick hold itself is pinned by `boss_phase_hold_shows_phase_label_and_ring_for_90_ticks`). Record the compact Firearms half from the existing `skills-firearms-column` still (`crates/horde_game/src/review_scenes.rs:294-304`, added in the closing phase for this item), so no new scene is needed. Correct §12.5 to list all ten commits (B1 owns this correction).
- **Files.** No code. Records in the production plan §10, §12.5 and §12.10.
- **Cost.** M (wall clock: one full still run and about 8 motion exports). **Risk.** Low for code; a real regression opens a CF row and turns `check_capture_findings.py --final` (G10.1) red until fixed. Disk: about 2.3 GB of stills (the w7-11 run's size), plus the motion exports; see D5 for `target/`.
- **Moves.** Capture records only.
- **Verify.** `compare_captures.py` exits 0 with every changed pair judged; `check_capture_findings.py --final` passes; the night and storm luminance gates still pass.

### B2. Night and neutral cue-legibility scenes

- **Problem.** The "Night and storm look" check asks whether the night grade hides a windup, contact or arrival cue and whether a lightning pulse masks a cue (`docs/ACCEPTANCE.md:254`). The night-checkpoint and dawn-reference fixtures are deployed paused with no hostile, as the luminance pairing needs (`crates/horde_game/src/ui/review/fixtures/production.rs:1058-1069`). §10.5 leaves night, storm and neutral cue legibility open (carried items 25 and 55, partial). Only fx-stress shows night light pools with enemies.
- **Critic correction.** The storm half is already pinned. The sky pulse paints under the cue pass and stays silent near an attack or boss cue, while paused and in reduced effects (`art/battlefield/mod.rs:335-337`; `SkyQuiet`, `art/lighting.rs:220`; test `sky_flash_paints_under_cue_pass`, `art/battlefield/tests.rs:2869`). A storm scene staged with cues would show no pulse at all. The storm fixture is not paused at tick 0 either: `prepare_storm` ticks to the pulse lead (`production.rs:1094-1110`).
- **Change.** Append two review scenes: `night-cues` on the night checkpoint stage (a spitter windup with its aim line, a brute in contact, INCOMING chevrons, an elite tell) and `neutral-cues` on a neutral stage, reusing the existing stagers. Leave the three luminance scenes unchanged. Count `sky_flash_paints_under_cue_pass` toward the storm half and shrink the human check's storm half to the reduced-effects static grade. Update the protocol rows in ACCEPTANCE.md and production plan §11 together.
- **Files.** `ui/review/fixtures/production.rs`, `review_scenes.rs`, `ui/review/tests.rs`, `docs/ACCEPTANCE.md`, production plan §11.
- **Cost.** M. **Risk.** Low: review-only code. Staging needs a tick that holds a windup and contact at once.
- **Moves.** Capture counts (2 scenes x 2 sizes x 2 modes). `acceptance_lists_every_pending_human_check` needs both lists changed together.
- **Verify.** Each fixture asserts a Windup enemy and a contact lane at its paused tick; `capture_review.ps1 -Scenes night-cues,neutral-cues -Modes normal,reduced`; the night gate is unchanged.

### B3. Near-continuous damage pop with a fixed glyph set

- **Problem.** Since W1-64 the punch snaps to `SCALE_STEPS = [0.85, 1.0, 1.15, 1.3]` (`crates/horde_game/src/ui/damage.rs:236`, via `snap` at `:346`), so the 1.3 peak lasts about one frame (CF-1-04). The steps exist only to keep the font atlas fixed (`:232-235`). The glyph set is 24 characters (`:238`).
- **Change.** (a) 10 steps from 0.85 to 1.30 in 0.05 increments, warmed once by `popup_sizes` (`:272`) and `warm_glyphs` (`:322`); or (b) lay out once at the peak size and scale the mesh down. Prefer (a). Reduced effects is unchanged. Design it together with D7, which rests on the same warm-glyph contract.
- **Files.** `ui/damage.rs` and its tests.
- **Cost.** S. **Risk.** Low. More warmed sizes raise the atlas fill. Keep-clear placement uses the peak size (`:817`, `:851`), which stays 1.3.
- **Moves.** Damage motion captures; interface_draw may move by microseconds.
- **Verify.** `damage_popups_stop_uploading_font_texture_after_warmup` (`:1093`) at ppp 1, 1.35, 2 and 2.7; a new test that scales are monotone and distinct over the first frames; `capture_motion.ps1 -Scene damage`; perf progressed-peak. Then the human check.

### B4. Subpixel glyphs for camp text (only if the human check asks for it)

- **Problem.** `install_font` turns subpixel binning off for every face (`crates/horde_game/src/settings.rs:584-592`), not only the battle text whose changing digits re-uploaded the atlas. CF-1-05 measured 0.23-1.20% of pixels changed against bins on; whether spacing looks uneven at 1050x700@1.35 is pending the whole-pixel text check.
- **Change.** Run the human check first. If it finds uneven spacing: register Atkinson twice, with bins on for camp text and a whole-pixel `hud` family for battle text that changes per frame (`battle_text_fonts`, `ui/damage.rs:301`), and warm that family.
- **Files.** `settings.rs`, `ui/damage.rs`, HUD and cue-label call sites.
- **Cost.** M. **Risk.** Medium: a missed battle call site brings atlas uploads back (the atlas test and frame-report gates catch it). Nearly every camp still changes, and the committed pairs use 39,099,685 of 40,000,000 B (§12.5).
- **Moves.** A full re-capture; perf gates re-measured.
- **Verify.** The atlas test plus a new one that camp and battle passes add no glyph after warm-up; `measure_performance.ps1` on progressed-peak and stress; the whole-pixel text check with bins on and off.

### B5. Sprite magnification on zoomed 3-7 lane fields

- **Problem.** A 3x12 field fits at zoom 2.5 (`crates/horde_game/src/art/camera.rs:121-124`), and the camera applies "No texel or magnification clamp" (`:46-50`). Walkers scale with lane height, `scale = (available.y / walk.height)...` (`visual_assets.rs:670`), from a NEAREST atlas, so non-integer upscales can give uneven texel columns. That is the sheet-upscale check. A precedent caps magnification for explosion icons (`art/battlefield/explosion.rs:29`, `clamp_sprite` at `:365`).
- **Change.** Step 1 (S): a pure probe test that tabulates pixels per texel for every walker and actor on the 3x12, 5x14 and 7x16 fields at both sizes. Step 2 (M, only if the probe shows more than 2 px per texel with a fractional part): snap the layout scale in `walk_frame_layout` (`:661`) and `standing_actor_layout` (`:647`) down to whole device pixels per texel. Zoom-1 fields stay bit-identical.
- **Files.** `visual_assets.rs`, `art/battlefield/enemies.rs`, a new test.
- **Cost.** M. **Risk.** Medium: snapping can shrink a figure by up to one texel step, so health bars and popups must use the snapped height.
- **Moves.** Pixel pins for zoomed fields only; captures of intro, weapon-classes and boss-phase.
- **Verify.** The probe output; `fit_field_keeps_every_filling_geometry_bit_identical`; re-capture, then the sheet-upscale check.

### B6. Dev-only field zoom ceiling for the camera-feel check

- **Problem.** The camera-feel check asks whether a 3x12 field at 2.5x plays well (`docs/ACCEPTANCE.md:248`). The camera is uniform zoom-to-fit with no ceiling by user requirement (§2.2 "Camera (R1)"). The check can only accept or reject, not tune.
- **Change.** Behind the existing `dev` feature, read an env var (for example `LASTLIGHT_FIELD_ZOOM_MAX`) once at launch; `BattleLayout` clamps the fit (`ui/battle/layout.rs:125`, `:131`) and re-centres. `fit_field` stays pure. The review launch ignores it. Add the A/B steps (default, 2.0, 1.6) to the camera-feel protocol. Any shipped ceiling is a user decision.
- **Files.** `ui/battle/layout.rs`, the launch config, `docs/PLAYTEST_GUIDE.md`, production plan §11.
- **Cost.** S. **Risk.** Low: compiled only with `dev`.
- **Moves.** Nothing in shipped builds. It adds a `dev` cfg, which D4's filter must include.
- **Verify.** Unit test: the clamp keeps the field inside the arena and is a no-op when unset; the `--no-default-features` build compiles it out; a manual A/B.

### B7. The battle toast hides older errors

- **Problem.** In battle every status message becomes a toast (`toast::status_toast`, called at `ui/battle/field.rs:205`; the battle has no status line, C-10). The battle draws only `newest(1)` (`ui/toast.rs:236`), so a later Info toast covers an unexpired Error. `Toasts::advance` starts the clock of every queued toast, drawn or not (`:126-131`). A 6 s Error (`TOAST_ERROR_SECS`, `:17`) covered by a 3 s Info can expire unseen.
- **Change.** (1) `battle_toast` shows the newest unexpired Error if one exists, else the newest toast. (2) `advance` starts only the toasts on screen (1 in battle, `STACK_VISIBLE` 3 elsewhere, `:25`); `QUEUE_CAP` 8 (`:34`) stays the bound. Drop a queued Info older than its life so stale messages do not appear late.
- **Files.** `ui/toast.rs` and its tests, the `advance` callers.
- **Cost.** S. **Risk.** Low.
- **Moves.** None; the default fixtures push at most one battle toast.
- **Verify.** New toast tests: an Error then an Info keeps the Error drawn in battle; a 4th stack toast waits with no `shown_at`. The existing toast tests stay green.

### B8. Captures for the remaining partial carried items

- **Problem.** §10.5 marks these as not or half captured: item 63 (the ApplyAutoSell modal, normal and requoted), item 16 (Briefing defender combo with 4+ defenders, a disabled option with its reason, a long label; the area of CF-3-08), item 31 (storage modal load and settings variants), items 62/80 (boss banner hint-callout and construction variants and the long-name shrink; `banner_rect` dodges an occluder at `ui/battle/boss_banner.rs:82`, text shrinks to `MIN_TEXT_SIZE` 9 at `:47`), and items 8 and 19/76 (small-size halves below the fold).
- **Change.** Append review scenes for each state (two Auto-sell modal states, a 4-defender briefing, load and settings storage errors, banner with construction open and with the hint band, scrolled armory and camp-reports at the small size). Judge each and record it per item in §10. D6's human-check kit consumes these scenes; it does not keep its own list.
- **Files.** `ui/review/fixtures/production.rs`, `review_scenes.rs`, `ui/review/tests.rs`.
- **Cost.** M. **Risk.** Low for code. A scene can surface a real layout defect, which becomes a CF row.
- **Moves.** Capture counts; check_views counts if views are counted.
- **Verify.** Each fixture asserts its state (the modal open, 4 roster entries, the occluder present); `capture_review.ps1 -Scenes <new>`; `check_capture_findings.py --final` stays green.

## 6. Direction C: new content

What exists (counted at HEAD): 18 archetypes and 12 wave tiers (`crates/horde_core/enemy_catalog.json`); 5 elite affixes of an allowed 16 (`src/enemy_catalog.rs:167`); 3 boss kinds over exactly 5 frozen levels (`src/content.rs:436`); 68 items; 177 skill nodes; 242 countries in 5 biomes (Tropical 121, Temperate 62, Arid 28, Boreal 17, Alpine 14).

Where the late game repeats:
- From wave tier 7 every tier holds the same 17 spawnable archetypes; tiers 8-11 only shift weights. Tier 11 starts at rank 28, and the director reads `max(level, snapshot wave_tier)` (`sim/director.rs:47-48`). `MAX_WAVE_TIER` is 15 (`src/progression.rs:8`), so tiers 12-15 are free; `tests/enemy_composition.rs:135-140` asserts the last tier is 11 and that tiers past it are rejected today.
- Most countries play the same three fights. The template is `stage % 5` (`sim.rs:1606`), and 195 of 242 countries have 3 stages, so most play The Old Road (Breacher), Ashwood Crossing (Caller) and Depot Nine (SiegeBrute) as the Capstone.
- The elite rate is flat from rank 23 (`progression.json` `elite.end_rank` 23, `end_bp` 1500).
- Archetype `tags` are validated and copied but no rule reads them (`src/enemy_catalog.rs:306-308`, `:974`; `docs/ENEMY_COMPOSITION.md:84`). Node `counters` are text only (`docs/SKILLS_AND_WEAPONS.md:103`). The R5 field `heavy_target_damage_multiplier` is pre-approved but not in the code (§2.4 R5). The `Pips2` tell marker can be drawn (`horde_game/src/art/battlefield/elite.rs:154`) but no affix uses it.
- GDD constraints: a new family must change a decision, not recolor a health bar (`docs/GDD.md:249`); spawn cues must give a response window (`:271`); every compulsory boss mechanic must be solvable by the NPC's legal actions (`:291`); the NPC sees only visible enemies (`:519`); "mutations" is a later decision (`:734`).

**Keeping the pins still.** GOLDEN runs at rank 0. The progressed pins run on AIA (`tests/support/mod.rs:200`), a 3-stage country, with `victories = rank` and rank 12 (`:290`). Gating a candidate at rank 13 or more, or at wave tier 12 or more, keeps GOLDEN, GAMEPLAY_TRACE and the progressed pins. A `RULESET_VERSION` bump is still needed, and pacing rows still move. Batch every rules candidate into one wave (section 8).

None of these is built or measured. Every number is a proposal to measure at 20 seeds.

### C1. Late composition tiers 12-15 with composite archetypes (data only)

- **Problem.** About the last 20+ runs of a 60-run campaign play one composition (tier 11).
- **Change.** Append 4-5 archetypes that recombine existing components: a spitter with shield-bearer defense, a spitter group of 3, an exploder with sprinter movement, a juggernaut that splits into splitter-brood. Append tiers 12-15 and floor rows 29+ that reach them, with boss health held flat for G6.17. The floor rows are the same table as A5. Each archetype gets tags, a tint-and-scale visuals record and an `ENEMY_VOICES` row that reuses an existing bank.
- **Files.** `enemy_catalog.json`, `progression.json` (shared with A5), `crates/horde_audio/src/voicing.rs`, `docs/ENEMY_COMPOSITION.md`.
- **Cost.** S (data and test updates). **Risk.** Low for rules: every behaviour exists. A shielded spitter could feel like a wall to a gun-only build.
- **Moves.** Tests to update: `catalogue_has_twelve_tiers_with_monotone_mean_health`, `every_floor_tier_exists_in_enemy_catalog`, `snapshot_wave_tiers_at_10_20_40_are_7_10_11`, `goal6_archetype_coverage` (G6.16), `every_catalog_archetype_has_explicit_voice_row_with_existing_files`, and the G6.14 row text (rank 40 maps to a new tier). GOLDEN and progressed pins unchanged; `FROZEN_CATALOGUE_FNV` and `waves_0_to_4_byte_identical` unchanged (append only). `RULESET_VERSION` bump. Perf progressed-peak and stress re-run.
- **Verify.** `cargo test --locked -p horde_core` and `-p horde_audio`; release `pacing_targets` (coverage includes the new ids; G6.17 boss TTK median 20-60 s, and p90 <= 60 s, the W4-04 band); the A1 damage column late.

### C2. Burrower: an approach that guns cannot hit

- **Problem.** No archetype makes the player react at short range, and the survivor takes no damage late.
- **Change.** An optional `burrow` movement field (convention C1 default) and an `Enemy.burrowed` flag. While burrowed, `HitSource::Shot` and `Turret` cannot hit it and the NPC does not target it; traps still trigger, grenades and fire still hit, and a live wall forces it up. It surfaces near the line with an appended event and draws no RNG. Tier 12+ or a rank gate of 13+. horde_game shows a dirt mound and a surfacing ring (static in reduced effects); horde_audio adds a synthesized voice bank (windup, contact and death, 5 variants each: 15 files) with a `WARNING_TWINS` row.
- **Files.** horde_core `enemy_catalog.rs`, `sim/movement.rs`, `sim/combat.rs`, `sim/npc.rs`, `sim/structures.rs`, event types, `campaign/tree_value.rs`; horde_game battlefield and inspector; horde_audio voicing and mix, the 15 WAVs written by `export_audio --write`, `assets/audio/SURVIVAL_PROVENANCE.json` and `assets/CREDITS.md`.
- **Cost.** M-L. **Risk.** Medium. Fairness needs a minimum response window between the first mound and contact (`GDD.md:271`). An NPC defender cannot act against a burrower until it surfaces, so the NPC-rules profile must be validated (`GDD.md:249`). One new saved convention-C1 field: check `save_v7.json` resaves byte-identically.
- **Moves.** With the gate, no GOLDEN or progressed move. `RULESET_VERSION` bump; the G8.4 WAV count 281 -> 296, and the G8.1, G8.11 and G6.16 rows; possibly the tree rules if the value model credits traps more.
- **Verify.** New tests: no Shot or Turret hit while burrowed, a trap triggers, a wall forces surfacing, the surface event fires at least N ticks before contact, a mid-burrow save resumes to the same digest; `every_warning_row_names_a_visual_twin`; the `export_audio` check and G8.4's provenance test; an Npc-mode pacing run and `tests/npc_balance.rs` (W4-14, R59); pacing at 20 seeds.

### C3. Herald: an aura that makes target priority matter

- **Problem.** Late fights have no target-priority decision. ENEMY_COMPOSITION names "an aura" as a behaviour that needs a new parameter (`docs/ENEMY_COMPOSITION.md:127-128`).
- **Change.** An optional archetype field `aura {radius, speed_mult, windup_mult}` (convention C1 default, bounded). Each tick, non-boss hostiles near a live herald in the same or next lane take the strongest single aura; auras do not stack and nothing new is saved. A low-health herald on the Caller sprite with a distinct tint, tier 12+.
- **Files.** horde_core `enemy_catalog.rs`, `sim/movement.rs`, `sim/combat.rs`, `campaign/tree_value.rs`; horde_game a ground ring and a buff chevron; horde_audio a voicing row on an existing bank.
- **Cost.** M. **Risk.** Low-medium. Clamp the combined speed and windup when a herald buffs a Frenzied elite.
- **Moves.** With the gate, no digest move. `RULESET_VERSION` bump; G6.16 and G8.1; possibly the tree-rule tests.
- **Verify.** Unit tests for inside and outside the radius, no stacking, no RNG draw, save mid-aura; an NPC priority test; captures at 3x12 and 17x30 in both effects modes.

### C4. Boss mutations past rank 13, with an optional keyed boss rotation

- **Problem.** 3 boss kinds, fixed per template (`content.json` levels; `sim/combat.rs:321`, `:327` read `content().levels[a.level].boss`). The rank-scaled boss differs only in numbers and one phase.
- **Change.** An appended `boss_mutations` table, up to 2 per boss kind (for example brood adds on a Breacher phase, spitter calls for the Caller, a structure burst on a SiegeBrute recovery). On a Regional or Capstone stage at rank 13+, pick one with `hash::keyed` and an appended tag into a convention-C1 snapshot field. Phase 2, separate change: a keyed boss override for countries under 5 stages.
- **Files.** horde_core `enemy_catalog.rs`, `progression.rs`, `sim/director.rs`, `sim/combat.rs`, `hash.rs`, `sim/stage_preview.rs`; horde_game briefing and phase label.
- **Cost.** M (L with the rotation). **Risk.** Medium. G6.17 must hold, so mutations add pressure, not boss health. The rotation touches first-clear and boss-unlock history.
- **Moves.** Rank gate 13+: no progressed or GOLDEN move. `RULESET_VERSION` bump; G6.4 and G6.8 gain tests; boss TTK.
- **Verify.** A mutation only at rank 13+ on Regional or Capstone; keyed, no RNG draw; survives save mid-phase; an NPC defender can win each mutated Capstone; release G6.17.

### C5. Rank-gated late elite affixes

- **Problem.** Elites stop changing at rank 23, and two of the five affixes are pure multipliers.
- **Change.** A `min_rank` field on `EliteAffix` (convention C1 default 0) checked in `offered_to` (`src/enemy_catalog.rs:271`), so ranks below the gate offer today's set and today's keyed pick (`:427`). Append "brood" (death effect Split into 2 splitter-brood, min rank 20) and "stalwart" (stagger immune, min rank 25, a new convention-C1 affix field). Brood uses the unused `Pips2` marker.
- **Critic correction.** Blue [0, 114, 178] is not "the one unused hue": Okabe-Ito orange [230, 159, 0] is also unused, and blue sits next to hardened's [86, 180, 233] (`assets/art/enemy_visuals.json`). Choose the colour with the colour-blind check in the elite review scene, and make the marker shape the primary cue.
- **Files.** `src/enemy_catalog.rs`, `enemy_catalog.json`, `assets/art/enemy_visuals.json`, horde_game `visual_assets/enemy_visuals.rs` and `art/battlefield/elite.rs`, `docs/ENEMY_COMPOSITION.md`.
- **Cost.** S. **Risk.** Low. The enemy cap already bounds split children.
- **Moves.** With min rank 20+, no GOLDEN, progressed or elite-rate pin move. `RULESET_VERSION` bump.
- **Verify.** `elite_roll_consumes_no_combat_rng`; a test that the pick at rank 12 equals today's; the archetype x affix load validation; a brood split's children are not elite; save round trip; the tells capture.

### C6. Biome strains: biome-weighted late pools

- **Problem.** Biome feeds only stat modifiers, and recruit origin is "flavour only, no stat effect" (`docs/STRATEGY.md:11`). All biomes draw the same late pools.
- **Change.** An appended `biome_strains` table (per biome, up to 4 archetype weight percents). Fold the country's biome into a convention-C1 snapshot field; `wave_pool` scales integer weights only from tier 8 (rank 13+). A briefing line names the strain.
- **Files.** horde_core `enemy_catalog.rs`, `progression.rs`, `sim/director.rs`, `sim/stage_preview.rs`; `enemy_catalog.json`; briefing and world panel text; `docs/ENEMY_COMPOSITION.md`, `docs/WORLD.md`.
- **Cost.** M. **Risk.** Low-medium. Mean pool health per tier shifts by biome. The solo bot always enters the easiest country (`sim/pacing.rs:1160`), so measure with A1's route variant or the effect is under-read.
- **Moves.** With the tier-8 gate, no pin move. `RULESET_VERSION` bump; the monotone-health test per biome.
- **Verify.** The strain applies only from tier 8, uses integer weights and no RNG, freezes across a save; pacing per start biome with the route variant.

### C7. Signature boss loot with tag-targeted trade-offs

- **Problem.** Late loot has no identity: the loot-only items are quality copies or sidegrades of earlier ones (`docs/ITEM_QUALITY.md`). Tags exist and nothing reads them.
- **Change.** Append about 6-8 Legendary items with two convention-C1 flags: `signature` (left out of `drop_pool`, `src/content.rs:411-418`, so ordinary drops are unchanged) and `perks`, including a `TagDamageMultiplier` whose heavy case feeds the R5 field. Each perk carries a cost. A Capstone kill at rank 13+ adds one keyed signature drop, so the combat-RNG draw count is unchanged (a boss always takes the normal drop, `sim/combat.rs:1024`). Perks act in Personal play only, like the mastery nodes, so the Npc rows (G3.x and `tests/npc_balance.rs`, R59) do not move.
- **Files.** horde_core `content.rs`, `loot.rs`, `sim/combat.rs`, `skill_catalog.rs` (`SkillModifiers`), `campaign/loadout.rs`, the recommender and `tree_value`; `content.json`; horde_game item detail and Results.
- **Cost.** M. **Risk.** Medium: it adds survivor power while G6.11 and G6.12 are red (section 8).
- **Moves.** Rank gate 13+: no pin move. `RULESET_VERSION` bump; tests that count 68 definitions.
- **Verify.** `drop_pool` unchanged at tiers 1-4; boss draw count unchanged; signature only on a Capstone at rank 13+; a perk folds in Personal and is ignored in Npc; the v7 fixture resaves byte-identically; late defeat windows do not fall further.

### C8. A late counter branch of the tree

- **Problem.** The tree is exhausted at run 28 [27, 29] (`docs/BALANCE_AND_RUN_STATS.md:171`), after which coins buy nothing permanent. The catalogue holds 177 nodes.
- **Change.** With a new user decision (decision 19 capped additions at 8), append about 8-12 counter nodes gated at 15, 20, 30 or 40 victories that answer only the new late threats (for example earlier burrower mounds, a herald damage multiplier, hazard resistance against brood bursts). No flat stats.
- **Critic correction.** "Trap mastery 11 and 12" cannot ship: `skill_catalog.rs:684` validates `StructureRank` ranks 1-10. Widening it raises `structure_bonus` to 3.4x and deepens the trap overkill A2-A4 try to cut. Use a separate unlock instead, or drop it.
- **Files.** `skills.json` (append), `skill_catalog.rs`, `campaign/tree_value.rs`, `tests/progression_tree.rs:78` and `tests/unified_journey.rs:16` (the 177 counts), `docs/SKILLS_AND_WEAPONS.md`, tree positions (unique; `skill_catalog.rs:673`).
- **Cost.** M. **Risk.** Medium: it needs the decision and adds power where G6.11 and G6.12 are red.
- **Moves.** Gates of 15+ victories stay unbought in the progressed staging (victories 12). `RULESET_VERSION` bump; G5.3, G5.9, G5.11. Tree rules 1-6 (dominance, the 2.5x depth-peer limit) constrain the prices; the append-only FNV over the first 169 nodes holds; `tier_costs_sit_in_design_bands` stays green with gates of 5 or more victories. The nodes answer threats from C2, C3 and C5, and "Herald hunter" needs C7's `TagDamageMultiplier`.
- **Verify.** `cargo test --locked -p horde_core --test progression_tree`; `pacing --values`; `goal5_tree_outlasts_second_liberation`.

## 7. Direction D: open technical work

The production plan's §12.10 open items, the agent-runnable human checks, engineering health and script and doc debt. B1 (the HEAD capture run) is the shared capture item and sits in direction B.

### D1. Pin the closing perf acceptance

- **Problem.** No test pins `PERF_BUDGET.json acceptance.closing`. `RECORDED_CPU_GATED_SCENES` is still `["progressed-peak", "stress"]` (`crates/horde_game/src/perf_budget.rs:130`), and the note says "no test pins it yet". Attempt 2 (`e54faa88`) has all four passes at exit 0 with `gate_clean` true.
- **Change.** Add `perf_budget_records_the_closing_acceptance`. Assert: hex commits, two-entry windows and `attempt_2.commit` different from `closing.commit`; attempt 1 keeps at least one `gate_clean` false (R34); the four attempt-2 passes (dev-novsync, playtest-novsync, dev-vsync, playtest-vsync) are present, exit 0 and are clean; CPU p99 at most the gate for every scene the budget gates, derived from `/scenes` so fx-stress is included; interface_draw within `scenes.<scene>.interface_draw_mean_ms_max` for peak (1.1904 / 1.0752 ms) and progressed-peak (G1.10, 1.129 / 1.030 ms); the A/B ratio within tolerance, with different sha256 values for A and B; `egui_texture_uploads_worst`, `frames_over_50ms_worst` and `driver_stalls` at their maxima; battlefield primitives and gated vertices within limits (fx-stress vertices ungated, R65). Update the closing note and §12.10.
- **Critic corrections.** Assert the texture uploads on attempt 2 only: attempt 1 has no `egui_texture_uploads_worst`. Attempt 1's passes carry `invocations` and attempt 2's do not, so `perf_budget_records_the_w2_47_regate` (`:1022`) cannot be copied as written.
- **Files.** `perf_budget.rs` (tests), `docs/PERF_BUDGET.json` (`acceptance.closing.note`), production plan §12.10.
- **Cost.** S. **Risk.** Low. `twelve_ms_is_never_an_acceptance_gate` (`:1411`) walks every string, so the new note must not say "12 ms".
- **Moves.** One test in all three configurations.
- **Verify.** Fail first by editing an attempt-2 value past its gate; then `cargo test --locked -p horde-defense perf_budget` in the default, `--no-default-features` and `--features dev` configurations, and `check_views.py` for the plan edit.

### D2. A node-envelope collision test for the skill graph

Merged with the polish candidate that extended the wires test to the compact zoom.

- **Problem.** design-progression-tree §3 item 8 asks that milestone-gated nodes do not collide: "validator plus capture". The catalogue validator rejects only exact duplicate positions (`crates/horde_core/src/skill_catalog.rs:673`). The upgrade-graph tests cover wires (`ui/progression/upgrade_graph/tests.rs:83`), focus (`:119`) and stub labels (`:434`, `:568`); none asserts that one node's envelope clears another's.
- **Change.** Add `every_node_envelope_clears_every_other_at_every_zoom`: project every node's icon and caption at the zoom set of `:434` plus the compact canvas default scale (compact switches at width < 900 or height < 430, `ui/progression.rs:404`), and assert no two envelopes overlap, naming the pair and the scale. `wires.rs:102` has `overlaps`. The capture half is B1's `skills-firearms-column` still.
- **Files.** `ui/progression/upgrade_graph/tests.rs`; production plan §10 record.
- **Cost.** S. **Risk.** Low. A failure is a real overlap; the fix moves a node position, which touches no id or save.
- **Moves.** One test.
- **Verify.** Fail first by moving a Firearms node onto its neighbour in a scratch edit; `cargo test --locked -p horde-defense upgrade_graph` in all three configurations.

### D3. A workspace `scripts/verify.ps1`

- **Problem.** The full verify is a hand-run list (production plan §12.6; logs in `target/closing-verify/`). The root helper's verify block (`scripts/dev.ps1:68-78` at the repo root) and CI, which calls it, run `--features dev` tests but never `cargo test -p horde-defense --no-default-features`, the run that caught the A6-18 failure (§12.6). CI runs none of the Python or script checks. R6 forbids root edits.
- **Change.** Add `scripts/verify.ps1` (5.1-safe, `CARGO_TARGET_DIR` through `Native.ps1`) that runs the §12.6 steps in order, logs each to `<target>/verify/<stamp>/NN-<step>.{out,err}.txt` and prints a summary with exit codes. Switches: `-Quick`, `-From <n>`, `-NoScripts`. Document it in AGENTS.md and VALIDATION.md with the `powershell -NoProfile -ExecutionPolicy Bypass -File` form (`docs_invoke_scripts_with_windows_powershell`, `repo_checks.rs:726`). Leave the root line byte-identical (`root_helper_line_is_unannotated`, `:828`). Raise the CI gap as a user decision.
- **Files.** `scripts/verify.ps1`, `scripts/tests/run_script_tests.ps1`, `AGENTS.md`, `docs/VALIDATION.md`.
- **Cost.** M. **Risk.** Low: tooling only.
- **Moves.** Nothing in rules, pins or gates.
- **Verify.** One run on a clean HEAD matches §12.6; `run_script_tests.ps1`; `cargo test repo_checks`; a scratch failure that only `--no-default-features` sees turns that step red.

### D4. Narrow the `--features dev` test run (policy change)

- **Problem.** The dev test binary is the slowest verify step: 996 tests in 239.65 s, against 133.93 s default and 56.20 s `--no-default-features` (`target/closing-verify/05c`, `06`, `07` logs). `feature = "dev"` appears 37 times in 7 files.
- **Critic correction.** `dev` changes Bevy itself: `dev = ["bevy/file_watcher", "bevy/track_location", "bevy/sysinfo_plugin"]` (`crates/horde_game/Cargo.toml:36`). `track_location` changes ECS change tracking for every App a test builds, so the 7 files are not the whole dev surface. The root helper and CI keep the full dev run, so this saves local time only.
- **Change.** If the user approves: keep clippy `--features dev`; run dev tests with name filters for the dev-gated modules; add a repo check that fails when a file outside the filter gains a `feature = "dev"` cfg (B6 adds one in `ui/battle/layout.rs`). Keep the full dev run at closing gates.
- **Files.** `AGENTS.md`, `docs/VALIDATION.md`, `crates/horde_game/src/repo_checks.rs`, `scripts/verify.ps1`.
- **Cost.** S. **Risk.** Medium-low: a `track_location` difference outside the filter goes untested between closing gates.
- **Moves.** Verify wall time only.
- **Verify.** Diff `-- --list` under the filter against the full dev list; a scratch dev cfg in an unlisted file fails the repo check.

### D5. `target/` retention: a dry-run prune script

- **Problem.** `target/` has no retention rule. Measured during the 2026-11-17 research pass: `target/production-after` held 45 GB, `target/closing-head/target` 3.5 GB, and four nested git-archive build trees exist (`closing-head/target`, `closing-packaging/target`, `closing-rebless/w4-09-target`, `w711/target`). These figures change with every build; recheck them before acting. Evidence the human checks name lives only under `target/` (for example `target/production-after/w1-64/text-subpixel-on-b` and `damage-motion`, `docs/ACCEPTANCE.md:250`, `:252`), and the docs cite many `target/` paths.
- **Change.** Add `scripts/prune_target.ps1`, dry run by default, printing what it would remove and the size. `-Apply` deletes only after the user confirms. Rules: remove nested build `target/` dirs of git-archive trees but keep their head, zip and sha records; remove playtest zips that are neither the current verified package (§12.7) nor cited; remove capture and perf runs no tracked doc cites (scan `docs/` including `docs/plans/**`, `README.md`, `AGENTS.md`); never touch `production-before` or a cited path.
- **Files.** `scripts/prune_target.ps1`, `scripts/tests/run_script_tests.ps1`, `AGENTS.md`.
- **Cost.** S-M. **Risk.** Medium if applied carelessly: deletion is permanent. Run the cite scan before any `-Apply`; the user approves each deletion.
- **Moves.** Nothing in game, pins or gates.
- **Verify.** Script tests: a cited path is kept, an uncited run is listed, a nested target is listed while its head record is kept.

### D6. Run the two agent-runnable human checks and build a review kit

- **Problem.** All 11 human checks are pending (§12.8). Two need no human judgement:
  - The integrated-GPU run needs only a command; `perf_budget_integrated_gpu_is_pending_or_has_report` (`perf_budget.rs:473`) already accepts a measured status. The protocols disagree: `docs/ACCEPTANCE.md:247` says "on progressed-peak and stress at both sizes", while production plan §11 and §12.8 give the command with no `-Scene`, which defaults to `all` (peak, progressed-peak, stress and fx-stress; `scripts/measure_performance.ps1:76`, `:100`).
  - The R54 world rebuild needs network access and a recorded exit code.

  Two visual checks (stepped damage pop, whole-pixel text) point at W1-64 evidence under `target/production-after/w1-64/` (`docs/ACCEPTANCE.md:250`, `:252`), which predates later `damage.rs` changes (`b33f03fe`).
- **Change.** (1) Run the integrated-GPU pass in an R30 quiet window after aligning ACCEPTANCE.md:247 with §11 on the scene set. Set `hardware.integrated_gpu` to measured, and also update or rule on the two strings `acceptance.closing.integrated_gpu` and `acceptance.closing.attempt_2.integrated_gpu` ("pending (not measured; informational only)") so the records agree (critic). (2) With the user's approval for the Natural Earth download, run `generate_world.py --rebuild --download --check` and record the exit code. (3) Add `scripts/prepare_human_checks.ps1`: from a HEAD capture run (B1, plus B2 and B8 scenes) write `target/human-checks/<sha>/index.html` with each check's stills and motion, 4x nearest crops for the upscale and text rows, the protocol text and a pass / fail / notes record. Listening, camera feel, click-to-photon and the difficulty playtest stay human.
- **Files.** `docs/PERF_BUDGET.json`, `docs/ACCEPTANCE.md:247-257`, production plan §11 and §12.8, `scripts/prepare_human_checks.ps1`, `scripts/tests/run_script_tests.ps1`.
- **Cost.** M (each run S; the kit M). **Risk.** Low. `acceptance_lists_every_pending_human_check` (`repo_checks.rs:2148`) needs §11 and ACCEPTANCE.md to change together.
- **Moves.** `hardware.integrated_gpu` pending -> measured; pending checks 11 -> 9 if both runs close.
- **Verify.** The perf pin takes its measured branch; `acceptance_lists_every_pending_human_check`; `generate_world.py --check`; kit script tests (a missing capture exits non-zero).

### D7. Headless texture-upload checks for the boss banner, low-HP band and Auto-sell panel

- **Problem.** Production plan §12.9 item 4 kept the default: the aggregate interface_draw and cpu_frame gates "cover" the boss banner, the low-HP band and an open Auto-sell panel. None of them appears in a measured scene (`$sceneChoices`, `scripts/measure_performance.ps1:100`), so no gate sees them. Per-frame text like the banner countdown is the known way the atlas gets re-uploaded; `b33f03fe` fixed exactly that miss for the CRIT label.
- **Change.** Keep the default (no perf fixture, frame report v3 unchanged). Add review-gated headless tests on the existing fixtures (`boss-warning`, `low-hp`, `equipment-autosell`; `ui/review/fixtures/production.rs:111`, `:116`, `:127`) that draw warm-up plus about 300 frames with the changing values and assert texture uploads after warm-up equal `EGUI_TEXTURE_UPLOADS_AFTER_WARMUP_MAX` (0, `perf_budget.rs:104`). Add a fourth test on a battle frame with the banner counting down. Add an `#[ignore]` `measure_unmeasured_interface_load` that prints tessellated vertices and primitives per fixture beside progressed-peak's, as information for the next re-gate. Design with B3.
- **Files.** `ui/review/tests.rs` or `ui/battle/tests.rs`; glyph warm-up in the affected module only if a test finds an upload; production plan §12.9 item 4.
- **Cost.** S-M. **Risk.** Low. The tests carry `cfg(feature = "review")`, as every review-scene test does (the A6-18 lesson).
- **Moves.** Tests only, default and dev configurations.
- **Verify.** Fail first by disabling one warm-up size; the new tests pass in default and dev, and the `--no-default-features` build leaves them out. Run the ignored measurement once and record its numbers in §12.9 item 4.

### D8. Script and doc debt

- **Problem.** Three PowerShell 7 scripts are allowlisted (`repo_checks.rs:51-55`) and `PENDING` is empty (`:58`), so nothing tracks their port. `trace_npc.ps1` builds with plain `cargo` (`:22`) and starts `target/debug/horde-defense.exe` from a hard-coded path (`:47`). `capture_player_models.ps1` builds with plain `cargo` (`:37`) but honours `CARGO_TARGET_DIR` (`:40-43`). Both skip `Invoke-HordeBuild`'s foreign-source check. `verify_player_models.ps1` builds nothing and only needs a 5.1 port. Only `trace_npc.ps1` hard-codes `target/`, as AGENTS.md says.
- **Stale records.** The `acceptance.closing.note` (D1); `ACCEPTANCE.md:247` against §11 (D6); two `...-30847f9-...` zips in `target/playtest/` that no record describes (§12.7); the §12 header still names HEAD `d4b8a197`; §12.5 lists four commits after `d53cfe0f`, not ten (B1 owns that fix).
- **Change.** Port the three scripts to 5.1 with `Native.ps1` (`Resolve-HordeTarget`, `Invoke-HordeBuild`), each listed in `PENDING` with its porting commit and then removed from both lists. Drop the AGENTS.md sentence about `trace_npc.ps1`. One docs commit fixes the stale records it owns: the §12 header and the `30847f9` zips (recorded in §12.7, or removed only with the user's approval; section 10).
- **Files.** The three scripts, `repo_checks.rs`, `AGENTS.md`, `docs/ACCEPTANCE.md`, the production plan §12.
- **Cost.** S-M. **Risk.** Low: evidence tools outside the verify list.
- **Moves.** Nothing in game, pins or gates.
- **Verify.** `pending_list_only_names_existing_unported_scripts` (`repo_checks.rs:439`); `run_script_tests.ps1`; `trace_npc.ps1 -NoBuild` with `CARGO_TARGET_DIR` elsewhere starts that exe; `check_views.py`.

### D9. Re-run the review package `-NativeCapture` check at the current package

- **Problem.** The playtest package was rebuilt and verified at `99d013af`, but the review package's `-NativeCapture` run was last done at `61bced1` and "was not repeated at 99d013af" (production plan §12.7). Five code commits came after `61bced1a`: `400c8efb`, `b33f03fe`, `27d763cb`, `a0278a08` and `ef778e28` (the executor change and the glyph warm-up among them). The five UI commits are older and were in the package built at `61bced1`.
- **Change.** Build the review package at HEAD and run `verify_playtest.ps1 -NativeCapture`; record the result in §12.7 and VALIDATION.md.
- **Files.** Records only.
- **Cost.** S. **Risk.** Low.
- **Moves.** None.
- **Verify.** `verify_playtest.ps1` prints PASS.

## 8. Conflicts and dependencies between directions

**Conflicts.**
1. **Pixel baseline first.** B3, B4 and B5 change pixels. If any lands before B1, `compare_captures.py` cannot separate the post-`d53cfe0f` commits from the polish. B4 also changes nearly every camp still and presses on the committed-pairs budget (39,099,685 of 40,000,000 B).
2. **The trap stance.** A4 option (a) makes traps weaker against some elites. C2 (burrower) makes traps the main answer to burrowed hostiles, and C8 counters lean on traps. A3's supply cut means fewer traps, which makes burrowers more lethal. Pick one stance on traps before A3, A4, C2 or C8 starts.
3. **Survivor power against overkill.** C7 (signature loot) and C8 (counter branch) add survivor power; A2 and A3 try to cut late overkill so G6.11 and G6.12 can close. Measure with A1 first and fold any loot or node power into A2's calibration; they cannot be tuned apart.
4. **One RULESET bump.** A2-A5 and every C candidate each claim their own `RULESET_VERSION` 5 -> 6. P3 gives one provisional ledger and one re-bless per wave. Batch them into one rules wave with one bump, or each later change needs another bump.
5. **Snapshot fields.** A2 reads `run_history`; C4, C6 and C7 add `ProgressionSnapshot` or item fields. All must keep convention-C1 defaults and `skip_serializing_if` so `v7_fixture_loads_validates_and_resaves_byte_identically` stays green. Coordinate the snapshot additions in one change.
6. **One floors table.** A5 (steeper `rate_cap` and `start_rate`) and C1 (rows 29+ mapping to tiers 12-15, boss health flat) edit the same `pressure_floors` rows. Write one table.
7. **Dev filter.** B6 adds a `dev` cfg in `ui/battle/layout.rs`. D4's filter and repo check fail unless the filter lists it. Land D4 after B6, or include the file.
8. **Pruning evidence.** D5 can delete evidence that D6's kit and ACCEPTANCE.md rows point at (`target/production-after/w1-64/...`). The cite scan must cover `docs/ACCEPTANCE.md` and `docs/plans/**`, and the user approves each deletion.
9. **Relief farming.** If a withdrawal or a deliberate defeat counts toward A2's relief, players can farm easier runs. This conflicts with the design stance that difficulty follows rank (design-enemy-balance §1). A2 must define it.

**Merged duplicates.** The two HEAD capture-run candidates are B1. The two node-collision checks are D2. A4 overlaps C2, C5 and C1 (all aim to make the survivor take damage late); they share the trap-stance decision. B2 and B8 feed D6's kit. B3 and D7 share the warm-glyph contract. D6 and D8 both fix the ACCEPTANCE.md:247 mismatch; D6 owns it. B1 and D8 both name the §12.5 ten-commit correction; B1 owns it.

**Dependencies.**
- A1 before A2-A5, C6 and C7 (calibration and routing).
- The R56 human difficulty playtest before any rules wave (R60 says it judges the late game). A1's telemetry and A6's streak readout are inputs to it (critic addition).
- B1 before B3, B4, B5 and D6's kit. B2 and B8 before the kit.
- B4 only after the whole-pixel text check; B6 before any shipped zoom ceiling decision.
- D1 before the D8 note edit; D3 and B6 before D4.
- C5 before A4 option (a), which needs C5's `min_rank`.
- C2, C3, C5 and C7 before C8, whose nodes answer their threats and use C7's `TagDamageMultiplier`.

## 9. Recommended order

1. **Technical housekeeping and the capture baseline** (no rule, no pin): D1, B1, D2, D8, D9, D7. Then D6's two agent runs (the world rebuild only with download approval). These close the production pass's loose ends and give every later change a clean baseline. D7 goes here and not beside B3 because it adds tests only and moves no pixels; B3 changes pixels, waits for B1, and reruns D7's tests. D3 and D5 when the user approves them, and D4 after D3 and B6 if approved.
2. **Review-led polish** (presentation only, no re-bless): B7, B2, B8, then D6's kit, then B3 and B5 step 1, and B6. Run the human checks from the kit. B4 and B5 step 2 only if their checks ask for them.
3. **Late-game measurement and the difficulty playtest** (no rule change): A1 and A6, then the R56 human difficulty playtest with A6's readout and A1's numbers beside it.
4. **One rules wave** after the playtest verdict and the decisions in section 10: the chosen subset of A2-A5 and C1-C6, one floors table, one snapshot change, one `RULESET_VERSION` bump and one re-bless, with pacing and smoke before and after (ISO-7).
5. **Power-adding content** (C7, C8) last, calibrated inside A2's heat once it exists.

## 10. Decisions needed from the user

The decisions are numbered so they can be answered by number.

**Before any direction.**
1. Which directions to run, and in what order (section 9 is a recommendation).

**Before any rules wave (A2-A5, C1-C8).**
2. Whether to wait for the R56 human difficulty playtest before any rules change (recommended).
3. Whether to lift the R60 freeze on balance data, bots and acceptance rows. Direction C needs it too: C1 edits `progression.json` and the G6.14 row text, and C1-C6 change the G6.16 coverage set.
4. Batching all rules changes into one wave with one `RULESET_VERSION` bump.
5. The trap stance (section 8, conflict 2).

**Direction D (technical).**
6. Approve `scripts/verify.ps1` (D3) and whether the CI gap stays (R6 forbids a root edit; closing it needs a root change).
7. Approve narrowing the dev test run (D4), which changes the AGENTS.md verify line, with the full run kept at closing gates.
8. Approve the prune rules (D5) and each `-Apply` deletion.
9. Approve the Natural Earth download for the R54 rebuild (D6).
10. The two `...-30847f9-...` zips in `target/playtest/` (D8): record them in §12.7, or delete them. Deletion is permanent and needs approval.

**Direction B (polish).**
11. Accept counting `sky_flash_paints_under_cue_pass` toward the storm half of "Night and storm look" and narrowing that check (B2).
12. After the A/B (B6): whether to ship a zoom ceiling, which amends the camera requirement (§2.2 "Camera (R1)").
13. After the whole-pixel text check: whether camp text gets subpixel glyphs (B4).

**Direction A (late-game).**
14. Whether to add an outcome-driven threat axis (A2), which design-enemy-balance §1 does not have; and if so, whether the boss is excluded from heat or the snapshot validator is widened; and what counts as streak-breaking (withdrawal versus defeat).
15. Whether to cut the free rank power (A3: fortification knee, supply cap, engineering stacking).
16. Whether A6's camp card that offers Hard ships before the R56 playtest, which changes what that playtest observes, or stays behind `dev` until after it.

**Direction C (content).**
17. Which candidates, and whether all gate at rank 13+ or tier 12+ to keep the pins.
18. Whether decision 9 still holds (no new ImageGen sheets, no music files).
19. A new node allowance past decision 19 (C8).
20. Whether power-adding content (C7, C8) may land while G6.11 and G6.12 are red.

## 11. Pending human checks

The 11 pending human checks and their protocols are listed in the production plan [§12.8](2026-11-05-production-pass.md#128-pending-human-checks) (and §11, and docs/ACCEPTANCE.md "Pending human checks"). They are not copied here. Candidates that touch them:

| Check | Candidates |
|---|---|
| Camera feel | B6 |
| Stepped damage pop | B3 |
| Whole-pixel text placement | B4 |
| Sheet upscale on 3-lane sprites | B5 |
| Weapon-classes review | B1, D6 kit |
| Night and storm look | B2, D6 kit |
| Listening pass, click-to-photon | stay human; no candidate |
| World generator (R54) | D6 |
| Human difficulty playtest | A1, A6 (inputs); gates step 4 of section 9 |
| Integrated-GPU run | D6 |
