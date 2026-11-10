# Production pass: Last Light (horde-defense)

Status: **wave 1 closed under R25 at HEAD ab03da9** (2026-11-07; §7, "Wave-1 close"); design frozen 2026-11-05. Brief: `docs/plans/2026-11-04-production-pass-prompt.md`. Branch `claude/horde-production-pass`, worktree `.claude/worktrees/horde-production-pass`, cut from local `main` at `7bd5de8`.

This file is the plan of record. The detail lives in `docs/plans/production-pass/`:

| File | Contents |
|---|---|
| `schema-v7.md` | The final frozen v7 schema: every saved field, migration step, validation rule, command, catalogue change and settings field, plus the digest ledger. |
| `waves.md` | The master wave plan, waves 1-7, one row per commit with owner, fail-first tests and dependencies. |
| `acceptance.md` | The acceptance matrix: every brief criterion, user restatement and ruling mapped to its proof and wave. |
| `design-<remit>.md` (10 files) | Each expert remit's synthesized spec, followed by the integration rulings that supersede parts of it. |

Precedence when documents disagree: user decisions, then orchestrator rulings R1-R35 and RL, then `schema-v7.md` / `waves.md` / `acceptance.md`, then the "Integration rulings" section of each design doc, then the synthesized spec text above it.

## Contents

