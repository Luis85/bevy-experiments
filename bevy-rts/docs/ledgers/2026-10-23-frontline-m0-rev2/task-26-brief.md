### Task 26: Probe — `bevy/dynamic_linking` with `bevy_ecs`-only sim crates

**Track:** probes · **Refs:** ADR-0013 (§13.5) · **Lane 2** (`C:\fl\t26`, cut after Task 1)

**Files:**
- Modify: `probes/dynlink/src/main.rs`; the section "Probe: dynamic_linking" in `docs/research/bevy020-api-reference.md`
- Modify `crates/fl_game/Cargo.toml` only on success: add the feature `dynamic = ["bevy/dynamic_linking"]`

**Interfaces:**
- Consumes: the `fl_sim` API as it stands on the probe's base commit (rev-1 `build_sim_app` plus `run_schedule(SimTick)` is fine; the probe is throwaway).
- Produces: a recorded decision on whether dev builds may use dynamic linking.

- [ ] **Step 1: Write the probe.** `main` builds a sim `App` from the `core_mod` fixture (by path) and steps it 10 ticks. It also builds a `bevy` `App` with `MinimalPlugins` and one system, runs it 10 updates, and prints `dynlink probe: OK sim_ticks=10 app_updates=10`.
- [ ] **Step 2: Run** `cargo run -p probe_dynlink` (static). Expected: the OK line.
- [ ] **Step 3: Run** `cargo run -p probe_dynlink --features dynamic`. Expected: either the OK line, or a duplicate-crate, link or type-mismatch error. Record the exact error text.
- [ ] **Step 4: Record** the result, the commands and the decision in the API reference:
  - On success: dev runs may use `cargo run -p fl_game --features dynamic`. Add the feature line.
  - On failure: dynamic linking is not used. Add nothing to `fl_game`.

  The controller adds one line about it to `CLAUDE.md` when it merges.
- [ ] **Step 5: Gate** `cargo xtask gate task --track probes`. Use the interim gate if Task 2 has not merged.
- [ ] **Step 6: Commit** `chore(probes): dynamic_linking with bevy_ecs-only sim crates — <result>`.

