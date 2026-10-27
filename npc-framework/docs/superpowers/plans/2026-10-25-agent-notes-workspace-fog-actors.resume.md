# Resume: agent notes, workspace, fog, actors (updated 2026-10-27)

## Current continuation (2026-10-27)

**Latest: Tasks 21–23 implemented; Gate H passed.** Feature `76b153a`, main merge `dd645c3`.
`target/verify-phase-h-final.json`: `ok:true`, 1,217 passed/12 skipped/one slow/two reported leaky, doc 1/10
ignored, architecture 7/7, replays 8/8, six unchanged snapshots, fmt/Clippy/deny passed, no optional tools
skipped. A first-run stale MCP tool-list assertion was corrected with expanded field checks; full gate rerun.
Five-app release build passed on main in 3m20s (`target/release-phase-h.log`); Gate H is complete.
Map live/generated-MCP/native evidence:
`docs/_notes/phase-h-landmarks-evidence.md`.

Next F18, final review refresh and one fix wave: crafting capacity/output feedback, secret-bearing Debug,
carried/equipped item visuals and clock-reset optimistic stamp collisions. Existing BRP world.query,
world.write_message(WindowCloseRequested) and world.trigger_event(Activate) can drive actual F7 tabs and
close-dialog Yes without OS input; primary/type paths and JSON were source-verified, live F validation pending.
R22 hunger/thirst exhaustion needing external rescue is deliberate, not a new defect.

**Previous checkpoint: Tasks 15–17 and Gate G passed** (`target/verify-phase-g.json`,
`ok:true`, 1,174 tests passed/12 skipped/two reported leaky; doc 1/10 ignored, architecture 7/7, replays 8/8,
six snapshots none pending, fmt/Clippy/deny passed, no optional tools skipped). Feature `51d804c` was merged
--no-ff to main `7bc8a2c`; five-app release build passed on main in 3m02s (`target/release-phase-g.log`). Gate G
is complete. Evidence:
`docs/_notes/phase-g-world-presets-evidence.md` (live Survival/source-edit resume and inspected UI/night captures).
Next: H / 21–23, F / 18, final review/fix wave. H interfaces are agreed read-only. The final audit found a
crafting full-pack matching-stack overflow and missing crafted-output delta/drop feedback for the final wave.

Gate I is complete: feature `28cb8d4`, main merge `f78702b`, five-app release passed in 3m24s. Its evidence is
`docs/_notes/phase-i-registration-evidence.md` (1,144 tests; live generated-MCP rover handoff/token rotation).

The user asked to **drive the whole plan to the end**. Phases A–E are complete in the current monorepo,
`C:\Projects\bevy-experiments\npc-framework`. Tasks 13/14 were committed as `e3b8475` and merged --no-ff to
`main` as `65ea248`. Full Gate E (`target/verify-phase-e-serial.json`) reports **`"ok": true`**: 1,105 tests
passed, 12 skipped, one slow and three reported leaky; doc 1 passed/10 ignored; architecture 7/7, replays 8/8,
six snapshots none pending, fmt/Clippy/deny passed, no optional tools skipped. The complete serial suite passed
two detached-host tests that failed under concurrent load; no assertions or baselines were changed.

The five-app release build passed on main, 2026-10-27. Artifacts are under `target/release/`:
`game_menu.exe`, `actor_testbed.exe`, `npcctl.exe`, `village.exe`, `rts_units_demo.exe`.
Evidence: `docs/_notes/task13-robot-explorer-evidence.md` and `task14-actor-editor-evidence.md`.
The LIDAR decision is resolved: optimized complete scans; rover median p95 37.2 µs perception /49.4 µs fog.

**Order after Gate E: Phase I / Tasks 30–31 (now complete), G / 15–17 (worlds), H / 21–23 (map), F / 18 (docs,
MCP end-to-end, final verify), final whole-change review and one fix wave.** Preserve the phase gates: full
verification, local main checkpoint and five release apps before starting the next phase. Use the current
checkout layout and `codex/` branches, preserving unrelated changes; no push/PR was requested.

The older handoff below records the previous checkout and workflow. Its Task 13 starting point is historical.

## Historical resume prompt (2026-10-26)