1. [Context and baselines](#1-context-and-baselines)
2. [User decisions](#2-user-decisions)
3. [Loop map](#3-loop-map)
4. [Findings with evidence](#4-findings-with-evidence)
5. [Chosen and rejected fixes](#5-chosen-and-rejected-fixes)
6. [Final v7 schema](#6-final-v7-schema)
7. [Master wave plan](#7-master-wave-plan)
8. [Acceptance matrix](#8-acceptance-matrix)
9. [Measurement tables (before / after)](#9-measurement-tables-before--after)
10. [Capture findings ledger](#10-capture-findings-ledger)
11. [Human checks pending](#11-human-checks-pending)
12. [Outcome](#12-outcome)

---

## 1. Context and baselines

### 1.1 Starting point

- Local `main` at `7bd5de8` (merge of the NPC Framework living-village work) includes the foundation pass (`3a85239`, `bddf820`, `b5dd12b`).
- Last gate from the brief: 644 workspace tests and 343 `--features dev` tests; `SAVE_VERSION 6` with fixtures `save_v1..v5`; 6 GOLDEN digests and `GAMEPLAY_TRACE`.
- Hardware for every number below: Intel Core Ultra 9 185H, NVIDIA RTX 1000 Ada Generation Laptop GPU, Vulkan, Windows scale 2.0. "1440x900" is 2880x1800 physical at UI scale 1.0; "1050x700@1.35" is 2100x1400 physical at UI scale 1.35.

### 1.2 Pacing (wave 0)

Source: `target/production-before/pacing.txt`, produced by `cargo run --locked --release -p horde_core --example pacing -- 60 --runs` (one seed per profile).

| Milestone (runs / battle min) | expert, tree first, Germany | novice, tree first, Germany | novice, gear first, Luxembourg |
|---|---|---|---|
| First victory | 4 / 13 | 3 / 9 | 2 / 6 |
| First liberation | 14 / 60 | 14 / 59 | 5 / 18 |
| Tier 1 affordable (930 coins) | 3 / 10 | 3 / 9 | 3 / 10 |
| Tier 2 affordable (2790 coins) | 5 / 17 | 5 / 17 | 5 / 18 |
| Tier 3 affordable (4975 coins) | 7 / 25 | 6 / 21 | 6 / 21 |
| Tier 4 affordable (6655 coins) | 7 / 25 | 7 / 25 | 7 / 26 |
| Fortifications unlocked | 4 / 13 | 3 / 9 | 3 / 10 |
| Mara unlocked | 14 / 60 | 14 / 59 | 5 / 18 |
| Recruitment unlocked | 14 / 60 | 14 / 59 | 6 / 21 |
| Parallel defenses unlocked | 17 / 72 | 17 / 72 | 8 / 30 |
| Country operations unlocked | 23 / 97 | 23 / 97 | 18 / 72 |

Readouts from the per-run logs:

| Profile | Defeats (runs) | Last defeat | Straight victories after it | Last node bought (tree exhausted) |
|---|---|---|---|---|
| expert, tree first, DEU (the default `expert-deu`) | 5 (1, 2, 3, 8, 10) | run 10 | 50 (runs 11-60) | run 24 |
| novice, tree first, DEU | 5 (1, 2, 4, 7, 8) | run 8 | 52 (runs 9-60) | run 24 |
| novice, gear first, LUX | 2 (1, 3) | run 3 | 57 (runs 4-60) | run 26 |

Orchestrator summary of the same file: first victory on run 4, then an unbroken victory streak of more than 50 runs, and the whole tree exhausted by run 24-26. That breaks goal 6 (defeat rate 10-25% per 20-run window, no 15-win streak after run 10), goal 5 (tiers at about runs 4/8/12, tree outlasting the 2nd liberation; today tiers 1-4 are all affordable by run 7) and the unlock order (Mara and Recruitment arrive in the same run in every profile).

### 1.3 Smoke (wave 0)

Source: `target/production-before/smoke.txt` (`cargo run --locked -p horde-defense -- --smoke`).

```text
PASS: 242 country profiles; 5 encounters; atomic resume identical; fresh companion gate is atomic; legacy autonomous Defeat: 33 kills, 110 coins in 119.0s.
Personal stage1 attempt1 Defeat 173.0s · 101 kills · 330 coins retained
Personal stage1 attempt2 Defeat 208.4s · 148 kills · 488 coins retained
Personal stage1 attempt3 Victory 189.5s · 147 kills · 610 coins retained
Personal stage2 attempt1 Victory 190.4s · 168 kills · 979 coins retained
Personal stage3 attempt1 Victory 222.4s · 213 kills · 1599 coins retained
Personal stage4 attempt1 Victory 221.3s · 227 kills · 2016 coins retained
Personal stage5 attempt1 Victory 254.2s · 284 kills · 2741 coins retained
Npc stage1 attempt1 Defeat 179.0s · 67 kills · 222 coins retained
Npc stage1 attempt2 Victory 188.0s · 70 kills · 354 coins retained
Npc stage2 attempt1 Victory 197.0s · 101 kills · 596 coins retained
Npc stage3 attempt1 Victory 229.0s · 107 kills · 881 coins retained
Npc stage4 attempt1 Victory 221.0s · 114 kills · 1181 coins retained
Npc stage5 attempt1 Victory 250.0s · 146 kills · 1615 coins retained
```

### 1.4 Frame reports (wave 0, dev build, VSync on)

Source: `target/production-before/perf/*.json` (frame report v2; 90 warm-up and 300 sample frames; work is wall-clock ms per frame for simulation and interface).

| Scene | Size | Field | Enemies | Mode, rank | Mean / p95 / p99 frame (ms) | Interface mean / max (ms) | Simulation mean (ms) | Frames > 50 ms |
|---|---|---|---|---|---|---|---|---|
| peak | 1440x900 | 7x16 | 52 | Npc, 4 | 16.67 / 17.09 / 17.66 | **1.190** / 4.31 | 0.050 | 0 |
| peak | 1050x700@1.35 | 7x16 | 52 | Npc, 4 | 16.67 / 17.10 / 17.60 | **1.075** / 6.55 | 0.050 | 0 |
| progressed-peak | 1440x900 | 17x30 | 225 | Npc, 50 | 16.67 / 17.16 / 17.51 | **1.613** / 4.84 | 0.162 | 0 |
| progressed-peak | 1050x700@1.35 | 17x30 | 225 | Npc, 50 | 16.66 / 17.22 / 17.75 | **1.472** / 7.38 | 0.164 | 0 |

- VSync caps every frame near 16.7 ms, so p99 (17.5-17.7 ms) says nothing about CPU headroom. That is why frame report v3 and `-VsyncOff` runs exist (G1.1, G1.9).
- **Goal-1 interface target** (30% below wave 0, same profile and scene): progressed-peak interface mean **≤ 1.129 ms** at 1440x900 and **≤ 1.030 ms** at 1050x700@1.35 (0.70 × 1.6128 = 1.12896 and 0.70 × 1.4719 = 1.03033, rounded to the nearest microsecond, not floored; R19). These numbers are pinned in `docs/PERF_BUDGET.json` by `perf_budget_baseline_is_wave0`.
- CPU gate (new, not measurable at wave 0): with `-VsyncOff`, progressed-peak and stress CPU (main + render-world) p99 ≤ 10 ms and 0 frames over 50 ms.

### 1.5 Before-captures (wave 0)

- `target/production-before/{normal,reduced}/`: **156 of 164** stills (41 scenes × 2 sizes × 2 modes).
- The 8 failures are `progressed-battle` and `leveled-armory` at both sizes in both modes: a panic at `crates/horde_game/src/ui/review/fixtures.rs:138:14`. Fixed in W1-05; their before evidence is captured by W1-06 on the fixed build (logged as CF-0-01).
- None of the brief's 11 new review scenes exist yet; they are registered in W1-54, W3-27 and W5-09/10, with `before_proxy` pairs.
- **Supplemental before set (W1-06):** `target/production-before/supplement/{normal,reduced}/`, **28 of 28** stills (7 scenes × 2 sizes × 2 modes), with `captures.json` (version 1) and `scenes.tsv`. Captured by `scripts/capture_review.ps1 -Modes normal,reduced -ContinueOnError` on the build at efd9c29 (after the W1-05 fixture fix and its review follow-ups, before any presentation change; W1-38 is the first). Executable sha256 `1157bfb124ed…`, `windows_scale` 2, `player_logs_unchanged` true, 0 failures. `scripts/compare_captures.py` does not exist until W7-11 (see 10.1, "Pairing"); W7-11's compare_captures will merge this folder into the before set, so the full before set will be 184 stills (156 wave-0 plus 28).
- **Wave-0 stills carry no scale record:** `target/production-before/{normal,reduced}/` has no `captures.json` and no `scenes.tsv`, so the 156 wave-0 stills record no `windows_scale`, which the equal-scale pairing rule (design-qa-captures.md: pairs are comparable only at equal `windows_scale`, and compare_captures refuses mismatched runs) reads. Their scale is inferred as 2 from the PNG sizes: all 78 large stills are 2880x1800 (1440x900 x 2) and all 78 small stills are 2100x1400 (1050x700 x 2), checked 2026-11-07. W7-11 has no sidecar to read for these two folders, so its pairing needs this inferred scale 2.
- The 7 scenes are the 2 that crashed in wave 0 (`progressed-battle`, `leveled-armory`) and the 5 that wave 0 did not list (`journey-victory`, `journey-liberated`, `journey-upgrades`, `skills-search`, `max-lanes`). Normal-mode sha256 prefixes:

| Scene | 1440x900 | 1050x700@1.35 | Reduced identical to normal (large / small) |
|---|---|---|---|
| progressed-battle | 1505dd69e6ab | 1a8b6bdc6567 | no / no |
| leveled-armory | b6163c62d2a9 | 961eb0db88bb | no / yes |
| journey-victory | e0c97fd928e9 | 0cf2e3c41209 | yes / no |
| journey-liberated | 743c11773db0 | 6ab5e301ebcd | yes / yes |
| journey-upgrades | bf2c07d86718 | 22742667212c | yes / yes |
| skills-search | 579f35076d5b | 4abe2acf7db4 | yes / no |
| max-lanes | f53dea21fffe | 4cad1a8572b9 | no / no |

---

## 2. User decisions

### 2.1 Answers to the brief's "decide before starting" list

| # | Topic | Decision in force | Where it lands |
|---|---|---|---|
| 1 | Screens | Results and Upgrades become two screens (`Screen::Upgrades`). | W3-03 |
| 2 | Takeover | Ready recruits and Mara can defend the main run, switching only between defenses. **Reading (R2):** Mara stays selectable from the Companion node (today's behaviour); recruits from the Recruitment node. No roster merge. | W2-11, W2-12 |
| 3 | Takeover mode | `Mode::Npc` rules with the defender's own loadout. **Reading (R2):** the serde key is `"defender"` on Attempt and Report; an absent value means the mode owner (Player for Personal, Mara for Npc), so GOLDEN does not move for this reason. One status model (`RecruitStatus::Defending`) and one npc source of truth (the roster entry). The takeover deploy stamps the recruit's appearance, not Mara's Female. | W1-10..17, W2-11 |
| 4 | Credit | The winning defender gets XP/CP; Personal `first_clear` XP unchanged. | W2-11 |
| 5 | Recruiting | Gated by its node; liberation Results shows "Recruit from <Country>" or names the node. | W2-20, W3-19 |
| 6 | Auto-sell | Per-profile rule on Campaign, off by default; strictly below Uncommon; settlement only, this run's drops only, including expedition and operation loot (`apply_to_npc_runs` on); `keep_upgrades` judged against the defender who ran the battle; quality None never sold; "Apply rule to stash now" confirms first. | W1-23, W2-14, W2-16, W3-21 |
| 7 | Quality | Option (b): relative to base quality; existing items keep stats and value. | W2-07 |
| 8 | Save format | **SAVE_VERSION 7: YES** (asked and approved). `save_v6.json` and `save_v7.json` fixtures. | W1-01, W1-14, W1-63 |
| 9 | Assets | **New gun, enemy and UI audio: YES. No new music files. No new ImageGen sheets.** Every file gets provenance (generator, seed, sha256) and an `assets/CREDITS.md` line. | W1-57, A6-05 |
| 10 | Legacy files | Archive (user list, see 2.3); nothing deleted. | W1-57, W1-58 |
| 11 | Hardware and latency | RTX 1000 Ada laptop; 1440x900@1.0 and 1050x700@1.35; 60 FPS; ceiling 225 enemies, stress 400; integrated GPU logged pending; Low latency = Mailbox (falls back to Immediate) with pipelined off; pipelined default decided by measurement. | W1-32..37, W1-61 |
| 12 | Enemies | Short-range spitter (never from the far half); in-country ramp with a capstone boss; static country difficulty about 0.85-1.6 via the generator; elites 0 below rank 3, 2% at 3, 15% at 23, Uncommon+ drops; boss phases only; Hard raises elite rate, ramp and phases; archetypes are catalog data; the threat guard is tested, not re-implemented. | W2-26..40 |
| 13 | Large countries | Stage counts kept; a regional checkpoint every 6 stages (max 3), derived, never saved per stage. | W2-26, W4-01, W4-02 |
| 14 | Level-up | A coin grant (ledger design). | W1-24, W2-22 (the settlement writer only), W2-54 (the authored values; §2.5, R32; closed by W2-54) |
| 15 | Recruits | Pick one of two archetypes; origin trait shown. | W2-20 |
| 16 | Graphics | `Event::Shot` unchanged; class from the gear snapshot; static grade per theme; storm lightning ≤ 1 pulse per 4 s **wall time** at any observation speed, ΔL < 0.2, never masking cues; reduced effects shows a static storm grade, including storm stages in non-storm themes; blood opt-in with capped decals. | W5-01..06 |
| 17 | Audio | Lane pan with a mono-compatible mix; small timbre offset per quality tier; no hover sound; a loot-reveal sound; one cue per auto-sell batch (R9). | A6-07, A6-12, A6-14 |
| 18 | Settings | Escape on the menu opens Quit; construction closes on Escape; otherwise Escape keeps the Pause binding; Camp home is the default page and owns the world clock; extra bindings, quality text/shape markers with a colour-blind palette, window mode, vsync, Music and UI volumes. | W1-31, W1-34, W3-04, W3-09, W3-10 |
| 19 | Skills | Repricing plus **up to 8 new append-only nodes: YES** (ids 170-177). | W2-41, W4-p |
| 20 | Packaging | Zipped Windows x64 playtest with version and commit stamp and a curated docs subset. | W7-01, W7-02 |

### 2.2 User restatements that override the brief text

| Topic | Decision | Proof |
|---|---|---|
| Loudness gate | A 60 s mix at **default sliders** reads **-23 ±2 LUFS integrated** with **true peak ≤ 0.95 FS**. This replaces the brief's "-16 to -14 LUFS". Recorded in ACCEPTANCE.md, AUDIO_SURVIVAL.md and this plan. | G8.5: `reference_mixed_60s_meets_loudness_gate` (both themes), `dense_battle_60s_true_peak_at_most_0_95` (A6-17) |
| Mastering (R3) | The exporter may apply an offline per-clip true-peak ceiling. A runtime master limiter is added only if the measured 60 s mix exceeds 0.95 FS true peak; the outcome is recorded here. | G8.6 |
| Music | MusicPlugin = music director + Music bus + stingers built from existing banks. Every track slot is `Track::Deferred`. The slider is labelled "Music & stingers". | G8.7 (A6-15) |
| Enemies overlap, no lane traffic jams (2026-11-06) | Enemies never block each other: no lane queue and no overtaking side step, bodies in a lane overlap, and every enemy that reaches the defense line or a live non-trap structure attacks. Structures still block (barricades, not traffic); traps still cover the lane. `EnemyPhase::Queued` stays for old saves and is never assigned; `pass_offset` only eases back to 0. Presentation fans a stack out with a static per-id stagger and paints each lane far to near. GOLDEN and GAMEPLAY_TRACE are re-blessed once for it (W1-69, the wave-1 re-bless). No balance retune here: wave 2's pacing work owns the measured shift (§9.6). | `enemies_in_one_lane_never_block_each_other`, `enemies_still_stop_at_structures`, `a_saved_queued_enemy_resolves_on_the_next_tick` (W1-69) |
| Gate 3: driver stalls (2026-11-06) | User ruling on gate 3, in chat: **"Exclude driver stalls"**. Frames over 50 ms in which the swap-chain acquire wait (present_wait, or channel_wait while the render thread is in present) holds at least 80% of the frame are classified as environmental driver stalls and excluded from frames_over_50ms. They must still be attributed (named coverage), the in-process work in that frame (main_cpu and render CPU) must stay below 50 ms, and the excluded stalls are counted and listed in the report and summary (frames_over_50ms_driver_stall). The same exclusion applies to the pipelined rule's frames_over_33ms and VSync-on FPS and p99-interval checks, computed on frames outside stalls. The gate itself (0 frames over 50 ms) is unchanged. Thresholds in PERF_BUDGET.json `driver_stall` (named coverage at least 95%). | `driver_stalls_are_excluded_from_the_gated_frame_counts`, `pipelined_channel_wait_is_a_stall_only_while_the_render_thread_presents`, `a_replaced_render_frame_still_attributes_the_channel_wait`, `driver_stall_rule_matches_the_ruling`, `driver_stall_in_process_work_reads_the_main_schedule_inside_the_window` and `driver_stall_needs_named_coverage_and_the_acquire_share` (W1-61 review); `measure_performance.ps1 -SelfTest` fixtures `driver-stall` (0) and `missing-driver-stalls` (2) (W1-61) |
| Camera (R1) | Uniform zoom-to-fit that keeps today's lane:yard proportion: base = today's per-geometry `clearing_rect` (with its 160/86 clamps), zoom = min(arena.w/base.w, arena.h/base.h) ≥ 1, centred, no texel clamp. Fewer lanes means more zoom. 17x30 and every other already-filling field stays pixel-identical. The observer window uses the same fit. Construction is an overlay panel and never rescales the arena; an INCOMING chevron marks spawns it covers; Save/Clear plan stay in the panel. Owner vfx-lighting, wave 1. | G7.9, G2.10 (W1-43..46) |

### 2.3 Archive list (decision 10 as extended by the user)

Moved with `git mv` to `horde-defense/archive/`, mirrored paths, provenance kept, nothing deleted:

| Class | Files | Destination |
|---|---|---|
| Legacy WAVs (12) | armor, boss, damage, incoming, melee, npc_shot, ready, reload, shot, structure, trap, turret_shot | `archive/audio/` (+ `archive/audio/PROVENANCE.json`, including the 8 preserved originals) |
| Root sheets (6) | actors-v1, biome-panoramas-v1, bosses-v1, checkpoint-keyart-v1, ground-v1, props-v1 (`.png`) | `archive/art/` |
| Animation strips (5) | everything in `assets/art/animation/` | `archive/art/animation/` |
| Key art | `assets/art/pixel/keyart-v1.png` (edit source of keyart-v2; its pin is retargeted) | `archive/art/pixel/` |
| Retired cue | `survival_npc_shot_0..2.wav`; `Cue::NpcShot` stays as a retired enum row | `archive/audio/` (+ `archive/audio/SURVIVAL_PROVENANCE.json`) |

Shipped WAV count: 90 today, 78 after W1-57, 75 after W1-58. `docs/audio/survival-audition.*` is not on the list and stays in place.

### 2.4 Orchestrator rulings (binding)

| Id | Ruling |
|---|---|
| R1 | Camera as in 2.2; owner vfx-lighting; wave 1 because later captures depend on it. |
| R2 | Defender identity as in 2.1 #2-3. |
| R3 | Offline per-clip true-peak ceiling allowed; runtime limiter only if the measured mix needs it. |
| R4 | No `rules_revision` field. Live legacy attempts keep their frozen snapshot fields; content-driven changes may reach continued legacy attempts; any continuation pin that moves is re-blessed with that wave's GOLDEN re-bless, with a written reason. A byte-identical legacy path is preferred where cheap (`drop_tier` None). |
| R5 | `SkillModifiers.heavy_target_damage_multiplier` may arrive later as a serde-defaulted field. |
| R6 | No root-repo files. No dev.ps1 annotation. |
| R7 | save-determinism re-blesses GOLDEN/TRACE once per wave in a dedicated commit with a reason per digest; moves expected in waves 2 and 4; a wave-1 format-only move only if the v7 schema makes it unavoidable. Superseded for wave 1 by the user decision of 2026-11-06 (§2.2): W1-69 is the wave-1 G re-bless. |
| R8 | Single owners: pacing harness v2 = progression-tree; per-archetype telemetry = enemy-balance; SHA-256, provenance and orphan helpers and `scripts/lib/Native.ps1` = qa-captures; `measure_performance.ps1` = perf-architecture on Native.ps1; storm tagging = a vfx presentation table only; regional checkpoints derived (CD presentation lines, EB Regional boss rank HP and reward x1.2 in wave-4 balance, no theme-rule change); `save_v7.json` cut at the wave-1 close and frozen; stress scene cap only behind a review/test seam (perf). |
| R9 | "50 sales produce 1 cue" means one auto-sell or stash-apply batch of 50 items gives 1 cue; single manual sales may each cue. |
| R10 | UI recruit and training prices read the core price function. |
| R11 | `Game::sale_eligibility` exists as the facade used by every sell path. |
| RL | Lightning uses wall-clock pacing; storm stages in non-storm themes get a static storm grade in reduced mode. |
| R12 | save_v6.json deviation (120,842 B, player level 33, 14 run records vs schema-v7 §11's 64 KB / level 12 / 2 records) is accepted: the fixture is frozen evidence and §11's numbers were estimates. Amend schema-v7.md §11 with a note. |
| R13 | Review recipes (crates/horde_game/src/ui/review/fixtures.rs, ui/review/fixtures/production.rs, ui/review/matrix.rs; review feature only) may call .tick(; record the exception in design-save-determinism.md Integration rulings and cite it in the architecture_tests.rs allowlist comment. |
| R14 | Per-commit capture reviews named in a done_when (e.g. W1-48 batching) are satisfied by the wave-close pair review (target/production-after/wave1 vs target/production-before, 356 captures, CF ledger rows). Note this on the W1-48 row in waves.md. |
| R15 | RULESET_VERSION is not bumped for W1-69 now; the planned W2-49 bump (2 -> 3) must name both W1-69 (no lane queue) and its own change. Document that pre-W1-69 recordings replay without a ruleset warning until then. |
| R16 | The level_grant_paid shim (neutral level-ups keep the grant ledger paid; test neutral_level_ups_keep_the_grant_ledger_paid) moves three Personal state digests, so it is scheduled as a new wave-2 commit before the wave-2 re-bless: append a row with the next free W2 id in waves.md (owner progression-tree) and a matching entry in the plan. |
| R17 | CF-1-01, CF-1-02, CF-1-03 and CF-1-07 get new wave-3 ids appended in waves.md (owner vfx-lighting, before W3-27, beside W3-14 for BattleLayout), each with its CF fail-first test; update the CF ledger rows with those ids. |
| R18 | W2-47 (stress fixture rebuild and re-gate) also owns: the sustained 15-20 FPS VSync-on stress episode, the 1050x700@1.35 VSync-on p99 interval of 19.1-19.7 ms, script-level battlefield primitive/vertex gates in measure_performance.ps1, and any egui_probe/present_wait span-ordering item not fixed in wave 1. Add these to the W2-47 row. |
| R19 | Peak interface_draw stays a not-above-wave-0 gate (fix plan §9.1 which calls it informational). 1.129 / 1.030 are the rounded 30% values (fix the 'floor' wording in the plan and acceptance.md). The 600/180 vs 300/90 window difference is accepted as documented. |
| R20 | capture_player_models, trace_npc and verify_player_models stay on the PS7 allowlist (ruling S15); no port in this pass. |
| R21 | The Custom difficulty chip keeps the basis of the preset it was clicked from (clicking Hard then Custom without edits sends Custom with the Hard basis). Fix in code with TDD. |
| R22 | Gates only tighten. BATTLEFIELD_PRIMITIVES_MAX goes 48 -> 24 (re-derived min(ceiling, ceil(max x 1.25)) from the post-W1-69 measure of 19). The stress vertex gate stays 61098 (no loosening; measured 49502). progressed-peak stays at its 40000 ceiling. live_battlefield_runs_stay_within_the_egui_gates samples every 2nd tick of the first 420 (or the exact maxima ticks). Update PERF_BUDGET.json evidence.egui_measured to the post-W1-69 maxima. |
| R23 | Pending human checks stay pending; fix the integrated-GPU check so it names how to select the adapter this machine actually has, or marks it not possible on this machine. |
| R24 | User decision (2026-11-07). `Invoke-Native` (`scripts/lib/Native.ps1`) starts native children with `[Diagnostics.Process]::Start` and a `ProcessStartInfo`, with UseShellExecute off and stdout and stderr redirected to the `<stem>.out.txt`/`<stem>.err.txt` logs. That is the accepted form of the brief's "run native exes through Start-Process with redirected streams" rule, because in 5.1 `Start-Process -PassThru` can report a `$null` ExitCode once the child has exited (fix 381e563). `>` stays forbidden, and a timeout still kills the tree with `taskkill /T /F`. Every binding text that names Start-Process for native runs points at R24 and allows this form. |
| R25 | User decision (2026-11-07): the wave-close rule. A wave closes when two review rounds in a row find nothing above minor, the perf acceptance and the full verify are green at HEAD, and every confirmed finding is fixed or explicitly ruled. Remaining minor findings carry into the next wave's review. |
| R26 | User decision (2026-11-09): the gate-3 `stress-on-r2` episode (W1-61 review; W2-50) closes as environmental: user activity during an evidence run (the laptop had been woken 5 minutes before, and the run carried window input). It was not reproduced in 28 repeats per size, 14 of them with the display on throughout. Gate 3 in the shipped default configuration (playtest, pipelined on, VSync on) can be claimed once the W2-47 re-gate passes. |
| R27 | User decision (2026-11-09): the 80% driver-stall rule (`PERF_BUDGET.json driver_stall`, acquire share 0.80) stays unchanged. Instead, `scripts/measure_performance.ps1` holds the display on for the whole run (`SetThreadExecutionState` with ES_CONTINUOUS \| ES_SYSTEM_REQUIRED \| ES_DISPLAY_REQUIRED, released in a finally block), and a run that overlaps a display-off interval (a System log Kernel-Power event 506 or 507 between its start and end) is invalid evidence, rerun like the script's other invalid runs. The summary records the display hold and any 506/507 events per run. The sample-932 stall tail is closed as display-off. |
| R28 | Orchestrator ruling (2026-11-09): the W2-08 move of the default `pacing 60 --runs` output is accepted, and pacing is re-baselined. Later commits diff the default output against the latest recorded one, `target/production-after/w2-41/pacing.txt` (sha256 e19f06694be215fe05068c542cb3f56e8741d2ed959aa5db03ba9d68d6bbe65c), not `w1-69/pacing.txt`. A wave-2 commit may move pacing on purpose if it records the before/after rows in §9.6. Reason: the 20-seed release medians (`pacing_targets`) did not move under W2-08, and W2-26, W2-32, W2-33, W2-36, W2-39 and W2-41 moved pacing on purpose anyway. W2-08 is complete. |
| R29 | Orchestrator ruling (2026-11-09): `PERF_BUDGET.json baseline.v3` is re-measured on the 7abb0ab egui_pass and present_wait spans in W2-47, before any interface_total re-gate. The old values stay under a previous key. Under R22 the gates only tighten: where the re-measured baseline would loosen a gate, the old gate value stays. |
| R30 | User decision (2026-11-09): perf evidence and machine load. (a) Quiet window: a perf pass starts only when no foreign cargo, rustc, cargo-nextest or other build or test process is running (the pass's own children are ignored) and the mean CPU load (`Win32_Processor.LoadPercentage`, sampled every 5 s) has stayed under 35% for 60 s. `scripts/measure_performance.ps1` implements it as a pre-flight, `-WaitQuiet [-QuietTimeoutMinutes N]`, on by default with a 240-minute timeout. It logs what it waited for, and a timeout exits 2 (invalid evidence). (b) Load validity: a 5 s load sampler runs beside every run, and a run with any load sample of 90% or more is invalid evidence, rerun like the other invalid runs (exit 2 if invalid twice). The summary records the load mean and max per run. Both land test-first in `scripts/tests/run_script_tests.ps1`, with the process and load queries behind shimmable functions (the `DisplayPower.ps1` pattern, edd2980) and cases for a quiet start, waiting then starting, a timeout, and a run invalid on a 90% sample then rerun. Gates are unchanged. |
| R31 | User decision (2026-11-09): the interface_total re-gate is an interleaved A/B against the wave-1-close executable. In one sitting, alternate A (the wave-1 close, sources at ab03da9) and B (HEAD), 5 pairs per size (normal and small), with the scene, profile and flags of the interface_total gate (progressed-peak, dev, VSync on, the default pipelined state). Per size, B's median interface_total must be at most 1.05 × A's median. `scripts/ab_interface_total.ps1` builds A from the ab03da9 sources (`git archive ab03da9 horde-defense`, extracted into `<target>/ab-w1/src`) with the same CARGO_TARGET_DIR, and uses the R30 quiet window and load validity. Its pass / fail / invalid-pair decision is self-tested with fixtures in `run_script_tests.ps1`. R29 still holds: `baseline.v3` is re-measured on the 7abb0ab spans and recorded, but the interface_total gate decision is this A/B, not the absolute baseline. It answers W2-47's "interface_total re-gate needs a tolerance" item. |
| R32 | Orchestrator ruling (2026-11-09): the defaults of the two §2.5 items that W2-22 opened are accepted. (1) W2-54 (progression-tree, before W2-49) authors the level grant and the rank milestones: `rewards.level_grant_xp_fraction` 0.25 and the four design-progression-tree §7 milestones in `progression.json`. It sets `PROVISIONAL` with a `PROVISIONAL_CAUSES` row and updates the pins the values move (the §2.5 item lists them). W2-49 depends on W2-54. (2) The `settle_parallel` narrowing stays: settlement runs only on an idle pass that credited a victory. |
| R33 | Orchestrator ruling (2026-11-09): W2-09's provisional cause stays class G, as its row and §6 say. `support::classify` labels the move C because outcome and trace are unchanged, but the change is a gameplay mechanic (the weighted drop pick). W2-49 records both labels in its reason table. `goal6_clamped_starts_first_victory_runs_2_to_4` (the AUT start's first victory 3 -> 5 after W2-09) is owned by the wave-4 tuning loop (W4-04..n). |
| R34 | Orchestrator ruling (2026-11-10): "A valid over in a pass masked by invalid runs" keeps its default. The over stands: the dev -VsyncOff progressed-peak gate (G1.9) is not claimed clean at W2-47, and a valid gated over may not be re-measured away, whatever the machine load was below the R30 threshold. The wave-2 close's own perf acceptance at HEAD is a new measurement, not a re-measure. To make such overs visible, `measure_performance.ps1` keeps its exit precedence (invalid evidence exits 2), but whenever a valid gated over exists it prints "VALID OVER: <scene> <size> <metric> <value> > <gate>" and summary.json gets a top-level `valid_overs` list. The self-test case lands fail-first in `scripts/tests/run_script_tests.ps1`. |
| R35 | Orchestrator ruling (2026-11-10): W2-47's R29 reading is confirmed. `baseline.v3` was re-measured on the 2339a3d renderer with only the 7abb0ab span edits, not at HEAD, and the gated progressed-peak pair kept its W1-37 values under R22. |

R12-R23 are the wave-1 close rulings (2026-11-07), recorded verbatim. Where they land:
- **R12:** schema-v7.md §11 note and VALIDATION.md "save_v6.json as cut".
- **R13:** design-save-determinism.md Integration rulings; the `.tick(` rule in `crates/horde_game/src/architecture_tests.rs`, whose allowlist already names the three files.
- **R14:** the W1-48 row in waves.md. The wave-close pair review is §10.3 (236 stills and 120 motion runs, 356 captures).
- **R15:** the W2-49 row in waves.md, the wave-1 result in §6, schema-v7.md §12 and VALIDATION.md "Strict sale and ruleset 2".
- **R16:** W2-52 in waves.md, §7 and the §6 expected-moves table.
- **R17:** the ids already exist: W3-29 (CF-1-01, CF-1-02; beside W3-14), W3-30 (CF-1-03) and W3-31 (CF-1-07), each before W3-27's matrix review. W3-30 moves from qa-captures to vfx-lighting. The §10 ledger rows name them.
- **R18:** the W2-47 row in waves.md.
- **R19:** the §9.1 peak row already names its gate (c533c04); the "floor" wording is fixed in §1.4 and acceptance.md G1.10, and the window note in §9.4 is kept.
- **R20:** design-qa-captures.md S15 is unchanged.
- **R21:** the settings screen (horde_game) and a pure core outcome in `Campaign`, in their own commit.
- **R22:** `PERF_BUDGET.json` and `perf_budget.rs`, in their own commit, which also answers the §2.5 row. W2-51 is then done in the W1 review.
- **R23:** §11 and docs/ACCEPTANCE.md.

R24 and R25 are user decisions made in chat (2026-11-07). Where they land:
- **R24:** the brief's PowerShell rule (`2026-11-04-production-pass-prompt.md`, an "amended by R24" note beside the original sentence), acceptance.md ISO-2, the qa-captures row "Native exes in 5.1" in §5.2, Ruling S14 in design-qa-captures.md and design-perf-architecture.md, the Native.ps1 contract and decisions table in design-qa-captures.md, design-perf-architecture.md §9, and the comment on the late-exit case in `scripts/tests/run_script_tests.ps1`, which now says the case guards against a return to `Start-Process -PassThru`. It resolves the Invoke-Native item, the one confirmed finding of review rounds 5-10 that was not fixed in code.
- **R25:** waves.md "Global rules for every commit", beside the verify rule. Applied to wave 1 in §7, "Wave-1 close".

R26 and R27 are user decisions made in chat (2026-11-09). Where they land:
- **R26:** the §2.5 row "Gate-3 episode in the shipped default configuration", the W2-50 row and its Result in waves.md, the "Gate 3 (W2-50)" item of §7 "Wave-1 close", the W2-50 block in §9.4 and VALIDATION.md "Gate 3 in the shipped default configuration".
- **R27:** the §2.5 row "Gate-3 stall tail" and the same W2-50 places as R26. The harness change is `scripts/measure_performance.ps1` (the hold and the 506/507 check behind a function that `scripts/tests/run_script_tests.ps1` shims) with its script cases, in its own commit (`feat(horde): measure_performance holds the display on and rejects display-off runs [W2-50]`), which also updates the run validity text in design-perf-architecture.md "Per run", acceptance.md G1.6, VALIDATION.md and AGENTS.md. W2-50's Done is met once that commit lands; the W2-47 re-gate runs on it.

R28 and R29 are orchestrator rulings (2026-11-09). Where they land:
- **R28:** the §2.5 row "W2-08 moved the default pacing output", the W2-01 pin and the W2-08 row in waves.md, design-loot-economy.md "Digest moves", ruling W-1 and its W2-08 writer note, and §9.2. W2-08 is complete.
- **R29:** the §2.5 row "Probe spans changed after W1-61", the W2-47 row in waves.md and the R18 item of §7 "Wave-1 close".

R30 and R31 are user decisions made in chat (2026-11-09). Where they land:
- **R30:** `scripts/lib/MachineLoad.ps1` (new: the process and load queries, the quiet window and the per-run load check, each behind a function that `scripts/tests/run_script_tests.ps1` shims), the pre-flight and the per-run sampler in `scripts/measure_performance.ps1`, and the sampler hook `Invoke-Native -Sample` in `scripts/lib/Native.ps1`, with their script cases, in their own commit (`feat(horde): perf passes wait for a quiet machine and reject loaded runs [W2-47]`). That commit also updates design-perf-architecture.md §9 "Per run", acceptance.md G1.6, VALIDATION.md and AGENTS.md. The W2-47 row in waves.md and the R18 item of §7 "Wave-1 close".
- **R31:** `scripts/ab_interface_total.ps1` (new: no A/B tooling existed under `scripts/`, the W1-61 head-ab passes were run by hand), with its decision in `scripts/lib/AbInterfaceTotal.ps1` and its fixture cases, in their own commit (`feat(horde): interleaved A/B interface_total gate against the wave-1 executable [W2-47]`). The W2-47 row in waves.md (its interface_total tolerance item), acceptance.md G1.10, design-perf-architecture.md §9 "Budget compare", §13 and its Integration rulings, and the R18 item of §7 "Wave-1 close".

R32 and R33 are orchestrator rulings (2026-11-09). Where they land:
- **R32:** the two §2.5 rows W2-22 opened ("Who authors the level grant and the rank milestones" and "`settle_parallel` settles only on a pass that credited an expedition victory"), the W2-54 row in waves.md (no longer pending; W2-49 already lists it in its Deps) and the W2-22 row, decision 14 in §2.1, PT7 and PT8 in §4.6, §6 "Expected moves after the freeze", §7 and design-progression-tree.md "W2-22, shipped values stay neutral".
- **R33:** the W2-09, W2-49 and W4-04..n rows in waves.md, the W2-09 row of §6 "Expected moves after the freeze" and §9.6 "W2-09 20-seed release acceptance".

R34 and R35 are orchestrator rulings (2026-11-10), the wave-2 close rulings. Where they land:
- **R34:** the §2.5 row "A valid over in a pass masked by invalid runs", the §7 "Wave-1 close" R18 item (its W2-47 "Done" text), the W2-47 row in waves.md (its review correction), acceptance.md G1.9 and VALIDATION.md "W2-47 re-gate on the wave-2 stress fixture". `gate_clean` false in `PERF_BUDGET.json acceptance.w2_47.passes.dev-novsync` stays as recorded. The harness change is `scripts/measure_performance.ps1` (`Measure-PerfBudget` returns `ValidOvers`; the VALID OVER line, summary.json `valid_overs` and a summary.md line) with its fail-first case in `scripts/tests/run_script_tests.ps1` (the fake game's `HORDE_FAKE_CPU_P99` and `HORDE_FAKE_UNFOCUSED_SCENE`), in its own commit (`feat(horde): measure_performance names every valid gated over [W2 review]`), which also updates design-perf-architecture.md §9 "Budget compare" (exit codes), acceptance.md G1.6 and AGENTS.md. The wave-2 close perf acceptance (R25) is measured fresh at HEAD.
- **R35:** the W2-47 row in waves.md (its "R29 landed" item), the "Re-measured on the 7abb0ab spans" block of §9.4 and the §2.5 row "Probe spans changed after W1-61".

### 2.5 Still open for the user

| Item | Default if unanswered |
|---|---|
| P8: create the Bevy-free path crate `crates/horde_audio` in wave 6 (A6-00); Cargo.lock gains one path entry, no external dependency | Create it |
| Lane-pan reading: along the lane, listener-relative, at most 3.1 dB level difference (audio, non-blocking) | Proceed with this reading |
| Any capture finding deferred at goal 10 | Needs "approved by user YYYY-MM-DD" in chat; the orchestrator cannot self-approve |
| Fast-forwarding local `main` at the end | Ask first |
| Probe spans changed after W1-61 (review commit `7abb0ab`, 2026-11-07; VALIDATION.md W1-61 "Span boundaries changed after this acceptance"): re-measure `PERF_BUDGET.json baseline.v3` on the new egui_pass and present_wait spans before the next interface_total re-gate (W2-47, then W5-11, W7-09)? Orchestrator or user decides. R18 (2026-11-07): W2-47 owns this item, and any other egui_probe/present_wait span-ordering item not fixed in wave 1. **Answered by R29 (2026-11-09):** `baseline.v3` is re-measured on the 7abb0ab egui_pass and present_wait spans in W2-47, before any interface_total re-gate. The old values stay under a previous key, and under R22 a gate that the re-measured baseline would loosen keeps its old value. The W2-47 row in waves.md notes it. **W2-47's reading confirmed by R35 (2026-11-10, orchestrator ruling):** `baseline.v3` was re-measured on the 2339a3d renderer with only the 7abb0ab span edits, not at HEAD, and the gated progressed-peak pair kept its W1-37 values (4.6447 / 4.5895) under R22 | None: the re-gates compare unlike spans until this is decided |
| Gate-3 episode in the shipped default configuration (W1-61 review, 2026-11-07; `target/perf/w1-61/head-ab-playtest-vsync/stress-on-r2`: playtest, pipelined on, VSync on, stress normal; 6 gated frames over 50 ms, 407 over 33 ms, p99 47.1 ms, exit 1; not a driver stall under the ruling, not attributed; plan 9.4): W2-50 (perf-architecture, before the W2-47 re-gate) reproduces that configuration and attributes or fixes the episode. If at least 12 repeats per size under the W1-68 probe do not reproduce it, does it close as unreproduced or stay open? Orchestrator or user decides. **W2-50 (2026-11-08, 9.4; VALIDATION.md "Gate 3 in the shipped default configuration"):** 28 repeats per size did not reproduce it; 14 per size ran with the display on throughout (12 of them with the display held on, added in the W2-50 review), so the 12-per-size floor holds for display-on runs too. The machine was unattended and locked, so there was no input, and the display was off for the other 14 per size. The time is held by the swap-chain acquire (about three vblanks per frame, in-process work 6-8 ms). The 4-6 ms outside App::update is winit input dispatch, present only in frames carrying window input events. The run is the only one on the branch with sustained input; it ran 5 minutes after the user woke the laptop. The cause of the slowed presentation is not named. To decide: close it as environmental (user activity during an evidence run), or keep it open until a supervised reproduction with pointer input over the unlocked window. **Answered by R26 (2026-11-09, user decision):** the episode closes as environmental: user activity during an evidence run (the laptop had been woken 5 minutes before, and the run carried window input). It was not reproduced in 28 repeats per size, 14 of them with the display on throughout. Gate 3 in the shipped default configuration can be claimed once the W2-47 re-gate passes | It stays open, and gate 3 is not claimed clean in the shipped default configuration; the wave-1 exit claims the PERF gates only at 199d823, with this episode excepted |
| Gate-3 stall tail (W1 review, 2026-11-07; Goal-1 acceptance re-run at 867f786, `target/perf/w1-review/acc-dev-vsync-stress/stress-normal-r1.json` sample 932: dev, VSync on, stress normal, 54.74 ms, acquire share 0.794, directly after three ~1,000 ms stalls; plan 9.4): the frame right after a stall counts acquire wait only where the hand-off overlaps prepare_windows (43.5 of 50.6 ms present wait), so it falls under the 0.80 share and stays gated (exit 1). W2-50 owns it with the stress-on-r2 episode. Should such a tail count its whole present wait toward the stall share, or stay gated? A rule change is a gate change: user ruling. **W2-50 (2026-11-08, 9.4):** driver stalls and their tails come from the display being off. Windows' Modern Standby on the idle timeout turns it off during unattended passes: 518 of 521 stalls were display-off or within 5 s of a display power transition, and sample 932 fell 8 minutes into a display-off interval. Holding the display on with a power request removed them (24 runs, 0 stalls). W2-50 found three more gated transition frames. Counting the whole present wait would catch one of them (share 0.660 to 0.80); it would not catch named coverage 0.932 or an 819 ms acquire with the main thread outside the update. Recommendation: keep the rule, and instead have `measure_performance.ps1` hold the display on and treat a run that overlaps a display-off interval (Kernel-Power 506/507) as invalid evidence. That is a harness ruling on evidence validity, wanted before W2-47 re-gates. **Answered by R27 (2026-11-09, user decision):** the 80% driver-stall rule stays unchanged. Instead, `measure_performance.ps1` holds the display on for the whole run, and a run that overlaps a display-off interval (Kernel-Power 506 or 507 between its start and end) is invalid evidence, rerun like the script's other invalid runs. The sample-932 tail is closed as display-off | It stays gated; the `driver_stall` rule is unchanged, and gate 3 is not claimed clean in dev VSync-on stress |
| W2-08 moved the default `pacing 60 --runs` output (W2-08 review, 2026-11-08; §9.6, "W2-08 moved the default pacing output"): `recommended_node` reads owned loot-only drops, so expert DEU's first victory goes from run 3 to 5 and novice DEU's from 5 to 6, against design-loot-economy's "smoke and pacing must be byte-identical across the whole loot series". Accept the move and re-baseline, so later wave-2 commits diff default pacing against `target/production-after/w2-08/pacing.txt` (sha256 547f7b4e...cb2e4d3c) instead of `w1-69/pacing.txt`, with the W2-01 pin wording in waves.md and design-loot-economy "Digest moves" amended to match? Or keep the w1-69 baseline and change W2-08? Orchestrator or user decides. W2-26 (the in-country front ramp, on purpose) moved the output again (§9.6, "W2-26 moved the default pacing output"), and so did W2-32 (the growth knee, floors past rank 4 and the rank boss armor, on purpose; §9.6, "W2-32 moved the default pacing output") and W2-33 (elites from rank 3, on purpose; §9.6, "W2-33 moved the default pacing output"; smoke moved too), and W2-36 (boss phases past rank 4, on purpose; §9.6, "W2-36 moved the default pacing output"; smoke unchanged), and W2-39 (country difficulty bands and the start clamp, on purpose; §9.6, "W2-39 moved the default pacing output"; smoke moved too), and W2-41 (the eight decision-19 nodes, on purpose: `tier_cost` counts them until W4-p's tier definition lands, and the recommender buys them from 5 victories; §9.6, "W2-41 moved the default pacing output"; smoke unchanged). If the move is accepted, the baseline is now the latest output, `target/production-after/w2-41/pacing.txt` (sha256 e19f0669...d6bbe65c), not `w2-08/pacing.txt`. **Answered by R28 (2026-11-09):** the move is accepted and pacing is re-baselined on `target/production-after/w2-41/pacing.txt` (sha256 e19f06694be215fe05068c542cb3f56e8741d2ed959aa5db03ba9d68d6bbe65c, checked 2026-11-09). The W2-01 pin wording in waves.md and design-loot-economy "Digest moves" are amended to match. A wave-2 commit may move pacing on purpose if it records the before/after rows in §9.6. W2-08 is complete | None: the W2-01 pin wording and "Digest moves" are not amended until this is ruled; a later writer diffing against `w1-69/pacing.txt` attributes the W2-08 rows of §9.6 to W2-08, the W2-26 rows to W2-26, the W2-32 rows to W2-32, the W2-33 rows to W2-33, the W2-36 rows to W2-36, the W2-39 rows to W2-39 and the W2-41 rows to W2-41 |
| Hard boss phases at ranks 0-4 (W2-26 review, 2026-11-08). schema-v7 §3.10 and the design-enemy-balance §6 table give Hard one boss phase at r ≤ 4 and two on a Capstone (`boss_phases = min(2, (r>4) as u8 + boss_phase_bonus + rank.extra_transitions)`). W2-26's pin `neutral_snapshot_serializes_no_new_keys_for_ranks_0_to_4_on_every_preset_at_depth_0` (waves.md) forbids a `boss_phases` key at ranks 0-4 on every preset, Hard included. W2-26 followed the pin, so `ProgressionSnapshot::fold_extras` adds `boss_phase_bonus` only past rank 4. Nothing reads `boss_phases` yet. W2-36 owns the answer (waves.md row, design-enemy-balance "Writer notes (wave 2)"). Two options: keep the pin reading and amend schema §3.10 and the §6 table (Hard r ≤ 4: Sector or Regional 0, Capstone 1), or follow the schema, drop the rank gate in `fold_extras` and narrow the pin to Easy and Normal. Orchestrator or user decides | The pin reading: the bonus applies only past rank 4, so Hard at r ≤ 4 has Normal's phase count (0, or 1 on a Capstone once W2-36 lands). W2-36 adds only the `(r>4)` and `extra_transitions` terms and never adds the bonus a second time. **W2-36 (2026-11-08) implemented this default:** `hard_ranks_zero_to_four_have_normals_phase_count` pins Hard at ranks 0-4 to Normal's count (0 on a Sector or Regional stage, 1 on a Capstone), and schema-v7 §3.10 and the design-enemy-balance §6 table were amended to match. The other option (drop the rank gate in `fold_extras`) would change that test, `preset_extras_fold_into_the_snapshot` and the rank 0-4 pins |
| Re-derive the W1-51 egui gates from the 4d599a5 re-measure (`PERF_BUDGET.json evidence.egui_measured`, the a93e340 figures are in `.previous`; W1 review, 2026-11-07)? W1-51's rule is min(W1-32 ceiling, ceil(measured max x 1.25)); the live gates still derive from the a93e340 maxima taken before W1-64 to W1-69. Re-derived: `BATTLEFIELD_PRIMITIVES_MAX` 48 -> 24 (tighten; 19 in every scene) and the stress vertex gate 61098 -> 61878 (loosen; stress-5000-corpses 49,502); progressed-peak stays at its 40,000 ceiling. Applied by W2-51 before the W2-47 re-gate. Orchestrator or user decides. **Answered by R22 (2026-11-07):** gates only tighten. Primitives 48 -> 24, the stress vertex gate stays 61098, progressed-peak stays at 40000, and `egui_measured` moves to the post-W1-69 maxima. Applied in the W1 review, so W2-51 has nothing left to change | Tighten primitives only (48 -> 24, `egui_measured` moves to the 4d599a5 maxima for primitives) and keep the stress vertex gate at 61098 |
| Who authors the level grant and the rank milestones (W2-22 review, 2026-11-09)? Decision 14 (§2.1), PT7 and PT8 (§4.6) name W2-22 as their last owner, design-progression-tree §6 says the grant fraction "ships at 0.25" and §7 lists the four milestones (rank 5/10/15/20: 500 and 1000 coins with an Uncommon item, 1500 and 2000 with a Rare item), and ruling E2 expects smoke and pacing to move. W2-22's row Done is "GOLDEN unchanged", so W2-22 shipped only the settlement writer (`Game::settle_progression_rewards`) on the still-neutral `progression.json`: the shipped game pays no grant and no milestone, and the design values run only through the test-support seam `progression::override_rewards`. No waves.md row authored the values, so decision 14, PT7 and PT8 had no owner. Measured in the W2-22 review (the values authored on the review tree, then reverted): `PROVISIONAL` moves for the three Personal state digests (0x1 0x2906db6eab9e16c6 -> 0xbed5e2714b6590fe, 0x1a571167 0x4b76d4cfc2085d5e -> 0x3368f18097f7fb5e, 0xdeadbeef2026 0xc0f5736bba394b29 -> 0x7be5c84d229d8d71), because a non-zero grant makes the W2-52 shim inert and their mid-run saves owe `level_grant_paid` again; `GAMEPLAY_TRACE`, V1-V6, `V6_DROP_TRACE` and the `save_migration` suite do not move. The `GOLDEN` table itself is edited only by W2-49. Other pins move with it: the save-determinism guard `default_and_golden_games_serialize_no_v7_key` (GOLDEN 0x1 Personal after a withdrawal now writes `level_grant_coins` and `progression_rewards`), `MODE_OWNER_DIGEST` (`mode_owner_paths_are_byte_identical`), the coin pins of `boss_timer_is_not_victory_and_boss_transaction_is_atomic`, `parallel_rewards_and_unique_loot_settle_exactly_once` and `normal_short_arena_earned_route_clears_within_five_runs_without_equipping_loot` (270 -> 298), and the neutral-value pins in `progression.rs` and `progression_schema.rs`. Smoke and pacing were not measured. Author the values, and where? **Answered by R32 (2026-11-09, orchestrator ruling):** the default is accepted. W2-54 (progression-tree, before W2-49) authors `rewards.level_grant_xp_fraction` 0.25 and the four design §7 milestones in `progression.json`, sets `PROVISIONAL` with a `PROVISIONAL_CAUSES` row and updates the pins listed above, each with its reason. W2-49 depends on W2-54. The W2-54 row in waves.md is no longer pending | Author them in W2-54 (progression-tree, waves.md; before W2-49): `rewards.level_grant_xp_fraction` 0.25 and the four §7 milestones, `PROVISIONAL` with a `PROVISIONAL_CAUSES` row as `support::classify` assigns, the moved pins updated with a reason each (the schema guard with save-determinism's agreement), smoke and pacing before and after in §9.6 (R28), and the W2-52 shim kept (inert while a grant is authored; it still covers a neutral catalogue). Decision 14, PT7 and PT8 close only when W2-54 lands |
| `settle_parallel` settles only on a pass that credited an expedition victory (W2-22 review, 2026-11-09). Ruling E2 says settlement runs "once at the end of settle_parallel when idle at depth 0". W2-22 reads "idle" as "no main attempt is live" and also requires the pass to have credited a victory, so an idle pass that credits nothing pays nothing: it has no report or run record to carry the receipt. A debt the pass did not raise (a save made before W2-52 whose `level_grant_paid` is below the level) waits for the next main finish or the next credited expedition pass (SAVE_FORMAT.md "Progression ledgers"). Keep the narrowing, or settle on every idle pass and drop a receipt that has no home? **Answered by R32 (2026-11-09, orchestrator ruling):** the default is accepted. The narrowing stays: `settle_parallel` settles only on an idle pass that credited a victory, and a debt the pass did not raise waits for the next main finish or credited expedition pass | Keep the narrowing |
| A valid over in a pass masked by invalid runs (W2-47 review, 2026-11-10; VALIDATION.md "W2-47 re-gate on the wave-2 stress fixture", `PERF_BUDGET.json acceptance.w2_47.passes.dev-novsync`). The first dev `-Scene all -VsyncOff -Repeat 3` invocation (`target/perf/w2-47/gate-dev-novsync`, 43184d1) had three groups invalid on 90-100% CPU load samples and one valid gated group over: progressed-peak small cpu_frame p99 median 12.7565 ms against 10.0 (repeats 24.55 / 12.76 / 10.51 ms at load maxima 72 / 52 / 44%, valid under R30). measure_performance ranks invalid evidence above an over budget, so the exit was 2, not 1. The writer re-measured the whole pass (`gate-dev-novsync-b`, exit 0, progressed-peak small 7.21 ms) because the machine was loaded below R30's threshold (mean 44-49%). R30 and the W1-37 method re-measure only groups with an invalid run. May a valid gated over, measured on a machine loaded below the R30 threshold, be re-measured? If yes, under what rule (R30 then needs a load criterion for valid runs)? If no, the over stands. Orchestrator or user decides. **Answered by R34 (2026-11-10, orchestrator ruling):** the default is kept. The over stands: the dev -VsyncOff progressed-peak gate (G1.9) is not claimed clean at W2-47, and a valid gated over may not be re-measured away, whatever the machine load was below the R30 threshold. The wave-2 close's own perf acceptance at HEAD is a new measurement, not a re-measure. `measure_performance.ps1` keeps its exit precedence (invalid evidence exits 2), and now prints "VALID OVER: <scene> <size> <metric> <value> > <gate>" for every valid gated over and lists them in a top-level summary.json `valid_overs` | The over stands. The dev -VsyncOff progressed-peak gate (G1.9) is not claimed clean at W2-47 (`gate_clean` false), and W5-11 re-gates it. W2-47's Done (stress `-VsyncOff`) is not affected |
| Two of the post-L4 loot numbers (W2 review, 2026-11-10; design-loot-economy "Numbers to measure", §9.7) are still unmeasured: the median stash size after 60 and 150 runs with the auto-sell profile, and the auto-sold coins per run in runs 1-6. The other three were measured in the W2 review (§9.7): drops per run, elite drops per run at ranks 3, 10 and 23, and the tier shares at drop tier 4. No waves.md row names these two. The W4-l row ("Auto-sell pacing gate and stash-growth measurement", `stash_growth_reported_for_150_runs`) is the nearest. W2-42's `auto-sell` profile and `PacingRun::auto_sold_coins` already make the second number measurable. Does W4-l own both numbers, or does another wave-4 row? Orchestrator or user decides | W4-l owns both. It records the stash medians at runs 60 and 150 and the auto-sold coins per run in runs 1-6 (auto-sell profile, 20 seeds, median) in §9.7, beside the rank 23 figure there: 75.6 extra elite drops per run. That figure is the design Risks item 3 input to its tuning |
| An unreadable System log makes a perf run invalid (W2 review, 2026-11-10). R27 makes a run that overlaps a Kernel-Power 506 or 507 event invalid evidence, but says nothing about a run whose System log cannot be read. The R27 commit (W2-50, edd2980) chose invalid: `scripts/lib/DisplayPower.ps1` reports the problem "display power unknown (Kernel-Power 506/507 unreadable: ...)", and `measure_performance.ps1` reruns the run once like its other invalid runs, exiting 2 if the rerun is invalid too (`run_script_tests.ps1` "Test-DisplayPower makes a run with a 506/507 event or an unreadable log invalid and keeps one without valid (R27)"; design-perf-architecture "Display"; VALIDATION.md "R27 landed"). That matches R30's rule for an unreadable CPU-load sample, but no ruling records it. Keep it, or let a run with an unreadable log stand as valid with a warning? Orchestrator or user decides | Keep it: a run whose System log cannot be read is invalid evidence, as R30 rules for an unreadable load sample, and is rerun like the other invalid runs |
| `defender_power` misses its cost target (W2 review, 2026-11-10; design-campaign-defenders §8 "Numbers to measure", as-built row). The W2-21 commit (bc85bf7) measured, in release over 2,000 rounds, 6.9 µs per call and 90 µs for 13 defenders against the "< 50 µs total, recorded" target, and 105-110 µs for `available_defenders`; the time is `Campaign::profile_for_loadout`, which the deploy shares. `outbreak_plan` fills `recommended_defender` through `available_defenders`, so every `outbreak_plan` call pays the cost again (a frame reading the picker and an outbreak plan pays it twice; not measured on its own). The target is "recorded", and no waves.md row owns a fix. Accept the recorded cost, or have a later row cache the power per frame or per state change? Orchestrator or user decides | Accept it as recorded: no cache until a frame report shows `available_defenders` or `outbreak_plan` in a gated span |
| Auto-sold line ties after `absorb` (W2 review, 2026-11-10). design-loot-economy line 66 says the receipt's lines keep the 12 highest-coin sales with "ties go to ascending id". A single settlement (`sim/auto_sell.rs`) does sort ties by item id. `AutoSoldReceipt::absorb` (`campaign/auto_sell.rs:82-93`), which merges an expedition's final sweep into the run's receipt, re-sorts the merged lines by coins with a stable sort, so on a tie the earlier receipt's line comes first, whatever the ids. `AutoSoldLine` has no item id in the frozen v7 schema ("no item_id by design"), so `absorb` cannot break ties by id. No waves.md row owns it. Amend the design to the as-built order, or rule otherwise? Orchestrator or user decides | Amend design-loot-economy to the as-built order: within one settlement ties go to ascending item id; on `absorb` the earlier receipt's lines come first on a tie. No schema, save or digest change |

---

## 3. Loop map

### 3.1 Today (wave 0, from the ux-flow audit)

| Screen | Primary action | Escape |
|---|---|---|
| Menu | "Resume journey" or "New game" (black primary; no amber style exists) | Nothing |
| Menu, new-journey modal | "Start new journey" / "Keep current journey" (neither primary-styled) | Closes the modal |
| SurvivorSelection | "Continue as <Rook/June>" | Nothing |
| Camp (default tab **World**) | World footer: "Choose survivor first" / "Start journey" / "Manage country" / "Return to active defense". "Start journey" on a neighbour dispatches `EnterCountry`, which deploys at once. | Toggles the world clock only when `operations_available` and no attempt; otherwise nothing |
| Camp/Briefing | "Deploy personally" / "Deploy survivor"; picker offers only You and Mara | As Camp |
| Camp/Equipment, Character, Mara | "Go to briefing" | As Camp |
| Camp/Operations | No footer primary unless an attempt is live; in-page "Defend now" deploys at once; the observer window closes only through its X | As Camp (the clock toggles instead of closing the observer) |
| Battle (Personal) | No amber primary; bar: Begin defense / Pause / Resume, Reload, Strike, Toggle firing, Emergency aid, Brace, Build/Close construction, "Retreat & train" (one click withdraws), Management. Wraps to 2-3 rows at 1050x700@1.35. | The rebindable Pause binding: toggles pause **and** closes construction; blocked while a popup owns the keyboard |
| Battle (NPC) | Observe 1x/2x/4x, Survivor popup, "Recall & train" (one click) | As Personal |
| Results (shared with the tree) | Continue primary, a "More" menu (2 hidden actions), "Leave journey", then the full tree: 5 reachable actions | Nothing |
| History | "Back" | Hard-coded Escape returns to `history_return` |
| Settings | `settings_return_label` | Hard-coded Escape saves and returns; cancels a rebind |
| Operation details window | n/a | Closes it |

### 3.2 Target (ux-flow loop map with the integration rulings applied)

| Screen | Amber primary (exactly one) | Escape target |
|---|---|---|
| Menu | Resume journey / New game | Quit modal ("Save and quit" / "Stay") |
| SurvivorSelection | Continue as <name> | Menu |
| Camp/World before the first start | Start journey (ChooseStart; the field opens paused) | Menu |
| Camp/Home (default tab) | `home_primary`: Return to active defense / Choose next country / Go to briefing / Choose a country | Results when a settled report exists, the journey is active and no attempt is live; otherwise the ReturnToMenu modal |
| Camp/Briefing | Deploy personally / Deploy <defender name>; dispatches `BeginDefense { stage }` for `next_defender()` (D16) | Camp/Home |
| Camp/Equipment, Character, World, Mara (drill-down), Operations (drill-down) | Go to briefing / Choose next country (World: Plan defense / Manage country; outbreaks: Plan response, both via `SelectCountry`, never deploying) | Camp/Home; the observer window and armory confirmations close first |
| Battle | Begin defense / Resume defense (amber only while paused) | Topmost overlay first, then cancel a retreat hold, then close the construction overlay; otherwise PassToPause only if `bindings[Pause] == "Escape"`, else ignored |
| Results | `results_primary`: Upgrade (after a defeat or withdrawal when the recommended node is Available), else `continue_label` (Try again / Next defense / Choose next country / Return to active defense). Exactly three action-bar buttons. | ReturnToMenu modal ("Save and return to main menu" / "Stay") |
| Upgrades (new Screen) | `upgrades_continue_label`: Back to battle / Choose next country / Return to active defense; with no report: Back to camp | Results if a report exists, else Camp/Home |
| History | none | Return screen |
| Settings | `settings_return_label` | Cancel a rebind; else the draft warning when the difficulty draft is dirty; else save and return |
| Recruit modal | "Recruit as <Archetype> · <core price>" (price from `Game::recruit_offer`, R10), then "Back to camp" | Close |

The headless loop test (`full_loop_new_game_to_recruit_defends`, W3-28) walks new game → Rook → country → defeat → Results → Upgrade → buy → Back to battle → victory → Next → liberation → Recruit → Back to camp → the recruit defends, asserting Screen, Tab and exactly one primary at each step, at both sizes.

---

## 4. Findings with evidence

157 findings from the 10 wave-0 audits, condensed. Severity: B blocker, M major, m minor. Evidence paths are under `crates/horde_core/src/` or `crates/horde_game/src/` unless they start with `scripts/`, `docs/`, `assets/` or `target/`. "Fix" names the commit in `waves.md`.

### 4.1 Goal 1: architecture and performance

| Id | Sev | Finding | Evidence | Fix |
|---|---|---|---|---|
| PA-1 | B | The goal-1 gates (CPU p99, frames > 50 ms, interface -30%, pipelined decision) cannot be evaluated: the frame report has no render-world CPU, egui shape or vertex counts and only VSync-capped frame times. | diagnostics/frame_report.rs:33-76,114,164,212; bevy_egui output.rs:51 | W1-32, W1-33 |
| PA-2 | M | BattleScreen is built 2 times per active pass (3 on a Pause press); HUD, BVM and SceneAnimation once each, with no guard. | ui/battle.rs:58-72,93-153,214-263; ui.rs:119-178 | W1-42 |
| PA-3 | M | Each enemy is 4-11 egui shapes with alternating white/sprite textures. | art/battlefield/enemies.rs:79-216; art.rs:582-624 | W1-48 |
| PA-4 | M | Corpses grow one per kill without a cap and are re-sampled and re-meshed every pass. | sim/combat.rs:327; animation.rs:232-245; battle_view.rs:322-348 | W1-49 |
| PA-5 | m | Up to 128 particles and every fx event are separate feathered paths. | art/battlefield/fx.rs:19-145 | W1-48 |
| PA-6 | M | No budget file: caps are scattered constants. | animation.rs:12-14; ui/damage.rs:11; ui/loot.rs:9 | W1-32 |
| PA-7 | B | `measure_performance.ps1` cannot run under PS 5.1 (`& exe *>` with Stop). | scripts/measure_performance.ps1:3-28 | W1-02, W1-36 |
| PA-8 | M | No vsync or low-latency control, no VSync-off measurement, no recorded pipelined decision. | main.rs:99-133 | W1-34, W1-61 |
| PA-9 | M | The whole Game is deep-cloned on the main thread on every autosave and save intent. | runner/apply.rs:290-295; persistence.rs:21-31 | W1-50 |
| PA-10 | M | battle_view.rs (1,282 lines) mixes plugin, view model, readouts, placement and tests. | battle_view.rs:1-575 | W1-38 |
| PA-11 | M | ui/battle.rs (1,200 lines) mixes input, camera, layout, panels and construction. | ui/battle.rs:23-201 | W1-39 |
| PA-12 | m | art.rs (1,191 lines) mixes palette, projection, dressing, figures and scenes. | art.rs:13-334 | W1-40 |
| PA-13 | m | upgrade_graph.rs is the largest file (1,388 lines). | ui/progression/upgrade_graph.rs:22-196 | W1-41 |
| PA-15 | m | Perf numbers come only from review builds (`default = ["review"]`) and reports carry no profile. | horde_game/Cargo.toml:26 | W1-33, W1-61 |
| PA-16 | m | ARCHITECTURE.md is prose with stale facts and no perf/diagnostics owner. | docs/ARCHITECTURE.md:5-36,188 | W1-62 |
| SD17 | m | Every save also fully decodes and validates the previous generation to decide the .bak copy. | persistence.rs:108-165 | W1-50 (conditional) |

### 4.2 Battle camera and construction (user requirement, goal 2)

| Id | Sev | Finding | Evidence | Fix |
|---|---|---|---|---|
| CAM-1 / UX-15 / PA-14 / QA12 / SD18 | B | The arena is a fixed-scale clearing: 3x12 uses 40% x 27% of the 1392x692 area at 1440x900 and 291.9x86 at 1050x700@1.35. | ui/battle.rs:475-489; ui/operations.rs:277 | W1-43, W1-44 |
| CAM-2 | B | Construction moves the right inset from 24 to 276 pt, shrinking the arena by 252 pt. | ui/battle.rs:229-232,490-498 | W1-44 |
| CAM-3 | M | The 84-pt first-defense hint callout and READY/PAUSED banner draw inside the field area and would cover a zoomed field. | ui/battle.rs:299-306,638-659 | W1-44 (hint band) |
| CAM-4 | M | Forest props scale with lane height and the effects clip is the canvas, so zoomed fields need a dressing cap and a defined effects clip. | art.rs:120-158,226-230 | W1-44 |
| CAM-5 | m | EXPANSION.md:99 and PIXEL_PRESENTATION.md:77 state the opposite contract; two tests pin the old fixed scale. | docs/EXPANSION.md:99; ui/battle.rs:1088-1104 | W1-44 |

### 4.3 Goal 2: post-run flow, camp and navigation

| Id | Sev | Finding | Evidence | Fix |
|---|---|---|---|---|
| UX-01 | B | Results, the tree and the upgrade route are one screen. | ui/results.rs:142-171; ui/screen.rs:7-17 | W3-03 |
| UX-02 | M | No amber primary style; Results has 5 reachable actions. | ui.rs:40-49; ui/results.rs:182-228 | W3-01, W3-03 |
| UX-03 | B | Default tab is World; no Home; clock controls on every camp footer. | ui/screen.rs:66-78; ui/camp.rs:41-49 | W3-02 |
| UX-04 | M | Recruits are reachable only in Operations and can only run expeditions; no picker. | ui/context.rs:230-240; ui/operations.rs:185-226 | W2-12, W3-16 |
| UX-05 | B | Escape has 6 meanings depending on which code reads it. | ui/camp.rs:289-307; ui/battle.rs:58-72; ui/preferences.rs:13-20 | W3-04 |
| UX-06 / CD14 | M | 17 player-facing lines print Rust Debug names. | ui/armory.rs:141..717; ui/character.rs:123-141; ui/operations.rs:189-253 | W1-10, W2-44, W3-26 |
| UX-07 | M | 3 names for a manual save plus variants. | ui/camp.rs:43; ui/results.rs:163-208; ui/battle.rs:452-455 | W3-05 |
| UX-08 | M | Save failures are red inline text; no Retry / Open folder. | ui/notices.rs:144-150; profile.rs:84-98 | W3-06 |
| UX-09 | m | Leaving Settings drops a changed difficulty draft silently. | ui/preferences.rs:13-29,275-289 | W3-07 |
| UX-10 | m | The scale slider applies mid-drag. | ui/preferences.rs:73 | W3-08 |
| UX-11 | M | The boss banner lasts 1.0 s of sim time (0.25 s at 4x). | art/battlefield/fx.rs:178-227 | W3-14 |
| UX-12 | M | No low-HP warning. | ui/battle.rs:518-523; ui/hud/view.rs:57-58 | W3-15 |
| UX-13 | M | One click on "Retreat & train" withdraws. | ui/battle.rs:429-445 | W3-13 |
| UX-14 | M | The bar needs 1,160 px in a 754 px row (2 button rows plus a status row). | ui/battle.rs:350-469 | W3-11, W3-12 |
| UX-16 | M | No unlock callout or seen-state. | ui/camp.rs:191-277 | W3-24 |
| UX-17 | m | Finished expeditions are announced nowhere except a collapsed list and the log. | runner/command.rs:41-56; ui/operations.rs:94-108 | W2-15, W3-25 |
| UX-18 | M | Liberation Results has no recruit offer. | ui/results.rs:53-95; world.rs:241-246 | W2-20, W3-19 |
| UX-19 | M | The harness renders single screens; no loop walk exists. | ui/tests/mod.rs:133-223 | W3-28 |
| UX-20 | m | Mara is called "Survivor" in actions; stale construction text. | ui/briefing.rs:206,270; ui/battle.rs:406,998 | W3-16, W3-26 |

### 4.4 Goal 3: defenders and campaign

| Id | Sev | Finding | Evidence | Fix |
|---|---|---|---|---|
| CD1 / SD2 | B | No defender identity in core: a recruit cannot defend the main run and Mara's takeover is not saved. | content.rs:8-11; sim.rs:674-709; journey.rs:43 | W1-10, W1-13, W2-11 |
| CD2 / SD9 | B | `continue_journey` reserves only the player's or Mara's loadout; a defending recruit breaks validation in 3 places. | lifecycle.rs:121; armory.rs:251-280; persistence.rs:442-618 | W1-11, W1-17, W2-11 |
| CD3 | M | No saved next defender, no defender list with blockers, no typed rejection. | command.rs:29-146; strategy.rs | W1-12, W2-12 |
| CD4 | M | Npc wins always credit Mara. | campaign/world.rs:35-54; ledger.rs:129-135 | W2-11 |
| CD5 / SD13 | M | The recruit allowance saturates at 52 (36 on a 3-stage route) after 5 steps. | campaign/progression.rs:371-373,779-789 | W2-10 |
| CD6 | M | Choosing a country is fused with deploying (`EnterCountry`). | command.rs:214-232; ui/operations.rs:46-57 | W2-19, W3-18 |
| CD7 | M | Settlement emits nothing on success and ignores worker liberations. | sim/parallel.rs:217-334; runner/apply.rs:119-129 | W2-15 |
| CD8 | M | `settle_parallel` is private with no unit tests. | sim/parallel.rs:201-349 | W2-15 |
| CD9 | M | Recruitment takes no choice; no trait or history. | strategy.rs:280-358 | W2-20, W2-21 |
| CD10 | m | Large countries have no intermediate milestone. | world.rs:204-216 | W2-26, W4-01, W4-02 |
| CD11 | m | No comparable power value across the two combat models. | strategy.rs:103-139; loadout.rs:94-200 | W2-21 |
| CD12 | M | No strategy profile; expedition coins and camp time ignored; strategy RNG ignores the seed. | sim/pacing.rs:160-297 | W2-43, W4-c |
| CD13 | m | continue, retry and repeat redeploy with `report.mode`. | journey.rs:43; lifecycle.rs:254-443 | W2-12 |
| SD15 | m | Grants, milestones and notices need exactly-once guarantees across reloads. | sim/parallel.rs:145-151 | W2-15, W2-22 |

### 4.5 Goal 4: loot, items and auto-sell

| Id | Sev | Finding | Evidence | Fix |
|---|---|---|---|---|
| LE-1 | B | No single sale-eligibility rule; SellItem sells recruit-equipped items that the quote filters out. | campaign/ledger.rs:52-128; sim.rs:883-888 | W1-18, W1-19, W1-20 |
| LE-2 | B | Auto-sell does not exist. | lifecycle.rs:285-400 | W1-23, W2-14, W2-16 |
| LE-3 | M | Quality rolls are absolute, so higher-tier drops mostly downgrade. | quality.rs:111-157 | W2-07 |
| LE-4 | M | Only Rifle ranged items at tiers 2-4; 1 item per other slot per tier. | content.json items 5-19 | W2-08 |
| LE-5 | M | Drops are uniform, not weighted by tier or trained class. | sim/combat.rs:370-378 | W2-09 |
| LE-6 | M | No core "better than the battle defender's equipment" predicate. | campaign/loadout.rs:263-341 | W1-22 |
| LE-7 | M | Report and RunRecord have no auto-sold fields. | sim.rs:674-709; stats.rs:143-180 | W1-21, W2-06, W3-20 |
| LE-8 | m | Rewards clamp silently at the wallet limit while sales fail. | ledger.rs:28,73-108 | W2-14 |
| LE-9 | m | Pacing never sells loot, so the auto-sell baseline is undefined. | sim/pacing.rs:160-260 | W2-42, W4-l |
| SD8 | M | "This run's drops" is not computable: loot_events is capped at 128. | quality.rs:207; sim/combat.rs:381-399 | W1-21, W2-06, W2-16 |

### 4.6 Goal 5: progression, tree and skills

| Id | Sev | Finding | Evidence | Fix |
|---|---|---|---|---|
| PT1 | M | Steady hands dominates (40 coins, DamageAdd 8). | skills.json:44 | W4-03 |
| PT2 | M | Hardened survivor makes early contact damage nearly zero. | skills.json:360; sim/combat.rs:252 | W4-03 |
| PT3 | M | Trap mastery 1 and most trap ranks add no value. | skills.json:3203; sim.rs:966-979 | W2-24, W4-03 |
| PT4 | M | Quick throw is worth nothing. | skills.json:659 | W4-03 |
| PT5 | M | Body rank-1 nodes are dominated and the ladders front-load value per coin. | skills.json:681 | W4-p |
| PT6 | M | Tiers become affordable at runs 3/5/7 against a target of about 4/8/12. | target/production-before/pacing.txt | W4-p |
| PT7 | M | Level-ups give no reward. | campaign/progression.rs:684-704 | W1-24, W1-25, W2-22 (writer only), W2-54 (authors the grant, §2.5, R32; closed) |
| PT8 | M | No rank 5/10/15/20 milestones. | skills.json gates | W1-24, W2-22 (writer only), W2-54 (authors the milestones, §2.5, R32; closed) |
| PT9 | M | Mara training (25) and recruiting (100) are flat and hard-coded in 4+ places. | campaign/progression.rs:726; ui/character.rs:193; ui/operations.rs:206 | W2-18 |
| PT10 | m | IQ/HP/FP filtering is duplicated ad hoc. | ui/character.rs:204-243 | W2-23, W3-22 |
| PT11 | M | The pacing tool cannot measure goal-5 acceptance. | sim/pacing.rs:27-35,191-297 | W2-01 |
| PT12 | M | Telemetry cannot tell archetypes apart. | sim/telemetry.rs:16 | W1-27, W2-05 |
| PT13 | m | Several "acts now" nodes are worth zero at the start. | skills.json:381 | W4-p |
| PT14 | M | Decision-19 perks and counters need effect types that do not exist. | skill_catalog.rs:64-88; loadout.rs:206-217 | W1-26, W2-24, W2-41 |
| PT15 | m | Weapon training values differ by up to 5x; one tier-1 gun is worse than the starter. | model at rank 0 | W4-p |

### 4.7 Goal 6: enemies, scaling and difficulty

| Id | Sev | Finding | Evidence | Fix |
|---|---|---|---|---|
| EB1 | B | From rank 4 every stage of every country is the same encounter: no in-country ramp. | sim.rs:910; progression.json:48-55 | W2-26 |
| EB2 | B | Composition tops out at tier 4, reached at rank 4. | progression.rs:134,247 | W1-28, W2-31, W2-32 |
| EB3 | M | Gear damage grows about 3x faster per rank than enemy HP. | progression.json:7-8,58-59 | W2-32, W4-04..n |
| EB4 | B | None of the requested archetypes can be expressed. | enemy_catalog.rs:49-77,229-321 | W1-28, W2-27..31 |
| EB5 | M | No ranged enemy attack. | movement.rs:35-116 | W2-27 |
| EB6 | M | Reap has no death hook. | sim/combat.rs:308-368 | W2-29 |
| EB7 | B | No elite system. | sim.rs:93-130; enemy_catalog.rs:49-77 | W2-33 |
| EB8 | M | No affix catalogue. | enemy_catalog.rs:85 | W2-33 |
| EB9 | M | Boss armor is constant; no phases; no capstone. | director.rs:195-205; sim/combat.rs:102-107 | W2-32 (rank boss armor), W2-36 (phases, the Capstone transition), W4-01 (Capstone multipliers) |
| EB10 | M | Hard cannot raise elite rate, ramp or phases; Custom normalisation loses the preset. | balance.rs:23-68; campaign.rs:196-208 | W1-29, W2-26 |
| EB11 | M | Country difficulty spans only 0.94-1.08. | scripts/generate_world.py:99 | W2-38, W2-39 |
| EB12 | m | The victory-to-threat guard is tested only for some paths. | campaign/world.rs:47 | W2-40 |
| EB13 | M | No per-archetype telemetry or pacing_targets. | sim/telemetry.rs:16; sim/pacing.rs | W2-04, W2-05 |
| EB14 | M | New systems can silently move GAMEPLAY_TRACE or GOLDEN. | director.rs:44-60; sim/combat.rs:368-376 | W1-09, W2-48 |

### 4.8 Goal 7: graphics

| Id | Sev | Finding | Evidence | Fix |
|---|---|---|---|---|
| FX-1 | M | All classes share one flash polygon and one tracer. | art/battlefield/fx.rs:415-467 | W5-03 |
| FX-2 | M | No lighting grade. | art.rs:291-330 | W5-01 |
| FX-3 | M | No storm tag and no lightning code. | world.rs:48-130,176-180 | W5-01, W5-02 |
| FX-4 | M | No light pools; single-frame explosions. | art/battlefield/fx.rs:326-364 | W5-04, W5-05 |
| FX-5 | m | Hit sparks are fixed size; no decal budget. | art/battlefield/fx.rs:482-521 | W5-06 |
| FX-6 | M | The enemy_visuals v1 loader rejects any other schema. | visual_assets/enemy_visuals.rs:17-46,110-115 | W1-52, W2-34, W5-07 |
| FX-7 | m | Fly-ups have no glint and ignore reduced effects. | ui/loot.rs:153-230 | W5-08 |
| FX-8 | M | No sha256 provenance test for art. | assets/art/PROVENANCE.json | W1-56, W1-57 |
| FX-9 | m | The goal-7 review scenes do not exist. | ui/review/matrix.rs:53-357 | W5-09, W5-10 |

### 4.9 Goal 8: audio

| Id | Sev | Finding | Evidence | Fix |
|---|---|---|---|---|
| AUD-01 | B | The mix is about 25-30 dB below the brief's target; source clips measure -29.8 to -38.8 LUFS. (The user's -23 LUFS gate replaces that target.) | audio.rs:160; sfx.rs:84 | A6-03..A6-17 |
| AUD-02 | M | The headroom proof covers only the effects bus. | audio.rs:477 | A6-17 |
| AUD-03 | M | 12 legacy WAVs are unreferenced; no provenance test. | assets/audio/PROVENANCE.json | W1-57, W1-58 |
| AUD-04 | M | `generate_atmosphere_audio.py` fails today and would break further after the archive. | scripts/generate_atmosphere_audio.py:134,151 | W1-57 |
| AUD-05 | M | Only 3 gain paths (hidden x0.7). | sfx.rs:76-85 | A6-02, A6-06 |
| AUD-06 | M | One global variant rotation: immediate repeats happen. | sfx.rs:150-151,192 | A6-05, A6-10 |
| AUD-07 | M | No enemy or boss banks; SimEvent mapping ends in a wildcard. | runner/runtime.rs:374-420 | A6-12 |
| AUD-08 | M | NPC shots ignore the weapon class. | runner/runtime.rs:390-392 | W1-58, A6-13 |
| AUD-09 | M | No pan. | sfx.rs:193 | A6-07 |
| AUD-10 | M | No ducking beyond the ambience context gain; hard-cut stealing. | atmosphere_audio.rs:55-62; sfx.rs:160-181 | A6-09, A6-11 |
| AUD-11 | M | No music system, and no new music files are allowed. | (none exists) | A6-15 |
| AUD-12 | M | The loot reveal reuses the Ready cue and fires in battle; sales give no batch cue. | ui/context.rs:254-256 | A6-14 |
| AUD-13 | m | The documented standalone exporter route no longer compiles. | audio.rs:6 | A6-00, A6-01, A6-05 |
| AUD-14 | m | No loudness or true-peak meter. | (none exists) | A6-03 |
| AUD-15 | m | Reduced effects only scales gains and cooldowns; warnings need visual twins. | audio.rs:99,123 | W2-28, W2-37, A6-12 |

### 4.10 Saves, determinism and settings (cross-cutting)

| Id | Sev | Finding | Evidence | Fix |
|---|---|---|---|---|
| SD1 | B | `save_v6.json` does not exist. | tests/fixtures (v1..v5 only) | W1-01 |
| SD3 | B | Many saved types have no per-field defaults (RunRecord, CombatTelemetry, ...). | stats.rs:142-180 | W1-09, W1-30 |
| SD4 | M | The digest hashes the whole payload, so any always-written field moves GOLDEN. | persistence.rs:95-98 | W1-09, W1-30 |
| SD5 | M | GAMEPLAY_TRACE has no re-bless tooling. | tests/determinism.rs:47-101 | W1-07 |
| SD6 | M | Legacy continuation traces cannot be honestly re-blessed. | tests/save_migration.rs:150-561 | W1-01, W2-06 (R4), W2-48 |
| SD7 | M | Elites, drops and presentation all risk the core RNG streams. | sim/combat.rs:368-378; quality.rs:111-136 | W1-08, W1-09, W2-33, W2-35 |
| SD10 | M | Settings: whole-file reset on a parse error; no version; enum fields would reset bindings. | settings.rs:41-84 | W1-31 |
| SD11 | M | Debug replay bypasses migrate, reconcile and validate. | debug_tools/replay.rs:56-92 | W1-14 |
| SD12 | m | RULESET_VERSION stays 1 across balance waves. | persistence.rs:9-11 | W1-20, W2-49, W4-08 |
| SD14 | m | New commands need one convention for typed errors and recording. | command.rs:26-232 | W1-12, W1-20, W1-23 |
| SD16 | m | BalanceSettings is deny_unknown_fields and saved in 3 places. | balance.rs:23-51 | W1-29 |
| SD19 | m | Smoke reaches rank 4, so balance waves move smoke. | smoke.rs:20-252 | W2-49, W4-08 (before/after) |
| SD20 | m | Docs and headers state "envelope v6". | docs/SIMULATION.md:43,49; docs/VALIDATION.md:217 | W1-63, W7-10 |

### 4.11 Goals 9-10, scripts and captures

| Id | Sev | Finding | Evidence | Fix |
|---|---|---|---|---|
| QA1 | B | Under PS 5.1 every `&`-invoked native exe that writes stderr kills the script when streams are redirected. | probe; scripts/*.ps1 | W1-02, W1-03 |
| QA2 | B | package_playtest uses `Path.GetRelativePath`, absent in .NET Framework. | scripts/package_playtest.ps1:74 | W7-02 |
| QA3 | B | verify_playtest cannot open the zip on 5.1 (assembly not loaded). | scripts/verify_playtest.ps1:14 | W7-02 |
| QA4 | M | The manifest says version 1 and "current working tree". | scripts/package_playtest.ps1:76 | W7-01, W7-02 |
| QA5 | M | The package ships the whole docs tree. | scripts/package_playtest.ps1:33 | W7-02 |
| QA6 | M | `verify_playtest -NativeCapture` on a non-review package never ends. | scripts/verify_playtest.ps1:48-52 | W7-02 |
| QA7 | M | capture_review cannot make reduced-effects stills. | scripts/capture_review.ps1:24 | W1-03 |
| QA8 | M | Captures never set LASTLIGHT_SAVE_DIR, so review logs land in the player's profile. | launch.rs:300-304,441-443 | W1-03, W1-04 |
| QA9 | M | No authoritative scene list; unknown names silently fall back. | ui/review/matrix.rs:358 | W1-05 |
| QA10 | M | `check_views.py` fails at HEAD with 11 errors. | 6 view docs | W1-55 |
| QA11 | M | None of the brief's 11 scenes exist. | ui/review/matrix.rs | W1-54, W3-27, W5-09, W5-10 |
| QA13 | M | The archive move breaks 7 things (provenance, generators, CREDITS, keyart pin, ...). | assets/*/PROVENANCE.json | W1-57 |
| QA14 | m | Caller environment leaks into captures. | scripts/capture_review.ps1:12 | W1-02, W1-03 |
| QA15 | m | Scripts hard-code ./target and ignore CARGO_TARGET_DIR. | scripts/capture_review.ps1:31,39 | W1-02 |
| QA16 | m | Documented commands use pwsh, which is absent. | README.md:28,171; docs/VALIDATION.md:175,235 | W1-55 (horde-defense docs only; root files untouched) |
| QA17 | m | The motion manifest cannot prove reduced-mode behaviour. | capture_sequence.rs:29-47,139 | W1-59, W5-10 |
| QA18 | m | Every non-practice review capture shows the Practice status text. | ui/review/matrix.rs:26 | W1-05 |
| QA19 | M | night-checkpoint and storm cannot be staged as named with today's content. | world.rs:174-181 | W1-54, W5-09 (predicate search, storm table) |
| QA20 | m | No findings ledger for goal 10. | docs/screenshots/* | §10 below, W7-11 |

---

## 5. Chosen and rejected fixes

The full decision and rejection lists (206 decisions, 239 rejected options) are in each `design-<remit>.md`. The tables below give the decisions that shape the work, with the main rejected alternatives. "Ruling" marks a choice that an integration ruling changed from the synthesized spec.

### 5.1 Cross-remit integration rulings (summary)

| Cluster | Rulings | Outcome |
|---|---|---|
| camera-layout | C-1..C-12 | vfx-lighting owns `art/camera.rs` `fit_field` and `ui/battle/layout.rs` `BattleLayout` in wave 1 (4 commits); construction overlay at alpha 235 with an INCOMING chevron; hint band 92 pt with a zoom ≥ 1 fallback; effects headroom 60 pt; observer uses the same fit; dressing capped at the 11-lane unit; HUD ≤ 2 rows is ux-flow's in wave 3. |
| defenders | D1..D17 | `DefenderId { Player, Mara, Recruit(u64) }` public, `DefenderSeat { ModeOwner, Recruit }` saved under `"defender"`; `RecruitStatus::Defending`; the roster entry is the battle's npc; `Campaign.next_defender` / `reserved_recruit`; `ParallelBattle.summary` plus `AcknowledgeOutcomes`; typed `DefenderUnavailable`; worker seats stamped in wave 1. |
| economy-progression | E1..E15 | Ledger-based grants and milestones; catalogue `PresetExtras` plus a saved `balance_extras_basis`; LE command names; no `item_id` in receipt lines; one `crate::hash` with keyed tags; elite flags never change combat-RNG draw counts; `Game::sale_eligibility` facade; byte-identical `drop_tier` None legacy path; `charges_per_rank` as content; `is_one_f32` predicates. |
| settings-audio-tooling | S1..S19 | One Settings v2 struct with per-key salvage and `settings.unreadable.json`; single startup read; `CampaignFeature::id()` seeding; one SHA-256 in `horde_core`; one `asset_audit` module; mirrored archive; NpcShot retired in wave 1; two shipped audio manifests with one writer each; generated CREDITS index; one `Native.ps1`; strict SellItem in three commits with RULESET 2. |
| enemies-world-pacing | W-1..W-15 | PT's harness with named profiles and one `pacing_targets.rs`; OperationsOpen = `operations_available()`; front ramp 0.18; boss damage formula; NPC 3d6 rules for every archetype; storm table plus wall-clock lightning; derived regional checkpoints; generator `--from-existing`/`--check`; stress seam with production cap 224; 10 ms CPU gate; visual twins; docs owners. |
| master planner | P1..P8 | MAX_WAVE_TIER 15 in wave 1; auto-sell handlers in wave 1; provisional-pin ledger; RULESET 2/3/4; LE owns the Results loot block and Auto-sell panel; EB owns checkpoint derivation; PT owns the harness; `horde_audio` crate deferred to wave 6 (open). |

### 5.2 Per remit

**ux-flow**

| Question | Chosen | Rejected (reason) |
|---|---|---|
| Results vs Upgrades | `Screen::Upgrades`; `Screen::ALL` = 8 | A ResultsPage sub-state (Escape and Screen asserts cannot tell them apart); Tab::Upgrades (breaks "fields paused during upgrades") |
| 3-action rule | Exactly 3 action-bar buttons plus a tested whitelist of inline controls | Recruit replacing continue (hides the loop spine) |
| Camp home | `Tab::Home` default, 6-entry row, Mara and Operations as drill-downs | Shortened 8-entry row (fragile at 1.35) |
| Escape | One pure `escape_action`; PassToPause only when Pause is bound to Escape | Per-screen handlers (same-frame races); hard-wired pause |
| Retreat confirmation | 0.8 s wall-clock hold | Modal (covers the field mid-combat) |
| Construction | **Ruling C-6:** overlay panel kept (alpha 235), field never rescaled | The spec's top-band palette and moving Save/Clear plan to Management |
| Settings types | **Ruling S1:** append-only enums (safe under per-key salvage) | Primitives-only bools |
| Expedition notices | **Ruling D10:** `ParallelBattle.summary` + `AcknowledgeOutcomes{battles, folded}` | `StrategyState.outcome_notices` queue |
| Boss banner / low HP | Wall time while unpaused, 2.0 s; low HP enter ≤ 30%, exit > 35% | Sim-time age (0.25 s at 4x); single threshold (flicker) |

**campaign-defenders**

| Question | Chosen | Rejected (reason) |
|---|---|---|
| Defender identity | **Ruling D1/D2:** `"defender"` seat on Attempt and Report, absent = mode owner | `Report.recruit_id` with a derived defender; a stored DefenderId default Player plus migration |
| Battle npc | **Ruling D6:** the roster entry, accessed by seat | Attempt-owned `Takeover { npc }` copy (two copies, copy-back at finish) |
| Status | **Ruling D5:** append `RecruitStatus::Defending` | Reusing Deployed with a derived duty |
| Next defender | **Ruling D9:** `Campaign.next_defender`, `BeginDefense`, Deploy frozen | `Game.next_defender`; overloading `Deploy{mode:Npc}` |
| Recruit allowance | `cap = max(1+first_clears, min(10, 1+ceil(3L/2)))`, allowance 12/28/36/52/60/76/84 for L = 0..6 | B's floor curve 12/20/36/44/60/68/84 (slower on 3-stage routes) |
| Archetype offer | offer[0] = today's rotation, offer[1] keyed (RECRUIT_OFFER), no strategy RNG | Seeding from strategy.rng (shifts every later roll) |
| Checkpoints | **Ruling P6/W-9:** presentation lines only (wave 4) | Night theme on checkpoint stages; a checkpoint coin grant |
| Credit | `commit_victory` + `credit_npc_victory(defender)` | Moving player completion XP to the NPC (moves lane unlocks and pacing) |

**loot-economy**

| Question | Chosen | Rejected (reason) |
|---|---|---|
| Single SellItem | Strict, typed `SaleBlocked` in a fixed guard order (Unknown, InBattle, Reserved, Protected, Locked, Favorite, NpcOnly, Equipped..., Preset) | Design B as the merge base (no Settlement origin, so worker drains during a deployed run hit InBattle); `sell_unprotected` returning Ok(0) in battle |
| Facade | **Ruling E10/R11:** `Game::sale_eligibility` over `sale_eligibility_for` / `plan_sale` | Campaign-level sell paths that bypass recruits |
| Worker drains | `SaleOrigin { Player, Settlement }`; W-a hook plus final sweep | W-b drain at final release; W-c sell on remap |
| Elite drops | **Ruling E8:** forced drop, keyed pick, draw counts unchanged | Short-circuit before `below(100)`; `rng` definition pick for elites |
| Quality | b1 `roll_relative` with sub-floor zeroing (80/17/3 spread) | max(roll, Uncommon) (95% piles on Uncommon) |
| New items | 39 `loot_only` items, never purchasable | All purchasable (moves smoke and pacing in wave 2) |
| Receipt | 12 lines, exact totals, no item_id | 32 lines; 128 lines |
| Legacy drops | **Ruling E11:** byte-identical `drop_tier` None path | Weighted fallback with no legacy promise |
| GOLDEN cadence | **Ruling P3:** provisional ledger, one re-bless per wave | Per-commit GOLDEN re-pins |

**progression-tree**

| Question | Chosen | Rejected (reason) |
|---|---|---|
| Retunes | Effect and price: steady_hands DamageAdd 4 at 120, hardened ArmorAdd 2 at 450, trap and quick-throw raises | B's trap rank_charge_step 2 with div_ceil (adjacent ranks give equal charges, so even ranks are near zero value) |
| Grants | **Ruling E1/E2:** `level_grant(L) = round(0.25 × xp_to_advance(L-1))` paid from a ledger at root finish and idle settle | Paying at the XP transition (double-pays through workers) |
| Milestones | 500/1,000/1,500/2,000 coins plus a fixed-quality item at ranks 5/10/15/20, no retroactive payout | Gates only; keyed quality roll |
| Camp prices | `camp_price(base, rank) = round5(base × min(1 + 0.10·rank, 6))`: training 25/30/50/75/150, recruits 100/110/200/300/600 | base × reward multiplier (near-flat) |
| New nodes | 6 class perks via `ClassModifier` plus 2 counters (shield_breaker, blast_discipline) | Milestone-gated perks that inflate tier_cost |
| Tier prices | Static depth bands T1 1,950 / T2 8,250 / T3 19,700 | Threat-scaled tree prices ("prices rose because I won") |
| Harness | **Ruling W-1/P7:** PT owns v2; acceptance on `expert-deu`, 20 seeds | Pacing ablation as a value metric (hours, noisy) |
| Trap charges | **Ruling E12:** `StructureDefinition.charges_per_rank` | `SkillModifiers.structure_charges` |

**enemy-balance**

| Question | Chosen | Rejected (reason) |
|---|---|---|
| Threat axes | Rank plus front depth (`FrontPosition`), both frozen in the snapshot | A ramp tier offset (risks the first liberation) |
| Ramp | **Ruling W-5:** `front_ramp_max` 0.18 (Normal 1.39x final/first) | 0.15 (1.32x, 1.7% margin) |
| Composition | 12 tiers via per-rank `pressure_floors` rows 5..=28; bound **MAX_WAVE_TIER 15 (P1)** | from_rank step table; a bound of 64 |
| Archetypes | 7 catalog archetypes with optional component fields; NPC rules per archetype (W-7) | Data-only shield-bearer; death-only exploder that hits the defender |
| Elites | 0 below rank 3, 200 + 65·(min(r,23)-3) bp, Easy x0.5 / Hard x1.5; keyed ELITE/AFFIX rolls | Runtime affix clamping; EliteSpawn replacing Spawn |
| Bosses | One phase profile per BossKind, transitions at 50%/25%, 90-tick hold; armor 1 + 0.02·(min(r,50)-4); damage per W-6 | Three phases with summons; phases from rank 1 |
| Hard extras | **Ruling E3:** integer `PresetExtras` plus saved basis | BalanceSettings fields with migration |
| Difficulty | **Ruling W-10:** FNV band by stage count, start clamp [0.95, 1.05] | Upper-only clamp (LUX and AUT starts get easier) |
| Stress | **Ruling W-11:** review/test seam, production cap 224 | `Game.review_only`; widening validation to 400 |

**vfx-lighting**

| Question | Chosen | Rejected (reason) |
|---|---|---|
| Camera | **Ruling R1/C-2:** per-geometry uniform zoom, wave 1 | Constant-k aspect with a 1.10 magnification clamp (shrinks 17x30) |
| Layout | **Ruling C-3:** one `BattleLayout` | Separate `battle_layout` with clamps |
| Storm | **Ruling W-8/RL:** presentation table (`STORM_THEME_IDS = ["depot"]`, 131 of 1,026 stages) with a **wall-clock** schedule. The spec had rejected wall-clock pacing for capture determinism; the orchestrator overrode that because the 4 s rule is in wall time. | `{depot, railway}` (287 stages, not rare); appending a storm theme (re-skins all 242 countries); weather tags in world.json; 240-tick slots |
| Theme freeze | Guard test pinning the 10 themes in order | `Attempt.scene_theme_id` (no reshuffle happens) |
| Render passes | world → grade + sky flash → additive light → vignette → cues with ink backing | A ground-layer grade with actor_multiply (survivor and structures ungraded); Bevy post-process or per-vertex grade |
| Weapon class | `gear_snapshot` Ranged class, falling back to the profile | `profile.weapon.class` only (decision 16 names the snapshot); MG tracer on `Event.seq % 3` (global cursor) |
| Explosions | Six procedural frames per kind; Molotov from a same-tick Burn sibling | UV sub-crops or tints only; a saved `Event.ordnance` field |
| Appearance | **Ruling D17:** `profile.appearance`, Mara unchanged | Npc means Female |
| Provenance | **Ruling S7/S8:** qa's `asset_audit` and `horde_core::sha256` | `test_support/sha256.rs` |

**audio**

| Question | Chosen | Rejected (reason) |
|---|---|---|
| Crate | **Ruling P8:** no crate in wave 1; `crates/horde_audio` proposed for wave 6 (open) | Wave-1 crate with sha256/wav/provenance |
| Loudness | **User gate:** -23 ±2 LUFS integrated, TP ≤ 0.95 at default sliders; offline per-clip ceiling (R3) | A master limiter from the start (allowed only if measured TP > 0.95); a static worst-case N-voice sum ≤ 0.95 (forces about -35 LUFS) |
| Pan | Along the lane, listener-relative, peak-safe balance law, mono-compatible | Literal lane-index or screen-centred pan (lanes are rows; the player's gun would sit hard left); equal-power sin/cos law (3 dB centre drop) |
| Variety | Per-bank round-robin, 5+ variants for frequent cues, presentation-counter jitter; 8 enemy voice families x 3 cue kinds x 5 variants | Per-archetype banks (about 380 files) or an `audio_bank` field in enemy_catalog.json |
| Archetype lookup | Presentation-side `EnemyVoiceIndex` keyed by (battle, enemy id) | A SimEvent payload filled after removal (Kill lookups fail); a saved `CorpseRecord.archetype_id` |
| Music | Director with 7 Deferred track slots plus stingers on the Music bus | New music files (not allowed); a liberation stinger on another remit's report field |
| NpcShot | **Ruling S10:** retired row, files archived in wave 1 | A camp "Murmur" repurposing of NpcShot; the spec's wave-6 move to `archive/audio/retired/` |
| Settings | **Ruling S1/S5:** `music`, `ui`, `mono_audio`; `Settings.version` is the mix version (the spec had rejected the name `music`; overridden) | `music_volume`, `ui_volume`, `audio_mix_version` |
| Provenance | **Ruling S11:** two shipped manifests, one writer each | One merged schema-2 file |

**perf-architecture**

| Question | Chosen | Rejected (reason) |
|---|---|---|
| Camera | **Ruling R1/C-2:** per-geometry uniform zoom, owned by vfx-lighting | Constant 11-lane proportion for every field (17x30 shrinks to 900.7x692); anisotropic fill; vfx's per-axis clamp |
| Normal play records nothing | `PerfProbe` resource inserted only by the review PerfPlugin | Static atomics plus PERF_TEST_LOCK (process-global, flaky) |
| Render-world CPU | extract + render schedule − present wait | ExtractCommands..PostCleanup only (under-reports extraction) |
| One build per pass | `BattleFrame { screen, hud, field }` built once in draw_screen | (no alternative carried to judging) |
| Batching | Runtime NEAREST atlas of the walkers and actors sheets plus a white block; procedural soft-shadow texture | Per-texture-run SpriteBatch with a separate flash pass (wrong overlap); Bevy Mesh2d or instanced sprites (crosses egui and Bevy); 12-segment radial-fan shadows |
| Corpses | Sample only collapsing corpses (≤ 64); cached append-only settled mesh | Newest-1024 cap with per-pass rebuild (still O(n), hides bodies) |
| Budget | `perf_budget.rs` constants mirrored by `docs/PERF_BUDGET.json` with a bidirectional test; **10 ms CPU gate (W-12)** | An extra -30% gate on interface_total; moving the gate to 0.70x of a v3 baseline on drift |
| File splits | Pure moves, one commit per file, behind `pub use` facades | Graph::show method extraction inside the move commit |
| Present modes | **Ruling S3:** `render_plan` with one startup read and live apply | A second settings read in FrontendParts |
| Script | **Ruling S14/S15:** rewrite on qa's Native.ps1 | Own Native.ps1 helper names |
| Stress | **Ruling W-11:** seam, cap 399 + boss, unsaveable | An unvalidated 400-enemy fixture (autosaves fail); presentation-only synthetic EnemyViews; `STAGED_ENEMY_CAP_MAX = 400` in validation |

**save-determinism**

| Question | Chosen | Rejected (reason) |
|---|---|---|
| Serialization | Every v7 field defaulted and skipped at its neutral value; **E13** `is_one_f32` for f32 multipliers | Always-serialized fields (move GOLDEN in wave 1); `_bp` integers only |
| Defender | **Ruling D2/D3:** seat key, worker-only migration | Root "Mara" insert |
| Grants | **Ruling E1:** ledger | Transition-based grants with no ledger |
| Expedition notices | **Ruling D10:** summary | `ParallelBattle.acknowledged` migrated true |
| Hard preset | **Ruling E3:** extras plus basis | `PresetThreat` with Custom = Normal |
| Legacy continuation | **Ruling R4/E11:** frozen snapshots, byte-identical drop path, re-bless with reason if content moves a pin | `ProgressionSnapshot.rules_revision` |
| Re-bless | **Ruling P3:** provisional ledger, one commit per wave | One commit per cause |
| Replay | `Game::from_payload` for old headers; documented divergence under strict sale (S18) | Plain `from_value` |
| save_v7 | Cut and frozen at the wave-1 close (D14) | Re-cut in wave 2 |
| RNG | `presentation_never_reads_core_rng` scanner with a self-test; one `crate::hash` (E7) | `keyed.rs` module |

**qa-captures**

| Question | Chosen | Rejected (reason) |
|---|---|---|
| Native exes in 5.1 | `scripts/lib/Native.ps1` with redirected streams; taskkill /T on timeout (S14). R24: `Invoke-Native` starts the child with `[Diagnostics.Process]::Start` and a `ProcessStartInfo` (UseShellExecute off, stdout and stderr redirected to the logs), the accepted form of the brief's Start-Process rule (Start-Process -PassThru until 381e563) | Stop-Process on timeout (leaves rustc children running); Pester (in-box 3.4 is outdated); `Start-Process -PassThru` (in 5.1 it can report a `$null` ExitCode once the child has exited; R24); `>` (forbidden by the brief) |
| Scene list | A ui-free `review_scenes` registry with `--list-scenes`; unknown names rejected at launch; matrix.rs:358 kept for Workshop prefixes | A registry embedding Screen/Tab (ui is private); `include_str!` sync tests over the ps1 lists; making matrix.rs:358 `unreachable!` (panics Workshop scenes) |
| fixtures.rs:138 panic | Idempotent `enable_legacy_ledgers` before progressed clears (W1-05), then supplemental before-captures (W1-06) | No supplemental before set (the crashed scenes would have no before evidence) |
| Sidecars | `ReviewFrameFacts` (screen, tab, arena_rect, effects_rect, hud_rows, ...) | No sidecar (construction rescale and HUD rows unverifiable); collecting amber primaries from every screen (invasive) |
| Archive | **Ruling S9/U7:** mirrored `git mv` of the full user list | Source-only tagging of strips and keyart (the user approved the archive) |
| Storm | **Ruling W-8:** vfx's table | `storm: bool` on themes; an 11th storm theme; a hashed storm chance |
| Build stamp | `option_env!(LASTLIGHT_BUILD_COMMIT)` on the menu, crash report, startup log and `--version` (W7-01) | A SaveEnvelope stamp (GOLDEN churn); build.rs running git |
| Root dev.ps1 | **Ruling R6/S16:** untouched | Annotating the `../scripts/dev.ps1 verify -Project horde-defense` line in README.md and docs/VALIDATION.md (README.md:165 at 3d492cd, :166 now; VALIDATION.md:147) |
| Committed pairs | Downsampled 2:1, at most 40 MB, CF ledger in this plan (§10) | Raw 2x PNG pairs with a 64 MB budget |

---

## 6. Final v7 schema

**Status: FROZEN at the wave-1 close (W1-63, 6 November 2026)** by `crates/horde_core/tests/fixtures/save_v7.json`; see "The freeze" below. Full table: `docs/plans/production-pass/schema-v7.md`. Summary of the frozen schema:

| Area | v7 change |
|---|---|
| Envelope | `SAVE_VERSION` 7 (v6_to_v7 arm; 8 rejected); `RULESET_VERSION` 2 (W1-20), 3 (W2-49), 4 (W4-08); `SETTINGS_VERSION` 1, then 2 in wave 6; recording FORMAT 1 unchanged; `GAMEPLAY_ENEMY_CAP` 224; `STRESS_ENEMY_CAP` 399 (test/review only); `MAX_WAVE_TIER` 15; `MAX_RUN_DROPS` 4096; `MAX_AUTO_SOLD_LINES` 12. |
| Campaign | `auto_sell: AutoSellRule`, `next_defender: Option<DefenderId>`, `reserved_recruit: Option<u64>`, `milestones_claimed: BTreeSet<u32>`, `balance_extras_basis: BalancePreset`. |
| Player | `level_grant_paid: u32`. |
| Attempt | `"defender"` seat (`DefenderSeat`), `drop_ids`, `drop_tier` (None = pre-v7 legacy loot path). |
| Report | `"defender"` seat, `drop_ids`, `loot_by_quality`, `auto_sold: AutoSoldReceipt`, `progression_rewards`, `boss_rank`. |
| RunRecord | `auto_sold_count/coins`, `level_grant_coins`, `milestone_coins`, `elite_kills`, `boss_ttk_ticks`, `boss_rank`; new writer for `recruit_id`. |
| ParallelBattle | `summary: Option<ExpeditionSummary>`, `pending_auto_sell`. |
| Strategy | `StrategyState.folded_outcomes`, `Recruit.containments`, `RecruitStatus::Defending`. |
| Enemy / ResolvedEnemy | `split_child`, `boss_state`; `reach`, `ranged_armor`, `npc_ranged_armor`, `stagger_immune`, `group_size`, `death_effect`, `tags`, `elite`. |
| ProgressionSnapshot | `elite_rate_bp`, `front_ramp`, `boss_rank`, `boss_phases`, `boss_armor/health/reward/damage_multiplier`, `npc_boss_armor_add`; `wave_tier` ≤ 15. |
| SkillModifiers | `class_modifiers`, `armor_penetration`, `hazard_damage_multiplier`; later (R5) `heavy_target_damage_multiplier`. |
| CombatTelemetry | per-archetype spawned/kills/damage-taken maps, elite counters, hazard damage, boss phase ticks. |
| Events | `Spit`, `Detonation`, `Split`, `BossPhase`. |
| Commands (appended) | `DeletePreset`, `SetAutoSell`, `ApplyAutoSellToStash`, `SetNextDefender`, `BeginDefense`, `SetRecruitTactics`, `SelectCountry`, `RecruitAs`, `AcknowledgeOutcomes`; outcome `Sold`; errors `SaleBlocked`, `WalletLimit`, `AutoSellRuleInvalid`, `NothingToSell`, `SaleQuoteChanged`, `DefenseInProgress`, `DefenderUnavailable`, `TrainingUnsupported`. |
| Migration v6 to v7 | (a) worker defender seats, (e) Npc appearance back-fill, (b) report drop tracking, (c) level-grant ledger when level ≥ 2, (d) milestones reached; idempotent and catalogue-free. |
| settings.json v2 | `version`, `extra_bindings`, `window_mode`, `vsync`, `low_latency`, `music`, `ui`, `mono_audio`, `quality_markers`, `palette`, `seen_callouts`; per-key salvage. |
| Fixtures | `save_v6.json` (W1-01, on untouched v6 code, with V6 pins); `save_v7.json` (W1-63, frozen); `V7_CONTINUATION_FNV` with PROGRESSED_GOLDEN/TRACE at W4-09. |

Digest ledger:

| Wave | GOLDEN (6) | GAMEPLAY_TRACE (6) | V1-V6 continuation | RULESET |
|---|---|---|---|---|
| 1 | 1/6 (G, W1-69 re-bless, user decision) | 3/6 (G, W1-69) | unchanged | 2 |
| 2 | F, T, C (+ G if a pick changes), one re-bless W2-49 | 0 expected | unchanged | 3 |
| 3 | 0 | 0 | unchanged | 3 |
| 4 | G, one re-bless W4-08 | G | unchanged (charges pin re-blessed with reason only if it moves) | 4 |
| 5-7 | 0 | 0 (PROGRESSED 0/4) | unchanged | 4 |

### The freeze (W1-63)

**Declared.** Save format 7 is frozen by `crates/horde_core/tests/fixtures/save_v7.json`, a real `Game::save` generation (envelope version 7, ruleset 2), 95,266 bytes (budget 96 KB), envelope checksum `8527615155064533385`. It was re-cut once in the W1-63 review, before the freeze closed (the live expedition's pending record and the takeover receipts). From now on:

- **Any later saved field** must be `#[serde(default)]` (or `default = "fn"`) with a legacy-neutral value, skipped at that value, and must load `save_v7.json` unchanged: it loads, validates and resaves to the same payload (`v7_fixture_loads_validates_and_resaves_byte_identically`). A field that cannot meet this stops the work, and the user is asked. The one pre-approved exception is R5, `SkillModifiers.heavy_target_damage_multiplier` (`is_one_f32` skip).
- **The file is never re-cut.** Its checksum and byte length are pinned (`v7_fixture_is_a_real_v7_generation`), so a re-cut fails and goes through the stop-and-ask path. Later cases use inline JSON or new `save_v7_w2_*.json` / `save_v7_w4_*.json` fixtures. Its `V7_CONTINUATION_FNV` is pinned at the wave-4 close (W4-09).

**How it was cut.** `save_fixtures.rs::write_save_v7_fixture` (`#[ignore]`, `LASTLIGHT_WRITE_SAVE_V7_FIXTURE=<path>`, refuses to overwrite, asserts `SAVE_VERSION == 7`) runs `stage_v7_scenario` with seed `0x5AFE_0007`:

- Played through `Game::apply` and the smoke bot: Rook liberates Moldova and Romania; Mara replays Moldova stage 1 (the root report, later staged as recruit 2's takeover); three recruits join; recruit 2 wins Ukraine stage 1 on an expedition; recruit 3 is out in Hungary with a synced drop; the stash is sold down; the preset becomes Custom on Hard's extras; auto-sell is on; Mara holds Romania stage 1 to tick 1740 with kills, a corpse and a drop.
- Staged through the test-support seams, because wave 1 has no writer for them: `Staging::stage_takeover(1)` (recruit 1 Defending, its loadout reserved, `reserved_recruit`, the seat and identity appearance); then, through the staging handle and the `*_mut` accessors (`Game::pending_runs_mut` was added for this), drop tracking at the frozen drop tier, a Capstone snapshot on wave tier 11 with every multiplier above 1, per-class skill modifiers, an armored stagger-immune elite, a stationed spitter mid-windup, a splitter and its child, a group of exploders, a boss with `boss_state.entered = 1`, per-archetype telemetry, the auto-sold receipt and unacknowledged summary of the settled expedition, the pending auto-sale and a pending run record of the live one (Hungary at its stage, a Withdrawal under an attempt id reserved from the shared counter, unstamped, so its sync clears nothing), `next_defender` = recruit 2, `milestones_claimed {5}`, `level_grant_paid`, a main-run takeover record with its receipts (recruit 2 on the root report's attempt; the report carries the same seat and the matching auto-sold, level and milestone receipt), `folded_outcomes` and `containments`. Combat logs are trimmed to their last four lines (presentation history) to fit the budget.

**Guards** (`tests/save_migration.rs`):

| Test | Proves |
|---|---|
| `v7_fixture_is_a_real_v7_generation` | The frozen checksum and byte length, envelope version 7, at most 96 KB, and every `V7_SAVED_KEYS` key (root or worker) at a non-default value; the section 11 content |
| `v7_fixture_holds_every_defender_key_at_non_default` | The defender state by JSON pointer; the takeover record holds its receipts and its report names the same recruit; each pending record is its worker's country under an attempt id not in `run_history`; the typed reads agree |
| `v7_fixture_loads_validates_and_resaves_byte_identically` | Load, `validate_game`, resave to the same payload; a second resave is byte-identical; one flipped seed digit (the payload still valid JSON) is refused with `ChecksumMismatch` |
| `v7_fixture_round_trips_every_enemy_balance_field_to_equal_digest` | The 37 enemy and balance keys (3.1 basis, 3.8-3.12) keep their saved text; the loaded digest is the saved checksum |

### Conventions (binding after the freeze; schema-v7.md section 1)

| # | Rule |
|---|---|
| C1 | Every new field: serde default with a legacy-neutral value plus `skip_serializing_if` at that value; mandatory on types without a container default. |
| C2 | Predicates live in `crate::schema` (`is_default`, `is_false`, `is_zero_f32` by bits, `is_one_f32` by bits, `one_f32`, `one_u8`). Writers produce the literal 1.0 at neutral inputs; validation rejects non-finite values. |
| C3 | Wave-1 writers were GOLDEN-neutral only (worker seat stamping, auto-sell commands, the extras basis). |
| C4 | `deny_unknown_fields` types stay closed: `BalanceSettings` gains no field; `ResolvedEnemy` only under C1. |
| C5 | `v6_to_v7` is path-based, idempotent and catalogue-free. |
| C6 | Command, CommandError, CommandOutcome, RecruitStatus, BossRank, SettlementIssue, EventKind/SimEvent and the hash tags are append-only; recorded Command shapes are frozen. |
| C7 | `state_digest` is the FNV of the serialized game; serde-skipped state, settings, catalogues and assets are never in it. |
| C8 | Presentation and keyed rolls never draw `Attempt.rng` or the strategy RNG; keyed rolls use `hash::keyed` with append-only tags. |

### Provisional-pin and re-bless protocol (ruling P3)

1. A digest-moving commit never edits `GOLDEN` or `GAMEPLAY_TRACE`. It sets `PROVISIONAL` (in `tests/determinism.rs`) to the six new digests of each family and appends one `PROVISIONAL_CAUSES` row: commit tag, class (F format, C content, T telemetry, G gameplay, as `support::classify` assigns from `determinism_anatomy`) and the reason. The test accepts `PROVISIONAL` only while the causes are non-empty.
2. The wave's one save-determinism re-bless commit (W2-49, W4-08) folds `PROVISIONAL` into `GOLDEN` and `GAMEPLAY_TRACE`, writes a reason per moved digest into the determinism log and the commit body, and resets `PROVISIONAL` to None and the causes to empty.
3. The wave-close critic checks that `PROVISIONAL` is None. A move with no listed cause, or a G move where none is expected, stops the work.

**Wave 1 result:** through W1-68 `GOLDEN` 0/6 and `GAMEPLAY_TRACE` 0/6 moved (both families identical to the pre-W1-01 parent of `b82658a`). W1-69, the user's 2026-11-06 decision that enemies overlap (§2.2), is the wave-1 re-bless: `GOLDEN` 1/6 (0x1A57_1167 Personal) and `GAMEPLAY_TRACE` 3/6 (0x1A57_1167 Personal and Npc, 0xDEAD_BEEF_2026 Npc), class G, folded directly with a reason per digest in `tests/determinism.rs`. `PROVISIONAL` is None with no causes; `RULESET_VERSION` 2 (W1-20, label only); V1-V6 continuation pins, `V6_DROP_TRACE_FNV` and the `save_v7.json` pins unchanged. R15: W1-69 does not bump `RULESET_VERSION`. The W2-49 bump (2 -> 3) names both W1-69 (no lane queue) and the wave-2 change, so until then a debug recording made before W1-69 replays without a ruleset warning even though its combat can diverge (VALIDATION.md "Strict sale and ruleset 2").

**Wave 2 result (W2-49, 10 November 2026):** six wave-2 commits moved `GOLDEN` provisionally: W2-05 (T), W2-06 (F), W2-08 (C), W2-52 (F), W2-09 (G under R33; `support::classify` labels it C) and W2-54 (F). W2-49, the wave-2 re-bless, folds them. `GOLDEN` moved 6/6, every run net class F against the wave-1 close anatomy (ab03da9): key paths and the telemetry-stripped payload move, while items, outcome summary and trace do not. `GAMEPLAY_TRACE` moved 0/6. `PROVISIONAL` is None with no causes, and the reasons per digest are in the `tests/determinism.rs` re-bless log and VALIDATION.md "Wave-2 re-bless and ruleset 3". `RULESET_VERSION` and `TRACE_RULESET` are 3, and the bump names W1-69 (no lane queue, R15) and the wave-2 rules. The V1-V6 continuation pins, `V6_DROP_TRACE_FNV`, the `save_v7.json` pins and the frozen-catalogue pin are unchanged. Smoke and pacing after are in `target/production-after-wave2/` (§9.2 and §9.6).

### Expected moves after the freeze

| Wave | Digest | Expected | Class | Cause (commit) | Re-bless |
|---|---|---|---|---|---|
| 1 | GOLDEN 1/6, GAMEPLAY_TRACE 3/6 | yes (user decision 2026-11-06) | G | enemies never block each other (W1-69) | W1-69 |
| 2 | GOLDEN 6/6 | yes | F | `drop_tier` always Some plus `drop_ids` at depth 0 (W2-06) | W2-49 |
| 2 | GOLDEN, the three Personal state digests | yes (R16) | as `support::classify` assigns | neutral level-ups keep `Player.level_grant_paid` at the level (W2-52) | W2-49 |
| 2 | GOLDEN, the three Personal state digests | yes (W2-54; R32 accepted the §2.5 default) | as `support::classify` assigns | the authored 0.25 grant makes the W2-52 shim inert, so the mid-run saves owe `level_grant_paid` again (W2-54) | W2-49 |
| 2 | GOLDEN 6/6 | yes | T | per-archetype telemetry writer (W2-05) | W2-49 |
| 2 | GOLDEN | yes | C | the 39 `loot_only` items enlarge the tier pool (W2-08) | W2-49 |
| 2 | GOLDEN | only if a GOLDEN item changes | G | the weighted or relative pick (W2-07, W2-09). W2-09 moved the 0x1a571167 Personal digest: class G under R33, though `support::classify` labels the move C (items only); W2-49 records both labels | W2-49 |
| 2 | GAMEPLAY_TRACE | no (rank 0: no elites, no new archetypes; one draw) | G | any move needs a G reason plus smoke and pacing evidence | W2-49 |
| 2 | V1-V6 continuation, `V6_DROP_TRACE` | no (the `drop_tier` None path is byte-identical) | - | - | - |
| 3 | all | no | - | UI only | - |
| 4 | GOLDEN and GAMEPLAY_TRACE | yes, if rank-0 inputs move | G | balance (W4-*) | W4-08 |
| 4 | trap `charges_per_rank` pin | not expected | G | re-blessed in W4-08 with its reason if it moves | W4-08 |
| 4 | PROGRESSED_GOLDEN (4), PROGRESSED_TRACE, `V7_CONTINUATION_FNV` | new pins | - | cut at the wave-4 close (W4-09) | - |
| 5-7 | every family | no | - | any core move: stop and ask the user | - |

---

## 7. Master wave plan

Full commit list: `docs/plans/production-pass/waves.md`. Every commit is test-first and leaves the full verify green; one writer at a time; one commit per finding.

| Wave | Scope | Commits | Owners | Digest | Exit |
|---|---|---|---|---|---|
| 1 | Goal 1, camera (R1), sale eligibility, v7 schema + migration + fixtures, enemy_visuals v2, archive, PS 5.1, review panic fix, enemies overlap (user decision) | W1-01..W1-69 | all but ux-flow | one re-bless (W1-69, user decision) | **Closed under R25 at HEAD ab03da9 (2026-11-07; "Wave-1 close" below).** PERF gates pass at 199d823 except the open gate-3 episode (stress-on-r2, W2-50); `save_v7.json` frozen; camera capture review logged (W1-60); GOLDEN 1/6, TRACE 3/6, all from W1-69 |
| 2 | Auto-sell, takeover, SelectCountry, scaling, archetypes, elites, bosses, progression and skills | W2-01..W2-54 (W2-50 and W2-51 before W2-47's re-gate; W2-52 and W2-54 before the W2-49 re-bless, R16 and R32; W2-53, the frozen-catalogue guard, before W2-08 and W2-26 (W1 review); W2-51's gate change was applied in the W1 review, R22) | all but audio | one re-bless (W2-49) | Pacing and smoke before/after; stress re-gate; visual twins for audio |
| 3 | Goal 2 and the UI for goals 3-4 (horde_game plus pure core reads) | W3-01..W3-31 (W3-29 slots beside W3-14, W3-30 and W3-31 before W3-27's review; all three vfx-lighting, R17) | ux-flow, loot-economy, progression-tree, enemy-balance, vfx-lighting, qa-captures | 0 | Full-loop headless test at both sizes; W3 capture matrix reviewed |
| 4 | Balance (goals 5-6) and the goal-3 numbers | W4-01..W4-11 plus tuning loops | enemy-balance, progression-tree, loot-economy, campaign-defenders, save-determinism | one re-bless (W4-08) | Every ignored `pacing_targets` assertion passes in release, with before/after tables |
| 5 | Goal 7 graphics (horde_game only) | W5-01..W5-12 | vfx-lighting, qa-captures, perf-architecture, save-determinism | 0 | fx-stress within budget; reduced MOTION gates; human review logged |
| 6 | Goal 8 audio | A6-00..A6-19 | audio, save-determinism | 0 | -23 ±2 LUFS / TP ≤ 0.95 gate closed; listening pass logged pending |
| 7 | Goals 9-10 and docs | W7-01..W7-12 | qa-captures, audio, loot-economy, progression-tree, campaign-defenders, ux-flow, perf-architecture, save-determinism | 0 | Package verified under 5.1; `check_capture_findings.py --final` exits 0; Outcome written |

Ordering constraints that cross remits: perf's `ui/battle` split (W1-39) lands before vfx C2 (W1-44); SD's settings commit (W1-31) lands before perf's `render_plan` (W1-34); qa's archive (W1-57) lands before audio's NpcShot commit (W1-58); CD's takeover core (W2-11) lands before LE's H1 (W2-14); H1 lands before PT's progression settlement (W2-22); EB's spitter and boss commits land before vfx's visual twins (W2-28, W2-37).

**Wave-1 close (R25, 2026-11-07).** Wave 1 is closed under R25 at HEAD ab03da9. Each condition:
- **Two review rounds in a row with nothing above minor.** Review rounds 1-4 confirmed 22, 14, 16 and 13 findings. Rounds 5-10 confirmed 4, 9, 2, 5, 3 and 3, all minor, so every pair of consecutive rounds from round 5 on holds nothing above minor.
- **Perf acceptance green at HEAD.** The Goal-1 acceptance re-run at bee4236 (attempt 2, recorded in ab03da9; §9.1 and §9.4) passed every cell. ab03da9 changes docs only.
- **Full verify green at HEAD.** The full verify at ab03da9 was green: fmt, the 3 clippy configs, the workspace tests (1,046 passed), `-p horde-defense --no-default-features` (521) and `--features dev` (578), `--smoke`, check_views, both audio generators' `--check`, `run_script_tests.ps1` (31 cases), and `pacing 60 --runs` byte-identical to the post-W1-69 baseline `target/production-after/w1-69/pacing.txt`. Evidence: `target/production-after/wave1/verify-final/`.
- **Every confirmed finding fixed or ruled.** Every finding of rounds 5-10 is fixed except the Invoke-Native item, which R24 resolves. The wave-1 close rulings are R12-R25. Under R25, minor findings still open would carry into the wave-2 review; rounds 5-10 leave none open.

Wave 1 hands on these open items:
- **`pacing_targets` (W2-03).** `tests/pacing_targets.rs` does not exist yet, so the wave-1 release run (`target/production-after/wave1/pacing_targets.log`) matched 0 tests. W2-03 creates the goal-5 evaluators and the ignored goal5_* tests, and W2-04 adds the goal6_* tests.
- **The W1-69 balance shift (wave 2).** The novice Germany profile went from 5 to 8 defeats and its first victory moved from run 3 to run 5; smoke failed attempts went Personal 2 → 1 and Npc 1 → 3 (§9.6). Balance was not retuned in wave 1. Wave 2's pacing work owns the shift, measured against the post-W1-69 baseline.
- **Gate 3 (W2-50).** The sample-932 stall-tail frame from the 867f786 re-run and the W1-61 stress-on-r2 episode stay open. W2-50 owns both, before the W2-47 re-gate. Gate 3 is not claimed clean in dev VSync-on stress or in the shipped default configuration (§2.5). W2-50 (2026-11-08) attributed the stalls and the tail to the display being off and did not reproduce the episode (9.4). Both §2.5 rows await an answer, so W2-50's Done is not met and W2-47's re-gate waits for them. **Answered 2026-11-09 (user decisions R26 and R27):** the episode closes as environmental (user activity during an evidence run), and gate 3 in the shipped default configuration can be claimed once the W2-47 re-gate passes. The `driver_stall` rule is unchanged; `measure_performance.ps1` holds the display on and treats a run that overlaps a Kernel-Power 506/507 display-off event as invalid evidence, and the sample-932 tail is closed as display-off. W2-50's Done is met once that harness change lands; it landed on 2026-11-09 (`feat(horde): measure_performance holds the display on and rejects display-off runs [W2-50]`), so W2-50 is done.
- **W2-52 before W2-49.** The `level_grant_paid` shim moves three Personal GOLDEN state digests, so it lands before the wave-2 re-bless (R16).
- **The R18 items in W2-47:** the sustained 15-20 FPS VSync-on stress episode, the 1050x700@1.35 VSync-on p99 interval of 19.1-19.7 ms, the script-level battlefield primitive and vertex gates in `measure_performance.ps1`, and any egui_probe/present_wait span-ordering item not fixed in wave 1, including the `baseline.v3` re-measure on the 7abb0ab spans (§2.5; R29, 2026-11-09: re-measured in W2-47 before any interface_total re-gate, gates only tighten). R30 (2026-11-09): every perf pass first waits for a quiet machine and rejects a run with a CPU load sample of 90% or more. R31 (2026-11-09): the interface_total re-gate decision is an interleaved A/B against the wave-1-close executable (ab03da9), B's median at most 1.05 × A's per size, not the absolute baseline. **Done (W2-47, 2026-11-10; VALIDATION.md "W2-47 re-gate on the wave-2 stress fixture", `PERF_BUDGET.json acceptance.w2_47`):** baseline.v3 re-measured (9.4; the gated pair kept its W1-37 values under R22); every gate pass exits 0 on the new stress fixture (dev and playtest, VSync on and off), but the dev -VsyncOff pass only on a rerun of a first invocation that had a valid progressed-peak small over (12.7565 ms against 10.0, masked to exit 2 by invalid runs), so that gate is not claimed clean pending the §2.5 row "A valid over in a pass masked by invalid runs" (corrected in the W2-47 review). Answered by R34 (2026-11-10): the over stands, G1.9 dev -VsyncOff progressed-peak is not claimed clean at W2-47, and W5-11 re-gates it. The A/B passes at B / A 0.962 / 0.963 after its build fix; gate 3 in the shipped default configuration is claimed (R26); the small-size VSync-on p99 interval was measured as hour-dependent and in the swap-chain acquire, not attributed further, and gates nothing.
- **Pending human checks (§11, R23).** The four wave-1 checks (camera feel, stepped damage pop, whole-pixel text placement, sheet upscale at 2.5x) and the integrated-GPU run stay pending.

---

## 8. Acceptance matrix

Full matrix: `docs/plans/production-pass/acceptance.md` (also copied into `docs/ACCEPTANCE.md` by W7-08).

| Group | Rows | Key numeric gates |
|---|---|---|
| Goal 1 | G1.1-G1.15 | -VsyncOff CPU p99 ≤ 10 ms and 0 frames > 50 ms (progressed-peak, stress); interface ≤ 1.129 / ≤ 1.030 ms; BattleScreen/BVM/HUD built {1,1,1} per pass; ≤ 24 battlefield primitives (48 until R22), 400 vs 52 enemies differ by ≤ 2; save clone remedy if p99 > 1 ms |
| Goal 2 | G2.1-G2.12 | exactly one amber primary per screen; 3 Results actions; ≤ 2 HUD rows at 1050x700@1.35; boss banner ≥ 1.5 s wall; retreat hold 0.8 s; construction keeps the arena rect |
| Goal 3 | G3.1-G3.16 | allowance 84 (10 levels) by the 6th liberation; parallel ≤ 2/3 of solo runs for 3-stage neighbours; < 5 outbreak reopens over 10 liberations; liberations 2/4/10 and coins per minute reported (20 seeds) |
| Goal 4 | G4.1-G4.15 | 500-case property test; receipt ≤ 12 lines with exact totals; auto-sell profile within 1 run of baseline at tiers 1-3 |
| Goal 5 | G5.1-G5.11 | first victory run 2-4; tiers 1-3 at about runs 4/8/12; tree outlasts the 2nd liberation; unlock order Fortifications → Mara → Operations; no dominated node; no node < 100 coins worth > 2x its branch per coin |
| Goal 6 | G6.1-G6.18 | defeat rate 10-25% per 20-run window after the first victory; no 15-win streak after run 10; final stage ≥ 1.35x first (guard; brief 1.3x); wave tiers 7/10/11 at ranks 10/20/40; LUX Easy ≤ 6 runs, Hard ≤ 1.5x Normal; boss TTK 20-60 s; elite 8.5 ± 0.5% at rank 13 |
| Goal 7 | G7.1-G7.9 | night ≤ 0.70 of dawn luminance; cue contrast ≥ 3:1; lightning ≥ 4,000 ms apart at 4x, ΔL < 0.2; decals ≤ 64, light pools ≤ 24; camera zoom table pinned |
| Goal 8 | G8.1-G8.12 | -23 ±2 LUFS integrated, TP ≤ 0.95 FS; no repeat in 1,000 triggers; ≥ 5 variants for frequent cues; 75 shipped WAVs all with matching sha256; 1 cue per sale batch |
| Goals 9-10 | G9.1-G9.3, G10.1 | docs in the zip = PLAYTEST_GUIDE.md ≤ 65,536 B; packaged smoke and verify pass under 5.1; every CF row fixed or user-approved |
| Decisions and isolation | D8, D9, D10, D18, ISO-1..ISO-11, R7, R8, HUMAN | root files untouched; 5.1 only; append-only content; old settings load; typed Commands; one re-bless per moving wave; presentation never reads core RNG; no screen shake; reduced effects covers every new effect |

---

## 9. Measurement tables (before / after)

Filled at each wave close. "Before" values are wave 0 unless a wave records a new baseline (W1-37 re-baselines frame report v3 on the unchanged renderer; W2-01 records the 20-seed pacing baseline).

### 9.1 Frame reports

| Scene | Size | Metric | Wave 0 | Target | After W1 | After W2 | After W5 | Release candidate (W7-09) |
|---|---|---|---|---|---|---|---|---|
| progressed-peak | 1440x900 | interface mean (ms, VSync on) | 1.613 | ≤ 1.129 | 0.998 (dev, median of 3, W1 review at bee4236; 867f786 1.039, W1-61 0.990) | | | |
| progressed-peak | 1050x700@1.35 | interface mean (ms, VSync on) | 1.472 | ≤ 1.030 | 0.989 (dev, median of 3, W1 review at bee4236; 867f786 1.012, W1-61 0.952) | | | |
| progressed-peak | both | CPU p99 (ms, -VsyncOff) | n/a (v3 dev: 20.7 / 19.1, see 9.4) | ≤ 10.0 | dev 6.77 / 6.63, playtest 6.25 / 5.92 (median of 3, W1 review at bee4236; 867f786 dev 6.80 / 6.47, playtest 5.91 / 5.61; W1-61 dev 8.85 / 7.01, playtest 6.26 / 6.44) | | | |
| stress (400) | both | CPU p99 (ms, -VsyncOff) | n/a (v3 dev: 31.8 / 38.1, see 9.4) | ≤ 10.0 | dev 7.30 / 7.28, playtest 6.43 / 6.28 (median of 3, W1 review at bee4236; 867f786 dev 7.58 / 7.13, playtest 6.48 / 6.09; W1-61 dev 7.50 / 7.84, playtest 6.48 / 6.54) | | | |
| progressed-peak, stress | both | frames > 50 ms | 0 (VSync on, 300 frames; v3 dev -VsyncOff, 600/1800 frames, worst repeat: progressed-peak 0 / 1, stress 39 / 40; VSync on 6-64, see 9.4) | 0 | W1 review at bee4236 (attempt 2): 0 outside driver stalls in every repeat of the three gate passes, with the stall active in all three: 15 driver stalls excluded (dev VSync on, 69-1,000 ms, acquire share 0.867-0.993), 48 (dev -VsyncOff) and 48 (playtest -VsyncOff), the VSync-off ones 487-612 ms at 0.983-0.991; the extra VSync-on peak and stress passes were not re-run; see 9.4. W1 review at 867f786: 0 outside driver stalls in every repeat of the three gate passes (dev VSync on progressed-peak; dev and playtest -VsyncOff, all scenes); the playtest -VsyncOff pass excluded 35 driver stalls (494-531 ms, acquire share 0.987-0.990), dev 0. The extra dev VSync-on stress pass exited 1 on one gated frame (stress normal r1, 54.7 ms, acquire share 0.794, directly after three ~1,000 ms stalls) beside 40 excluded stalls; see 9.4. W1-61: 0 in every repeat of every W1-61 acceptance pass (dev VSync on; dev and playtest -VsyncOff), 0 driver stalls excluded, in a stall-free hour (see 9.4: the A/B harness also had 0 then). The A/B passes had 7-21 driver stalls per VSync-off cell and 0-5 per VSync-on cell, excluded by the user ruling (2.2). **Not clean:** one A/B repeat in the shipped default configuration (playtest, pipelined on, VSync on, stress normal: `head-ab-playtest-vsync/stress-on-r2`) had 6 gated frames over 50 ms and exited 1, with 407 frames over 33 ms outside stalls (p99 47.1 ms, mean 46.3 FPS; about 400 frames of 47-67 ms). Open item, see 9.4 | | | |
| peak | 1440x900 / 1050x700@1.35 | interface mean (ms, VSync on) | 1.190 / 1.075 | ≤ 1.1904 / 1.0752 (not above wave 0; PERF_BUDGET.json `scenes.peak`, gated by measure_performance.ps1 in the dev profile with VSync on) | 0.784 / 0.774 (dev, VSync on, median of 3, W1 review at 867f786; W1-61 0.775 / 0.768) | | | |
| camera zoom delta (W1-47) | both | peak and progressed-peak, commit before C2 vs C2 | n/a | recorded | interface_draw peak +6.2% / +4.1%, progressed-peak -1.0% / +1.9% (see 9.5) | | | |
| pipelined decision (W1-61) | both | on vs off, Repeat 3 | n/a | recorded | default stays on: off misses the VSync-on p99 interval at the small size (19.13 / 19.66 ms > 17.7); see 9.4 | | | |
| save clone (W1-50) | n/a | p99 (ms) at 1,000 / 2,500 / 5,000 items, 3 workers | n/a | remedy if > 1 ms | remedy applied (Arc run history, a93e340): p99 median dev 0.654 / 0.900 / 1.144, playtest 0.672 / 0.739 / 1.157 ms; gate (1,000 items) passes; VALIDATION.md "Save clone" | | | |
| shipped `--smoke` | n/a | seconds | n/a | recorded | shipped_smoke_seconds 1.26 (median of 2.72 / 1.25 / 1.26, playtest `--no-default-features`, W1 review at bee4236; 867f786 1.23 of 2.79 / 1.23 / 1.19; W1-61 1.23 of 2.43 / 1.19 / 1.23) | | | |

**Spans (W1 review, 2026-11-07).** The W1-61 values in brackets, and baseline.v3, were measured on the probe spans from before review commit `7abb0ab`. That commit narrowed egui_pass to `EndPass` itself and moved surface configuration out of present_wait. So interface_total (same or lower) and cpu_frame (same or higher) from `7abb0ab` on are not like-for-like with them. The "W1 review at bee4236" and "867f786" values are the two re-runs at HEAD on the new spans (9.4, "Goal-1 acceptance re-run at HEAD" and "attempt 2"); the dev executable was byte-identical in both, the commits between them being docs only. The peak row stays at 867f786: attempt 2 ran only the three gate passes. baseline.v3 itself was not re-measured then. See 9.4 and the open item in 2.5. (W2-47, R29: baseline.v3 is now re-measured on the 7abb0ab spans, 9.4.)

### 9.2 Pacing and smoke

| Measure | Wave 0 | After W2 (W2-49) | After W4 (W4-08) |
|---|---|---|---|
| expert-deu first victory (1 seed / 20-seed median) | run 4 / (W2-01, post-W1-69: run 3 / 3 [2, 5]) | run 2 / 3 [2, 4] | |
| Tier 1 / 2 / 3 affordable | runs 3 / 5 / 7 | 20-seed 3 [2, 3] / 5 [5, 6] / 8 [8, 9] at 930 / 5,190 / 11,675 coins (W2-41 nodes counted) | |
| Tree exhausted vs Liberation(2) | run 24 vs (W2-01, post-W1-69 20-seed median: 25 [24, 26] vs 17 [16, 18]) | 26 [25, 27] vs 17 [16, 18] (177 nodes) | |
| Unlock order Fortifications / Mara / Operations (median) | 4 / 14 / 14 (Recruitment) | 3 [2, 4] / 14 [13, 15] / 14 [13, 15] | |
| Defeat rate per 20-run window after first victory | 0% after run 10 / (W2-04, 20 seeds: 0.0-13.2%, 36 of 39 windows under 10%) | 0.0-16.2%, 35 of 39 windows outside 10-25% | |
| Longest win streak after run 10 | 50 / (W2-04, 20 seeds: median 50, every seed 46-50) | median 50, every seed 45-50 | |
| LUX runs to liberate Easy / Normal / Hard | (W2-04, 20-seed median: 4 [4, 4] / 6 [5, 7] / 8 [8, 8]) | 4 [4, 5] / 5 [5, 6] / 9 [8, 11] | |
| Boss TTK median (s) | (W2-04, 20 seeds: 6.4 [p25 5.9, p90 32.1] over 1,091 kills) | 25.5 [p25 21.5, p90 36.4] over 1,090 kills | |
| Smoke Personal stages 1-5 / Npc stages 1-5 | see 1.3 | PASS; every stage won, failed attempts Personal 2 (stage 1 x2) / Npc 3 (stage 1 x2, stage 2 x1) (§9.6) | |
| Strategy profile: liberations 2/4/10, coins per minute | (W2-43, before tuning, 20 seeds: S1 runs 8 / 18 / 42, 3,154 coins/min; S2b/S2a runs to free 3 neighbours 9 / 9) | S1 runs 8 / 18 / 41, 3,183.5 coins/min; S2b/S2a 9 / 9 | |

W2-01 20-seed baseline (2026-11-07, pacing harness v2 on the post-W1-69 rules): `cargo run --locked --release -p horde_core --example pacing -- 60 --profile expert-deu --profile novice-deu --profile novice-gear-lux --seeds 20`, once with `--csv --out target/production-before/pacing_20seed_before.csv` (3,600 runs, sha256 af9763f3...54b1b) and once with `--fit-tiers 3,5,7,9,11,13 --out target/production-before/pacing_20seed_before.txt` (sha256 321a9af1...98b3), both not committed. Cells are the lower median [p25, p75] of runs completed; a missed milestone counts as 61. expert-deu: first victory 3 [2, 5]; tiers 1 / 2 / 3 affordable 3 / 5 / 7; Fortifications / Mara / Recruitment / Operations unlocked 3 / 14 / 14 / 23; Liberation(1) 14 [13, 15], Liberation(2) 17 [16, 18]; tree exhausted 25 [24, 26]; rank 5 / 10 / 15 / 20 at 10 / 15 / 20 / 25. Median earned at runs 3 / 5 / 7 / 9 / 11 / 13 over tier 1-6 cost: 1.37 / 0.87 / 1.07 / 1.56 / 1.88 / 2.23. Seed index 0 is the profile's own seed, so the default `pacing 60 --runs` still equals `target/production-after/w1-69/pacing.txt` byte for byte (§9.6).

W2-08 (loot-only drops) moved that default one-seed output: it is now `target/production-after/w2-08/pacing.txt` (sha256 547f7b4e...cb2e4d3c). The before/after rows are in §9.6, "W2-08 moved the default pacing output", and whether it becomes the wave-2 baseline was open in §2.5 (answered by R28, below).

W2-26 (the in-country front ramp) moved it again: the latest default output is `target/production-after/w2-26/pacing.txt` (sha256 2849040d...59b68bcb). The rows and the W2-26 20-seed acceptance (goal-6 LUX and start-country lines) are in §9.6, "W2-26 moved the default pacing output".

W2-32 and W2-33 moved it again, and W2-36 (boss phases past rank 4) once more; the latest default output is `target/production-after/w2-36/pacing.txt` (sha256 f8000150...017da5f2). W2-33 also moved smoke stages 4-5 (§9.6, "W2-33 moved the default pacing output"); W2-36 did not move smoke (§9.6, "W2-36 moved the default pacing output").

W2-39 (country difficulty bands and the start clamp) moved it again, and smoke's legacy autonomous line: the latest default output is `target/production-after/w2-39/pacing.txt` (sha256 f8856265...7918d5a0). The rows and the W2-39 20-seed acceptance are in §9.6, "W2-39 moved the default pacing output".

W2-41 (the eight decision-19 nodes, which `tier_cost` still counts) moved it again; smoke did not move. The latest default output is `target/production-after/w2-41/pacing.txt` (sha256 e19f0669...d6bbe65c). The rows and the W2-41 20-seed acceptance are in §9.6, "W2-41 moved the default pacing output".

R28 (2026-11-09) accepts the W2-08 move and re-baselines: later commits diff the default `pacing 60 --runs` output against `target/production-after/w2-41/pacing.txt` (sha256 e19f06694be215fe05068c542cb3f56e8741d2ed959aa5db03ba9d68d6bbe65c), not `w1-69/pacing.txt`. A wave-2 commit may move pacing on purpose if it records the before/after rows in §9.6.

W2-09 (the weighted DropTable) moved it again after the R28 re-baseline, on purpose; smoke did not move. The latest default output is `target/production-after/w2-09/pacing.txt` (sha256 0510b062...f2574879). The rows and the W2-09 20-seed acceptance are in §9.6, "W2-09 moved the default pacing output".

W2-22 (progression settlement, 2026-11-09) moved neither. The shipped catalogue still authors no level grant and no milestone, so settlement pays 0 coins and grants no item; it only claims the reached frozen ranks, as the removed wave-1 shim did. Before is the last recorded output of each: pacing `w2-09/pacing.txt` (W2-14, W2-16 and W2-17 moved no balance and did not re-run it) and smoke `w2-16/smoke.txt`. After, at W2-22: `--smoke` (`target/production-after/w2-22/smoke.txt`, PASS, sha256 c47b083a...eb8969ae) is byte-identical to `w2-16/smoke.txt`, and `cargo run --locked --release -p horde_core --example pacing -- 60 --runs --out target/production-after/w2-22/pacing.txt` (sha256 0510b062...f2574879) is byte-identical to `w2-09/pacing.txt`. The 20-seed acceptance was not re-run: the default and 20-seed sweeps run the same rules, and no reward is authored. The smoke and pacing move that ruling E2 expects comes with the authored values, which the W2-22 review handed to W2-54 (§2.5).

W2-42 (the pacing profiles that need other remits' commands, 2026-11-09) moved neither. It adds the registry rows `manual-sell`, `auto-sell`, `takeover`, `farm`, `easy`, `hard` and `long` and the plan variants `PacingSell::{ManualExtras, Auto}`, `PacingDefender::RecruitWhenReady` and `PacingFarm::ReplayAfterDefeats(u32)`; the three legacy profiles keep the solo plan and their code path. `cargo run --locked --release -p horde_core --example pacing -- 60 --runs --out target/production-after/w2-42/pacing.txt` (sha256 0510b062...f2574879) is byte-identical to `w2-22/pacing.txt`, and no rule moved, so smoke was not re-run. The new rows at one seed (`--profile expert-deu --profile manual-sell --profile auto-sell --profile takeover --profile farm --profile easy --profile hard --profile long`, `target/production-after/w2-42/profiles.txt`, sha256 15e7f5b9...5db09ac6; not committed), runs to first victory / first liberation / tree exhausted: expert-deu 2 / 15 / 27, manual-sell 3 / 13 / 25, auto-sell 5 / 17 / 29, takeover (Luxembourg) 2 / 5 / 43 with the first takeover at run 7, farm (novice) 6 / 19 / 30, easy 2 / 10 / 26, hard 5 / 18 / 30, long as expert-deu (first takeover > 150). Selling drops moves the expert's first victory because `recommended_node` reads owned items (as W2-08 found). Manual-sale coins reach the wallet but not `cumulative_coins`, whose definition (design-progression-tree §11) counts only auto-sold coins. The takeover plan hands every stage a recruit lost back to the survivor, otherwise one starter-loadout recruit loses the same stage for the rest of the profile (measured before that rule: second liberation > 60 runs). `equip-drops` (G5.6) is not in the W2-42 row and stays unimplemented (`equip_drops` is still rejected). The W2 review implemented it later (147f422, `equip_drops_profile_wears_dropped_upgrades`; waves.md W2-42 note): the solo loop supports `PacingPlan.equip_drops`, only the strategy bot still rejects it, and the default output did not move.

W2 review (2026-11-10): the W2-42 rows above are one seed, and design-progression-tree §"Acceptance" runs `long` at 5 seeds (informational). All eight rows at 5 seeds, on the post-W2-49 rules (so after W2-54's level grant and milestones; the one-seed rows above predate them): `cargo run --locked --release -p horde_core --example pacing -- --profile expert-deu --profile manual-sell --profile auto-sell --profile takeover --profile farm --profile easy --profile hard --profile long --seeds 5 --out target/production-after/w2-review-b8/profiles-5seed.txt` (sha256 6d8b3fc1...95dcdfa8, not committed; about 5 minutes with the release build). Cells are the lower median [p25, p75], every seed reaching each milestone; runs to first victory / first liberation / second liberation / tree exhausted: expert-deu 4 [3, 4] / 15 [14, 15] / 18 [17, 18] / 27 [26, 27]; manual-sell 3 [3, 4] / 13 [13, 14] / 16 [16, 17] / 25 [25, 26]; auto-sell 4 [3, 4] / 15 [14, 16] / 18 [17, 19] / 27 [26, 28]; takeover (Luxembourg) 3 [2, 4] / 5 [5, 6] / 11 [10, 12] / 41 [39, 43], first takeover 7 [7, 8]; farm (novice) 3 [2, 3] / 16 [16, 17] / 19 [19, 20] / 27 [27, 28]; easy 2 [2, 2] / 10 [10, 11] / 13 [13, 14] / 24 [24, 24]; hard 5 [4, 5] / 15 [14, 15] / 18 [17, 18] / 27 [26, 27]; long equals expert-deu on every milestone, with no takeover in 150 runs in any seed. The rows are informational: none is a wave-2 acceptance, so `tests/pacing_targets.rs` does not sweep them. The one gate on them, G4.13 (`auto_sell_profile_within_one_run_of_default_at_tiers_1_to_3`, release), is wave-4 work (W4-l) and joins `pacing_targets` there.

W2-43 strategy pacing before tuning (the strategy rows on the harness, 2026-11-09) moved neither: the three legacy profiles keep the solo loop, and `pacing 60 --runs` (`target/production-after/w2-43/pacing.txt`) is byte-identical to `w2-42/pacing.txt` (sha256 0510b062...f2574879). The goal-3 table: `cargo run --locked --release -p horde_core --example pacing -- --strategy --seeds 20 --out target/production-before/strategy-pacing.txt` (sha256 85b7ace5...0b474ad6, not committed; about 8 minutes on 22 threads). Cells are lower median [p90] over 20 seeds. S1 (`strategy`, novice, gear first, Luxembourg, 120 runs): liberations 2 / 4 / 10 at runs 8 [8] / 18 [18] / 42 [45] and 34 / 84 / 206 minutes of battle and camp; 3,154 [3,212] coins per minute; 602 battle and 60 camp minutes; the Operations layer unlocks at run 18; 19 [23] outbreaks and no overrun, so 0 reopens over the 10 liberations after the unlock in every seed (G3.12 passes). S2a / S2b (Germany's 3-stage neighbours, stop at the fourth liberation): 9 [9] / 9 [9] runs to free 3 neighbours after Germany, a ratio of 1.00 against the 0.67 target (G3.11 fails); 684 / 773 coins per minute. Expeditions settle (S1 a median 367, S2b 18) and bring coins (16,185 and 1,367) but clear no stage in any seed: the bot sends recruits on their starter loadout, as the design specifies, and they lose every stage (the W2-42 takeover row saw the same). W4-c owns making G3.11 pass, through balance with evidence (design-campaign-defenders §7 risk 7), not through the harness.

W2-54 (the authored level grant and milestones, 2026-11-09) moved both, on purpose: the latest default output is `target/production-after/w2-54/pacing.txt` (sha256 902cd895...c7f7d738), and smoke's Personal stage 1 now takes three attempts. The rows, the smoke lines and the W2-54 20-seed acceptance are in §9.6, "W2-54 moved the default pacing output and smoke".

W2-03 goal-5 "before" (2026-11-07, post-W1-69 rules): `cargo test --locked --release -p horde_core --test pacing_targets -- --ignored --nocapture`, output in `target/production-before/pacing_targets_goal5_before.txt` (not committed; the sweep took about 62 s). Tier windows are the "about 4 / 8 / 12" target with one run either side (3-5, 7-9, 11-13). `goal5_first_victory_run_2_to_4` PASS: 3 [2, 5] 20/20. `goal5_tiers_affordable_3_5_7_9_11_13` FAIL: tier 1 3 [3, 3] passes; tier 2 5 [5, 6] and tier 3 7 [6, 7] are early. `goal5_tree_outlasts_second_liberation` PASS: Liberation(2) 17 [16, 18] 20/20, tree exhausted 25 [24, 26] 20/20, median 37 [33, 41] nodes bought after Liberation(2). `goal5_unlock_order_fortifications_mara_operations` FAIL: Fortifications 3 [2, 5], Mara 14 [13, 15], Operations open 14 [13, 15]. Every seed keeps Fortifications <= Mara <= Operations, but the Mara and Operations medians are equal. Both failures are wave-4 work (W4-p).

W2-04 goal-6 "before" (2026-11-07, post-W1-69 rules): the same command, now 11 tests (`--test-threads 1` for a stable print order), output in `target/production-before/pacing_targets_before.txt` (sha256 fa2f5b86...720be0, not committed; both sweeps took about 59 s). The goal-5 lines equal the W2-03 run. The defeat windows are aligned on each seed's own first victory, and a run that is not a victory counts as a defeat. `goal6_defeat_rate_windows` FAIL: 36 of the 39 pooled 20-run windows are outside 10-25%, lowest 0.0% at offset 8, highest 13.2% at offset 0. `goal6_no_15_streak_after_run_10` FAIL: every seed has a 46-50 win streak from run 11 (median 50). `goal6_boss_ttk_median_20_60_s` FAIL: 6.4 s [p25 5.9, p90 32.1] over 1,091 boss kills. `goal6_archetype_coverage` FAIL: 0 of the 11 catalogue archetypes are counted, because the per-archetype telemetry is populated only from W2-05. `goal6_lux_easy_at_most_6_runs` and `goal6_lux_hard_at_most_1_5x_normal` PASS: Liberation(1) Easy 4 [4, 4], Normal 6 [5, 7], Hard 8 [8, 8], all 20/20. `goal6_clamped_starts_first_victory_runs_2_to_4` PASS: AUT 3 [3, 5] and BGD 4 [2, 5], no run-1 victory in either. These starts are not clamped yet, because the clamp lands with the difficulty bands (ruling W-10). The four failures are wave-4 work (W4-04..n), apart from coverage, which W2-05 unblocks.

W2-49 (the wave-2 re-bless, 2026-11-10) fills the "After W2" column. It moved neither pacing nor smoke. The default `pacing 60 --runs` and `--smoke` outputs in `target/production-after-wave2/` are byte-identical to W2-54's. The 20-seed release acceptance (`target/production-after-wave2/pacing_targets.txt`, 294 s, not committed) equals `w2-54/pacing_targets.txt` apart from its timing line: 8 pass, 6 fail. The cells are the 20-seed lower median [p25, p75] unless stated otherwise. The wave-2 before/after rows are in §9.6, "Wave 2 before/after, at the W2-49 re-bless".

### 9.6 Enemies overlap (W1-69): pacing and smoke

User decision of 2026-11-06 (§2.2). Pacing: `cargo run --locked --release -p horde_core --example pacing -- 60 --runs`, on the parent `63168c1` (before) and on W1-69 (after), 60 runs per profile, as runs / battle minutes.

| Milestone | expert, tree first, DEU (before → after) | novice, tree first, DEU | novice, gear first, LUX |
|---|---|---|---|
| First victory | 4 / 13 → 3 / 9 | 3 / 9 → 5 / 17 | 2 / 6 → 3 / 10 |
| First liberation | 14 / 60 → 14 / 58 | 14 / 59 → 17 / 69 | 5 / 18 → 6 / 22 |
| Tier 2 affordable | 5 / 17 → 5 / 17 | 5 / 17 → 6 / 20 | 5 / 18 → 5 / 18 |
| Tier 3 affordable | 7 / 25 → 7 / 25 | 6 / 21 → 8 / 28 | 6 / 21 → 6 / 22 |
| Tier 4 affordable | 7 / 25 → 7 / 25 | 7 / 25 → 9 / 32 | 7 / 26 → 7 / 25 |
| Parallel defenses unlocked | 17 / 72 → 17 / 71 | 17 / 72 → 20 / 82 | 8 / 30 → 9 / 34 |
| Defeats in 60 runs (runs) | 5 (1, 2, 3, 8, 10) → 5 (1, 2, 4, 8, 10) | 5 (1, 2, 4, 7, 8) → 8 (1, 2, 3, 4, 6, 7, 11, 12) | 2 (1, 3) → 3 (1, 2, 5) |
| Coins over 60 runs | 269,900 → 269,927 | 269,546 → 248,312 | 306,599 → 299,150 |

Runs are harder, as expected: every enemy at a wall or the line now attacks, instead of one per lane with the rest queued behind it (59 enemies in contact against 19 at tick 420 of the progressed-peak fixture). The novice profile takes the shift: 3 more defeats, 2 of them after run 10, and its first victory moves from run 3 to run 5. The expert profile is unchanged within noise. Nothing is retuned in W1-69. This is the input for wave 2's pacing work (W2-01 baseline, W2-49 re-bless).

Wave 2's baseline is the post-W1-69 output, not wave 0's. The W1-69 run is saved byte-for-byte (UTF-8, no BOM, LF) as `target/production-after/w1-69/pacing.txt`, and its smoke stdout as `target/production-after/w1-69/smoke.txt` (not committed, like every `target/` file). Wherever a wave-2 design or pin says the default pacing output "reproduces pacing.txt" or smoke "equals smoke.txt" (W2-01's pin in `waves.md`, the "Integration rulings" of the remit designs, design-campaign-defenders §8), it means these two files (for pacing until R28, 2026-11-09: from W2-08 on, the latest recorded default output, `target/production-after/w2-41/pacing.txt`). `target/production-before/pacing.txt` and `smoke.txt` stay the wave-0 record of §1.2 and §1.3.

Smoke (`cargo run --locked -p horde-defense -- --smoke`): PASS before and after. Before is wave 0 §1.3; gameplay did not move between wave 0 and W1-68 (GOLDEN and TRACE 0/6), and it was not re-run on `63168c1`. Failed attempts: Personal 2 → 1 (stage 1 victory at attempt 2 instead of 3), Npc 1 → 3 (stage 1 victory at attempt 3 instead of 2, plus one stage-2 defeat). The legacy autonomous run is still a Defeat: 33 kills, 110 coins in 119.0 s before; 49 kills, 163 coins in 136.0 s after. Personal stage 5 is a Victory in 253.8 s with 284 kills and 2,653 coins (254.2 s, 284 kills, 2,741 before). Npc stage 5 is a Victory in 256.0 s with 150 kills and 1,564 coins (250.0 s, 146 kills, 1,615 before).

**W2-08 moved the default pacing output (W2-08 review, 2026-11-08; ruling pending, §2.5; accepted by R28, 2026-11-09, which re-baselines on `w2-41/pacing.txt`).** design-loot-economy ("Digest moves") expected smoke and pacing to stay byte-identical across the loot series, and §5.2 chose `loot_only` over "All purchasable" for that reason. The premise was wrong for pacing. The default profiles never equip or sell drops, but `recommended_node` reads owned items ("Train for your owned ..."). W2-08's larger drop pool (18 instead of 12 tier-1 definitions) changes which item drops, so a different node is learned; expert run 2, for example, learns `smg_training` instead of `second_wind`. Smoke did not move. `cargo run --locked --release -p horde_core --example pacing -- 60 --runs --out target/production-after/w2-08/pacing.txt` at W2-08 (re-run on the review fixes, which change no pick) is saved as `target/production-after/w2-08/pacing.txt` (UTF-8, no BOM, LF; sha256 547f7b4e...cb2e4d3c; not committed). It no longer equals `target/production-after/w1-69/pacing.txt` (sha256 5bf2aec2...4f3fa0), so the W2-01 pin wording (waves.md) does not hold after W2-08. Runs / battle minutes, w1-69 → w2-08:

| Milestone | expert, tree first, DEU | novice, tree first, DEU | novice, gear first, LUX |
|---|---|---|---|
| First victory | 3 / 9 → 5 / 17 | 5 / 17 → 6 / 20 | 3 / 10 (unchanged) |
| First liberation | 14 / 58 → 16 / 65 | 17 / 69 (unchanged) | 6 / 22 (unchanged) |
| Tier 1 / 2 / 3 / 4 affordable | 3 / 5 / 7 / 7 → 3 / 6 / 8 / 9 | 3 / 6 / 8 / 9 → 3 / 6 / 9 / 10 | 3 / 5 / 6 / 7 (unchanged) |
| Fortifications / Mara / Recruitment unlocked | 3 / 14 / 14 → 5 / 16 / 16 | 5 / 17 / 17 → 6 / 17 / 17 | unchanged |
| Parallel defenses / Country operations unlocked | 17 / 23 → 19 / 25 | 20 / 26 (runs unchanged; operations at 108 → 107 battle min) | unchanged |
| Defeats in 60 runs (runs) | 5 (1, 2, 4, 8, 10) → 7 (1, 2, 3, 4, 6, 7, 11) | 8 (1, 2, 3, 4, 6, 7, 11, 12) → 8 (1, 2, 3, 4, 5, 7, 13, 14) | 3 (1, 2, 5) (unchanged) |

The W2-08 commit reports that the 20-seed release acceptance (`pacing_targets --ignored`) keeps the W2-01 medians: first victory 3 [2, 5], tiers 3 / 5 / 7, Liberation(2) 17 [16, 18], tree exhausted 25 [24, 26]. Only the one-seed default output moved.

**W2-26 moved the default pacing output (W2-26 review, 2026-11-08).** The cause is balance on purpose: the in-country front ramp (design-enemy-balance §3, ruling W-5). A country stage at depth `stage/(count - 1)` multiplies enemy health and arrival by `1 + 0.18 x front_ramp_pct/100 x depth` (Normal 1.18 on a final stage), so later stages field more and tougher enemies and pay more coins. The boss damage rank term also starts at rank 5. Smoke did not move (`target/production-after/w2-26/smoke.txt` equals `smoke_before.txt`). `cargo run --locked --release -p horde_core --example pacing -- 60 --runs` on the parent `f8812aa` reproduces `target/production-after/w2-08/pacing.txt` byte for byte (saved again as `w2-26/pacing_before.txt`). At W2-26 the output is saved as `target/production-after/w2-26/pacing.txt` (UTF-8, no BOM, LF; sha256 2849040d...59b68bcb; not committed). It is the latest default output and replaces `w2-08/pacing.txt` for the next wave-2 diff. Runs / battle minutes, w2-08 → w2-26:

| Milestone | expert, tree first, DEU | novice, tree first, DEU | novice, gear first, LUX |
|---|---|---|---|
| First victory | 5 / 17 (unchanged) | 6 / 20 (unchanged) | 3 / 10 (unchanged) |
| First liberation | 16 / 65 → 17 / 70 | 17 / 69 → 17 / 67 | 6 / 22 → 6 / 21 |
| Tier 2 / 3 affordable | unchanged | unchanged | 5 / 18 → 5 / 17, 6 / 22 → 6 / 21 |
| Tier 4 first node | unchanged | unchanged | 5 / 18 → 8 / 29 |
| Mara / Recruitment unlocked | 16 / 65 → 17 / 70 | 17 / 69 → 17 / 67 | Mara 6 / 22 → 6 / 21, Recruitment 7 / 25 → 6 / 21 |
| Parallel defenses unlocked | 19 / 78 → 20 / 83 | 20 / 82 → 20 / 80 | 9 / 34 → 9 / 33 |
| Country operations unlocked | 25 / 103 → 26 / 108 | 26 / 107 → 26 / 105 | 19 / 76 (unchanged) |
| Defeats in 60 runs (runs) | 7 (1, 2, 3, 4, 6, 7, 11) → 8 (1, 2, 3, 4, 6, 7, 11, 12) | 8 (1, 2, 3, 4, 5, 7, 13, 14) → 8 (1, 2, 3, 4, 5, 7, 10, 11) | 3 (1, 2, 5) → 3 (1, 2, 4) |
| Coins over 60 runs | 253,389 → 271,241 | 247,914 → 268,254 | 299,150 → 325,826 |

The country-stage render fixtures move with it. `peak_render_fixture` and `maximum_progression_render_fixture` deploy stage 4 of their active country on Normal, so their encounter now carries a front ramp of `1 + 0.18 x 4/(count - 1)` (about 1.031 on peak's 24-stage USA front) on enemy health and arrival. The practice scenes built on them (peak, progressed-peak, max-lanes) and the stress and save-clone fixtures (`deploy_country_stage(stage, ..)`) move the same way. Captures and perf re-gates compared against pre-W2-26 baselines include this move; `render_fixtures_carry_their_stage_front_ramp` (tests/two_axis_threat.rs) pins it. GOLDEN deploys without a country and does not move.

W2-26 20-seed release acceptance (2026-11-08): `cargo test --locked --release -p horde_core --test pacing_targets -- --ignored --nocapture --test-threads 1`, run on the parent `f8812aa` (exported tree, own target directory) and at W2-26 (`target/production-after/w2-26/pacing_targets.txt`, not committed). Parent → W2-26:
- `goal6_lux_easy_at_most_6_runs` PASS → PASS: Liberation(1) Easy 4 [4, 4] → 4 [4, 4].
- `goal6_lux_hard_at_most_1_5x_normal` FAIL → PASS: Normal 5 [5, 6] → 6 [6, 7], Hard 8 [8, 8] → 9 [8, 11]. Hard is now exactly at the 1.5x limit (9 ≤ 9). This check passed at W2-04 (Normal 6, Hard 8) and already failed on the parent: a commit after W2-04 and up to `f8812aa` moved LUX Normal from 6 to 5, and that move is not recorded. W2-26 makes it pass again, with no margin.
- `goal6_clamped_starts_first_victory_runs_2_to_4` PASS → PASS: AUT 3 [3, 4] → 3 [3, 4], BGD 3 [2, 5] → 3 [2, 5], no run-1 victory.
- `goal6_archetype_coverage` PASS → PASS (11 of 11).
- Still failing, as at W2-04 (wave-4 work): `goal6_defeat_rate_windows` (36 → 34 of 39 windows outside 10-25%, highest 14.5% → 18.5% at offset 0), `goal6_no_15_streak_after_run_10` (every seed still has a 47-50 win streak from run 11), `goal6_boss_ttk_median_20_60_s` (6.4 s [p25 5.9, p90 32.8] → 7.3 s [p25 6.5, p90 30.4]).
- Goal 5: `goal5_first_victory_run_2_to_4` PASS, 3 [2, 5] unchanged. `goal5_tree_outlasts_second_liberation` PASS: Liberation(2) 17 [16, 18] → 18 [17, 19], tree exhausted 25 [24, 26] unchanged. `goal5_tiers_affordable_3_5_7_9_11_13` and `goal5_unlock_order_fortifications_mara_operations` still FAIL (tier 3 7 → 6; Mara and Operations both 14 → 15).

W4 tuning keeps `front_ramp_max` within [0.17, 0.25] (ruling W-5). Raising it widens the Hard/Normal gap, and the Hard limit now has no margin.

**W2-32 moved the default pacing output (2026-11-08).** The cause is balance on purpose: past rank 4 enemy health and damage grow on the late slopes (`growth_knee_rank` 4, `health_per_rank_late` 0.04, `damage_per_rank_late` 0.018), `pressure_floors` rows 5..=28 raise the floors and carry the composition from tier 4 to tier 11 (ranks 10 / 20 / 40 field tiers 7 / 10 / 11), and bosses gain armor with rank (Personal ×`1 + 0.02 × (min(r, 50) - 4)`, NPC `+ (r - 4) / 8`). Ranks 0-4 are unchanged (`ranks_zero_to_four_snapshot_pinned`), so GOLDEN, GAMEPLAY_TRACE, V1-V6 and smoke do not move (`target/production-after/w2-32/smoke.txt` equals `w2-26/smoke.txt`). `cargo run --locked --release -p horde_core --example pacing -- 60 --runs` on the parent `51bd4e3` (exported tree, own target directory) reproduces `w2-26/pacing.txt` byte for byte (saved again as `w2-32/pacing_before.txt`). At W2-32 the output is `target/production-after/w2-32/pacing.txt` (UTF-8, no BOM, LF; sha256 45e5b794...77f3c6da; not committed), now the latest default output. Every run before rank 5 is identical; later wins take longer and pay more:

| Milestone | expert, tree first, DEU | novice, tree first, DEU | novice, gear first, LUX |
|---|---|---|---|
| First victory, tiers 1-4, Fortifications | unchanged | unchanged | unchanged |
| First liberation, Mara, Recruitment | 17 / 70 → 17 / 71 | 17 / 67 → 17 / 68 | 6 / 21 (unchanged) |
| Parallel defenses unlocked | 20 / 83 → 20 / 84 | 20 / 80 → 20 / 81 | 9 / 33 (unchanged) |
| Country operations unlocked | 26 / 108 → 26 / 109 | 26 / 105 → 26 / 108 | 19 / 76 → 19 / 78 |
| Defeats in 60 runs (runs) | 8 (1, 2, 3, 4, 6, 7, 11, 12) (unchanged) | 8 (1, 2, 3, 4, 5, 7, 10, 11) (unchanged) | 3 (1, 2, 4) (unchanged) |
| Coins over 60 runs | 271,241 → 346,158 | 268,254 → 353,989 | 325,826 → 442,890 |
| Battle time over 60 runs | 14,849 s → 15,735 s | 14,813 s → 16,051 s | 14,797 s → 16,286 s |

Late victories now last about 255-278 s instead of 246-252 s against `boss_time` 240, because the boss takes longer to kill. The rank-50 render fixtures move with it: `maximum_progression_render_fixture` sets the campaign to `max_rank` (50), so progressed-peak, max-lanes and the stress and save-clone fixtures built on it now carry enemy health and damage on the late slope (×2.90 and ×1.86 instead of ×1.75 and ×1.40 before threat growth), the rank-28 floor row (tier 11 waves for the live director, boss floor 21,200 / NPC 115) and the rank boss armor (×1.92, NPC +5). `peak_render_fixture` stays at rank 4 and does not move. Captures and perf re-gates compared against pre-W2-32 baselines include this move.

W2-32 20-seed release acceptance (2026-11-08): `cargo test --locked --release -p horde_core --test pacing_targets -- --ignored --nocapture --test-threads 1`, on the parent `51bd4e3` (exported tree) and at W2-32 (`target/production-after/w2-32/pacing_targets_before.txt` and `pacing_targets.txt`, not committed). Parent → W2-32:
- `goal6_boss_ttk_median_20_60_s` FAIL → PASS: 7.3 s [p25 6.5, p90 30.4] → 29.4 s [p25 24.7, p90 37.6].
- `goal6_archetype_coverage` FAIL → PASS: 11 of 18 → 18 of 18 (the W2-31 archetypes in waves 5-11 were unreachable until these floors).
- Unchanged PASS: `goal5_first_victory_run_2_to_4` (3 [2, 5]), `goal5_tree_outlasts_second_liberation` (Liberation(2) 18 [17, 19], tree exhausted 25 [24, 26]; nodes bought after it 24 → 22), `goal6_clamped_starts_first_victory_runs_2_to_4`, `goal6_lux_easy_at_most_6_runs` and `goal6_lux_hard_at_most_1_5x_normal` (Easy 4, Normal 6, Hard 9: still no margin).
- Still failing (wave-4 work): `goal6_defeat_rate_windows` (34 of 39 windows outside 10-25% either way; highest 18.5% → 20.0% at offset 0), `goal6_no_15_streak_after_run_10` (every seed still has a 46-50 win streak from run 11), `goal5_tiers_affordable_3_5_7_9_11_13` and `goal5_unlock_order_fortifications_mara_operations` (unchanged).

**W2-33 moved the default pacing output and smoke (2026-11-08).** The cause is balance on purpose: elites (design-enemy-balance §5). From rank 3 a director wave spawn is an elite with chance `elite_bp(r)` (200 bp at rank 3, +65 a rank, 1500 from rank 23; Easy ×0.5, Hard ×1.5), rolled by a keyed hash that draws no combat RNG. An elite has health ×1.5 (NPC hit points too) and pays reward ×4 and xp ×3, plus one affix (hardened, frenzied, vicious, shielded or volatile). Ranks 0-2 are unchanged, so GOLDEN, GAMEPLAY_TRACE (both rank 0) and V1-V6 do not move. The measured elite fraction at rank 13 is 8.54% (1,730 of 20,250 eligible director spawns, `elite_fraction_at_rank_13_is_8_5_pm_0_5_over_20k`). The elite forced drop is not in this commit (W2-35), so the extra coins come only from the reward ×4. `cargo run --locked --release -p horde_core --example pacing -- 60 --runs` at W2-33 is `target/production-after/w2-33/pacing.txt` (UTF-8, no BOM, LF; sha256 774449b7...af118054; not committed); the parent's output is `w2-32/pacing.txt` (W2-32 is the parent). Every run before rank 3 is identical; later runs diverge as elites change the fights:

| Milestone | expert, tree first, DEU | novice, tree first, DEU | novice, gear first, LUX |
|---|---|---|---|
| First victory, tiers 1-3, Fortifications | unchanged | unchanged | unchanged |
| Tier 4 first node | unchanged | unchanged | 8 / 29 → 7 / 25 |
| First liberation, Mara, Recruitment | 17 / 71 → 15 / 60 | 17 / 68 → 20 / 82 | 6 / 21 (unchanged) |
| Parallel defenses unlocked | 20 / 84 → 18 / 74 | 20 / 81 → 23 / 95 | 9 / 33 (unchanged) |
| Country operations unlocked | 26 / 109 → 24 / 100 | 26 / 108 → 29 / 121 | 19 / 78 → 19 / 77 |
| Defeats in 60 runs (runs) | 8 (1, 2, 3, 4, 6, 7, 11, 12) → 6 (1, 2, 3, 4, 6, 7) | 8 (1, 2, 3, 4, 5, 7, 10, 11) → 11 (1, 2, 3, 4, 5, 7, 10, 11, 12, 13, 14) | 3 (1, 2, 4) (unchanged) |
| Coins over 60 runs | 346,158 → 500,174 | 353,989 → 439,013 | 442,890 → 617,281 |
| Battle time over 60 runs | 15,735 s → 15,706 s | 16,051 s → 15,959 s | 16,286 s → 16,389 s |

The expert profile's two stage-5 defeats (runs 11-12) became victories, so it liberates Germany two runs earlier; the novice profile stalls on stage 4 (rank 3, elites at 2%) for five defeats instead of two, so it liberates three runs later. The one-seed defaults swing both ways; the 20-seed medians below barely move.

Smoke (`cargo run --locked -p horde-defense -- --smoke`, PASS) moved only at stages 4-5 (ranks 3-4), as design-enemy-balance §14 predicts; every attempt count is unchanged. `target/production-after/w2-33/smoke.txt` against `w2-32/smoke.txt`: Personal stage 4 Victory 220.5 s, 225 kills, 1,972 → 1,995 coins; Personal stage 5 Victory 253.8 → 253.4 s, 284 → 282 kills, 2,653 → 2,941 coins; Npc stage 4 Victory 223.0 → 220.0 s, 118 → 122 kills, 1,223 → 1,345 coins; Npc stage 5 Victory 264.0 → 259.0 s, 138 → 163 kills, 1,502 → 1,728 coins.

The render fixtures stage their initial hostiles through `spawn_enemy`, which never rolls elite, so their first frame does not move. Their live directors do roll: peak (rank 4, 265 bp) and the rank-50 fixtures (1500 bp) can field elites once they tick, so motion captures and perf re-gates compared against pre-W2-33 baselines include this move.

W2-33 20-seed release acceptance (2026-11-08): `cargo test --locked --release -p horde_core --test pacing_targets -- --ignored --nocapture --test-threads 1` at W2-33 (`target/production-after/w2-33/pacing_targets.txt`, not committed) against the parent's `w2-32/pacing_targets.txt`. The PASS/FAIL set is unchanged:
- PASS, unchanged: `goal5_first_victory_run_2_to_4` (3 [2, 5]), `goal6_clamped_starts_first_victory_runs_2_to_4` (AUT 3 [3, 4], BGD 3 [2, 5]), `goal6_lux_easy_at_most_6_runs` and `goal6_lux_hard_at_most_1_5x_normal` (Easy 4, Normal 6, Hard 9: still no margin), `goal6_archetype_coverage` (18 of 18).
- PASS, moved: `goal5_tree_outlasts_second_liberation` (Liberation(2) 18 [17, 19] → 18 [17, 20], tree exhausted 25 [24, 26] → 25 [24, 27], nodes bought after it 22 → 16), `goal6_boss_ttk_median_20_60_s` (29.4 s [p25 24.7, p90 37.6] over 1,071 kills → 29.4 s [p25 24.2, p90 37.6] over 1,075).
- Still failing (wave-4 work): `goal6_defeat_rate_windows` (34 of 39 windows outside 10-25%; highest 20.0% → 19.0% at offset 0), `goal6_no_15_streak_after_run_10` (every seed still has a 46-50 win streak from run 11), `goal5_tiers_affordable_3_5_7_9_11_13` (unchanged) and `goal5_unlock_order_fortifications_mara_operations` (Mara and Operations both 15 [14, 16] → 15 [14, 17]).

**W2-35 moved neither pacing nor smoke (2026-11-08).** The elite forced drop (ruling E8) adds one item per elite kill on the `drop_tier` Some path, picked by the keyed ELITE_PICK hash when only the elite term fires, so the combat RNG draws exactly what it drew before and the fights do not change. The pacing profiles never equip recovered loot (`equip_drops` is false), so `cargo run --locked --release -p horde_core --example pacing -- 60 --runs` at W2-35 (`target/production-after/w2-35/pacing.txt`, not committed) is byte-identical to `w2-33/pacing.txt` (sha256 774449b7...af118054), and `--smoke` (`w2-35/smoke.txt`) equals `w2-33/smoke.txt`. W2-34 touched only horde_game, so W2-33's evidence is the parent's. The stash growth from forced elite drops (design-loot-economy "Risks" item 3) is measured in wave 4.

**W2-36 moved the default pacing output (2026-11-08).** The cause is balance on purpose: boss phases (design-enemy-balance §6). Past rank 4 every boss may enter one phase transition (Hard two, a Capstone one more), at 50% and then 25% of its hit points: it holds for 90 ticks (no movement, no attack countdown), then fights on with its kind's profile (Breacher speed ×1.4 and armor ×0.6, Caller call interval ×0.6, Siege Brute recovery ×0.6 and speed ×1.2). Ranks 0-4 off a Capstone are unchanged, so GOLDEN, GAMEPLAY_TRACE (rank 0, no country), V1-V6 and smoke do not move: `--smoke` at W2-36 (`target/production-after/w2-36/smoke.txt`, PASS) is byte-identical to `w2-35/smoke.txt`. `cargo run --locked --release -p horde_core --example pacing -- 60 --runs` at W2-36 is `target/production-after/w2-36/pacing.txt` (UTF-8, no BOM, LF; sha256 f8000150...017da5f2; not committed); the parent's output is `w2-35/pacing.txt` (= `w2-33/pacing.txt`). Every run before rank 5 is identical; the expert profile's first diverging run is run 12 (DEU stage 6, rank 5):

| Milestone | expert, tree first, DEU | novice, tree first, DEU | novice, gear first, LUX |
|---|---|---|---|
| First victory, tiers 1-4, Fortifications | unchanged | unchanged | unchanged |
| First liberation, Mara, Recruitment | 15 / 60 → 16 / 65 | 20 / 82 (unchanged) | 6 / 21 (unchanged) |
| Parallel defenses unlocked | 18 / 74 → 19 / 79 | 23 / 95 → 23 / 96 | 9 / 33 (unchanged) |
| Country operations unlocked | 24 / 100 → 25 / 105 | 29 / 121 → 29 / 123 | 19 / 77 (unchanged) |
| Defeats in 60 runs (runs) | 6 (1, 2, 3, 4, 6, 7) → 7 (1, 2, 3, 4, 6, 7, 13) | 11 (1, 2, 3, 4, 5, 7, 10, 11, 12, 13, 14) (unchanged) | 3 (1, 2, 4) (unchanged) |
| Coins over 60 runs | 500,174 → 485,911 | 439,013 → 440,296 | 617,281 → 610,351 |
| Battle time over 60 runs | 15,706 s → 15,721 s | 15,959 s → 16,041 s | 16,389 s → 16,330 s |

The expert profile loses DEU stage 7 once (run 13, 307 s) to a phased boss and liberates Germany one run later. The rank-50 render fixtures (`maximum_progression_render_fixture` and the stress and save-clone fixtures built on it) now spawn their boss with at least one phase transition, so their boss carries `boss_state` (no transition entered until it is hurt); `peak_render_fixture` (rank 4, a Sector stage) does not move.

W2-36 20-seed release acceptance (2026-11-08): `cargo test --locked --release -p horde_core --test pacing_targets -- --ignored --nocapture --test-threads 1` at W2-36 (`target/production-after/w2-36/pacing_targets.txt`, not committed) against `w2-33/pacing_targets.txt` (W2-35 moved no pacing). The PASS/FAIL set is unchanged:
- PASS, unchanged: `goal5_first_victory_run_2_to_4` (3 [2, 5]), `goal6_clamped_starts_first_victory_runs_2_to_4` (AUT 3 [3, 4], BGD 3 [2, 5]), `goal6_lux_easy_at_most_6_runs` and `goal6_lux_hard_at_most_1_5x_normal` (Easy 4, Normal 6, Hard 9: still no margin), `goal6_archetype_coverage` (18 of 18).
- PASS, moved: `goal5_tree_outlasts_second_liberation` (Liberation(2) 18 [17, 20] → 18 [16, 19], tree exhausted 25 [24, 27] → 25 [23, 26], nodes bought after it 16 → 18), `goal6_boss_ttk_median_20_60_s` (29.4 s [p25 24.2, p90 37.6] → 29.2 s [p25 24.1, p90 37.1], both over 1,075 kills).
- Still failing (wave-4 work): `goal6_defeat_rate_windows` (34 of 39 windows outside 10-25%; highest 19.0% at offset 0, unchanged), `goal6_no_15_streak_after_run_10` (every seed still has a 46-50 win streak from run 11), `goal5_tiers_affordable_3_5_7_9_11_13` (unchanged) and `goal5_unlock_order_fortifications_mara_operations` (Mara and Operations both 15 [14, 17] → 15 [13, 16]).

**W2-39 moved the default pacing output and smoke (2026-11-08).** The cause is content on purpose: the country difficulty bands and the start clamp (design-enemy-balance §8, ruling W-10). `scripts/generate_world.py` now sets each country's difficulty to `(lo + fnv1a64("difficulty:" + id) % (hi - lo + 1)) / 100` in the band of its stage count (≤ 3 stages 85-115, 4-9 100-135, ≥ 10 120-160): the 242 countries span 0.85-1.60 with 50 distinct values and mean 1.049, and 235 of them changed. A deployment's enemy health and reward multiply by the difficulty and enemy speed by its square root, and the campaign's start country now plays it clamped to [0.95, 1.05] (`Country::effective_difficulty`). So Germany, the expert and novice start, plays 1.04 instead of 1.06, and Luxembourg, the gear-first start, plays the clamped 0.95 (raw 0.92) instead of 1.03; Austria, the first front the expert opens, is 0.85 (was 1.03). GOLDEN, GAMEPLAY_TRACE (no country) and V1-V6 do not move, and `PROVISIONAL` stays None. Smoke's seeded campaign lines are unchanged, but its legacy autonomous run (a Germany start) moves from "Defeat: 49 kills, 163 coins in 136.0s" to "Defeat: 70 kills, 240 coins in 174.0s" (`target/production-after/w2-39/smoke.txt`, PASS, sha256 c47b083a...eb8969ae, against `w2-36/smoke.txt`). The v6 equipped-sale recording's replay pin moves from fa528d88f6639802 to 711acf17d72357c2 (VALIDATION.md, "Strict sale and ruleset 2"). `cargo run --locked --release -p horde_core --example pacing -- 60 --runs` at W2-39 is `target/production-after/w2-39/pacing.txt` (UTF-8, no BOM, LF; sha256 f8856265...7918d5a0; not committed); the parent's output is `w2-36/pacing.txt` (W2-37 moved no pacing). Every profile diverges from run 1:

| Milestone | expert, tree first, DEU | novice, tree first, DEU | novice, gear first, LUX |
|---|---|---|---|
| First victory, Fortifications | 5 / 17 → 2 / 6 | 6 / 20 (unchanged) | 3 / 10 → 2 / 7 |
| Tiers 1 / 2 / 3 / 4 affordable (runs) | 3 / 6 / 8 / 9 → 3 / 4 / 6 / 6 | 3 / 6 / 9 / 10 → 3 / 6 / 8 / 9 | 3 / 5 / 6 / 7 → 2 / 4 / 6 / 6 |
| First liberation, Mara, Recruitment | 16 / 65 → 15 / 63 | 20 / 82 → 16 / 67 | 6 / 21 → 5 / 18 |
| Parallel defenses unlocked | 19 / 79 → 18 / 76 | 23 / 96 → 19 / 80 | 9 / 33 → 8 / 31 |
| Country operations unlocked | 25 / 105 → 24 / 102 | 29 / 123 → 25 / 107 | 19 / 77 → 20 / 83 |
| Defeats in 60 runs (runs) | 7 (1, 2, 3, 4, 6, 7, 13) → 6 (1, 3, 6, 7, 9, 10) | 11 (1, 2, 3, 4, 5, 7, 10, 11, 12, 13, 14) → 7 (1, 2, 3, 4, 5, 11, 12) | 3 (1, 2, 4) → 2 (1, 3) |
| Coins over 60 runs | 485,911 → 489,912 | 440,296 → 489,542 | 610,351 → 561,289 |
| Battle time over 60 runs | 15,721 s → 15,806 s | 16,041 s → 16,260 s | 16,330 s → 16,303 s |

The render fixtures start in the USA (24 stages, raw 1.27), so they now deploy at the clamped 1.05 instead of 1.02; `field_view_reads_poses_and_blended_positions_without_id_lookups` therefore runs its progressed peak until a body collapses instead of for a fixed 120 ticks.

W2-39 20-seed release acceptance (2026-11-08): `cargo test --locked --release -p horde_core --test pacing_targets -- --ignored --nocapture --test-threads 1` at W2-39 (`target/production-after/w2-39/pacing_targets.txt`, sha256 acda123b...1ff743, not committed) against `w2-36/pacing_targets.txt`. One row changes from PASS to FAIL:
- PASS, unchanged: `goal5_first_victory_run_2_to_4` (3 [2, 5]), `goal6_lux_easy_at_most_6_runs` (Easy 4 [4, 4]), `goal6_archetype_coverage` (18 of 18).
- PASS, moved: `goal6_clamped_starts_first_victory_runs_2_to_4`, now on clamped starts (AUT 3 [3, 4] → 3 [2, 5], BGD 3 [2, 5] → 3 [2, 4], no run-1 victory in either), `goal5_tree_outlasts_second_liberation` (Liberation(2) 18 [16, 19] → 18 [16, 18], tree exhausted 25 [23, 26] → 25 [23, 25], nodes bought after it 18 → 21), `goal6_boss_ttk_median_20_60_s` (29.2 s [p25 24.1, p90 37.1] over 1,075 kills → 29.2 s [p25 23.1, p90 38.1] over 1,085).
- PASS → FAIL: `goal6_lux_hard_at_most_1_5x_normal`. The clamp makes the Luxembourg start easier on every preset (0.95 instead of 1.03), and Normal gains a run where Hard does not: Liberation(1) Normal 6 [6, 7] → 5 [5, 6], Hard 9 [8, 11] → 9 [9, 10], so Hard takes 1.8× Normal's runs against the 1.5× bound (W2-36 already noted no margin). Hard's ramp, elite rate and phases are wave-4 balance work (W4-04..n).
- Still failing (wave-4 work): `goal6_defeat_rate_windows` (34 → 35 of 39 windows outside 10-25%; highest 19.0% → 16.8% at offset 0), `goal6_no_15_streak_after_run_10` (every seed still has a 45-50 win streak from run 11), `goal5_tiers_affordable_3_5_7_9_11_13` (unchanged) and `goal5_unlock_order_fortifications_mara_operations` (Mara and Operations both 15 [13, 16] → 15 [13, 15]).

**W2-41 moved the default pacing output (W2-41 review, 2026-11-08).** The cause is content on purpose: the eight decision-19 nodes (ids 170-177, design-progression-tree §5). The W2-41 commit body said pacing need not be re-run because "the new nodes only add milestone-gated purchases, which tier_cost excludes"; that is wrong. `tier_cost` (`sim/pacing.rs`) sums every node at depth <= d with no gate filter; the exclusion of `minimum_victories >= 5` nodes is design-progression-tree §4's wave-4 tier definition (W4-p) and has not landed. The new nodes sit at depths 2 (`pistol_double_tap`, `smg_spray_control`), 3 (`shotgun_pump_drill`, `shield_breaker`, `blast_discipline`, `rifle_breath_control`) and 4 (`marksman_steady_aim`, `mg_belt_feeder`), so tier 1 stays 930 coins, tier 2 goes from 2,790 to 5,190, tier 3 from 4,975 to 11,675 and tier 4 from 6,655 to 15,655, and the tree is exhausted only with 177 nodes. `learn_recommended` also buys the new nodes once a run has 5 committed victories, which changes the fights after that. GOLDEN, GAMEPLAY_TRACE and V1-V6 do not move, and `PROVISIONAL` stays None. Smoke does not move: `--smoke` at W2-41 (`target/production-after/w2-41/smoke.txt`, PASS) is byte-identical to `w2-39/smoke.txt`. `cargo run --locked --release -p horde_core --example pacing -- 60 --runs --out target/production-after/w2-41/pacing.txt` at W2-41 is saved as `target/production-after/w2-41/pacing.txt` (UTF-8, no BOM, LF; sha256 e19f0669...d6bbe65c; not committed); the parent's output is `w2-39/pacing.txt` (W2-40 added only tests). The first diverging run is the first that buys a decision-19 node: expert run 12 (`shotgun_pump_drill`, `marksman_steady_aim`, `shield_breaker`), novice DEU run 13 and novice LUX run 13.

| Milestone | expert, tree first, DEU | novice, tree first, DEU | novice, gear first, LUX |
|---|---|---|---|
| First victory, Fortifications, tier 1 | unchanged | unchanged | unchanged |
| Tier 2 affordable (2,790 → 5,190 coins) | 4 / 13 → 6 / 22 | 6 / 20 → 8 / 28 | 4 / 14 → 6 / 22 |
| Tier 3 affordable (4,975 → 11,675 coins) | 6 / 22 → 8 / 31 | 8 / 28 → 11 / 42 | 6 / 22 → 8 / 31 |
| Tier 4 affordable (6,655 → 15,655 coins) | 6 / 22 → 10 / 41 | 9 / 32 → 12 / 47 | 6 / 22 → 9 / 35 |
| First liberation, Mara, Recruitment | 15 / 63 → 15 / 64 | 16 / 67 → 17 / 71 | 5 / 18 (unchanged) |
| Parallel defenses unlocked | 18 / 76 → 18 / 77 | 19 / 80 → 20 / 85 | 8 / 31 (unchanged) |
| Country operations unlocked | 24 / 102 → 24 / 103 | 25 / 107 → 26 / 111 | 20 / 83 → 20 / 84 |
| Defeats in 60 runs (runs) | 6 (1, 3, 6, 7, 9, 10) (unchanged) | 7 (1, 2, 3, 4, 5, 11, 12) → 8 (1, 2, 3, 4, 5, 11, 12, 14) | 2 (1, 3) (unchanged) |
| Coins over 60 runs | 489,912 → 488,663 | 489,542 → 469,197 | 561,289 → 548,198 |
| Battle time over 60 runs | 15,806 s → 15,697 s | 16,260 s → 15,998 s | 16,303 s → 16,041 s |

W2-41 20-seed release acceptance (W2-41 review, 2026-11-08): `cargo test --locked --release -p horde_core --test pacing_targets -- --ignored --nocapture --test-threads 1` at W2-41 (`target/production-after/w2-41/pacing_targets.txt`, sha256 de95aeb7...9e3c1bce, not committed) against `w2-39/pacing_targets.txt`. The PASS/FAIL set is unchanged:
- PASS, unchanged: `goal5_first_victory_run_2_to_4` (3 [2, 5]), `goal6_clamped_starts_first_victory_runs_2_to_4` (AUT 3 [2, 5], BGD 3 [2, 4]), `goal6_lux_easy_at_most_6_runs` (Easy 4 [4, 4]).
- PASS, moved: `goal5_tree_outlasts_second_liberation` (Liberation(2) 18 [16, 18] → 18 [16, 20], tree exhausted 25 [23, 25] → 27 [25, 29], nodes bought after it 21 → 31), `goal6_boss_ttk_median_20_60_s` (29.2 s [p25 23.1, p90 38.1] over 1,085 kills → 25.9 s [p25 21.9, p90 38.2] over 1,075), `goal6_archetype_coverage` (18 of 18, counts moved).
- Still failing, moved: `goal5_tiers_affordable_3_5_7_9_11_13` (tier 2 5 [4, 6] → 7 [6, 7], now inside its 7-9 window; tier 3 6 [6, 7] → 9 [8, 10], still before 11-13; both only because the tier costs grew, see above), `goal5_unlock_order_fortifications_mara_operations` (Mara and Operations both 15 [13, 15] → 15 [13, 17]), `goal6_defeat_rate_windows` (35 → 34 of 39 windows outside 10-25%; highest 16.8% → 19.2% at offset 0), `goal6_no_15_streak_after_run_10` (every seed still has a 43-50 win streak from run 11).
- Still failing, unchanged: `goal6_lux_hard_at_most_1_5x_normal` (Normal 5 [5, 6], Hard 9 [9, 10]).

The tier rows are a measurement move, not a pacing gain: once W4-p lands design-progression-tree §4's tier definition (milestone-gated nodes out of `tier_cost`), tiers 1-3 return to 930 / 2,790 / 4,975 coins.

**W2-09 moved the default pacing output (2026-11-09).** The cause is the weighted single-draw DropTable on purpose (plan LE-5, design-loot-economy §3): a `drop_tier` Some pick now weighs each pool entry by its drop-tier step (6 / 3 / 1 / 1) and a trained Ranged class x3, with the same one combat-RNG draw. The default profiles never equip or sell drops, but `recommended_node` reads owned items, so where a different item drops a node is learned one run earlier or later. GAMEPLAY_TRACE, V1-V6 and smoke do not move: `--smoke` at W2-09 (`target/production-after/w2-09/smoke.txt`, PASS) is byte-identical to `w2-41/smoke.txt`. GOLDEN moves only for 0x1a571167 Personal (provisional, cause G). Two test pins move with it, each restored by the neutral weights: `MODE_OWNER_DIGEST` (`mode_owner_paths_are_byte_identical`) from 17352768058746973511 to 9781996419595183472, and the v6 equipped-sale recording's replay pin from 711acf17d72357c2 to 0e558e53029c6c41 (VALIDATION.md, "Strict sale and ruleset 2"). `cargo run --locked --release -p horde_core --example pacing -- 60 --runs --out target/production-after/w2-09/pacing.txt` at W2-09 is saved as `target/production-after/w2-09/pacing.txt` (UTF-8, no BOM, LF; sha256 0510b062...f2574879; not committed). The parent's output is the R28 baseline `w2-41/pacing.txt`: the same build with `DROP_TIER_WEIGHTS` and `TRAINED_CLASS_WEIGHT` set to 1 reproduces it byte for byte (`w2-09/pacing_neutral_probe.txt`). Only the learned-node lists of seven runs move. No outcome, coins, battle time or milestone moves, so the milestone table is unchanged:

| Run | w2-41 learned | w2-09 learned |
|---|---|---|
| expert DEU run 2 | ... `grenade_training`, `smg_training` | ... `grenade_training`, `second_wind` |
| expert DEU run 3 | `second_wind`, `endurance_drill`, ... | `smg_training`, `endurance_drill`, ... |
| novice DEU run 3 | `endurance_drill`, ..., `shotgun_training`, `heavy_strike` | `smg_training`, `endurance_drill`, ..., `shotgun_training`, `body_vitality_1` |
| novice DEU run 4 | `rifle_training`, ..., `grenade_bandolier`, `bottle_carrier` | `heavy_strike`, `rifle_training`, ..., `grenade_bandolier` |
| novice DEU run 5 | `smg_training`, ..., `body_vitality_1`, `body_strength_1` | `bottle_carrier`, ..., `body_strength_1` |
| novice LUX run 3 | `smg_training`, `second_wind`, ..., `trap_blueprint` | `second_wind`, ..., `trap_blueprint`, `molotov_training` |
| novice LUX run 4 | `molotov_training`, `brace`, ... | `smg_training`, `brace`, ... |

W2-09 20-seed release acceptance (2026-11-09): `cargo test --locked --release -p horde_core --test pacing_targets -- --ignored --nocapture --test-threads 1` at W2-09 (`target/production-after/w2-09/pacing_targets.txt`, sha256 93f439a3...dcdec729, not committed) against `w2-41/pacing_targets.txt`. The neutral-weight build reproduces `w2-41/pacing_targets.txt` (finish times aside; `w2-09/pacing_targets_neutral_probe.txt`), so the drop weights cause every move:
- PASS → FAIL: `goal6_clamped_starts_first_victory_runs_2_to_4`. AUT start first victory 3 [2, 5] → 5 [2, 5] (lower median of 20 seeds; p25 and p75 unchanged); BGD 3 [2, 4] → 3 [2, 5] still passes. The start profiles' drops change which nodes they learn and so which early runs they win. The start difficulty and early pacing are wave-4 balance work (W4-04..n). R33 (2026-11-09): the wave-4 tuning loop W4-04..n owns this test, and W2-09's provisional cause stays G (`support::classify` labels it C; W2-49 records both).
- PASS, unchanged: `goal5_first_victory_run_2_to_4`, `goal6_lux_easy_at_most_6_runs` (Easy 4 [4, 4]).
- PASS, moved: `goal5_tree_outlasts_second_liberation` (Liberation(2) 18 [16, 20] → 18 [17, 19], tree exhausted 27 [25, 29] → 27 [26, 28], nodes bought after it 31 [23, 42] → 31 [24, 40]), `goal6_boss_ttk_median_20_60_s` (25.9 s [p25 21.9, p90 38.2] over 1,075 kills → 26.1 s [p25 22.2, p90 38.8] over 1,076), `goal6_archetype_coverage` (18 of 18, counts moved).
- Still failing, unchanged: `goal5_tiers_affordable_3_5_7_9_11_13`.
- Still failing, moved: `goal5_unlock_order_fortifications_mara_operations` (Mara and Operations 15 [13, 17] → 15 [14, 16]), `goal6_defeat_rate_windows` (34 of 39 windows outside 10-25% either way; highest 19.2% → 19.0% at offset 0), `goal6_lux_hard_at_most_1_5x_normal` (Hard 9 [9, 10] → 10 [9, 12], Normal 5 [5, 6]), `goal6_no_15_streak_after_run_10` (longest win streak from run 11: median 50 → 49).

**W2-54 moved the default pacing output and smoke (2026-11-09).** The cause is the authored rewards on purpose (decision 14, PT7, PT8; design-progression-tree §6-7, ruling E2, R32): `progression.json` now authors `rewards.level_grant_xp_fraction` 0.25 (5 coins at level 2, 77 over a run from level 1 to 6) and the four milestones (rank 5 500 coins and an Uncommon item, 10 1000 Uncommon, 15 1500 Rare, 20 2000 Rare), paid by W2-22's settlement at a run's finish. GAMEPLAY_TRACE, V1-V6 and `V6_DROP_TRACE` do not move; GOLDEN moves for the three Personal runs (provisional, class F, the W2-52 shim is inert once a grant is authored). Before is the last recorded output of each, on the same horde_core rules (no horde_core change since W2-43): pacing `w2-43/pacing.txt` (sha256 0510b062...f2574879) and smoke `w2-22/smoke.txt` (sha256 c47b083a...eb8969ae). `cargo run --locked --release -p horde_core --example pacing -- 60 --runs --out target/production-after/w2-54/pacing.txt` at W2-54 is saved as `target/production-after/w2-54/pacing.txt` (UTF-8, no BOM, LF; sha256 902cd895...c7f7d738; not committed), now the latest default output. Every profile's run 1 is identical; run 2 diverges in all three, because run 1's grant changes what the profile can buy. The extra coins bring tiers, the first liberation and the unlocks earlier. Runs / battle minutes, w2-43 → w2-54:

| Milestone | expert, tree first, DEU | novice, tree first, DEU | novice, gear first, LUX |
|---|---|---|---|
| First victory | 2 / 6 → 2 / 6 | 6 / 20 → 3 / 9 | 2 / 7 → 2 / 7 |
| First liberation | 15 / 64 → 14 / 62 | 17 / 71 → 15 / 60 | 5 / 18 → 5 / 18 |
| Tier 1 affordable (930 coins) | 3 / 10 → 2 / 6 | 3 / 9 → 3 / 9 | 2 / 7 → 2 / 7 |
| Tier 2 affordable (5190 coins) | 6 / 22 → 5 / 17 | 8 / 28 → 6 / 20 | 6 / 22 → 5 / 18 |
| Tier 3 affordable (11675 coins) | 8 / 31 → 7 / 27 | 11 / 42 → 9 / 34 | 8 / 31 → 8 / 30 |
| Tier 4 affordable (15655 coins) | 10 / 41 → 9 / 37 | 12 / 47 → 10 / 38 | 9 / 35 → 9 / 34 |
| Fortifications unlocked | 2 / 6 → 2 / 6 | 6 / 20 → 3 / 9 | 2 / 7 → 2 / 7 |
| Mara / Recruitment unlocked | 15 / 64 → 14 / 62 | 17 / 71 → 15 / 60 | 5 / 18 → 5 / 18 |
| Parallel defenses unlocked | 18 / 77 → 17 / 75 | 20 / 85 → 18 / 73 | 8 / 31 → 8 / 30 |
| Country operations unlocked | 24 / 103 → 23 / 100 | 26 / 111 → 24 / 100 | 20 / 84 → 20 / 83 |
| Non-victories in 60 runs (runs) | 6 (1, 3, 6, 7, 9, 10) → 5 (1, 3, 7, 8, 9) | 8 (1-5, 11, 12, 14) → 6 (1, 2, 4, 7, 8, 9) | 2 (1, 3) → 2 (1, 3) |
| Battle coins over 60 runs (report coins, grants and milestones excluded) | 488,663 → 505,870 | 469,197 → 496,870 | 548,198 → 547,785 |

Smoke (`cargo run --locked -p horde-defense -- --smoke`, stdout `target/production-after/w2-54/smoke.txt`, PASS, exit 0) moved. The legacy autonomous line is unchanged (Defeat: 70 kills, 240 coins in 174.0 s). Personal: stage 1 attempt 1 is unchanged (Defeat, 288 coins); attempt 2 is now a Defeat (209.5 s, 150 kills, 500 coins; was a Victory in 202.1 s, 150 kills, 628 coins) and attempt 3 the Victory (195.1 s, 149 kills, 615 coins), so failed Personal attempts go 1 → 2; stages 2-5 are Victories in 190.4 / 222.4 / 221.3 / 254.2 s with 168 / 212 / 230 / 286 kills and 979 / 1585 / 2128 / 2996 coins (was 194.5 / 222.2 / 220.5 / 253.4 s, 170 / 210 / 225 / 282 kills, 977 / 1643 / 1995 / 2941 coins). The settlement coins are the only change, so they change what the smoke bot buys before attempt 2. Npc stages 1-4 are unchanged; stage 5 is a Victory in 256.0 s with 152 kills and 1632 coins (was 259.0 s, 163 kills, 1728 coins).

W2-54 20-seed release acceptance (2026-11-09): `cargo test --locked --release -p horde_core --test pacing_targets -- --ignored --nocapture --test-threads 1` at W2-54 (`target/production-after/w2-54/pacing_targets.txt`, sha256 6b437ad1...eb383e7f8, not committed; 306 s with the strategy sweep). Before is `w2-09/pacing_targets.txt` for the goal-5 and goal-6 rows (the same rules) and the W2-43 table (§9.2) for the goal-3 rows. 8 pass, 6 fail:
- FAIL → PASS: `goal6_clamped_starts_first_victory_runs_2_to_4`. AUT start first victory 5 [2, 5] → 4 [2, 4]; BGD 3 [2, 5] → 3 [2, 4].
- PASS, moved: `goal5_first_victory_run_2_to_4` (3 [2, 5] → 3 [2, 4]), `goal5_tree_outlasts_second_liberation` (Liberation(2) 18 [17, 19] → 17 [16, 18], tree exhausted 27 [26, 28] → 26 [25, 27], nodes bought after it 31 [24, 40] → 20 [17, 27]), `goal6_boss_ttk_median_20_60_s` (26.1 s [p25 22.2, p90 38.8] over 1,076 kills → 25.5 s [21.5, 36.4] over 1,090), `goal6_lux_easy_at_most_6_runs` (Easy 4 [4, 4] → 4 [4, 5]), `strategy_profile_reports_liberations_2_4_10_and_coins_per_minute` (liberations 2 / 4 / 10 at runs 8 / 18 / 42 [45] → 8 / 18 / 41 [46], 34 / 84 / 206 → 34 / 83 / 203 minutes, 3,154 → 3,183.5 [3,289.5] coins per minute).
- PASS, unchanged: `goal6_archetype_coverage` (18/18), `fewer_than_five_outbreak_reopens_over_ten_liberations` (median 0).
- Still failing, moved: `goal5_tiers_affordable_3_5_7_9_11_13` (tier 1 3 [3, 3] → 3 [2, 3], tier 2 7 [6, 7] → 5 [5, 6], now early too, tier 3 9 [8, 10] → 8 [8, 9]; the grants and milestones bring every tier earlier, PT6 and the tier definition are W4-p's), `goal5_unlock_order_fortifications_mara_operations` (Fortifications 3 [2, 5] → 3 [2, 4], Mara and Operations 15 [14, 16] → 14 [13, 15]), `goal6_defeat_rate_windows` (34 → 35 of 39 windows outside 10-25%; highest 19.0% → 16.2% at offset 0), `goal6_lux_hard_at_most_1_5x_normal` (Hard 10 [9, 12] → 9 [8, 11] against Normal 5 [5, 6]), `goal6_no_15_streak_after_run_10` (longest win streak from run 11: median 49 → 50; every seed a 45-50 win streak).
- Still failing: `parallel_frees_three_stage_neighbours_within_two_thirds_of_solo_runs` (9 / 9, ratio 1.00).

**Wave 2 before/after, at the W2-49 re-bless (2026-11-10).** W2-49 bumps `RULESET_VERSION` to 3 and moves no rule. In `target/production-after-wave2/` (UTF-8, no BOM, LF; not committed):
- `cargo run --locked --release -p horde_core --example pacing -- 60 --runs --out target/production-after-wave2/pacing.txt` is byte-identical to `w2-54/pacing.txt` (sha256 902cd895...c7f7d738).
- `--smoke` (`smoke.txt`, PASS, exit 0) is byte-identical to `w2-54/smoke.txt` (sha256 846f0b4c...3425fe50).

So the whole wave-2 move is the sum of the commits recorded above. Before is the post-W1-69 baseline (`w1-69/pacing.txt` and `w1-69/smoke.txt`, the wave-1 close). Runs / battle minutes, w1-69 → wave 2:

| Milestone | expert, tree first, DEU | novice, tree first, DEU | novice, gear first, LUX |
|---|---|---|---|
| First victory | 3 / 9 → 2 / 6 | 5 / 17 → 3 / 9 | 3 / 10 → 2 / 7 |
| First liberation | 14 / 58 → 14 / 62 | 17 / 69 → 15 / 60 | 6 / 22 → 5 / 18 |
| Tier 1 affordable (930 coins) | 3 / 9 → 2 / 6 | 3 / 9 → 3 / 9 | 3 / 10 → 2 / 7 |
| Tier 2 affordable (2,790 → 5,190 coins, W2-41) | 5 / 17 → 5 / 17 | 6 / 20 → 6 / 20 | 5 / 18 → 5 / 18 |
| Tier 3 affordable (4,975 → 11,675 coins) | 7 / 25 → 7 / 27 | 8 / 28 → 9 / 34 | 6 / 22 → 8 / 30 |
| Tier 4 affordable (6,655 → 15,655 coins) | 7 / 25 → 9 / 37 | 9 / 32 → 10 / 38 | 7 / 25 → 9 / 34 |
| Fortifications unlocked | 3 / 9 → 2 / 6 | 5 / 17 → 3 / 9 | 3 / 10 → 2 / 7 |
| Mara / Recruitment unlocked | 14 / 58 → 14 / 62 | 17 / 69 → 15 / 60 | 6 / 22 and 7 / 25 → 5 / 18 |
| Parallel defenses unlocked | 17 / 71 → 17 / 75 | 20 / 82 → 18 / 73 | 9 / 34 → 8 / 30 |
| Country operations unlocked | 23 / 96 → 23 / 100 | 26 / 108 → 24 / 100 | 19 / 76 → 20 / 83 |
| Non-victories in 60 runs (runs) | 5 (1, 2, 4, 8, 10) → 5 (1, 3, 7, 8, 9) | 8 (1, 2, 3, 4, 6, 7, 11, 12) → 6 (1, 2, 4, 7, 8, 9) | 3 (1, 2, 5) → 2 (1, 3) |
| Battle coins over 60 runs (grants and milestones excluded) | 269,927 → 505,870 | 248,312 → 496,870 | 299,150 → 547,785 |
| Battle time over 60 runs | 14,850 s → 15,835 s | 14,924 s → 15,908 s | 14,772 s → 15,995 s |

Smoke, w1-69 → wave 2 (PASS both):
- The legacy autonomous run is still a Defeat: 49 kills, 163 coins in 136.0 s → 70 kills, 240 coins in 174.0 s (W2-39).
- Failed Personal attempts go 1 → 2, because stage 1 is now won at attempt 3 (W2-54). Personal stage 5 is a Victory: 253.8 s, 284 kills, 2,653 coins → 254.2 s, 286 kills, 2,996 coins.
- Failed Npc attempts stay at 3. Npc stage 4 goes 223.0 s, 118 kills, 1,223 coins → 220.0 s, 122 kills, 1,345 coins (W2-33), and stage 5 goes 256.0 s, 150 kills, 1,564 coins → 256.0 s, 152 kills, 1,632 coins.

The 20-seed release acceptance, post-W1-69 (W2-01, W2-03 and W2-04 "before" runs) → wave 2 (`target/production-after-wave2/pacing_targets.txt`):
- First victory: 3 [2, 5] → 3 [2, 4].
- Liberation(2): 17 [16, 18] → 17 [16, 18]. Tree exhausted: 25 [24, 26] → 26 [25, 27].
- Tiers 1 / 2 / 3 affordable: 3 / 5 / 7 → 3 / 5 / 8.
- LUX Easy / Normal / Hard: 4 / 6 / 8 → 4 / 5 / 9.
- Boss TTK: 6.4 s → 25.5 s.
- Archetype coverage: 0 of 11 → 18 of 18.
- AUT / BGD clamped starts: 3 / 4 → 4 / 3.

Still failing, all wave-4 work (W4-04..n, W4-p, W4-c): `goal5_tiers_affordable_3_5_7_9_11_13`, `goal5_unlock_order_fortifications_mara_operations`, `goal6_defeat_rate_windows`, `goal6_lux_hard_at_most_1_5x_normal`, `goal6_no_15_streak_after_run_10` and `parallel_frees_three_stage_neighbours_within_two_thirds_of_solo_runs`.

### 9.3 Audio

| Measure | Before (A6-04) | After (A6-17) | Gate |
|---|---|---|---|
| 60 s reference mix, integrated LUFS (both themes) | about -38..-42 | | -23 ±2 |
| 60 s dense battle true peak (FS) | | | ≤ 0.95 |
| Runtime master limiter needed (R3) | n/a | yes / no | only if TP > 0.95 |

### 9.4 Frame report v3 baseline (W1-37)

Recorded 2026-11-05/06 on `2339a3d`, before any BattleFrame, camera, batching or corpse commit: the wave-0 painters plus the W1-33 probe and the W1-34 render plan at its defaults (Fifo, pipelined, frame latency 1). Hardware as `PERF_BUDGET.json` (RTX 1000 Ada Laptop, Vulkan, 2880x1800 at OS scale 2.0); mains power, Windows power plan Balanced (not changed for the run). `measure_performance.ps1 -Scene all -Repeat 3` per profile and VSync state, the budget window (warmup 180, 600 / 600 / 1800 frames), every recorded report valid (version 3, sample target met, focused, requested present mode and profile echoed; one attempt, `dev-vsync-b` peak normal repeat 3, ended with the battle before its sample target and was rerun by the script's one-rerun rule). Values are medians of 3 repeats; frames > 50 ms is the worst repeat. Evidence: `target/perf/w1-37/<dir>` (not committed), each with summary.json and summary.md.

**Spans (W1 review, 2026-11-07).** Every value in this section was measured on the probe spans from before review commit `7abb0ab` (2026-11-07 02:30, after the W1-61 acceptance). Before it, egui_pass opened at the start of PostUpdate, so it held the PostUpdate systems unordered against `EndPass`. present_wait was not ordered after `create_surfaces`, so surface (re)configuration and the `PrepareViews` work before it could count as present wait instead of render-world CPU. From `7abb0ab` on, interface_total and cpu_frame are not like-for-like with these values or with baseline.v3. The `interface_total_not_above_baseline_v3` gate is therefore looser than at W1-61, and cpu_frame may now count work the W1-61 p99 values left out. Whether baseline.v3 is re-measured on the new spans is open (2.5). VALIDATION.md W1-61 "Span boundaries changed after this acceptance" has the details. **Answered by R29 and re-measured in W2-47 (below):** the table in this section stays the W1-37 record and `PERF_BUDGET.json baseline.v3.previous`.

**Re-measured on the 7abb0ab spans (W2-47, R29, 2026-11-09 19:59-22:00 UTC).** Same renderer, new spans: the `2339a3d` sources (`git archive`) with only the two 7abb0ab span changes applied (egui_pass opens in an exclusive system inside `EndPass`, directly before `run_egui_context_pass_loop_system`; present_wait opens after `create_surfaces`), built dev and playtest with the same CARGO_TARGET_DIR under `target/v3-remeasure/` (not committed). HEAD's `measure_performance.ps1 -Scene all -Repeat 3 -NoBuild -NoBudget` ran them per profile and VSync state (Fifo, pipelined, frame latency 1), power plan Balanced (not changed for the run), with the R27 display hold and the R30 quiet window and load check; budget window as above. Only run validity counts: the old reports have no driver-stall, texture-upload or battlefield fields, so each pass's own checks are invalid by design (exit 2). The machine was shared (another session's cargo builds and tests, user input): three scene groups had an invalid run after the rerun (dev VSync on peak, unfocused; dev VSync on stress and playtest VSync on stress, a window at 0x0 or CPU load samples of 90-100% during a foreign build) and were re-measured as whole groups, as W1-37 did (dev peak `-peak-b`, dev stress `-stress-b`, playtest stress `-stress-c`). Values are medians of 3 valid repeats. Evidence: `target/perf/w2-47/v3-spans-*`.

| Config | Scene | interface_total normal / small | W1-37 (old spans) | interface_draw normal / small | cpu_frame p99 normal / small |
|---|---|---|---|---|---|
| dev-vsync-on | peak | 2.525 / 2.330 | 2.548 / 2.383 | 1.207 / 1.072 | - |
| dev-vsync-on | progressed-peak | 4.822 / 4.708 | 4.645 / 4.590 | 1.710 / 1.537 | - |
| dev-vsync-on | stress | 6.526 / 6.057 | 6.969 / 6.525 | 2.067 / 1.771 | - |
| dev-vsync-off | peak | 2.425 / 2.180 | 2.634 / 2.324 | 1.131 / 0.960 | 12.9 / 15.1 |
| dev-vsync-off | progressed-peak | 4.819 / 4.267 | 4.739 / 4.270 | 1.688 / 1.385 | 19.2 / 19.6 |
| dev-vsync-off | stress | 6.803 / 6.164 | 6.608 / 6.237 | 2.136 / 1.780 | 29.0 / 35.5 |
| playtest-vsync-on | peak | 1.960 / 1.888 | 2.051 / 1.943 | 0.926 / 0.872 | - |
| playtest-vsync-on | progressed-peak | 3.491 / 3.415 | 3.601 / 3.493 | 1.280 / 1.213 | - |
| playtest-vsync-on | stress | 4.712 / 4.573 | 5.157 / 4.931 | 1.556 / 1.450 | - |
| playtest-vsync-off | peak | 1.912 / 1.705 | 1.927 / 1.744 | 0.882 / 0.738 | 12.1 / 15.5 |
| playtest-vsync-off | progressed-peak | 3.580 / 3.315 | 3.524 / 3.213 | 1.295 / 1.130 | 19.6 / 18.1 |
| playtest-vsync-off | stress | 5.065 / 4.689 | 5.023 / 4.695 | 1.640 / 1.478 | 25.2 / 32.1 |

- The narrower egui_pass span can only lower interface_total for the same work, and it did in 17 of 24 cells (by up to 8.6%, playtest VSync-on stress normal). The other 7 are 0.8-3.8% above W1-37, among them the gated progressed-peak dev VSync-on pair (4.822 / 4.708 against 4.645 / 4.590). So the day-to-day spread of this measurement on this machine is at least the size of the span change, which is what R31's A/B against the wave-1 executable answers.
- **R35 (2026-11-10, orchestrator ruling).** W2-47's R29 reading is confirmed: `baseline.v3` was re-measured on the 2339a3d renderer with only the 7abb0ab span edits, not at HEAD, and the gated progressed-peak pair kept its W1-37 values under R22 (next item).
- **Gate (R22).** `PERF_BUDGET.json baseline.v3` now holds the re-measure (`spans` 7abb0ab, `measured_interface_total_mean_ms` every median). The script gates only progressed-peak interface_total; both of its re-measured cells are above W1-37, so they would loosen the gate, and `interface_total_mean_ms` keeps the W1-37 4.6447 / 4.5895 there (`kept_previous`). The W1-37 baseline stays verbatim under `baseline.v3.previous`. `perf_budget_v3_baseline_is_re_measured_on_the_7abb0ab_spans` pins the layout and the rule.
- cpu_frame p99 (wider spans now) moved by -4.4 to +1.2 ms per cell against W1-37; it is informational in the baseline (the gate is the absolute 10.0 ms).
- Drift progressed-peak normal: the re-measured dev VSync-on interface_draw is 1.710, +6.0% over wave 0 (W1-37 1.655, +2.6%). Investigated: the span patch touches two probe systems outside `ui::draw`, so interface_draw (the `ui::draw` wall time) has the same span as at W1-37. The other re-measured progressed-peak normal cells move both ways against W1-37 (playtest VSync on 1.280 against 1.303, -1.8%; dev VSync off 1.688 against 1.663, +1.5%; playtest VSync off 1.295 against 1.214, +6.7%) on executables whose ui::draw is unchanged, and the dev VSync-on pass ran at the highest mean CPU load of the re-measure (41% over its runs, with user input and a foreign session active). It is recorded as run-to-run spread on a shared machine, not a draw change. The interface_draw gate stays 0.70x wave 0.

`PERF_BUDGET.json baseline.v3` holds the dev, VSync-on run (`dev-vsync-b`): `interface_total_mean_ms` is the anti-shift guard `measure_performance.ps1` gates progressed-peak on, `interface_draw_mean_ms` is the v2-compatible ui::draw mean. `baseline.v3.runs` records the other three profile and VSync states, informational. `baseline.v3` also records the conditions it was measured under (`present_mode` Fifo, `pipelined` true, `power_plan` Balanced; W1 review): the script gates only repeats that echo that present mode and pipelined state (any other is skipped, a repeat without its state is invalid evidence) and still gates a run under another power plan, with a `noted` `baseline_v3_conditions` check that names both plans. The interface_draw gate stays 0.70x wave 0 (≤ 1.129 / ≤ 1.030); nothing here moves it.

The machine was shared with other sessions during the runs, so the script's runs were wrapped in a 5 s `Win32_Processor.LoadPercentage` sampler (scratch only). A scene whose repeats overlapped a ≥ 90% load sample was re-measured as a whole group and the rerun recorded: dev VSync off progressed-peak (`dev-novsync` ran it under 100% load for 2 minutes; recorded from `dev-novsync-progressed-peak-b`) and playtest stress, VSync on and off (`playtest-vsync-stress-b`, `playtest-novsync-stress-b`). The first dev VSync-on pass (`dev-vsync`) ran before the sampler existed, so the baseline is the second pass `dev-vsync-b`; both are compared below.

| Config | Scene | Size | interface_draw | vs wave 0 | interface_total | egui_pass | tessellate | main_cpu | extract | render_schedule | present_wait | cpu_frame p99 | primitives | vertices | textured quads | GPU egui | frames > 50 ms (worst) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| dev-vsync-on | peak | normal | 1.215 | +2.1% | 2.548 | 1.489 | 1.059 | 4.169 | 1.055 | 22.983 | 19.677 | 20.3 | 149 | 49847 | 11804 | 1.774 | 12 |
| dev-vsync-on | peak | small | 1.084 | +0.8% | 2.383 | 1.382 | 1.005 | 4.150 | 1.319 | 20.081 | 16.272 | 31.9 | 149 | 41759 | 7524 | 1.182 | 10 |
| dev-vsync-on | progressed-peak | normal | 1.655 | +2.6% | 4.645 | 1.957 | 2.688 | 6.435 | 1.116 | 16.866 | 12.938 | 23.4 | 514 | 94402 | 14399 | 1.973 | 6 |
| dev-vsync-on | progressed-peak | small | 1.518 | +3.1% | 4.589 | 1.849 | 2.740 | 6.548 | 1.352 | 19.149 | 14.127 | 36.7 | 516 | 83419 | 8410 | 1.448 | 12 |
| dev-vsync-on | stress | normal | 2.171 | - | 6.969 | 2.506 | 4.424 | 9.006 | 1.420 | 14.648 | 9.815 | 31.6 | 859 | 132687 | 16140 | 2.739 | 16 |
| dev-vsync-on | stress | small | 1.904 | - | 6.525 | 2.261 | 4.264 | 8.878 | 1.690 | 19.018 | 12.827 | 63.4 | 857 | 121083 | 10153 | 1.720 | 64 |
| dev-vsync-off | peak | normal | 1.210 | - | 2.634 | 1.498 | 1.135 | 4.076 | 0.719 | 3.135 | 0.130 | 17.3 | 150 | 50148 | 11803 | 0.938 | 7 |
| dev-vsync-off | peak | small | 1.017 | - | 2.324 | 1.267 | 1.057 | 3.663 | 0.814 | 2.934 | 0.126 | 16.9 | 149 | 41918 | 7523 | 0.440 | 16 |
| dev-vsync-off | progressed-peak | normal | 1.663 | - | 4.739 | 1.957 | 2.783 | 6.179 | 0.690 | 3.804 | 0.132 | 20.7 | 515 | 94363 | 14395 | 1.322 | 0 |
| dev-vsync-off | progressed-peak | small | 1.350 | - | 4.270 | 1.619 | 2.645 | 5.662 | 0.687 | 3.249 | 0.118 | 19.1 | 517 | 83232 | 8406 | 0.724 | 1 |
| dev-vsync-off | stress | normal | 2.047 | - | 6.608 | 2.348 | 4.259 | 8.420 | 1.170 | 4.554 | 0.137 | 31.8 | 861 | 132688 | 16139 | 1.420 | 39 |
| dev-vsync-off | stress | small | 1.827 | - | 6.237 | 2.137 | 4.104 | 8.091 | 1.149 | 4.919 | 0.371 | 38.1 | 863 | 122048 | 10151 | 1.104 | 40 |
| playtest-vsync-on | peak | normal | 0.966 | - | 2.051 | 1.229 | 0.822 | 3.575 | 1.041 | 20.304 | 17.192 | 18.2 | 148 | 49858 | 11804 | 1.679 | 7 |
| playtest-vsync-on | peak | small | 0.881 | - | 1.943 | 1.154 | 0.789 | 3.578 | 1.166 | 19.420 | 15.844 | 21.5 | 149 | 41896 | 7524 | 1.086 | 7 |
| playtest-vsync-on | progressed-peak | normal | 1.303 | - | 3.601 | 1.596 | 2.005 | 5.322 | 1.130 | 18.534 | 14.681 | 23.7 | 514 | 94411 | 14399 | 1.996 | 7 |
| playtest-vsync-on | progressed-peak | small | 1.223 | - | 3.493 | 1.526 | 1.967 | 5.282 | 1.375 | 19.467 | 15.167 | 34.9 | 516 | 83262 | 8411 | 1.400 | 14 |
| playtest-vsync-on | stress | normal | 1.669 | - | 5.157 | 1.988 | 3.169 | 7.143 | 1.392 | 17.677 | 12.537 | 37.8 | 856 | 132086 | 16141 | 3.260 | 27 |
| playtest-vsync-on | stress | small | 1.530 | - | 4.931 | 1.844 | 3.087 | 6.964 | 1.644 | 19.658 | 13.894 | 58.7 | 858 | 121150 | 10153 | 1.688 | 54 |
| playtest-vsync-off | peak | normal | 0.886 | - | 1.927 | 1.124 | 0.804 | 3.163 | 0.605 | 2.651 | 0.115 | 14.5 | 149 | 50003 | 11802 | 0.931 | 5 |
| playtest-vsync-off | peak | small | 0.760 | - | 1.744 | 0.989 | 0.755 | 2.928 | 0.553 | 2.515 | 0.109 | 15.2 | 149 | 41966 | 7522 | 0.400 | 5 |
| playtest-vsync-off | progressed-peak | normal | 1.214 | - | 3.524 | 1.478 | 2.023 | 4.845 | 0.749 | 3.462 | 0.118 | 18.4 | 515 | 93771 | 14395 | 1.207 | 7 |
| playtest-vsync-off | progressed-peak | small | 1.078 | - | 3.213 | 1.324 | 1.890 | 4.424 | 0.603 | 3.029 | 0.118 | 17.0 | 517 | 82915 | 8406 | 0.552 | 8 |
| playtest-vsync-off | stress | normal | 1.592 | - | 5.023 | 1.869 | 3.153 | 6.482 | 1.005 | 4.062 | 0.134 | 27.3 | 864 | 133408 | 16138 | 1.457 | 31 |
| playtest-vsync-off | stress | small | 1.461 | - | 4.695 | 1.734 | 2.961 | 6.212 | 1.037 | 4.864 | 0.471 | 33.7 | 866 | 122340 | 10150 | 1.045 | 35 |

All times are means in ms except cpu_frame p99; primitives, vertices and textured quads are per-frame means over the whole egui output (not the battlefield painter alone); GPU egui is the `lastlight/egui` timestamp span (supported on this adapter). The cpu_frame p99 column is only gated with VSync off.

**interface_draw against wave 0 (the 5% check).** Wave 0 measured 300 frames after a 90-frame warmup; v3 uses the budget's 600 after 180. R19 accepts this window difference as documented here; the gates stay 0.70x and 1.0x the wave-0 means.

| Scene | Size | wave 0 (300 frames) | v3 pass 1 (600) | v3 pass 2 = baseline (600) | v3 at the wave-0 window (300, warmup 90) |
|---|---|---|---|---|---|
| peak | normal | 1.1904 | 1.3221 (+11.1%) | 1.2152 (+2.1%) | 1.2195 (+2.4%) |
| peak | small | 1.0752 | 1.1002 (+2.3%) | 1.0841 (+0.8%) | 1.0783 (+0.3%) |
| progressed-peak | normal | 1.6128 | 1.6502 (+2.3%) | 1.6550 (+2.6%) | 1.6542 (+2.6%) |
| progressed-peak | small | 1.4719 | 1.5123 (+2.7%) | 1.5180 (+3.1%) | 1.4990 (+1.8%) |

- Drift peak normal: pass 1 was 11.1% above wave 0 (repeats 1.296 / 1.340 / 1.322). Investigated: it is not instrumentation and not content. The W1-33 probe inside ui::draw is three `Instant` pairs (battle_frame_build, battlefield_paint, backdrop), one atomic egui-pass count and one scene-load record per pass; the other eleven cells of this table bound it together with noise at 0.3-3.1%, at most 0.05 ms. The same binary, scene and window measured 1.2152 (+2.1%) in pass 2 with the same scene load (52 enemies; 5-6 structures and 9-10 corpses at the end of the window, as in pass 1), and 1.2195 (+2.4%) at the wave-0 window, so the 600-frame window does not explain it either. Pass 1 ran without a load trace while other sessions were building; the machine sampled 93% load minutes after it ended. It is recorded as unreproduced pass-to-pass noise of up to about 9% on this shared machine, and the baseline uses pass 2. No cell of the baseline pass is more than 5% off wave 0.

**What the baseline shows before any wave-1 optimisation.**
- interface_draw is 47% over the progressed-peak gate at both sizes (1.655 vs ≤ 1.129, 1.518 vs ≤ 1.030); peak is at +2.1% / +0.8% over its not-above-wave-0 gate.
- interface_total is 2.8-3.0x interface_draw at progressed-peak (tessellation 2.7 ms is the largest share, 515 clipped primitives and 94k vertices per frame), so the anti-shift guard has a real margin to watch.
- cpu_frame p99 with VSync off is 16.9-38.1 ms in dev and 14.5-33.7 ms in playtest, against 10.0 ms; from the means, main_cpu is the larger share at progressed-peak and stress (4.4-8.4 ms against 3.5-5.7 ms of extract + render_schedule - present_wait), and the two are about equal at peak.
- Frames over 50 ms are 0 in only one cell, dev VSync-off progressed-peak normal (wave 0 had 0 in 300 frames). The long frames are about 0.5 s each: with VSync on and pipelined rendering they sit in present_wait (swap-chain acquire, 514-530 ms), with VSync off they fall outside every span (main_cpu ≤ 17 ms, present_wait 0), and with VSync on and pipelining off they nearly vanish (pipelined A/B below). This is the isolated 0.5 s frame class in VALIDATION.md, not caused by the probe; the budget's frames_over_50ms 0 is unreachable on this machine until it is understood.
- Two launches hung instead of reporting, both peak normal repeat 1 with VSync on: `dev-vsync-pipelined-off` ended in a Vulkan DeviceLost after 5 minutes, `dev-vsync-peak-wave0-window` (pipelined default) hit the 900 s script timeout. Both logged the boss victory at tick 15420 and wrote no report; the rejected `dev-vsync-b` attempt above is the same battle ending first but with a report. The reruns (`-b`) completed. Logged for W1-61, not reproduced.

**Pipelined A/B (informational; W1-61 decides with both profiles).** dev, median of 3; on = the recorded runs above (`dev-novsync` + `dev-novsync-progressed-peak-b`, `dev-vsync-b`), off = `-Pipelined off` (`dev-novsync-pipelined-off`, `dev-vsync-pipelined-off-b`). Every cpu_frame p99 on both sides is above the 12.0 ms decision threshold. In `dev-vsync-pipelined-off-b`, progressed-peak small repeat 3 and both stress sizes overlapped ≥ 90% load samples, so those rows are the weakest.

| VSync | Scene | Size | cpu_frame p99 on / off | critical_path p99 on / off | median frame on / off | mean FPS on / off | frames > 50 ms on / off |
|---|---|---|---|---|---|---|---|
| off | peak | normal | 17.3 / 15.4 | 12.4 / 15.4 | 4.59 / 6.82 | 103.4 / 62.6 | 7 / 13 |
| off | peak | small | 16.9 / 18.9 | 11.9 / 18.9 | 4.10 / 6.29 | 100.1 / 68.2 | 16 / 10 |
| off | progressed-peak | normal | 20.7 / 18.9 | 13.3 / 18.9 | 6.72 / 9.74 | 134.8 / 49.2 | 0 / 14 |
| off | progressed-peak | small | 19.1 / 23.8 | 11.5 / 23.8 | 6.09 / 9.49 | 146.4 / 54.7 | 1 / 11 |
| off | stress | normal | 31.8 / 31.7 | 21.5 / 31.7 | 8.75 / 12.69 | 54.7 / 38.1 | 39 / 53 |
| off | stress | small | 38.1 / 37.4 | 28.0 / 37.4 | 8.21 / 12.34 | 55.7 / 38.1 | 40 / 57 |
| on | peak | normal | 20.3 / 17.3 | 15.2 / 17.3 | 16.68 / 16.69 | 41.3 / 60.0 | 12 / 0 |
| on | peak | small | 31.9 / 20.6 | 26.7 / 20.6 | 16.69 / 16.71 | 44.1 / 59.4 | 10 / 1 |
| on | progressed-peak | normal | 23.4 / 21.1 | 16.0 / 21.1 | 16.67 / 16.73 | 48.1 / 59.6 | 6 / 1 |
| on | progressed-peak | small | 36.7 / 32.8 | 27.9 / 32.8 | 16.70 / 16.75 | 43.7 / 58.3 | 12 / 3 |
| on | stress | normal | 31.6 / 31.0 | 20.8 / 31.0 | 16.68 / 16.68 | 59.7 / 46.4 | 16 / 19 |
| on | stress | small | 63.4 / 49.9 | 52.2 / 49.9 | 16.72 / 16.86 | 37.7 / 38.0 | 64 / 254 |

Reading: with VSync off, pipelining raises throughput (median frame 4.1-8.8 ms against 6.3-12.7 ms) at a similar cpu_frame p99. With VSync on, both hold a 16.7 ms median, but pipelining carries the 0.5 s present_wait stalls (mean FPS 41-48 against 58-60 at peak and progressed-peak). Mean FPS includes those stalls; it is not a smoothness measure on its own.

**Isolated 0.5 s frames attributed (W1-68, 2026-11-06).** The frame report now records the between-frames gap (previous end of `Last` to First), its render hand-off and the pipelined channel wait inside it, the time outside App::update, and render-thread idle. It also records per-frame window, image-asset, audio-asset and audio-voice counts, wall-clock instants in `longest_frames` with a per-frame window `attribution`, and a `slow_frames` summary. Fresh probes (`target/perf/w1-68/`) had 0 frames over 50 ms in 66 runs, 6 of them per pass:
- playtest, VSync off, progressed-peak: pipelined default, pipelined off, `-Adapter` Intel Arc, `WGPU_BACKEND=dx12`, and a 3,600-frame window;
- playtest, VSync off, stress;
- dev, VSync on, progressed-peak and stress;
- playtest, VSync on, stress;
- 12 runs of the unchanged W1-67 binary (dev, VSync on, stress).

That binary had 4-14 such frames per stress run 40 minutes earlier, so the A/B arms cannot be told apart today. The stalled W1-67 frames sat in `get_current_texture` (present_wait 497-547 ms, or about 1.0 s, which is wgpu-core's 1,000 ms acquire timeout) with normal main and render CPU. The fresh probes had no egui texture uploads, asset events or audio voices, but they also had no stalls. The finding has two halves (narrowed by the W1-68 review):
- VSync on (W1-67): an intermittent swap-chain acquire stall in the driver or compositor of this hybrid-graphics laptop, not in-process.
- VSync off (W1-61, the spec's 446-532 ms stalls): not attributed. All 16 frames over 50 ms had present_wait 0.09-0.20 ms, so they were not in the acquire call. They sat outside every span the report then recorded, and they did not recur under the new probe. W1-68 is not done for this half. **Closed by W1-61 (2026-11-06):** this half is the same swap-chain acquire stall. With `slowest_present`, all 220 VSync-off frames over 50 ms in the W1-61 A/B, pipelined or not, were acquire waits (acquire shares 0.981-0.992, in-process work 7.8-19.2 ms). Pipelined, the render frame that stalled was replaced before the main thread read it; W1-61 gives that as the reason these 16 frames showed a present_wait of 0.09-0.20 ms. See the end of this section.

The attribution `coverage` is 1.0 by construction inside the recorded range, because `outside_update` is the residual of the gap. So the review added `named_coverage`, `covered_95_named` and `max_outside_update_ms`, which leave that residual out. Gate 3 is unchanged and needs a user ruling on two separate open items: the environmental VSync-on stall, and the unattributed VSync-off stall, which needs a reproduction under the W1-68 probe. (Both settled since: the user ruling below, and W1-61 naming the VSync-off half as the same acquire stall.) Details are in VALIDATION.md, "The isolated 0.5 s frames (gate 3)". In the same probes, cpu_frame p99 with VSync off was 6.2-7.8 ms (informational for W1-61).

**User ruling on gate 3 (2026-11-06, in chat): "Exclude driver stalls" (W1-61).** Verbatim: frames over 50 ms in which the swap-chain acquire wait (present_wait, or channel_wait while the render thread is in present) holds at least 80% of the frame are classified as environmental driver stalls and excluded from frames_over_50ms. They must still be attributed (named coverage), the in-process work in that frame (main_cpu and render CPU) must stay below 50 ms, and the excluded stalls are counted and listed in the report and summary (frames_over_50ms_driver_stall). The same exclusion applies to the pipelined rule's frames_over_33ms and VSync-on FPS and p99-interval checks, computed on frames outside stalls. How it is implemented:
- The frame report (still version 3, every earlier key unchanged, `frames_over_50ms` still counts every frame) adds `attribution.acquire_wait_ms`: the main thread's render hand-offs that overlap a render frame's `prepare_windows`. Not pipelined, that is the frame's own present_wait; pipelined, it is the channel wait while the render thread presents.
- The probe now also hands each close the render frame with the longest present wait since the previous close (`slowest_present`). Pipelined, the render frame that stalled is replaced by the next one before the main thread reads it, so without this the report never saw its present wait.
- A frame is a stall when it is over 50 ms, `acquire_wait_ms` is at least 80% of it, `named_coverage` is at least 95%, and main CPU plus render CPU is below 50 ms. Main CPU is the larger of the main schedules overlapping the frame's window and the sample's own First..Last: the window closes at First, so the main schedule inside a stalled frame is the previous sample's (W1-61 review). Render CPU is the larger of the latest and the stalled render frame. A frame without attribution never is.
- `frames_over_50ms_driver_stall` counts the stalls, and `driver_stalls.frames` lists them. `driver_stalls.outside` gives the sample count, mean FPS, p99 interval and frames over 33 and 50 ms of every other frame.
- `measure_performance.ps1` gates `frames_over_50ms` on `driver_stalls.outside.frames_over_50ms` (worst repeat). A report without it is invalid evidence (exit 2). summary.json and summary.md list every stall per run. The pipelined decision reads the `outside` statistics.
- The thresholds live in PERF_BUDGET.json `driver_stall`, mirrored by `perf_budget::DRIVER_STALL_*`.

**Goal-1 acceptance and the pipelined decision (W1-61, 2026-11-06, commit 199d823 with W1-69).** RTX 1000 Ada, Vulkan, mains, focused window, a 5 s CPU-load sampler beside every pass. Every run was valid at its first attempt.
- Acceptance passes, all exit 0, 0 frames over 50 ms in every repeat and 0 driver stalls excluded (in a stall-free hour; see "Why the acceptance passes had 0 driver stalls" below):
  - dev, VSync on, progressed-peak, Repeat 3: interface_draw 0.990 / 0.952 ms against ≤ 1.129 / 1.030; interface_total 1.95 / 1.80 against the v3 4.64 / 4.59.
  - dev, VSync on, peak and stress (exit 0): peak interface_draw 0.775 / 0.768 against its gate, not above wave 0's 1.190 / 1.075 (PERF_BUDGET.json `scenes.peak`).
  - -VsyncOff, Repeat 3, all scenes: cpu_frame p99 dev 8.85 / 7.01 (progressed-peak) and 7.50 / 7.84 (stress); playtest 6.26 / 6.44 and 6.48 / 6.54, all against ≤ 10.0.
  - CPU load during the measured runs was at most 64% (dev) and 78% (playtest). The playtest pass's 100% sample fell in its shipped cargo build, before the first run.
- Low latency: a launch without a scenario, with settings `{version 1, low_latency true, master 0}` and `LASTLIGHT_PERF_REPORT`, reported `present {requested_mode Mailbox, source settings, pipelined_rendering false, low_latency true}` (`target/perf/w1-61/head-low-latency-playtest`).
- Shipped smoke: `-ShippedSmoke` (playtest, `--no-default-features`) took 2.43, 1.19 and 1.23 s, so shipped_smoke_seconds is 1.23 (the median).
- Integrated GPU: still logged pending (`hardware.integrated_gpu.status` pending, ACCEPTANCE.md human-check row).

The pipelined A/B was re-run on this commit. The first W1-61 A/B predates W1-69 and its slow frames were never classified, so it was not reused. The protocol was interleaved ABAB at the run level: playtest and dev, -VsyncOff, progressed-peak and stress, both sizes, 3 repeats per arm, 1,800 frames, then the playtest VSync-on pair. That gave 12 valid runs per arm and group. Medians of 3. "Outside stalls" values follow the user ruling, and stalls are summed over the 3 repeats.

| Profile, VSync off | Scene | Size | cpu_frame p99 on / off | median frame on / off | frames > 33 ms outside stalls (worst) on / off | driver stalls on / off |
|---|---|---|---|---|---|---|
| playtest | progressed-peak | normal | 6.15 / 6.32 | 2.99 / 4.99 | 0 / 0 | 13 / 13 |
| playtest | progressed-peak | small | 6.12 / 5.73 | 2.91 / 4.89 | 0 / 0 | 7 / 16 |
| playtest | stress | normal | 7.30 / 6.09 | 3.06 / 5.15 | 0 / 0 | 17 / 16 |
| playtest | stress | small | 6.08 / 6.03 | 3.02 / 5.01 | 0 / 0 | 8 / 9 |
| dev | progressed-peak | normal | 7.11 / 6.93 | 3.52 / 5.84 | 0 / 0 | 9 / 21 |
| dev | progressed-peak | small | 6.49 / 6.54 | 3.40 / 5.67 | 0 / 0 | 10 / 13 |
| dev | stress | normal | 7.55 / 6.87 | 3.72 / 6.00 | 0 / 0 | 20 / 21 |
| dev | stress | small | 7.03 / 6.73 | 3.59 / 5.85 | 0 / 0 | 13 / 14 |

| Playtest, VSync on | Size | mean FPS outside stalls on / off | p99 interval outside stalls on / off | frames > 50 ms outside stalls (worst) on / off | driver stalls on / off |
|---|---|---|---|---|---|
| progressed-peak | normal | 60.00 / 60.00 | 17.45 / 17.40 | 0 / 0 | 1 / 5 |
| progressed-peak | small | 60.00 / 59.99 | 19.69 / 19.13 | 0 / 0 | 3 / 3 |
| stress | normal | 60.00 / 60.00 | 17.82 / 17.35 | 6 / 0 | 3 / 0 |
| stress | small | 60.00 / 60.00 | 19.60 / 19.66 | 0 / 0 | 0 / 0 |

Decision: **the default stays on** (`PIPELINED_DEFAULT` true, `decisions.pipelined_rendering`). With pipelining off, every VSync-off cell meets the 12.0 ms and 0-over-33-ms limits. But the VSync-on p99 interval at the small size is 19.13 and 19.66 ms, above 17.7 ms, so the rule keeps pipelining on. Both arms miss that interval at the small size, so it is not a pipelining cost. With VSync off, pipelining lowers the median frame from 4.9-6.0 ms to 2.9-3.7 ms at a similar cpu_frame p99. One VSync-on stress repeat with pipelining (`stress-on-r2`) had a stretch of about 400 frames of 47-67 ms. The six of them over 50 ms (52-67 ms) were mostly acquire wait, with acquire shares of 0.78-0.86. But their named coverage was 0.89-0.94, with 4-6 ms outside App::update, so they are not driver stalls. That repeat exited 1.

**Open (W1-61 review, 2026-11-07): the sustained 15-20 FPS episode in the shipped default configuration.** `head-ab-playtest-vsync/stress-on-r2` (playtest, pipelined on, VSync on, stress, 1440x900) ran about 400 consecutive frames at 47-67 ms instead of 16.7 ms. Outside driver stalls it had 6 frames over 50 ms (gate 3 fails, exit 1), 407 over 33 ms, a p99 interval of 47.1 ms and a mean of 46.3 FPS. Its twins (the other two on repeats, all three off repeats, the small size) were clean, so this is a single, unexplained occurrence, not a pipelining cost the decision could weigh. It is not a driver stall under the ruling (named coverage 0.89-0.94), and it is not attributed: 4-6 ms per frame sat outside App::update. It stays open (W1-68's VSync-off half is closed by W1-61, see below), and gate 3 on HEAD is not claimed clean while it stands. W1 review: it is scheduled as W2-50 (perf-architecture, before the W2-47 re-gate; waves.md) and tracked in 2.5.

**Why the acceptance passes had 0 driver stalls (W1-61 review, 2026-11-07).** The A/B and the acceptance passes ran the same commit, 1,800 stress frames, pipelining on. The 12 acceptance stress runs with -VsyncOff had 0 driver stalls, against 58 in the 12 pipelined-on A/B stress runs (0-12 each). The harness differs only in `-Pipelined on` (`LASTLIGHT_PERF_PIPELINED=1`, which sets the same plan value; `present.source` is `launch-override` in both because of -VsyncOff) and `-Repeat 1` per invocation. A discriminating pass on 2026-11-06, 23:10-23:16 UTC (`target/perf/w1-61/review/`, playtest, -VsyncOff, stress, both sizes, 1,800 frames) ran both harnesses interleaved: `-Pipelined on -Repeat 1` twice, the acceptance default `-Repeat 3`, `-Pipelined on -Repeat 3` and the default with `-Repeat 1`. All 18 runs had 0 frames over 50 ms and 0 driver stalls. No harness variable brings the stalls in. The stalls come and go with time: the last driver stall in the W1-61 evidence fell in the VSync-on A/B at about 22:22 UTC, and none occurred in the 54 acceptance runs from 22:32 or the 18 review runs. That is the same pattern W1-68 saw (4-14 per stress run at 18:09, then 0 in 66 probes). So the A/B on arm stands for the default, and the acceptance pass's 0 stalls shows a stall-free hour, not that the stall is gone. The discriminating pass ran the W1-61 review binary (the stall rule fix only) without the CPU-load sampler. (W2-50, 2026-11-08: the stalls come from the display being off, not from the hour; see "W2-50: gate 3 in the shipped default configuration" below.)

The VSync-off half of the 0.5 s stall is now named. With `slowest_present`, all 220 VSync-off frames over 50 ms in this A/B, pipelined or not, were the swap-chain acquire. Their acquire shares were 0.981-0.992, the in-process work 7.8-19.2 ms and the frames 480-623 ms. Pipelined, the render frame that held the stall finished before the main thread closed its frame, which is why the earlier reports never saw it (VALIDATION.md, "Goal-1 acceptance").

**Goal-1 acceptance re-run at HEAD (W1 review, 2026-11-07, commit 867f786).** The W1-61 passes, unchanged harness and gates, on the probe spans from `7abb0ab`. Same hardware, Vulkan, mains, power plan Balanced, focused window. No other horde-defense.exe ran, and a 5 s CPU-load sampler ran beside every pass (`target/perf/w1-review/<pass>.load.tsv`). Every run was valid at its first attempt. Evidence: `target/perf/w1-review/acc-*`.

| Pass | Exit | Gate | normal | small | Limit | Driver stalls excluded | Load during runs, max |
|---|---|---|---|---|---|---|---|
| dev, VSync on, progressed-peak | 0 | interface_draw mean (ms) | 1.039 | 1.012 | ≤ 1.129 / 1.030 | 0 | 47% |
| dev, VSync on, progressed-peak | 0 | interface_total mean (ms) | 1.965 | 1.866 | ≤ 4.645 / 4.590 (v3, old spans) | 0 | 47% |
| dev, -VsyncOff, all scenes | 0 | cpu_frame p99 progressed-peak / stress (ms) | 6.80 / 7.58 | 6.47 / 7.13 | ≤ 10.0 | 0 | 67% |
| playtest, -VsyncOff, all scenes | 0 | cpu_frame p99 progressed-peak / stress (ms) | 5.91 / 6.48 | 5.61 / 6.09 | ≤ 10.0 | 35 | 59% |
| dev, VSync on, peak (extra) | 0 | interface_draw mean (ms) | 0.784 | 0.774 | ≤ 1.1904 / 1.0752 | 5 | 51% |
| dev, VSync on, stress (extra) | 1 | frames > 50 ms outside stalls (worst) | 1 | 0 | 0 | 40 | 46% |

- Gate 3 in the three gate passes: 0 frames over 50 ms outside driver stalls in every repeat. Unlike W1-61's stall-free hour, the stall was active: the playtest pass excluded 35 (494-531 ms, acquire share 0.987-0.990, in-process work 7.3-13.9 ms), the extra VSync-on passes 5 and 40 (7 of them about 1,000 ms, wgpu's acquire timeout).
- The one gated frame is `acc-dev-vsync-stress/stress-normal-r1.json` sample 932: 54.7 ms, present wait 50.6 ms, acquire wait 43.5 ms (share 0.794, under the 0.80 threshold), named coverage 0.989, in-process work about 11 ms (main 7.1, render 3.9), directly after the stalls at samples 929-931 (998-1,001 ms each). Its five twins were clean. The gate was not moved. It is the tail of a driver stall, not in-process work, and joins the W1-61 open item for W2-50.
- The progressed-peak VSync-on interface_draw is 4.9% / 6.3% above W1-61 (0.990 / 0.952). That leaves 8.0% / 1.8% headroom. The -VsyncOff twins at progressed-peak fell instead (dev 0.952 / 0.880 to 0.909 / 0.872 ms; playtest 0.738 / 0.739 to 0.671 / 0.671 ms), and peak moved +1.2% / +0.7%. So no draw regression shows outside the VSync-on cell.
- Shipped smoke: 2.79, 1.23 and 1.19 s, median 1.23 (`-ShippedSmoke` in the playtest pass). Low latency was not re-run; its W1-61 evidence stands.

**Goal-1 acceptance re-run at HEAD, attempt 2 (W1 review, 2026-11-07, commit bee4236).** The three W1-61 gate passes again, same harness, gates, spans and hardware as the 867f786 re-run, 17:42-17:56 UTC. The commits since 867f786 are docs only, and the dev executable hashed the same. Before each pass `tasklist` showed no horde-defense.exe and no other cargo command ran; the 5 s CPU-load sampler ran beside every pass (`target/perf/w1-review2/<pass>.load.tsv`). Every run was valid at its first attempt. Evidence: `target/perf/w1-review2/acc-*`.

| Pass | Exit | Gate | normal | small | Limit | Driver stalls excluded | Load during runs, max / mean |
|---|---|---|---|---|---|---|---|
| dev, VSync on, progressed-peak | 0 | interface_draw mean (ms) | 0.998 | 0.989 | ≤ 1.129 / 1.030 | 15 | 55% / 25% |
| dev, VSync on, progressed-peak | 0 | interface_total mean (ms) | 1.898 | 1.824 | ≤ 4.645 / 4.590 (v3, old spans) | 15 | 55% / 25% |
| dev, -VsyncOff, all scenes | 0 | cpu_frame p99 progressed-peak / stress (ms) | 6.77 / 7.30 | 6.63 / 7.28 | ≤ 10.0 | 48 | 60% / 28% |
| playtest, -VsyncOff, all scenes | 0 | cpu_frame p99 progressed-peak / stress (ms) | 6.25 / 6.43 | 5.92 / 6.28 | ≤ 10.0 | 48 | 52% / 27% |

- Gate 3: 0 frames over 50 ms outside driver stalls in every repeat of all three passes. The stall was active throughout: VSync on 69-1,000 ms (acquire share 0.867-0.993, in-process work 8.8-11.3 ms, five at 955-1,000 ms, near wgpu's acquire timeout; the 85 and 69 ms frames directly after two of them cleared the 0.80 share at 0.869 / 0.867), VSync off 487-612 ms (0.983-0.991, 6.7-12.5 ms), named coverage at least 0.992.
- No CPU p99 cell missed, so no pass was re-run. Load during runs averaged 25-28%, with isolated 5 s samples of 52-60%; the 81% and 94% samples of the playtest pass fell before its first measured run (shipped build and smoke, review build).
- Against 867f786: interface_draw fell to 0.998 / 0.989 (from 1.039 / 1.012; 11.6% / 4.0% headroom), on the same executable, so the earlier rise was run-to-run spread. cpu_frame p99 moved -0.28 to +0.34 ms per cell, all at least 2.7 ms under the gate.
- Shipped smoke: 2.72, 1.25 and 1.26 s, median 1.26. The extra dev VSync-on peak and stress passes, and low latency, were not re-run; the 867f786 sample-932 frame and the W1-61 stress-on-r2 episode stay open for W2-50, so gate 3 on HEAD is still not claimed clean.

**W2-50: gate 3 in the shipped default configuration (2026-11-08, commit ddea5a6; 8 more held repeats per size in the W2-50 review, on the same executable).** Setup:
- `measure_performance.ps1 -Scene stress -Profile playtest -Pipelined on -Repeat 1`, VSync on, 28 invocations, so 28 repeats per size: 16 plain and 12 with the display held on. 14 per size ran with the display on throughout (the 12 held, plus 2 plain); the other 14 were display-off or crossed a display power transition. A 5 s CPU-load sampler and a 1 s `nvidia-smi` sampler ran beside each.
- 22:32-23:10 and 23:25-23:36 UTC, unattended, with the session locked. Every run passed the harness validity checks at its first attempt; under the recommendation below only the 14 display-on repeats per size would count. Evidence: `target/perf/w2-50/`. Details and tables: VALIDATION.md, "Gate 3 in the shipped default configuration".
- No gate or rule moved.

Results:
- **Driver stalls are the display being off.** Windows entered Modern Standby on the idle timeout (Kernel-Power 506, display off) several times during the runs. Across W2-50 and the W1-61 and W1-review evidence (every 7 November pass included), 521 frames were classed as driver stalls. Of these, 518 were display-off frames or fell within 5 s of a display power transition. No 500 ms or 1,000 ms stall occurred with the display on. The W1-67 dev VSync-on passes (2026-11-06, 17:06-17:13 UTC, v3 reports without a stall class) ran with the display off, and all 56 of their frames over 50 ms are display-off; the 66 W1-68 probes after them ran with the display on and had none. That explains the W1-61 "stall-free hour": the user woke the laptop at 22:17:09 UTC on 2026-11-06 and the display stayed on. It also explains the 867f786 re-run, whose stalls start once the display went off at 17:18:23 UTC on 2026-11-07.
- **The perturbation proof.** A display-required power request held by the driver process stopped the idle timeout. Its 24 runs (12 per size) had 0 stalls and 0 frames over 33 ms, against 95 stalls in the 32 plain runs; between its two batches, with no request held, the display went off 11 times.
- **Sample 932** fell about 8 minutes into a display-off interval. It is a display-off stall tail. W2-50 found the same class again: 65.9 ms, acquire share 0.660, at a display transition. It also found two other ways a stall escapes the rule there: named coverage 0.932, because the main thread spent 15.8 ms outside the update in the stall's second frame; and an 819 ms acquire while the main thread sat outside the update, so the acquire share was 0. Counting the whole present wait would catch only the first of these three. The rule is unchanged and the tail stays gated (§2.5 default). Recommendation: keep `driver_stall`, and hold the display on during perf passes, treating a run that overlaps a display-off interval as invalid evidence (a harness ruling, §2.5).
- **The `stress-on-r2` episode is not reproduced.** None of the 56 runs had it, including the 14 per size with the display on throughout. Its frames were held by the swap-chain acquire: present_wait 41.8-64.1 ms, about three vblanks, with in-process work of 6-8 ms. The 4-6 ms outside App::update came only in frames carrying window input events (21-52 per frame), and the episode's event-free frames had a named coverage of 0.99. It is the only report on the branch with sustained window input (mean 4.74 per frame; every W2-50 run had 0). It ran 5 minutes after the user woke the laptop by touchpad.
- **What is still open on the episode.** Input alone did not cause it: a VSync-on progressed-peak run at 22:18, with 7.65 events per frame, stayed at 60 FPS. The cause of the third-vblank cadence is not named. A supervised reproduction with pointer movement over the unlocked window is the remaining step. Until the §2.5 row is answered, the episode stays open, and gate 3 is not claimed clean in the shipped default configuration.
- **Rulings (2026-11-09, user decisions R26 and R27).** R26 closes the `stress-on-r2` episode as environmental: user activity during an evidence run (the laptop had been woken 5 minutes before, and the run carried window input). No supervised reproduction is run. Gate 3 in the shipped default configuration can be claimed once the W2-47 re-gate passes. R27 keeps the `driver_stall` rule unchanged and closes the sample-932 tail as display-off; instead, `measure_performance.ps1` holds the display on for the whole run, and a run that overlaps a Kernel-Power 506/507 display-off event is invalid evidence, rerun like the script's other invalid runs. The W2-47 re-gate runs on that harness.

### 9.5 Camera zoom delta (W1-47)

Recorded 2026-11-06 on `bbaa01c` (W1-43, the commit before C2: `fit_field` exists but the battle still draws the fixed-scale `clearing_rect`) against `d74a13e` (W1-44, C2: `BattleLayout` and uniform zoom-to-fit), same hardware and power state as 9.4. `measure_performance.ps1 -Scene peak` and `-Scene progressed-peak`, `-Profile dev`, VSync on (Fifo, pipelined default, the 9.4 baseline state), `-Repeat 3`, budget window (warmup 180, 600 frames). Each commit was checked out detached in this worktree, built by the script and measured, then the branch was restored. The machine was shared again, so the same 5 s `Win32_Processor.LoadPercentage` sampler ran beside every pass and both sides were measured in more than one pass: before C2 two passes per scene (`before-c2-bbaa01c`, `-b`), C2 three passes of peak (`c2-d74a13e`, `-b`, `-c`) and two of progressed-peak (`c2-d74a13e`, `-b`). Values are the median of every valid repeat over all passes of a side (6 before; 8 / 9 / 6 / 6 at C2); frames > 50 ms is the worst repeat. One repeat was invalid and is not counted: `c2-d74a13e-b` peak normal repeat 2 ran during about 60 s of 100% load and missed its sample target on both attempts. Evidence: `target/perf/w1-47/<pass>-<scene>` (not committed), each with summary.json and summary.md.

What C2 changes in these scenes: peak is the 7x16 field, which was drawn at 742.4x440.4 pt (normal) and 389.2x197.6 pt (small) and is now fit at zoom 1.571 to 1166.6x692 and 611.6x310.5, 1.571² = 2.47x the pixels. progressed-peak is 17x30, which fills the arena at zoom 1 before and after (pixel-identical by the W1-43 pin), so it is the control for C2's code cost and for machine noise.

| Scene | Size | Field | interface_draw before / C2 | delta | interface_total before / C2 | delta | battlefield_paint before / C2 | egui_pass before / C2 | tessellate before / C2 | main_cpu before / C2 | primitives before / C2 | vertices before / C2 | textured quads before / C2 | GPU egui before / C2 | frames > 50 ms (worst) before / C2 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| peak | normal | 7x16, z 1.571 | 1.1888 / 1.2623 | +0.074 ms (+6.2%) | 2.5215 / 2.6351 | +4.5% | 0.490 / 0.560 | 1.461 / 1.541 | 1.066 / 1.099 | 4.096 / 4.227 | 149 / 149 | 49893 / 53954 (+8.1%) | 11804 / 13636 (+15.5%) | 1.776 / 1.987 | 13 / 8 |
| peak | small | 7x16, z 1.571 | 1.0655 / 1.1087 | +0.043 ms (+4.1%) | 2.3551 / 2.4706 | +4.9% | 0.466 / 0.495 | 1.355 / 1.414 | 1.003 / 1.061 | 4.042 / 4.246 | 149 / 149 | 41710 / 43223 (+3.6%) | 7524 / 8082 (+7.4%) | 1.257 / 1.353 | 13 / 16 |
| progressed-peak | normal | 17x30, z 1 | 1.6594 / 1.6421 | -0.017 ms (-1.0%) | 4.7029 / 4.6447 | -1.2% | 0.689 / 0.680 | 1.950 / 1.935 | 2.760 / 2.715 | 6.507 / 6.514 | 514 / 514 | 94380 / 94404 (+0.0%) | 14399 / 14399 (+0.0%) | 2.094 / 2.228 | 12 / 8 |
| progressed-peak | small | 17x30, z 1 | 1.4401 / 1.4675 | +0.027 ms (+1.9%) | 4.4162 / 4.4801 | +1.4% | 0.580 / 0.591 | 1.743 / 1.784 | 2.666 / 2.730 | 6.280 / 6.418 | 516 / 516 | 83328 / 83346 (+0.0%) | 8411 / 8411 (+0.0%) | 1.372 / 1.418 | 12 / 11 |

Times are means in ms; primitives, vertices and textured quads are per-frame means over the whole egui output; GPU egui is the `gpu.egui_pass_ms` timestamp mean.

Per-pass interface_draw medians (3 repeats each, 2 for `c2-d74a13e-b` peak normal): peak normal 1.1785 / 1.2041 before against 1.2635 / 1.2627 / 1.2562 at C2; peak small 1.0238 / 1.0873 against 1.1087 / 1.1216 / 1.0973; progressed-peak normal 1.6329 / 1.7710 against 1.6767 / 1.6373; progressed-peak small 1.4281 / 1.4409 against 1.4954 / 1.4348.

- peak: every C2 pass is above every before pass at both sizes, so the +4-6% interface_draw and +4.5-4.9% interface_total are the camera's cost, not noise. The zoomed field emits the same 149 clipped primitives but 8.1% / 3.6% more vertices and 15.5% / 7.4% more textured quads, and battlefield_paint is the largest CPU share of the rise (+0.070 / +0.029 ms).
- progressed-peak: its egui output is unchanged (primitives, vertices and textured quads within 0.1%), and its interface_draw and interface_total deltas (-1.2% to +1.9%) are small. Only at normal are they inside the spread of its own before passes (1.6329 / 1.7710, a 0.138 ms spread against the -0.017 ms delta). At small the two before passes span only 0.0128 ms (1.4281 / 1.4409), the +0.027 ms interface_draw delta is about twice that, and one C2 pass (1.4954) is above both, so two before passes cannot separate a small-size rise of up to about 2% from noise. C2's layout code costs nothing measurable at normal and at most about 0.03 ms at small on a field that already filled the arena, so the progressed-peak interface gate (≤ 1.129 / ≤ 1.030) faces essentially the same work after C2 as before.
- GPU egui rose at peak (+0.21 / +0.10 ms) and also at the unchanged progressed-peak (+0.13 / +0.05 ms); with these repeats the fill cost of 2.47x the pixels is not separable from the span's pass-to-pass noise. median frame stayed 16.67-16.69 ms everywhere (VSync).
- Load check: three valid repeats overlapped a ≥ 90% load sample (`c2-d74a13e` peak small repeat 2, 1.5017; `c2-d74a13e` progressed-peak small repeat 2, 2.1560; `before-c2-bbaa01c-b` peak normal repeat 3) and one a 99% sample 2 s after launch (`before-c2-bbaa01c-b` progressed-peak normal repeat 1). Without those four the interface_draw deltas are +6.9% / +3.9% at peak and -0.4% / +1.0% at progressed-peak: the same reading.
- Gates: nothing here moves a budget. The 30% progressed-peak interface gate (≤ 1.129 / ≤ 1.030) is unchanged and still evaluated in W1-61. The peak not-above-wave-0 gate (1.1904 / 1.0752) was met before C2 (1.1888 / 1.0655) and is now exceeded by 6.0% / 3.1% (1.2623 / 1.1087); the camera's battlefield_paint and vertex cost at peak is work for the batched battlefield meshes (W1-48) before W1-61's acceptance runs.

### 9.7 Loot numbers after L4 (W2 review)

design-loot-economy "Numbers to measure" asks for five numbers after L4 (W2-09, the weighted table). Three were measured in the W2 review on 759cf49 (2026-11-10). The other two are W4-l's (§2.5).

Method. `le_model.py` builds the 39 wave-2 items itself, so on the shipped catalogue it would count them twice. The tier shares therefore apply its weights to the shipped `content.json`, the same way `DropTable::new` does: `drop_pool(4)`, `DROP_TIER_WEIGHTS` [6, 3, 1, 1] and `TRAINED_CLASS_WEIGHT` 3 for a trained Ranged class. Drops and elites per run come from the simulation. A temporary print of `Report::loot`, `telemetry.elites_killed` and `drop_ids.len()` was added in `sim/smoke.rs` and in the pacing recorder's `record`, then reverted before this commit. The two runs were:
- `cargo run --locked --release -p horde_core --example campaign_smoke`. The campaign lines are byte-identical to lines 3-17 of `w2-54/smoke.txt`.
- `cargo run --locked --release -p horde_core --example pacing -- 150 --profile long --seeds 5`, which is expert, Normal, Germany, 150 runs and 750 runs in all.

The evidence is in `target/production-after/w2-review-loot/` (not committed). The printed lines are in `smoke.err.txt` (sha256 600dc69a...79973b51) and `long.err.txt` (ce2f7d9c...50cab9f9). The outputs are `smoke.out.txt` (5523355f...0a0b622a) and `long.out.txt` (317f4099...0e5e237c), and the prints are in `instrumentation.diff` (699176f6...a7e7eb74). Each run's rank is the progression rank before it, which is the previous run's rank after it. Every drop in these runs is on the `drop_tier` Some path (`drop_ids.len()` equals `loot`).

| Number | Model or prediction | Measured after L4 |
|---|---|---|
| Tier shares at drop tier 4, trained Pistol + Rifle (t4 / t3 / t2 / t1) | 52.8 / 26.4 / 8.8 / 12.0% | 52.8 / 26.4 / 8.8 / 12.0% (total weight 216). With one trained class: 53.1 / 26.6 / 8.9 / 11.5% (192) |
| Drops per run, baseline smoke | 1 + 0.0785 x kills, 9-23 | Personal 7-31 (87-286 kills), Npc 5-15 (59-152 kills). Elite-free runs (ranks 0-2) follow the model: 7 / 16 / 14 / 14 / 16 against 7.8 / 12.8 / 12.7 / 14.2 / 17.6 (Personal) and 7 / 5 / 8 / 9 / 10 / 14 against 5.6 / 5.7 / 7.4 / 8.6 / 8.8 / 10.5 (Npc). The rank 3 and 4 stages add elites: Personal stage 4 (7 elites) 24 and stage 5 (10 elites) 31; Npc stages 4 and 5 (3 elites each) 14 and 15 |
| Drops per run, long profile ranks 0-2 (29 runs) | 1 + 0.0785 x kills | median 15 (6-24) at median 153 kills; mean 14.3 against the model's 14.1 |
| Elite drops per run at rank 3 (6 runs) | none given | elites killed median 5.5 (2-7); drops median 24.5 (23-27) at 273-282 kills; mean drops 24.7 against 22.7 without elites |
| Elite drops per run at rank 10 (5 runs) | none given | elites killed median 29 (20-33); drops median 57 (50-63) at 426-455 kills; mean drops 58.0 against 35.8 without elites |
| Elite drops per run at rank 23 (5 runs) | about 45 extra per 300-kill run (design Risks item 3) | elites killed median 90 (71-92); drops median 124 (112-129) at 569-596 kills; mean drops 122.8 against 47.2 without elites, so 75.6 extra drops per run |

Each elite kill drops one item (ruling E8). On a kill that already drops normally, the elite term adds nothing, so the extra drops are a little below the elite count. Per kill, rank 23 matches the design's estimate: 85.6 elites per 589 kills is 14.5%, or about 44 per 300 kills. But the long profile's rank-23 runs have about 590 kills, not 300, so a run there drops about 2.6 times what the model predicts without elites. Past rank 23 (601 runs), drops average 143.1 per run against 58.7 without elites, at median 722 kills and 98 elites. This is the stash growth that design Risks item 3 and W4-l measure. Nothing is tuned here: drop and sale balance belongs to wave 4.

Not measured here; handed to W4-l (§2.5): the median stash size after 60 and 150 runs with the auto-sell profile, and the auto-sold coins per run in runs 1-6.

---

## 10. Capture findings ledger

Format owned by qa-captures and checked by `scripts/check_capture_findings.py` (vocabulary: Size `1440x900`, `1050x700@1.35` or `both`; Mode `normal`, `reduced` or `both`; Severity blocker/major/minor/polish; Status open/fixed/deferred; Deferral only as `approved by user YYYY-MM-DD`, given by the user in chat).

<!-- capture-findings:begin -->
| ID | Wave | Scene | Size | Mode | Finding | Severity | Owner | Status | Fix commit | After evidence | Deferral |
|---|---|---|---|---|---|---|---|---|---|---|---|
| CF-0-01 | 0 | progressed-battle, leveled-armory | both | both | Capture panics at ui/review/fixtures.rs:138:14; no before still (8 of 164 failed) | blocker | qa-captures | fixed | 299470a | target/production-after/w1-05/normal/progressed-battle-small.png#6bc210e3bed4 | |
| CF-0-02 | 0 | battle | both | both | 3x12 field uses about 40% x 27% of the arena (fixed-scale clearing) | blocker | vfx-lighting | fixed | bbaa01c, d74a13e | target/production-after/w1-60/normal/intro.png#1ffd0c7515b8, target/production-after/w1-60/normal/intro-small.png#38ae78b08b48, target/production-after/w1-60/normal/battle-small.png#ce1dc61dca49 | |
| CF-0-03 | 0 | construction | both | both | Construction mode shrinks the arena by 252 pt | blocker | vfx-lighting | fixed | d74a13e, d6a9b7b | target/production-after/w1-60/normal/construction.png#dc416f49dbdb, target/production-after/w1-60/normal/construction-small.png#55f83b52dada | |
| CF-1-01 | 1 | battle, peak, damage, boss, intro | both | both | The PAUSED / READY banner keeps its arena.bottom-18 position (ruling C-3), so on a zoomed field it now covers units in the bottom lane (peak-small lane 7 barricade, the damage target under "69!"); before the zoom it sat below the small clearing. Intro only at 1050x700@1.35: "READY · Begin defense or Escape" draws at y 400-432 pt inside the 217.8-432.8 field, over about half of the bottom lane of the 3-lane first defense (at 1440x900 it sits below the field). Scheduled as W3-29 (R17; fail-first `pause_banner_never_covers_the_bottom_lane`) | minor | vfx-lighting | open | | | |
| CF-1-02 | 1 | construction | 1050x700@1.35 | both | The field-top "NO BUILDING BEYOND MIDPOINT" label overlaps the top edge of the construction overlay panel; "OND MIDPOINT" lies on the panel at low contrast. Scheduled as W3-29 (R17; fail-first `midpoint_label_clears_the_construction_panel`) | polish | vfx-lighting | open | | | |
| CF-1-03 | 1 | max-lanes, progressed-peak, every Battle still | both | both | Review captures read the live OS pointer: `f.battle.aim` follows `response.hover_pos()` whenever the reticle preview is off (ui/battle/field.rs:56-66), so aim, the painted reticle and inspector hover depend on where the mouse sat during the run (most of the 7,026 changed pixels in w1-60 normal max-lanes.png). Before/after pairs then differ for reasons outside the reviewed change, including in W7-11's compare_captures. Scheduled as W3-30 (R17, owner vfx-lighting; fail-first `review_capture_aim_ignores_the_os_pointer`) | minor | vfx-lighting | open | | | |
| CF-1-04 | 1 | damage, damage-4x | both | normal | Damage-number pop is stepped since W1-64: the punch snaps to the nearest of x0.85 / 1.0 / 1.15 / 1.3 (0.85, 1.0, 1.15, 1.3, 1.15, 1.0 over its first 0.18 s at 60 fps) instead of a continuous scale, and every face places glyphs on whole pixels, so battle frames stop re-uploading the egui font atlas. Reduced mode never punched and is unchanged. Motion rows in 10.2; the feel of the stepped pop is a human check | polish | perf-architecture | fixed | 6977198 | target/production-after/w1-64/damage-motion/frame-0004.png#75d2c40b0af8, target/production-after/w1-64/damage-motion-small/frame-0004.png#fcf13e1011b1 | |
| CF-1-05 | 1 | menu, battle, leveled-armory | both | normal | Since W1-64 every font face places glyphs on whole pixels (no subpixel bins), not only the display face, so all text on every screen can move by under a pixel. Against a build with the bins back on (`text-subpixel-on-b`), 0.23 / 0.36% of menu pixels change (1440x900 / 1050x700@1.35), 0.43 / 1.05% of battle pixels and 1.20 / 1.11% of leveled-armory pixels; two runs of the same build differ by 0.04-0.17%. Side-by-side crops show no wrap, clip or layout change. Whether spacing looks uneven at 1.35 is a human check (section 11) | polish | perf-architecture | fixed | 6977198 | target/production-after/w1-64/text-after/normal/menu-small.png#b66540800f6d, target/production-after/w1-64/text-after/normal/battle-small.png#4a244348b2ce, target/production-after/w1-64/text-after/normal/leveled-armory-small.png#cea1d9c06907 | |
| CF-1-06 | 1 | progressed-peak, stress (motion), peak, progressed-peak, fx-stress | both | both | Enemies overlap since W1-69 (user decision), so 2-6 bodies stack on each wall face and at the line. Unhandled, a stack drew as one body with 2-6 red windup rings and contact bands on top of each other. W1-69 fans each body by a static per-id stagger (`crowd_stagger`: up to 0.15 yd behind the feet and 0.12 lane across) and draws one contact band per lane and one windup ring per stack. Since bb78430 a lane paints far to near, a stack on one spot by its drawn feet (y + stagger, farther first), then by id, and collapsing and settled bodies lie at the same staggered spot, so a kill no longer jumps. Every body keeps its own windup and health bars and its hit flash. Motion re-captured on a5b83e9 (after bb78430; the w1-69-motion frames predate it): at 1440x900 wall stacks read as 2-3 distinct bodies, and in progressed-peak a fresh body lies under a live wall stack at its staggered spot; at 1050x700@1.35 stacks overlap more tightly but still show 2-3 bodies | minor | enemy-balance | fixed | db25cd7, bb78430 | target/production-after/w1-review-cf106-motion/progressed-peak-normal/frame-0023.png#596ec4f607c8, target/production-after/w1-review-cf106-motion/progressed-peak-small/frame-0023.png#166f35cdd979, target/production-after/w1-review-cf106-motion/stress-normal/frame-0023.png#fb703ba3bbea, target/production-after/w1-review-cf106-motion/stress-small/frame-0023.png#8a00c4f6ac20 | |
| CF-1-07 | 1 | progressed-peak, stress (motion) | 1050x700@1.35 | normal | With every stacked enemy now striking walls, nearly every wall in the 17-lane field shows a damage number or MISS in a frame. At 1050x700@1.35 those numbers ("-80", "MISS") cover the wall face and the front of its stack. At 1440x900 they cover the wall prop only. Placement of wall-hit numbers is fx work and is not changed here. Scheduled as W3-31 (R17; fail-first `wall_hit_numbers_clear_the_wall_face_and_its_stack_at_small_size`) | minor | vfx-lighting | open | | | |
| CF-1-08 | 1 | damage, battle, every Battle still whose bar wraps to two rows | 1050x700@1.35 | both | The controls legend ("Mouse1 fire · Mouse2 melee · R reload · B build · Escape pause", drawn at `layout.legend_anchor`, ui/battle/field.rs:187-203) lies under the second button row of the battle bar, so the bar's Build defenses / Retreat & train / Management buttons cover most of it (w1-60 damage-small and battle-small, normal and reduced). It predates wave 1: the wave-0 damage-small still shows the same overlap. At 1440x900 the bar is one row and the legend sits clear below it. Noted in the W1-64 motion review (10.2) without a row; scheduled for W3-12 (battle bar at most 2 rows), whose layout must keep the legend clear of the bar | minor | ux-flow | open | | | |
| CF-0-04 | 0 | battle, menu, leveled-armory | both | both | Non-practice review captures show "Practice scenario - progress is isolated from your campaign", and menu and camp reviews draw "Practice profile" (amber), "Separate practice saves" and the "Practice · separate campaign" footer | minor | qa-captures | fixed | 299470a, 3d6e5c0 | target/production-after/w1-05/normal/battle.png#d44738e6cb6a, target/production-after/w1-05-review/normal/menu.png#be3544456adb, target/production-after/w1-05-review/normal/leveled-armory.png#b6163c62d2a9 | |
| CF-1-09 | 1 | ordnance, arid, boss, peak, progressed-battle, stress (motion), peak (motion) | both | both | Damage numbers and loot labels in the 60-pt effects headroom (W1-44, `effects_clip` top at canvas.top+64) draw on the HUD top band, not in a free strip: the band's centre holds the selector, its status line and the "Last loot" receipt, and the HUD panels reach into it. At 1050x700@1.35 "46" (ordnance-small) and "29" (arid-small) lie on the green "Uncommon · Padded vest" receipt line, "32" (boss-small) shows through the player panel, and "23" lies on the player panel's action line in stress-small motion. At 1440x900 "32" (boss) lies on "Last loot · already banked" and a MISS sits at the player panel edge in peak motion. Before W1-44 the small clearing kept these popups below the band. design-perf-architecture.md:161 assumed "the free centre of the 124 pt top band", which the HUD does not leave free. Evidence: target/production-after/wave1/normal/ordnance-small.png#fe3ceac3c434, boss.png#b85503a656a8, boss-small.png#f9b35381fe07, motion/stress-small-normal/frame-0008.png#c6cddecf935c. Fixed by moving each popup and fly-up under any HUD rect it would meet (the clip keeps its headroom). The stills cannot show it: outside the damage scenes popups expire on the wall clock, and the re-captured stills (target/production-after/wave1/review-ui-battle) were slower and caught none, so the after evidence is the fixed-timestep motion frames. Since 45597c7, a HUD-shifted popup that meets the struck enemy's health bar sits beside the bar at its height (KEEP_CLEAR_GAP), so top-lane numbers on the zoomed 7x16 field no longer cover it; 3924a7c places it beside the struck hostile's live (drawn) bar, not its position at hit time. The after evidence predates both follow-ups | minor | vfx-lighting | fixed | 45195d0, 45597c7, 3924a7c | target/production-after/wave1/review-ui-battle/motion/stress-small-normal/frame-0008.png#723e3ad9bd8a, target/production-after/wave1/review-ui-battle/motion/peak-normal-normal/frame-0008.png#9d92af8af9b0 | |
| CF-1-10 | 1 | save-error | both | both | The critical save-failure notice draws "Details" and "Dismiss" past the right edge of the window, so "Dismiss" is cut to "Dismis" at both sizes. ui/feedback.rs reserves 120 pt for the two small buttons, which is less than their drawn width. The code predates wave 1; W1-54's save-error scene is the first capture to show it (target/production-after/wave1/normal/save-error.png#99d12307d5c8, save-error-small.png#f770f886d40f). The one-button notices ("Dismiss" alone) fit. Fixed by reserving the buttons' measured width (an invisible, disabled probe of the same small buttons) instead of the fixed 120/65 pt; widget and Tab order are unchanged. "Dismiss" now ends on the panel's content edge at both sizes. Reduced mode draws the same pixels (the critical notice never fades) | minor | ux-flow | fixed | e86d9e7 | target/production-after/wave1/review-ui-other/normal/save-error.png#16147a0e1805, target/production-after/wave1/review-ui-other/normal/save-error-small.png#b4c5409fdc7a, target/production-after/wave1/review-ui-other/reduced/save-error-small.png#b4c5409fdc7a | |
| CF-1-11 | 1 | max-lanes | 1050x700@1.35 | both | A boss name label at the field's right edge is cut by the arena clip: "SIEGE BRUTE" reads "SIEGE BR" in max-lanes-small (target/production-after/wave1/normal/max-lanes-small.png#3d3b5b32a88d). The label is painted by the battlefield painter, which stays clipped to the arena (ruling C-12 effects_clip covers only the overlay passes). The supplemental wave-0 before still shows the same cut, so it predates wave 1 | polish | vfx-lighting | fixed | 160cde7 | target/production-after/wave1/review-ui-battle/normal/max-lanes-small.png#c73cec3d0b69, target/production-after/wave1/review-ui-battle/reduced/max-lanes-small.png#30b813387207 | |
| CF-1-12 | 1 | weapon-classes-shotgun | both | both | Every missed shotgun pellet draws its own MISS at the field's right edge, so one shot paints a column of 6 MISS labels, and two of them overlap where pellets leave through the same edge point (target/production-after/wave1/normal/weapon-classes-shotgun.png#fa160eb031ec, weapon-classes-shotgun-small.png#38a732fc39a8). Reduced mode shows the same column. W1-54 registered the scene; the per-pellet popups predate it. The stills hold no MISS at all once the capture runs slower (wall-clock expiry), so the after evidence is the fixed-timestep motion frame of the first shot. 71c3f00 draws one MISS per shot, not one per pellet. a312872: a new MISS replaces any active MISS drawn at the same point, so steady-aim single-projectile fire refreshes one MISS instead of stacking a column. The after evidence predates a312872 | polish | vfx-lighting | fixed | 71c3f00, a312872 | target/production-after/wave1/review-ui-battle/motion/weapon-classes-shotgun-normal-normal/frame-0001.png#80318eba556e, target/production-after/wave1/review-ui-battle/motion/weapon-classes-shotgun-normal-reduced/frame-0001.png#a2d7faf8b958, target/production-after/wave1/review-ui-battle/motion/weapon-classes-shotgun-small-normal/frame-0001.png#39b3ff1a93e5 | |
| CF-2-01 | 2 | skills-survival | both | both | W2-41's Spray control (`smg_spray_control`, -660, 0) sits where the long `pistol_training` → `smg_training` wire arrives. That route enters SMG proficiency from the right along y = 0, which is where the new `smg_training` → `smg_spray_control` wire leaves, so the 70-unit stub draws over the start of the real edge (one line with an arrowhead at each end) and its "from Survivor foundations" label, anchored at the stub's far end, lies on that edge. At 1440x900 the label ends on Spray control's incoming arrowhead; at 1050x700@1.35 (11-pt minimum label font at the compact zoom) "from Survivor fou…" runs under the Spray control icon. Reduced draws the same. Found by the W2-41 wave-2 close capture (waves.md W2-41 row): target/production-after/w2-review-ui2-upgrades-b/normal/skills-survival.png#db050482c760, skills-survival-small.png#7cf635516ae6, reduced/skills-survival-small.png#8cc16d6658db. The fix is wire routing or stub-label placement in `upgrade_graph` (Graph::show is progression-tree's); it needs a `waves.md` slot | minor | progression-tree | open | | | |
<!-- capture-findings:end -->

### 10.1 Camera capture review (W1-60)

Ruling C-12, after C1-C4 (W1-43..W1-46) and the sidecar (W1-53). Recorded 2026-11-06 on `fa8ef3e`.

- **Run:** `scripts/capture_review.ps1 -Modes normal,reduced -ContinueOnError -OutputDirectory target/production-after/w1-60` (not committed), scenes battle, construction, max-lanes, peak, progressed-peak and operations-observer (the C-12 set) plus every other wave-0 Battle still (battle-female, progressed-battle, quality-loot, reticle-reload, damage, damage-4x, critical-npc, intro, npc, npc-expanded, boss, ordnance, arid, tropical). **80 of 80** captures, 0 failed, executable sha256 `fabb770f5d5d…`, `windows_scale` 2, `player_logs_unchanged` true. Sidecar checks: 80 sidecars present; `construction_keeps_battle_arena` 4/4 pass; `max_lanes_keeps_pre_zoom_arena` 4/4 pass; `hud_rows_at_most_2` 38 pending (null until C-10, wave 3).
- **Pairing:** each after still is paired with `target/production-before` (plus `supplement`) by (mode, scene, size), using `before_proxy` from the run's scenes.tsv (operations-observer pairs with operations). compare_captures.py does not exist until W7-11, so a scratch std-lib-plus-PIL pairing did the same pairing. It flagged **80 of 80** pairs as changed; none is byte-identical and none is missing a before still. "Changed %" counts pixels whose largest channel delta is over 8. Every reduced still differs from its normal twin (`identical_to_normal` false).
- **Field geometry:** the "before" field is the wave-0 `clearing_rect(battle_arena(canvas, construction), lanes, width)` recomputed for the sidecar's lanes, width and overlays. The "after" field is the sidecar `arena_rect` (= `f.battle.arena_rect` = `layout.field.rect`). Every after field matches `fit_field` of its geometry in the arena (intro under its 92-pt hint band). The effects clip is the arena plus 60 pt above it (`effects_rect` top 64) in all 76 Battle sidecars.
- **No arena move at 17x30 or 11x20:** max-lanes and progressed-peak are 17x30. At both sizes in both modes their sidecar `arena_rect` equals the pre-zoom arena (1392x692 / 729.8x310.5 at canvas inset 24/124/24/84), with zoom 1. Their 0.0-0.25% pixel change is outside the camera's reach. It is the reticle drawn at the live OS pointer, which sat elsewhere during the wave-0 run (nearly all of the 7,026 changed pixels in max-lanes at 1440x900 normal); the per-sprite animation pose of a few enemies; and a few points of horizontal offset in the status line. A W1-53 (`b32df96`) to W1-60 comparison of the same scenes shows the same kind of change. No registered still has an 11x20 field, so the 11x20 sentinel is proven by the pins `art::camera::tests::fit_field_keeps_every_filling_geometry_bit_identical` and `ui::battle::layout::tests::battle_layout_matches_todays_geometry_at_17x30`. Both were re-run green on `fa8ef3e`, together with `construction_panel_overlays_instead_of_shrinking_the_field`.
- **Reasons** (C-12): **zoom** = uniform zoom-to-fit; **overlay** = construction overlay; **headroom** = effects headroom. A row's Review column is what was inspected beyond the sidecar numbers: side-by-side before/after views, and contact sheets of the other Battle stills at 1050x700@1.35 normal and 1440x900 reduced.

<!-- camera-review:begin -->
| Scene | Geometry | Size | Mode | Changed % (normal / reduced) | Field before → after, pt (zoom) | Arena move | Reason | Review |
|---|---|---|---|---|---|---|---|---|
| battle | 7x16 | 1440x900 | both | 21.58 / 21.55 | 742.4x440.4 → 1166.6x692 (1.571) | yes, intended | zoom; headroom | Field fills the arena height, centred; HUD, selector and bar unmoved; pause banner now over the bottom lane (CF-1-01) |
| battle | 7x16 | 1050x700@1.35 | both | 15.55 / 15.58 | 389.2x197.6 → 611.6x310.5 (1.571) | yes, intended | zoom; headroom | As at 1440x900; the reduced still keeps the same field |
| construction | 7x16 | 1440x900 | both | 27.27 / 27.18 | 608.0x440.4 (arena cut by 252) → 1166.6x692, same rect as battle (1.571) | yes, intended | zoom; overlay | The panel overlays the right of the field, the field does not shrink; INCOMING chevrons mark covered spawns; Save/Clear country plan inside the panel (CF-0-03 fixed) |
| construction | 7x16 | 1050x700@1.35 | both | 26.83 / 27.29 | 254.8x197.6 → 611.6x310.5, same rect as battle (1.571) | yes, intended | zoom; overlay | As at 1440x900; four INCOMING chevrons; the midpoint label overlaps the panel edge (CF-1-02) |
| max-lanes | 17x30 | 1440x900 | both | 0.14 / 0.11 | 1392x692 → 1392x692 (1.0) | no (sidecar equals pre-zoom arena) | zoom (no-op on a filling field); headroom | Field pixel-placed as before; residual is the OS-pointer reticle and sprite poses only |
| max-lanes | 17x30 | 1050x700@1.35 | both | 0.22 / 0.18 | 729.8x310.5 → 729.8x310.5 (1.0) | no (sidecar equals pre-zoom arena) | zoom (no-op on a filling field); headroom | As at 1440x900 |
| peak | 7x16 | 1440x900 | both | 29.68 / 30.09 | 742.4x440.4 → 1166.6x692 (1.571) | yes, intended | zoom; headroom | 52-enemy field readable at 1.571x; boss tag and bar inside the field; banner over lane 7 (CF-1-01) |
| peak | 7x16 | 1050x700@1.35 | both | 21.51 / 21.91 | 389.2x197.6 → 611.6x310.5 (1.571) | yes, intended | zoom; headroom | As at 1440x900; lane 7 barricade under the banner (CF-1-01) |
| progressed-peak | 17x30 | 1440x900 | both | 0.25 / 0.24 | 1392x692 → 1392x692 (1.0) | no (sidecar equals pre-zoom arena) | zoom (no-op on a filling field); headroom | Field unmoved; residual is the reticle, sprite poses and a few-point status-line offset |
| progressed-peak | 17x30 | 1050x700@1.35 | both | 0.00 / 0.18 | 729.8x310.5 → 729.8x310.5 (1.0) | no (sidecar equals pre-zoom arena) | zoom (no-op on a filling field); headroom | Normal still differs from wave 0 in 41 pixels only |
| operations-observer | observer box | 1440x900 | both | 32.39 / 32.41 | proxy operations has no observer → observer field 742.6x360 (box height 360) | n/a (new window) | zoom (observer uses the same fit) | Live battlefield window fits its field to the box (`observer_arena_rect`), centred, no hint band; plan buttons and Trap/Wall/Turret in the window |
| operations-observer | observer box | 1050x700@1.35 | both | 74.73 / 74.72 | proxy operations has no observer → observer field 613.3x318.5 (box height 318.5) | n/a (new window) | zoom (observer uses the same fit) | As at 1440x900; the window covers most of the Operations tab at this size, as an opened observer should |
| intro | 3x12, hint band | 1440x900 | both | 14.45 / 14.43 | 556.8x188.7 → 1392x471.8 under the 92-pt band (2.5) | yes, intended | zoom; headroom | Field fills the width below the first-defense callout, which no longer covers it (CF-0-02 fixed) |
| intro | 3x12, hint band | 1050x700@1.35 | both | 7.72 / 7.75 | 291.9x86 → 729.8x215.0 under the band (2.5) | yes, intended | zoom; headroom | As at 1440x900, except the READY banner, which here sits at y 400-432 pt inside the field and covers about half of the bottom lane (CF-1-01); Rook drawn at 2.5x (sheet upscale, pending human check) |
| progressed-battle | 9x18 | 1440x900 | both | 19.19 / 19.14 | 835.2x566.2 → 1020.8x692 (1.222) | yes, intended | zoom; headroom | Field fills the height; INCOMING cues and boss tag inside |
| progressed-battle | 9x18 | 1050x700@1.35 | both | 13.93 / 14.68 | 437.9x254.1 → 535.2x310.5 (1.222) | yes, intended | zoom; headroom | A top-lane loot label draws into the 60-pt headroom and ends under the right HUD panel; that is the headroom by design |
| battle-female, quality-loot, reticle-reload, damage, damage-4x, critical-npc, npc, npc-expanded, boss, ordnance, arid, tropical | 7x16 | 1440x900 | both | 9.53-32.57 | 742.4x440.4 → 1166.6x692 (1.571) | yes, intended | zoom; headroom | Same field as battle in all 24 stills; aim lines, reticle, damage numbers, ordnance rings and theme ground follow the zoom; banner over the bottom lane in damage and boss (CF-1-01) |
| battle-female, quality-loot, reticle-reload, damage, damage-4x, critical-npc, npc, npc-expanded, boss, ordnance, arid, tropical | 7x16 | 1050x700@1.35 | both | 6.59-24.15 | 389.2x197.6 → 611.6x310.5 (1.571) | yes, intended | zoom; headroom | As at 1440x900; the npc-expanded decision panel still covers the middle of the field, as it did before |
<!-- camera-review:end -->

- **Human checks:** "Camera feel: a 3x12 field at 2.5x zoom" and "Sheet upscale on 3-lane sprites at 2.5x" were not done in this review and stay **pending** in §11 and, as "pending human check", in `docs/ACCEPTANCE.md`. Captures for the upscale check are intro and intro-small in this run, plus the 3x12 weapon-classes stills from W1-54.
- **Ledger:** CF-0-02 and CF-0-03 are fixed. The review opened CF-1-01 (minor) and CF-1-02 (polish), both owned by vfx-lighting, and the review fix opened CF-1-03 (minor, qa-captures; vfx-lighting since R17). CF-1-01 and CF-1-02 are placement rows for marks drawn over the field (the banner and the midpoint label), not camera rows: the camera rows are CF-0-02 (the fit) and CF-0-03 (the overlay without rescale), and both are closed. CF-1-03 is capture tooling.
- **Scheduling:** W7-11's `check_capture_findings.py --final` fails on any row that is not fixed or "approved by user", so each row has a commit in `waves.md` (W1 review): W3-29 fixes CF-1-01 and CF-1-02, W3-30 fixes CF-1-03, W3-31 fixes CF-1-07 and W3-12 fixes CF-1-08. The owner, fail-first test and latest slot are:
  - CF-1-01, owner vfx-lighting: `pause_banner_center` is C-3 geometry and C-11 bars ux-flow from redefining it, so the fix is a BattleLayout change by vfx-lighting, in wave 3 beside the ux-flow banner work (W3-14) as design-perf-architecture.md:441 asks (W3-29). Fail-first `pause_banner_never_covers_the_bottom_lane` over the zoomed 7x16 and the 3x12 hint-band fields at both sizes (design-ux-flow.md:376). The new place is the owner's choice: design-vfx-lighting.md:108's letterbox rule alone does not help a height-limited 7x16 field, which has no bottom letterbox.
  - CF-1-02, owner vfx-lighting: same commit (W3-29). Fail-first `midpoint_label_clears_the_construction_panel` at 1050x700@1.35 (the label rect and `construction_panel` do not intersect).
  - CF-1-03, owner vfx-lighting since R17 (first qa-captures; W3-30): before the W3-27 capture matrix is reviewed. Fail-first `review_capture_aim_ignores_the_os_pointer`: with a review-capture launch and a hover position over the field, `f.battle.aim` stays at the scene's pinned aim and no hover reticle or inspector draws.
  - CF-1-07 (opened by W1-69, not by this review), owner vfx-lighting (W3-31): beside W3-29, before the W3-27 capture matrix is reviewed. Fail-first `wall_hit_numbers_clear_the_wall_face_and_its_stack_at_small_size`: at 1050x700@1.35 on the progressed-peak field, no wall-hit popup rect intersects the struck wall's face rect or the drawn body of the front enemy in its stack. Done sets CF-1-07 to fixed with after-capture evidence from progressed-peak and stress motion at 1050x700@1.35.
  - CF-1-08, owner ux-flow (W3-12): fixed by the battle bar at most two rows.

### 10.2 Damage pop motion (W1-64)

The stepped pop of CF-1-04, recorded 2026-11-06 with `scripts/capture_motion.ps1 -Scene damage -OutputDirectory target/production-after/w1-64/damage-motion` (not committed; manifest v2, `reduced_effects` false, 1440x900 at scale 1.0, `windows_scale` 2), and in the W1-64 review with `-Viewport small -OutputDirectory target/production-after/w1-64/damage-motion-small` (1050x700 at scale 1.35, `windows_scale` 2, so 2.7 pixels per point).

<!-- motion-review:begin -->
| Scene | Viewport | Mode | Directory | Frames | Simulation ticks | Change | Review |
|---|---|---|---|---|---|---|---|
| damage | normal | normal | target/production-after/w1-64/damage-motion | 24 (native 60-152) | 2339 → 2431 | Pop scale stepped to x0.85 / 1.0 / 1.15 / 1.3; whole-pixel glyph placement in every face | Numbers draw with their outline at the stepped sizes ("29" in frame-0004); rise and fade unchanged; the feel of the step at 60 fps is a human check |
| damage | small | normal | target/production-after/w1-64/damage-motion-small | 24 (native 60-152) | 2339 → 2431 | As at normal, at the fractional 2.7 pixels per point | "29" draws whole and outlined at the stepped size in frame-0004, over the loot label in the 60-pt headroom; the HUD hint line under the second button row predates W1-64 (same in the w1-60 damage-small still, C-10; capture row CF-1-08) |
<!-- motion-review:end -->

### 10.3 Wave-1 capture review (W1 review)

Recorded 2026-11-07 on `0af829a` (HEAD after the W1 review commits), debug build, default features.

- **Stills:** `scripts/capture_review.ps1 -Modes normal,reduced -ContinueOnError -NoBuild -OutputDirectory target/production-after/wave1-review-stills` (every Still in --list-scenes, no -Scenes), then its `normal/` and `reduced/` folders, captures.json and scenes.tsv copied to `target/production-after/wave1/` (that folder already held wave-1 pacing and smoke logs, and the script refuses a non-empty folder). **236 of 236** captures (59 stills x 2 sizes x 2 modes), 0 failed, executable sha256 `eff727342458…`, `windows_scale` 2, `player_logs_unchanged` true. Sidecar checks: 236 `window_matches_request`, 236 `sidecar_present` and 236 `sidecar_echoes_request` pass; `construction_keeps_battle_arena` passes 4 of 4 (1440x900 [136.686, 124, 1303.314, 816], 1050x700@1.35 [83.078, 124, 694.704, 434.531], both modes); `max_lanes_keeps_pre_zoom_arena` passes 4 of 4 (17x30 at canvas inset 24/124/24/84, both sizes and modes); 58 `hud_rows_at_most_2` rows stay pending until W3 (C-10).
- **Motion:** `scripts/capture_motion.ps1 -NoBuild` for each of the 30 scenes the binary marks motion=true, at `-Viewport normal` and `small`, with and without `-ReducedEffects`, into `target/production-after/wave1/motion/<scene>-<viewport>-<mode>`: **120 of 120** runs exit 0, each with 24 frames, a version-2 manifest echoing reduced_effects, viewport and scale, and `windows_scale` 2. Reduced menu, menu-female and survivor-select are one frame repeated (static, as the script requires); reduced intro has 4 distinct frames over 24. Frame-to-frame mean luminance steps (0-255, frames downsampled to 360x225) are at most 1.51 (weapon-classes-shotgun small normal, the tracer fan) and 1.38 (gesture-throws small normal, the grenade and Molotov bursts); the gesture-throws reduced twin steps at most 0.33, and no reduced sequence steps more than 1.31 (shotgun small reduced, the same fan without a muzzle flash).
- **Pairing:** a scratch PIL script paired each after still with `target/production-before/{normal,reduced}/` or `supplement/` by file name, or by the scene's `before_proxy` for the 13 W1-54 scenes (results-defeat → results, save-error → world, night-checkpoint → arid, weapon-classes-* → battle, weapon-classes-turret → construction, fx-stress → max-lanes, battle-mid → battle, operations-observer → operations). Every still has a pair; none is byte-identical to its before. "Changed %" counts pixels whose largest channel delta is over 8. Every normal pair and its reduced pair were viewed side by side, plus 1:1 crops where text or marks met.
- **Camera fit:** every Battle sidecar `arena_rect` is the fit of its geometry: 7x16 1166.6x692 (1440x900) and 611.6x310.5 (1050x700@1.35), 9x18 1020.8x692 / 535.2x310.5, 3x12 under the hint band 1392x471.8 / 729.8x215, 5x14 (weapon-classes-turret) 1392x674 / 729.8x302.5, battle-mid 7x16 under the band 1011.6x600 / 430.4x218.5. Full lanes and full width are visible in every field; fields with fewer lanes are zoomed in; 17x30 (max-lanes, progressed-peak, fx-stress) keeps the pre-zoom arena; construction keeps the battle rect.
- **Stacked enemies (W1-69):** peak, progressed-peak, max-lanes, fx-stress and the stress and peak motion show wall and line stacks as 2-3 distinct bodies with one contact band per lane and one windup ring per stack, as CF-1-06 records. At 1050x700@1.35 the wall-hit numbers cover the stacks (CF-1-07, open).
- **Reduced effects:** every reduced still echoes reduced_effects true. Reduced stills draw no additive bursts (ordnance shows the Molotov ring without the fireball, gesture-throws keeps the arc and target cell), and the reduced motion meets the rules above.
- **`{:?}` labels:** none of the 236 stills shows a Debug-formatted type name. Debug-formatted words that read as plain English ("Guarding", "Deployed", slot names) remain on Operations and Equipment; their removal is W3-26.

Accepted differences, with the reason (C-12 words where they apply):

| Scenes | Size | Mode | Changed % (normal / reduced) | Reason |
|---|---|---|---|---|
| battle, battle-female, quality-loot, reticle-reload, damage, damage-4x, critical-npc, npc, npc-expanded, boss, ordnance, arid, tropical, peak, construction | both | both | 11.8-32.9 | zoom (7x16 at 1.571); headroom; overlay for construction; W1-69 positions (enemies overlap instead of queueing, so stacks and corpses lie elsewhere); pause banner over the bottom lane is CF-1-01 |
| progressed-battle | both | both | 14.6-19.5 | zoom (9x18 at 1.222); headroom; the loot label under the right HUD panel is CF-1-09 |
| intro | both | both | 8.5-14.8 | zoom (3x12 at 2.5 under the hint band); READY banner inside the small field is CF-1-01 |
| max-lanes, progressed-peak | both | both | 5.6-8.6 | no arena move (sidecar equals the pre-zoom arena); the change is W1-69 (bodies stack at walls and the line instead of queueing in lanes) plus CF-1-03 (OS pointer) and sprite poses |
| weapon-classes-pistol, -smg, -shotgun, -rifle, -marksman, -machine-gun, night-checkpoint, battle-mid | both | both | 12.3-26.0 against proxies | new W1-54 scenes against a different proxy scene: 3x12 and 7x16 fields under the hint band, new loadouts and fixtures. night-checkpoint draws the arid ground without a night grade: theme grades are W5-01, so the scene is a pre-W5 baseline |
| weapon-classes-turret, fx-stress, operations-observer, results-defeat, save-error | both | both | 0.8-74.7 against proxies | new W1-54 scenes: turret on a 5x14 field; fx-stress is the 400-enemy stress fixture; the observer window (10.1); a defeat report; the save-failure notice (its clipped buttons are CF-1-10) |
| menu, menu-female, survivor-select | both | both | 0.2-2.2 | whole-pixel glyphs (CF-1-05); menu animation phase |
| world, campaign, briefing, briefing-npc, character, character-max, survivor, armory, leveled-armory, quality-armory, skills, skills-survival, skills-ordnance, skills-search, engineering, results, operations, settings, history, history-empty, active-camp, journey-victory, journey-liberated, journey-upgrades | both | both | 0.3-12.8 | the "Practice scenario" status line and Dismiss button are gone (CF-0-04), so pages sit higher or keep the footer free; whole-pixel glyphs (CF-1-05); wave-1 copy and controls (defender status "On expedition", settings v2 rows such as the Aiming & firing section and the Brace binding). No clipping or overlap beyond the rows above |

- **Ledger:** this review opened CF-1-09 (minor, vfx-lighting), CF-1-10 (minor, ux-flow), CF-1-11 (polish, vfx-lighting) and CF-1-12 (polish, vfx-lighting). CF-1-09 is the only one caused by a wave-1 change (the W1-44 effects headroom); CF-1-10, CF-1-11 and CF-1-12 predate wave 1 and first show in W1-54 scenes or at 17x30. All four were fixed in the W1 review (CF-1-09 45195d0 with follow-ups 45597c7 and 3924a7c, CF-1-10 e86d9e7, CF-1-11 160cde7, CF-1-12 71c3f00 with follow-up a312872; rows marked fixed by f0d25c6 and a4328d4; the after evidence of CF-1-09 and CF-1-12 predates their follow-ups), so none needs a `waves.md` slot.

---

## 11. Human checks pending

These have no automated substitute. Each is logged as "pending human check" in `docs/ACCEPTANCE.md` and here if not done; none blocks a wave.

| Check | Wave | Protocol | Status |
|---|---|---|---|
| Camera feel: a 3x12 field at 2.5x zoom (on-screen enemy speed rises 2.5x) | 1 | Play the first defense at both sizes | pending |
| Stepped damage pop (CF-1-04, W1-64) | 1 | Watch damage numbers at 60 fps in a live battle, normal mode, both sizes | pending |
| Whole-pixel text placement (CF-1-05, W1-64) | 1 | Compare `text-subpixel-on-b` with `text-after` (menu, battle, leveled-armory, both sizes), then read body text live at 1050x700@1.35 | pending |
| Sheet upscale on 3-lane sprites at 2.5x | 1 | Inspect battle captures at both sizes | pending |
| Weapon-classes review (silhouette and readability per class) | 5 | `weapon-classes-*` captures | pending |
| Night and storm look (grade, lightning, cue readability) | 5 | `night-checkpoint`, `storm`, `dawn-reference` captures and motion | pending |
| Listening pass | 6-7 | PLAYTEST_GUIDE "Listening pass" section | pending |
| Click-to-photon run | 7 | PLAYTEST_GUIDE protocol: 240 fps phone, 20 shots per row in 3 rows (default VSync, Low latency, VSync off); targets median ≤ 50 ms default and ≤ 34 ms Low latency | pending |
| Integrated-GPU run | 1-7 | `measure_performance.ps1 -Adapter "Intel(R) Arc(TM) Pro Graphics" -VsyncOff`; `PERF_BUDGET.json hardware.integrated_gpu.status`. That is the name wgpu gives this machine's integrated adapter: `-Adapter` sets `WGPU_ADAPTER_NAME`, Bevy picks the adapter whose lowercased name contains it, and the W1-68 run `target/perf/w1-68/playtest-novsync-arc` reports that adapter. The script marks a report invalid when its adapter does not match the request (R23) | pending |

---

## 12. Outcome

To be filled by the orchestrator at W7-12: branch, commits, the change made for each finding, the before/after numbers in §9, verify results, pending human checks, and then a request before fast-forwarding local `main`.
