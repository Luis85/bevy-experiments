# Production pass: Last Light (horde-defense)

Status: **design frozen, ready for wave 1** (2026-11-05). Brief: `docs/plans/2026-11-04-production-pass-prompt.md`. Branch `claude/horde-production-pass`, worktree `.claude/worktrees/horde-production-pass`, cut from local `main` at `7bd5de8`.

This file is the plan of record. The detail lives in `docs/plans/production-pass/`:

| File | Contents |
|---|---|
| `schema-v7.md` | The final frozen v7 schema: every saved field, migration step, validation rule, command, catalogue change and settings field, plus the digest ledger. |
| `waves.md` | The master wave plan, waves 1-7, one row per commit with owner, fail-first tests and dependencies. |
| `acceptance.md` | The acceptance matrix: every brief criterion, user restatement and ruling mapped to its proof and wave. |
| `design-<remit>.md` (10 files) | Each expert remit's synthesized spec, followed by the integration rulings that supersede parts of it. |

Precedence when documents disagree: user decisions, then orchestrator rulings R1-R11, then `schema-v7.md` / `waves.md` / `acceptance.md`, then the "Integration rulings" section of each design doc, then the synthesized spec text above it.

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
- **Goal-1 interface target** (30% below wave 0, same profile and scene): progressed-peak interface mean **≤ 1.129 ms** at 1440x900 and **≤ 1.030 ms** at 1050x700@1.35 (floor of 0.70 × 1.6128 and 0.70 × 1.4719). These numbers are pinned in `docs/PERF_BUDGET.json` by `perf_budget_baseline_is_wave0`.
- CPU gate (new, not measurable at wave 0): with `-VsyncOff`, progressed-peak and stress CPU (main + render-world) p99 ≤ 10 ms and 0 frames over 50 ms.

### 1.5 Before-captures (wave 0)

- `target/production-before/{normal,reduced}/`: **156 of 164** stills (41 scenes × 2 sizes × 2 modes).
- The 8 failures are `progressed-battle` and `leveled-armory` at both sizes in both modes: a panic at `crates/horde_game/src/ui/review/fixtures.rs:138:14`. Fixed in W1-05; their before evidence is captured by W1-06 on the fixed build (logged as CF-0-01).
- None of the brief's 11 new review scenes exist yet; they are registered in W1-54, W3-27 and W5-09/10, with `before_proxy` pairs.

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
| 14 | Level-up | A coin grant (ledger design). | W1-24, W2-22 |
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
| R7 | save-determinism re-blesses GOLDEN/TRACE once per wave in a dedicated commit with a reason per digest; moves expected in waves 2 and 4; a wave-1 format-only move only if the v7 schema makes it unavoidable. |
| R8 | Single owners: pacing harness v2 = progression-tree; per-archetype telemetry = enemy-balance; SHA-256, provenance and orphan helpers and `scripts/lib/Native.ps1` = qa-captures; `measure_performance.ps1` = perf-architecture on Native.ps1; storm tagging = a vfx presentation table only; regional checkpoints derived (CD presentation lines, EB Regional boss rank HP and reward x1.2 in wave-4 balance, no theme-rule change); `save_v7.json` cut at the wave-1 close and frozen; stress scene cap only behind a review/test seam (perf). |
| R9 | "50 sales produce 1 cue" means one auto-sell or stash-apply batch of 50 items gives 1 cue; single manual sales may each cue. |
| R10 | UI recruit and training prices read the core price function. |
| R11 | `Game::sale_eligibility` exists as the facade used by every sell path. |
| RL | Lightning uses wall-clock pacing; storm stages in non-storm themes get a static storm grade in reduced mode. |

### 2.5 Still open for the user

