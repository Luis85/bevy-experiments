### Task 30: Integration — demo, determinism, perf baseline, render stress, debugging guide (rev-1 T17)

**Track:** docs + perf · **Refs:** ADR-0009, ADR-0010

**Files:**
- Create: `docs/debugging.md`, `docs/m0-demo-checklist.md`, `docs/perf/M0.md`, `docs/perf/M0.json`
- Modify: `README.md`

- [ ] **Step 1: Merge and gate.** With every branch merged into `m0/phase-a`, run `cargo xtask gate pr`. Expected: `gate pr: PASS`.
- [ ] **Step 2: Write `docs/debugging.md`** for humans and agents. It covers:
  - launching in debug mode: `--remote`, `--remote-port`, `--agent`, `debug.ron`, `RUST_LOG`, and the `trace`/`trace_chrome`/`trace_tracy` features
  - picking a BRP port per agent
  - every `fl_cli` command with a `--json` example
  - every `fl.*` method with example request and response JSON
  - where logs, journals, replays and saves are written
  - the reproduce-a-bug recipe: seed + scenario → `run-scenario --trace` → `replay-verify` → `bisect-desync` → `inspect --entity` → `journal --sim-id`
  - the dynamic-linking result (Task 26)

  Run every documented command and check its output matches the document.
- [ ] **Step 3: Content and determinism.**
  - `cargo run -p fl_cli --release -- data-check assets/base` → exit 0, 10 units, rules hash printed.
  - `FL_SHUFFLE_SEEDS=3 cargo nextest run --profile agent -p fl_sim -p fl_test_utils` → PASS.
  - `cargo run -p fl_cli -- replay-verify crates/fl_test_utils/tests/replays/testbed_wander.replay` → exit 0.
- [ ] **Step 4: Perf baseline (tier 3).** Ask the user to confirm the fixed power plan first (a system setting). Run `cargo run -p fl_cli --release -- bench --units 500,3000,5000 --ticks 400 --warmup 40 --repeat 5 --out docs/perf/M0.md --json docs/perf/M0.json`.
  - Expected: three rows, with entity counts of about 1.7k, 10k and 17k.
  - Expected: a 3000-unit mean of **≤ 15 ms** (G7, entity mix).
  - Add the machine, rustc version, commit and power plan to the files.
  - If the 3000-unit mean is over 15 ms, profile with `--features fl_game/trace_chrome` (or the `perf-analyst` agent on the Tracy CSV) and fix before closing M0.
- [ ] **Step 5: Render stress test.** Run `cargo run -p fl_game --release -- --stress 3000 --exit-after 25 --agent`.
  - Expected: `stress: units=3000 avg_fps=…` with an average of at least 60. Append the line to `docs/perf/M0.md`.
  - If it is below 60, check shared mesh and material handles, change-only snapshot records, and that only on-screen visuals update their transforms. Fix before closing M0.
- [ ] **Step 6: BRP smoke through `bevy_brp_mcp`.** Launch `cargo run -p fl_game --features debug_remote -- --remote --remote-port 15702 --agent`. Through the MCP, poll `fl.sim_state` until tick ≥ 1, then call `fl.spawn`, `fl.step {n: 20}`, `fl.entity` and `fl.screenshot`. Record the results in the checklist.
- [ ] **Step 7: Demo checklist.** Write `docs/m0-demo-checklist.md` and walk it with `cargo run -p fl_game --release`:
  - Rev-1 Task 17 Step 6's items: the iso view, 10 placeholders, camera controls clamped at the edge, console `spawn`, click-to-place, `wander 0 100` with smooth interpolation, and the stats panel.
  - The `setowner`/`invuln`/`sethp`/`teleport` console cheats work, and are rejected in a match started with `cheats_allowed = false`.
  - Orders given while paused apply on resume; speed ×0.25–×8; single step.
  - `reveal` toggles; `player 1` switches the controlled player.
  - The `inspect` window shows components.
  - The F2 perf panel lists phases and features.
  - `fl.save` then `fl.load` resumes at the same tick and hash.
  - **ADR confirmations:** one row per Confirmation bullet of ADR-0001…0013, naming the passing test or gate step, or `deferred: <reason>` (D1 cross-OS, D2 instruction counts).
- [ ] **Step 8: Milestone gate.** Run `cargo xtask gate milestone --milestone M0`. Expected: `gate milestone: PASS`.
- [ ] **Step 9: Review and commit.** Run the final whole-branch review (`superpowers:requesting-code-review`), fix the findings, and re-run `cargo xtask gate pr`. Commit `docs: M0 debugging guide, demo checklist and perf baseline`. The controller fast-forwards `main`.

---