> Resume the plan `docs/superpowers/plans/2026-10-25-agent-notes-workspace-fog-actors.md` with
> superpowers:subagent-driven-development. Read
> `docs/superpowers/plans/2026-10-25-agent-notes-workspace-fog-actors.resume.md` first, then the
> progress file `docs/superpowers/plans/2026-10-25-agent-notes-workspace-fog-actors.progress.md`
> (the ledger: every task, review, fix round and ruling so far). Work in a fresh worktree branched
> from `main`; copy the progress file into the new SDD workspace as `progress.md` so tasks marked
> complete are not re-run. Next task: Task 13. Keep the dev's rules: every phase ends with a full
> `cargo xtask verify`, a `--no-ff` merge into `main` in the main checkout (`C:\Projects`, index clean,
> never touch the dev's uncommitted files) and a release build of the five apps in
> `C:\Projects\npc-framework`; the progress file is updated on `main` only. Ask me first about the open
> decision below.

## Where it stands

- On `main` (merge 95ff601 of the branch `claude/single-npc-testbed-48eb8e` at 775ea1b): Phases A–D
  complete and gated; Phase E partly done — **Task 11** (actor presets), **Task 29** (quitting the game
  stops every process) and **Task 12** (LIDAR, camera, radar with ROS-shaped readings) are done and
  reviewed. Gate E has **not** run (Phase E is unfinished); this merge is a pause, verified with a full
  `cargo xtask verify`.
- Remaining, in execution order:
  1. **Task 13** (E3) robots in the view, in fog (LIDAR/camera/radar reveal fog), in the workspace and
     the menu; **Task 14** (E4) the actor editor (incl. the reach vicinity field) → **Gate E**.
  2. **Phase I** (agent registration): **Task 30** (I1) the id/token lock in the agent interface,
     **Task 31** (I2) the explorer as a registered world + dungeon-master briefing → **Gate I**.
     Spec: `docs/superpowers/specs/2026-10-26-agent-registration-design.md`.
  3. **Phase G** (world presets): Tasks 15–17 → Gate G.
  4. **Phase H** (map/landmarks, session-bound tools): Tasks 21–23 → Gate H.
  5. **Phase F**: Task 18 (docs, end to end, full verify) → Gate F, then the final whole-branch
     review, one fix wave, and the "Rulings I made" report to the dev.

## Historical decision (resolved 2026-10-27)

The user selected optimizing traversal while preserving complete scans. The following was the decision at
the previous pause; do not ask it again.

A walking **rover** costs 54.6–57 µs perception p95 against the 55 µs budget (idle 15 µs after the
LIDAR cache). Options: (a) a rotating LIDAR that scans 1/stagger of its beams per tick, as a real
spinning scanner does (≈4× lower walking p95); (b) skip empty blocks in the beam walk; (c) accept ≈55
µs while a rover moves. No budget was raised.

## Things to know

- Specs: `2026-10-25-agent-notes-and-workspace-design.md` (main), `…-actor-map-and-memory-design.md`,
  `…-notes-as-obsidian-files-design.md`, `…-agent-feedback-increment-design.md`,
  `2026-10-26-agent-registration-design.md`.
- Rulings R1–R17 and every parked Minor are in the progress file; the final review should sweep the
  parked Minors.
- `deny.toml` ignores RUSTSEC-2026-0391 (zstd 0.13.3), -0398 and -0403 (zstd-safe 7.3.0), published
  2026-10-26, by the dev's decision: they come through Bevy 0.20's `bevy_image` (`zstd_c`), the fixes
  need zstd 0.14, and the flawed paths are unreachable. Drop them when Bevy moves to zstd 0.14.
- Flaky under load: `xtask::hooks stop_hook_loop_guard` (failed once at 65 s, passes alone in 6 s) and `npcctl::session_host a_long_step_does_not_block_other_requests` (wall clock) —
  rerun alone. Builds can fail with corrupt metadata when the disk is near full; rerun with
  `CARGO_BUILD_JOBS=8`.
- Disk: build caches grow fast (a worktree's `target` reaches 100+ GB). `cargo clean` finished
  worktrees; on `main`, `cargo clean --profile dev` keeps the release apps.
- `npcctl mcp` servers belong to AI-tool sessions (Claude Code in VS Code); they can lock
  `target/release/npcctl.exe` during a release build — rename the locked exe aside and rebuild rather
  than killing them.
- Subagents cannot write report files (the tool blocks it): the controller writes
  `task-N-report.md` from the hand-back.
