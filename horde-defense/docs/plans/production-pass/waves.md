# Master wave plan (waves 1-7)

## Global rules for every commit
- **One commit per finding.** Writers run one at a time.
- **Test-first.** Each commit's named test fails on the parent commit. A characterisation pin must record a mutation or perturbation proof in the commit message.
- **Verify before committing.** Every commit leaves the full verify green: fmt, the 3 clippy configs, the workspace tests, the `-p horde-defense --no-default-features` and `--features dev` tests, `--smoke` and check_views.
- **Wave-close rule (R25, user decision 2026-11-07).** A wave closes when two review rounds in a row find nothing above minor, the perf acceptance and the full verify are green at HEAD, and every confirmed finding is fixed or explicitly ruled. Remaining minor findings carry into the next wave's review.
- **Digest moves.** A commit that moves a digest sets the provisional pin and its cause (P3). Only the wave's SD re-bless commit folds the pins.
- **Scope.** No root-repo file is touched (R6). The review checks `git diff --name-only main... | grep -v ^horde-defense/` is empty.
- **Scripts.** Scripts run under Windows PowerShell 5.1 and use `Native.ps1` once it exists (W1-02).
- **Captures.** Every wave closes with before/after captures at 1440x900@1.0 and 1050x700@1.35, in normal and `LASTLIGHT_REVIEW_REDUCED`. Pacing is re-run whenever balance moved.
- **Wave close.** The completeness critic checks PROVISIONAL is None, the capture ledger, and that every AC row of the wave has evidence.

---

## WAVE 1: goal 1, camera (R1), sale eligibility, the v7 schema plus migration plus fixtures, enemy_visuals v2, the archive, PS 5.1, the review panic fix

The full commit detail is in `wave1_commits`. Order and summary:

| Id | Owner | Title | Depends |
|---|---|---|---|
| W1-01 | save-determinism | v6 evidence commit on untouched v6 code: `save_v6.json`, recording fixtures, `settings_v0.json`, V6 pins | - |
| W1-02 | qa-captures | `scripts/lib/Native.ps1` and `run_script_tests.ps1`; `.gitignore` gets `/scripts/__pycache__/` | - |
| W1-03 | qa-captures | 5.1 lint (`repo_checks`) and ports of `capture_review.ps1` and `capture_motion.ps1` | W1-02 |
| W1-04 | qa-captures | Rust log isolation guard | - |
| W1-05 | qa-captures | Scene registry, `--list-scenes`, launch validation, the **fixtures.rs:138 panic fix**, practice status | - |
| W1-06 | qa-captures | Supplemental before-captures (7 scenes x 2 sizes x 2 modes) recorded in the plan | W1-05 |
| W1-07 | save-determinism | Determinism tooling and the provisional-pin ledger | W1-01 |
| W1-08 | save-determinism | Architecture test: presentation never reads core RNG (scanner self-test) | - |
| W1-09 | save-determinism | `crate::schema` predicates and the `crate::hash` mix/keyed/tags extraction | W1-07 |
| W1-10 | campaign-defenders | `defenders.rs` identity types (DefenderId, DefenderSeat, resolve, RECRUIT_IDENTITIES) and labels | - |
| W1-11 | campaign-defenders | `RecruitStatus::Defending`, `reserve_defender`/`release_defender`, tick_world predicate | W1-10 |
| W1-12 | campaign-defenders | `DefenseInProgress` / `DefenderUnavailable` errors and the Defending pre-checks | W1-11 |
| W1-13 | campaign-defenders | Attempt/Report `"defender"` seat and accessors; lifecycle copies the seat | W1-10 |
| W1-14 | save-determinism | **SAVE_VERSION 7**: v6_to_v7 (a)+(e), `Game::from_payload`, replay migrates old headers | W1-13 |
| W1-15 | save-determinism | Worker seat and appearance stamping at both deploy sites, plus worker-seat validation | W1-14 |
| W1-16 | campaign-defenders | Campaign `next_defender` and `reserved_recruit`; ParallelBattle `summary`; StrategyState `folded_outcomes`; Recruit `containments` (types) | W1-14 |
| W1-17 | save-determinism | D7 seat, Defending and reservation validation; D10 summary bounds; the `stage_takeover` seam | W1-11, W1-15, W1-16 |
| W1-18 | loot-economy | Characterisation: the old Campaign filter sells a recruit-equipped item | W1-01 |
| W1-19 | loot-economy | Route every sell path through `Game::sale_eligibility` / `plan_sale` (no behaviour change) | W1-18 |
| W1-20 | loot-economy | Strict SellItem, `DeletePreset`, the armory button is disabled with a reason, RULESET 2, the recording-divergence test | W1-19, W1-14 |
| W1-21 | loot-economy | Loot schema types, v6_to_v7 (b), validation, `loot_only` + `purchasable()` | W1-14 |
| W1-22 | loot-economy | Shared upgrade metric (moved from wave 2; P2) | W1-19 |
| W1-23 | loot-economy | `SetAutoSell`, `ApplyAutoSellToStash`, `Sold`, stash preview | W1-20, W1-21, W1-22 |
| W1-24 | progression-tree | Progression ledgers and v6_to_v7 (c)(d) | W1-14 |
| W1-25 | progression-tree | Receipts: `Report.progression_rewards`, RunRecord grant and milestone coins | W1-24 |
| W1-26 | progression-tree | Skill and content schema at neutral values | W1-09 |
| W1-27 | progression-tree (with enemy-balance) | CombatTelemetry fields | W1-09 |
| W1-28 | enemy-balance | Enemy-side saved fields, EventKind appends, MAX_WAVE_TIER 15, GAMEPLAY_ENEMY_CAP | W1-09 |
| W1-29 | enemy-balance | PresetExtras catalogue, `balance_extras_basis`, accessors | W1-09 |
| W1-30 | save-determinism | `V7_SAVED_KEYS` registry and `schema_v7` guards | W1-15 to W1-29 |
| W1-31 | save-determinism | Settings v2 fields, salvage loader, `read_settings` | W1-01 |
| W1-32 | perf-architecture | `docs/PERF_BUDGET.json` (10 ms gate, integrated GPU pending) and `perf_budget.rs` | - |
| W1-33 | perf-architecture | Frame report v3 on PerfProbe | W1-32 |
| W1-34 | perf-architecture | `render_plan`: present mode, low latency, pipelined; single startup settings read; live display apply | W1-31, W1-33 |
| W1-35 | perf-architecture | 400-enemy stress review seam and the `stress` screen | W1-28, W1-32 |
| W1-36 | perf-architecture | `measure_performance.ps1` rewrite on Native.ps1 | W1-02, W1-03, W1-33, W1-35 |
| W1-37 | perf-architecture | v3 re-baseline on the unchanged renderer | W1-36 |
| W1-38 | perf-architecture | Pure move: `battle_view` split | W1-37 |
| W1-39 | perf-architecture | Pure move: `ui/battle` split (ui/battle/mod.rs) | W1-37 |
| W1-40 | perf-architecture | Pure move: `art` split | W1-37 |
| W1-41 | perf-architecture | Pure move: `upgrade_graph` split | W1-37 |
| W1-42 | perf-architecture | BattleFrame: one BattleScreen, BattleViewModel and HudView per egui pass | W1-39 |
| W1-43 | vfx-lighting | C1: `art/camera.rs` fit_field and pins | W1-40 |
| W1-44 | vfx-lighting | C2: BattleLayout and main-battle wiring | W1-43, W1-39, W1-42 |
| W1-45 | vfx-lighting | C3: observer window fit | W1-44 |
| W1-46 | vfx-lighting | C4: INCOMING occlusion chevron | W1-44 |
| W1-47 | perf-architecture | Record the camera-zoom perf delta (the commit before C2 vs C2) | W1-44 |
| W1-48 | perf-architecture | Batched battlefield meshes. R14: the done_when's peak, progressed-peak and stress pair review is satisfied by the wave-close pair review (target/production-after/wave1 against target/production-before, 356 captures, CF ledger rows; plan §10.3) | W1-44, W1-42 |
| W1-49 | perf-architecture | Corpse sampling and the cached settled mesh | W1-48 |
| W1-50 | perf-architecture | Save-clone benchmark, `save_snapshot`, conditional remedy | W1-35, W1-30 |
| W1-51 | perf-architecture | Tighten the egui budgets | W1-48, W1-49 |
| W1-52 | vfx-lighting | `enemy_visuals.json` v2 loader (v1 still loads) | - |
| W1-53 | qa-captures | ReviewFrameFacts sidecar | W1-44 |
| W1-54 | qa-captures | New wave-1 review scenes | W1-05, W1-35 |
| W1-55 | qa-captures | check_views sources, 5.1 doc commands, the AGENTS.md target rule | W1-03 |
| W1-56 | qa-captures | `horde_core::sha256` (test-support) | - |
| W1-57 | qa-captures | Archive move, asset_audit, generator retarget plus `--check` (both `--check` runs join the docs/VALIDATION.md Full verify), CREDITS index | W1-56 |
| W1-58 | audio | Retire NpcShot and archive survival_npc_shot_0..2 | W1-57 |
| W1-59 | qa-captures | Motion manifest v2 and `-Viewport` | W1-03 |
| W1-60 | qa-captures | Camera capture review with written reasons | W1-45, W1-46, W1-53 |
| W1-61 | perf-architecture | Acceptance runs, the pipelined decision, shipped smoke timing | W1-34, W1-36, W1-48 to W1-51 |
| W1-62 | perf-architecture | Rewrite ARCHITECTURE.md as an ownership table plus the coverage gate | W1-38 to W1-46 |
| W1-63 | save-determinism | Wave-1 close: freeze `save_v7.json`, docs | W1-17, W1-23, W1-25, W1-28, W1-29, W1-30 |
| W1-64 | perf-architecture | Fixed damage-popup font sizes so battle frames stop re-uploading the egui font atlas (gates 2 and 1) | W1-61 |
| W1-65 | perf-architecture | Paint the ground once: skip patches under the field and cache ground/backdrop meshes (gate 1) | W1-64 |
| W1-66 | perf-architecture | One quad per hit flash via a packed white silhouette (gates 1 and 2) | W1-65 |
| W1-67 | perf-architecture | Build BattleFrame without per-enemy maps and clones (gate 1) | W1-66 |
| W1-68 | perf-architecture | Attribute the 0.5 s frames: between-frame gap, pipelined wait, per-frame event counts (gate 3 investigation) | W1-67 |
| W1-69 | enemy-balance | Enemies never block each other: no lane queue, overlap allowed, every enemy that reaches the line or a structure attacks (user decision, 2026-11-06); the wave-1 GOLDEN/TRACE re-bless | W1-68 |