| Item | Default if unanswered |
|---|---|
| P8: create the Bevy-free path crate `crates/horde_audio` in wave 6 (A6-00); Cargo.lock gains one path entry, no external dependency | Create it |
| Lane-pan reading: along the lane, listener-relative, at most 3.1 dB level difference (audio, non-blocking) | Proceed with this reading |
| Any capture finding deferred at goal 10 | Needs "approved by user YYYY-MM-DD" in chat; the orchestrator cannot self-approve |
| Fast-forwarding local `main` at the end | Ask first |

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
| PT7 | M | Level-ups give no reward. | campaign/progression.rs:684-704 | W1-24, W1-25, W2-22 |
| PT8 | M | No rank 5/10/15/20 milestones. | skills.json gates | W1-24, W2-22 |
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
| EB9 | M | Boss armor is constant; no phases; no capstone. | director.rs:195-205; sim/combat.rs:102-107 | W2-36, W4-01 |
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
| Native exes in 5.1 | `scripts/lib/Native.ps1` with Start-Process and redirected streams; taskkill /T on timeout (S14) | Stop-Process on timeout (leaves rustc children running); Pester (in-box 3.4 is outdated) |
| Scene list | A ui-free `review_scenes` registry with `--list-scenes`; unknown names rejected at launch; matrix.rs:358 kept for Workshop prefixes | A registry embedding Screen/Tab (ui is private); `include_str!` sync tests over the ps1 lists; making matrix.rs:358 `unreachable!` (panics Workshop scenes) |
| fixtures.rs:138 panic | Idempotent `enable_legacy_ledgers` before progressed clears (W1-05), then supplemental before-captures (W1-06) | No supplemental before set (the crashed scenes would have no before evidence) |
| Sidecars | `ReviewFrameFacts` (screen, tab, arena_rect, effects_rect, hud_rows, ...) | No sidecar (construction rescale and HUD rows unverifiable); collecting amber primaries from every screen (invasive) |
| Archive | **Ruling S9/U7:** mirrored `git mv` of the full user list | Source-only tagging of strips and keyart (the user approved the archive) |
| Storm | **Ruling W-8:** vfx's table | `storm: bool` on themes; an 11th storm theme; a hashed storm chance |
| Build stamp | `option_env!(LASTLIGHT_BUILD_COMMIT)` on the menu, crash report, startup log and `--version` (W7-01) | A SaveEnvelope stamp (GOLDEN churn); build.rs running git |
| Root dev.ps1 | **Ruling R6/S16:** untouched | Annotating README.md:165 and VALIDATION.md:147 |
| Committed pairs | Downsampled 2:1, at most 40 MB, CF ledger in this plan (§10) | Raw 2x PNG pairs with a 64 MB budget |

---

## 6. Final v7 schema

Full table: `docs/plans/production-pass/schema-v7.md` (frozen by `save_v7.json` at W1-63). Summary:

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
| 1 | 0 | 0 | unchanged | 2 |
| 2 | F, T, C (+ G if a pick changes), one re-bless W2-49 | 0 expected | unchanged | 3 |
| 3 | 0 | 0 | unchanged | 3 |
| 4 | G, one re-bless W4-08 | G | unchanged (charges pin re-blessed with reason only if it moves) | 4 |
| 5-7 | 0 | 0 (PROGRESSED 0/4) | unchanged | 4 |

---

## 7. Master wave plan

Full commit list: `docs/plans/production-pass/waves.md`. Every commit is test-first and leaves the full verify green; one writer at a time; one commit per finding.

