### Task 17: Integration, stress test, perf baseline, demo checklist

**Files:** Create `docs/perf/m0-baseline.md`, `docs/perf/baseline.ron`, `docs/perf/m0.json`, `docs/m0-demo-checklist.md` and `docs/debugging.md`; modify `README.md`.

- [ ] **Step 0:** Write `docs/debugging.md` for humans and agents (spec §15 C). It covers:
  - launching in debug mode (`--remote`, `debug.ron`, `RUST_LOG`, `trace_chrome` / `trace_tracy` features)
  - every `fl_cli` command with `--json` examples
  - every BRP method with example request and response JSON
  - log, journal and replay locations
  - the reproduce-a-bug recipe: seed + scenario + replay → `bisect-desync` → `inspect`

  Verify each documented command by running it.

- [ ] **Step 1:** With all branches merged into `main`, run the task gate. Expected: all green and `deps OK`.
- [ ] **Step 2:** `cargo run -p fl_cli --release -- data-check assets/base`. Expected: exit 0, 10 units, and the rules hash printed.
- [ ] **Step 3:** `cargo test -p fl_test_utils --release --test determinism`. Expected: PASS.
- [ ] **Step 4:** `cargo run -p fl_cli --release -- bench --units 500,3000,5000 --ticks 400 --warmup 40 --out docs/perf/m0-baseline.md --save-baseline docs/perf/baseline.ron --profile --json docs/perf/m0.json`. Expected: a table with three rows, and a 3000-unit mean of at most 15 ms (G7). Add machine, rustc and commit metadata.
- [ ] **Step 5:** `cargo run -p fl_game --release -- --stress 3000 --exit-after 25`. Expected: `stress: units=3000 avg_fps=…` with an average of at least 60; append that line to `docs/perf/m0-baseline.md`. If it is below 60, profile and fix (shared mesh handles, change-only sync) before closing M0.
- [ ] **Step 6:** `cargo run -p fl_game --release`, then walk through `docs/m0-demo-checklist.md`:
  - An iso view of the 1 km ground, with 10 placeholders in faction colours.
  - Camera: WASD, arrow and edge pan, wheel zoom, Q/E rotate, R snap, all clamped at the map edge.
  - Debug tools: console `spawn`, spawn-panel click-to-place, and `wander 0 100` showing smooth interpolated movement.
  - The stats panel shows the tick, FPS and hash.
- [ ] **Step 7:** Run a final whole-branch review (requesting-code-review), fix the findings, run the gate, and commit `docs: M0 perf baseline and demo checklist`.

---

