### Task 25: Live debug — BRP `fl.*` methods, agent tools, offscreen screenshot, perf panel (rev-1 T21)

**Track:** client · **Refs:** ADR-0008, ADR-0013 · **Lane 2** (after Tasks 22 and 27)

**Files:**
- Create: `crates/fl_client/src/debug/{remote,perf,screenshot,codecs}.rs`, `crates/fl_client/tests/remote.rs`, `crates/fl_game/tests/remote.rs`
- Modify: `crates/fl_client/src/debug/mod.rs`, `crates/fl_game/src/builder.rs`

**Interfaces:**
- Consumes: Task 27's probe findings (method registration, how to block for a runner reply, offscreen `Screenshot::image`, `register_agent_tool`); `RunnerLink`, `LocalDriver`, `PendingReplies`; `fl_observe::global_profiler`.
- Produces:
```rust
#[derive(Resource, Default)] pub struct ClientCommandCodecs { /* name → fn(serde_json::Value) -> Result<Vec<u8>, String> (postcard) */ }
pub trait CommandCodecAppExt { fn register_command_json<P: CommandPayload>(&mut self) -> &mut Self; }   // fl_client registers the core and debug payloads; feature clients theirs
#[derive(Resource)] pub struct ScenarioLoader(pub Box<dyn Fn(LoadRequest) -> Result<LoadedMatch, String> + Send + Sync>);   // inserted by fl_game
pub enum LoadRequest { Scenario(String), Save(Vec<u8>) }
pub struct LoadedMatch { pub link: RunnerLink, pub next: Tick, pub pending: Vec<PlayerCommand>, pub last_seq: BTreeMap<PlayerId, u32> }
pub struct RemoteDebugPlugin { pub port: u16 }   // cfg(feature = "debug_remote"); default 15702
pub struct OverlayRegistry;                      // name → toggle; "grid" (F3) in M0
```
- **Methods.** Params and results are JSON. Reads go through `SimQuery`; writes go through commands or the driver.

  | Method | Behaviour |
  |---|---|
  | `fl.sim_state` | `WorldSummary` + `{tick, last_hash, paused, speed}` |
  | `fl.entity {sim_id}` | `SimQuery::Entity` |
  | `fl.hash` | the last hash |
  | `fl.issue_command {player, units, command, payload}` | `ClientCommandCodecs` → `LocalDriver::issue`; returns `{exec_tick, seq}` |
  | `fl.pause`, `fl.resume` | driver controls |
  | `fl.step {n}` | steps; returns the new tick once reached |
  | `fl.set_speed {x}` | driver speed |
  | `fl.spawn {unit, player, x, y}` | issues `debug.spawn` |
  | `fl.journal_since {tick, limit}` | `SimQuery::JournalSince` |
  | `fl.perf` | profiler snapshot + `SimQuery::Perf` + frame stats |
  | `fl.screenshot {path}` | dedicated offscreen camera into an `Image`, captured with `Screenshot::image` and saved |
  | `fl.toggle_overlay {name}` | overlay toggle |
  | `fl.load_scenario {id}`, `fl.load {path}` | `ScenarioLoader`; swaps `RunnerLink` and restores `LocalDriver` |
  | `fl.save_scenario {path}` | writes the current snapshot's units as a `*.scenario.ron` that `fl_content` loads |
  | `fl.save {path}` | `SimQuery::Save { pending: driver.pending() }` |

  A method that needs a runner reply blocks for at most 2 s. Any `TickOutput`s that arrive meanwhile are queued for `drive_runner`, never dropped.
- Every `fl.*` method is published with a description through `bevy_brp_extras` as an agent tool.
- `world.*` stays client-only and is never wired to sim state.
- **Perf panel** (F2): a table of features, phases and systems (p50/p95/max) with a sparkline per row, FPS and frame-time graphs, an entity count and render stats.

- [ ] **Step 1: Write the failing tests:**
```rust
// crates/fl_client/tests/remote.rs  (#![cfg(feature = "debug_remote")]; MinimalPlugins + fake runner)
#[test] fn brp_issue_command_stamps_and_queues()   // JSON-RPC fl.issue_command {player:0, units:[1], command:"core.move", payload:{"target":[200,100]}} → {exec_tick, seq: 0}
#[test] fn brp_unknown_command_is_error()          // command "core.fly" → JSON-RPC error naming it
// crates/fl_game/tests/remote.rs  (#![cfg(feature = "debug_remote")]; GameBuilder::prepare_headless with remote on a free port; real threaded runner)
#[test] fn brp_step_state_command()                // fl.step {n:5} → tick ≥ 5; fl.sim_state tick ≥ 5; fl.issue_command core.move for unit 1;
                                                   //   fl.step {n:40}; fl.entity {sim_id:1} → position changed
#[test] fn gpu_screenshot_offscreen()              // full DefaultPlugins with the window hidden; fl.screenshot {path} → PNG exists, larger than 1 KB, not all black
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_client -p fl_game --features fl_game/debug_remote -E 'test(/brp_/)'`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** the step 2 command. Expected: PASS. Then run `cargo nextest run -p fl_game --features debug_remote -E 'test(/gpu_/)'` on the dev GPU. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track client`, then `cargo xtask gate nightly`. Expected: `PASS brp`, `PASS gpu` and `gate nightly: PASS`.
- [ ] **Step 6: Commit** `feat(fl_client): BRP fl.* methods via runner queries and commands, agent tools, offscreen screenshot, perf panel` with trailer `Refs: ADR-0008`.

---