| Wave | Scope | Commits | Owners | Digest | Exit |
|---|---|---|---|---|---|
| 1 | Goal 1, camera (R1), sale eligibility, v7 schema + migration + fixtures, enemy_visuals v2, archive, PS 5.1, review panic fix | W1-01..W1-63 | all 10 | 0 moves | PERF gates pass (W1-61); `save_v7.json` frozen; camera capture review logged (W1-60); GOLDEN 0/6, TRACE 0/6 |
| 2 | Auto-sell, takeover, SelectCountry, scaling, archetypes, elites, bosses, progression and skills | W2-01..W2-49 | all but audio | one re-bless (W2-49) | Pacing and smoke before/after; stress re-gate; visual twins for audio |
| 3 | Goal 2 and the UI for goals 3-4 (horde_game plus pure core reads) | W3-01..W3-28 | ux-flow, loot-economy, progression-tree, enemy-balance, vfx-lighting, qa-captures | 0 | Full-loop headless test at both sizes; W3 capture matrix reviewed |
| 4 | Balance (goals 5-6) and the goal-3 numbers | W4-01..W4-11 plus tuning loops | enemy-balance, progression-tree, loot-economy, campaign-defenders, save-determinism | one re-bless (W4-08) | Every ignored `pacing_targets` assertion passes in release, with before/after tables |
| 5 | Goal 7 graphics (horde_game only) | W5-01..W5-12 | vfx-lighting, qa-captures, perf-architecture, save-determinism | 0 | fx-stress within budget; reduced MOTION gates; human review logged |
| 6 | Goal 8 audio | A6-00..A6-19 | audio, save-determinism | 0 | -23 ±2 LUFS / TP ≤ 0.95 gate closed; listening pass logged pending |
| 7 | Goals 9-10 and docs | W7-01..W7-12 | qa-captures, audio, loot-economy, progression-tree, campaign-defenders, ux-flow, perf-architecture, save-determinism | 0 | Package verified under 5.1; `check_capture_findings.py --final` exits 0; Outcome written |

Ordering constraints that cross remits: perf's `ui/battle` split (W1-39) lands before vfx C2 (W1-44); SD's settings commit (W1-31) lands before perf's `render_plan` (W1-34); qa's archive (W1-57) lands before audio's NpcShot commit (W1-58); CD's takeover core (W2-11) lands before LE's H1 (W2-14); H1 lands before PT's progression settlement (W2-22); EB's spitter and boss commits land before vfx's visual twins (W2-28, W2-37).

---

## 8. Acceptance matrix

Full matrix: `docs/plans/production-pass/acceptance.md` (also copied into `docs/ACCEPTANCE.md` by W7-08).

| Group | Rows | Key numeric gates |
|---|---|---|
| Goal 1 | G1.1-G1.15 | -VsyncOff CPU p99 ≤ 10 ms and 0 frames > 50 ms (progressed-peak, stress); interface ≤ 1.129 / ≤ 1.030 ms; BattleScreen/BVM/HUD built {1,1,1} per pass; ≤ 48 battlefield primitives, 400 vs 52 enemies differ by ≤ 2; save clone remedy if p99 > 1 ms |
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
| progressed-peak | 1440x900 | interface mean (ms, VSync on) | 1.613 | ≤ 1.129 | | | | |
| progressed-peak | 1050x700@1.35 | interface mean (ms, VSync on) | 1.472 | ≤ 1.030 | | | | |
| progressed-peak | both | CPU p99 (ms, -VsyncOff) | n/a | ≤ 10.0 | | | | |
| stress (400) | both | CPU p99 (ms, -VsyncOff) | n/a | ≤ 10.0 | | | | |
| progressed-peak, stress | both | frames > 50 ms | 0 (VSync on) | 0 | | | | |
| peak | 1440x900 / 1050x700@1.35 | interface mean (ms) | 1.190 / 1.075 | informational | | | | |
| camera zoom delta (W1-47) | both | peak and progressed-peak, commit before C2 vs C2 | n/a | recorded | | | | |
| pipelined decision (W1-61) | both | on vs off, Repeat 3 | n/a | recorded | | | | |
| save clone (W1-50) | n/a | p99 (ms) at 1,000 / 2,500 / 5,000 items, 3 workers | n/a | remedy if > 1 ms | | | | |
| shipped `--smoke` | n/a | seconds | n/a | recorded | | | | |

### 9.2 Pacing and smoke

| Measure | Wave 0 | After W2 (W2-49) | After W4 (W4-08) |
|---|---|---|---|
| expert-deu first victory (1 seed / 20-seed median) | run 4 / (W2-01) | | |
| Tier 1 / 2 / 3 affordable | runs 3 / 5 / 7 | | |
| Tree exhausted vs Liberation(2) | run 24 vs (W2-01) | | |
| Unlock order Fortifications / Mara / Operations (median) | 4 / 14 / 14 (Recruitment) | | |
| Defeat rate per 20-run window after first victory | 0% after run 10 | | |
| Longest win streak after run 10 | 50 | | |
| LUX runs to liberate Easy / Normal / Hard | (W2-04) | | |
| Boss TTK median (s) | (W2-04) | | |
| Smoke Personal stages 1-5 / Npc stages 1-5 | see 1.3 | | |
| Strategy profile: liberations 2/4/10, coins per minute | (W2-43) | | |

