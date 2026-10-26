### Task 27: Probe — `bevy_remote` custom methods, a runner-thread round trip, offscreen screenshot

**Track:** probes · **Refs:** ADR-0008, ADR-0013 · **Lane 2** (after Task 4)

**Files:**
- Modify: `probes/remote/src/main.rs`; the section "Probe: bevy_remote" in `docs/research/bevy020-api-reference.md`

**Interfaces:**
- Produces findings that Task 25 consumes:
  - the 0.20 names for adding the remote and HTTP plugins on a chosen port and for registering a custom method (instant vs watching)
  - whether a method may block up to 2 s waiting on a crossbeam reply from another thread without stalling rendering
  - the `bevy_brp_extras` `register_agent_tool` signature and the `brp_extras/*` listing method
  - the offscreen camera → `Image` → `Screenshot::image(handle)` → `save_to_disk` recipe that works with the window hidden or unfocused
  - how to set continuous update mode when the window is unfocused

- [ ] **Step 1: Write the probe.**
  - App: `DefaultPlugins` with the window hidden, plus the remote and HTTP plugins on port 15799.
  - A worker thread stands in for the runner: it echoes `{n}` → `{n + 1}` over crossbeam.
  - A custom method `fl.ping {n}` that blocks on the worker's reply, and one agent tool registered for it.
  - An offscreen camera rendering a lit cube into an `Image`.
  - A self-test thread posts JSON-RPC over `std::net::TcpStream`: `fl.ping {n: 41}` must return `42`; `rpc.discover` must list `fl.ping`; the agent-tool listing must list `fl.ping`. It then requests a screenshot of the offscreen image to `target/probe-remote.png`.
  - On success it prints `remote probe: OK ping=42 screenshot=<bytes>` and exits 0.
- [ ] **Step 2: Run** `cargo run -p probe_remote` (dev GPU) twice: once with the window focused, once with it behind another window. Expected: the OK line both times, and a non-black PNG of more than 1 KB.
- [ ] **Step 3: Record** every finding above, with the API names exactly as compiled, in the API reference.
- [ ] **Step 4: Gate** `cargo xtask gate task --track probes`. Expected: PASS.
- [ ] **Step 5: Commit** `chore(probes): bevy_remote custom methods, runner round trip, offscreen screenshot`.

