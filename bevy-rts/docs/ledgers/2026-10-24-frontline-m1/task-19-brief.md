### Task 19: Hot reload, game side — watcher, both reload paths, visuals, replay capture

**Track:** client + tools · **Refs:** ADR-0012 (refinement) · **Lane 2** (after Task 12)

**Files:**
- Create: `crates/fl_game/src/{reload,replay_capture}.rs`, `crates/fl_game/tests/reload.rs`
- Modify:
  - `crates/fl_game/src/{builder,lib}.rs`
  - `crates/fl_client/src/{placeholder,link,view}.rs` (`view.rs`: `drive_runner` appends to `InputLog`), `crates/fl_client/src/debug/{mod,console}.rs` (`LoadRequest::Rebuild`)
  - `crates/fl_observe/src/config.rs` (and its `repo_debug_ron_parses` test), `debug.ron` (`hot_reload: true`, `hot_reload_poll_ms: 500`; the log filter stays `info,fl_sim=debug,fl_sim::spawn=info`)

**Interfaces:**
- Consumes: `apply_content_values` through `RunnerControl::ReloadContent`, `load_world_with`, `LoadOptions`, `ContentDb::{same_id_sets, id_set_diff}` (Task 12); `load_match`, `ScenarioLoader`; `ReplayRecorder`, `Replay`, `ReplayEvent::ContentReload`.
- Produces:
```rust
// fl_client
pub enum LoadRequest { Scenario(String), Save(Vec<u8>), Rebuild { save: Vec<u8>, content: Arc<ContentDb> } }   // Rebuild loads with drop_unknown_units
#[derive(Resource, Default)] pub struct InputLog { pub inputs: Vec<TickInput>, pub hashes: Vec<(Tick, u64)> }   // drive_runner appends when present
// placeholder.rs: PlaceholderAssets rebuild when ClientContent changes; existing visuals get the new parts
// fl_game/src/reload.rs
#[derive(Resource)] pub struct ContentWatcher { .. }
impl ContentWatcher { pub fn new(dirs: Vec<PathBuf>, every: Duration) -> Self; pub fn scan(&mut self) -> bool; }   // (mtime, len) of every file under the dirs, sorted
    // mtime kept as a Duration since UNIX_EPOCH: the client clippy profile bans naming the std::time::SystemTime type
#[derive(Debug, Clone, PartialEq)] pub enum ReloadOutcome { Values { tick: Tick, rules_hash: u64 }, Rebuilt { tick: Tick, dropped: Vec<(SimId, String)> },
                                                            VisualsOnly, Failed(String), Refused(String) }
pub fn reload_content(world: &mut World) -> ReloadOutcome;
    // load_content(features, mods): Err → Failed(errors) and the match keeps running; cheats off → Refused; same ids and same rules hash → VisualsOnly
    // (ClientContent swapped); same ids → Control ReloadContent, blocking on the reply like load_match → Values; ids differ → SimQuery::Save { pending }
    // → LoadRequest::Rebuild → Rebuilt. A SimReply::Error to the save (SaveError, for example spawn requests still waiting) → Failed(text), and the
    // match keeps running. Each outcome is logged and printed to the console; Values and Rebuilt push ReplayEvent::ContentReload
// automatic in dev builds when debug.ron hot_reload is true: the watcher scans every hot_reload_poll_ms; console "reload" forces one
// fl_game/src/replay_capture.rs
#[derive(Resource)] pub struct ReplayCapture { .. }   // header + ReplayStart of the running match, InputLog, events; reset by load_match
// console "replay save <path>" writes Replay::to_bytes (a replay with a reload is not reproducible: replay-verify reports "non-reproducible")
```

- [ ] **Step 1: Write the failing tests** (`crates/fl_game/tests/reload.rs`; `prepare_headless` on a temp copy of `assets/base`):
```rust
#[test] fn value_edit_reloads_in_place()             // edit pz3 hp 110 → 220 → Values; SimQuery::Entity(#2) shows max 220 and hp at the same ratio
#[test] fn adding_a_unit_rebuilds_with_same_sim_ids() // add a unit file (+ visual) → Rebuilt; snapshot sim ids identical; the tick unchanged
#[test] fn removing_a_unit_type_drops_its_units()    // remove tiger from units, visuals, roster and scenario → Rebuilt { dropped: [(#3, "base.de.tiger")] }
#[test] fn malformed_ron_keeps_match_running()       // a truncated pz3.unit.ron → Failed containing "pz3.unit.ron"; ticks keep advancing
#[test] fn reload_refused_without_cheats()           // match with cheats_allowed false → Refused
#[test] fn reload_while_paused_applies()             // paused: Values; still paused; tick unchanged
#[test] fn visual_only_change_swaps_client_content() // edit a visual's scale → VisualsOnly; PlaceholderAssets rebuilt
#[test] fn reload_records_replay_event()             // replay save → Replay.events == [ContentReload { tick, rules_hash }]; !reproducible()
#[test] fn watcher_detects_change()                  // scan() false; rewrite a file with new length → true; then false
#[test] fn failed_rebuild_keeps_match_running()      // debug.set_terrain paints base.swamp; remove swamp.terrain.ron and its legend entry → Failed containing
                                                     //   "core.terrain"; the match keeps its state and ticks keep advancing
```
- [ ] **Step 2: Run** `cargo nextest run --profile agent -p fl_game --test reload`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run** `cargo nextest run --profile agent -p fl_game -p fl_client -p fl_observe`. Expected: PASS.
- [ ] **Step 5: Gate** `cargo xtask gate task --track client --track tools`. Expected: PASS.
- [ ] **Step 6: Commit** `feat(fl_game): hot reload — file watcher, in-place value reload, ID-set rebuild via save/load, visuals, replay capture` with trailer `Refs: ADR-0012`.