W1-64..W1-68 were added after W1-61 blocked on the Goal-1 gates; W1-61 re-ran after W1-68 and W1-69 (W1-69, db25cd7, landed on W1-68's review commit 63168c1, before every W1-61 commit), so its acceptance, measured on 199d823, includes the overlap rule.

W1-64 done_when evidence (review fix, 2026-11-06): the W1-64 commit's probes ran on a shared machine (41-56% CPU from other processes) and missed render_schedule and cpu_frame. They were re-run with `measure_performance.ps1 -Scene all -Profile playtest -VsyncOff -Repeat 3`, the budget's repeat_acceptance, at about 22-24% load from other processes before the run. All 18 runs were valid and the script exited 0 with the new egui upload gate in place. Medians of 3 are given as 1440x900 / 1050x700@1.35. Progressed-peak: extract p95 0.328 / 0.291, render_schedule p95 2.644 / 2.370, cpu_frame p99 6.785 / 6.099. Stress: extract p95 0.324 / 0.340, render_schedule p95 2.803 / 2.976, cpu_frame p99 7.847 / 8.188. Every scene had 0 egui texture uploads and 0 frames over 50 ms. One stress repeat per size went above 3.0 ms render_schedule p95 (3.075 at 1440x900, 3.083 at 1050x700@1.35), and peak, which is not part of done_when, has a render_schedule median of 3.506 at 1440x900. Evidence is in `target/perf/w1-64/review-playtest-novsync`. Gate 3 (the 0.5 s frames) is still W1-68's.

W1-65 done_when evidence (2026-11-06): `measure_performance.ps1 -Scene progressed-peak -Profile dev -Repeat 3` (VSync on), before in `target/perf/w1-65/before-dev-vsync` and after in `target/perf/w1-65/after-dev-vsync`, medians of 3 as 1440x900 / 1050x700@1.35. Backdrop 0.230 / 0.110 ms before, 0.011 / 0.013 ms after. Battlefield paint 0.603 / 0.504 before, 0.472 / 0.481 after. interface_draw 1.436 / 1.212 before, 1.081 / 1.109 after: under 1.129 at 1440x900, still over 1.03 at 1050x700@1.35. egui mesh vertices 49,875 / 39,033 before, 45,056 / 37,750 after; indices 121,932 / 85,422 before, 105,685 / 80,977 after; textured quads 14,632 / 8,644 before, 11,941 / 8,149 after. Captures: 11 battle scenes (max-lanes and progressed-peak are 17x30) at both sizes, normal and reduced, two runs per build, in `target/production-before/w1-65-a`, `-b` and `target/production-after/w1-65`, `-b`. Captures are not repeatable run to run (poses and HUD timing), so pixels were compared where both runs of both builds agree. All 8 17x30 captures differ by 0 px there. Normal max-lanes-small is byte-identical in all four runs. 35 of the 44 captures differ by 0 px. The other 9 differ only at the survivor's aiming pose or at small HUD spots, which also differ between two runs of one build. The skipped patches lie wholly inside the field's opaque soil, so they cannot change any pixel.

W1-66 done_when evidence (2026-11-06): `measure_performance.ps1 -Scene progressed-peak` and `-Scene stress`, `-Profile dev -Repeat 3` (VSync on), before (with the new `scene_load.flashing_enemies` / `flash_quads` counts, mask runs still drawn) in `target/perf/w1-66/before-dev-vsync`, `-stress` and after in `target/perf/w1-66/after-dev-vsync`, `-stress`, medians of 3 as 1440x900 / 1050x700@1.35. One before progressed-peak 1440x900 repeat was unfocused twice (invalid), so that size has two valid before repeats. The share is small: these scenes have at most 1 flashing hostile in a frame (mean 0.05 progressed-peak, 0.03 stress). Flash quads were mean 9.69 and max 249 before, and mean 0.05 and max 1 after (stress 6.45 / 249 before, 0.03 / 1 after), so the flash quad count equals the flashing hostiles. Textured quads 11,941 / 8,149 before, 11,932 / 8,140 after (stress 13,855 / 10,064 before, 13,849 / 10,057 after). Battlefield paint 0.470 / 0.469 ms before, 0.468 / 0.473 after (stress 0.510 / 0.508, 0.501 / 0.504). egui mesh vertices 44,463 / 38,266 before, 45,243 / 37,708 after (stress 50,579 / 43,625, 51,136 / 43,606) and indices 103,759 / 82,742 before, 106,211 / 80,904 after (stress 110,306 / 86,472, 112,173 / 86,454). The vertex and index means move by up to about 2,500 between repeats of one build (HUD and pose timing), more than the at most 996 flash vertices of a peak frame. interface_draw 1.073 / 1.089 before, 1.104 / 1.090 after: still over 1.03 at 1050x700@1.35, no gate moved. The silhouettes fit the free column under the actor sheet, so the packed atlas stays 2898x1402. `flash_silhouette_matches_mask_runs` pins the geometry: every silhouette texel equals its 2x2 block's run coverage, the unposed quad, sampled NEAREST at every pixel centre that is not on a run edge, lights exactly the run pixels for every enemy frame (one size, 140x117 pt), and a posed quad (lean and scale) equals the unposed quad under the same `pose_vertices` transform that poses the runs. Captures: damage, damage-4x, battle, boss, peak, progressed-peak and max-lanes at both sizes, normal and reduced, two runs per build, in `target/production-before/w1-66-before-a`, `-b` and `target/production-after/w1-66-after-a`, `-b`. Recounted and inspected in the W1 review: pixels are compared only where both runs of each build agree (exact RGBA), and every differing pixel was grouped into clusters and inspected in before/after crops. The captures do hold hit flashes: in damage and damage-4x the struck brute is lit by the normal-mode additive flash at both sizes, and reduced damage-4x shows the reduced-mode warm tint, so the posed flash is compared at 1440x900 and 1050x700@1.35. Per pair (28):
- 0 px (10): normal boss-small, max-lanes-small, peak-small, progressed-peak-small; reduced battle-small, damage, damage-small, max-lanes-small, peak-small, progressed-peak-small.
- Flash silhouette edge pixels only (5): normal damage 6 px, damage-4x 3, damage-4x-small 1; reduced damage-4x 11 (two run-edge rows of 7 and 4 px), damage-4x-small 1. Each pixel lies on the flash's outline, where a run edge and the NEAREST-sampled silhouette edge fall on different sides of a pixel centre (before ground and after lit, or the reverse); no interior flash pixel differs. This is the done_when's antialias-edge allowance.
- A damage-popup digit only (1): normal battle-small 2 px, a channel delta of 1.
- OS pointer, survivor pose and popups (10): normal battle 241, boss 85, max-lanes 376, peak 83, progressed-peak 84; reduced battle 151, boss 100, max-lanes 37, peak 57, progressed-peak 72. The clusters are the hover reticle, its ammo readout and aim line at the OS pointer (CF-1-03), the survivor's aiming pose, and fading damage-popup digits. None lies on a flashing hostile.
- Hover plus the above (2): normal damage-small 23,329 px, of which 23,327 are the hovered "Toggle firing" button (OS pointer) and 2 are flash edge pixels; reduced boss-small 24,236 px, the same 23,327 hover pixels plus 909 px of a fading "32" popup.

So every difference has a written reason, the flash itself differs only at edge pixels, and the flash quad count equals the flashing hostiles. The W1-66 commit's own tally (8 at 0 px, 8 at 1-16, 8 at 38-225, 4 at 379-24,240, the rest not inspected, "no capture holds a flash") is superseded by this recount.

W1-67 done_when evidence (2026-11-06): `measure_performance.ps1 -Scene progressed-peak` and `-Scene stress`, `-Profile dev -Repeat 3` (VSync on), after in `target/perf/w1-67/after-dev-vsync` and `-stress`, against the W1-66 after runs in `target/perf/w1-66/after-dev-vsync` and `-stress` as before, medians of 3 as 1440x900 / 1050x700@1.35. battle_frame_build 0.279 / 0.281 ms before, 0.165 / 0.162 after: under 0.20 at both sizes (stress 0.417 / 0.421 before, 0.218 / 0.220 after). Both scenes hold about 224 visible hostiles. Battlefield paint did not move: 0.468 / 0.473 before, 0.480 / 0.471 after (stress 0.501 / 0.504, 0.502 / 0.503). interface_draw 1.104 / 1.090 before, 0.963 / 0.943 after. That is under 1.129 and 1.030, so the 1050x700@1.35 interface_draw gate passes for the first time (stress 1.250 / 1.242, 1.025 / 1.037). Every run was valid with 0 egui texture uploads. Both scripts still exit 1, on frames_over_50ms only: progressed-peak worst 5 / 1, stress 11 / 14. W1-66 had 0. These are the isolated 0.5 s and 1.0 s frames of gate 3. Main CPU in those frames is 7-13 ms, and the time is spent in the present wait (about 500 or 1,000 ms) or between frames. That is W1-68's attribution, and no gate was changed. Where the time went, measured headless in the dev test profile on the live progressed peak (222 enemies, 18 structures, 45 events): scene sample plus view 0.190 ms before and 0.088 after. Most of the gain comes from one pass over the events for every enemy's latest hit and attack pulse, instead of two event scans per enemy. The aligned pose and position stores and the hoisted structure range give the rest. Equivalence: a before/after digest of 516 views per scene (progressed-peak and stress, 1,200 live ticks, fractions 0.5, 0.25 reduced and 1.0) was 9245be1f2bb82098 / 17a033d3804309dd at both builds. The permanent pins are `field_views_equal_the_by_id_reference_on_peak_and_stress` and `enemy_pulses_match_the_latest_event_scans`.

W1-68 done_when evidence (2026-11-06): the report now records the between-frames gap, hand-off, channel wait, time outside the update, render idle, per-frame events, wall-clock instants and a window attribution (`slow_frame_reports_unattributed_time`, `perf_launch_records_the_between_frames_gap_and_events`). Fresh probes in `target/perf/w1-68/`, 11 passes of `-Repeat 3` at both sizes (66 runs), had 0 frames over 50 ms. The passes are:
- playtest, VSync off, progressed-peak: pipelined default, pipelined off, Intel Arc adapter, DX12, and 3,600 frames;
- playtest, VSync off, stress;
- dev, VSync on, progressed-peak and stress;
- playtest, VSync on, stress;
- two passes of the unchanged W1-67 binary, dev VSync-on stress.

At 18:09 that binary had 4-14 such frames per stress run. With 0 slow frames, the 95% coverage holds trivially. Over the 20 longest frames of each instrumented run (1,080 frames, the longest 23.4 ms), 1,077 are covered to 100%; the other 3 are first samples whose windows begin before the first kept sample. That 100% is an identity: `outside_update` is the gap's residual, so `coverage` is 1.0 inside the recorded range. The W1-68 review added `named_coverage`, `covered_95_named` and `max_outside_update_ms`, which leave the residual out, and pins for both hand-off paths. Cause, in two halves (narrowed by the review):
- VSync on (W1-67): named. It is an intermittent swap-chain acquire stall in the driver or compositor, not in-process. The stall sits in `get_current_texture` in 47 of 56 frames, and the 1.0 s frames equal wgpu-core's acquire timeout.
- VSync off (W1-61, the spec's 446-532 ms stalls): not named. All 16 frames over 50 ms had present_wait 0.09-0.20 ms, so they were not in the acquire call and sat outside every span then recorded. None has recurred under the new probe. **W1-68 is not done for this half.** **Closed by W1-61 (2026-11-06):** this half is the same swap-chain acquire stall. With `slowest_present`, all 220 VSync-off frames over 50 ms in the W1-61 A/B, pipelined or not, were acquire waits (acquire shares 0.981-0.992, in-process work 7.8-19.2 ms). Pipelined, the render frame that stalled was replaced before the main thread read it; W1-61 gives that as the reason these 16 frames showed a present_wait of 0.09-0.20 ms. See plan 9.4 and VALIDATION.md, "Goal-1 acceptance".

The 0 egui uploads, 0 asset events and 0 audio voices (review scenes are muted) come from probes with no stalls, so they exclude nothing. No fix commit follows. Gate 3 needs a user ruling on two open items: the environmental VSync-on stall, and the unattributed VSync-off stall, which needs a reproduction under the W1-68 probe. No gate was moved. Details are in VALIDATION.md. Both items are settled since: the user ruled "Exclude driver stalls" on 2026-11-06 (plan §2.2), and W1-61 named the VSync-off half as the same acquire stall.

Open (W1-61 review, 2026-11-07; W1-68's VSync-off half is closed by W1-61): one W1-61 A/B repeat in the shipped default configuration (playtest, pipelined on, VSync on, stress normal: `target/perf/w1-61/head-ab-playtest-vsync/stress-on-r2`) ran a sustained 15-20 FPS episode. It had about 400 frames of 47-67 ms. Outside driver stalls: 6 frames over 50 ms (exit 1), 407 over 33 ms, p99 47.1 ms, 46.3 FPS. It is not a driver stall under the ruling (named coverage 0.89-0.94, 4-6 ms per frame outside App::update) and is not attributed. Scheduled as W2-50 (perf-architecture, before the W2-47 re-gate) and tracked in plan §2.5; details in plan 9.4 and VALIDATION.md.

W1-69 done_when evidence (2026-11-06): enemies never block each other, the user's decision of 2026-11-06 (plan §2.2). In core, `sim/movement.rs` lost the lane queue (the stop 0.65 yd behind the body in front, `EnemyPhase::Queued`) and the speed-difference passing offsets. Each enemy stops only at the defense line or the face of a live non-trap structure in its lane, and attacks there. `Queued` stays for old saves (`a_saved_queued_enemy_resolves_on_the_next_tick`), and a saved `pass_offset` eases back to 0 at the old rate. The commit pins the rule in `enemies_in_one_lane_never_block_each_other` and `enemies_still_stop_at_structures`. `tests/overtaking.rs` and the simulation.rs hit-stop test were rewritten for the new rule. Presentation-only changes:
- a static per-id crowd stagger, R2 sequence, at most 0.15 yd behind the feet and 0.12 lane across;
- lanes painted far to near, then by id (drawn-feet tie-break since bb78430);
- one contact band per lane and one windup ring per stack;
- prop hit outlines moved to the readout pass, so struck walls no longer split the props pass. With every wall struck, the progressed peak drew 51 primitives against the gate of 48; the pin is `struck_structures_add_no_primitives`.

Without the per-stack rings the progressed peak drew 40,124 vertices at tick 420 against the gate of 40,000, from 59 enemies in contact where there were 19. No gate was moved. Re-bless (wave-1, R7): GOLDEN 1/6 and GAMEPLAY_TRACE 3/6, class G, with a reason per digest in `tests/determinism.rs`. The V1-V6 continuation pins, `V6_DROP_TRACE_FNV` and the `save_v7.json` pins did not move. Pacing (60 runs) and smoke before and after are in plan §9.6. The novice Germany profile has 5 → 8 defeats and its first victory moves from run 3 to run 5. Smoke passes, with failed attempts Personal 2 → 1 and Npc 1 → 3. Balance is not retuned; wave 2's pacing work owns the shift. Captures: peak, progressed-peak and fx-stress stills at both sizes in both modes (12 of 12, `target/production-after/w1-69`), plus motion for progressed-peak and stress at both viewports (`target/production-after/w1-69-motion`), reviewed in ledger rows CF-1-06 (fixed) and CF-1-07 (open, vfx-lighting).

Wave-1 exit:
- **Closed under R25 at HEAD ab03da9 (2026-11-07).** Review rounds 5-10 confirmed 4, 9, 2, 5, 3 and 3 findings, all minor and all fixed except the Invoke-Native item, which R24 resolves (rounds 1-4: 22, 14, 16, 13). The Goal-1 acceptance re-run at bee4236 passed every cell (ab03da9), and the full verify at ab03da9 was green (`target/production-after/wave1/verify-final/`). Evidence and the items handed on are in plan §7, "Wave-1 close": `pacing_targets` (W2-03), the W1-69 balance shift (wave 2), the gate-3 sample-932 frame and stress-on-r2 episode (W2-50), W2-52 before W2-49, the R18 items in W2-47, and the pending human checks.
- GOLDEN 1/6, TRACE 3/6, all from the W1-69 re-bless; V1-V6 unchanged.
- PERF gates pass at 199d823 except the open gate-3 episode (stress-on-r2), which W2-50 owns (W1-61; plan §2.5). At 867f786 the extra dev VSync-on stress pass also kept one stall-tail frame gated (sample 932, acquire share 0.794); W2-50 owns it too.
- `save_v7.json` frozen.
- Capture review logged (W1-60).
- The first freeze's open items are closed.

---

## WAVE 2: auto-sell, takeover, SelectCountry, scaling, archetypes, elites, bosses, progression and skills

Format: **Id · owner · title.** Files. *Fail-first.* Done. Deps.

**W2-01 · progression-tree · Pacing harness v2**
- Files: `sim/pacing.rs`, `examples/pacing.rs`, `src/strategy.rs` (seeded-rng seam), `tests/pacing_harness.rs`.
- What it adds:
  - `PacingProfile { id, name, seed, country, aim, spend, max_runs, difficulty, plan, stop_after }`;
  - the registry `by_id`, `DEFAULT_PROFILE_ID = "expert-deu"`;
  - `pacing_sweep` with thread::scope;
  - `MilestoneStats` (lower median/p25/p75; a missing milestone counts as max_runs+1);
  - the milestones `OperationsOpen`, `Liberation(n)`, `TreeExhausted`, `Rank`, `FirstTakeover`, `TierOwned`;
  - `--csv`, `--profile`, `--seeds`, `--fit-tiers`, `--values` stub. Unknown arguments exit 2.
- Fail-first:
  - `pacing_registry_ids_are_unique_and_default_is_expert_deu`
  - `stop_after_liberation_ends_the_profile_after_that_run`
  - `pacing_sweep_is_order_independent_of_thread_count`
  - `milestone_stats_counts_missing_as_max_runs_plus_one`
- Pin: the default `pacing 60 --runs` output equals the post-W1-69 baseline `target/production-after/w1-69/pacing.txt` (UTF-8, no BOM, LF), not the wave-0 `target/production-before/pacing.txt`: W1-69 moved pacing on purpose (plan §9.6). Mutation proof: change the legacy seed.
- Done: the 20-seed "before" baseline CSV is recorded.
- Deps: W1 close.

**W2-02 · progression-tree · `Game::operations_available` moves to core**
- Files: the predicate moves verbatim from `ui/context.rs:230-240`; `Milestone::OperationsOpen`.
- Pin: `operations_available_matches_legacy_ui_predicate` over 5 fixtures. Mutation proof: drop the roster term.
- Also: `context_operations_available_delegates_to_core`.
- Deps: W2-01.

**W2-03 · progression-tree · Goal-5 evaluators and `tests/pacing_targets.rs`**
- Files: `unlock_order`, `tree_outlasts_liberation`, `tier_affordable_runs`; the file is all `#[ignore = "release-only"]` with a OnceLock sweep cache; AGENTS.md gains the run command.
- Fail-first (synthetic):
  - `unlock_order_rejects_mara_before_fortifications`
  - `unlock_order_rejects_same_run_mara_and_operations_median`
  - `tree_outlasts_rejects_exhaustion_before_second_liberation`
  - `tree_exhausted_ignores_retired_nodes`
- Done: the ignored goal5_* tests exist, and their "before" values are recorded.
- Deps: W2-02.

**W2-04 · enemy-balance · Goal-6 pacing rows and evaluators**
- Files: the rows `lux-easy`, `lux-normal`, `lux-hard` (stop_after Liberation(1), max_runs 20), `start-aut`, `start-bgd`; the evaluators `defeat_rate_windows(20)`, `max_win_streak(11)`, `boss_ttk_median_s`, `archetype_coverage`, `runs_to_liberate`; ignored goal6_* tests.
- Fail-first (synthetic):
  - `runs_to_liberate_ratio_rejects_hard_above_1_5x_normal`
  - `summary_windows_and_per_seed_streaks_on_synthetic_runs`
- Done: `target/production-before/pacing_targets_before.txt` is written.
- Deps: W2-01.

**W2-05 · enemy-balance · Per-archetype telemetry population (T move)**
- Files: `director.rs:218`, `combat.rs:351`, the npc injury path, `lifecycle.rs` RunRecord `elite_kills`, `boss_ttk_ticks`, `boss_rank`.
- Writer obligation (schema-v7 §3.12, design-progression-tree "Schema v7 §3.12"): loading rejects an archetype with more kills than spawns, and an attempt resumed from an older save has living enemies whose spawns were never counted. So a kill is counted in `kills_by_archetype` only while that archetype's kills are below its `spawned_by_archetype` count.
- Fail-first:
  - `combat_telemetry_counts_spawns_kills_and_damage_taken_by_archetype`
  - `kills_by_archetype_sums...`
  - `resumed_attempt_counts_a_kill_only_below_its_archetype_spawns`: a resumed attempt kills an enemy spawned before the maps were written, and the save still loads.
- Done: provisional GOLDEN with cause T, "telemetry population, no gameplay change". TRACE unchanged.
- Deps: W2-01.

**W2-06 · loot-economy · L1: drop_ids, drop_tier always Some, Report loot fields; byte-identical legacy path (R4)**
- Files: `combat.rs:368-395` (the `drop_tier.is_none()` branch is verbatim today's code), `deploy_internal`, `lifecycle.rs:307-372`, `loot.rs` (`max_drop_tier`).
- Fail-first:
  - `drop_ids_complete_beyond_128_receipts`
  - `new_attempts_always_store_some_drop_tier`
  - `legacy_attempt_never_pushes_drop_ids_and_is_never_auto_sold`
  - `late_country_stage_sets_unlocked_drop_tier`
- Pin: `V6_DROP_TRACE_FNV` is asserted. Mutation proof: route None through DropTable.
- Done: provisional GOLDEN cause F. TRACE, smoke and pacing unchanged.
- Deps: W2-05.

**W2-07 · loot-economy · L2: relative quality `roll_relative` (b1)**
- Fail-first:
  - `relative_roll_never_drops_a_tier4_below_epic`
  - `floor_zeroing_distribution_matches_table`
  - `drop_quality_consumes_no_combat_rng`
- Pin: `roll_relative_with_common_base_and_no_floor_equals_roll`.
- Done: provisional cause G (Uncommon-base tier-1 items), only if a GOLDEN digest moves.
- Deps: W2-06.

**W2-08 · loot-economy · L3: 39 `loot_only` items (indices 29-67) and `purchasable()` at every site**
- Fail-first:
  - `every_tier_1_to_4_has_one_ranged_per_class_and_three_per_other_slot`
  - `loot_only_items_are_never_offered_bought_or_recommended`
- Pin: `appended_ids_keep_v1_order`.
- Done: provisional cause C.
- Deps: W2-07, W2-53 (the frozen-catalogue guard lands before the first wave-2 catalogue edit).

**W2-09 · loot-economy · L4: weighted single-draw DropTable**
- Fail-first:
  - `weighted_drops_favour_current_tier`
  - `trained_class_weighted`
  - `npc_mode_weights_its_own_weapon_class`
  - `definition_pick_consumes_exactly_one_draw`
- Done: TRACE unchanged (the gate). Provisional GOLDEN cause G only if moved.
- Deps: W2-08.

**W2-10 · campaign-defenders · `liberated_count` rewrite and the 10-level allowance**
- Fail-first:
  - `allowance_reaches_level_ten_by_sixth_liberation` (84; today 36)
  - `three_stage_only_route_keeps_raising_the_allowance`
  - `allowance_never_below_legacy_and_monotonic`
  - `liberated_count_matches_full_scan`
- Done: smoke equals smoke.txt; GOLDEN unchanged.
- Deps: W1 close.

**W2-11 · campaign-defenders · Takeover core (D6 + D8)**
- Files:
  - `deploy_as(level, DefenderId, stage)` with preflight then commit;
  - `defender_npc(seat)`/`_mut`, `record_defender_progress`, with the reads replaced at sim.rs:1106-1117, npc.rs:72/102/125/181/272 and lifecycle.rs:318;
  - `reserve_takeover`, the stamp via `stamp_recruit_defender`;
  - finish order: victory credit, then the npc attempts bump moves out of `finish_attempt`, then takeover release, then the H1 slot, then RunRecord `recruit_id`, then the Report seat.
- Fail-first (`tests/defenders.rs`):
  - `recruit_takeover_deploys_with_recruit_loadout_and_identity`
  - `takeover_uses_recruit_recall_not_maras`
  - `takeover_progress_accrues_to_roster_recruit_only`
  - `takeover_victory_credits_recruit_not_mara`
  - `takeover_defeat_and_withdraw_return_recruit_ready_and_release_items`
  - `takeover_save_reload_mid_battle_continues_identically`
  - `v7_fixture_withdraw_returns_defending_recruit_ready_and_releases_items`
- Pins: `mode_owner_paths_are_byte_identical`; `mara_finish_matches_legacy_attempt_counting`.
- Deps: W2-10.

**W2-12 · campaign-defenders · Next defender (D9)**
- Files: `SetNextDefender`, `BeginDefense`; ContinueJourney, retry and repeat use the report's defender; `available_defenders`, `defender_card`, `active_defender`, `battle_defender_card`; the dispatch resets next_defender with a note.
- Fail-first:
  - `next_defender_persists_through_save_load_and_continue_deploys_it`
  - `set_next_defender_typed_rejections_leave_state_unchanged`
  - `begin_defense_for_mara_matches_legacy_deploy_digest`
  - `deploy_command_stays_player_or_mara_with_next_defender_set`
  - `dispatching_the_next_defender_resets_it_to_player_with_note`
  - `recruit_on_main_run_cannot_be_dispatched_trained_or_equipped`
  - `migrated_v6_continues_with_mara` (the v6 fixture loads with `next_defender` None, pinned in wave 1 by `v6_fixture_defenders_resolve`; a retry or repeat of its settled Mara report deploys Mara and `active_defender()` then names Mara)
  - `new_defender_commands_round_trip_serde`
- Deps: W2-11.

**W2-13 · campaign-defenders · `SetRecruitTactics` (D12)**
- Fail-first:
  - `set_recruit_tactics_edits_ready_and_defending_only`
  - `mid_battle_tactics_change_takes_effect_next_turn_deterministically`
- Deps: W2-12.

**W2-14 · loot-economy · H1 main-run auto-sell**
- Files: `sim/auto_sell.rs`; finish() after the takeover release; the judge is `UpgradeJudge::Defender(a.defender())`.
- Fail-first:
  - `rule_off_moves_nothing`
  - `rule_sells_only_this_runs_drops_below_threshold`
  - `guarded_drops_are_kept`
  - `keep_upgrades_judges_the_takeover_recruits_loadout_not_maras`
  - `takeover_auto_sell_runs_after_release_and_never_sells_the_recruits_loadout`
  - `auto_sell_consumes_no_rng`
  - `wallet_overflow_keeps_items`
  - `fifty_sales_one_receipt_12_lines_exact_totals`
- Done: no digest move.
- Deps: W2-11, W2-09.

**W2-15 · campaign-defenders · Expedition settlement writers (D10)**
- Files: summary create, update and finalise; folded pruning; `AcknowledgeOutcomes`; `unacknowledged_outcomes`; `liberation_line`; idempotent `settle_parallel`.
- Fail-first:
  - `each_settled_expedition_yields_exactly_one_notice_across_save_reload`
  - `country_operation_notice_appears_once_after_final_stage_with_liberation_and_fronts`
  - `pruning_folds_unacknowledged_outcomes_and_conserves_the_count`
  - `pruning_prefers_acknowledged_and_legacy_battles`
  - `settle_parallel_twice_is_a_no_op`
  - `random_tick_recall_interleavings_settle_each_battle_once` (200 cases)
- Pins:
  - `v6_settled_battles_produce_no_notice_burst`;
  - `worker_victory_credits_recruit_and_threat_once` (acceptance G3.8; design-campaign-defenders §1.9 "Worker credit"): a settled worker victory writes exactly one RunRecord with `recruit_id` Some(id), and the parent's victory and threat credit is applied once, across a repeated `settle_parallel` and a save reload. Mutation proof: apply the parent's credit for the worker victory twice.
- Deps: W2-11.

**W2-16 · loot-economy · W-a worker and expedition auto-sell plus the final sweep**
- Judge: `Loadout { Npc, battle.loadout, battle.game.campaign.npc }`.
- Fail-first:
  - `worker_loot_sold_exactly_once_in_main_campaign`
  - `repeated_settle_parallel_is_idempotent`
  - `save_mid_expedition_preserves_pending_auto_sell_and_sells_once`
  - `apply_to_npc_runs_false_leaves_expedition_loot`
  - `chained_operation_gets_one_receipt_per_stage_with_count_le_loot`
  - `v6_inflight_attempt_and_expedition_sell_nothing_after_enabling_rule`
- Deps: W2-15, W2-14.

**W2-17 · loot-economy · `results_loot` view-model and the 500-case property test**
- Fail-first:
  - `results_loot_survives_reload_and_offers_upgrades_for_defender`
  - `worker_report_results_loot_has_no_kept_or_upgrades`
  - `previews_and_results_loot_do_not_mutate_game`
  - `properties::sale_paths_never_sell_guarded_items`
- Deps: W2-16.

**W2-18 · progression-tree · W2-D threat-scaled prices and UI literals (R10/D15)**
- Files: `camp_price`, `Campaign::recruit_price()`, `npc_training_price()`; `strategy.rs:296`, `progression.rs:726`; `ui/operations.rs:158/160/206`, `ui/character.rs:193`.
- Fail-first:
  - `recruit_and_training_prices_scale_with_threat` (100/110/200/300/600; 25/30/50/75/150)
  - `camp_price_labels_follow_core_price_at_rank_10`
- Done: the grep test finds no literal 25 or raw `recruit_cost` in core purchase paths.
- Deps: W2-10.

**W2-19 · campaign-defenders · `SelectCountry`, `outbreak_plan`, `country_choice_preview`**
- Fail-first:
  - `select_country_never_creates_an_attempt`
  - `select_country_typed_rejections_leave_state_unchanged`
  - `outbreak_plan_targets_final_stage_and_lists_ready_guards`
  - `begin_defense_from_plan_replays_final_stage_and_contains_the_outbreak`
- Deps: W2-12.

**W2-20 · campaign-defenders · `RecruitAs`, two-archetype keyed offer, origin trait**
- Files: also adds `DefenderCard.archetype: Option<Archetype>` (design-campaign-defenders §1.5, deferred from W2-12).
- Fail-first:
  - `recruit_offer_has_two_distinct_archetypes_and_recruit_as_applies_the_choice`
  - `legacy_recruit_equals_first_offer`
  - `recruit_as_rejects_unoffered_archetype`
  - `recruit_offer_names_the_missing_recruitment_node`
  - `recruiting_draws_no_strategy_rng`
  - `recruit_offer_price_reads_core_price`
- Deps: W2-18.

**W2-21 · campaign-defenders · `defender_power`, `service_record`, containments writer**
- Files: also adds `DefenderOption.power: DefenderPower` to `available_defenders` (design-campaign-defenders §1.4, deferred from W2-12).
- Fail-first:
  - `fresh_starter_power_is_100_at_threat_zero_in_both_models`
  - `defender_power_drops_as_threat_rises`
  - `defender_power_is_pure`
  - `defender_power_inputs_match_the_deployed_attempt`
  - `guard_containment_increments_recruit_containments_and_survives_guard_removal`
  - `service_record_is_derived_from_run_history_and_containments`
- Deps: W2-11.

**W2-22 · progression-tree · W2-C `settle_progression_rewards`**
- Files: finish() after H1; once at the end of settle_parallel when idle; milestone item keyed MILESTONE; remove `Campaign::claim_neutral_milestones` (the wave-1 neutral claim from 3df1242, called from `first_clear`), which this settlement replaces.
- Fail-first:
  - `level_grant_is_paid_once_at_root_finish` (77)
  - `grant_is_not_paid_while_the_attempt_is_live`
  - `worker_victory_level_up_pays_the_grant_once_in_the_parent_and_never_in_the_worker`
  - `milestone_paid_once_per_rank_across_save_reload`
  - `milestone_item_pick_draws_no_rng`
  - `finish_settlement_order_auto_sell_runs_before_milestone_item`
  - `settle_parallel_defers_rewards_while_main_attempt_is_live`
- Done: GOLDEN unchanged; smoke and pacing before/after recorded.
- Deps: W2-14, W2-16.

**W2-23 · progression-tree · W2-E `NpcTraining::TRAINABLE` and `TrainingUnsupported`**
- Fail-first:
  - `npc_trainable_rows_exclude_iq_hp_fp`
  - `apply_train_npc_intelligence_returns_training_unsupported`
- Deps: W1 close.

**W2-24 · progression-tree · W2-F mechanisms at neutral authored values**
- Files: class perks baked into the profile, armor penetration, `hazard_scaled`, `charges_at_rank` (sim.rs:979).
- Fail-first:
  - `class_modifier_applies_only_to_matching_class`
  - `armor_penetration_reduces_enemy_armor_for_personal_shots_only`
  - `hazard_scaled_is_identity_at_one_and_scales_at_point_six`
  - `trap_rank_r_places_three_plus_r_charges` (test-only authored value)
- Pin: `existing_structures_keep_charges_without_charges_per_rank`.
- Deps: W2-23.

**W2-25 · progression-tree · W2-G `tree_value` and `--values`**
- Fail-first:
  - `field_fitness_ehp_gain_matches_ratio`
  - `quick_throw_is_zero_today`
  - `dominance_groups_pair_vitality_with_health_add`
- Deps: W2-24.

**W2-26 · enemy-balance · Two-axis snapshot**
- Files: `FrontPosition`; `progression_for(.., Option<FrontPosition>)`; front ramp 0.18 × extras pct; the extras fold (elite pct, boss_phase_bonus); BossRank stamping; boss_damage rank term at read sites `combat.rs:102-108` (Personal) and `:166-176` (NPC).
- Fail-first:
  - `front_ramp_final_over_first_normal_is_at_least_1_35` (ranks 0/5/10/20/40, all countries with ≥ 2 stages)
  - `front_ramp_orders_hard_over_normal_over_easy`
  - `boss_damage_multiplier_follows_rank`
  - `personal_boss_contact_damage_scales_with_boss_damage_multiplier`
  - `npc_boss_attack_adds_include_boss_damage_term`
  - `boss_rank_derivation_usa_24`
  - `regional_checkpoint_distribution_is_219_12_4_7`
- Pins: `front_ramp_is_exactly_one_without_country`; `boss_damage_multiplier_is_exactly_one_through_rank_4`; `neutral_snapshot_serializes_no_new_keys_for_ranks_0_to_4_on_every_preset_at_depth_0`; `stage_theme_with_is_unchanged_on_checkpoint_stages`.
- Deps: W2-04, W2-53 (the frozen-catalogue guard lands before the enemy catalogue rows W2-26..W2-36).

**W2-27 · enemy-balance · Spitter**
- Files: reach, station, walls, followers walk through (W1-69: no queue, no overtaking); far-half guard.
- Fail-first:
  - `spitter_never_winds_up_or_damages_from_the_far_half`
  - `spitter_stations_at_clamped_stop`
  - `wall_in_lane_intercepts_spit`
  - `npc_spitter_uses_range_penalty_and_dodge`
- Pin: `reach_zero_profile_identical`.
- Deps: W2-26.

**W2-28 · vfx-lighting · Visual twin for ranged windups**
- Files: `EnemyView.aim_target`; a dashed aim line.
- Fail-first: `spitter_windup_draws_aim_line_to_wall_or_defender`.
- Pin: `every_windup_event_has_a_windup_view_in_the_same_frame` (normal and reduced). Mutation proof: force windup None.
- Deps: W2-27.

**W2-29 · enemy-balance · Exploder and splitter; `rules::resolve_blast` / `resolve_object_blast`; NPC rules**
- Fail-first:
  - `resolve_blast_draws_defense_only_when_dodged`
  - `npc_exploder_fuse_detonates_on_turn_without_reward_or_corpse`
  - `npc_death_burst_hits_enemies_in_radius_in_id_order_never_defender`
  - `npc_split_children_use_own_npc_stats_and_draw_no_rng`
  - `chain_bursts_resolve_one_tick_later_and_terminate`
  - `splitter_at_cap_minus_one_spawns_exactly_one_child_and_saves`
- Deps: W2-26.

**W2-30 · enemy-balance · Shield ranged armor, juggernaut stagger immunity, sprinter groups, hazard seam**
- Fail-first:
  - `shield_ranged_armor_applies_to_shot_and_turret_until_contact...`
  - `npc_juggernaut_major_wound_clears_stagger_flags_and_matches_draw_count`
  - `npc_sprinter_group_spawns_group_size_members_without_extra_rng`
- Deps: W2-29.

**W2-31 · enemy-balance · Composition: 7 archetypes plus waves 5-11 plus the catalogue floor check**
- Fail-first:
  - `catalogue_has_twelve_tiers_with_monotone_mean_health`
  - `npc_resolution_for_each_new_archetype`
  - `every_new_archetype_uses_an_existing_visual_sprite`
  - `every_floor_tier_exists_in_enemy_catalog`
- Pin: `waves_0_to_4_byte_identical`.
- Deps: W2-30.

**W2-32 · enemy-balance · Growth knee and `pressure_floors` rows 5..=28**
- Fail-first:
  - `snapshot_wave_tiers_at_10_20_40_are_7_10_11`
  - `growth_matches_gear_within_band`
  - `rank_20_boss_armor_exceeds_rank_0_both_modes`
- Pin: `ranks_zero_to_four_snapshot_pinned`.
- Deps: W2-31.

**W2-33 · enemy-balance · Elites**
- Files: `elite_bp`; keyed ELITE and AFFIX rolls; affix bake; the shared health guard.
- Fail-first:
  - `elite_bp_pins`
  - `elites_consume_no_combat_rng`
  - `elite_fraction_at_rank_13_is_8_5_pm_0_5_over_20k`
  - `hard_elite_rate_1_5x_normal_easy_0_5x`
  - `no_elites_below_rank_three`
- Pin: `quality_rolls_unchanged...`.
- Deps: W2-32.

**W2-34 · vfx-lighting · Validate `enemy_visuals` affix ids against the core elite table**
- Fail-first: `v2_unknown_affix_rejected`.
- Deps: W2-33, W1-52.

**W2-35 · loot-economy · Elite forced drop**
- Files: `combat.rs:368` normal/elite split; keyed ELITE_PICK; `roll_relative` Uncommon floor.
- Fail-first:
  - `elite_status_never_changes_combat_rng_draw_count`
  - `elite_only_drop_uses_keyed_pick_and_draws_no_rng`
  - `split_child_kill_takes_the_drop_draw_and_never_drops`
  - `elite_drop_quality_is_at_least_uncommon_and_boss_drop_has_no_floor`
- Deps: W2-33.

**W2-36 · enemy-balance · Bosses**
- Files: armor scaling, single-profile phases with a 90-tick hold, `BossPhase` event, Capstone +1 transition.
- Fail-first:
  - `boss_crossing_50_percent_enters_phase_once_with_90_tick_hold_event_and_survives_save`
  - `normal_rank_4_boss_has_no_phases_rank_5_has_one`
  - `hard_has_two_above_rank_4`
  - `caller_phase_shortens_interval`
  - `second_transition_clamped`
- Deps: W2-32.

**W2-37 · vfx-lighting · Visual twin for BossPhase**
- Files: `BattleView.boss_phase`; the "PHASE n - NAME" label and a ring for the whole hold (static ring in reduced mode).
- Fail-first: `boss_phase_hold_shows_phase_label_and_ring_for_90_ticks` (normal and reduced).
- Deps: W2-36.

**W2-38 · enemy-balance · generate_world.py commit A (writer only)**
- Files: indent-2 writer, `--from-existing` default, `--check`, `--rebuild` with sha256 pins (exit 3) and the stage-count guard (exit 4), `--self-test`.
- Fail-first: `--check` with the OLD formula exits 0 (today the compact writer fails); `--self-test`.
- Deps: W1 close.

**W2-39 · enemy-balance · generate_world.py commit B: difficulty bands, regeneration, start clamp**
- Files: bands 85-115 / 100-135 / 120-160 by stage count; `Country::effective_difficulty` and `Game::effective_country_difficulty`; `tests/simulation.rs:372` reason "start clamp"; WORLD.md paragraph.
- Fail-first:
  - `world_difficulty_matches_band_formula` (242 countries)
  - `start_country_difficulty_is_clamped`
- Pin: `world_stage_themes_and_strategy_config_survive_regeneration`.
- Deps: W2-38, W2-26.

**W2-40 · enemy-balance · Victory-to-threat guard tests**
- Files: `tests/threat_guard.rs`; proptest rank == committed victories over 120 seeds.
- Fail-first: the Capstone, Regional and takeover rows. Existing rows are mutation-checked by removing `saturating_add` at `campaign/world.rs:47`.
- Deps: W2-36, W2-11.

**W2-41 · progression-tree · W2-H: 8 decision-19 nodes (ids 170-177)**
- Fail-first:
  - `skill_catalog_is_append_only_with_eight_weapon_mastery_nodes`
  - `counters_name_catalog_archetypes`
  - `counter_nodes_open_no_later_than_their_archetypes`
- Note: if `big_game_rounds` replaces `blast_discipline`, R5's serde-defaulted `heavy_target_damage_multiplier` lands here.
- Deps: W2-31, W2-25.

**W2-42 · progression-tree · W2-I: profiles needing other remits' commands**
- Profiles: auto-sell, manual-sell, takeover (`PacingDefender::RecruitWhenReady`), farm, Easy/Hard, 150-run.
- Fail-first:
  - `auto_sell_profile_records_auto_sold_coins`
  - `takeover_profile_records_first_takeover`
- Deps: W2-14, W2-12, W2-20.

**W2-43 · campaign-defenders · Strategy rows and evaluators on PT's API**
- Evaluators: liberations 2/4/10, coins per minute, parallel vs solo, outbreak reopens. Ignored goal-3 tests.
- Fail-first:
  - `pacing_seeds_vary_strategy_rng`
  - `strategy_bot_acts_only_through_commands`
- Pin: `solo_plan_is_todays_profile`.
- Done: `target/production-before/strategy-pacing.txt`.
- Deps: W2-42, W2-15.

**W2-44 · ux-flow · Core labels (`Slot::label`, `DamageKind::label`)**
- Fail-first: `labels_are_human_over_all`.
- Deps: W1-10.

**W2-45 · ux-flow · `Campaign::feature_unlock` (pure)**
- Fail-first:
  - `feature_unlock_companion_is_75_after_1_liberation`
  - `feature_unlock_is_pure`
- Deps: W1 close.

**W2-46 · qa-captures · Staging helpers through Commands**
- Fail-first:
  - `equipment_autosell_rule_counts_only_below_uncommon_in_core`
  - `briefing_recruit_staging_sets_next_defender_through_command`
  - `repo_checks::production_fixtures_stage_features_through_commands`
- Deps: W2-12, W1-23.

**W2-47 · perf-architecture · Stress fixture follows the new content, then re-gate**
- Fail-first: `stress_render_fixture_includes_every_catalog_archetype_and_an_elite`.
- Done: `-Scene stress -VsyncOff` (dev and playtest) exits 0.
- Deps: W2-36, W2-33, W2-50, W2-51.
- Also owns (R18, wave-1 close):
  - the sustained 15-20 FPS VSync-on stress episode (`target/perf/w1-61/head-ab-playtest-vsync/stress-on-r2`; W2-50 attributes it first);
  - the 1050x700@1.35 VSync-on p99 interval of 19.1-19.7 ms;
  - script-level battlefield primitive and vertex gates in `measure_performance.ps1`;
  - any egui_probe/present_wait span-ordering item not fixed in wave 1, including the `baseline.v3` item below.
- Open before this re-gate (W1 review, 2026-11-07): review commit `7abb0ab` changed the egui_pass and present_wait spans after baseline.v3 and the W1-61 acceptance. The orchestrator or the user decides whether `PERF_BUDGET.json baseline.v3` is re-measured on the new spans before interface_total is gated again. Until then, interface_total and cpu_frame compare unlike spans (plan 2.5; VALIDATION.md W1-61 "Span boundaries changed after this acceptance"). The same applies to W5-11 and W7-09.

**W2-48 · save-determinism · RNG isolation and legacy guards, plus behaviour tests on the v6/v7 fixtures**
- Fail-first:
  - `elite_roll_consumes_no_combat_rng`
  - `rank_zero_deploy_spawns_no_elites_and_has_no_new_archetypes`
  - `legacy_attempt_snapshots_have_zero_elite_rate_and_no_boss_phases` (v1-v6)
  - `v6_active_attempt_spawns_no_elites_for_3600_ticks`
  - `v6_legacy_boss_has_no_phases`
  - `continue_journey_after_migrated_companion_report_redeploys_companion`
  - `v6_fixture_recruit_can_take_over_the_next_defense`
  - `worker_level_up_does_not_pay_the_level_grant_twice`
  - `migrated_threat_jump_pays_no_milestone`
  - `commands_round_trip_through_serde_for_replay` (all 9 appended variants)
- Deps: W2-01 to W2-47.

**W2-49 · save-determinism · Wave-2 re-bless (single)**
- Steps: fold PROVISIONAL into GOLDEN; write a per-digest reason table (classes F/T/C/G with anatomy diffs); `RULESET_VERSION` 3 and `TRACE_RULESET`; smoke and pacing after in `target/production-after-wave2/`.
- R15 (wave-1 close): W1-69 did not bump the ruleset, so the 2 -> 3 bump names both W1-69 (no lane queue) and the wave-2 change, in the commit body, the reason table and VALIDATION.md. Until this commit a recording made before W1-69 replays without a ruleset warning.
- R16: it also folds the three Personal GOLDEN state digests that W2-52 moves.
- Fail-first: `envelopes_carry_version_ruleset_and_fingerprint` asserts 3.
- Done: PROVISIONAL None; V1-V6 unchanged.
- Deps: W2-48, W2-52.

**W2-50 · perf-architecture · The gate-3 episode in the shipped default configuration, before the W2-47 re-gate**
- Slot: before W2-47; the id is appended so no id changes. W1-61's A/B repeat `target/perf/w1-61/head-ab-playtest-vsync/stress-on-r2` (playtest, pipelined on, VSync on, stress normal: the shipped default configuration) ran about 400 frames of 47-67 ms. Outside driver stalls it had 6 frames over 50 ms (exit 1), 407 over 33 ms, a p99 of 47.1 ms and 46.3 FPS. It is not a driver stall under the user ruling (named coverage 0.89-0.94) and is not attributed (4-6 ms per frame outside App::update). Plan 9.4 and §2.5; VALIDATION.md W1-61.
- Also owns (W1 review, 2026-11-07): the second gated VSync-on stress frame on this branch. In the Goal-1 acceptance re-run at 867f786, `target/perf/w1-review/acc-dev-vsync-stress/stress-normal-r1.json` sample 932 (dev, VSync on, stress normal) was 54.74 ms with a present wait of 50.61 ms and an acquire wait of 43.46 ms: acquire share 0.794, under the 0.80 threshold, so the rule keeps it gated and that pass exited 1. Named coverage 0.989, in-process work about 11 ms; it comes directly after three ~1,000 ms stalls (samples 929-931). The acquire wait is counted only where the hand-off overlaps prepare_windows (43.5 of the 50.6 ms present wait). Plan 9.4 and §2.5; VALIDATION.md 'Goal-1 acceptance re-run at HEAD'.
- Steps:
  - reproduce that configuration (`measure_performance.ps1 -Scene stress -Profile playtest -Pipelined on`, VSync on) at both sizes, at least 12 repeats per size, under the W1-68 probe and with the load sampler beside it;
  - record per repeat `named_coverage`, `covered_95_named`, `max_outside_update_ms`, and the per-frame events of every frame over 33 ms outside stalls;
  - attribute the episode (name the span, or the process outside the update, that holds the time) or fix it.
  - for the sample-932 tail: decide whether the frame right after a stall counts its whole present wait toward the acquire share, or whether such tails stay gated. A change to the `driver_stall` rule is a gate change: it needs a user ruling (§2.5 row) first and lands as its own commit with a pin in `frame_report/tests.rs`.
- Fail-first: only when a fix lands: a pin for the attributed cause that fails on the parent (a test in the owning crate, or a `scripts/fixtures/perf/` report that `-SelfTest` must classify). Attribution alone needs no test.
- Done: evidence in `target/perf/w2-50/`. VALIDATION.md and plan 9.4 record the attribution, the fix, or that the repeats did not reproduce it, and the sample-932 tail's outcome; both §2.5 rows are answered. No gate is moved without that ruling.
- Deps: none; W2-47 depends on it.

**W2-51 · perf-architecture · Apply the W1-51 egui gate re-derivation decision, before the W2-47 re-gate**
- Slot: before W2-47; the id is appended so no id changes. W1-51's done_when derives the egui gates as min(W1-32 ceiling, ceil(measured max x 1.25)). The W1 review re-measured on the post-W1-69 renderer (4d599a5, `PERF_BUDGET.json evidence.egui_measured`; the a93e340 figures are in `.previous`): 19 primitives in every scene, stress vertices 47,435 and stress-5000-corpses 49,502. Re-deriving moves `BATTLEFIELD_PRIMITIVES_MAX` 48 -> 24 and the stress vertex gate 61098 -> 61878; progressed-peak stays at its 40,000 ceiling. The plan §2.5 row decides which of them moves (default: primitives only).
- Steps: change `PERF_BUDGET.json` (`egui` and the `egui_measured` gate source) and `crates/horde_game/src/perf_budget.rs` (the constants and their doc comments) in the same commit, so `perf_budget_json_matches_presentation_caps` stays bidirectional and `egui_gates_are_measured_x_1_25_capped_at_the_w1_32_ceilings` re-derives the decided gates from the recorded maxima.
- Fail-first: `egui_gates_are_measured_x_1_25_capped_at_the_w1_32_ceilings` with the decided maxima recorded as the gate source and the old constants, failing before the constants move.
- Done: `measure_battlefield_egui_load -- --ignored` and `live_battlefield_runs_stay_within_the_egui_gates` pass at the decided gates; the §2.5 row is answered.
- Deps: none; W2-47 depends on it.
- **Applied in the W1 review (R22, wave-1 close):** gates only tighten. `BATTLEFIELD_PRIMITIVES_MAX` 48 -> 24, the stress vertex gate stays 61098 (no loosening; measured 49,502), progressed-peak stays at its 40,000 ceiling, `evidence.egui_measured` holds the post-W1-69 maxima, and `live_battlefield_runs_stay_within_the_egui_gates` paints every 2nd tick of the first 420. W2-47 still depends on this row, which has nothing left to change.

**W2-52 · progression-tree · Neutral level-ups keep the grant ledger paid (the `level_grant_paid` shim), before the W2-49 re-bless**
- Slot: before W2-49 (R16, wave-1 close); the id is appended so no id changes. Wave 1 writes saves at v7 but nothing keeps `Player.level_grant_paid` current while no grant is paid, so a wave-1 save can owe level grants that a migrated v6 save at the same level never gets. The milestone half was fixed in the W1 review (3df1242, `Campaign::claim_neutral_milestones`); this level-grant half moves the three Personal GOLDEN state digests, so it waits for the wave-2 re-bless.
- Fail-first: `neutral_level_ups_keep_the_grant_ledger_paid`.
- Digest: sets `PROVISIONAL` for the three Personal GOLDEN digests with a `PROVISIONAL_CAUSES` row (P3). W2-49 folds them.
- Deps: W1 close; W2-49 depends on it.

**W2-53 · save-determinism · Frozen catalogue records guard, before the first wave-2 catalogue edit**
- Slot: before W2-08 (the first wave-2 commit that edits a catalogue file: content.json gains 39 items) and W2-26 (the first enemy catalogue row; W2-31 appends the 7 archetype records and waves 5-11); the id is appended so no id changes. W1 review: acceptance ISO-3 and design-save-determinism §13 risk 2 named this guard, but no wave-1 commit or waves.md row created it.
- Files: a horde_core test beside the V1-V6 continuation pins (`tests/save_migration.rs`), using `support::fnv1a` like those pins.
- Fail-first (characterisation pin): `legacy_catalogue_records_that_tick_saved_encounters_are_frozen`. It pins the FNV-1a of the catalogue records that V1-V6 encounters tick (design-save-determinism invariant 3): the 11 existing archetype resolutions, wave tiers 0..=4 of `enemy_catalog.json` and the 5 `content.json` levels, as the bundled catalogue resolves them. Appended records, tiers 5 and up, and new fields at their skipped neutral default do not move it. Mutation proof: change one value of a tier-0..=4 wave and one existing archetype's resolved health; each fails the pin.
- Done: no digest move; V1-V6 pins unchanged. A later wave-2 or wave-4 commit that moves this pin edited a frozen (append-only, ISO-3) record: fix the edit. If a content-driven change is meant to reach legacy attempts (Ruling R4), the continuation pins it moves are re-blessed in that wave's GOLDEN re-bless with a written reason. There is no `rules_revision` field (R4).
- Deps: W1 close; W2-08 and W2-26 depend on it.

Wave-2 exit:
- One re-bless with reasons.
- Pacing and smoke before/after recorded.
- Captures (stress, battles) taken.
- Audio handoffs exist: the BossPhase event and the twins W2-28/W2-37.
- The gate-3 episode of the shipped default configuration (W2-50) is attributed or fixed, or closed by the §2.5 decision; the sample-932 stall-tail question is answered.

---

## WAVE 3: goal 2 and the UI for goals 3-4 (horde_game only, plus pure core reads)

**W3-01 · ux-flow · Harness conventions, `View::Full`, the amber primary registry**
- Fail-first:
  - `primary_is_amber_and_registered`
  - `at_most_one_screen_primary_matrix`
  - `registry_empty_on_running_battle_frame`

**W3-02 · ux-flow · `Tab::Home` default, 6-entry row, drill-downs (Mara, Operations), Home-owned clock**
- Fail-first:
  - `camp_default_tab_is_home`
  - `clock_controls_only_on_home`
  - `roster_shows_core_power_and_status_labels`
  - `home_primary_after_liberation_is_choose_next_country`
- Rewrites: `navigation_tests` (reason recorded).

**W3-03 · ux-flow · `Screen::Upgrades` split and 3-action Results**
- Fail-first:
  - `results_action_bar_has_exactly_three_buttons`
  - `results_primary_table`
  - `upgrade_opens_upgrades_screen_with_back_to_battle_and_back_to_camp`
  - `back_to_battle_retries_same_stage`

**W3-04 · ux-flow · Escape router, pure `escape_action`, `keyboard_owner`, Quit and ReturnToMenu modals**
- Fail-first:
  - `escape::tests::matrix_table` (~45 rows)
  - `properties`
  - `escape_on_menu_opens_quit_confirmation`
  - `escape_in_construction_closes_without_pausing`
  - `pause_bound_to_escape_still_pauses_after_router_consumes`
- Scan: no `Key::Escape` outside escape.rs.

**W3-05 · ux-flow · One "Save" label and the toast layer**
- Note: the battle toast draws in `layout.receipt_slot` and takes priority over the receipt.
- Fail-first:
  - `exactly_one_save_label`
  - `manual_save_shows_saved_toast_on_results`
  - `battle_toast_draws_in_receipt_slot`
  - `toast_expires_after_3s_and_is_static_in_reduced_effects`

**W3-06 · ux-flow · Storage-error modal, runner `OpenSaveFolder`/`ReloadProfile`, settings-unreadable modal text**
- Fail-first:
  - `save_failure_opens_retry_modal`
  - `open_save_folder_uses_profile_directory`
  - `unreadable_settings_modal_names_copy`

**W3-07 · ux-flow · Unapplied difficulty draft warning**
- Fail-first: `dirty_draft_escape_opens_warning`.

**W3-08 · ux-flow · Scale slider applies on release**
- Fail-first: `scale_applies_only_on_release`.

**W3-09 · ux-flow · Decision-18 settings widgets**
- Rows: Display (window mode, vsync, low latency with "(applies after restart)"), Readability (quality markers, palette), Audio (Music & stingers, Interface sounds), Controls.
- Note: no apply system here; perf's W1-34 owns it.
- Fail-first:
  - `settings_show_music_and_stingers_and_interface_sounds_sliders`
  - `colour_blind_palette_changes_marker_colours_and_shapes`
  - `low_latency_row_shows_restart_note_exactly_when_pipelined_differs`

**W3-10 · ux-flow · Extra bindings**
- Files: `ExtraAction`, `binding_pressed`; rebind spans both tables; `cycle_attack`, `world_clock`.
- Fail-first:
  - `rebind_conflict_spans_core_and_extra_tables`
  - `unbound_extra_action_never_fires`
  - `cycle_attack_key_cycles_unlocked_attacks`

**W3-11 · ux-flow · Bar-rows characterisation**
- Files: `BarRows`; `BattleUi.bar_rows`; qa `ReviewFrameFacts.hud_rows`.
- Pin: `battle_bar_rows_measure_todays_bar` (expected 3). Mutation proof is in the commit message.

**W3-12 · ux-flow · Battle bar ≤ 2 rows**
- Change: Reload, Strike contact lane and Toggle firing move to Management > Field actions; the status row is removed.
- Fail-first:
  - `battle_bar_stays_within_two_rows_at_small_size` (matrix)
  - `field_actions_keep_their_keys`
- Deps: W3-11.
- Done: capture row CF-1-08 (the controls legend at `layout.legend_anchor` lies under the second button row at 1050x700@1.35) is fixed in the plan's capture ledger; the legend is clear of the bar at both sizes.

**W3-13 · ux-flow · Hold-to-retreat (0.8 s)**
- Fail-first:
  - `single_click_retreat_does_not_withdraw`
  - `held_retreat_withdraws_after_0_8s_and_opens_results`
  - `retreat_hold_stepped_in_reduced_effects`

**W3-14 · ux-flow · Wall-clock boss banner at the BattleLayout position**
- Fail-first:
  - `boss_banner_visible_at_1_2s_and_gone_after_2s_wall_at_speed_4`
  - `boss_arrived_banner_lasts_1_5s`
  - `boss_banner_never_overlaps_the_hint_callout`
  - `boss_banner_static_in_reduced_effects`

**W3-15 · ux-flow · Low-HP edge warning with hysteresis (30/35%)**
- Fail-first:
  - `low_hp_edge_below_30_percent`
  - `low_hp_hysteresis_exits_above_35`
  - `ui_frames_do_not_change_state_digest`

**W3-16 · ux-flow · Defender picker and takeover labels (D16)**
- Changes: the Briefing primary dispatches `BeginDefense`; the HUD hard-coded "MARA" and the forced portrait are removed.
- Fail-first:
  - `picker_lists_ready_recruit_and_dispatches_set_next_defender`
  - `guarding_recruit_disabled_with_core_reason`
  - `results_picker_persists_choice_across_save_load`
  - `recruit_takeover_hud_names_the_recruit`
  - `hud_tactics_dispatch_set_recruit_tactics_during_takeover`

**W3-17 · vfx-lighting · Battlefield appearance from the stamped profile (D17); the observer names the recruit**
- Fail-first:
  - `battlefield_draws_profile_appearance_not_forced_female`
  - `observer_window_draws_the_expedition_recruit_not_mara`
- Pin: `mara_battlefield_model_unchanged`.

**W3-18 · ux-flow · World "Plan defense" and outbreak "Plan response" via SelectCountry**
- Fail-first:
  - `world_footer_never_creates_attempt`
  - `plan_response_opens_briefing_without_attempt`

**W3-19 · ux-flow · Liberation recruit block and Recruit modal (price from core) with "Back to camp"**
- Fail-first:
  - `liberation_results_offers_recruit_when_node_owned`
  - `liberation_results_names_missing_survivor_network`
  - `liberation_recruit_price_reads_core_at_rank_10`
  - `liberation_results_action_bar_still_three_buttons`

**W3-20 · loot-economy · Results loot block, history columns, no-sale-in-battle guards**
- Fail-first:
  - `results_shows_drops_by_quality_and_auto_sold_line`
  - `results_better_than_equipped_row_dispatches_equip_command`
  - `recruit_upgrade_row_points_to_operations`
  - `results_loot_block_fits_1050x700_at_1_35`
  - `battle_and_observer_never_render_sale_text`
  - `history_lists_auto_sold_count_and_coins`

**W3-21 · loot-economy · Equipment auto-sell panel, confirmation modal, blocked-sale shortcuts**
- Fail-first:
  - `equipment_autosell_panel_preview_counts_match_core` ("would have sold N")
  - `apply_to_stash_requires_confirmation_and_escape_closes`
  - `stale_quote_refreshes_modal`
  - `threshold_chips_show_text_and_shape_markers`
  - `blocked_sale_shows_typed_reason_and_shortcut`
  - `delete_preset_shortcut_dispatches_command`

**W3-22 · progression-tree · Goal-5 UI**
- Content: Results level and milestone lines, TRAINABLE lists, IQ text removed, perk and counter previews.
- Fail-first:
  - `results_shows_level_grant_line`
  - `results_shows_rank_10_milestone_headline`
  - `training_lists_have_no_iq_hp_fp_text`
  - `counter_node_tooltip_names_its_archetypes`

**W3-23 · enemy-balance · Enemy UI hooks**
- Content: `stage_preview`, effective difficulty, Capstone/Regional badge, Results elites and boss TTK, "Custom (Hard rules)".
- Fail-first:
  - `briefing_shows_effective_difficulty_and_capstone_badge`
  - `results_lists_elite_kills_and_boss_ttk`
  - `preferences_shows_custom_basis_label`

**W3-24 · ux-flow · Onboarding callouts and country dossier**
- Files: `seed_seen_callouts`, `CampaignFeature::id`; New game sets Some(empty); `restore_defaults` keeps `seen_callouts`.
- Fail-first:
  - `unlock_callout_shows_once_and_survives_restart`
  - `none_is_seeded_with_every_unlocked_callout_at_profile_load`
  - `restore_defaults_keeps_seen_callouts_and_writes_current_version`
  - `country_dossier_names_defense_count_and_mara_timing`
  - `campaign_feature_ids_are_stable_and_unique`

**W3-25 · ux-flow · Expedition outcome toasts**
- Files: `unacknowledged_outcomes`; `AcknowledgeOutcomes` is dispatched in the same frame.
- Fail-first:
  - `one_toast_per_unacknowledged_outcome_and_ack_dispatched_same_frame`
  - `folded_overflow_shows_single_summary_toast`
  - `expedition_toasts_deferred_during_live_unpaused_battle`

**W3-26 · ux-flow · Remove `{:?}` labels; source scans**
- Fail-first:
  - `no_debug_format_in_player_text` (17 hits today)
  - `ui_has_no_literal_camp_prices_or_mara_mapping`

**W3-27 · qa-captures · UI scenes for goals 2-4 and the W3 capture matrix**
- Scenes: `results-defeat`, `results-liberation-recruit`, `upgrades`, `camp-home`, `briefing-recruit`, `equipment-autosell`, `save-error`, takeover battle.
- Fail-first: the scene tests (per the qa #11 list).

**W3-28 · ux-flow · Full-loop headless acceptance test at both sizes**
- Fail-first: `full_loop_new_game_to_recruit_defends`. Each of the 10 steps asserts Screen, Tab and exactly one primary. The battle steps assert "Begin defense" / "Resume defense".

**W3-29 · vfx-lighting · Pause banner and midpoint label placement (capture rows CF-1-01, CF-1-02)**
- Slot: beside W3-14 (the ux-flow banner work), not after W3-28: the id is appended so no id changes. `pause_banner_center` is C-3 geometry and C-11 bars ux-flow from redefining it, so this is a BattleLayout change by vfx-lighting (design-perf-architecture.md, "Zoom-to-fit and wave-3 UI"). The new banner place is the owner's choice; design-vfx-lighting.md's letterbox rule alone does not help a height-limited 7x16 field.
- R17 (wave-1 close): owner vfx-lighting, before W3-27's capture matrix is reviewed.
- Fail-first:
  - `pause_banner_never_covers_the_bottom_lane` over the zoomed 7x16 and the 3x12 hint-band fields at both sizes (CF-1-01)
  - `midpoint_label_clears_the_construction_panel` at 1050x700@1.35: the label rect and `construction_panel` do not intersect (CF-1-02)
- Done: CF-1-01 and CF-1-02 are fixed in the plan's capture ledger.
- Deps: W3-14.

**W3-30 · vfx-lighting · Review captures ignore the OS pointer (capture row CF-1-03)**
- Slot: before the W3-27 capture matrix is reviewed; the id is appended so no id changes. R17 (wave-1 close): owner vfx-lighting (first qa-captures), beside the other field placement rows W3-29 and W3-31.
- Fail-first: `review_capture_aim_ignores_the_os_pointer`: with a review-capture launch and a hover position over the field, `f.battle.aim` stays at the scene's pinned aim and no hover reticle or inspector draws.
- Done: CF-1-03 is fixed in the plan's capture ledger.
- Deps: none; W3-27's matrix review depends on it.

**W3-31 · vfx-lighting · Wall-hit numbers clear the wall face and its stack (capture row CF-1-07)**
- Slot: beside W3-29 (the other vfx-lighting placement fix over the field), before W3-27's capture matrix is reviewed; the id is appended so no id changes. R17 (wave-1 close) confirms the owner and slot. Since W1-69 every stacked enemy strikes its wall, so at 1050x700@1.35 the wall-hit numbers ("-80", "MISS") cover the wall face and the front of its stack. The placement is `ui/damage.rs` presentation only: it reads the drawn wall and stack rects and never touches core RNG or state. The new place is the owner's choice, and reduced mode uses the same placement.
- Fail-first: `wall_hit_numbers_clear_the_wall_face_and_its_stack_at_small_size`: at 1050x700@1.35, on the progressed-peak field, the drawn rect of each wall-hit popup does not intersect the struck wall's face rect or the drawn body of the front enemy in its stack.
- Done: CF-1-07 is fixed in the plan's capture ledger, with after-capture evidence from progressed-peak and from stress motion (`scripts/capture_motion.ps1 -Scene stress -Viewport small`) at 1050x700@1.35.
- Deps: none.

Wave-3 exit:
- GOLDEN and TRACE 0.
- The W3 capture matrix is reviewed.
- Capture rows CF-1-01, CF-1-02 (W3-29), CF-1-03 (W3-30), CF-1-07 (W3-31) and CF-1-08 (W3-12) are fixed.
- The construction-rect CF row is closed (already fixed in W1).

---

## WAVE 4: balance (goals 5-6) and the goal-3 numbers

| Id | Owner | Title | Fail-first / gate | Deps |
|---|---|---|---|---|
| W4-01 | enemy-balance | Regional and Capstone boss multipliers: Regional HP and reward x1.2; Capstone HP x1.4, armor x1.25, NPC armor +2, reward x1.4, damage x1.10 | `regional_boss_health_and_reward_are_1_2x_sector_at_equal_rank`, `boss_damage_capstone_factor_is_1_10` | W2 |
| W4-02 | campaign-defenders | Regional checkpoint presentation lines: briefing "Regional checkpoint - boss +20% health, +20% reward" and "Final stand"; world panel "N checkpoints"; Results "Regional checkpoint secured" | `briefing_names_regional_checkpoint_on_checkpoint_stage` | W4-01 |
| W4-03 | progression-tree | Effect retunes: steady_hands, hardened, quick_throw, combat_reload; shrapnel_trap `charges_per_rank` 1 | `hardened_leaves_runner_contact_damaging`, `trap_mastery_1_adds_a_trap_charge`, `steady_hands_..._at_most_40_percent`; pacing and smoke before/after | W2 |
| W4-04..n | enemy-balance | Tuning loop, one lever per commit, in this order: health_per_rank_late, boss_health slope, front_ramp_max ∈ [0.17, 0.25], affixes, capstone HP | Ignored `pacing_targets` goal6_* in release; each commit records pacing and smoke before/after and its provisional cause G | W4-01 |
| W4-p | progression-tree | Repricing, tree-quality rules and the blueprint price fix (one green commit), then the calibration loop | `no_valued_node_is_dominated_at_its_reference_states`, `cheap_nodes_stay_within_twice_branch_value_per_coin`, `tier_costs_sit_in_design_bands`, goal5_* (release) | W4-03, EB loop |
| W4-l | loot-economy | Auto-sell pacing gate and stash-growth measurement | `auto_sell_profile_within_one_run_of_default_at_tiers_1_to_3`, `stash_growth_reported_for_150_runs` | W4-p |
| W4-c | campaign-defenders | Goal-3 strategy acceptance (20 seeds) | `strategy_profile_reports_liberations_2_4_10_and_coins_per_minute`, `parallel_frees_three_stage_neighbours_within_two_thirds_of_solo_runs`, `fewer_than_five_outbreak_reopens_over_ten_liberations` | W4-l |
| W4-08 | save-determinism | Balance re-bless (single): RULESET 4; the conditional `follow_named_preset` if existing preset values changed; continuation re-bless reasons if any | `v6_hard_campaign_deploys_with_current_hard_threat_but_in_flight_snapshot_stays_legacy`; ruleset assert = 4 | all W4 tuning |
| W4-09 | save-determinism | PROGRESSED_GOLDEN (4 runs: seeds {1, 0xDEAD_BEEF_2026} x {Personal Hard capstone, Npc recruit takeover}, rank 12, 3600 ticks), PROGRESSED_TRACE, `V7_CONTINUATION_FNV` | `progressed_golden_runs_exercise_elites_boss_phases_and_new_archetypes` (perturbation proofs) | W4-08 |
| W4-10 | enemy-balance | Docs: ENEMY_COMPOSITION.md, PROGRESSION_SCALING.md, the goal-6 section of BALANCE_AND_RUN_STATS.md, the WORLD.md band paragraph | qa `required_docs_sections_exist` | W4-09 |
| W4-11 | progression-tree | Docs: the goal-5 section of BALANCE_AND_RUN_STATS.md (tier definition, TreeExhausted, OperationsOpen) and the AGENTS.md pacing_targets section | check_views | W4-09 |

Wave-4 exit: every ignored `pacing_targets` assertion passes in release (goal 3, goal 4 auto-sell, goal 5, goal 6), with before/after tables in the plan.

---

## WAVE 5: goal 7 (horde_game only)

| Id | Owner | Title | Fail-first | Deps |
|---|---|---|---|---|
| W5-01 | vfx-lighting | 5a: render-pass split, theme grades, cue halos with ink backing, vignette move, `STORM_THEME_IDS`/`is_storm`, static storm grade on storm stages in reduced mode, observer `scene_look` | `night_grade_at_most_0_70_of_dawn...`, `every_cue_primitive_has_an_ink_backing_with_contrast_at_least_3`, `storm_stage_coverage_is_131_of_1026`, `reduced_effects_storm_stage_draws_static_storm_grade_and_no_flash`, `worker_view_uses_biome_theme_not_level`; pin `stage_theme_ids_are_pinned_to_the_10_in_order` with its self-test | W4 |
| W5-02 | vfx-lighting | 5b: wall-clock lightning (`SKY_SLOT_MS` 8000, quiet windows; ΔL < 0.2) | `sky_flash_starts_at_least_4000_ms_apart_at_observation_speed_4x`, `sky_flash_peak_delta_below_0_2...`, `sky_flash_never_reads_core_rng` | W5-01 |
| W5-03 | vfx-lighting | 5c: weapon_fx profiles (6 classes plus turret), shotgun fan, MG tracers, gear-snapshot class | `every_class_and_turret_profile_distinct_on_two_axes...`, `shotgun_volley_paints_one_flash_and_nine_streaks`, `machine_gun_bright_tracer_every_third_volley`, `reduced_shot_paints_no_flash...`; pin `shot_fx_never_mutates_attempt` (acceptance ISO-8: painting every weapon_fx profile leaves the attempt, its RNG and the state digest unchanged) | W5-01 |
| W5-04 | vfx-lighting | 5d: additive light pools (cap 24) | `light_pools_capped_at_24...`, `reduced_has_no_light_pools` | W5-03 |
| W5-05 | vfx-lighting | 5e: multi-frame explosions and fire; grenade vs Molotov | `grenade_and_molotov_explosions_paint_distinct_shape_sets`, `explosion_has_at_least_4_distinct_frames...` | W5-04 |
| W5-06 | vfx-lighting | 5f: damage-scaled hits, boss kill burst, blood decals (opt-in, cap 64) | `hit_spark_length_monotonic_in_damage...`, `blood_decals_zero_when_off_newest_64...`; new `field_rect_never_offsets_on_damage_or_explosion` (no screen shake) | W5-05 |
| W5-07 | vfx-lighting | 5g: elite and affix tells | `elite_enemy_paints_ring_and_marker_with_ink_backing_in_cue_pass`, `reduced_elite_has_no_pulse` | W5-01 |
| W5-08 | vfx-lighting | 5h: quality glint, tier markers, static reduced fly-ups | `rare_or_better_flyup_paints_one_glint_common_none`, `reduced_flyup_is_static_and_glintless`; pin `flyup_never_shows_sale_status` (acceptance G4.9: no fly-up in ui/loot.rs draws auto-sell or sale text, normal and reduced) | W5-01 |
| W5-09 | vfx-lighting | 5i: review scenes (weapon-classes, storm, night-checkpoint, dawn-reference, fx-stress, elite-tells), `measure_field_luminance.py`, fx budget, reduced MOTION gate | `reduced_96_frame_fx_stress_sequence_has_no_additive_vertex...`, `fx_stress_shape_counts_within_perf_budget` | W5-01..08 |
| W5-10 | qa-captures | Storm scene, weapon snapshot assertion, flash counters in manifest v2; reduced zero asserts | `storm_fixture_is_a_storm_tagged_stage`, `manifest_v2_records_flash_counters` | W5-09 |
| W5-11 | perf-architecture | Re-gate after graphics: fx caps in PERF_BUDGET (decals 64, lights 24, explosion frames 8, flames 21/9); fx-stress gates | `perf_budget_json_matches_presentation_caps`, `perf_scenes_agree...`; stress and fx-stress exit 0 with -VsyncOff | W5-09 |
| W5-12 | save-determinism | Zero-move verification record (0/6, 0/4, 0/6) | existing pins | W5-11 |

The human review for weapon-classes and night/storm is logged as pending if it is not done.

---

## WAVE 6: goal 8 (new gun/enemy/UI audio; no new music files; tracks Deferred)

| Id | Owner | Title | Fail-first | Deps |
|---|---|---|---|---|
| A6-00 | audio | (if the user approves; P8) Bevy-free path crate `crates/horde_audio`; Cargo.lock diff = one path entry; uses `horde_core::sha256` through dev-dependencies | `horde_audio_builds_without_bevy` (cargo tree) | W5 |
| A6-01 | audio | 6.1: move Cue, BankId and legacy synthesis verbatim | `raw_synthesis_matches_v1_library`, `bank_ordinals_are_append_only` | A6-00 |
| A6-02 | audio | 6.2: move the mix table out of horde_game (value-preserving); `WARNING_TWINS` names vfx predicates (W2-28/W2-37/existing windup ring) | `horde_game_audio_sources_define_no_gain_tables`, `every_warning_row_names_a_visual_twin` | A6-01 |
| A6-03 | audio | 6.3: BS.1770-4 meter and 4x true peak | `ebu3341_case1...`, `true_peak_of_fs_over_4_sine_at_45deg_is_unity` | A6-01 |
| A6-04 | audio | 6.4: offline renderer and BEFORE numbers (gate red, ignored) | `reference_mixed_60s_meets_loudness_gate` recorded at about -38..-42 LUFS | A6-03 |
| A6-05 | audio | 6.5: exporter v2. 5+ variants for frequent cues; new gun, enemy, boss and UI families; offline per-clip true-peak ceiling as mastering (R3); schema-2 SURVIVAL_PROVENANCE with the v1 file archived; refresh of the CREDITS index; reel to `target/audio-audition/` | `frequent_cues_have_at_least_five_variants`, `survival_generator_sha256_matches_lf_normalised_sources`, `schema2_survival_records_have_seed_and_true_peak`, `credits_index_matches_provenance` | A6-04 |
| A6-06 | audio | 6.6: buses Master, Music, Effects, UI, Atmosphere; UI and Music routing; sliders | `ui_cues_follow_ui_volume_not_effects`, `stingers_follow_music_bus_and_warnings_do_not` | A6-05 |
| A6-07 | audio | 6.7: lane pan (balance law, mono-compatible), `mono_audio` | `pan_is_mono_compatible_for_every_lane_count_3_to_17`, `mono_audio_forces_centre` | A6-06 |
| A6-08 | audio | 6.8: selection budgets and the player-weapon lane | `player_shot_starts_even_with_two_critical_cues_in_frame` | A6-07 |
| A6-09 | audio | 6.9: caps and oldest-first stealing with a 40 ms fade | `steal_prefers_oldest_among_equal_priority_and_fades_40ms` | A6-08 |
| A6-10 | audio | 6.10: per-bank round-robin and presentation-counter jitter | `round_robin_never_repeats_in_1000_triggers`, `horde_audio_never_reads_core_rng` | A6-09 |
| A6-11 | audio | 6.11: ducking (Atmosphere and Music only) and density compensation | `boss_arrival_ducks_atmosphere_6db_releases_800ms`, `effects_bus_never_ducks` | A6-10 |
| A6-12 | audio | 6.12: exhaustive SimEvent mapping, enemy voices by catalog id, BossKind banks, class shots (NPCs use their own class), quality tone, Windup and Contact cues, visual twins | `sim_event_mapping_is_exhaustive`, `every_catalog_archetype_has_explicit_voice_row_with_existing_files`, `every_boss_kind_has_arrival_phase_death_banks...`, `warning_cue_table_has_a_visual_twin` | A6-11 |
| A6-13 | audio | 6.13: GunTracker (staged reloads, shell inserts, bolt, dry fire) | `manual_reload_plays_out_in_finish_once`, `dry_trigger_auto_reload_off_plays_dry_fire_once_per_350ms` | A6-12 |
| A6-14 | audio | 6.14: UI cues. Runner LootDrop and loot reveal; one CoinBatch per auto-sell or stash batch (R9); no hover sound | `fifty_auto_sold_items_one_coin_batch_on_results_none_on_rerender`, `sell_extras_of_50_plays_one_coin_batch`, `no_widget_hover_writes_a_sound_request` (with mutation proof) | A6-13 |
| A6-15 | audio | 6.15: MusicPlugin director, Music bus, stingers from existing banks, tracks Deferred, slider "Music & stingers" | `music_director_matrix`, `every_stinger_resolves_to_existing_bank`, `music_volume_scales_stingers_only` | A6-14 |
| A6-16 | audio | Settings: `SETTINGS_VERSION` 2 and the per-field default migration | `untouched_wave1_defaults_migrate_once_per_field`, `customised_levels_are_kept`, `version_2_file_never_remigrates` | A6-15 |
| A6-17 | audio | 6.17: calibrate and close the gate. 60 s mix at default sliders: -23 ±2 LUFS integrated, TP ≤ 0.95 FS. A runtime master limiter is added only if the measured TP exceeds 0.95 (R3). | `reference_mixed_60s_meets_loudness_gate` (both themes), `dense_battle_60s_true_peak_at_most_0_95` | A6-16 |
| A6-18 | audio | (optional) 6.18: review cue log and replay cross-check | `cue_log_round_trips_frame_actions` | A6-17 |
| A6-19 | save-determinism | Zero-move verification record | existing pins | A6-17 |

The listening pass is logged as pending.

---

## WAVE 7: goals 9-10 and docs

| Id | Owner | Title | Fail-first / gate |
|---|---|---|---|
| W7-01 | qa-captures | Build stamp (menu, crash report, startup log, `--version`) | `stamp_formats_dev_and_commit_builds`, `version_flag_selects_version_action`, `crash_report_names_build_stamp` |
| W7-02 | qa-captures | package_playtest and verify_playtest under 5.1, manifest v2 (version and commit), curated docs, `PENDING_W7` emptied | `playtest_guide_has_no_links_outside_the_package`; `verify_playtest` PASS; the packaged `--smoke` passes |
| W7-03 | audio | AUDIO_SURVIVAL.md, PLAYTEST_GUIDE listening pass (pending), ACCEPTANCE (-23 LUFS gate), CREDITS, packaging excludes `docs/audio` | check_views link check |
| W7-04 | loot-economy | Docs: PROGRESSION.md:23 (drop "no automatic sale"), GDD, ITEM_QUALITY, BALANCE columns, PLAYTEST_GUIDE auto-sell | `rg 'There is no automatic sale system' docs` is empty |
| W7-05 | progression-tree | Docs: PROGRESSION, SKILLS_AND_WEAPONS, BALANCE CLI | check_views |
| W7-06 | campaign-defenders | Docs: STRATEGY, JOURNEYS (takeover, picker, BeginDefense, SelectCountry), PROGRESSION allowance table, VALIDATION:291, PLAYTEST_GUIDE | check_views |
| W7-07 | ux-flow | Docs: UI_REDESIGN, VIEW_HIERARCHY (Upgrades, Home, modals, toasts, BattleLayout), JOURNEYS loop map, `docs/components`, PLAYTEST_GUIDE Escape table | check_views (VIEW_HIERARCHY lists the new views) |
| W7-08 | qa-captures | Docs inventory, PLAYTEST_GUIDE sections (Loop, Escape, Auto-sell, Takeover, Listening pass, Click-to-photon), ACCEPTANCE ACs, human-check rows | `playtest_guide_covers_the_production_loop`, `required_docs_sections_exist` |
| W7-09 | perf-architecture | Final acceptance at the release candidate: full matrix, Repeat 3, packaged smoke time, integrated GPU still pending or informational | measure_performance exits 0 |
| W7-10 | save-determinism | SIMULATION/VALIDATION/ARCHITECTURE v7 docs; final zero-move record | existing pins |
| W7-11 | qa-captures | Final full-loop capture review at both sizes, normal and reduced; findings table with owners; pairs in `docs/screenshots/production/` (≤ 40 MB) | `check_capture_findings.py --final` exits 0 only when every CF row is fixed or "approved by user" |
| W7-12 | orchestrator | The plan's Outcome section: branch, commits, change per finding, numbers, verify results, pending human checks; ask before fast-forwarding `main` | n/a |
