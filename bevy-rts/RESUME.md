# Resume prompt: Frontline (Rust + Bevy 0.20 RTS)

Paste the text below into a fresh Claude Code session started in `C:\Projects\bevy-experiments`.

---

You are resuming the build of **Frontline**, a deterministic, lockstep-ready, 3D isometric WWII RTS in Rust on Bevy 0.20. It is a clean-room take on the old Blitzkrieg games. Never copy code or data from Blitzkrieg or nival. Crates are prefixed `fl_`. You are the **controller**: you plan, dispatch subagents, review, bless and merge. You rarely write code yourself.

## The user's standing instruction (2026-10-25)
All earlier decisions are confirmed and approved. Drive **all milestones (M2-M9) through implementation**. When everything is done, **merge it into `main` locally** and send **one final report**. Do not stop except for these four things:
- a toolchain update
- the parent `C:\Projects\.gitignore`
- an asset download
- a real blocker

Rulings you make instead are recorded in the ledgers (`R<n>`), and the final report lists them for the user.

R147: keep assets generated in the repo (procedural meshes, synthesized CC0 audio). Ask the user before any external asset download.

## Where things stand
- **M0 foundation:** done, tag `m0`.
- **M1 control and movement:** done, tag `m1`. Terrain, pathfinding (A* and HPA*), flow fields, steering, orders and formations, selection, overlays, hot reload, the AI hook, the determinism suite.
- **M2 prelude** (Group C entry requirements plus M1 carry-overs): 6 of 21 tasks done on `main` (`c7088a6`).
  - Done: Tasks 1, 2, 3, 4, 5, 7.
  - To do: Tasks 6 and 8-21.
- **M2-M9:** not started. Roadmap: `docs/superpowers/plans/2026-10-23-frontline-m0-rev2.md`. Write each milestone's plan just before starting it (`superpowers:writing-plans`).
- `main` is clean; other branches and worktrees were removed. Tags `m0`, `m1` kept.

## Read first
1. `.superpowers/sdd/2026-10-25-frontline-m2-prelude/progress.md`: every ruling plus the `=== PAUSED` resume block at the end. Earlier ledgers: `.superpowers/sdd/2026-10-23-frontline-m0-rev2/progress.md`, `.superpowers/sdd/2026-10-24-frontline-m1/progress.md`. (`.superpowers/` is git-ignored but present in this checkout.)
   A committed snapshot of all four ledgers (no review diffs) is in `docs/ledgers/`. Use it if `.superpowers/` is missing, and refresh it at each pause or plan end.
2. `docs/superpowers/plans/2026-10-25-frontline-m2-prelude.md`: 21 tasks, topology, Global Constraints, M2 roadmap.
3. `docs/superpowers/specs/2026-10-23-frontline-rts-design.md` (revisions through 2.5) and `docs/adr/` (0001-0022, all accepted).
4. `CLAUDE.md`, `README.md`, `docs/debugging.md`, `docs/design/GDD.md`.

## How to work (the process that has been running)
- Skills: `superpowers:subagent-driven-development` (execution), `superpowers:writing-plans` (plans).
- Setup: make a lane-1 worktree, e.g. `git worktree add C:/fl/a -b m2p/phase-a main` (lane-2 worktrees under `C:\fl\`, one at a time). The Write/Edit tools are blocked under `C:\Projects\`: edit only in worktrees under `C:\fl\`, and fast-forward main with `git -C C:/Projects/bevy-experiments merge --ff-only <sha>`.
- Per task: extract the brief with the skill's `task-brief` script; dispatch a general-purpose implementer (R65, not the repo role agents); generate a review diff and dispatch a reviewer; up to 5 fix rounds via `SendMessage` to the same implementer, then a scoped re-review; bless `.new` artifacts yourself with `cargo xtask bless <paths> --reason "..."` and commit with the `Blessed:` trailer.
- At most 2 builders at once. Lane 2 builds with `CARGO_BUILD_BUILD_DIR=C:/t/fl/28/2710d09957a748`. Docs-only tasks may share a worktree.
- Gate: `cargo xtask gate task|pr|nightly|milestone`. `change-class` and `golden-policy` rows run at pr and milestone. Put all trailers (`Change-Class:`, `MCR:`, `Blessed:`, `Refs:`, `Co-Authored-By:`) in **one final block** of the commit message. Never pipe commit messages through Windows Python (it wrote CRLF and broke the checks).
- Protected artifacts (golden replays, snapshots, `docs/perf/budgets.ron`, `M*.json`, `docs/design/mechanics/**`, `*.proptest-regressions`, API listings): agents write `.new`, never bless; only you bless.
- Never `git stash`; use a WIP commit.
- Hard rules: fixed-point maths only in the sim; no float or `Time`; no HashMap iteration; SimId ordering; unit types never a closed enum; no `Local<T>` in sim systems; determinism tests never read the debug-only journal (R139); clamp or validate wire numbers before arithmetic (R104).
- Record every ruling in the ledger as `Ruling: R<n> <what> - <why> - <cost if wrong>`. Next free number: **R158**.

## Next steps
1. Create the lane-1 worktree from `main`; write `.gate-base` with the base commit.
2. Dispatch **Task 6**: journal opt-in for tests, one proptest policy helper and migration (incl. `fl_content/tests/tree.rs`), release-build determinism gate row; also apply R156 #4 (gate builds untimed before its timed test rows).
3. In parallel, lane 2: **Task 8** (provenance ledger; test fixture mods exempt, R151).
4. Follow the plan's topology: Phase II 9 -> 11 -> 13 alongside 10 -> 12 -> 14; Phase III 15 -> 17 alongside 16 -> 18 -> 19 -> 20; Phase IV Task 21. Fast-forward `main` after Tasks 6, 13, 17, 21.
5. Carry into Task 17's dispatch: new observability names `core.effect_dropped` (journal) and `effects.dropped` (counter).
6. Then M2 (direct-fire combat) through M9.

## List in the final report for the user
- Accepted ADRs, rulings and guessed values (the prelude's provenance ledger lists every guess).
- MCR-000 (replay format 4 re-bless) and every later golden re-bless (R146).
- The 6 mechanic-spec acceptance criteria still open (the test-author writes them in Task 11).
- Caveats: benchmarks ran on the Balanced Windows power plan; one GPU device-lost crash during the M1 demo; demo items needing a human (mouse feel, console typing, terrain readability).
- Per milestone: what was built, benchmarks, deferrals and guesses, how to run the testbed.

## Run the game
```bash
cargo run -p fl_game --release
```
