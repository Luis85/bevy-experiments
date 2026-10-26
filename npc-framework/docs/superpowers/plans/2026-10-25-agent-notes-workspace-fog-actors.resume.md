# Resume: agent notes, workspace, fog, actors (paused 2026-10-26)

## Paste this into a new Claude Code session started in `npc-framework/`

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

## Open decision for the dev (ask before Task 13)

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