### 9.3 Audio

| Measure | Before (A6-04) | After (A6-17) | Gate |
|---|---|---|---|
| 60 s reference mix, integrated LUFS (both themes) | about -38..-42 | | -23 ±2 |
| 60 s dense battle true peak (FS) | | | ≤ 0.95 |
| Runtime master limiter needed (R3) | n/a | yes / no | only if TP > 0.95 |

---

## 10. Capture findings ledger

Format owned by qa-captures and checked by `scripts/check_capture_findings.py` (vocabulary: Size `1440x900`, `1050x700@1.35` or `both`; Mode `normal`, `reduced` or `both`; Severity blocker/major/minor/polish; Status open/fixed/deferred; Deferral only as `approved by user YYYY-MM-DD`, given by the user in chat).

<!-- capture-findings:begin -->
| ID | Wave | Scene | Size | Mode | Finding | Severity | Owner | Status | Fix commit | After evidence | Deferral |
|---|---|---|---|---|---|---|---|---|---|---|---|
| CF-0-01 | 0 | progressed-battle, leveled-armory | both | both | Capture panics at ui/review/fixtures.rs:138:14; no before still (8 of 164 failed) | blocker | qa-captures | fixed | 299470a | target/production-after/w1-05/normal/progressed-battle-small.png#6bc210e3bed4 | |
| CF-0-02 | 0 | battle | both | both | 3x12 field uses about 40% x 27% of the arena (fixed-scale clearing) | blocker | vfx-lighting | open | | | |
| CF-0-03 | 0 | construction | both | both | Construction mode shrinks the arena by 252 pt | blocker | vfx-lighting | open | | | |
| CF-0-04 | 0 | battle, menu, leveled-armory | both | both | Non-practice review captures show "Practice scenario - progress is isolated from your campaign", and menu and camp reviews draw "Practice profile" (amber), "Separate practice saves" and the "Practice · separate campaign" footer | minor | qa-captures | fixed | 299470a, 3d6e5c0 | target/production-after/w1-05/normal/battle.png#d44738e6cb6a, target/production-after/w1-05-review/normal/menu.png#be3544456adb, target/production-after/w1-05-review/normal/leveled-armory.png#b6163c62d2a9 | |
<!-- capture-findings:end -->

---

## 11. Human checks pending

These have no automated substitute. Each is logged as "pending human check" in `docs/ACCEPTANCE.md` and here if not done; none blocks a wave.

| Check | Wave | Protocol | Status |
|---|---|---|---|
| Camera feel: a 3x12 field at 2.5x zoom (on-screen enemy speed rises 2.5x) | 1 | Play the first defense at both sizes | pending |
| Sheet upscale on 3-lane sprites at 2.5x | 1 | Inspect battle captures at both sizes | pending |
| Weapon-classes review (silhouette and readability per class) | 5 | `weapon-classes-*` captures | pending |
| Night and storm look (grade, lightning, cue readability) | 5 | `night-checkpoint`, `storm`, `dawn-reference` captures and motion | pending |
| Listening pass | 6-7 | PLAYTEST_GUIDE "Listening pass" section | pending |
| Click-to-photon run | 7 | PLAYTEST_GUIDE protocol: 240 fps phone, 20 shots per row in 3 rows (default VSync, Low latency, VSync off); targets median ≤ 50 ms default and ≤ 34 ms Low latency | pending |
| Integrated-GPU run | 1-7 | `measure_performance.ps1 -Adapter "Intel(R) Arc(TM) Graphics" -VsyncOff`; `PERF_BUDGET.json hardware.integrated_gpu.status` | pending |

---

## 12. Outcome

To be filled by the orchestrator at W7-12: branch, commits, the change made for each finding, the before/after numbers in §9, verify results, pending human checks, and then a request before fast-forwarding local `main`.
